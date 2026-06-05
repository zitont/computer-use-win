mod apps;
mod input;
mod mcp;
mod screenshot;
mod uia;

use mcp::{ToolDef, JsonRpcResponse};
use serde_json::{json, Value};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use windows::core::BOOL;

static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// 写入调试日志到文件
pub(crate) fn log_debug(msg: &str) {
    let _ = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("D:\\project\\demo\\omkz\\server.log")
        .and_then(|mut f| {
            use std::io::Write;
            writeln!(f, "{}", msg)
        });
}

fn main() -> io::Result<()> {
    log_debug("[SERVER] 启动中...");
    screenshot::init_dpi_awareness();
    log_debug("[SERVER] DPI 感知初始化完成");

    ctrlc_handler();
    log_debug("[SERVER] 信号处理器注册完成");

    let tools = register_tools();
    log_debug(&format!("[SERVER] 工具注册完成,共 {} 个工具", tools.len()));
    log_debug("[SERVER] 等待 stdin 消息...");

    loop {
        if SHUTDOWN_REQUESTED.load(Ordering::Relaxed) {
            log_debug("[SERVER] 收到关闭信号");
            break;
        }
        match mcp::read_message() {
            Ok(message) => {
                log_debug(&format!("[SERVER] 收到消息: {} 字节", message.len()));
                let request: Result<mcp::JsonRpcRequest, _> = serde_json::from_str(&message);
                match request {
                    Ok(req) => {
                        log_debug(&format!("[SERVER] 解析请求: method={}", req.method));
                        if let Some(response) = handle_request(req, &tools) {
                            let response_json = serde_json::to_string(&response).unwrap();
                            log_debug(&format!("[SERVER] 发送响应: {} 字节", response_json.len()));
                            if let Err(e) = mcp::write_message(&response_json) {
                                log_debug(&format!("[SERVER] 写入响应失败: {}", e));
                            }
                        } else {
                            log_debug("[SERVER] 通知消息,不返回响应");
                        }
                    }
                    Err(e) => {
                        log_debug(&format!("[SERVER] 解析请求失败: {}", e));
                        let err_resp = JsonRpcResponse::error(
                            None,
                            -32700,
                            format!("解析请求失败: {}", e),
                        );
                        let response_json = serde_json::to_string(&err_resp).unwrap();
                        let _ = mcp::write_message(&response_json);
                    }
                }
            }
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    log_debug("[SERVER] EOF,退出");
                    break;
                }
                log_debug(&format!("[SERVER] 读取消息失败: {}", e));
                break;
            }
        }
    }

    log_debug("[SERVER] 正常退出");
    Ok(())
}

/// 注册 Ctrl+C / Ctrl+Break 信号处理,设置退出标志
fn ctrlc_handler() {
    unsafe {
        windows::Win32::System::Console::SetConsoleCtrlHandler(
            Some(console_ctrl_handler),
            true,
        ).ok();
    }
}

unsafe extern "system" fn console_ctrl_handler(ctrl_type: u32) -> BOOL {
    match ctrl_type {
        // CTRL_C_EVENT | CTRL_BREAK_EVENT | CTRL_CLOSE_EVENT
        0 | 1 | 2 => {
            SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
            BOOL(1) // 已处理
        }
        _ => BOOL(0),
    }
}

fn handle_request(req: mcp::JsonRpcRequest, tools: &[ToolDef]) -> Option<JsonRpcResponse> {
    // 通知消息(notification)没有 id 字段,按照 JSON-RPC 2.0 规范不应返回响应
    match req.method.as_str() {
        "initialize" => {
            // 始终返回服务器实际支持的协议版本 "2024-11-05"
            // 即使客户端请求更高版本,也诚实声明以让客户端降级
            Some(JsonRpcResponse::success(req.id, mcp::initialize_result(None)))
        }
        "notifications/initialized" | "notifications/cancelled" | "notifications/progress" => {
            // 所有通知消息不返回响应 (JSON-RPC 2.0 规范)
            None
        }
        "tools/list" => {
            Some(JsonRpcResponse::success(req.id, mcp::tools_list(tools)))
        }
        "tools/call" => {
            let params = req.params.unwrap_or(json!({}));
            let name = params.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

            let result = call_tool(name, arguments);
            match result {
                Ok(value) => Some(JsonRpcResponse::success(req.id, value)),
                Err(e) => Some(JsonRpcResponse::error(req.id, -32000, format!("工具执行失败: {}", e))),
            }
        }
        "ping" => Some(JsonRpcResponse::success(req.id, json!({}))),
        _ => Some(JsonRpcResponse::error(req.id, -32601, format!("未知方法: {}", req.method))),
    }
}

fn call_tool(name: &str, args: Value) -> Result<Value, String> {
    let raw = match name {
        "get_window_state" => tool_get_window_state(&args),
        "get_ui_tree" => tool_get_ui_tree(&args),
        "click" => tool_click(&args),
        "scroll" => tool_scroll(&args),
        "drag" => tool_drag(&args),
        "type_text" => tool_type_text(&args),
        "press_key" => tool_press_key(&args),
        "launch_app" => tool_launch_app(&args),
        "list_installed_apps" => tool_list_installed_apps(&args),
        "shutdown" => tool_shutdown(),
        _ => Err(format!("未知工具: {}", name)),
    }?;

    // 将响应包装为 MCP content 块格式
    // 如果包含 image 字段,拆分为 image 内容块 + text 内容块
    let mut content: Vec<Value> = Vec::new();

    if let Some(image_data) = raw.get("image").and_then(|v| v.as_str()) {
        // 提取 image 字段中的 base64 数据 (去掉 data:image/png;base64, 前缀)
        let b64 = image_data
            .split(',')
            .nth(1)
            .unwrap_or(image_data);
        content.push(json!({
            "type": "image",
            "data": b64,
            "mimeType": "image/jpeg"
        }));
    }

    // 其余字段序列化为 text 内容块
    let mut text_obj = raw.clone();
    if text_obj.get("image").is_some() {
        text_obj.as_object_mut().map(|o| o.remove("image"));
    }
    let text_content = serde_json::to_string(&text_obj).unwrap_or_default();
    if !text_content.is_empty() && text_content != "null" {
        content.push(json!({
            "type": "text",
            "text": text_content
        }));
    }

    Ok(json!({ "content": content }))
}

/// 捕获全屏截图 + UIA 元素树,返回 JSON 响应
fn capture_state(max_depth: i32) -> Result<Value, String> {
    let mut tree = uia::UiaTree::new();
    let hwnd = tree
        .capture_foreground(max_depth)
        .map_err(|e| format!("UIA 遍历失败: {}", e))?;

    let (screenshot_b64, width, height) =
        screenshot::capture_screen().map_err(|e| format!("截图失败: {}", e))?;

    let window_title = apps::get_window_title(hwnd);
    let process_name = apps::get_window_process_name(hwnd);

    let cursor_pos = unsafe {
        let mut point = windows::Win32::Foundation::POINT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point);
        point
    };

    Ok(json!({
        "image": format!("data:image/jpeg;base64,{}", screenshot_b64),
        "image_width": width,
        "image_height": height,
        "window_title": window_title,
        "process_name": process_name,
        "cursor": format!("{},{}", cursor_pos.x, cursor_pos.y),
        "elements": uia::elements_to_json(&tree.elements)
    }))
}

/// 操作工具执行后,等待 UI 稳定并返回最新窗口状态
/// include_screenshot=false 时仅返回 UIA 树,响应体积从 ~900KB 降至 ~10KB
fn capture_after_action(delay_ms: u64, include_screenshot: bool) -> Result<Value, String> {
    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    // 操作后只需浅层 UIA 树即可定位元素,无需 depth=10 的全量遍历
    let depth = if include_screenshot { 8 } else { 5 };
    if include_screenshot {
        capture_state(depth)
    } else {
        capture_state_lightweight(depth)
    }
}

/// 轻量级状态捕获: 仅返回 UIA 元素树,不截取屏幕截图
/// 用于 AI 在连续操作间快速确认 UI 状态,响应体积约 10-50KB
fn capture_state_lightweight(max_depth: i32) -> Result<Value, String> {
    let mut tree = uia::UiaTree::new();
    let hwnd = tree
        .capture_foreground(max_depth)
        .map_err(|e| format!("UIA 遍历失败: {}", e))?;

    let window_title = apps::get_window_title(hwnd);
    let process_name = apps::get_window_process_name(hwnd);

    let cursor_pos = unsafe {
        let mut point = windows::Win32::Foundation::POINT::default();
        let _ = windows::Win32::UI::WindowsAndMessaging::GetCursorPos(&mut point);
        point
    };

    Ok(json!({
        "window_title": window_title,
        "process_name": process_name,
        "cursor": format!("{},{}", cursor_pos.x, cursor_pos.y),
        "elements": uia::elements_to_json(&tree.elements)
    }))
}

fn tool_get_window_state(args: &Value) -> Result<Value, String> {
    let max_depth = args
        .get("max_depth")
        .and_then(|v| v.as_i64())
        .unwrap_or(8) as i32;
    capture_state(max_depth)
}

fn tool_click(args: &Value) -> Result<Value, String> {
    let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let click_count = args.get("click_count").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let ds = screenshot::get_downscale();
    let include_screenshot = args.get("include_screenshot").and_then(|v| v.as_bool()).unwrap_or(true);
    // 观察深度: 必须与 get_ui_tree/get_window_state 使用的 max_depth 一致
    // 否则 element_index 会映射到不同的元素
    let observe_depth = args.get("observe_depth").and_then(|v| v.as_i64()).unwrap_or(5) as i32;

    // 批量模式: element_indices 数组
    if let Some(indices) = args.get("element_indices").and_then(|v| v.as_array()) {
        let tree = capture_fresh_tree_with_depth(observe_depth)?;
        for val in indices {
            let idx = val.as_i64().ok_or("element_indices 元素必须为整数")?;
            let (px, py) = resolve_element_point(&tree, idx as i32)?;
            input::click(px, py, button, click_count)
                .map_err(|e| format!("点击失败: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return capture_after_action(200, include_screenshot);
    }

    // 批量模式: points 数组 [{x, y}, ...]
    if let Some(points) = args.get("points").and_then(|v| v.as_array()) {
        for point in points {
            let px = point.get("x").and_then(|v| v.as_i64()).ok_or("points 元素缺少 x")? as i32 * ds;
            let py = point.get("y").and_then(|v| v.as_i64()).ok_or("points 元素缺少 y")? as i32 * ds;
            input::click(px, py, button, click_count)
                .map_err(|e| format!("点击失败: {}", e))?;
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        return capture_after_action(200, include_screenshot);
    }

    // 单次点击模式
    let x = args.get("x").and_then(|v| v.as_i64());
    let y = args.get("y").and_then(|v| v.as_i64());
    let element_index = args.get("element_index").and_then(|v| v.as_i64());

    let (final_x, final_y) = if let Some(idx) = element_index {
        let tree = capture_fresh_tree_with_depth(observe_depth)?;
        resolve_element_point(&tree, idx as i32)?
    } else if let (Some(x), Some(y)) = (x, y) {
        (x as i32 * ds, y as i32 * ds)
    } else {
        return Err("需要提供 x/y 坐标、element_index、element_indices 或 points".to_string());
    };

    input::click(final_x, final_y, button, click_count)
        .map_err(|e| format!("点击失败: {}", e))?;

    capture_after_action(200, include_screenshot)
}

fn tool_scroll(args: &Value) -> Result<Value, String> {
    let delta_x = args.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let delta_y = args.get("delta_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let element_index = args.get("element_index").and_then(|v| v.as_i64());
    let ds = screenshot::get_downscale();
    let include_screenshot = args.get("include_screenshot").and_then(|v| v.as_bool()).unwrap_or(true);
    let observe_depth = args.get("observe_depth").and_then(|v| v.as_i64()).unwrap_or(5) as i32;

    let (final_x, final_y) = if let Some(idx) = element_index {
        let tree = capture_fresh_tree_with_depth(observe_depth)?;
        resolve_element_point(&tree, idx as i32)?
    } else {
        (
            args.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds,
            args.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds,
        )
    };

    input::scroll(final_x, final_y, delta_x, delta_y)
        .map_err(|e| format!("滚动失败: {}", e))?;

    capture_after_action(300, include_screenshot)
}

fn tool_drag(args: &Value) -> Result<Value, String> {
    let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let ds = screenshot::get_downscale();
    let include_screenshot = args.get("include_screenshot").and_then(|v| v.as_bool()).unwrap_or(true);
    let observe_depth = args.get("observe_depth").and_then(|v| v.as_i64()).unwrap_or(5) as i32;

    let (start_x, start_y) = resolve_position(args, "start", ds, observe_depth)?;
    let (end_x, end_y) = resolve_position(args, "end", ds, observe_depth)?;

    input::drag(start_x, start_y, end_x, end_y, button)
        .map_err(|e| format!("拖拽失败: {}", e))?;

    capture_after_action(300, include_screenshot)
}

/// 重新遍历 UIA 树获取最新的前台窗口元素列表
/// 操作工具需要在操作时刻获取最新的元素位置,
/// 而不是复用上次 get_window_state 的快照 (UI 可能在两次调用间变化)
/// depth 参数应与观察时使用的 max_depth 一致,确保 element_index 稳定对应
fn capture_fresh_tree_with_depth(depth: i32) -> Result<uia::UiaTree, String> {
    let mut tree = uia::UiaTree::new();
    let hwnd = tree.capture_foreground(depth).map_err(|e| format!("UIA 遍历失败: {}", e))?;
    log_debug(&format!("[FRESH] 前台窗口 HWND={:?}, 元素数={}, depth={}", hwnd.0, tree.elements.len(), depth));
    Ok(tree)
}

/// 默认使用 depth=5 与 get_ui_tree 默认值一致
#[allow(dead_code)]
fn capture_fresh_tree() -> Result<uia::UiaTree, String> {
    capture_fresh_tree_with_depth(5)
}

/// 从 UIA 树中查找 element_index 对应的元素,验证可见性后返回物理像素坐标
/// 拒绝: 零尺寸元素(无法定位中心)、离屏元素(被滚动隐藏)
fn resolve_element_point(tree: &uia::UiaTree, index: i32) -> Result<(i32, i32), String> {
    let element = tree.elements.iter().find(|e| e.element_index == index);
    match element {
        Some(e) => {
            log_debug(&format!("[RESOLVE] index={} 匹配 '{}', 类型={}, 坐标=({},{},{},{})",
                index, e.name, e.control_type,
                e.bounding_rect.x, e.bounding_rect.y, e.bounding_rect.width, e.bounding_rect.height));
            // 零尺寸元素无法定位点击中心
            if e.bounding_rect.width <= 0 || e.bounding_rect.height <= 0 {
                return Err(format!(
                    "element_index={} 是零尺寸元素 ({},{})，无法定位点击中心",
                    index, e.bounding_rect.width, e.bounding_rect.height
                ));
            }
            // 离屏元素被滚动隐藏，点击会偏离目标
            if e.is_offscreen {
                return Err(format!(
                    "element_index={} '{}' 是离屏元素 (被滚动隐藏)，需先滚动到可视区域",
                    index, e.name
                ));
            }
            let cx = e.bounding_rect.x + e.bounding_rect.width / 2;
            let cy = e.bounding_rect.y + e.bounding_rect.height / 2;
            // UIA CurrentBoundingRectangle 在 Per-Monitor V2 DPI 感知进程中
            // 直接返回物理像素坐标, 无需 DPI 转换
            log_debug(&format!("[RESOLVE] 中心物理坐标: ({},{})", cx, cy));
            Ok((cx, cy))
        },
        None => Err(format!("未找到 element_index={}，元素可能已消失或 UI 变化", index)),
    }
}

fn resolve_position(args: &Value, prefix: &str, ds: i32, observe_depth: i32) -> Result<(i32, i32), String> {
    let idx_key = format!("{}_element_index", prefix);
    let x_key = format!("{}_x", prefix);
    let y_key = format!("{}_y", prefix);

    if let Some(idx) = args.get(&idx_key).and_then(|v| v.as_i64()) {
        let tree = capture_fresh_tree_with_depth(observe_depth)?;
        resolve_element_point(&tree, idx as i32)
    } else {
        let x = args.get(&x_key).and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds;
        let y = args.get(&y_key).and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds;
        Ok((x, y))
    }
}

fn tool_type_text(args: &Value) -> Result<Value, String> {
    let text = args.get("text").and_then(|v| v.as_str()).ok_or("缺少 text 参数")?;
    let use_unicode = args.get("use_unicode").and_then(|v| v.as_bool()).unwrap_or(false);
    let include_screenshot = args.get("include_screenshot").and_then(|v| v.as_bool()).unwrap_or(true);

    input::type_text(text, use_unicode).map_err(|e| format!("输入失败: {}", e))?;

    capture_after_action(200, include_screenshot)
}

fn tool_press_key(args: &Value) -> Result<Value, String> {
    let key = args.get("key").and_then(|v| v.as_str()).ok_or("缺少 key 参数")?;
    let include_screenshot = args.get("include_screenshot").and_then(|v| v.as_bool()).unwrap_or(true);

    input::press_key(key).map_err(|e| format!("按键失败: {}", e))?;

    capture_after_action(200, include_screenshot)
}

fn tool_launch_app(args: &Value) -> Result<Value, String> {
    let aumid = args.get("aumid").and_then(|v| v.as_str()).ok_or("缺少 aumid 参数")?;
    let include_screenshot = args.get("include_screenshot").and_then(|v| v.as_bool()).unwrap_or(true);

    apps::launch_app(aumid).map_err(|e| format!("启动失败: {}", e))?;

    // 应用启动需要更长的等待时间
    capture_after_action(1500, include_screenshot)
}

fn tool_list_installed_apps(args: &Value) -> Result<Value, String> {
    let filter = args.get("filter").and_then(|v| v.as_str());
    let apps_list = apps::list_installed_apps(filter).map_err(|e| format!("枚举应用失败: {}", e))?;

    let apps_json: Vec<Value> = apps_list.iter().map(|app| {
        json!({ "name": app.name, "aumid": app.aumid })
    }).collect();

    Ok(json!({ "apps": apps_json, "count": apps_json.len() }))
}

/// 仅返回 UIA 元素树,不截取屏幕截图
/// 用于 AI 在连续操作间快速确认 UI 状态,响应体积约 10-50KB
fn tool_get_ui_tree(args: &Value) -> Result<Value, String> {
    let max_depth = args
        .get("max_depth")
        .and_then(|v| v.as_i64())
        .unwrap_or(5) as i32;
    capture_state_lightweight(max_depth)
}

fn tool_shutdown() -> Result<Value, String> {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    Ok(json!({ "success": true, "message": "服务器正在关闭" }))
}

fn register_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_window_state".into(),
            description: "获取前台窗口的全屏截图 + UIA 元素树。任务开始时必须调用此工具观察当前屏幕。坐标为原生物理像素 (1:1)。响应约 900KB，连续操作间可用 get_ui_tree 或 include_screenshot=false 减小体积".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "max_depth": { "type": "integer", "description": "UIA 树遍历最大深度，默认 8 足够定位绝大多数元素", "default": 8 }
                }
            }),
        },
        ToolDef {
            name: "get_ui_tree".into(),
            description: "仅获取前台窗口的 UIA 元素树，不含截图。响应约 10-50KB，用于连续操作间快速确认 UI 状态，避免每次都传输截图浪费 token".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "max_depth": { "type": "integer", "description": "UIA 树遍历最大深度，默认 5 适合中间状态检查", "default": 5 }
                }
            }),
        },
        ToolDef {
            name: "click".into(),
            description: "点击指定位置。优先用 element_index（UIA 树中的编号）定位，比坐标更稳定。支持批量: element_indices 数组或 points 数组。连续操作中间步骤设 include_screenshot=false 可省 token".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer", "description": "点击的 X 坐标（物理像素）" },
                    "y": { "type": "integer", "description": "点击的 Y 坐标（物理像素）" },
                    "element_index": { "type": "integer", "description": "UIA 元素编号，优先于 x/y" },
                    "element_indices": { "type": "array", "items": { "type": "integer" }, "description": "批量点击: element_index 数组，依次点击每个元素" },
                    "points": { "type": "array", "items": { "type": "object", "properties": { "x": { "type": "integer" }, "y": { "type": "integer" } } }, "description": "批量点击: 坐标数组" },
                    "button": { "type": "string", "enum": ["left", "right"], "default": "left" },
                    "click_count": { "type": "integer", "description": "1=单击, 2=双击", "default": 1 },
                    "include_screenshot": { "type": "boolean", "description": "false 时跳过截图，响应从 ~900KB 降至 ~10KB，用于连续操作中间步骤", "default": true },
                    "observe_depth": { "type": "integer", "description": "必须与观察时 get_ui_tree/get_window_state 的 max_depth 一致，否则 element_index 会错位。get_ui_tree 默认 5，get_window_state 默认 8", "default": 5 }
                }
            }),
        },
        ToolDef {
            name: "scroll".into(),
            description: "在指定位置滚动鼠标滚轮。delta_y 正值向下滚、负值向上滚；delta_x 正值向右、负值向左。优先用 element_index 定位滚动区域".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer", "description": "滚动位置的 X 坐标" },
                    "y": { "type": "integer", "description": "滚动位置的 Y 坐标" },
                    "element_index": { "type": "integer", "description": "滚动区域的 UIA 元素编号，优先于 x/y" },
                    "delta_x": { "type": "integer", "description": "水平滚动量，正=右，负=左", "default": 0 },
                    "delta_y": { "type": "integer", "description": "垂直滚动量，正=下，负=上", "default": 0 },
                    "include_screenshot": { "type": "boolean", "description": "false 时跳过截图", "default": true }
                }
            }),
        },
        ToolDef {
            name: "drag".into(),
            description: "从起点拖拽到终点。起终点均可独立使用 element_index 或坐标。典型场景: 拖动滑块、移动窗口、拖放文件".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "start_x": { "type": "integer", "description": "起点 X 坐标" },
                    "start_y": { "type": "integer", "description": "起点 Y 坐标" },
                    "end_x": { "type": "integer", "description": "终点 X 坐标" },
                    "end_y": { "type": "integer", "description": "终点 Y 坐标" },
                    "start_element_index": { "type": "integer", "description": "起点 UIA 元素编号" },
                    "end_element_index": { "type": "integer", "description": "终点 UIA 元素编号" },
                    "button": { "type": "string", "enum": ["left", "right"], "default": "left" },
                    "include_screenshot": { "type": "boolean", "description": "false 时跳过截图", "default": true }
                }
            }),
        },
        ToolDef {
            name: "type_text".into(),
            description: "向当前焦点元素输入文本。默认用剪贴板+Ctrl+V（快速通用）；设 use_unicode=true 改为逐字符 KEYEVENTF_UNICODE 注入（用于拦截 Ctrl+V 的应用或需保留剪贴板时）。调用前确保目标字段已聚焦（先 click 元素）".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string", "description": "要输入的文本内容" },
                    "use_unicode": { "type": "boolean", "description": "true=逐字符 Unicode 事件注入，false=剪贴板粘贴", "default": false },
                    "include_screenshot": { "type": "boolean", "description": "false 时跳过截图", "default": true }
                },
                "required": ["text"]
            }),
        },
        ToolDef {
            name: "press_key".into(),
            description: "按下按键或组合键。修饰键+主键用 + 连接，如 ctrl+c、alt+tab、win+d。仅用于无 UI 等价的快捷键、模态关闭(Escape)或键盘导航；有 UIA 目标时优先用 click".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string", "description": "按键表达式，如 enter、ctrl+c、alt+tab、win+r、f5" },
                    "include_screenshot": { "type": "boolean", "description": "false 时跳过截图", "default": true }
                },
                "required": ["key"]
            }),
        },
        ToolDef {
            name: "launch_app".into(),
            description: "通过 AUMID 启动或激活应用程序。若应用已运行则激活其窗口。启动后需等待 1-2 秒再 get_window_state 确认".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "aumid": { "type": "string", "description": "应用的 AUMID，从 list_installed_apps 获取" },
                    "include_screenshot": { "type": "boolean", "description": "false 时跳过截图", "default": true }
                },
                "required": ["aumid"]
            }),
        },
        ToolDef {
            name: "list_installed_apps".into(),
            description: "列出已安装应用程序的名称和 AUMID。filter 做大小写不敏感的子串匹配。用于启动应用前获取 AUMID".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string", "description": "按应用名称过滤（大小写不敏感子串匹配）" }
                }
            }),
        },
        ToolDef {
            name: "shutdown".into(),
            description: "关闭 MCP 服务器".into(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
    ]
}

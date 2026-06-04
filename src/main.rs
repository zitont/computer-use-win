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
fn log_debug(msg: &str) {
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
            // 从客户端请求中提取协议版本,回传相同版本以完成版本协商
            let client_protocol = req.params
                .as_ref()
                .and_then(|p| p.get("protocolVersion"))
                .and_then(|v| v.as_str());
            Some(JsonRpcResponse::success(req.id, mcp::initialize_result(client_protocol)))
        }
        "notifications/initialized" => {
            // 这是通知,不返回响应
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
        "cursor_position": { "x": cursor_pos.x, "y": cursor_pos.y },
        "elements": uia::elements_to_json(&tree.elements)
    }))
}

/// 操作工具执行后,等待 UI 稳定并返回最新窗口状态
fn capture_after_action(delay_ms: u64) -> Result<Value, String> {
    std::thread::sleep(std::time::Duration::from_millis(delay_ms));
    capture_state(10)
}

fn tool_get_window_state(args: &Value) -> Result<Value, String> {
    let max_depth = args
        .get("max_depth")
        .and_then(|v| v.as_i64())
        .unwrap_or(10) as i32;
    capture_state(max_depth)
}

fn tool_click(args: &Value) -> Result<Value, String> {
    let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let click_count = args.get("click_count").and_then(|v| v.as_i64()).unwrap_or(1) as i32;
    let ds = screenshot::get_downscale();

    // 批量模式: element_indices 数组
    if let Some(indices) = args.get("element_indices").and_then(|v| v.as_array()) {
        let mut tree = uia::UiaTree::new();
        tree.capture_foreground(10).map_err(|e| format!("UIA 遍历失败: {}", e))?;

        for val in indices {
            let idx = val.as_i64().ok_or("element_indices 元素必须为整数")?;
            let element = tree.elements.iter().find(|e| e.element_index == idx as i32);
            match element {
                Some(e) => {
                    // UIA 坐标已是物理屏幕空间,直接使用
                    let cx = e.bounding_rect.x + e.bounding_rect.width / 2;
                    let cy = e.bounding_rect.y + e.bounding_rect.height / 2;
                    input::click(cx, cy, button, click_count)
                        .map_err(|e| format!("点击失败: {}", e))?;
                    std::thread::sleep(std::time::Duration::from_millis(100));
                }
                None => return Err(format!("未找到 element_index={}", idx)),
            }
        }
        return capture_after_action(200);
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
        return capture_after_action(200);
    }

    // 单次点击模式
    let x = args.get("x").and_then(|v| v.as_i64());
    let y = args.get("y").and_then(|v| v.as_i64());
    let element_index = args.get("element_index").and_then(|v| v.as_i64());

    let (final_x, final_y) = if let Some(idx) = element_index {
        let mut tree = uia::UiaTree::new();
        tree.capture_foreground(10).map_err(|e| format!("UIA 遍历失败: {}", e))?;
        let element = tree.elements.iter().find(|e| e.element_index == idx as i32);
        match element {
            Some(e) => (
                e.bounding_rect.x + e.bounding_rect.width / 2,
                e.bounding_rect.y + e.bounding_rect.height / 2,
            ),
            None => return Err(format!("未找到 element_index={}", idx)),
        }
    } else if let (Some(x), Some(y)) = (x, y) {
        // AI 坐标在图像空间 (0..out_w),乘以缩小倍数得到物理屏幕坐标
        (x as i32 * ds, y as i32 * ds)
    } else {
        return Err("需要提供 x/y 坐标、element_index、element_indices 或 points".to_string());
    };

    input::click(final_x, final_y, button, click_count)
        .map_err(|e| format!("点击失败: {}", e))?;

    capture_after_action(200)
}

fn tool_scroll(args: &Value) -> Result<Value, String> {
    let delta_x = args.get("delta_x").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let delta_y = args.get("delta_y").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
    let element_index = args.get("element_index").and_then(|v| v.as_i64());
    let ds = screenshot::get_downscale();

    let (final_x, final_y) = if let Some(idx) = element_index {
        let mut tree = uia::UiaTree::new();
        tree.capture_foreground(10).map_err(|e| format!("UIA 遍历失败: {}", e))?;
        let element = tree.elements.iter().find(|e| e.element_index == idx as i32);
        match element {
            Some(e) => (
                e.bounding_rect.x + e.bounding_rect.width / 2,
                e.bounding_rect.y + e.bounding_rect.height / 2,
            ),
            None => return Err(format!("未找到 element_index={}", idx)),
        }
    } else {
        (
            args.get("x").and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds,
            args.get("y").and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds,
        )
    };

    input::scroll(final_x, final_y, delta_x, delta_y)
        .map_err(|e| format!("滚动失败: {}", e))?;

    capture_after_action(300)
}

fn tool_drag(args: &Value) -> Result<Value, String> {
    let button = args.get("button").and_then(|v| v.as_str()).unwrap_or("left");
    let ds = screenshot::get_downscale();

    let (start_x, start_y) = resolve_position(args, "start", ds)?;
    let (end_x, end_y) = resolve_position(args, "end", ds)?;

    input::drag(start_x, start_y, end_x, end_y, button)
        .map_err(|e| format!("拖拽失败: {}", e))?;

    capture_after_action(300)
}

fn resolve_position(args: &Value, prefix: &str, ds: i32) -> Result<(i32, i32), String> {
    let idx_key = format!("{}_element_index", prefix);
    let x_key = format!("{}_x", prefix);
    let y_key = format!("{}_y", prefix);

    if let Some(idx) = args.get(&idx_key).and_then(|v| v.as_i64()) {
        let mut tree = uia::UiaTree::new();
        tree.capture_foreground(10).map_err(|e| format!("UIA 遍历失败: {}", e))?;
        let element = tree.elements.iter().find(|e| e.element_index == idx as i32);
        match element {
            Some(e) => Ok((
                e.bounding_rect.x + e.bounding_rect.width / 2,
                e.bounding_rect.y + e.bounding_rect.height / 2,
            )),
            None => Err(format!("未找到 element_index={}", idx)),
        }
    } else {
        let x = args.get(&x_key).and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds;
        let y = args.get(&y_key).and_then(|v| v.as_i64()).unwrap_or(0) as i32 * ds;
        Ok((x, y))
    }
}

fn tool_type_text(args: &Value) -> Result<Value, String> {
    let text = args.get("text").and_then(|v| v.as_str()).ok_or("缺少 text 参数")?;
    let use_unicode = args.get("use_unicode").and_then(|v| v.as_bool()).unwrap_or(false);

    input::type_text(text, use_unicode).map_err(|e| format!("输入失败: {}", e))?;

    capture_after_action(200)
}

fn tool_press_key(args: &Value) -> Result<Value, String> {
    let key = args.get("key").and_then(|v| v.as_str()).ok_or("缺少 key 参数")?;
    input::press_key(key).map_err(|e| format!("按键失败: {}", e))?;

    capture_after_action(200)
}

fn tool_launch_app(args: &Value) -> Result<Value, String> {
    let aumid = args.get("aumid").and_then(|v| v.as_str()).ok_or("缺少 aumid 参数")?;
    apps::launch_app(aumid).map_err(|e| format!("启动失败: {}", e))?;

    // 应用启动需要更长的等待时间
    capture_after_action(1500)
}

fn tool_list_installed_apps(args: &Value) -> Result<Value, String> {
    let filter = args.get("filter").and_then(|v| v.as_str());
    let apps_list = apps::list_installed_apps(filter).map_err(|e| format!("枚举应用失败: {}", e))?;

    let apps_json: Vec<Value> = apps_list.iter().map(|app| {
        json!({ "name": app.name, "aumid": app.aumid })
    }).collect();

    Ok(json!({ "apps": apps_json, "count": apps_json.len() }))
}

fn tool_shutdown() -> Result<Value, String> {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
    Ok(json!({ "success": true, "message": "服务器正在关闭" }))
}

fn register_tools() -> Vec<ToolDef> {
    vec![
        ToolDef {
            name: "get_window_state".into(),
            description: "获取全屏截图 + UI 元素树,坐标为图像空间 (物理像素 / DOWNSCALE)".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "max_depth": { "type": "integer", "description": "UI 树遍历最大深度", "default": 10 }
                }
            }),
        },
        ToolDef {
            name: "click".into(),
            description: "在图像空间坐标或 element_index 处点击,返回操作后截图。支持批量: element_indices 数组或 points 数组".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "element_index": { "type": "integer" },
                    "element_indices": { "type": "array", "items": { "type": "integer" }, "description": "批量点击: element_index 数组" },
                    "points": { "type": "array", "items": { "type": "object", "properties": { "x": { "type": "integer" }, "y": { "type": "integer" } } }, "description": "批量点击: 坐标数组" },
                    "button": { "type": "string", "default": "left" },
                    "click_count": { "type": "integer", "default": 1 }
                }
            }),
        },
        ToolDef {
            name: "scroll".into(),
            description: "在指定坐标或 element_index 处滚动,返回操作后截图".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "x": { "type": "integer" },
                    "y": { "type": "integer" },
                    "element_index": { "type": "integer" },
                    "delta_x": { "type": "integer", "default": 0 },
                    "delta_y": { "type": "integer", "default": 0 }
                }
            }),
        },
        ToolDef {
            name: "drag".into(),
            description: "从起点拖拽到终点,返回操作后截图".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "start_x": { "type": "integer" },
                    "start_y": { "type": "integer" },
                    "end_x": { "type": "integer" },
                    "end_y": { "type": "integer" },
                    "start_element_index": { "type": "integer" },
                    "end_element_index": { "type": "integer" },
                    "button": { "type": "string", "default": "left" }
                }
            }),
        },
        ToolDef {
            name: "type_text".into(),
            description: "向当前焦点元素输入文本,返回操作后截图".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "text": { "type": "string" },
                    "use_unicode": { "type": "boolean", "default": false }
                },
                "required": ["text"]
            }),
        },
        ToolDef {
            name: "press_key".into(),
            description: "按下单个按键或组合键,返回操作后截图".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "key": { "type": "string" }
                },
                "required": ["key"]
            }),
        },
        ToolDef {
            name: "launch_app".into(),
            description: "通过 AUMID 启动应用程序,返回启动后截图".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "aumid": { "type": "string" }
                },
                "required": ["aumid"]
            }),
        },
        ToolDef {
            name: "list_installed_apps".into(),
            description: "列出已安装的应用程序".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "filter": { "type": "string" }
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

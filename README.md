# computer-use-win

Windows 桌面自动化 MCP 服务器，基于 Rust + Windows UI Automation (UIA)，为 AI Agent 提供屏幕观察与桌面操控能力。

单二进制文件，无 Electron，无 Node.js，开箱即用。

## 为什么需要这个

AI Agent 需要"看"屏幕和"操作"桌面才能完成真实任务。本项目通过 MCP 协议将 Windows 桌面能力封装为标准化工具，让任何支持 MCP 的 AI 客户端（QoderWork、Claude Desktop、Cursor 等）都能操控 Windows 应用。

与基于截图坐标的方案不同，本项目优先使用 UIA 元素树定位 -- AI 描述"点击名为'发送'的按钮"比"点击坐标 (847, 523)"更可靠，且不受窗口位置变化影响。

## 架构

```
AI 客户端 (QoderWork / Claude Desktop / ...)
    |
    | MCP stdio (JSON-RPC 2.0)
    |
computer-use-win.exe
    |
    |-- UI Automation (IUIAutomation)  --> 元素树遍历、焦点管理
    |-- GDI (BitBlt)                   --> 全屏截图、JPEG 编码
    |-- SendInput                      --> 鼠标点击、键盘输入
    |-- Shell API                      --> 应用启动、窗口管理
```

## 工具一览

通过 MCP stdio 传输暴露 10 个工具，AI 通过自然语言描述即可调用：

| 工具 | 用途 | 典型场景 |
|------|------|----------|
| `get_window_state` | 全屏截图 + UIA 元素树 | 任务开始时观察屏幕，响应约 900KB |
| `get_ui_tree` | 仅 UIA 元素树 | 连续操作间快速确认状态，响应约 10-50KB |
| `click` | 点击 | 按钮、链接、菜单项，支持 element_index / 坐标 / 批量 |
| `scroll` | 滚轮 | 页面滚动、列表浏览 |
| `drag` | 拖拽 | 滑块调节、窗口移动、文件拖放 |
| `type_text` | 文本输入 | 搜索框、表单填写、消息发送 |
| `press_key` | 按键/组合键 | 快捷键 (ctrl+c)、模态关闭 (Escape) |
| `launch_app` | 启动/激活应用 | 打开计算器、浏览器等 |
| `list_installed_apps` | 枚举已安装应用 | 获取 AUMID 供 launch_app 使用 |
| `shutdown` | 关闭服务器 | 优雅退出 |

## 工具参数详解

### get_window_state

获取前台窗口的全屏截图和 UIA 元素树。AI 任务的第一步应调用此工具观察当前屏幕状态。

```json
{ "max_depth": 8 }
```

- `max_depth` (默认 8)：UIA 树遍历深度，8 层足够定位绝大多数应用的交互元素

返回内容：
- `image`：Base64 JPEG 截图（原生物理像素，无缩放）
- `elements`：管道符分隔的 UIA 元素列表，每行格式为 `index|type|name|automation_id|class_name|x,y,w,h|flags`
- `cursor`：当前光标位置 `"x,y"`
- `window_title` / `process_name`：前台窗口信息

### get_ui_tree

仅获取 UIA 元素树，不含截图。连续操作间的中间步骤使用此工具可将响应体积从 ~900KB 降至 ~10-50KB，大幅节省 token。

```json
{ "max_depth": 5 }
```

### click

点击指定位置。优先使用 `element_index`（UIA 树中的编号）定位，比坐标更稳定。

```json
{
  "element_index": 39,
  "observe_depth": 5,
  "include_screenshot": false
}
```

**定位方式（优先级从高到低）：**

1. `element_index`：UIA 元素编号，推荐使用。操作前需先通过 `get_ui_tree` 获取元素列表
2. `x` / `y`：物理像素坐标，直接传给 SetCursorPos
3. `element_indices`：批量点击，传入编号数组依次点击
4. `points`：批量点击，传入坐标数组 `[{x, y}, ...]`

**关键参数：**

- `observe_depth`：必须与观察时 `get_ui_tree` 的 `max_depth` 一致，否则 `element_index` 会映射到不同元素
- `include_screenshot`：设为 `false` 可跳过截图，响应体积降至 ~10KB
- `button`：`"left"`（默认）或 `"right"`
- `click_count`：`1`（单击，默认）或 `2`（双击）

**元素树格式：**

```
index|type|name|automation_id|class_name|x,y,w,h|flags
```

- `flags`：`!` = 禁用，`O` = 离屏，`*` = 有键盘焦点
- 离屏元素（`O`）需先滚动到可视区域才能点击
- 零尺寸元素（w=0 或 h=0）无法定位点击中心

### scroll

在指定位置滚动鼠标滚轮。

```json
{
  "element_index": 25,
  "delta_y": -3,
  "include_screenshot": false
}
```

- `delta_y`：正值向下滚，负值向上滚
- `delta_x`：正值向右，负值向左（水平滚动）
- 支持 `element_index` 定位滚动区域

### drag

从起点拖拽到终点。起终点均可独立使用 `element_index` 或坐标。

```json
{
  "start_element_index": 10,
  "end_element_index": 20
}
```

或：

```json
{
  "start_x": 100, "start_y": 200,
  "end_x": 500, "end_y": 200
}
```

### type_text

向当前焦点元素输入文本。

```json
{
  "text": "Hello World",
  "use_unicode": false
}
```

- `use_unicode`：`false`（默认）使用剪贴板 + Ctrl+V，快速通用；`true` 使用逐字符 Unicode 事件注入，适用于拦截 Ctrl+V 的应用或需保留剪贴板时
- 调用前需先 `click` 目标输入框使其获得焦点

### press_key

按下按键或组合键。修饰键用 `+` 连接。

```json
{ "key": "ctrl+c" }
```

支持的修饰键：`ctrl`、`alt`、`shift`、`win`
支持的功能键：`enter`、`escape`、`tab`、`space`、`backspace`、`delete`、`f1`-`f12`、方向键等

### launch_app

通过 AUMID 启动或激活应用程序。

```json
{ "aumid": "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App" }
```

若应用已运行则激活其窗口。启动后需等待 1-2 秒再调用 `get_window_state` 确认。

### list_installed_apps

列出已安装应用程序。

```json
{ "filter": "计算器" }
```

`filter` 做大小写不敏感的子串匹配。用于在调用 `launch_app` 前获取 AUMID。

## 快速开始

### 1. 构建

```bash
cargo build --release
```

输出 `target/release/computer-use-win.exe`（约 500KB）。

### 2. 配置 MCP 客户端

在 MCP 客户端配置中添加（以 QoderWork 为例）：

```json
{
  "mcpServers": {
    "rust-computer-use": {
      "command": "D:\\path\\to\\computer-use-win.exe"
    }
  }
}
```

### 3. 在 AI 对话中使用

配置完成后，AI Agent 会自动发现 10 个工具。示例对话：

> **用户**：帮我打开计算器算一下 123+456
>
> **AI**：我先获取屏幕状态...
> *调用 get_window_state*
> 然后启动计算器...
> *调用 launch_app with aumid="Microsoft.WindowsCalculator_8wekyb3d8bbwe!App"*
> 现在点击按钮输入 123...
> *调用 click with element_index=39 (按钮 1)*
> *调用 click with element_index=40 (按钮 2)*
> ...

## 典型工作流

```
1. get_window_state          -- 观察屏幕，获取元素树
2. click(element_index=X)    -- 点击目标按钮
3. get_ui_tree               -- 确认操作结果（省 token）
4. type_text(text="...")     -- 输入文本
5. press_key(key="enter")    -- 按回车确认
6. get_ui_tree               -- 再次确认
```

**省 token 技巧：**

- 连续操作中间步骤使用 `get_ui_tree` 代替 `get_window_state`
- 点击/滚动/输入时设 `include_screenshot=false`
- 用 `element_index` 定位比坐标更稳定，且减少截图分析需求

## 技术细节

### UIA 元素树

通过 `IUIAutomation` COM 接口遍历前台窗口的 UI Automation 树。使用 `RawViewWalker` 而非 `ControlViewWalker`，确保隐藏的子元素（如微信的自定义控件）也能被暴露。

每个元素包含：
- `element_index`：遍历序号，作为稳定标识符
- `control_type`：控件类型（Button、Edit、Text、ListItem 等）
- `name`：控件名称
- `automation_id`：自动化 ID（用于区分同名元素）
- `class_name`：窗口类名
- `bounding_rect`：边界矩形 `{x, y, width, height}`
- `is_enabled` / `is_offscreen` / `has_keyboard_focus`：状态标志

### DPI 感知

使用 `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` 初始化，UIA 的 `CurrentBoundingRectangle` 在此模式下直接返回物理像素坐标，无需额外的逻辑坐标到物理坐标的转换。

### 截图

使用 GDI `BitBlt` 从屏幕 DC 捕获全屏截图，JPEG 编码（质量 92），Base64 编码后通过 MCP 响应传输。支持多显示器环境。

### 鼠标模拟

使用 `SendInput` API 发送合成鼠标事件。点击前通过 `WindowFromPoint` 激活目标窗口，通过 `IUIAutomationElement::SetFocus` 设置 UIA 焦点（解决微信等应用不响应 SendInput 的问题）。鼠标移动使用 ease-in-out 曲线模拟人类操作轨迹。

## 系统要求

- Windows 10 1607+ 或 Windows 11
- 需要 UI Automation 支持（系统默认启用）
- 无运行时依赖

## 下载

从 [Releases](https://github.com/zitont/computer-use-win/releases) 页面下载最新版本。

## License

MIT

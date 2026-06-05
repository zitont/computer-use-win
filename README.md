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

## 使用示例

### 场景 1：打开计算器算 123+456

```
用户：帮我用计算器算一下 123+456

AI 执行步骤：
1. launch_app(aumid="Microsoft.WindowsCalculator_8wekyb3d8bbwe!App")
   --> 启动计算器

2. get_window_state(max_depth=8)
   --> 观察屏幕，获得元素树：
   39|Button|一|num1Button|...|708,1002,115,77|
   40|Button|二|num2Button|...|825,1002,114,77|
   41|Button|三|num3Button|...|943,1002,114,77|
   47|Button|加|plusButton|...|1062,1002,114,76|
   ...

3. click(element_index=39, observe_depth=8, include_screenshot=false)
   --> 点击"1"

4. click(element_index=40, observe_depth=8, include_screenshot=false)
   --> 点击"2"

5. click(element_index=41, observe_depth=8, include_screenshot=false)
   --> 点击"3"

6. click(element_index=47, observe_depth=8, include_screenshot=false)
   --> 点击"+"

7. click(element_index=39, observe_depth=8, include_screenshot=false)
   --> 点击"4"

8. click(element_index=40, observe_depth=8, include_screenshot=false)
   --> 点击"5"

9. click(element_index=41, observe_depth=8, include_screenshot=false)
   --> 点击"6"

10. click(element_index=49, observe_depth=8, include_screenshot=false)
    --> 点击"="

11. get_ui_tree(max_depth=8)
    --> 确认结果：显示为 579
```

### 场景 2：在 QQ 中给"我的手机"发消息

```
用户：帮我在QQ里给我的手机发一条"你好"

AI 执行步骤：
1. get_window_state(max_depth=10)
   --> 观察屏幕，找到 QQ 窗口和左侧栏的"我的手机"按钮

2. click(element_index=59, observe_depth=10)
   --> 点击左侧栏"我的手机"按钮

3. get_ui_tree(max_depth=10)
   --> 确认聊天窗口已打开，找到输入框

4. click(element_index=125, observe_depth=10)
   --> 点击输入框使其获得焦点

5. type_text(text="你好", include_screenshot=false)
   --> 输入文字

6. click(element_index=128, observe_depth=10, include_screenshot=false)
   --> 点击"发送"按钮
```

### 场景 3：浏览网页并填写表单

```
用户：帮我在浏览器里搜索"Rust programming"

AI 执行步骤：
1. get_window_state(max_depth=8)
   --> 观察当前浏览器页面

2. click(element_index=15, observe_depth=8)
   --> 点击地址栏

3. type_text(text="https://www.google.com", include_screenshot=false)
   --> 输入网址

4. press_key(key="enter", include_screenshot=false)
   --> 按回车导航

5. get_window_state(max_depth=8)
   --> 等待页面加载，观察搜索框位置

6. click(element_index=20, observe_depth=8)
   --> 点击搜索框

7. type_text(text="Rust programming", include_screenshot=false)
   --> 输入搜索关键词

8. press_key(key="enter", include_screenshot=false)
   --> 按回车搜索
```

### 场景 4：批量操作 - 快速填写多行数据

```
用户：帮我在表格里输入这5个数字

AI 执行步骤：
1. get_window_state(max_depth=8)
   --> 观察表格结构

2. click(element_indices=[10, 11, 12, 13, 14], include_screenshot=false)
   --> 批量点击5个单元格（依次聚焦）

   或者分步操作：
   click(element_index=10, observe_depth=8) --> 聚焦第1个单元格
   type_text(text="100", include_screenshot=false)
   press_key(key="tab", include_screenshot=false) --> 跳到下一个单元格
   type_text(text="200", include_screenshot=false)
   press_key(key="tab", include_screenshot=false)
   ...
```

### 场景 5：拖拽滑块调节音量

```
用户：帮我把系统音量调到50%

AI 执行步骤：
1. press_key(key="win+r")
   --> 打开运行对话框

2. type_text(text="sndvol", include_screenshot=false)
   --> 输入音量控制程序名

3. press_key(key="enter", include_screenshot=false)
   --> 启动音量控制

4. get_window_state(max_depth=8)
   --> 观察滑块位置

5. drag(start_element_index=15, end_x=500, end_y=300)
   --> 从滑块当前位置拖拽到50%位置
```

## 工具参数参考

### get_window_state / get_ui_tree

```json
{ "max_depth": 8 }
```

- `max_depth`：UIA 树遍历深度。`get_window_state` 默认 8，`get_ui_tree` 默认 5
- 返回 `elements` 格式：`index|type|name|automation_id|class_name|x,y,w,h|flags`
- `flags`：`!` = 禁用，`O` = 离屏，`*` = 有键盘焦点

### click

```json
{
  "element_index": 39,
  "observe_depth": 8,
  "include_screenshot": false,
  "button": "left",
  "click_count": 1
}
```

- `element_index`：UIA 元素编号（推荐，需先通过 get_ui_tree 获取）
- `x` / `y`：物理像素坐标（备选）
- `element_indices`：批量点击编号数组
- `points`：批量点击坐标数组 `[{x, y}, ...]`
- `observe_depth`：必须与观察时 `max_depth` 一致，否则 element_index 会错位

### scroll

```json
{
  "element_index": 25,
  "delta_y": -3,
  "include_screenshot": false
}
```

- `delta_y`：正值向下滚，负值向上滚
- `delta_x`：正值向右，负值向左

### drag

```json
{
  "start_element_index": 10,
  "end_element_index": 20
}
```

起终点均可独立使用 `element_index` 或坐标 (`start_x`/`start_y`/`end_x`/`end_y`)。

### type_text

```json
{
  "text": "Hello World",
  "use_unicode": false,
  "include_screenshot": false
}
```

- `use_unicode`：`false` = 剪贴板粘贴（默认，快速通用）；`true` = 逐字符 Unicode 注入（拦截 Ctrl+V 的应用）
- 调用前需先 click 目标输入框

### press_key

```json
{ "key": "ctrl+c" }
```

修饰键：`ctrl`、`alt`、`shift`、`win`，用 `+` 连接。支持 `enter`、`escape`、`tab`、`f1`-`f12` 等。

### launch_app

```json
{ "aumid": "Microsoft.WindowsCalculator_8wekyb3d8bbwe!App" }
```

通过 `list_installed_apps` 获取 AUMID。已运行则激活窗口。

### list_installed_apps

```json
{ "filter": "计算器" }
```

大小写不敏感子串匹配。

## 省 token 技巧

- 连续操作中间步骤用 `get_ui_tree` 代替 `get_window_state`（900KB → 10KB）
- 点击/滚动/输入时设 `include_screenshot=false`
- 用 `element_index` 定位比坐标更稳定，减少截图分析需求
- 操作完成后用 `get_ui_tree` 确认结果，不必每次都截全屏

## 快速开始

### 构建

```bash
cargo build --release
```

输出 `target/release/computer-use-win.exe`（约 500KB）。

### 配置 MCP 客户端

```json
{
  "mcpServers": {
    "rust-computer-use": {
      "command": "D:\\path\\to\\computer-use-win.exe"
    }
  }
}
```

## 技术细节

### UIA 元素树

通过 `IUIAutomation` COM 接口遍历前台窗口的 UI Automation 树。使用 `RawViewWalker` 而非 `ControlViewWalker`，确保隐藏的子元素（如微信的自定义控件）也能被暴露。

每个元素包含：`element_index`（遍历序号）、`control_type`（控件类型）、`name`（控件名称）、`automation_id`（自动化 ID）、`class_name`（窗口类名）、`bounding_rect`（边界矩形）、状态标志（enabled/offscreen/focus）。

### DPI 感知

使用 `DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2` 初始化，UIA 的 `CurrentBoundingRectangle` 在此模式下直接返回物理像素坐标。

### 截图

GDI `BitBlt` 全屏截图 → JPEG 编码（质量 92）→ Base64 传输。支持多显示器。

### 鼠标模拟

`SendInput` 发送合成事件。点击前 `WindowFromPoint` 激活目标窗口 + `IUIAutomationElement::SetFocus` 设置焦点。鼠标移动使用 ease-in-out 曲线模拟人类轨迹。

## 系统要求

- Windows 10 1607+ 或 Windows 11
- 无运行时依赖

## 下载

从 [Releases](https://github.com/zitont/computer-use-win/releases) 页面下载最新版本。

## License

MIT

# computer-use-win

Windows 桌面自动化 MCP 服务器，基于 Rust + Windows UI Automation，为 AI Agent 提供屏幕观察与桌面操控能力。

## 功能

通过 MCP (Model Context Protocol) stdio 传输，暴露 10 个工具：

| 工具 | 说明 |
|------|------|
| `get_window_state` | 全屏截图 + UIA 元素树（约 900KB） |
| `get_ui_tree` | 仅 UIA 元素树，无截图（约 10-50KB） |
| `click` | 点击，支持 element_index / 坐标 / 批量 |
| `scroll` | 滚轮，支持 element_index 定位 |
| `drag` | 拖拽，起终点独立定位 |
| `type_text` | 文本输入，剪贴板粘贴或 Unicode 逐字符注入 |
| `press_key` | 按键或组合键（如 ctrl+c、alt+tab） |
| `launch_app` | 通过 AUMID 启动或激活应用 |
| `list_installed_apps` | 枚举已安装应用 |
| `shutdown` | 关闭服务器 |

## 核心特性

- **UIA 元素树**：遍历前台窗口的 UI Automation 树，返回元素编号、类型、名称、automation_id、class_name、边界矩形和状态标志
- **DPI 感知**：Per-Monitor V2，UIA 坐标直接映射物理像素，无需额外转换
- **元素验证**：点击前校验零尺寸和离屏元素，拒绝无效操作
- **轻量模式**：`include_screenshot=false` 跳过截图，响应体积降至十分之一
- **批量操作**：`element_indices` / `points` 数组一次性执行多次点击
- **平滑鼠标**：ease-in-out 曲线模拟人类操作轨迹

## 构建

```bash
cargo build --release
```

输出 `target/release/computer-use-win.exe`。

## 使用

在 MCP 客户端配置中添加：

```json
{
  "mcpServers": {
    "rust-computer-use": {
      "command": "path/to/computer-use-win.exe"
    }
  }
}
```

## 依赖

仅依赖 `windows` crate 调用 Win32 API，无 Electron，无 Node.js，单二进制文件约 500KB。

## License

MIT

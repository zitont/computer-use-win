use serde::{Deserialize, Serialize};
use serde_json::Value;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UiElement {
    #[serde(rename = "element_index")]
    pub element_index: i32,
    #[serde(rename = "control_type")]
    pub control_type: String,
    pub name: String,
    /// AutomationId: 唯一标识控件的稳定 ID,用于区分同名元素
    #[serde(rename = "automation_id")]
    pub automation_id: String,
    /// ClassName: 控件的窗口类名,辅助识别无法通过 name 区分的元素
    #[serde(rename = "class_name")]
    pub class_name: String,
    #[serde(rename = "is_enabled")]
    pub is_enabled: bool,
    #[serde(rename = "bounding_rect")]
    pub bounding_rect: Rect,
    #[serde(rename = "has_keyboard_focus")]
    pub has_keyboard_focus: bool,
    /// 元素是否在可视区域内 (Offscreen=false 时可点击)
    #[serde(rename = "is_offscreen")]
    pub is_offscreen: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

pub struct UiaTree {
    pub elements: Vec<UiElement>,
}

impl UiaTree {
    pub fn new() -> Self {
        Self { elements: Vec::new() }
    }

    pub fn capture_foreground(&mut self, max_depth: i32) -> Result<HWND> {
        unsafe {
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        }

        let foreground_hwnd = unsafe { GetForegroundWindow() };
        if foreground_hwnd.is_invalid() {
            return Err(E_FAIL.into());
        }

        let automation: IUIAutomation = unsafe {
            CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER)?
        };
        let root: IUIAutomationElement = unsafe {
            automation.ElementFromHandle(foreground_hwnd)?
        };
        // 优先用 RawViewWalker: 微信等应用会隐藏 ControlView 子树,
        // RawView 能强制暴露所有子元素,提供 element_index 供点击使用
        let walker: IUIAutomationTreeWalker = unsafe {
            match automation.RawViewWalker() {
                Ok(w) => w,
                Err(_) => automation.ControlViewWalker()?,
            }
        };

        let mut index = 0;
        self.walk_element(&automation, &walker, &root, &mut index, 0, max_depth)?;

        Ok(foreground_hwnd)
    }

    fn walk_element(
        &mut self,
        automation: &IUIAutomation,
        walker: &IUIAutomationTreeWalker,
        element: &IUIAutomationElement,
        index: &mut i32,
        depth: i32,
        max_depth: i32,
    ) -> Result<()> {
        if depth > max_depth {
            return Ok(());
        }

        let name = unsafe { element.CurrentName().unwrap_or_default() };
        let control_type_id = unsafe {
            element.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0)).0
        };
        let control_type_name = control_type_to_string(control_type_id);
        let is_enabled = unsafe {
            element.CurrentIsEnabled().map(|b| b.as_bool()).unwrap_or(false)
        };
        let has_focus = unsafe {
            element.CurrentHasKeyboardFocus().map(|b| b.as_bool()).unwrap_or(false)
        };
        let rect = unsafe { element.CurrentBoundingRectangle().unwrap_or_default() };

        // 获取 AutomationId 和 ClassName,增强元素辨识度
        let automation_id = unsafe {
            element.CurrentAutomationId().unwrap_or_default().to_string()
        };
        let class_name = unsafe {
            element.CurrentClassName().unwrap_or_default().to_string()
        };
        let is_offscreen = unsafe {
            element.CurrentIsOffscreen().map(|b| b.as_bool()).unwrap_or(false)
        };

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        // 跳过零尺寸元素: 宽或高为 0 时无法定位点击中心,无交互价值
        // 但根元素(Window 类型,depth=0)即使尺寸异常也保留,因为它是树的锚点
        let is_zero_size = width <= 0 || height <= 0;
        if is_zero_size && depth > 0 {
            // 仍需遍历子树,因为零尺寸容器可能有正常尺寸的子元素
            if let Ok(child) = unsafe { walker.GetFirstChildElement(element) } {
                let mut current = child;
                loop {
                    self.walk_element(automation, walker, &current, index, depth + 1, max_depth)?;
                    match unsafe { walker.GetNextSiblingElement(&current) } {
                        Ok(next) => current = next,
                        Err(_) => break,
                    }
                }
            }
            return Ok(());
        }

        let current_index = *index;
        *index += 1;

        self.elements.push(UiElement {
            element_index: current_index,
            control_type: control_type_name,
            name: name.to_string(),
            automation_id,
            class_name,
            is_enabled,
            bounding_rect: Rect {
                x: rect.left,
                y: rect.top,
                width,
                height,
            },
            has_keyboard_focus: has_focus,
            is_offscreen,
        });

        if let Ok(child) = unsafe { walker.GetFirstChildElement(element) } {
            let mut current = child;
            loop {
                self.walk_element(automation, walker, &current, index, depth + 1, max_depth)?;
                match unsafe { walker.GetNextSiblingElement(&current) } {
                    Ok(next) => current = next,
                    Err(_) => break,
                }
            }
        }

        Ok(())
    }
}

fn control_type_to_string(id: i32) -> String {
    match id {
        50000 => "Button",
        50001 => "Calendar",
        50002 => "CheckBox",
        50003 => "ComboBox",
        50004 => "Edit",
        50005 => "Hyperlink",
        50006 => "Image",
        50007 => "ListItem",
        50008 => "List",
        50009 => "Menu",
        50010 => "MenuBar",
        50011 => "MenuItem",
        50012 => "ProgressBar",
        50013 => "RadioButton",
        50014 => "ScrollBar",
        50015 => "Slider",
        50016 => "Spinner",
        50017 => "StatusBar",
        50018 => "Tab",
        50019 => "TabItem",
        50020 => "Text",
        50021 => "ToolBar",
        50022 => "ToolTip",
        50023 => "Tree",
        50024 => "TreeItem",
        50025 => "DataGrid",
        50026 => "DataItem",
        50027 => "Document",
        50028 => "SplitButton",
        50029 => "Window",
        50030 => "Pane",
        50031 => "Header",
        50032 => "HeaderItem",
        50033 => "Table",
        50034 => "Thumb",
        50035 => "DataColumn",
        50036 => "DataRow",
        50039 => "IPAddress",
        50040 => "Document",
        50042 => "Group",
        50044 => "DataGrid",
        50045 => "DataItem",
        _ => "Unknown",
    }
    .to_string()
}

/// 将元素列表序列化为紧凑文本格式，大幅降低 token 消耗
/// 格式: index|type|name|automation_id|class_name|x,y,w,h|flags
/// flags: ! = 禁用, O = 离屏, * = 有键盘焦点
/// automation_id 和 class_name 为空时省略,进一步压缩
pub fn elements_to_json(elements: &[UiElement]) -> Value {
    let lines: Vec<String> = elements
        .iter()
        .map(|e| {
            // 名称过长时截断，避免单行爆炸
            let name = if e.name.len() > 60 {
                format!("{}...", &e.name[..57])
            } else {
                e.name.clone()
            };
            // automation_id/class_name 为空时省略,减少噪声
            let auto_id = if e.automation_id.is_empty() { "".to_string() } else { e.automation_id.clone() };
            let cls = if e.class_name.is_empty() { "".to_string() } else { e.class_name.clone() };
            // 标志位组合
            let mut flags = String::new();
            if !e.is_enabled { flags.push('!'); }
            if e.is_offscreen { flags.push('O'); }
            if e.has_keyboard_focus { flags.push('*'); }
            format!(
                "{}|{}|{}|{}|{}|{},{},{},{}|{}",
                e.element_index,
                e.control_type,
                name,
                auto_id,
                cls,
                e.bounding_rect.x,
                e.bounding_rect.y,
                e.bounding_rect.width,
                e.bounding_rect.height,
                flags
            )
        })
        .collect();
    Value::String(lines.join("\n"))
}

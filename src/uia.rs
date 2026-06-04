use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
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
    #[serde(rename = "is_enabled")]
    pub is_enabled: bool,
    #[serde(rename = "bounding_rect")]
    pub bounding_rect: Rect,
    #[serde(rename = "has_keyboard_focus")]
    pub has_keyboard_focus: bool,
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
        let walker: IUIAutomationTreeWalker = unsafe {
            automation.ControlViewWalker()?
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

        let current_index = *index;
        *index += 1;

        self.elements.push(UiElement {
            element_index: current_index,
            control_type: control_type_name,
            name: name.to_string(),
            is_enabled,
            bounding_rect: Rect {
                x: rect.left,
                y: rect.top,
                width: rect.right - rect.left,
                height: rect.bottom - rect.top,
            },
            has_keyboard_focus: has_focus,
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

pub fn elements_to_json(elements: &[UiElement]) -> Value {
    let items: Vec<Value> = elements
        .iter()
        .map(|e| {
            json!({
                "element_index": e.element_index,
                "control_type": e.control_type,
                "name": e.name,
                "is_enabled": e.is_enabled,
                "bounding_rect": {
                    "x": e.bounding_rect.x,
                    "y": e.bounding_rect.y,
                    "width": e.bounding_rect.width,
                    "height": e.bounding_rect.height
                },
                "has_keyboard_focus": e.has_keyboard_focus,
            })
        })
        .collect();
    Value::Array(items)
}

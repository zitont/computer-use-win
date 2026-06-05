use crate::log_debug;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;
use windows::Win32::UI::Accessibility::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::WindowsAndMessaging::*;

/// 键盘布局守卫: 切换为英文布局,析构时恢复
struct KeyboardLayoutGuard {
    original_layout: HKL,
}

impl KeyboardLayoutGuard {
    /// 保存当前前台线程的键盘布局,切换到英文(US)避免 IME 干扰
    fn switch_to_english() -> Option<Self> {
        unsafe {
            let foreground_hwnd = GetForegroundWindow();
            let foreground_tid = GetWindowThreadProcessId(foreground_hwnd, None);
            let original_layout = GetKeyboardLayout(foreground_tid);

            // 英文 US 布局 ID = 0x0409
            let english_layout = LoadKeyboardLayoutW(w!("00000409"), KLF_ACTIVATE).ok()?;
            if english_layout == original_layout {
                // 已经是英文布局,无需切换
                return None;
            }
            Some(Self { original_layout })
        }
    }
}

impl Drop for KeyboardLayoutGuard {
    fn drop(&mut self) {
        unsafe {
            let _ = ActivateKeyboardLayout(self.original_layout, KLF_ACTIVATE);
        }
    }
}

pub fn click(x: i32, y: i32, button: &str, click_count: i32) -> Result<()> {
    unsafe {
        // 先激活目标窗口
        let hwnd = WindowFromPoint(POINT { x, y });
        if !hwnd.is_invalid() {
            let _ = SetForegroundWindow(hwnd);
        }

        // 通过 UIA ElementFromPoint + SetFocus 让目标控件获得焦点,
        // 解决微信等应用不响应 SendInput 合成鼠标事件的焦点问题
        set_focus_via_uia_at_point(x, y);

        SetCursorPos(x, y)?;
        for _ in 0..click_count {
            match button {
                "right" => {
                    let inputs = [
                        make_mouse_input(MOUSEEVENTF_RIGHTDOWN, 0, 0),
                        make_mouse_input(MOUSEEVENTF_RIGHTUP, 0, 0),
                    ];
                    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                }
                _ => {
                    let inputs = [
                        make_mouse_input(MOUSEEVENTF_LEFTDOWN, 0, 0),
                        make_mouse_input(MOUSEEVENTF_LEFTUP, 0, 0),
                    ];
                    SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
                }
            }
            if click_count > 1 {
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        }
    }
    Ok(())
}

/// 通过 UIA ElementFromPoint 找到指定坐标处的元素并调用 SetFocus,
/// 使微信等隐藏 UIA 树的应用能正确响应焦点切换
unsafe fn set_focus_via_uia_at_point(x: i32, y: i32) {
    let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

    let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
        Ok(a) => a,
        Err(e) => {
            log_debug(&format!("[CLICK] UIA 初始化失败: {}", e));
            return;
        }
    };

    let point = POINT { x, y };
    let element: IUIAutomationElement = match automation.ElementFromPoint(point) {
        Ok(e) => e,
        Err(e) => {
            log_debug(&format!("[CLICK] ElementFromPoint({}, {}) 失败: {}", x, y, e));
            return;
        }
    };

    let name = element.CurrentName().unwrap_or_default();
    let ctrl_type = element.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0));
    log_debug(&format!("[CLICK] ElementFromPoint({}, {}) -> name='{}', ctrl_type={}", x, y, name, ctrl_type.0));

    match element.SetFocus() {
        Ok(_) => log_debug("[CLICK] SetFocus 成功"),
        Err(e) => log_debug(&format!("[CLICK] SetFocus 失败: {}", e)),
    }
}

pub fn scroll(x: i32, y: i32, delta_x: i32, delta_y: i32) -> Result<()> {
    unsafe {
        SetCursorPos(x, y)?;
        if delta_y != 0 {
            let wheel_delta = delta_y * 120;
            let inputs = [make_mouse_input(MOUSEEVENTF_WHEEL, 0, wheel_delta)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
        if delta_x != 0 {
            let wheel_delta = delta_x * 120;
            let inputs = [make_mouse_input(MOUSEEVENTF_HWHEEL, 0, wheel_delta)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
    Ok(())
}

pub fn drag(
    start_x: i32, start_y: i32, end_x: i32, end_y: i32, button: &str,
) -> Result<()> {
    unsafe {
        SetCursorPos(start_x, start_y)?;
        std::thread::sleep(std::time::Duration::from_millis(50));

        match button {
            "right" => {
                let inputs = [make_mouse_input(MOUSEEVENTF_RIGHTDOWN, 0, 0)];
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
            _ => {
                let inputs = [make_mouse_input(MOUSEEVENTF_LEFTDOWN, 0, 0)];
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
        }

        std::thread::sleep(std::time::Duration::from_millis(50));

        let steps = 10;
        for i in 1..=steps {
            let t = i as f64 / steps as f64;
            let cx = start_x as f64 + (end_x - start_x) as f64 * t;
            let cy = start_y as f64 + (end_y - start_y) as f64 * t;
            SetCursorPos(cx as i32, cy as i32)?;
            std::thread::sleep(std::time::Duration::from_millis(10));
        }

        std::thread::sleep(std::time::Duration::from_millis(50));

        match button {
            "right" => {
                let inputs = [make_mouse_input(MOUSEEVENTF_RIGHTUP, 0, 0)];
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
            _ => {
                let inputs = [make_mouse_input(MOUSEEVENTF_LEFTUP, 0, 0)];
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
        }
    }
    Ok(())
}

pub fn type_text(text: &str, use_unicode: bool) -> Result<()> {
    log_debug(&format!("[TYPE] type_text('{}', unicode={})", text, use_unicode));
    // 优先级 1: UIA ValuePattern.SetValue (绕过 UIPI,直接写入控件值)
    if type_text_via_uia_value(text) {
        log_debug("[TYPE] 路径 1: UIA ValuePattern 成功");
        return Ok(());
    }
    log_debug("[TYPE] 路径 1: UIA ValuePattern 失败,尝试路径 2");
    // 优先级 2: Win32 WM_SETTEXT (标准 Edit 控件)
    if type_text_via_window_message(text) {
        log_debug("[TYPE] 路径 2: WM_SETTEXT 成功");
        return Ok(());
    }
    log_debug("[TYPE] 路径 2: WM_SETTEXT 失败,尝试路径 3");
    // 优先级 3: UIA 焦点激活 + 键盘事件注入 (最后手段)
    focus_active_element();
    unsafe {
        let hwnd = GetForegroundWindow();
        if !hwnd.is_invalid() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
    std::thread::sleep(std::time::Duration::from_millis(50));
    if use_unicode {
        log_debug("[TYPE] 路径 3: unicode 键盘注入");
        type_text_unicode(text)
    } else {
        log_debug("[TYPE] 路径 3: clipboard+Ctrl+V 注入");
        type_text_clipboard(text)
    }
}

/// 通过 UIA ValuePattern.SetValue 直接写入文本到控件,
/// 绕过 UIPI 限制且无需键盘事件,适用于微信等自定义控件
fn type_text_via_uia_value(text: &str) -> bool {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
            Ok(a) => a,
            Err(_) => return false,
        };

        // 策略 1: 直接获取焦点元素的 ValuePattern
        // 必须验证焦点元素属于前台窗口,避免写入 QoderWork 等后台窗口
        if let Ok(focused) = automation.GetFocusedElement() {
            let focused_hwnd = focused.CurrentNativeWindowHandle().ok();
            let fg_hwnd = GetForegroundWindow();
            if let Some(fh) = focused_hwnd {
                if fh == fg_hwnd {
                    if set_value_via_pattern(&automation, &focused, text) {
                        return true;
                    }
                }
            }
        }

        // 策略 2: 在前台窗口中搜索 Edit 控件
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }
        let root: IUIAutomationElement = match automation.ElementFromHandle(hwnd) {
            Ok(e) => e,
            Err(_) => return false,
        };

        // 用 ControlViewWalker 遍历查找可写入的控件
        let walker: IUIAutomationTreeWalker = match automation.ControlViewWalker() {
            Ok(w) => w,
            Err(_) => return false,
        };

        search_and_set_value(&automation, &walker, &root, text)
    }
}

/// 尝试对单个元素获取 ValuePattern 并 SetValue
unsafe fn set_value_via_pattern(
    _automation: &IUIAutomation,
    element: &IUIAutomationElement,
    text: &str,
) -> bool {
    // 只对 Edit 类型控件尝试写入,避免写入 Pane 等非输入控件
    let control_type = element.CurrentControlType().unwrap_or(UIA_CONTROLTYPE_ID(0));
    if control_type.0 != 50004 {
        return false;
    }

    // 尝试获取 ValuePattern (pattern ID = 10002)
    let pattern_id = UIA_PATTERN_ID(10002);
    let unknown = match element.GetCurrentPattern(pattern_id) {
        Ok(u) => u,
        Err(_) => return false,
    };
    let value_pattern: IUIAutomationValuePattern = match unknown.cast() {
        Ok(p) => p,
        Err(_) => return false,
    };

    // 检查是否只读
    let is_read_only = match value_pattern.CurrentIsReadOnly() {
        Ok(v) => v.as_bool(),
        Err(_) => return false,
    };
    if is_read_only {
        return false;
    }

    // SetValue: 直接写入文本
    let bstr_text = BSTR::from(text);
    value_pattern.SetValue(&bstr_text).is_ok()
}

/// 递归遍历 UIA 树,查找第一个可写入 ValuePattern 的元素并写入文本
unsafe fn search_and_set_value(
    automation: &IUIAutomation,
    walker: &IUIAutomationTreeWalker,
    element: &IUIAutomationElement,
    text: &str,
) -> bool {
    // 先尝试当前元素
    if set_value_via_pattern(automation, element, text) {
        return true;
    }

    // 递归子元素
    if let Ok(child) = walker.GetFirstChildElement(element) {
        let mut current = child;
        loop {
            if search_and_set_value(automation, walker, &current, text) {
                return true;
            }
            match walker.GetNextSiblingElement(&current) {
                Ok(next) => current = next,
                Err(_) => break,
            }
        }
    }

    false
}

/// 通过 UIA 获取前台窗口的焦点元素并调用 SetFocus,
/// 确保键盘事件发送到正确的输入控件
fn focus_active_element() {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return;
        }
        let automation: IUIAutomation = match CoCreateInstance(&CUIAutomation, None, CLSCTX_INPROC_SERVER) {
            Ok(a) => a,
            Err(_) => return,
        };
        let focused: IUIAutomationElement = match automation.GetFocusedElement() {
            Ok(e) => e,
            Err(_) => return,
        };
        let _ = focused.SetFocus();
    }
}

/// 通过 Win32 API 查找前台窗口中的输入框并写入文本,
/// 绕过 UIPI 限制 (不依赖 SendInput 键盘注入)
fn type_text_via_window_message(text: &str) -> bool {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_invalid() {
            return false;
        }

        // 递归查找子窗口中的 Edit 控件 (类名 RichEdit20W / RICHEDIT50W / Edit)
        let edit_hwnd = find_edit_child(hwnd);
        if edit_hwnd.is_invalid() {
            return false;
        }

        // 用 WM_SETTEXT 直接写入文本,无需键盘事件
        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let lparam = LPARAM(wide.as_ptr() as isize);
        let result = SendMessageW(edit_hwnd, WM_SETTEXT, None, Some(lparam));
        result != LRESULT(0)
    }
}

/// 递归查找子窗口中的 Edit 类控件
unsafe fn find_edit_child(parent: HWND) -> HWND {
    // 常见的 Edit 控件类名
    let edit_classes: Vec<Vec<u16>> = [
        "RichEdit20W",
        "RICHEDIT50W",
        "Edit",
    ]
    .iter()
    .map(|s| s.encode_utf16().chain(std::iter::once(0)).collect())
    .collect();

    let mut child = match FindWindowExW(Some(parent), None, None, None) {
        Ok(h) => h,
        Err(_) => return HWND(std::ptr::null_mut()),
    };

    loop {
        // 检查当前窗口是否为 Edit 类
        let mut class_buf = [0u16; 128];
        let len = GetClassNameW(child, &mut class_buf);
        if len > 0 {
            let class_name: Vec<u16> = class_buf[..len as usize].to_vec();
            for edit_class in &edit_classes {
                if class_name == edit_class.as_slice() {
                    return child;
                }
            }
        }

        // 递归搜索子窗口
        let found = find_edit_child(child);
        if !found.is_invalid() {
            return found;
        }

        child = match FindWindowExW(Some(parent), Some(child), None, None) {
            Ok(h) => h,
            Err(_) => break,
        };
    }

    HWND(std::ptr::null_mut())
}

pub fn type_text_clipboard(text: &str) -> Result<()> {
    let original = get_clipboard_text();
    set_clipboard_text(text)?;
    std::thread::sleep(std::time::Duration::from_millis(50));

    unsafe {
        let inputs = [
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(VK_CONTROL.0),
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x56),
                        dwFlags: KEYBD_EVENT_FLAGS(0),
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(0x56),
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
            INPUT {
                r#type: INPUT_KEYBOARD,
                Anonymous: INPUT_0 {
                    ki: KEYBDINPUT {
                        wVk: VIRTUAL_KEY(VK_CONTROL.0),
                        dwFlags: KEYEVENTF_KEYUP,
                        ..Default::default()
                    },
                },
            },
        ];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
    }

    std::thread::sleep(std::time::Duration::from_millis(50));
    if let Some(original) = original {
        let _ = set_clipboard_text(&original);
    }
    Ok(())
}

fn type_text_unicode(text: &str) -> Result<()> {
    for ch in text.chars() {
        let mut buf = [0u16; 2];
        let len = ch.encode_utf16(&mut buf).len();
        for i in 0..len {
            let code_unit = buf[i];
            unsafe {
                let inputs = [
                    INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: VIRTUAL_KEY(0),
                                wScan: code_unit,
                                dwFlags: KEYEVENTF_UNICODE,
                                ..Default::default()
                            },
                        },
                    },
                    INPUT {
                        r#type: INPUT_KEYBOARD,
                        Anonymous: INPUT_0 {
                            ki: KEYBDINPUT {
                                wVk: VIRTUAL_KEY(0),
                                wScan: code_unit,
                                dwFlags: KEYEVENTF_UNICODE | KEYEVENTF_KEYUP,
                                ..Default::default()
                            },
                        },
                    },
                ];
                SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
            }
        }
    }
    Ok(())
}

pub fn press_key(key: &str) -> Result<()> {
    let (modifiers, vk) = parse_key_expression(key)?;

    // 普通字母键切换英文布局避免 IME 干扰;修饰键组合不需要
    let _guard = if modifiers.is_empty() {
        KeyboardLayoutGuard::switch_to_english()
    } else {
        None
    };

    unsafe {
        for &modifier in &modifiers {
            let inputs = [make_keybd_input(modifier, KEYBD_EVENT_FLAGS(0))];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }

        let inputs = [
            make_keybd_input(vk, KEYBD_EVENT_FLAGS(0)),
            make_keybd_input(vk, KEYEVENTF_KEYUP),
        ];
        SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);

        for &modifier in modifiers.iter().rev() {
            let inputs = [make_keybd_input(modifier, KEYEVENTF_KEYUP)];
            SendInput(&inputs, std::mem::size_of::<INPUT>() as i32);
        }
    }
    Ok(())
}

fn parse_key_expression(key: &str) -> Result<(Vec<VIRTUAL_KEY>, VIRTUAL_KEY)> {
    let parts: Vec<String> = key.split('+').map(|s| s.trim().to_lowercase()).collect();
    let mut modifiers = Vec::new();
    let mut main_key = VIRTUAL_KEY(0);

    for part in &parts {
        match part.as_str() {
            "ctrl" | "control" => modifiers.push(VIRTUAL_KEY(VK_CONTROL.0)),
            "alt" => modifiers.push(VIRTUAL_KEY(VK_MENU.0)),
            "shift" => modifiers.push(VIRTUAL_KEY(VK_SHIFT.0)),
            "win" | "lwin" | "rwin" => modifiers.push(VIRTUAL_KEY(VK_LWIN.0)),
            "enter" | "return" => main_key = VIRTUAL_KEY(VK_RETURN.0),
            "escape" | "esc" => main_key = VIRTUAL_KEY(VK_ESCAPE.0),
            "tab" => main_key = VIRTUAL_KEY(VK_TAB.0),
            "space" => main_key = VIRTUAL_KEY(VK_SPACE.0),
            "backspace" => main_key = VIRTUAL_KEY(VK_BACK.0),
            "delete" | "del" => main_key = VIRTUAL_KEY(VK_DELETE.0),
            "home" => main_key = VIRTUAL_KEY(VK_HOME.0),
            "end" => main_key = VIRTUAL_KEY(VK_END.0),
            "pageup" | "pgup" => main_key = VIRTUAL_KEY(VK_PRIOR.0),
            "pagedown" | "pgdn" => main_key = VIRTUAL_KEY(VK_NEXT.0),
            "up" => main_key = VIRTUAL_KEY(VK_UP.0),
            "down" => main_key = VIRTUAL_KEY(VK_DOWN.0),
            "left" => main_key = VIRTUAL_KEY(VK_LEFT.0),
            "right" => main_key = VIRTUAL_KEY(VK_RIGHT.0),
            "f1" => main_key = VIRTUAL_KEY(VK_F1.0),
            "f2" => main_key = VIRTUAL_KEY(VK_F2.0),
            "f3" => main_key = VIRTUAL_KEY(VK_F3.0),
            "f4" => main_key = VIRTUAL_KEY(VK_F4.0),
            "f5" => main_key = VIRTUAL_KEY(VK_F5.0),
            "f6" => main_key = VIRTUAL_KEY(VK_F6.0),
            "f7" => main_key = VIRTUAL_KEY(VK_F7.0),
            "f8" => main_key = VIRTUAL_KEY(VK_F8.0),
            "f9" => main_key = VIRTUAL_KEY(VK_F9.0),
            "f10" => main_key = VIRTUAL_KEY(VK_F10.0),
            "f11" => main_key = VIRTUAL_KEY(VK_F11.0),
            "f12" => main_key = VIRTUAL_KEY(VK_F12.0),
            "printscreen" | "prtsc" => main_key = VIRTUAL_KEY(VK_SNAPSHOT.0),
            "insert" | "ins" => main_key = VIRTUAL_KEY(VK_INSERT.0),
            "capslock" => main_key = VIRTUAL_KEY(VK_CAPITAL.0),
            "numlock" => main_key = VIRTUAL_KEY(VK_NUMLOCK.0),
            _ => {
                if part.len() == 1 {
                    let ch = part.chars().next().unwrap() as u16;
                    // 0x30-0x39 数字, 0x41-0x5A 大写, 0x61-0x7A 小写
                    if (0x30..=0x39).contains(&ch)
                        || (0x41..=0x5A).contains(&ch)
                        || (0x61..=0x7A).contains(&ch)
                    {
                        // 小写转大写: 减去 'a'-'A' 的差值 0x20
                        let vk = if (0x61..=0x7A).contains(&ch) { ch - 0x20 } else { ch };
                        main_key = VIRTUAL_KEY(vk);
                    } else {
                        return Err(E_INVALIDARG.into());
                    }
                } else {
                    return Err(E_INVALIDARG.into());
                }
            }
        }
    }

    if main_key.0 == 0 && modifiers.is_empty() {
        return Err(E_INVALIDARG.into());
    }
    Ok((modifiers, main_key))
}

unsafe fn make_mouse_input(flags: MOUSE_EVENT_FLAGS, dx: i32, dy: i32) -> INPUT {
    INPUT {
        r#type: INPUT_MOUSE,
        Anonymous: INPUT_0 {
            mi: MOUSEINPUT {
                dx,
                dy,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

unsafe fn make_keybd_input(vk: VIRTUAL_KEY, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

pub fn set_clipboard_text(text: &str) -> Result<()> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return Err(E_FAIL.into());
        }

        let _ = EmptyClipboard();

        let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
        let size = wide.len() * 2;
        let h_mem = GlobalAlloc(GMEM_MOVEABLE, size)?;
        let ptr = GlobalLock(h_mem) as *mut u16;
        std::ptr::copy_nonoverlapping(wide.as_ptr(), ptr, wide.len());
        let _ = GlobalUnlock(h_mem);

        let _ = SetClipboardData(13, Some(HANDLE(h_mem.0)));
        let _ = CloseClipboard();
    }
    Ok(())
}

pub fn get_clipboard_text() -> Option<String> {
    unsafe {
        if OpenClipboard(None).is_err() {
            return None;
        }

        let result = GetClipboardData(13)
            .ok()
            .and_then(|h_data| {
                let ptr = GlobalLock(HGLOBAL(h_data.0)) as *const u16;
                if ptr.is_null() {
                    return None;
                }
                let mut len = 0;
                while *ptr.add(len) != 0 {
                    len += 1;
                }
                let slice = std::slice::from_raw_parts(ptr, len);
                let text = String::from_utf16_lossy(slice);
                let _ = GlobalUnlock(HGLOBAL(h_data.0));
                Some(text)
            });

        let _ = CloseClipboard();
        result
    }
}

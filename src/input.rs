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
        // 先激活目标窗口再发送鼠标事件
        let hwnd = WindowFromPoint(POINT { x, y });
        if !hwnd.is_invalid() {
            let _ = SetForegroundWindow(hwnd);
        }
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
    // 1. 通过 UIA 尝试聚焦当前焦点元素 (部分应用有效)
    focus_active_element();
    // 2. 确保前台窗口激活 (覆盖 UIA 失败的情况)
    unsafe {
        let hwnd = GetForegroundWindow();
        if !hwnd.is_invalid() {
            let _ = SetForegroundWindow(hwnd);
        }
    }
    // 等待窗口激活生效,避免键盘事件丢失
    std::thread::sleep(std::time::Duration::from_millis(50));
    if use_unicode {
        type_text_unicode(text)
    } else {
        type_text_clipboard(text)
    }
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

fn type_text_clipboard(text: &str) -> Result<()> {
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

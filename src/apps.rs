use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::System::Registry::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct InstalledApp {
    pub name: String,
    pub aumid: String,
}

pub fn list_installed_apps(filter: Option<&str>) -> Result<Vec<InstalledApp>> {
    let mut apps = Vec::new();

    let paths = [
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Uninstall",
        r"SOFTWARE\WOW6432Node\Microsoft\Windows\CurrentVersion\Uninstall",
    ];

    for path in &paths {
        let _ = enumerate_registry_apps(path, filter, &mut apps);
    }

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps.dedup_by(|a, b| a.name == b.name);

    Ok(apps)
}

fn enumerate_registry_apps(
    path: &str,
    filter: Option<&str>,
    apps: &mut Vec<InstalledApp>,
) -> Result<()> {
    unsafe {
        let wide_path: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();

        let mut h_key = HKEY::default();
        let result = RegOpenKeyExW(
            HKEY_LOCAL_MACHINE,
            PCWSTR(wide_path.as_ptr()),
            Some(0),
            KEY_READ,
            &mut h_key,
        );

        if result.is_err() {
            return Err(result.into());
        }

        let mut index = 0u32;
        let mut name_buf = [0u16; 256];
        let mut name_len: u32;

        loop {
            name_len = 256;
            let result = RegEnumKeyExW(
                h_key,
                index,
                Some(PWSTR(name_buf.as_mut_ptr())),
                &mut name_len,
                None,
                None,
                None,
                None,
            );

            if result.is_err() {
                break;
            }

            let subkey_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            let display_name = read_reg_value_string(h_key, &subkey_name, "DisplayName");

            if let Some(name) = display_name {
                let should_include = match filter {
                    Some(f) => name.to_lowercase().contains(&f.to_lowercase()),
                    None => true,
                };

                if should_include && !name.is_empty() {
                    let aumid = format!("{}_app", name.replace(' ', "_"));
                    apps.push(InstalledApp { name, aumid });
                }
            }

            index += 1;
            if index > 1000 {
                break;
            }
        }

        let _ = RegCloseKey(h_key);
    }
    Ok(())
}

unsafe fn read_reg_value_string(parent_key: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    let wide_subkey: Vec<u16> = subkey.encode_utf16().chain(std::iter::once(0)).collect();
    let wide_value: Vec<u16> = value_name.encode_utf16().chain(std::iter::once(0)).collect();

    let mut h_subkey = HKEY::default();
    let result = RegOpenKeyExW(
        parent_key,
        PCWSTR(wide_subkey.as_ptr()),
        Some(0),
        KEY_READ,
        &mut h_subkey,
    );

    if result.is_err() {
        return None;
    }

    let mut buf = [0u16; 512];
    let mut buf_size = (buf.len() * 2) as u32;
    let mut reg_type = REG_VALUE_TYPE(0);

    let result = RegQueryValueExW(
        h_subkey,
        PCWSTR(wide_value.as_ptr()),
        None,
        Some(&mut reg_type),
        Some(buf.as_mut_ptr() as *mut u8),
        Some(&mut buf_size),
    );

    let _ = RegCloseKey(h_subkey);

    if result.is_ok() && reg_type == REG_SZ {
        let char_count = (buf_size / 2) as usize;
        Some(String::from_utf16_lossy(&buf[..char_count]).trim().to_string())
    } else {
        None
    }
}

pub fn launch_app(aumid: &str) -> Result<()> {
    unsafe {
        let shell_cmd = format!("shell:AppsFolder\\{}", aumid);
        let wide_cmd: Vec<u16> = shell_cmd.encode_utf16().chain(std::iter::once(0)).collect();

        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(wide_cmd.as_ptr()),
            None,
            None,
            SW_SHOWNORMAL,
        );
    }
    Ok(())
}

pub fn get_window_title(hwnd: HWND) -> String {
    unsafe {
        let mut title = [0u16; 512];
        let len = GetWindowTextW(hwnd, &mut title);
        if len > 0 {
            String::from_utf16_lossy(&title[..len as usize])
        } else {
            String::new()
        }
    }
}

pub fn get_window_process_name(hwnd: HWND) -> String {
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));

        let process_handle = OpenProcess(
            PROCESS_QUERY_INFORMATION | PROCESS_VM_READ,
            false,
            pid,
        );

        if let Ok(handle) = process_handle {
            let mut name = [0u16; 260];
            let mut size = name.len() as u32;
            let result = QueryFullProcessImageNameW(
                handle,
                PROCESS_NAME_FORMAT(0),
                PWSTR(name.as_mut_ptr()),
                &mut size,
            );
            let _ = CloseHandle(handle);

            if result.is_ok() {
                let path = String::from_utf16_lossy(&name[..size as usize]);
                path.rsplit('\\').next().unwrap_or(&path).to_string()
            } else {
                String::new()
            }
        } else {
            String::new()
        }
    }
}

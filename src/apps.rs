use serde::Deserialize;
use std::process::Command;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::System::Threading::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub struct InstalledApp {
    pub name: String,
    pub aumid: String,
}

#[derive(Deserialize)]
struct StartAppEntry {
    #[serde(rename = "Name")]
    name: String,
    #[serde(rename = "AppID")]
    app_id: String,
}

/// 通过 PowerShell Get-StartApps 列出已安装应用,获取真实 AUMID / 路径
pub fn list_installed_apps(filter: Option<&str>) -> std::result::Result<Vec<InstalledApp>, String> {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "Get-StartApps | Select-Object Name, AppID | ConvertTo-Json -Compress",
        ])
        .output()
        .map_err(|e| format!("执行 PowerShell 失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("PowerShell 执行失败: {}", stderr));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);
    let entries: Vec<StartAppEntry> = serde_json::from_str(&json_str)
        .map_err(|e| format!("解析 JSON 失败: {}", e))?;

    let mut apps: Vec<InstalledApp> = entries
        .into_iter()
        .filter(|entry| {
            if let Some(keyword) = filter {
                entry.name.to_lowercase().contains(&keyword.to_lowercase())
            } else {
                true
            }
        })
        .filter(|entry| !entry.name.is_empty())
        .map(|entry| InstalledApp {
            name: entry.name,
            aumid: entry.app_id,
        })
        .collect();

    apps.sort_by(|a, b| a.name.cmp(&b.name));
    apps.dedup_by(|a, b| a.name == b.name);

    Ok(apps)
}

/// 通过 AUMID 启动应用程序
pub fn launch_app(aumid: &str) -> Result<()> {
    unsafe {
        let shell_cmd = format!("shell:AppsFolder\\{}", aumid);
        let wide_cmd: Vec<u16> = shell_cmd
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();

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

        let process_handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid);

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

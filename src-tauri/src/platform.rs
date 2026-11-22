#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    #[link(name = "kernel32")]
    extern "system" {
        fn CreateMutexW(attributes: *mut c_void, owner: i32, name: *const u16) -> *mut c_void;
        fn GetLastError() -> u32;
    }

    #[link(name = "user32")]
    extern "system" {
        fn FindWindowW(class_name: *const u16, window_name: *const u16) -> isize;
        fn ShowWindow(hwnd: isize, cmd: i32) -> i32;
        fn SetForegroundWindow(hwnd: isize) -> i32;
    }

    pub fn single_instance() -> bool {
        let name: Vec<u16> = "Local\\StarDrop.SingleInstance"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let handle = unsafe { CreateMutexW(std::ptr::null_mut(), 1, name.as_ptr()) };
        if handle.is_null() {
            return true;
        }
        unsafe { GetLastError() != 183 }
    }

    pub fn focus_existing_window() {
        let title: Vec<u16> = "星传 StarDrop".encode_utf16().chain(std::iter::once(0)).collect();
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if hwnd != 0 {
            unsafe {
                ShowWindow(hwnd, 9);
                SetForegroundWindow(hwnd);
            }
        }
    }

    pub fn prepare_webview_data() {
        if std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_some() {
            return;
        }
        let dir = match std::env::var_os("STARDROP_WEBVIEW_DIR") {
            Some(custom) => std::path::PathBuf::from(custom),
            None => {
                let base = std::env::var("LOCALAPPDATA").unwrap_or_default();
                if base.is_empty() {
                    return;
                }
                std::path::Path::new(&base).join("StarDrop").join("WebView2")
            }
        };
        if std::fs::create_dir_all(&dir).is_ok() {
            std::env::set_var("WEBVIEW2_USER_DATA_FOLDER", dir);
        }
    }

    pub fn open_path(path: &str) -> Result<(), String> {
        if path.trim().is_empty() {
            return Err("没有可打开的文件".into());
        }
        Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn reveal_path(path: &str) -> Result<(), String> {
        if path.trim().is_empty() {
            return Err("没有文件位置".into());
        }
        Command::new("explorer.exe")
            .raw_arg(format!("/select,\"{}\"", path))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn open_folder(path: &str) -> Result<(), String> {
        Command::new("explorer.exe")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn copy_text(text: &str) -> Result<(), String> {
        let encoded = crate::util::base64(text.as_bytes());
        let script = format!(
            "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}')))",
            encoded
        );
        let status = Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| e.to_string())?;
        if status.success() {
            Ok(())
        } else {
            Err("复制失败".into())
        }
    }

    pub fn open_firewall() -> Result<String, String> {
        let exe = std::env::current_exe().map_err(|e| e.to_string())?;
        let exe_text = exe.to_string_lossy().replace('\'', "''");
        let netsh_args = format!(
            "advfirewall firewall add rule name=\"StarDrop\" dir=in action=allow program=\"{}\" enable=yes profile=any",
            exe_text
        );
        let script = format!(
            "Start-Process -FilePath netsh.exe -Verb RunAs -WindowStyle Hidden -ArgumentList '{}'",
            netsh_args.replace('\'', "''")
        );
        Command::new("powershell.exe")
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-WindowStyle",
                "Hidden",
                "-Command",
                &script,
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok("已弹出授权窗口，允许后手机即可访问".into())
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io::Write;
    use std::process::{Command, Stdio};

    pub fn single_instance() -> bool {
        true
    }

    pub fn focus_existing_window() {}

    pub fn prepare_webview_data() {}

    fn opener() -> &'static str {
        if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        }
    }

    pub fn open_path(path: &str) -> Result<(), String> {
        if path.trim().is_empty() {
            return Err("没有可打开的文件".into());
        }
        Command::new(opener())
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    pub fn reveal_path(path: &str) -> Result<(), String> {
        if path.trim().is_empty() {
            return Err("没有文件位置".into());
        }
        #[cfg(target_os = "macos")]
        {
            return Command::new("open")
                .args(["-R", path])
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
        #[cfg(not(target_os = "macos"))]
        {
            let parent = std::path::Path::new(path)
                .parent()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|| path.to_string());
            Command::new("xdg-open")
                .arg(parent)
                .spawn()
                .map(|_| ())
                .map_err(|e| e.to_string())
        }
    }

    pub fn open_folder(path: &str) -> Result<(), String> {
        open_path(path)
    }

    pub fn copy_text(text: &str) -> Result<(), String> {
        #[cfg(target_os = "macos")]
        {
            let mut child = Command::new("pbcopy")
                .stdin(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            if let Some(mut stdin) = child.stdin.take() {
                stdin.write_all(text.as_bytes()).map_err(|e| e.to_string())?;
            }
            return match child.wait() {
                Ok(status) if status.success() => Ok(()),
                _ => Err("复制失败".into()),
            };
        }
        #[cfg(not(target_os = "macos"))]
        {
            let candidates: [(&str, &[&str]); 3] = [
                ("wl-copy", &[]),
                ("xclip", &["-selection", "clipboard"]),
                ("xsel", &["--clipboard", "--input"]),
            ];
            for (program, args) in candidates {
                let Ok(mut child) = Command::new(program)
                    .args(args)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
                else {
                    continue;
                };
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(text.as_bytes());
                }
                if child.wait().map(|s| s.success()).unwrap_or(false) {
                    return Ok(());
                }
            }
            Err("系统里没有可用的剪贴板工具（xclip / xsel / wl-copy）".into())
        }
    }

    pub fn open_firewall() -> Result<String, String> {
        Err("macOS / Linux 不需要配置 Windows 防火墙".into())
    }
}

pub use imp::*;

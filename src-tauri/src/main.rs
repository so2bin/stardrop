#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod model;
mod server;
mod util;

use model::{Item, Shared};
use std::ffi::c_void;
use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Arc;
use tauri::{LogicalSize, Manager, State};

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

fn single_instance() -> bool {
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

fn focus_existing_window() {
    let title: Vec<u16> = "星传 StarDrop".encode_utf16().chain(std::iter::once(0)).collect();
    let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
    if hwnd != 0 {
        unsafe {
            ShowWindow(hwnd, 9);
            SetForegroundWindow(hwnd);
        }
    }
}

fn prepare_webview_data() {
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

fn fit_window(window: &tauri::WebviewWindow) {
    if let Ok(Some(monitor)) = window.current_monitor() {
        let scale = monitor.scale_factor();
        let logical = monitor.size().to_logical::<f64>(scale);
        let width = (logical.width - 110.0).clamp(880.0, 1140.0);
        let height = (logical.height - 110.0).clamp(560.0, 780.0);
        let _ = window.set_size(LogicalSize::new(width, height));
        let _ = window.center();
    }
    let _ = window.show();
}

#[tauri::command]
fn get_state(shared: State<'_, Arc<Shared>>) -> serde_json::Value {
    shared.state_json()
}

#[tauri::command]
fn refresh_net(shared: State<'_, Arc<Shared>>) -> serde_json::Value {
    let ips = util::lan_ips();
    serde_json::json!({
        "urls": shared.urls_from(&ips),
        "lan": ips.first(),
        "port": shared.port
    })
}

#[tauri::command]
fn add_paths(paths: Vec<String>, shared: State<'_, Arc<Shared>>) -> Vec<Item> {
    let mut added = Vec::new();
    for p in paths {
        let path = std::path::PathBuf::from(&p);
        if !path.is_file() {
            continue;
        }
        let duplicate = {
            let outbox = shared.outbox.lock().unwrap();
            outbox.iter().any(|item| item.path.eq_ignore_ascii_case(&p))
        };
        if duplicate {
            continue;
        }
        let item = shared.item_from_path(path.as_path(), "desktop");
        shared.outbox.lock().unwrap().insert(0, item.clone());
        added.push(item);
    }
    if !added.is_empty() {
        shared.emit("outbox://change", true);
    }
    added
}

#[tauri::command]
fn remove_item(id: String, list: String, shared: State<'_, Arc<Shared>>) -> bool {
    let mut guard = if list == "inbox" {
        shared.inbox.lock().unwrap()
    } else {
        shared.outbox.lock().unwrap()
    };
    let before = guard.len();
    guard.retain(|item| item.id != id);
    let changed = guard.len() != before;
    drop(guard);
    if changed {
        shared.emit(if list == "inbox" { "inbox://change" } else { "outbox://change" }, true);
    }
    changed
}

#[tauri::command]
fn clear_list(list: String, shared: State<'_, Arc<Shared>>) -> bool {
    if list == "inbox" {
        shared.inbox.lock().unwrap().clear();
        shared.emit("inbox://change", true);
    } else {
        shared.outbox.lock().unwrap().clear();
        shared.emit("outbox://change", true);
    }
    true
}

#[tauri::command]
fn open_path(path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("没有可打开的文件".into());
    }
    Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("没有文件位置".into());
    }
    Command::new("explorer.exe")
        .raw_arg(format!("/select,\"{}\"", path))
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    Command::new("explorer.exe")
        .arg(&path)
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    let encoded = util::base64(text.as_bytes());
    let script = format!(
        "Set-Clipboard -Value ([Text.Encoding]::UTF8.GetString([Convert]::FromBase64String('{}')))",
        encoded
    );
    let status = Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err("复制失败".into())
    }
}

#[tauri::command]
fn open_firewall(shared: State<'_, Arc<Shared>>) -> Result<String, String> {
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
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| e.to_string())?;
    let _ = shared;
    Ok("已弹出授权窗口，允许后手机即可访问".into())
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    if !single_instance() {
        focus_existing_window();
        return;
    }
    prepare_webview_data();
    let shared = Shared::boot();
    if std::env::var("STARDROP_HEADLESS").ok().as_deref() == Some("1") {
        loop {
            std::thread::sleep(std::time::Duration::from_secs(3600));
        }
    }
    tauri::Builder::default()
        .manage(shared.clone())
        .setup(move |app| {
            shared.set_app(app.handle().clone());
            if let Some(window) = app.get_webview_window("main") {
                fit_window(&window);
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            refresh_net,
            add_paths,
            remove_item,
            clear_list,
            open_path,
            reveal_path,
            open_folder,
            copy_text,
            open_firewall,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("StarDrop failed to start");
}

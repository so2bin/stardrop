#![cfg_attr(all(not(debug_assertions), windows), windows_subsystem = "windows")]

mod model;
mod platform;
mod server;
mod util;

use model::{Item, Shared};
use std::sync::Arc;
use tauri::{LogicalSize, Manager, State};

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
    platform::open_path(&path)
}

#[tauri::command]
fn reveal_path(path: String) -> Result<(), String> {
    platform::reveal_path(&path)
}

#[tauri::command]
fn open_folder(path: String) -> Result<(), String> {
    platform::open_folder(&path)
}

#[tauri::command]
fn copy_text(text: String) -> Result<(), String> {
    platform::copy_text(&text)
}

#[tauri::command]
fn open_firewall() -> Result<String, String> {
    platform::open_firewall()
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

fn main() {
    if !platform::single_instance() {
        platform::focus_existing_window();
        return;
    }
    platform::prepare_webview_data();

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

use crate::util::{ext_of, hhmm, kind_of, lan_ips, today};
use serde::Serialize;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

pub const PORT_BASE: u16 = 47823;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    pub kind: String,
    pub ext: String,
    pub at: String,
    pub text: Option<String>,
    pub source: String,
}

pub struct Shared {
    pub pin: String,
    pub port: u16,
    pub device: String,
    pub save_dir: PathBuf,
    pub send_dir: PathBuf,
    pub started: String,
    pub inbox: Mutex<Vec<Item>>,
    pub outbox: Mutex<Vec<Item>>,
    pub thumbs: Mutex<HashMap<String, Vec<u8>>>,
    pub last_seen: Mutex<Option<Instant>>,
    counter: AtomicU64,
    app: Mutex<Option<AppHandle>>,
}

impl Shared {
    pub fn boot() -> Arc<Shared> {
        let home = std::env::var("USERPROFILE")
            .or_else(|_| std::env::var("HOME"))
            .unwrap_or_else(|_| ".".to_string());
        let save_dir = match std::env::var("STARDROP_DIR") {
            Ok(dir) if !dir.trim().is_empty() => PathBuf::from(dir),
            _ => Path::new(&home).join("Pictures").join("星传"),
        };
        let send_dir = save_dir.join(".outbox");
        let _ = fs::create_dir_all(&save_dir);
        let _ = fs::create_dir_all(&send_dir);

        let device = std::env::var("COMPUTERNAME")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "我的电脑".to_string());

        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(1)
            ^ (std::process::id() as u64).wrapping_mul(2654435761);

        let saved = read_config();
        let pin = std::env::var("STARDROP_PIN")
            .ok()
            .filter(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_digit()))
            .or_else(|| saved.as_ref().and_then(|c| c.0.clone()))
            .unwrap_or_else(|| format!("{:04}", 1000 + (seed % 9000)));

        let preferred = std::env::var("STARDROP_PORT")
            .ok()
            .and_then(|v| v.parse::<u16>().ok())
            .or_else(|| saved.as_ref().and_then(|c| c.1))
            .unwrap_or(PORT_BASE);
        let (listener, port) = bind_listener(preferred);
        write_config(&pin, port);

        let shared = Arc::new(Shared {
            pin,
            port,
            device,
            save_dir,
            send_dir,
            started: hhmm(),
            inbox: Mutex::new(Vec::new()),
            outbox: Mutex::new(Vec::new()),
            thumbs: Mutex::new(HashMap::new()),
            last_seen: Mutex::new(None),
            counter: AtomicU64::new(0),
            app: Mutex::new(None),
        });

        shared.load_today();
        if let Some(listener) = listener {
            crate::server::spawn_listener(listener, shared.clone());
        }
        shared
    }

    pub fn set_app(&self, handle: AppHandle) {
        *self.app.lock().unwrap() = Some(handle);
    }

    pub fn emit<S: Serialize + Clone>(&self, event: &str, payload: S) {
        let handle = self.app.lock().unwrap().clone();
        if let Some(app) = handle {
            let _ = app.emit(event, payload);
        }
    }

    pub fn next_id(&self) -> String {
        let n = self.counter.fetch_add(1, Ordering::SeqCst);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0);
        format!("{millis:x}{n:x}")
    }

    pub fn ips(&self) -> Vec<String> {
        lan_ips()
    }

    pub fn urls_from(&self, ips: &[String]) -> Vec<String> {
        ips.iter()
            .map(|ip| format!("http://{}:{}/?k={}", ip, self.port, self.pin))
            .collect()
    }

    pub fn item_from_path(&self, path: &Path, source: &str) -> Item {
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("file")
            .to_string();
        let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        Item {
            id: self.next_id(),
            ext: ext_of(&name),
            kind: kind_of(&name).to_string(),
            name,
            path: path.to_string_lossy().to_string(),
            size,
            at: hhmm(),
            text: None,
            source: source.to_string(),
        }
    }

    pub fn text_item(&self, text: String, source: &str) -> Item {
        let preview: String = text.chars().take(40).collect();
        Item {
            id: self.next_id(),
            name: if preview.trim().is_empty() {
                "文字消息".to_string()
            } else {
                preview.trim().to_string()
            },
            path: String::new(),
            size: text.len() as u64,
            kind: "text".to_string(),
            ext: "txt".to_string(),
            at: hhmm(),
            text: Some(text),
            source: source.to_string(),
        }
    }

    pub fn push_inbox(&self, item: Item) {
        self.inbox.lock().unwrap().insert(0, item.clone());
        self.emit("inbox://new", item);
    }

    pub fn push_outbox(&self, item: Item) {
        self.outbox.lock().unwrap().insert(0, item);
        self.emit("outbox://change", true);
    }

    pub fn touch_peer(&self) {
        let now = Instant::now();
        let should_emit = {
            let mut guard = self.last_seen.lock().unwrap();
            let emit = match *guard {
                Some(previous) => now.duration_since(previous) >= Duration::from_secs(1),
                None => true,
            };
            *guard = Some(now);
            emit
        };
        if should_emit {
            self.emit("peer://seen", true);
        }
    }

    pub fn peer_connected(&self) -> bool {
        match *self.last_seen.lock().unwrap() {
            Some(seen) => seen.elapsed() < Duration::from_secs(25),
            None => false,
        }
    }

    pub fn state_json(&self) -> serde_json::Value {
        let ips = self.ips();
        let urls = self.urls_from(&ips);
        serde_json::json!({
            "pin": self.pin,
            "port": self.port,
            "device": self.device,
            "urls": urls,
            "saveDir": self.save_dir.to_string_lossy(),
            "started": self.started,
            "inbox": *self.inbox.lock().unwrap(),
            "outbox": *self.outbox.lock().unwrap(),
            "lan": ips.first(),
            "connected": self.peer_connected(),
            "version": env!("CARGO_PKG_VERSION"),
        })
    }

    fn load_today(&self) {
        let dir = self.save_dir.join(today());
        let Ok(entries) = fs::read_dir(&dir) else { return };
        let mut files: Vec<(SystemTime, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let p = e.path();
                if !p.is_file() {
                    return None;
                }
                let modified = e.metadata().ok()?.modified().ok()?;
                Some((modified, p))
            })
            .collect();
        files.sort_by(|a, b| b.0.cmp(&a.0));
        let mut list = Vec::new();
        for (_, path) in files.into_iter().take(60) {
            let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.starts_with('.') {
                continue;
            }
            let mut item = self.item_from_path(&path, "disk");
            item.at = "已保存".to_string();
            list.push(item);
        }
        *self.inbox.lock().unwrap() = list;
    }
}

fn bind_listener(preferred: u16) -> (Option<std::net::TcpListener>, u16) {
    let mut candidates: Vec<u16> = Vec::new();
    if preferred > 0 {
        candidates.push(preferred);
    }
    for port in PORT_BASE..PORT_BASE.saturating_add(30) {
        if !candidates.contains(&port) {
            candidates.push(port);
        }
    }
    for port in candidates {
        if let Ok(listener) = std::net::TcpListener::bind(("0.0.0.0", port)) {
            return (Some(listener), port);
        }
    }
    if let Ok(listener) = std::net::TcpListener::bind(("0.0.0.0", 0)) {
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(preferred);
        return (Some(listener), port);
    }
    (None, preferred)
}

fn config_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        let base = std::env::var("LOCALAPPDATA").ok()?;
        return Some(Path::new(&base).join("StarDrop").join("config.json"));
    }
    #[cfg(target_os = "macos")]
    {
        let home = std::env::var("HOME").ok()?;
        return Some(
            Path::new(&home)
                .join("Library")
                .join("Application Support")
                .join("StarDrop")
                .join("config.json"),
        );
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let base = std::env::var("XDG_CONFIG_HOME")
            .ok()
            .filter(|v| !v.trim().is_empty())
            .or_else(|| std::env::var("HOME").ok().map(|h| format!("{h}/.config")))?;
        return Some(Path::new(&base).join("StarDrop").join("config.json"));
    }
    #[allow(unreachable_code)]
    None
}

fn read_config() -> Option<(Option<String>, Option<u16>)> {
    let path = config_path()?;
    let text = fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&text).ok()?;
    let pin = value
        .get("pin")
        .and_then(|v| v.as_str())
        .filter(|p| p.len() == 4 && p.chars().all(|c| c.is_ascii_digit()))
        .map(|p| p.to_string());
    let port = value
        .get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok());
    Some((pin, port))
}

fn write_config(pin: &str, port: u16) {
    if let Some(path) = config_path() {
        if let Some(dir) = path.parent() {
            let _ = fs::create_dir_all(dir);
        }
        let body = serde_json::json!({ "pin": pin, "port": port }).to_string();
        let _ = fs::write(path, body);
    }
}

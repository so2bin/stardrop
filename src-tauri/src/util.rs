use std::path::{Path, PathBuf};

#[cfg(windows)]
#[repr(C)]
#[derive(Default)]
struct WinSystemTime {
    year: u16,
    month: u16,
    day_of_week: u16,
    day: u16,
    hour: u16,
    minute: u16,
    second: u16,
    milliseconds: u16,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetLocalTime(lp_system_time: *mut WinSystemTime);
}

#[cfg(windows)]
fn local_now() -> (u16, u16, u16, u16, u16, u16) {
    let mut t = WinSystemTime::default();
    unsafe { GetLocalTime(&mut t) };
    (t.year, t.month, t.day, t.hour, t.minute, t.second)
}

#[cfg(unix)]
#[repr(C)]
struct UnixTm {
    tm_sec: i32,
    tm_min: i32,
    tm_hour: i32,
    tm_mday: i32,
    tm_mon: i32,
    tm_year: i32,
    tm_wday: i32,
    tm_yday: i32,
    tm_isdst: i32,
    tm_gmtoff: i64,
    tm_zone: *const i8,
}

#[cfg(unix)]
extern "C" {
    fn time(t: *mut i64) -> i64;
    fn localtime_r(t: *const i64, tm: *mut UnixTm) -> *mut UnixTm;
}

#[cfg(unix)]
fn local_now() -> (u16, u16, u16, u16, u16, u16) {
    let mut stamp: i64 = 0;
    let mut tm = UnixTm {
        tm_sec: 0,
        tm_min: 0,
        tm_hour: 0,
        tm_mday: 1,
        tm_mon: 0,
        tm_year: 70,
        tm_wday: 0,
        tm_yday: 0,
        tm_isdst: 0,
        tm_gmtoff: 0,
        tm_zone: std::ptr::null(),
    };
    unsafe {
        time(&mut stamp);
        localtime_r(&stamp, &mut tm);
    }
    (
        (tm.tm_year + 1900) as u16,
        (tm.tm_mon + 1) as u16,
        tm.tm_mday as u16,
        tm.tm_hour as u16,
        tm.tm_min as u16,
        tm.tm_sec as u16,
    )
}

pub fn today() -> String {
    let (y, m, d, _, _, _) = local_now();
    format!("{y:04}-{m:02}-{d:02}")
}

pub fn hhmm() -> String {
    let (_, _, _, h, mi, _) = local_now();
    format!("{h:02}:{mi:02}")
}

pub fn sanitize_filename(input: &str) -> String {
    let base = input
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(input)
        .trim()
        .trim_matches('.');
    let mut out = String::with_capacity(base.len());
    for ch in base.chars() {
        let bad = matches!(ch, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') || (ch as u32) < 32;
        out.push(if bad { '_' } else { ch });
    }
    let out = out.trim().trim_matches('.').to_string();
    if out.is_empty() {
        return "未命名文件".to_string();
    }
    let chars: Vec<char> = out.chars().collect();
    if chars.len() <= 96 {
        return out;
    }
    let path = Path::new(&out);
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    let stem: String = chars[..80.min(chars.len())].iter().collect();
    if ext.is_empty() {
        stem
    } else {
        format!("{stem}.{ext}")
    }
}

pub fn unique_path(dir: &Path, name: &str) -> PathBuf {
    let safe = sanitize_filename(name);
    let mut candidate = dir.join(&safe);
    if !candidate.exists() {
        return candidate;
    }
    let path = Path::new(&safe);
    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
    for i in 1..9999u32 {
        let file = if ext.is_empty() {
            format!("{stem} ({i})")
        } else {
            format!("{stem} ({i}).{ext}")
        };
        candidate = dir.join(file);
        if !candidate.exists() {
            return candidate;
        }
    }
    dir.join(format!("{}-{}", std::process::id(), safe))
}

pub fn ext_of(name: &str) -> String {
    Path::new(name)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase()
}

pub fn kind_of(name: &str) -> &'static str {
    match ext_of(name).as_str() {
        "jpg" | "jpeg" | "png" | "gif" | "webp" | "bmp" | "heic" | "heif" | "avif" | "tif"
        | "tiff" | "jfif" | "ico" | "svg" => "image",
        "mp4" | "mov" | "m4v" | "webm" | "avi" | "mkv" | "3gp" => "video",
        _ => "file",
    }
}

pub fn mime_of(name: &str) -> &'static str {
    match ext_of(name).as_str() {
        "jpg" | "jpeg" | "jfif" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "bmp" => "image/bmp",
        "svg" => "image/svg+xml",
        "heic" => "image/heic",
        "heif" => "image/heif",
        "avif" => "image/avif",
        "ico" => "image/x-icon",
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "pdf" => "application/pdf",
        "txt" | "md" | "log" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                if let Ok(v) = u8::from_str_radix(hex, 16) {
                    out.push(v);
                    i += 3;
                    continue;
                }
                out.push(b'%');
                i += 1;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(*b as char),
            _ => out.push_str(&format!("%{:02X}", b)),
        }
    }
    out
}

pub fn base64(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((input.len() + 2) / 3 * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = *chunk.get(1).unwrap_or(&0) as u32;
        let b2 = *chunk.get(2).unwrap_or(&0) as u32;
        let n = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

pub fn local_ip() -> Option<String> {
    use std::net::UdpSocket;
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("223.5.5.5:80").ok()?;
    let ip = socket.local_addr().ok()?.ip();
    if ip.is_loopback() || ip.is_unspecified() {
        return None;
    }
    let s = ip.to_string();
    if s.starts_with("169.254.") {
        return None;
    }
    Some(s)
}

#[cfg(windows)]
fn is_private_v4(ip: &std::net::Ipv4Addr) -> bool {
    let o = ip.octets();
    o[0] == 10
        || (o[0] == 172 && (16..=31).contains(&o[1]))
        || (o[0] == 192 && o[1] == 168)
}

pub fn lan_ips() -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    if let Some(primary) = local_ip() {
        out.push(primary);
    }

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use std::process::Command;
        let output = Command::new("ipconfig.exe")
            .creation_flags(0x0800_0000)
            .output();
        if let Ok(output) = output {
            let text = String::from_utf8_lossy(&output.stdout);
            for line in text.lines() {
                if !line.contains("IPv4") {
                    continue;
                }
                for token in line.split(|c: char| !(c.is_ascii_digit() || c == '.')) {
                    if token.is_empty() || token.matches('.').count() != 3 {
                        continue;
                    }
                    if let Ok(ip) = token.parse::<std::net::Ipv4Addr>() {
                        if !ip.is_loopback() && is_private_v4(&ip) && !ip.is_link_local() {
                            let s = ip.to_string();
                            if !out.contains(&s) {
                                out.push(s);
                            }
                        }
                    }
                }
            }
        }
    }

    out
}

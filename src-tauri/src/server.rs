use crate::model::{Item, Shared};
use crate::util::*;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

const MOBILE_HTML: &str = include_str!("../../ui/mobile.html");
const MOBILE_CSS: &str = include_str!("../../ui/mobile.css");
const MOBILE_JS: &str = include_str!("../../ui/mobile.js");
const ICON_PNG: &[u8] = include_bytes!("../../ui/icon-192.png");

const MAX_UPLOAD: u64 = 2 * 1024 * 1024 * 1024;

pub fn spawn_listener(listener: TcpListener, shared: Arc<Shared>) {
    thread::spawn(move || {
        for conn in listener.incoming() {
            match conn {
                Ok(stream) => {
                    let shared = shared.clone();
                    thread::spawn(move || {
                        let _ = handle_conn(stream, shared);
                    });
                }
                Err(_) => thread::sleep(Duration::from_millis(40)),
            }
        }
    });
}

fn handle_conn(stream: TcpStream, shared: Arc<Shared>) -> io::Result<()> {
    stream.set_read_timeout(Some(Duration::from_secs(120)))?;
    stream.set_write_timeout(Some(Duration::from_secs(120)))?;
    let _ = stream.set_nodelay(true);
    let mut writer = stream;
    let read_half = writer.try_clone()?;
    let mut reader = BufReader::new(read_half);

    let mut request_line = String::new();
    if reader.read_line(&mut request_line)? == 0 {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let target = parts.next().unwrap_or("/").to_string();
    if method.is_empty() {
        return Ok(());
    }

    let mut headers: HashMap<String, String> = HashMap::new();
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    let path = target.split('?').next().unwrap_or("/").to_string();
    let query = parse_query(&target);

    match (method.as_str(), path.as_str()) {
        ("OPTIONS", _) => {
            respond(&mut writer, 204, "text/plain", b"", &[])?;
        }
        ("GET", "/") | ("GET", "/index.html") => {
            respond(&mut writer, 200, "text/html; charset=utf-8", MOBILE_HTML.as_bytes(), &[])?;
        }
        ("GET", "/m.css") => {
            respond(
                &mut writer,
                200,
                "text/css; charset=utf-8",
                MOBILE_CSS.as_bytes(),
                &[("Cache-Control", "public, max-age=600".into())],
            )?;
        }
        ("GET", "/m.js") => {
            respond(
                &mut writer,
                200,
                "application/javascript; charset=utf-8",
                MOBILE_JS.as_bytes(),
                &[("Cache-Control", "public, max-age=600".into())],
            )?;
        }
        ("GET", "/icon-192.png") | ("GET", "/favicon.ico") | ("GET", "/apple-touch-icon.png") => {
            respond(
                &mut writer,
                200,
                "image/png",
                ICON_PNG,
                &[("Cache-Control", "public, max-age=86400".into())],
            )?;
        }
        ("GET", "/api/info") => {
            let json = serde_json::json!({
                "device": shared.device,
                "version": env!("CARGO_PKG_VERSION"),
                "pinRequired": true,
            });
            json_response(&mut writer, 200, &json)?;
        }
        ("GET", "/api/ping") => {
            respond(&mut writer, 200, "text/plain", b"ok", &[])?;
        }
        ("GET", "/api/outbox") => {
            if !authorized(&shared, &query, &headers) {
                unauthorized(&mut writer)?;
                return Ok(());
            }
            shared.touch_peer();
            let items = shared.outbox.lock().unwrap().clone();
            json_response(&mut writer, 200, &serde_json::json!({ "ok": true, "items": items }))?;
        }
        ("POST", "/api/outbox") => {
            if !authorized(&shared, &query, &headers) {
                unauthorized(&mut writer)?;
                return Ok(());
            }
            receive_file(&mut writer, &mut reader, &shared, &query, &headers, true)?;
        }
        ("POST", "/api/upload") => {
            if !authorized(&shared, &query, &headers) {
                unauthorized(&mut writer)?;
                return Ok(());
            }
            receive_file(&mut writer, &mut reader, &shared, &query, &headers, false)?;
        }
        ("POST", "/api/text") => {
            if !authorized(&shared, &query, &headers) {
                unauthorized(&mut writer)?;
                return Ok(());
            }
            let body = read_small_body(&mut reader, &headers)?;
            let text = String::from_utf8_lossy(&body).trim().to_string();
            if text.is_empty() {
                json_response(&mut writer, 400, &serde_json::json!({"ok": false, "error": "empty"}))?;
                return Ok(());
            }
            shared.touch_peer();
            let item = shared.text_item(text, "phone");
            shared.push_inbox(item.clone());
            json_response(&mut writer, 200, &serde_json::json!({"ok": true, "item": item}))?;
        }
        ("GET", "/thumb") => {
            if !authorized(&shared, &query, &headers) {
                unauthorized(&mut writer)?;
                return Ok(());
            }
            let id = query.get("id").cloned().unwrap_or_default();
            match find_item(&shared, &id) {
                Some(item) if item.kind == "image" => match thumb_bytes(&shared, &item) {
                    Some(bytes) => {
                        respond(
                            &mut writer,
                            200,
                            "image/jpeg",
                            &bytes,
                            &[("Cache-Control", "public, max-age=86400".into())],
                        )?;
                    }
                    None => send_item_file(&mut writer, &item, false)?,
                },
                _ => {
                    respond(&mut writer, 404, "text/plain", b"not found", &[])?;
                }
            }
        }
        ("GET", "/file") | ("GET", "/download") => {
            if !authorized(&shared, &query, &headers) {
                unauthorized(&mut writer)?;
                return Ok(());
            }
            let id = query.get("id").cloned().unwrap_or_default();
            match find_item(&shared, &id) {
                Some(item) => send_item_file(&mut writer, &item, path == "/download")?,
                None => {
                    respond(&mut writer, 404, "text/plain", b"not found", &[])?;
                }
            }
        }
        _ => {
            respond(&mut writer, 404, "text/plain; charset=utf-8", "404".as_bytes(), &[])?;
        }
    }
    Ok(())
}

fn parse_query(target: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    if let Some((_, q)) = target.split_once('?') {
        for pair in q.split('&') {
            if pair.is_empty() {
                continue;
            }
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            map.insert(percent_decode(k), percent_decode(v));
        }
    }
    map
}

fn authorized(shared: &Shared, query: &HashMap<String, String>, headers: &HashMap<String, String>) -> bool {
    let key = query
        .get("k")
        .cloned()
        .or_else(|| headers.get("x-key").cloned())
        .unwrap_or_default();
    key == shared.pin
}

fn unauthorized(stream: &mut TcpStream) -> io::Result<()> {
    json_response(
        stream,
        403,
        &serde_json::json!({"ok": false, "error": "pin", "message": "配对码不正确"}),
    )
}

fn json_response(stream: &mut TcpStream, status: u16, value: &serde_json::Value) -> io::Result<()> {
    let body = serde_json::to_vec(value).unwrap_or_else(|_| b"{}".to_vec());
    respond(stream, status, "application/json; charset=utf-8", &body, &[])
}

fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        204 => "No Content",
        400 => "Bad Request",
        403 => "Forbidden",
        404 => "Not Found",
        405 => "Method Not Allowed",
        413 => "Payload Too Large",
        500 => "Internal Server Error",
        _ => "OK",
    }
}

fn respond(
    stream: &mut TcpStream,
    status: u16,
    ctype: &str,
    body: &[u8],
    extra: &[(&str, String)],
) -> io::Result<()> {
    let mut head = format!("HTTP/1.1 {} {}\r\n", status, reason(status));
    head.push_str(&format!("Content-Type: {}\r\n", ctype));
    head.push_str(&format!("Content-Length: {}\r\n", body.len()));
    head.push_str("Connection: close\r\n");
    head.push_str("Access-Control-Allow-Origin: *\r\n");
    head.push_str("Access-Control-Allow-Headers: content-type, x-key\r\n");
    head.push_str("Access-Control-Allow-Methods: GET, POST, OPTIONS\r\n");
    head.push_str("X-Content-Type-Options: nosniff\r\n");
    head.push_str("Cache-Control: no-store\r\n");
    for (k, v) in extra {
        head.push_str(&format!("{}: {}\r\n", k, v));
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(body)?;
    stream.flush()
}

fn receive_file(
    writer: &mut TcpStream,
    reader: &mut BufReader<TcpStream>,
    shared: &Shared,
    query: &HashMap<String, String>,
    headers: &HashMap<String, String>,
    to_phone: bool,
) -> io::Result<()> {
    let raw_name = query.get("name").cloned().unwrap_or_else(|| "image.jpg".to_string());
    let name = percent_decode(&raw_name);
    let dir = if to_phone {
        shared.send_dir.clone()
    } else {
        shared.save_dir.join(today())
    };
    if let Err(e) = fs::create_dir_all(&dir) {
        json_response(
            writer,
            500,
            &serde_json::json!({"ok": false, "error": format!("无法创建保存目录：{e}")}),
        )?;
        return Ok(());
    }
    let path = unique_path(&dir, &name);
    let mut file = match File::create(&path) {
        Ok(file) => file,
        Err(e) => {
            let _ = json_response(
                writer,
                500,
                &serde_json::json!({"ok": false, "error": format!("无法写入保存目录：{e}")}),
            );
            return Ok(());
        }
    };
    let written = match copy_body(reader, headers, &mut file, MAX_UPLOAD) {
        Ok(n) => n,
        Err(e) => {
            let _ = fs::remove_file(&path);
            let _ = json_response(
                writer,
                500,
                &serde_json::json!({"ok": false, "error": format!("{e}")}),
            );
            return Ok(());
        }
    };
    let _ = file.flush();
    drop(file);
    if written == 0 {
        let _ = fs::remove_file(&path);
        json_response(writer, 400, &serde_json::json!({"ok": false, "error": "empty file"}))?;
        return Ok(());
    }
    let item = shared.item_from_path(&path, if to_phone { "desktop" } else { "phone" });
    if to_phone {
        shared.push_outbox(item.clone());
    } else {
        shared.touch_peer();
        shared.push_inbox(item.clone());
    }
    json_response(
        writer,
        200,
        &serde_json::json!({"ok": true, "item": item, "bytes": written}),
    )
}

fn copy_body<R: BufRead>(
    reader: &mut R,
    headers: &HashMap<String, String>,
    out: &mut File,
    max: u64,
) -> io::Result<u64> {
    let chunked = headers
        .get("transfer-encoding")
        .map(|v| v.to_ascii_lowercase().contains("chunked"))
        .unwrap_or(false);
    if chunked {
        let mut total = 0u64;
        loop {
            let mut line = String::new();
            if reader.read_line(&mut line)? == 0 {
                break;
            }
            let size_text = line.trim().split(';').next().unwrap_or("0").trim().to_string();
            let size = u64::from_str_radix(&size_text, 16).unwrap_or(0);
            if size == 0 {
                loop {
                    let mut trailer = String::new();
                    if reader.read_line(&mut trailer)? == 0 {
                        break;
                    }
                    if trailer == "\r\n" || trailer == "\n" {
                        break;
                    }
                }
                break;
            }
            if total + size > max {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "payload too large"));
            }
            let copied = io::copy(&mut reader.by_ref().take(size), out)?;
            total += copied;
            let mut crlf = [0u8; 2];
            let _ = reader.read_exact(&mut crlf);
        }
        return Ok(total);
    }

    let len = headers
        .get("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0);
    if len > 0 {
        if len > max {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "payload too large"));
        }
        return io::copy(&mut reader.by_ref().take(len), out);
    }
    io::copy(&mut reader.by_ref().take(max), out)
}

fn read_small_body<R: BufRead>(reader: &mut R, headers: &HashMap<String, String>) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    let len = headers
        .get("content-length")
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(0)
        .min(256 * 1024);
    if len > 0 {
        reader.by_ref().take(len).read_to_end(&mut out)?;
    } else {
        reader.by_ref().take(256 * 1024).read_to_end(&mut out)?;
    }
    Ok(out)
}

fn find_item(shared: &Shared, id: &str) -> Option<Item> {
    if id.is_empty() {
        return None;
    }
    shared
        .inbox
        .lock()
        .unwrap()
        .iter()
        .chain(shared.outbox.lock().unwrap().iter())
        .find(|i| i.id == id)
        .cloned()
}

fn thumb_bytes(shared: &Shared, item: &Item) -> Option<Vec<u8>> {
    if let Some(cached) = shared.thumbs.lock().unwrap().get(&item.id) {
        return Some(cached.clone());
    }
    let path = Path::new(&item.path);
    let img = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.decode().ok()?;
    let small = img.thumbnail(520, 520);
    let rgb = image::DynamicImage::ImageRgb8(small.to_rgb8());
    let mut cursor = io::Cursor::new(Vec::new());
    rgb.write_to(&mut cursor, image::ImageFormat::Jpeg).ok()?;
    let bytes = cursor.into_inner();
    shared
        .thumbs
        .lock()
        .unwrap()
        .insert(item.id.clone(), bytes.clone());
    Some(bytes)
}

fn send_item_file(stream: &mut TcpStream, item: &Item, attachment: bool) -> io::Result<()> {
    let path = Path::new(&item.path);
    let file = match File::open(path) {
        Ok(f) => f,
        Err(_) => {
            return respond(stream, 404, "text/plain", b"missing", &[]);
        }
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let mime = mime_of(&item.name);
    let mut head = format!("HTTP/1.1 200 OK\r\nContent-Type: {}\r\nContent-Length: {}\r\n", mime, len);
    head.push_str("Connection: close\r\n");
    head.push_str("Access-Control-Allow-Origin: *\r\n");
    head.push_str("Cache-Control: private, max-age=300\r\n");
    let ascii: String = item
        .name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    let disp = if attachment { "attachment" } else { "inline" };
    head.push_str(&format!(
        "Content-Disposition: {}; filename=\"{}\"; filename*=UTF-8''{}\r\n",
        disp,
        ascii,
        url_encode(&item.name)
    ));
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    let mut file = file;
    io::copy(&mut file, stream)?;
    stream.flush()
}

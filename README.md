# StarDrop

> Phone photos, straight to your PC. Scan a QR code, pick photos — done. No phone app to install.

[![build](https://github.com/so2bin/stardrop/actions/workflows/build.yml/badge.svg)](https://github.com/so2bin/stardrop/actions/workflows/build.yml)
![platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-0078D4)
![tauri](https://img.shields.io/badge/Tauri-2-24C8DB)
![license](https://img.shields.io/badge/license-MIT-green)

**English** | [简体中文](README.zh-CN.md)

## Why a browser page instead of a NearLink app

Huawei NearLink (星闪) file transfer is a private capability inside the Huawei ecosystem. There is **no public Windows SDK / API** for third-party apps to send files over NearLink. A native NearLink app would only cover Huawei phones, and would still require HarmonyOS native development and signing — high cost, narrow reach.

StarDrop takes the LAN + mobile browser route:

- Works on iPhone, Android and HarmonyOS — **nothing to install on the phone**
- Original quality, direct transfer, no cloud in the middle
- One Tauri desktop app; the phone side is just a QR code
- If NearLink shows up as a network adapter on the PC (or you use a hotspot / USB tethering), the same page works over that network with no code changes

## Screenshots

<table>
<tr>
<td><img src="docs/screenshots/app.png" width="760" alt="StarDrop desktop app" /></td>
<td><img src="docs/screenshots/mobile.png" width="250" alt="StarDrop mobile page" /></td>
</tr>
</table>

When a phone connects, the QR panel collapses to a single “Phone connected” row. Click the header to expand it again; it reopens automatically about 25 seconds after the phone disconnects.

## Features

- Scan a QR code on the phone, then pick photos / videos or take a photo — multi-select, progress, original quality
- Drag files onto the desktop window to make them downloadable from the phone
- Send text / links from the phone, copy them from a desktop card
- Full-screen preview; open the file, reveal it in Explorer, copy its path
- Auto archive by date: `C:\Users\<you>\Pictures\星传\YYYY-MM-DD\`
- Duplicate names get a numeric suffix; Chinese and spaces in file names work
- **Pairing PIN and port are persisted** — no re-scan after restarting the app or the PC
- QR panel auto-collapses while a phone is connected and reopens after it disconnects
- Single instance: launching again just focuses the existing window
- Built-in “Fix connection” button for the Windows Firewall rule

## Quick start

### Download

Get the installer for your platform from [Releases](https://github.com/so2bin/stardrop/releases/latest):

| Platform | File | Notes |
| --- | --- | --- |
| Windows 10/11 x64 | `StarDrop_<version>_x64-setup.exe` | one-click installer; a portable `StarDrop-<version>-win-x64.zip` is also available |
| macOS 11+ (Apple Silicon) | `StarDrop_<version>_aarch64.dmg` | unsigned; right-click the app → **Open** on first launch |
| Linux x64 | `StarDrop_<version>_amd64.deb` / `.AppImage` | AppImage: `chmod +x` and run |

> The builds are unsigned. On Windows, if SmartScreen warns, choose **More info → Run anyway**. On macOS, right-click → **Open**, or run `xattr -dr com.apple.quarantine /Applications/StarDrop.app`.
> On first launch, allow the firewall prompt (Windows), otherwise the phone cannot connect.

### Usage

1. Connect the phone to the same Wi-Fi as the PC (NearLink network, hotspot, or USB tethering also work).
2. Scan the QR code in the app with the phone camera or browser.
3. Pick photos — they are saved to `~/Pictures/星传/<date>/` (`C:\Users\<you>\Pictures\星传\<date>\` on Windows).
4. To send files the other way, drag them into the app window and download them from the phone page.

The PIN and port live in:

| Platform | Path |
| --- | --- |
| Windows | `%LOCALAPPDATA%\StarDrop\config.json` |
| macOS | `~/Library/Application Support/StarDrop/config.json` |
| Linux | `~/.config/StarDrop/config.json` |

For example `{"pin":"1234","port":47823}`. To change them, close the app, edit this file, and start it again.

## Build

### Requirements

- Windows 10/11 with the WebView2 Runtime (bundled with Windows 11; usually present on Windows 10)
- macOS 11+ or a Linux desktop with `webkit2gtk-4.1`
- Rust 1.90+
- Windows: MSVC toolchain (recommended) or MinGW-w64 GNU toolchain (this project has also been verified with GNU)
- CI builds the Windows NSIS installer, macOS DMG and Linux `.deb`/AppImage with the official Tauri CLI

### Compile

```powershell
cargo build --release --manifest-path src-tauri/Cargo.toml
```

The binary is `src-tauri/target/release/stardrop.exe`.

- **MSVC**: WebView2Loader is linked statically — the exe is self-contained.
- **MinGW-w64 GNU**: a `WebView2Loader.dll` is produced next to the exe. It must be shipped together with the exe, otherwise the app fails with `0xC0000135`.

### Package

```powershell
pwsh scripts/package.ps1
```

This creates `dist/StarDrop-<version>-win-x64.zip` with the exe, icon, usage notes and uninstall script; GNU builds also include `WebView2Loader.dll`. If PowerShell blocks the script, run:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package.ps1
```

### Installer bundles

With the [Tauri CLI](https://tauri.app/start/prerequisites/):

```powershell
npm install -g @tauri-apps/cli@^2
tauri build --bundles nsis          # Windows setup.exe
tauri build --bundles dmg           # macOS .dmg
tauri build --bundles deb,appimage  # Linux .deb / .AppImage
```

The same commands run in CI on `windows-latest`, `macos-latest` and `ubuntu-22.04`; artifacts are uploaded for every push, and on a `v*` tag they are attached to the GitHub Release.

### Development notes

The frontend is plain HTML/CSS/JS with no bundler. Everything under `ui/` is embedded into the exe at compile time via `include_str!`, so **UI changes require a rebuild** — there is no hot reload.

## Project layout

```
ui/
  index.html / styles.css / app.js       desktop UI
  mobile.html / mobile.css / mobile.js   phone page (served at /)
  qr.js                                  QR generator (byte mode / EC M)
src-tauri/
  src/main.rs                            Tauri commands, single instance, DPI, WebView2 data dir
  src/model.rs                           state, persisted config, PIN / port
  src/server.rs                          HTTP server, upload / download / thumbnails
  src/util.rs                            time, file names, IPs, encoding
  app.manifest                           PerMonitorV2 DPI awareness
  tauri.conf.json
install/                                 usage notes and uninstall script shipped with the app
scripts/                                 build / package scripts
```

## Implementation notes

- **HTTP server**: `std::net::TcpListener` plus one thread per connection, with hand-written request parsing, chunked decoding and streaming to disk. No axum/hyper, so the binary stays small.
- **Phone page**: a single-page H5 app with XHR upload progress, 3 concurrent uploads and retry hints. It polls the download list every 5 seconds, which doubles as the presence heartbeat.
- **QR code**: `ui/qr.js` implements QR generation (byte mode / EC level M / versions 1–6); verified with OpenCV decoding.
- **Thumbnails**: decoded with the `image` crate, resized to 520 px JPEG and cached in memory.
- **Presence**: only phone-facing calls (upload / text / outbox polling) refresh `last_seen`; the desktop loading its own thumbnails does not count.
- **DPI**: `app.manifest` declares PerMonitorV2 so the window does not overflow at 200% scaling.
- **Port**: saved port first, then 47823–47852, then an ephemeral port; the chosen port is written back to the config.

## HTTP API

| Method | Path | Description |
| --- | --- | --- |
| GET | `/`, `/m.css`, `/m.js` | Phone page |
| GET | `/api/info` | Device name (no PIN required) |
| GET | `/api/outbox?k=` | Pending downloads for the phone (presence heartbeat) |
| POST | `/api/upload?name=` | Phone upload; raw body streamed to disk |
| POST | `/api/text` | Send text from the phone |
| POST | `/api/outbox?name=` | Desktop file picker adds a file to the outbox |
| GET | `/thumb?id=&k=` | Thumbnail |
| GET | `/file?id=&k=` | Original image / video, inline |
| GET | `/download?id=&k=` | Attachment download (RFC 5987 file names) |

Pairing check: query parameter `k` or header `X-Key`.

## Security

- The server listens on the LAN only; photos never leave your network
- The PIN lives in the QR URL, effectively a local network passphrase. Do not port-forward it to the internet.
- Plain HTTP; use it on trusted home / office networks
- Received files are written straight to disk — only let devices you trust scan the code

## Limitations

- Windows only (Tauri itself is portable, but the WebView2-specific bits need work)
- Single instance and the “Fix connection” firewall helper are Windows-only
- macOS / Linux builds are new and lightly tested; the core transfer flow is shared, but open/reveal uses Finder / `xdg-open`
- HEIC preview depends on the system codec; if decoding fails you get an extension placeholder, but the file is still saved
- The phone and PC must be able to reach each other; guest Wi-Fi / AP isolation will block it
- No resume support — prefer a stable Wi-Fi connection for very large videos

## License

[MIT](LICENSE) © 2026 heli (so2bin)

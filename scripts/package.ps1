param(
    [string]$Version = "",
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

if (-not $Version) {
    $conf = Get-Content (Join-Path $root "src-tauri\tauri.conf.json") -Raw -Encoding UTF8 | ConvertFrom-Json
    $Version = $conf.version
}

if (-not $SkipBuild) {
    Push-Location (Join-Path $root "src-tauri")
    try {
        cargo build --release
        if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }
    }
    finally {
        Pop-Location
    }
}

$release = Join-Path $root "src-tauri\target\release"
$stage = Join-Path $root "dist\StarDrop-$Version-win-x64"
$zip = Join-Path $root "dist\StarDrop-$Version-win-x64.zip"

Remove-Item $stage -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force $stage | Out-Null

Copy-Item (Join-Path $release "stardrop.exe") (Join-Path $stage "StarDrop.exe")

# GNU builds link WebView2Loader dynamically; MSVC links it statically.
$loader = Join-Path $release "WebView2Loader.dll"
if (Test-Path $loader) { Copy-Item $loader (Join-Path $stage "WebView2Loader.dll") }

$ico = Join-Path $root "src-tauri\icons\icon.ico"
if (Test-Path $ico) { Copy-Item $ico (Join-Path $stage "StarDrop.ico") }

Copy-Item (Join-Path $root "install\README-使用说明.txt") (Join-Path $stage "使用说明.txt")
Copy-Item (Join-Path $root "install\卸载.cmd") (Join-Path $stage "卸载.cmd")

Remove-Item $zip -Force -ErrorAction SilentlyContinue
Compress-Archive -Path $stage -DestinationPath $zip -CompressionLevel Optimal

Write-Host ""
Write-Host "Packaged: $zip" -ForegroundColor Green
Write-Host "Contents:"
Get-ChildItem $stage | Select-Object Name, Length | Format-Table -AutoSize

param(
    [switch]$SkipBuild
)

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $PSScriptRoot

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

Write-Host "Build finished. Run scripts\package.ps1 to create the release zip." -ForegroundColor Green

# NekoLib — Windows Plug & Play Installer (PowerShell)
# Supports: Windows 10/11 x86_64 via WSL or native cargo
# Author: Vaibhav — GPLv3
$ErrorActionPreference = "Stop"
Write-Host @"
 ███╗   ██╗███████╗██╗  ██╗ ██████╗ ██╗     ██╗██████╗
 ████╗  ██║██╔════╝██║ ██╔╝██╔═══██╗██║     ██║██╔══██╗
 ██╔██╗ ██║█████╗  █████╔╝ ██║   ██║██║     ██║██████╔╝
 ██║╚██╗██║██╔══╝  ██╔═██╗ ██║   ██║██║     ██║██╔══██╗
 ██║ ╚████║███████╗██║  ██╗╚██████╔╝███████╗██║██████╔╝
"@ -ForegroundColor Magenta
Write-Host "  Pure-CPU L1 • BLAKE3 SIMD • 100k cap — by Vaibhav (GPLv3)" -ForegroundColor DarkGray

$DEST = "$HOME\nekolib"
$REPO = "https://github.com/thisisforlearn/nekolib.git"

function Ask($msg, $def="Y") {
  $p = if ($def -eq "Y") { "$msg [Y/n]: " } else { "$msg [y/N]: " }
  $ans = Read-Host $p
  if ([string]::IsNullOrWhiteSpace($ans)) { $ans = $def }
  return ($ans -match "^[Yy]")
}

Write-Host "`nThis will install Rust (if missing), clone NekoLib, and build optimized binary." -ForegroundColor White
Write-Host "Estimated: 4-8 min • Disk: ~15 MB" -ForegroundColor DarkGray
if (-not (Ask "Install NekoLib now?")) { Write-Host "Cancelled." -ForegroundColor Yellow; exit 0 }

# check git
if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
  Write-Host "Installing git..." -ForegroundColor Cyan
  winget install --id Git.Git -e --silent --accept-package-agreements
  $env:Path = [System.Environment]::GetEnvironmentVariable("Path","Machine") + ";" + [System.Environment]::GetEnvironmentVariable("Path","User")
}
if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
  Write-Host "Installing Rust..." -ForegroundColor Cyan
  Invoke-WebRequest https://win.rustup.rs/x86_64 -OutFile "$env:TEMP\rustup-init.exe"
  & "$env:TEMP\rustup-init.exe" -y --default-toolchain stable
  $env:Path += ";$HOME\.cargo\bin"
}

if (Test-Path "$DEST\.git") { git -C $DEST pull }
else { git clone $REPO $DEST }

Set-Location $DEST
$env:RUSTFLAGS="-C target-cpu=native"
cargo build --release
if ($LASTEXITCODE -ne 0) { Write-Host "Build failed — see README nerd guide" -ForegroundColor Red; exit 1 }

Write-Host "`n✓ Build done! $DEST\target\release\nekod.exe" -ForegroundColor Green
if (Ask "Create wallet?") { & "$DEST\target\release\nekod.exe" wallet }
if (Ask "Start mining now?") { & "$DEST\target\release\nekod.exe" start --mine }
else {
  Write-Host "`nQuick commands:" -ForegroundColor White
  Write-Host "  cd $DEST; .\target\release\nekod.exe wallet" -ForegroundColor Cyan
  Write-Host "  .\target\release\nekod.exe start --mine" -ForegroundColor Cyan
}

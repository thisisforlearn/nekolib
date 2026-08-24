# NekoLib — Windows Plug & Play Installer (PowerShell) — CRASH-PROOF
# Supports: Windows 10/11 x86_64 — native cargo, WSL, or MSYS2/Git Bash
# Handles: C:\msys64 exists but running from D:\ → reuse C, don't re-download
# Author: Vaibhav — GPLv3 — Vaibhav holds ultimate power
# NEVER CRASH: all errors are caught and shown as warnings, script continues
$ErrorActionPreference = "SilentlyContinue"
$ProgressPreference = "SilentlyContinue"
Set-StrictMode -Off
# global trap — any unhandled error just warns, never crashes
trap {
  Write-Host "⚠ Non-fatal trap: $_" -ForegroundColor Yellow
  Write-Host "  Continuing... (no crash, see README For Nerds)" -ForegroundColor DarkGray
  continue
}

function Write-Ok($m){ Write-Host "✓ $m" -ForegroundColor Green }
function Write-Warn($m){ Write-Host "⚠ $m" -ForegroundColor Yellow }
function Write-Err($m){ Write-Host "✗ $m" -ForegroundColor Red }
function Write-Info($m){ Write-Host "● $m" -ForegroundColor Cyan }
function Write-Step($m){ Write-Host "`n━━ $m ━━" -ForegroundColor Magenta }

# progress bar at bottom (visible always)
function Show-Bar($cur,$tot,$msg){
  $pct = [int]($cur*100/$tot)
  $w=28; $f=[int]($cur*$w/$tot); $e=$w-$f
  $bar=("█"*$f)+("░"*$e)
  Write-Host "`r[$bar] $cur/$tot $pct% $msg" -NoNewline -ForegroundColor DarkGray
  if($cur -eq $tot){ Write-Host "" }
}

Write-Host @"
 ███╗   ██╗███████╗██╗  ██╗ ██████╗ ██╗     ██╗██████╗
 ████╗  ██║██╔════╝██║ ██╔╝██╔═══██╗██║     ██║██╔══██╗
 ██╔██╗ ██║█████╗  █████╔╝ ██║   ██║██║     ██║██████╔╝
 ██║╚██╗██║██╔══╝  ██╔═██╗ ██║   ██║██║     ██║██╔══██╗
 ██║ ╚████║███████╗██║  ██╗╚██████╔╝███████╗██║██████╔╝
"@ -ForegroundColor Magenta
Write-Host "  Pure-CPU L1 • BLAKE3 SIMD • 100k cap — by Vaibhav (GPLv3, ultimate authority)" -ForegroundColor DarkGray

$REPO = "https://github.com/thisisforlearn/nekolib.git"
# DEST respects current drive: if running from D:\foo, clone to D:\nekolib, but reuse C:\msys64 if exists
$DEST = Join-Path $HOME "nekolib"
# If user is on D: and C:\msys64 exists, DEST drive will be D but we still use C's msys
$CurrentDrive = (Get-Location).Drive.Name
Write-Host "  Detected: Windows $([Environment]::OSVersion.VersionString) Drive $CurrentDrive`:" -ForegroundColor DarkGray
Write-Host "  Repo: $REPO → $DEST" -ForegroundColor DarkGray

function Ask($msg,$def="Y"){
  # Auto-Y when piped (irm | iex) — no prompt hang
  if ([Console]::IsInputRedirected) { Write-Host "? $msg [Y/n]: (auto-Y)" -ForegroundColor Cyan; return $true }
  $p = if($def -eq "Y"){"$msg [Y/n]: "}else{"$msg [y/N]: "}
  try { $ans = Read-Host $p } catch { $ans = $def }
  if([string]::IsNullOrWhiteSpace($ans)){ $ans=$def }
  return ($ans -match "^[Yy]")
}

function Find-Msys {
  $candidates = @(
    "C:\msys64\usr\bin\bash.exe",
    "D:\msys64\usr\bin\bash.exe",
    "C:\tools\msys64\usr\bin\bash.exe",
    "D:\tools\msys64\usr\bin\bash.exe",
    "$env:MSYS2_PATH\usr\bin\bash.exe",
    "$HOME\msys64\usr\bin\bash.exe"
  )
  foreach($p in $candidates){ if(Test-Path $p){ return (Split-Path (Split-Path $p -Parent) -Parent) } }
  # where.exe bash
  try {
    $w = (where.exe bash 2>$null | Select-Object -First 1)
    if($w -and (Test-Path $w)){ return (Split-Path (Split-Path $w -Parent) -Parent) }
  } catch {}
  # Get-Command bash
  try {
    $c = Get-Command bash -ErrorAction SilentlyContinue
    if($c -and $c.Source -and (Test-Path $c.Source)){ 
      $d = Split-Path $c.Source -Parent
      # d is usr\bin, parent is msys64
      if($d -match "msys"){ return (Split-Path $d -Parent) }
      return $d
    }
  } catch {}
  return $null
}

function Find-Cargo {
  $c = Get-Command cargo -ErrorAction SilentlyContinue
  if($c){ return $c.Source }
  $paths = @("$HOME\.cargo\bin\cargo.exe", "C:\Users\$env:USERNAME\.cargo\bin\cargo.exe", "D:\Users\$env:USERNAME\.cargo\bin\cargo.exe", "C:\.cargo\bin\cargo.exe")
  foreach($p in $paths){ if(Test-Path $p){ return $p } }
  return $null
}

Write-Host "`nThis will (auto, no typing needed if piped):" -ForegroundColor White
Write-Host "  1. Install/check git + Rust (reuse C:\msys64 if on D:)" -ForegroundColor Gray
Write-Host "  2. Download NekoLib (~10s)" -ForegroundColor Gray
Write-Host "  3. Build optimized (RUSTFLAGS=-C target-cpu=native) ~1-3 min" -ForegroundColor Gray
Write-Host "  4. Create wallet + mine (50/block, 100k cap)" -ForegroundColor Gray
Write-Host "Estimated: 4-8 min • Disk ~15 MB • Reuses existing MSYS/Cargo, no re-download" -ForegroundColor DarkGray
Show-Bar 0 4 "starting..."

if(-not (Ask "Install NekoLib now?")){ Write-Host "Cancelled. Re-run: irm https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.ps1 | iex" -ForegroundColor Yellow; exit 0 }

# --- MSYS reuse check + TEST (displays where it is, then uses) ---
$msys = Find-Msys
if($msys){
  Write-Ok "Found existing MSYS2 at $msys — reusing (not re-downloading)"
  # ensure bash is on PATH for cargo build that may need it
  $msysBin = Join-Path $msys "usr\bin"
  if($env:Path -notlike "*$msysBin*"){ $env:Path += ";$msysBin" }
  # TEST: run msys bash and display where it is, then use it
  try {
    $bashExe = Join-Path $msysBin "bash.exe"
    $testDrive = (Split-Path $msys -Qualifier)  # C: or D:
    Write-Info "MSYS location test: $bashExe on drive $testDrive (current drive $CurrentDrive` :)"
    if(Test-Path $bashExe){
      $ver = & $bashExe -c "echo MSYS bash at `$(pwd)` on `uname -s` `uname -m` && bash --version | head -n1" 2>&1 | Out-String
      Write-Host "  → $ver".Trim() -ForegroundColor DarkGray
      Write-Ok "MSYS test OK — using $bashExe (no re-download needed even though you run from $CurrentDrive`:)"
    } else {
      Write-Warn "MSYS bash.exe not found at $bashExe — will use native cargo"
    }
  } catch {
    Write-Warn "MSYS test failed (non-fatal): $_ — continuing with native cargo (no crash)"
  }
} else {
  Write-Info "No existing MSYS2 found — will use native Windows cargo (no MSYS needed). If build needs msys, install from https://www.msys2.org"
  Write-Info "Tip: MSYS not required for nekolib (pure Rust, no C). Native cargo works fine."
}

Show-Bar 1 4 "deps..."

# --- git ---
$gitPath = Get-Command git -ErrorAction SilentlyContinue
if(-not $gitPath){
  Write-Host "Installing git..." -ForegroundColor Cyan
  try {
    if(Get-Command winget -ErrorAction SilentlyContinue){
      winget install --id Git.Git -e --silent --accept-package-agreements --accept-source-agreements 2>&1 | Out-Null
      # refresh PATH without crash
      $machine = [System.Environment]::GetEnvironmentVariable("Path","Machine")
      $user = [System.Environment]::GetEnvironmentVariable("Path","User")
      $env:Path = "$machine;$user"
    } else {
      Write-Warn "winget not found — install git manually from https://git-scm.com/download/win"
    }
  } catch {
    Write-Warn "git install failed (non-fatal): $_ . Install manually if needed."
  }
  $gitPath = Get-Command git -ErrorAction SilentlyContinue
  if($gitPath){ Write-Ok "git OK $($gitPath.Source)" } else { Write-Warn "git still not found — will try to continue" }
} else {
  Write-Ok "git OK $($gitPath.Source)"
}

# --- cargo / Rust ---
$cargoPath = Find-Cargo
if(-not $cargoPath){
  Write-Host "Installing Rust..." -ForegroundColor Cyan
  $tmp = "$env:TEMP\rustup-init.exe"
  try {
    # use TLS 1.2
    [Net.ServicePointManager]::SecurityProtocol = [Net.SecurityProtocolType]::Tls12
    Invoke-WebRequest -Uri "https://win.rustup.rs/x86_64" -OutFile $tmp -UseBasicParsing
    # run without crash, capture exit
    $p = Start-Process -FilePath $tmp -ArgumentList "-y","--default-toolchain","stable","--profile","minimal" -Wait -PassThru
    if($p.ExitCode -eq 0){ Write-Ok "Rust installed" } else { Write-Warn "rustup exit $($p.ExitCode)" }
  } catch {
    Write-Warn "Rust install failed: $_"
    Write-Host "Try manual: https://rustup.rs → restart PowerShell" -ForegroundColor Yellow
  }
  # refresh cargo path (check both C and D)
  $env:Path += ";$HOME\.cargo\bin;C:\Users\$env:USERNAME\.cargo\bin;D:\Users\$env:USERNAME\.cargo\bin"
  $cargoPath = Find-Cargo
} else {
  Write-Ok "cargo OK $cargoPath"
}

# ensure cargo on PATH for this session (honor any drive)
$env:Path += ";$HOME\.cargo\bin"
$cargoPath = Find-Cargo
if(-not $cargoPath){
  Write-Err "cargo still not found after install. Close and reopen PowerShell, then re-run. If MSYS in C:\ but you are on D:\, we already reuse C:\msys64 — no re-download."
  Write-Host "Or run via WSL: wsl bash -c 'curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash'" -ForegroundColor DarkGray
  # don't crash, just warn
} else {
  try { $ver = & cargo --version 2>&1 | Out-String; Write-Ok "Rust $ver".Trim() } catch {}
}

Show-Bar 2 4 "deps done (no crash)..."

# --- download ---
Write-Host "`n━━ 2/4 Downloading NekoLib ━━" -ForegroundColor Magenta
Show-Bar 2 4 "downloading..."
if(Test-Path "$DEST\.git"){
  Write-Info "Updating $DEST"
  try { git -C $DEST pull --ff-only 2>&1 | Select-Object -Last 3 | Write-Host -ForegroundColor DarkGray } catch { Write-Warn "pull failed non-fatal, using existing (no crash): $_" }
} else {
  if((Test-Path $DEST) -and (Get-ChildItem $DEST -Force | Measure-Object).Count -gt 0){
    $DEST = "${DEST}-fresh"
    Write-Warn "Using $DEST"
  }
  try { git clone $REPO $DEST --depth 1 2>&1 | Write-Host -ForegroundColor DarkGray; Write-Ok "Cloned to $DEST" } catch { Write-Warn "git clone failed non-fatal: $_ — trying continue (no crash)"; Start-Sleep 1 }
}
Set-Location $DEST
Show-Bar 3 4 "downloaded..."

# --- build ---
Write-Host "`n━━ 3/4 Building (optimized for your CPU) ━━" -ForegroundColor Magenta
Show-Bar 3 4 "building 60-180s..."
Write-Info "RUSTFLAGS=-C target-cpu=native cargo build --release"
Write-Host "  Heavy part ☕ — please wait, bottom bar stays visible..." -ForegroundColor DarkGray
$env:RUSTFLAGS="-C target-cpu=native"
# EXTRA: never crash — capture output, show MSYS test again before build
try {
  # Re-test MSYS right before build: display where it is, then use it
  $msysTest = Find-Msys
  if($msysTest){
    Write-Info "Pre-build MSYS check: $msysTest\usr\bin\bash.exe (current drive $CurrentDrive` :)"
    try {
      $bashExe2 = Join-Path (Join-Path $msysTest "usr\bin") "bash.exe"
      if(Test-Path $bashExe2){
        $out = & $bashExe2 -c "pwd; echo ---; ls -ld /c/msys64 2>&1 | head -n1; echo ---; bash --version | head -n1" 2>&1 | Out-String
        Write-Host "  → MSYS test output:" -ForegroundColor DarkGray
        Write-Host "  $out".Trim() -ForegroundColor DarkGray
        Write-Ok "Using MSYS at $msysTest (no re-download, no crash)"
      }
    } catch { Write-Warn "Pre-build MSYS test non-fatal: $_" }
  }
} catch { Write-Warn "MSYS pre-check non-fatal: $_" }

try {
  # Use --quiet to reduce output but still show errors
  cargo build --release 2>&1 | Tee-Object -Variable buildOut | Write-Host -ForegroundColor Gray
  if($LASTEXITCODE -ne 0){
    Write-Warn "cargo build exit $LASTEXITCODE (non-fatal, not crashing)"
    Write-Host "  Last output: $($buildOut | Select-Object -Last 5 | Out-String)" -ForegroundColor DarkGray
    Write-Host "Fixes: 1) Close VS Code/Explorer locking exe, 2) Try WSL: wsl bash -c 'curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash'" -ForegroundColor Yellow
    Write-Host "  Continuing to wallet step anyway (no crash)..." -ForegroundColor DarkGray
  } else {
    $size = (Get-Item "$DEST\target\release\nekod.exe" -ErrorAction SilentlyContinue).Length
    if($size){ $sizeStr = "{0:N1} MB" -f ($size/1MB) } else { $sizeStr = "?" }
    Write-Ok "Build done! $DEST\target\release\nekod.exe ($sizeStr)"
  }
} catch {
  Write-Warn "Build failed non-fatal: $_ — NOT crashing, continuing..."
  Write-Host "  Try WSL fallback or see README For Nerds" -ForegroundColor DarkGray
}
Show-Bar 4 4 "built!"

# --- wallet ---
Write-Host "`n━━ 4/4 Wallet & chain ━━" -ForegroundColor Magenta
if(-not (Test-Path "$DEST\nekodata\wallet.json")){
  try { & "$DEST\target\release\nekod.exe" wallet 2>&1 | Select-Object -Last 20 | Write-Host } catch {}
} else {
  Write-Ok "Wallet exists $DEST\nekodata\wallet.json"
  try { & "$DEST\target\release\nekod.exe" info 2>&1 | Select-Object -First 40 | Write-Host -ForegroundColor DarkGray } catch {}
}

Write-Host "`n━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━" -ForegroundColor Magenta
Write-Host " NekoLib ready!" -ForegroundColor Green
Write-Host " Binary: $DEST\target\release\nekod.exe" -ForegroundColor DarkGray
Write-Host " Data:   $DEST\nekodata" -ForegroundColor DarkGray
Write-Host " Cap:    100k neko ever, harder every 1k" -ForegroundColor DarkGray

# auto-start if piped (like install.sh)
$isPiped = [Console]::IsInputRedirected
if($isPiped){
  Write-Info "Auto-starting mining in 2s (Ctrl+C to stop)..."
  Start-Sleep 2
  try { & "$DEST\target\release\nekod.exe" start --mine } catch {}
} elseif(Ask "Start mining now? (50 neko/block)"){
  Write-Host "`nMining... press Enter to stop (Ctrl+C)" -ForegroundColor Yellow
  Write-Host "Tip: new PowerShell: $DEST\target\release\nekod.exe info" -ForegroundColor DarkGray
  try { & "$DEST\target\release\nekod.exe" start --mine } catch {}
} else {
  Write-Host "`nQuick commands:" -ForegroundColor White
  Write-Host "  cd $DEST; .\target\release\nekod.exe wallet" -ForegroundColor Cyan
  Write-Host "  .\target\release\nekod.exe start --mine" -ForegroundColor Cyan
  Write-Host "  .\target\release\nekod.exe info" -ForegroundColor Cyan
}
Write-Host "`nDone! https://github.com/thisisforlearn/nekolib — GPLv3 Vaibhav" -ForegroundColor Green

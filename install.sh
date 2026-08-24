#!/usr/bin/env bash
# NekoLib — Plug & Play Installer (TRULY ZERO-TOUCH)
# Supports: x86_64 Linux, ARM64 Linux (Pi/SBC), macOS, WSL, Termux (Android)
# Author: Vaibhav — GPLv3 — Vaibhav holds ultimate power
set -e

# colors
if [ -t 1 ] || [ -n "${TERM:-}" ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'
  MAG='\033[0;35m'; BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'
else
  RED=''; GREEN=''; YELLOW=''; CYAN=''; MAG=''; BOLD=''; DIM=''; RESET=''
fi
ok() { echo -e "${GREEN}✓${RESET} $*"; }
warn() { echo -e "${YELLOW}⚠${RESET} $*"; }
err() { echo -e "${RED}✗${RESET} $*"; }
info() { echo -e "${CYAN}●${RESET} $*"; }
step() { echo -e "\n${MAG}━━ $* ━━${RESET}"; }

banner() {
cat <<'BANNER'
 ███╗   ██╗███████╗██╗  ██╗ ██████╗ ██╗     ██╗██████╗
 ████╗  ██║██╔════╝██║ ██╔╝██╔═══██╗██║     ██║██╔══██╗
 ██╔██╗ ██║█████╗  █████╔╝ ██║   ██║██║     ██║██████╔╝
 ██║╚██╗██║██╔══╝  ██╔═██╗ ██║   ██║██║     ██║██╔══██╗
 ██║ ╚████║███████╗██║  ██╗╚██████╔╝███████╗██║██████╔╝
 ╚═╝  ╚═══╝╚══════╝╚═╝  ╚═╝ ╚═════╝ ╚══════╝╚═╝╚═════╝
BANNER
echo -e "${DIM}  Pure-CPU L1 • BLAKE3 SIMD • 100k cap • bump every 1k — by Vaibhav (GPLv3)${RESET}\n"
}

detect_os() {
  OS="$(uname -s 2>/dev/null || echo Linux)"
  ARCH="$(uname -m 2>/dev/null || echo unknown)"
  case "$ARCH" in x86_64|amd64) ARCH="x86_64" ;; aarch64|arm64) ARCH="aarch64" ;; armv7l) ARCH="armv7" ;; *) ARCH="$ARCH" ;; esac
  IS_TERMUX=0
  if [ -n "${TERMUX_VERSION:-}" ] || [ -d "/data/data/com.termux" ] || echo "$PREFIX" | grep -q termux 2>/dev/null; then IS_TERMUX=1; fi
  echo -e "${DIM}Detected: $OS $ARCH $([ $IS_TERMUX -eq 1 ] && echo "(Termux)")${RESET}"
}

need_cmd() { command -v "$1" >/dev/null 2>&1; }

# FIXED ask: auto-Y when piped (curl | bash) — truly plug-and-play, no typing
ask() {
  local prompt="$1" def="${2:-Y}"
  local yn
  if [ "$def" = "Y" ]; then prompt="$prompt [Y/n]: "; else prompt="$prompt [y/N]: "; fi
  # If stdin is not a tty (piped via curl|bash) → auto-Y, no waiting
  if [ ! -t 0 ]; then
    printf "${CYAN}?${RESET} $prompt ${DIM}(auto-Y)${RESET}\n"
    yn="$def"
  else
    printf "${CYAN}?${RESET} $prompt"
    read -r yn || yn="$def"
  fi
  yn="$(echo "$yn" | tr '[:upper:]' '[:lower:]')"
  if [ -z "$yn" ]; then yn="$(echo "$def" | tr '[:upper:]' '[:lower:]')"; fi
  [ "$yn" = "y" ] || [ "$yn" = "yes" ]
}

banner
detect_os
echo

TOTAL_EST="2–5 min"
if ! need_cmd cargo; then TOTAL_EST="4–8 min (includes Rust)"; fi
if [ "$ARCH" = "aarch64" ]; then TOTAL_EST="$TOTAL_EST (Pi/ARM slower)"; fi
if [ $IS_TERMUX -eq 1 ]; then TOTAL_EST="3–6 min on phone"; fi

echo -e "${BOLD}This will (auto, no typing needed):${RESET}"
echo -e "  1. Install Rust (if missing) — ${DIM}~1-2 min${RESET}"
echo -e "  2. Download NekoLib — ${DIM}~10s${RESET}"
echo -e "  3. Build for YOUR CPU — ${DIM}~1-3 min${RESET}"
echo -e "  4. Create wallet + start mining — ${DIM}immediate${RESET}"
echo -e "\n${DIM}Estimated: $TOTAL_EST • Disk: ~15 MB + 1 MB chain • RAM ~30 MB${RESET}"
echo -e "${DIM}Works on: x86_64 Linux/Windows(WSL)/macOS, ARM64 Pi, Android Termux (one-tap)${RESET}"
echo -e "${DIM}Just wait 3 sec at each [Y/n] — it auto-continues. No typing needed!${RESET}\n"

# Non-interactive: if piped, auto-continue; if tty, still give 3s to cancel
if ! ask "Install NekoLib now?" Y; then
  echo -e "\n${YELLOW}Cancelled.${RESET} Re-run: curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash"
  exit 0
fi

step "1/4 Checking dependencies"
# Termux fast path — pkg install is more reliable than rustup on Android
if [ $IS_TERMUX -eq 1 ]; then
  if ! need_cmd cargo; then
    info "Termux detected — installing Rust via pkg (faster, no rustup)..."
    pkg update -y && pkg install -y rust git termux-tools clang 2>&1 | tail -n 5
    ok "Rust via pkg: $(rustc --version 2>&1 | head -n1)"
  fi
  if ! need_cmd git; then pkg install -y git curl; fi
else
  # normal Linux/macOS
  if ! need_cmd git; then
    if need_cmd apt-get; then sudo apt-get update && sudo apt-get install -y git curl build-essential
    elif need_cmd pacman; then sudo pacman -Sy --noconfirm git curl base-devel
    elif need_cmd dnf; then sudo dnf install -y git curl gcc
    elif need_cmd yum; then sudo yum install -y git curl gcc
    elif need_cmd brew; then brew install git curl
    else err "Install git & curl manually"; exit 1; fi
  fi
  ok "git & curl OK"
fi

if ! need_cmd cargo; then
  step "Installing Rust (stable) — auto"
  info "Downloading rustup (~200 MB)..."
  # auto without asking — for true plug-and-play
  if [ $IS_TERMUX -eq 0 ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  if need_cmd cargo; then ok "Rust installed $(cargo --version)"; else err "Rust install failed — try: pkg install rust (Termux) or visit https://rustup.rs"; exit 1; fi
else
  ok "Rust $(cargo --version 2>&1 | head -n1) already installed"
fi
export PATH="$HOME/.cargo/bin:$PATH"

step "2/4 Downloading NekoLib"
DEST="${1:-$HOME/nekolib}"
REPO="https://github.com/thisisforlearn/nekolib.git"
if [ -d "$DEST/.git" ]; then
  info "Updating $DEST"
  git -C "$DEST" pull --ff-only 2>&1 | tail -n 3 || warn "pull failed, using existing"
else
  [ -d "$DEST" ] && [ -n "$(ls -A "$DEST" 2>/dev/null)" ] && DEST="${DEST}-fresh" && warn "Using $DEST"
  git clone "$REPO" "$DEST"
  ok "Cloned to $DEST"
fi
cd "$DEST"

step "3/4 Building (optimized for your CPU) — please wait"
info "RUSTFLAGS=-C target-cpu=native cargo build --release"
info "Heavy part ☕ ETA 60–180s (phone a bit longer) — building..."
if RUSTFLAGS="-C target-cpu=native" cargo build --release; then
  ok "Build done! $DEST/target/release/nekod ($(du -h target/release/nekod 2>&1 | cut -f1 | head -n1))"
else
  err "Build failed. Try termux: pkg update && pkg install rust clang -y, then re-run."
  err "Or see README For Nerds → Fix common bugs"
  exit 1
fi

step "4/4 Wallet & chain"
if [ ! -f "$DEST/nekodata/wallet.json" ]; then
  "$DEST/target/release/nekod" wallet 2>&1 | tail -n 20 || true
else
  ok "Wallet exists $DEST/nekodata/wallet.json"
  "$DEST/target/release/nekod" info 2>&1 | head -n 35 || true
fi

echo -e "\n${MAG}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "${BOLD}${GREEN} NekoLib ready!${RESET}"
echo -e "${DIM}Binary:${RESET} $DEST/target/release/nekod"
echo -e "${DIM}Data:${RESET}   $DEST/nekodata"
echo -e "${DIM}Cap:${RESET}    100k neko ever, harder every 1k"
echo

# auto-start mining without asking if piped (true plug-and-play)
if [ ! -t 0 ]; then
  info "Auto-starting mining in 2s (press Ctrl+C to stop later)..."
  sleep 2
  "$DEST/target/release/nekod" start --mine || true
elif ask "Start mining now? (50 neko/block)" Y; then
  echo -e "\n${YELLOW}Mining... press Enter to stop (Ctrl+C)${RESET}"
  echo -e "${DIM}Tip: new terminal: $DEST/target/release/nekod info${RESET}\n"
  "$DEST/target/release/nekod" start --mine || true
else
  echo -e "\n${BOLD}Quick commands:${RESET}"
  echo -e "  ${CYAN}cd $DEST && ./target/release/nekod wallet${RESET}  ${DIM}# new wallet${RESET}"
  echo -e "  ${CYAN}./target/release/nekod start --mine${RESET}       ${DIM}# mine${RESET}"
  echo -e "  ${CYAN}./target/release/nekod info${RESET}              ${DIM}# supply${RESET}"
fi
echo -e "\n${GREEN}Done!${RESET} ${DIM}https://github.com/thisisforlearn/nekolib — GPLv3 Vaibhav${RESET}"

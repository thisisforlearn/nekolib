#!/usr/bin/env bash
# NekoLib — Plug & Play Installer (ZERO typing, bottom progress bar)
# Supports: x86_64 Linux, ARM64 Pi, macOS, WSL, Termux (Android)
# Author: Vaibhav — GPLv3 — ultimate power: Vaibhav
set -e

# colors (force on even when piped for beauty)
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'
MAG='\033[0;35m'; BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'
ok() { echo -e "${GREEN}✓${RESET} $*"; }
warn() { echo -e "${YELLOW}⚠${RESET} $*"; }
err() { echo -e "${RED}✗${RESET} $*"; }
info() { echo -e "${CYAN}●${RESET} $*"; }
step() { echo -e "\n${MAG}━━ $* ━━${RESET}"; }

# --- bottom progress bar (always visible) ---
BAR_WIDTH=28
PROG_CUR=0
PROG_TOT=4
draw_bar() {
  local cur=$1 tot=$2 msg="$3"
  local pct=$(( cur * 100 / tot ))
  local filled=$(( cur * BAR_WIDTH / tot ))
  local empty=$(( BAR_WIDTH - filled ))
  local bar="$(printf '█%.0s' $(seq 1 $filled 2>/dev/null) 2>/dev/null)$(printf '░%.0s' $(seq 1 $empty 2>/dev/null) 2>/dev/null)"
  # save cursor, move to bottom, draw, restore
  # use tput if available, else simple line
  if command -v tput >/dev/null 2>&1 && [ -t 1 ]; then
    tput sc 2>/dev/null; tput cup $(tput lines 2>/dev/null || echo 999) 0 2>/dev/null
    printf "${DIM}─${RESET}${MAG} %s ${RESET}${DIM}[%s] %d%% %s${RESET}\033[K" "$bar" "$cur/$tot" "$pct" "$msg" 2>/dev/null
    tput rc 2>/dev/null
  else
    # fallback: inline progress line (still visible)
    printf "\r${DIM}[%s] %d%% %s${RESET}   \n" "$bar" "$pct" "$msg"
  fi
}
progress() { PROG_CUR=$1; draw_bar "$1" "$PROG_TOT" "$2"; }
clear_bar() { if command -v tput >/dev/null 2>&1 && [ -t 1 ]; then tput cup $(tput lines 2>/dev/null || echo 999) 0 2>/dev/null; printf "\033[K" 2>/dev/null; tput rc 2>/dev/null; fi; }

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
  if [ -n "${TERMUX_VERSION:-}" ] || [ -d "/data/data/com.termux" ] || echo "${PREFIX:-}" | grep -q termux 2>/dev/null; then IS_TERMUX=1; fi
  echo -e "${DIM}Detected: $OS $ARCH $([ $IS_TERMUX -eq 1 ] && echo "(Termux)")${RESET}"
}

need_cmd() { command -v "$1" >/dev/null 2>&1; }

ask() {
  local prompt="$1" def="${2:-Y}"
  local yn
  if [ "$def" = "Y" ]; then prompt="$prompt [Y/n]: "; else prompt="$prompt [y/N]: "; fi
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

echo -e "${BOLD}This will (auto, no typing):${RESET}"
echo -e "  1. Install Rust (if missing)  ${DIM}~1-2 min${RESET}"
echo -e "  2. Download NekoLib           ${DIM}~10s${RESET}"
echo -e "  3. Build for YOUR CPU         ${DIM}~1-3 min${RESET}"
echo -e "  4. Create wallet + mine       ${DIM}immediate${RESET}"
echo -e "\n${DIM}Estimated: $TOTAL_EST • Disk: ~15 MB + 1 MB chain • RAM ~30 MB${RESET}"
echo -e "${DIM}All: x86_64 Linux/Win(WSL)/macOS, ARM64 Pi, Termux — one-tap${RESET}\n"

progress 0 "starting..."
sleep 0.3

if ! ask "Install NekoLib now?" Y; then
  clear_bar
  echo -e "\n${YELLOW}Cancelled.${RESET} Re-run: curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash"
  exit 0
fi

# ensure bottom bar stays during long ops: trap to redraw
step "1/4 Checking dependencies"
progress 1 "checking deps..."
# --- Termux fast path: DO NOT pkg update (15 min mirror scan) ---
if [ $IS_TERMUX -eq 1 ]; then
  if ! need_cmd cargo; then
    info "Termux — installing Rust via pkg (fast, no full upgrade)..."
    # DO NOT run pkg update (scans 20 mirrors 15 min). Just install directly.
    # Use apt directly with one mirror if needed, but pkg install already fetches lists fast if not updating.
    if ! pkg install -y rust git clang termux-tools 2>&1 | tail -n 15; then
      warn "Direct install failed, trying single apt update..."
      # pick already-chosen mirror, update only that one quickly
      apt update -o Acquire::Retries=2 2>&1 | tail -n 10 || true
      pkg install -y rust git clang termux-tools 2>&1 | tail -n 10
    fi
    if need_cmd cargo; then ok "Rust via pkg: $(rustc --version 2>&1 | head -n1)"; fi
  fi
  # git may already be there from pkg's deps
  if ! need_cmd git; then pkg install -y git curl 2>&1 | tail -n 5 || true; fi
else
  if ! need_cmd git; then
    if need_cmd apt-get; then sudo apt-get update -qq && sudo apt-get install -y git curl build-essential -qq
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
  progress 1 "installing Rust..."
  info "Downloading rustup (~200 MB)..."
  if [ $IS_TERMUX -eq 0 ]; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable --profile minimal
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env" 2>/dev/null || export PATH="$HOME/.cargo/bin:$PATH"
  fi
  export PATH="$HOME/.cargo/bin:$PATH"
  if need_cmd cargo; then ok "Rust installed $(cargo --version 2>&1 | head -n1)"; else err "Rust failed — Termux: pkg install rust, else https://rustup.rs"; exit 1; fi
else
  ok "Rust $(cargo --version 2>&1 | head -n1) already installed"
fi
export PATH="$HOME/.cargo/bin:$PATH"
progress 2 "deps done..."

step "2/4 Downloading NekoLib"
progress 2 "downloading..."
DEST="${1:-$HOME/nekolib}"
REPO="https://github.com/thisisforlearn/nekolib.git"
if [ -d "$DEST/.git" ]; then
  info "Updating $DEST"
  git -C "$DEST" pull --ff-only 2>&1 | tail -n 3 || warn "pull failed, using existing"
else
  [ -d "$DEST" ] && [ -n "$(ls -A "$DEST" 2>/dev/null)" ] && DEST="${DEST}-fresh" && warn "Using $DEST"
  git clone "$REPO" "$DEST" --depth 1
  ok "Cloned to $DEST"
fi
cd "$DEST"
progress 3 "downloaded..."

step "3/4 Building (optimized for your CPU) — please wait"
progress 3 "building... (60–180s)"
info "RUSTFLAGS=-C target-cpu=native cargo build --release"
info "Heavy part ☕ — building..."
# show build with inline progress: cargo build outputs, bottom bar stays via progress() before
if RUSTFLAGS="-C target-cpu=native" cargo build --release; then
  ok "Build done! $DEST/target/release/nekod ($(du -h target/release/nekod 2>&1 | cut -f1 | head -n1))"
else
  err "Build failed. Termux: pkg install rust clang -y, then re-run."
  err "Or see README For Nerds → Fix common bugs"
  exit 1
fi
progress 4 "built!"

step "4/4 Wallet & chain"
progress 4 "wallet..."
if [ ! -f "$DEST/nekodata/wallet.json" ]; then
  "$DEST/target/release/nekod" wallet 2>&1 | tail -n 25 || true
else
  ok "Wallet exists $DEST/nekodata/wallet.json"
  "$DEST/target/release/nekod" info 2>&1 | head -n 40 || true
fi

clear_bar
echo -e "\n${MAG}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "${BOLD}${GREEN} NekoLib ready!${RESET}"
echo -e "${DIM}Binary:${RESET} $DEST/target/release/nekod"
echo -e "${DIM}Data:${RESET}   $DEST/nekodata"
echo -e "${DIM}Cap:${RESET}    100k neko ever, harder every 1k"
echo

if [ ! -t 0 ]; then
  info "Auto-starting mining in 2s (Ctrl+C to stop)..."
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
clear_bar
echo -e "\n${GREEN}Done!${RESET} ${DIM}https://github.com/thisisforlearn/nekolib — GPLv3 Vaibhav${RESET}"

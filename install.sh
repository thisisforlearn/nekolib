#!/usr/bin/env bash
# NekoLib — Plug & Play Installer
# Supports: x86_64 Linux, ARM64 Linux (Pi/SBC), macOS (Intel/Apple Silicon), WSL, Termux (Android)
# Author: Vaibhav — GPLv3, ultimate authority
set -e

# --- colors & helpers ---
if [ -t 1 ]; then
  RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[0;33m'; CYAN='\033[0;36m'
  MAG='\033[0;35m'; BOLD='\033[1m'; DIM='\033[2m'; RESET='\033[0m'
else
  RED='';.GREEN='';YELLOW='';CYAN='';MAG='';BOLD='';DIM='';RESET=''
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
echo -e "${DIM}  Pure-CPU L1 • BLAKE3 SIMD • 100k cap • bump every 1k tokens${RESET}"
echo -e "${DIM}  v0.1.0 — by Vaibhav (GPLv3, ultimate authority: Vaibhav)${RESET}\n"
}

detect_os() {
  OS="$(uname -s)"
  ARCH="$(uname -m)"
  case "$ARCH" in
    x86_64|amd64) ARCH="x86_64" ;;
    aarch64|arm64) ARCH="aarch64" ;;
    armv7l) ARCH="armv7" ;;
    *) ARCH="unknown" ;;
  esac
  echo -e "${DIM}Detected: $OS $ARCH${RESET}"
}

need_cmd() { command -v "$1" >/dev/null 2>&1; }

ask() {
  local prompt="$1" def="${2:-Y}"
  local yn
  if [ "$def" = "Y" ]; then prompt="$prompt [Y/n]: "; else prompt="$prompt [y/N]: "; fi
  printf "${CYAN}?${RESET} $prompt"
  read -r yn || yn="$def"
  yn="$(echo "$yn" | tr '[:upper:]' '[:lower:]')"
  if [ -z "$yn" ]; then yn="$(echo "$def" | tr '[:upper:]' '[:lower:]')"; fi
  [ "$yn" = "y" ] || [ "$yn" = "yes" ]
}

# --- start ---
banner
detect_os
echo

# estimated time
TOTAL_EST="2–5 min"
if ! need_cmd cargo; then TOTAL_EST="4–8 min (includes Rust install)"; fi
if [ "$ARCH" = "aarch64" ]; then TOTAL_EST="$TOTAL_EST (Pi/ARM a bit slower)"; fi

echo -e "${BOLD}This will:${RESET}"
echo -e "  1. Install Rust (if missing) — ${DIM}~1-2 min${RESET}"
echo -e "  2. Download NekoLib — ${DIM}~10s${RESET}"
echo -e "  3. Build optimized binary (RUSTFLAGS=-C target-cpu=native) — ${DIM}~1-3 min${RESET}"
echo -e "  4. Create wallet + start mining — ${DIM}immediate${RESET}"
echo -e "\n${DIM}Estimated total: $TOTAL_EST • Disk: ~15 MB + ~1 MB chain • RAM: ~30 MB${RESET}"
echo -e "${DIM}All systems: x86_64 Linux/Windows(WSL)/macOS, ARM64 Pi/SBC, Android Termux, iOS (Xcode)${RESET}\n"

if ! ask "Install NekoLib now?" Y; then
  echo -e "\n${YELLOW}Cancelled. Run again when ready:${RESET} curl -fsSL https://raw.githubusercontent.com/thisisforlearn/nekolib/main/install.sh | bash"
  exit 0
fi

step "1/4 Checking dependencies"
# git, curl
if ! need_cmd git; then
  if need_cmd apt-get; then sudo apt-get update && sudo apt-get install -y git curl build-essential
  elif need_cmd pacman; then sudo pacman -Sy --noconfirm git curl base-devel
  elif need_cmd dnf; then sudo dnf install -y git curl gcc
  elif need_cmd yum; then sudo yum install -y git curl gcc
  elif need_cmd brew; then brew install git curl
  else err "Please install git & curl manually"; exit 1; fi
fi
ok "git & curl OK"

if ! need_cmd cargo; then
  step "Installing Rust (stable)"
  if ask "Install Rust via rustup? (official, ~200 MB)" Y; then
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # shellcheck disable=SC1090
    source "$HOME/.cargo/env" || export PATH="$HOME/.cargo/bin:$PATH"
    ok "Rust installed $(rustc --version)"
  else
    err "Rust is required. Install from https://rustup.rs and re-run."
    exit 1
  fi
else
  ok "Rust $(rustc --version) already installed"
fi
export PATH="$HOME/.cargo/bin:$PATH"

# also ensure build tools on Termux
if need_cmd pkg && [ -n "${TERMUX_VERSION:-}" ]; then
  pkg install -y rust git termux-tools 2>&1 | tail -n 5
fi

step "2/4 Downloading NekoLib"
DEST="${1:-$HOME/nekolib}"
REPO="https://github.com/thisisforlearn/nekolib.git"
if [ -d "$DEST/.git" ]; then
  info "Updating existing $DEST"
  git -C "$DEST" pull --ff-only || warn "pull failed, using existing"
else
  if [ -d "$DEST" ] && [ -n "$(ls -A "$DEST" 2>/dev/null)" ]; then
    warn "$DEST not empty, cloning to ${DEST}-fresh"
    DEST="${DEST}-fresh"
  fi
  git clone "$REPO" "$DEST"
  ok "Cloned to $DEST"
fi
cd "$DEST"

step "3/4 Building (optimized for your CPU)"
info "RUSTFLAGS=-C target-cpu=native cargo build --release"
info "This is the heavy part — grab a coffee ☕ (ETA 60–180s)"
if RUSTFLAGS="-C target-cpu=native" cargo build --release; then
  ok "Build done! Binary: $DEST/target/release/nekod ($(du -h target/release/nekod | cut -f1))"
else
  err "Build failed. See above. Common fixes in README 'For Nerds → Fix common bugs'"
  exit 1
fi

step "4/4 Wallet & chain"
if [ ! -f "$DEST/nekodata/wallet.json" ]; then
  "$DEST/target/release/nekod" wallet || true
else
  ok "Wallet exists at $DEST/nekodata/wallet.json"
  "$DEST/target/release/nekod" info 2>&1 | head -n 30 || true
fi

echo
echo -e "${MAG}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${RESET}"
echo -e "${BOLD}${GREEN} NekoLib ready!${RESET}"
echo -e "${DIM}Binary:${RESET} $DEST/target/release/nekod"
echo -e "${DIM}Data:${RESET}   $DEST/nekodata  (1–100 MB, pruning on)"
echo -e "${DIM}Cap:${RESET}    100,000 neko ever, harder every 1,000"
echo

if ask "Start mining now? (CPU, 50 neko/block)" Y; then
  echo -e "\n${YELLOW}Mining... press Enter to stop (or Ctrl+C)${RESET}"
  echo -e "${DIM}Tip: open another terminal and run: $DEST/target/release/nekod info${RESET}\n"
  # run in foreground so user sees colorful logs
  "$DEST/target/release/nekod" start --mine || true
else
  echo
  echo -e "${BOLD}Quick commands (copy-paste):${RESET}"
  echo -e "  ${CYAN}cd $DEST && ./target/release/nekod wallet${RESET}      ${DIM}# new wallet${RESET}"
  echo -e "  ${CYAN}./target/release/nekod start --mine${RESET}            ${DIM}# mine (50/block, colorful)${RESET}"
  echo -e "  ${CYAN}./target/release/nekod info${RESET}                   ${DIM}# height, supply, utxos${RESET}"
  echo -e "  ${CYAN}./target/release/nekod bench${RESET}                  ${DIM}# ~13M H/s test${RESET}"
  echo
  echo -e "${DIM}Docs: $DEST/README.md (easy + nerd guide) • License: GPLv3, author Vaibhav (ultimate power)${RESET}"
fi

echo -e "\n${GREEN}Done!${RESET} ${DIM}Share: https://github.com/thisisforlearn/nekolib${RESET}"

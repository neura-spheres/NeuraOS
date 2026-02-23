#!/usr/bin/env bash
# =============================================================================
#   NeuraOS · Setup Script
#   Makes sure your machine has everything it needs before you try to build.
#   If something's missing, it'll walk you through exactly what to do.
# =============================================================================

set -euo pipefail

# ── Tokyo Night color palette ─────────────────────────────────────────────────
R='\033[0m'
BOLD='\033[1m'
DIM='\033[2m'
ACCENT='\033[38;2;122;162;247m'    # #7aa2f7  blue
FG='\033[38;2;192;202;245m'        # #c0caf5
MUTED='\033[38;2;130;140;170m'     # #828caa
OK='\033[38;2;158;206;106m'        # #9ece6a  green
WARN='\033[38;2;224;175;104m'      # #e0af68  orange
ERR='\033[38;2;247;118;142m'       # #f7768e  red
CYAN='\033[38;2;125;207;255m'      # #7dcfff
PURPLE='\033[38;2;187;154;247m'    # #bb9af7

RUST_MIN_VERSION="1.85"
RUSTUP_URL="https://rustup.rs"
REPO_URL="https://github.com/neura-spheres/NeuraOS"

MISSING_DEPS=0
RUST_OK=0

# ── Helpers ───────────────────────────────────────────────────────────────────

banner() {
    echo
    echo -e "${ACCENT}${BOLD}  ╭─────────────────────────────────────────────────────╮${R}"
    echo -e "${ACCENT}${BOLD}  │                                                     │${R}"
    echo -e "${ACCENT}${BOLD}  │   ███╗  ██╗███████╗██╗   ██╗██████╗  █████╗         │${R}"
    echo -e "${ACCENT}${BOLD}  │   ████╗ ██║██╔════╝██║   ██║██╔══██╗██╔══██╗        │${R}"
    echo -e "${ACCENT}${BOLD}  │   ██╔██╗██║█████╗  ██║   ██║██████╔╝███████║        │${R}"
    echo -e "${ACCENT}${BOLD}  │   ██║╚████║██╔══╝  ██║   ██║██╔══██╗██╔══██║        │${R}"
    echo -e "${ACCENT}${BOLD}  │   ██║ ╚███║███████╗╚██████╔╝██║  ██║██║  ██║        │${R}"
    echo -e "${ACCENT}${BOLD}  │   ╚═╝  ╚══╝╚══════╝ ╚═════╝ ╚═╝  ╚═╝╚═╝  ╚═╝        │${R}"
    echo -e "${ACCENT}${BOLD}  │                                                     │${R}"
    echo -e "${MUTED}${BOLD}  │   setup script  ·  the AI-native terminal OS        │${R}"
    echo -e "${ACCENT}${BOLD}  ╰─────────────────────────────────────────────────────╯${R}"
    echo
}

section() {
    local title="$1"
    local pad=$(( 48 - ${#title} ))
    local line
    line=$(printf '─%.0s' $(seq 1 $pad))
    echo
    echo -e "${MUTED}${BOLD}  ─── ${ACCENT}${title}${MUTED} ${line}${R}"
    echo
}

check_ok()   { echo -e "  ${OK}${BOLD}  ✓  ${R}  ${FG}$1${R}"; }
check_warn() { echo -e "  ${WARN}${BOLD}  !  ${R}  ${WARN}$1${R}"; }
check_err()  { echo -e "  ${ERR}${BOLD}  ✗  ${R}  ${ERR}$1${R}"; }
info()       { echo -e "       ${MUTED}$1${R}"; }
link()       { echo -e "       ${CYAN}→  $1${R}"; }
cmd_hint()   { echo -e "       ${PURPLE}$  ${FG}$1${R}"; }
step_num()   { echo -e "  ${PURPLE}${BOLD}  [$1]${R}  ${FG}$2${R}"; }
divider()    { echo -e "  ${MUTED}$(printf '·%.0s' {1..52})${R}"; }

version_gte() {
    # Returns true if version $1 >= $2
    [ "$(printf '%s\n' "$1" "$2" | sort -V | head -1)" = "$2" ]
}

# ── Banner ────────────────────────────────────────────────────────────────────
banner

# ── Section 1: System checks ──────────────────────────────────────────────────
section "Checking Requirements"

# Check: git
if command -v git &>/dev/null; then
    GIT_VER=$(git --version | awk '{print $3}')
    check_ok "git ${GIT_VER}"
else
    check_err "git is not installed"
    info "You need git to clone the repo. Grab it from:"
    link "https://git-scm.com/downloads"
    MISSING_DEPS=$(( MISSING_DEPS + 1 ))
fi

# Check: rustc + cargo
if command -v rustc &>/dev/null && command -v cargo &>/dev/null; then
    RUST_VER=$(rustc --version | awk '{print $2}')
    if version_gte "$RUST_VER" "$RUST_MIN_VERSION"; then
        check_ok "rustc ${RUST_VER}   (min required: ${RUST_MIN_VERSION} — you're good)"
        check_ok "cargo $(cargo --version | awk '{print $2}')"
        RUST_OK=1
    else
        check_warn "rustc ${RUST_VER}  ← too old, need ≥ ${RUST_MIN_VERSION}"
        info "Your Rust is a bit behind. Just run this to update:"
        cmd_hint "rustup update stable"
        MISSING_DEPS=$(( MISSING_DEPS + 1 ))
    fi
else
    check_err "Rust is not installed"
    RUST_OK=0
    MISSING_DEPS=$(( MISSING_DEPS + 1 ))
fi

# Check: build essentials (cc linker)
if command -v cc &>/dev/null || command -v gcc &>/dev/null || command -v clang &>/dev/null; then
    CC_BIN=$(command -v cc || command -v gcc || command -v clang)
    check_ok "C linker found at ${CC_BIN}"
else
    check_warn "No C linker (cc/gcc/clang) found"
    info "You might need build-essential (Debian/Ubuntu) or Xcode CLI tools (macOS):"
    case "$(uname)" in
        Darwin)  cmd_hint "xcode-select --install" ;;
        Linux)   cmd_hint "sudo apt install build-essential  # Debian/Ubuntu"
                 echo
                 cmd_hint "sudo dnf groupinstall 'Development Tools'  # Fedora" ;;
    esac
    MISSING_DEPS=$(( MISSING_DEPS + 1 ))
fi

echo

# ── Section 2: Rust install guide (only if needed) ────────────────────────────
if [ "$RUST_OK" -eq 0 ]; then
    section "How to Install Rust"

    echo -e "  ${FG}Rust uses a tool called ${ACCENT}${BOLD}rustup${R}${FG} to manage everything. It's basically${R}"
    echo -e "  ${FG}the official way to install Rust and it keeps it up to date too.${R}"
    echo -e "  ${FG}Honestly the whole process takes like 3-5 minutes tops.${R}"
    echo

    divider
    echo

    step_num "1" "Install rustup  (this installs Rust + Cargo automatically)"
    echo
    cmd_hint "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo
    info "That one command does everything. It'll ask you to press Enter to"
    info "confirm the default install — just go with it, defaults are fine."
    info "Official site if you want to read more before running it:"
    link "${RUSTUP_URL}"
    echo

    divider
    echo

    step_num "2" "Reload your shell so the 'cargo' and 'rustc' commands work"
    echo
    cmd_hint "source \"\$HOME/.cargo/env\""
    echo
    info "Or just close and reopen your terminal — same effect."
    echo

    divider
    echo

    step_num "3" "Verify everything installed correctly"
    echo
    cmd_hint "rustc --version"
    cmd_hint "cargo --version"
    echo
    info "You should see something like:  rustc ${RUST_MIN_VERSION}.x (xxxxxxx 20xx-xx-xx)"
    echo

    divider
    echo

    step_num "4" "Come back and run this script again to build NeuraOS"
    echo
    cmd_hint "bash scripts/setup.sh"
    echo

    echo -e "  ${MUTED}──  More info / troubleshooting  ──────────────────────────────────${R}"
    echo
    info "Full Rust installation guide:"
    link "https://doc.rust-lang.org/book/ch01-01-installation.html"
    echo
    info "Linux-specific notes (some distros need extra packages):"
    link "https://forge.rust-lang.org/infra/other-installation-methods.html"
    echo
    info "macOS users: if curl fails, try installing via Homebrew instead:"
    link "https://formulae.brew.sh/formula/rustup"
    echo

    echo -e "  ${WARN}${BOLD}  →  Re-run this script after installing Rust to continue.${R}"
    echo
    exit 1
fi

# ── Section 3: All good — let's build ─────────────────────────────────────────
if [ "$MISSING_DEPS" -gt 0 ]; then
    section "Almost There"
    echo -e "  ${WARN}Fix the issues above and re-run: ${CYAN}bash scripts/setup.sh${R}"
    echo
    exit 1
fi

# final sanity check: do a quick build to ensure the linker/toolchain actually works.
section "Verifying build environment"
echo -e "  ${MUTED}Running 'cargo check --quiet' – this may take a minute...${R}"
if ! cargo check --quiet; then
    check_err "build verification failed"
    info "Your Rust toolchain or C linker might still be misconfigured."
    info "Try opening a fresh shell or installing any missing build tools, then re-run this script."
    exit 1
else
    check_ok "verification build succeeded"
fi

section "Everything Looks Good"

echo -e "  ${OK}${BOLD}All requirements satisfied.${R} ${FG}Let's build NeuraOS.${R}"
echo

divider
echo

echo -e "  ${FG}Building in ${ACCENT}${BOLD}release mode${R}${FG} — this compiles everything fully optimized.${R}"
echo -e "  ${FG}First build takes a few minutes (Rust compiles all dependencies too).${R}"
echo -e "  ${FG}Subsequent builds are much faster because of caching.${R}"
echo

# Ask user if they want to build now
if [ -t 0 ]; then
    echo -ne "  ${ACCENT}${BOLD}Build now? [Y/n] ${R}"
    read -r REPLY
    echo
    REPLY="${REPLY:-Y}"
else
    REPLY="Y"
fi

if [[ "$REPLY" =~ ^[Yy]$ ]]; then
    section "Building NeuraOS"
    echo -e "  ${MUTED}Running: ${PURPLE}cargo build --release${R}"
    echo
    cargo build --release
    echo
    section "Done"
    echo -e "  ${OK}${BOLD}  ✓  ${R}  ${FG}Build complete! Run NeuraOS with:${R}"
    echo
    cmd_hint "./target/release/neuraos"
    echo
    echo -e "  ${MUTED}Or with cargo:${R}"
    cmd_hint "cargo run --release"
    echo
    echo -e "  ${FG}Once it boots up:${R}"
    echo -e "  ${MUTED}  ·  Type ${FG}help${MUTED} to see all shell commands${R}"
    echo -e "  ${MUTED}  ·  Press ${FG}Ctrl+P${MUTED} to open the app launcher${R}"
    echo -e "  ${MUTED}  ·  Open Settings to drop in your AI API key${R}"
    echo
    echo -e "  ${ACCENT}${BOLD}Enjoy NeuraOS. Drop a ⭐ on GitHub if you like it!${R}"
    link "${REPO_URL}"
    echo
else
    section "Ready to Build Whenever You Are"
    echo -e "  ${FG}Run this when you're ready:${R}"
    echo
    cmd_hint "cargo build --release"
    echo
    cmd_hint "./target/release/neuraos"
    echo
fi

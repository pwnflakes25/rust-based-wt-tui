#!/usr/bin/env bash
set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

info()  { printf "${BOLD}%s${NC}\n" "$*"; }
ok()    { printf "${GREEN}%s${NC}\n" "$*"; }
warn()  { printf "${YELLOW}%s${NC}\n" "$*"; }
err()   { printf "${RED}error:${NC} %s\n" "$*" >&2; exit 1; }

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# --- Pre-checks -----------------------------------------------------------

command -v cargo >/dev/null 2>&1 || err "Rust toolchain not found. Install it from https://rustup.rs"
command -v git   >/dev/null 2>&1 || err "git is not installed."

# --- Build -----------------------------------------------------------------

info "Building wt (release)..."
cargo build --release

BINARY="$(pwd)/target/release/wt"
[ -f "$BINARY" ] || err "Build succeeded but binary not found at $BINARY"

# --- Install ---------------------------------------------------------------

mkdir -p "$INSTALL_DIR"
cp "$BINARY" "$INSTALL_DIR/wt"
chmod +x "$INSTALL_DIR/wt"
ok "Installed wt to $INSTALL_DIR/wt"

# --- PATH check ------------------------------------------------------------

if ! echo "$PATH" | tr ':' '\n' | grep -qx "$INSTALL_DIR"; then
    warn ""
    warn "$INSTALL_DIR is not in your PATH."
    warn "Add one of the following to your shell config (~/.zshrc or ~/.bashrc):"
    warn ""
    warn "  export PATH=\"$INSTALL_DIR:\$PATH\""
    warn ""
fi

# --- Shell integration -----------------------------------------------------

info ""
info "Shell integration (recommended):"
info "Add this to your ~/.zshrc or ~/.bashrc so 'wts' can cd into worktrees:"
info ""
echo '  wts() {'
echo '    local dir'
echo '    dir="$(wt switch "$@")" && cd "$dir"'
echo '  }'
info ""
info "Or run:  wt init  to print the snippet."

ok ""
ok "Done! Run 'wt --help' to get started."

#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"
DATA_DIR="${DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}}"
APP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"

INSTALL_SERVICE="auto"
SKIP_BUILD=0

usage() {
  cat <<'EOF'
Usage: ./install.sh [options]

Options:
  --prefix DIR        Install prefix (default: ~/.local)
  --with-service      Always install/restart systemd user service
  --no-service        Skip systemd user service installation
  --skip-build        Install already built binaries without rebuilding
  -h, --help          Show this help message
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --prefix)
      PREFIX="$2"
      BIN_DIR="$PREFIX/bin"
      shift 2
      ;;
    --with-service)
      INSTALL_SERVICE="yes"
      shift
      ;;
    --no-service)
      INSTALL_SERVICE="no"
      shift
      ;;
    --skip-build)
      SKIP_BUILD=1
      shift
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ "$SKIP_BUILD" -eq 0 ]]; then
  echo "[1/4] Building gemini-remote daemon (Go)..."
  (cd "$ROOT_DIR" && go build -o "$ROOT_DIR/gemini-remote" ./cmd/gemini-remote)

  echo "[2/4] Building gemini-ui desktop app (Rust release)..."
  cargo build --release --manifest-path "$ROOT_DIR/ui/Cargo.toml"
else
  echo "[1-2/4] Skipping build (--skip-build)"
fi

echo "[3/4] Installing binaries, icon, and .desktop entry..."
install -Dm755 "$ROOT_DIR/gemini-remote" "$BIN_DIR/gemini-remote"
install -Dm755 "$ROOT_DIR/ui/target/release/gemini-ui" "$BIN_DIR/gemini-ui"
install -Dm644 "$ROOT_DIR/assets/gemini-ui.svg" "$ICON_DIR/gemini-ui.svg"

mkdir -p "$APP_DIR"
sed "s|^Exec=.*|Exec=$BIN_DIR/gemini-ui|" "$ROOT_DIR/assets/gemini-ui.desktop" > "$APP_DIR/gemini-ui.desktop"
chmod 644 "$APP_DIR/gemini-ui.desktop"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "[4/4] Configuring systemd user service..."
SERVICE_FILE="$HOME/.config/systemd/user/gemini-remote.service"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/gemini-remote"
SHOULD_INSTALL_SERVICE=0

if [[ "$INSTALL_SERVICE" == "yes" ]]; then
  SHOULD_INSTALL_SERVICE=1
elif [[ "$INSTALL_SERVICE" == "auto" ]]; then
  if [[ -f "$SERVICE_FILE" || -f "$CONFIG_DIR/token.json" ]]; then
    SHOULD_INSTALL_SERVICE=1
  fi
fi

if [[ "$SHOULD_INSTALL_SERVICE" -eq 1 ]] && command -v systemctl >/dev/null 2>&1; then
  "$BIN_DIR/gemini-remote" service install
  systemctl --user restart gemini-remote.service || true
else
  echo "  Skipped systemd service auto-start (run 'gemini-remote login' and 'gemini-remote service install' after setup)."
fi

echo ""
echo "Installation complete:"
echo "  Daemon : $BIN_DIR/gemini-remote"
echo "  GUI    : $BIN_DIR/gemini-ui"
echo "  Desktop: $APP_DIR/gemini-ui.desktop"
echo "  Icon   : $ICON_DIR/gemini-ui.svg"
if [[ ":$PATH:" != *":$BIN_DIR:"* ]]; then
  echo ""
  echo "Note: $BIN_DIR is not in your PATH. Add it to your shell profile to run 'gemini-remote' and 'gemini-ui' directly."
fi

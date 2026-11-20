#!/usr/bin/env bash
set -euo pipefail

PREFIX="${PREFIX:-$HOME/.local}"
BIN_DIR="${BIN_DIR:-$PREFIX/bin}"
DATA_DIR="${DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}}"
APP_DIR="$DATA_DIR/applications"
ICON_DIR="$DATA_DIR/icons/hicolor/scalable/apps"

if [[ -x "$BIN_DIR/gemini-remote" ]] && command -v systemctl >/dev/null 2>&1; then
  "$BIN_DIR/gemini-remote" service uninstall >/dev/null 2>&1 || true
fi

rm -f \
  "$BIN_DIR/gemini-remote" \
  "$BIN_DIR/gemini-ui" \
  "$APP_DIR/gemini-ui.desktop" \
  "$ICON_DIR/gemini-ui.svg"

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$APP_DIR" >/dev/null 2>&1 || true
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$DATA_DIR/icons/hicolor" >/dev/null 2>&1 || true
fi

echo "Uninstalled gemini-remote, gemini-ui, desktop entry, and icon."
echo "User configuration under ~/.config/gemini-remote was preserved."

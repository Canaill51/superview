#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="$ROOT_DIR/superview-gui"
ICON_SOURCE="$ROOT_DIR/Icon.png"
APP_ID="com.canaill51.superview"

if [[ ! -f "$BIN_PATH" ]]; then
  echo "Missing binary: $BIN_PATH"
  echo "Build it first with: go build superview-gui.go"
  exit 1
fi

if [[ ! -f "$ICON_SOURCE" ]]; then
  echo "Missing icon file: $ICON_SOURCE"
  exit 1
fi

ICON_BASE="$HOME/.local/share/icons/hicolor"
DESKTOP_DIR="$HOME/.local/share/applications"
DESKTOP_FILE="$DESKTOP_DIR/$APP_ID.desktop"

SIZES=(16 24 32 48 64 128 256 512)

mkdir -p "$DESKTOP_DIR"

# Install a proper icon theme set so desktop environments can resolve the icon.
python3 - "$ICON_SOURCE" "$ICON_BASE" "$APP_ID" <<'PY'
import sys
from pathlib import Path
from PIL import Image

src = Path(sys.argv[1])
base = Path(sys.argv[2])
app_id = sys.argv[3]
sizes = [16, 24, 32, 48, 64, 128, 256, 512]

img = Image.open(src).convert("RGBA")
for size in sizes:
  d = base / f"{size}x{size}" / "apps"
  d.mkdir(parents=True, exist_ok=True)
  out = d / f"{app_id}.png"
  icon = img.resize((size, size), resample=Image.Resampling.LANCZOS)
  icon.save(out, format="PNG", optimize=True)

pixmaps = Path.home() / ".local" / "share" / "pixmaps"
pixmaps.mkdir(parents=True, exist_ok=True)
img.resize((512, 512), resample=Image.Resampling.LANCZOS).save(
  pixmaps / f"{app_id}.png", format="PNG", optimize=True
)
PY

cat > "$DESKTOP_FILE" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Superview
Comment=Transform FPV videos into Superview format
Exec=$BIN_PATH
Icon=$APP_ID
Terminal=false
Categories=AudioVideo;Video;
StartupWMClass=Superview
EOF

chmod 644 "$DESKTOP_FILE"

echo "Installed launcher: $DESKTOP_FILE"
echo "Installed icons under: $ICON_BASE/*/apps/$APP_ID.png"
echo "Open Superview from your applications menu for taskbar icon integration."

if command -v gtk-update-icon-cache >/dev/null 2>&1; then
  gtk-update-icon-cache -f -t "$ICON_BASE" >/dev/null 2>&1 || true
fi

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "$DESKTOP_DIR" >/dev/null 2>&1 || true
fi

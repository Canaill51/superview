#!/bin/sh
# Superview's Linux AppImage: one file, run by double-click, nothing to install.
#
#   APPIMAGETOOL=path/to/appimagetool-x86_64.AppImage \
#     sh packaging/linux/build_appimage.sh <fyne-package.tar.xz> <dir with ffmpeg+ffprobe> <out.AppImage>
#
# The tarball is what `fyne package` writes. Only the binary is taken from it: it is the one
# that carries the version stamped by --app-version and the vcs.* build info the release
# checks. The icon is the repository's own.
#
# Layout inside the image:
#   usr/bin/superview            the application
#   usr/lib/superview/ffmpeg     the bundled tools. The application looks for them in
#   usr/lib/superview/ffprobe    ../lib/superview, relative to its own binary.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <fyne-package.tar.xz> <tools-dir> <out.AppImage>" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
TARBALL="$1"
TOOLS="$2"
OUT="$3"
TOOL="${APPIMAGETOOL:-appimagetool}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/Superview.AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib/superview" "$WORK/unpacked"

tar -xJf "$TARBALL" -C "$WORK/unpacked"
BIN="$(find "$WORK/unpacked" -type f -path '*/usr/local/bin/*' | head -n 1)"
if [ -z "$BIN" ]; then
  echo "error: the fyne package has no usr/local/bin/<binary>; its layout changed" >&2
  find "$WORK/unpacked" -maxdepth 4 >&2
  exit 1
fi
install -m 0755 "$BIN" "$APPDIR/usr/bin/superview"
install -m 0755 "$TOOLS/ffmpeg" "$APPDIR/usr/lib/superview/ffmpeg"
install -m 0755 "$TOOLS/ffprobe" "$APPDIR/usr/lib/superview/ffprobe"
cp "$ROOT/THIRD_PARTY_NOTICES.md" "$APPDIR/"

# Entry point: the image is mounted, then this script runs the application.
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/superview" "$@"
EOF
chmod 0755 "$APPDIR/AppRun"

# The file is named after the application ID the GUI passes to app.NewWithID, and Icon= after
# it too. On Wayland the window carries that ID as its app_id and the compositor finds the
# icon by looking for a desktop file of exactly that name: with `superview.desktop` KDE shows
# its generic icon instead of ours (measured, 2026-10-14). appimagetool wants the icon beside
# the desktop file under the name Icon= gives.
APP_ID="com.canaill51.superview"
cat > "$APPDIR/$APP_ID.desktop" <<EOF
[Desktop Entry]
Type=Application
Name=Superview
Comment=Convert 4:3 video to 16:9 by dynamic distortion
Exec=superview
Icon=$APP_ID
Terminal=false
Categories=AudioVideo;Video;
EOF
cp "$ROOT/cmd/superview/Icon.png" "$APPDIR/$APP_ID.png"
cp "$ROOT/cmd/superview/Icon.png" "$APPDIR/.DirIcon"

# APPIMAGE_EXTRACT_AND_RUN: appimagetool is itself an AppImage, run here without FUSE
# (GitHub's runners do not allow it).
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"

#!/bin/sh
# Superview's Linux AppImage: one file, run by double-click, nothing to install.
#
#   APPIMAGETOOL=path/to/appimagetool-x86_64.AppImage \
#     sh packaging/linux/build_appimage.sh <superview binary> <dir with ffmpeg+ffprobe> <out.AppImage>
#
# The binary is `target/release/superview`, built with SUPERVIEW_VERSION set (it carries
# the version and the build marker the release checks). The icon is the repository's own.
#
# Layout inside the image:
#   usr/bin/superview            the application
#   usr/lib/superview/ffmpeg     the bundled tools. The application looks for them in
#   usr/lib/superview/ffprobe    ../lib/superview, relative to its own binary.
#   usr/lib/libxkbcommon.so.0, libxkbcommon-x11.so.0, libxcb-xkb.so.1
#                                the libraries winit loads with dlopen on X11 and that a
#                                minimal system may lack (DEC-47). Copied from the build
#                                machine, so they must be installed there (libxkbcommon0, libxkbcommon-x11-0, libxcb-xkb1).
#   usr/share/doc/<package>/copyright   the licence texts of those libraries.
#
# All three travel together on purpose: libxkbcommon-x11 and libxkbcommon must be the same
# version. An embedded x11 1.6 loaded against a newer system libxkbcommon crashed at start
# (segfault, measured with the 24.04 image on a 1.13 system).
#
# How the application finds them, without LD_LIBRARY_PATH (AppRun must not set it: it would be
# inherited by ffmpeg, zenity and kdialog, and a Qt tool would load our copies): the binary is
# linked with a DT_RPATH of $ORIGIN/../lib (crates/superview-app/build.rs). A DT_RPATH,
# unlike a RUNPATH, also applies to the dependencies of a library loaded by dlopen, so
# libxkbcommon-x11 finds libxcb-xkb.so.1 beside itself. All measured with LD_DEBUG=libs.
set -eu

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <superview binary> <tools-dir> <out.AppImage>" >&2
  exit 2
fi

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
BIN="$1"
TOOLS="$2"
OUT="$3"
TOOL="${APPIMAGETOOL:-appimagetool}"
# Where the build machine keeps the two libraries (Debian/Ubuntu x86_64 by default).
LIBDIR="${SUPERVIEW_LIBDIR:-/usr/lib/x86_64-linux-gnu}"

if [ ! -f "$BIN" ]; then
  echo "error: $BIN is not a file; build it first (cargo build --release --locked -p superview-app)" >&2
  exit 1
fi
for lib in libxkbcommon.so.0 libxkbcommon-x11.so.0 libxcb-xkb.so.1; do
  if [ ! -e "$LIBDIR/$lib" ]; then
    echo "error: $LIBDIR/$lib is missing; the AppImage embeds it. Install libxkbcommon0, libxkbcommon-x11-0 and libxcb-xkb1 on the build machine" >&2
    exit 1
  fi
done

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
APPDIR="$WORK/Superview.AppDir"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib/superview"

install -m 0755 "$BIN" "$APPDIR/usr/bin/superview"
install -m 0755 "$TOOLS/ffmpeg" "$APPDIR/usr/lib/superview/ffmpeg"
install -m 0755 "$TOOLS/ffprobe" "$APPDIR/usr/lib/superview/ffprobe"
# cp -L: the real file, under the name the loader asks for (these are symlinks on the system).
for lib in libxkbcommon.so.0 libxkbcommon-x11.so.0 libxcb-xkb.so.1; do
  cp -L "$LIBDIR/$lib" "$APPDIR/usr/lib/$lib"
  chmod 0644 "$APPDIR/usr/lib/$lib"
done
# Their licence texts travel with them (THIRD_PARTY_NOTICES.md quotes them too).
for pkg in libxkbcommon0 libxkbcommon-x11-0 libxcb-xkb1; do
  if [ ! -f "/usr/share/doc/$pkg/copyright" ]; then
    echo "error: /usr/share/doc/$pkg/copyright is missing; the AppImage ships it beside the library" >&2
    exit 1
  fi
  install -D -m 0644 "/usr/share/doc/$pkg/copyright" "$APPDIR/usr/share/doc/$pkg/copyright"
done
cp "$ROOT/THIRD_PARTY_NOTICES.md" "$APPDIR/"

# Entry point: the image is mounted, then this script runs the application.
cat > "$APPDIR/AppRun" <<'EOF'
#!/bin/sh
HERE="$(dirname "$(readlink -f "$0")")"
exec "$HERE/usr/bin/superview" "$@"
EOF
chmod 0755 "$APPDIR/AppRun"

# The file is named after the application ID the window sets (slint::set_xdg_app_id), and Icon= after
# it too. On Wayland the window carries that ID as its app_id and the compositor finds the
# icon by looking for a desktop file of exactly that name (StartupWMClass does not help there:
# measured, 2026-10-14). The ID is the plain `superview` because the tools that integrate an
# AppImage (Gear Lever) write their own desktop file named after the application, `superview.desktop`:
# an ID of the form com.example.superview would never match it (DEC-64). appimagetool wants the
# icon beside the desktop file under the name Icon= gives.
APP_ID="superview"
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
cp "$ROOT/crates/superview-app/ui/Icon.png" "$APPDIR/$APP_ID.png"
cp "$ROOT/crates/superview-app/ui/Icon.png" "$APPDIR/.DirIcon"

# APPIMAGE_EXTRACT_AND_RUN: appimagetool is itself an AppImage, run here without FUSE
# (GitHub's runners do not allow it).
ARCH=x86_64 APPIMAGE_EXTRACT_AND_RUN=1 "$TOOL" --no-appstream "$APPDIR" "$OUT"

#!/bin/sh
# Proves that an AppImage runs on its own copies of libxkbcommon-x11 and libxcb-xkb (DEC-47).
#
#   sh packaging/linux/check_appimage_libs.sh <image.AppImage> [seconds]
#
# The image is extracted, its AppRun is started under xvfb with LD_DEBUG=libs, and the loader's
# own log must show both libraries initialised from the extracted tree: not from /usr/lib, not
# missing. The window must also still be open after the delay (timeout's exit code 124). Whether
# the system copy exists is not tested here: with or without it, the log says which one won.
# The caller removes the system package first when it wants the stronger statement "it starts
# without". WAYLAND_DISPLAY is unset on purpose: only the X11 path loads these libraries.
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
  echo "usage: $0 <image.AppImage> [seconds]" >&2
  exit 2
fi
IMAGE="$(readlink -f "$1")"
SECONDS_OPEN="${2:-10}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
cd "$WORK"
"$IMAGE" --appimage-extract > /dev/null
ROOT="$WORK/squashfs-root"

set +e
env -u WAYLAND_DISPLAY LD_DEBUG=libs xvfb-run -a timeout "$SECONDS_OPEN" "$ROOT/AppRun" > "$WORK/ld.log" 2>&1
code=$?
set -e
echo "exit code: ${code}"
if [ "$code" -ne 124 ]; then
  echo "::error::the window did not stay open (exit code ${code}, expected 124)"
  tail -n 20 "$WORK/ld.log"
  exit 1
fi

for lib in libxkbcommon-x11.so.0 libxcb-xkb.so.1; do
  # "calling init: <path>" is printed once per library, with the file the loader chose.
  found="$(grep "calling init: .*/${lib}\$" "$WORK/ld.log" || true)"
  if [ -z "$found" ]; then
    echo "::error::${lib} was never loaded (the window did not reach X11?)"
    exit 1
  fi
  echo "$found" | sed 's/^ *[0-9]*: *//'
  if echo "$found" | grep -qvF "${ROOT}/usr/"; then
    echo "::error::${lib} came from outside the image"
    exit 1
  fi
done
echo "libxkbcommon-x11 and libxcb-xkb: both come from the image"

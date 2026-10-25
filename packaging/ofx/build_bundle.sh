#!/bin/sh
# Superview's OpenFX plugin for DaVinci Resolve: a Superview.ofx.bundle folder (DEC-65).
#
#   sh packaging/ofx/build_bundle.sh [out dir]        (default: target/ofx)
#
# Builds the release library of crates/superview-ofx and lays it out the way OpenFX hosts
# read it, for the platform this runs on:
#   Superview.ofx.bundle/Contents/Info.plist
#   Superview.ofx.bundle/Contents/Linux-x86-64/Superview.ofx    (a renamed .so)
#   Superview.ofx.bundle/Contents/Win64/Superview.ofx           (a renamed .dll)
#
# Install by copying the folder into /usr/OFX/Plugins (Linux) or
# C:\Program Files\Common Files\OFX\Plugins (Windows), then restart Resolve.
set -eu

# Resolved before the cd below: a relative out dir is relative to where the script was run.
mkdir -p "${1:-$(dirname "$0")/../../target/ofx}"
OUT="$(cd "${1:-$(dirname "$0")/../../target/ofx}" && pwd)"
cd "$(dirname "$0")/../.."
TARGET="${CARGO_TARGET_DIR:-target}"

# bindgen (crate ofx_sys) needs stdbool.h; see scripts/checks/rust.sh.
# Linux only: the gcc on a Windows runner is mingw, whose headers must not shadow clang's.
if [ "$(uname -s)" = Linux ] && [ -z "${BINDGEN_EXTRA_CLANG_ARGS:-}" ] && command -v gcc >/dev/null 2>&1; then
  GCC_INCLUDE="$(gcc -print-file-name=include)"
  [ -f "$GCC_INCLUDE/stdbool.h" ] && export BINDGEN_EXTRA_CLANG_ARGS="-I$GCC_INCLUDE"
fi

case "$(uname -s)" in
  Linux)                 ARCH_DIR="Linux-x86-64"; LIB="libsuperview_ofx.so" ;;
  MINGW*|MSYS*|CYGWIN*)  ARCH_DIR="Win64";        LIB="superview_ofx.dll" ;;
  *) echo "Unsupported platform: $(uname -s) (Linux and Windows only)" >&2; exit 1 ;;
esac

cargo build --release --locked -p superview-ofx

BUNDLE="$OUT/Superview.ofx.bundle"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/$ARCH_DIR"
cp packaging/ofx/Info.plist "$BUNDLE/Contents/Info.plist"
cp "$TARGET/release/$LIB" "$BUNDLE/Contents/$ARCH_DIR/Superview.ofx"
cp LICENSE "$BUNDLE/Contents/LICENSE"
echo "Bundle ready: $BUNDLE"

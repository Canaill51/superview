#!/bin/sh
# Proves that an archive of the Resolve plugin holds what an OpenFX host loads (DEC-69).
#
#   sh packaging/ofx/check_bundle_zip.sh <superview-ofx-...-windows-x86_64.zip>
#   sh packaging/ofx/check_bundle_zip.sh <superview-ofx-...-linux-x86_64.zip>
#
# The archive is extracted, and it must hold one folder, Superview.ofx.bundle, with the plugin
# library, its Info.plist, the licence and the third-party notices. The platform is read from the
# archive's name, and the library must sit in that platform's folder (Win64, Linux-x86-64). It
# must export OfxGetPlugin and OfxGetNumberOfPlugins: they are what Resolve calls when it loads a
# plugin, and a library without them is skipped without a word. Loading it in Resolve is not tried
# here: no runner has Resolve.
#
# Exports are read with llvm-readobj on Windows, which the build already needs (bindgen's libclang
# comes with it), and with objdump on Linux, which also gives the newest glibc symbol version the
# library asks for. That version is the oldest glibc on which Resolve can load the plugin, and it
# must not rise above GLIBC_FLOOR: a newer toolchain or dependency would otherwise drop Ubuntu
# 22.04, Debian 12 and Rocky 9 without a word (DEC-69, amended 2026-11-15).
set -eu

GLIBC_FLOOR=2.34

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <archive.zip>" >&2
  exit 2
fi
ZIP="$1"
[ -f "$ZIP" ] || { echo "no such archive: $ZIP" >&2; exit 1; }
case "$(basename "$ZIP")" in
  *-windows-x86_64.zip) ARCH_DIR="Win64" ;;
  *-linux-x86_64.zip)   ARCH_DIR="Linux-x86-64" ;;
  *) echo "cannot tell the platform from the name $(basename "$ZIP") (…-windows-x86_64.zip or …-linux-x86_64.zip)" >&2; exit 2 ;;
esac
LIBRARY="Contents/$ARCH_DIR/Superview.ofx"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
# 7z.exe is a native program: it takes a Windows path, which an MSYS /tmp/... is not.
native() { if command -v cygpath >/dev/null 2>&1; then cygpath -m "$1"; else printf '%s' "$1"; fi; }

# 7z on the release runner; unzip where 7z is missing (a developer's Git Bash).
if command -v 7z >/dev/null 2>&1; then
  7z x -y -o"$(native "$WORK")" "$(native "$ZIP")" > /dev/null
elif command -v unzip >/dev/null 2>&1; then
  unzip -q "$ZIP" -d "$WORK"
else
  echo "neither 7z nor unzip is available to extract $ZIP" >&2
  exit 1
fi

failed=0
top="$(ls -A "$WORK" | tr '\n' ' ' | sed 's/ $//')"
if [ "$top" != "Superview.ofx.bundle" ]; then
  echo "the archive must hold the Superview.ofx.bundle folder and nothing else, found: $top" >&2
  failed=1
fi

BUNDLE="$WORK/Superview.ofx.bundle"
for f in "$LIBRARY" Contents/Info.plist Contents/LICENSE Contents/THIRD_PARTY_NOTICES.md; do
  if [ -f "$BUNDLE/$f" ]; then
    echo "present: $f"
  else
    echo "missing from the archive: Superview.ofx.bundle/$f" >&2
    failed=1
  fi
done
[ "$failed" -eq 0 ] || exit 1

if [ "$ARCH_DIR" = "Win64" ]; then
  READOBJ="$(command -v llvm-readobj || true)"
  if [ -z "$READOBJ" ] && [ -x "/c/Program Files/LLVM/bin/llvm-readobj.exe" ]; then
    READOBJ="/c/Program Files/LLVM/bin/llvm-readobj.exe"
  fi
  if [ -z "$READOBJ" ]; then
    echo "llvm-readobj not found (PATH or C:\\Program Files\\LLVM\\bin): the exports cannot be checked" >&2
    exit 1
  fi
  # One "  Name: <symbol>" line per export.
  exports="$("$READOBJ" --coff-exports "$(native "$BUNDLE/$LIBRARY")" | sed -n 's/^  *Name: //p')"
else
  command -v objdump >/dev/null 2>&1 || { echo "objdump not found: the exports cannot be checked" >&2; exit 1; }
  symbols="$(objdump -T "$BUNDLE/$LIBRARY")"
  # Defined dynamic symbols only: an undefined one (*UND*) is an import, not an export.
  exports="$(printf '%s\n' "$symbols" | awk '!/\*UND\*/ { print $NF }')"
  glibc="$(printf '%s\n' "$symbols" | grep -o 'GLIBC_[0-9][0-9.]*' | sed 's/^GLIBC_//' | sort -uV | tail -n 1)"
  if [ -z "$glibc" ]; then
    echo "no glibc symbol version found in Superview.ofx: objdump's output was not understood" >&2
    exit 1
  fi
  if [ "$(printf '%s\n%s\n' "$GLIBC_FLOOR" "$glibc" | sort -V | tail -n 1)" != "$GLIBC_FLOOR" ]; then
    echo "Superview.ofx asks for glibc $glibc, above the floor of $GLIBC_FLOOR: Resolve would not load it on a system with an older glibc" >&2
    exit 1
  fi
  echo "glibc asked for: $glibc (floor $GLIBC_FLOOR)"
fi
for symbol in OfxGetPlugin OfxGetNumberOfPlugins; do
  if printf '%s\n' "$exports" | grep -qx "$symbol"; then
    echo "exported: $symbol"
  else
    echo "Superview.ofx does not export $symbol: Resolve would not load it" >&2
    failed=1
  fi
done
[ "$failed" -eq 0 ] || exit 1
echo "plugin archive: ok"

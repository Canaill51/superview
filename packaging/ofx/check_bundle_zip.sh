#!/bin/sh
# Proves that the Windows archive of the Resolve plugin holds what an OpenFX host loads (DEC-69).
#
#   sh packaging/ofx/check_bundle_zip.sh <superview-ofx-...-windows-x86_64.zip>
#
# The archive is extracted, and it must hold one folder, Superview.ofx.bundle, with the plugin
# library, its Info.plist, the licence and the third-party notices. The library must export
# OfxGetPlugin and OfxGetNumberOfPlugins: they are what Resolve calls when it loads a plugin, and a
# DLL without them is skipped without a word. Loading it in Resolve is not tried here: no runner
# has Resolve. Exports are read with llvm-readobj, which the build already needs (bindgen's
# libclang comes with it).
set -eu

if [ "$#" -ne 1 ]; then
  echo "usage: $0 <archive.zip>" >&2
  exit 2
fi
ZIP="$1"
[ -f "$ZIP" ] || { echo "no such archive: $ZIP" >&2; exit 1; }

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
for f in Contents/Win64/Superview.ofx Contents/Info.plist Contents/LICENSE Contents/THIRD_PARTY_NOTICES.md; do
  if [ -f "$BUNDLE/$f" ]; then
    echo "present: $f"
  else
    echo "missing from the archive: Superview.ofx.bundle/$f" >&2
    failed=1
  fi
done
[ "$failed" -eq 0 ] || exit 1

READOBJ="$(command -v llvm-readobj || true)"
if [ -z "$READOBJ" ] && [ -x "/c/Program Files/LLVM/bin/llvm-readobj.exe" ]; then
  READOBJ="/c/Program Files/LLVM/bin/llvm-readobj.exe"
fi
if [ -z "$READOBJ" ]; then
  echo "llvm-readobj not found (PATH or C:\\Program Files\\LLVM\\bin): the exports cannot be checked" >&2
  exit 1
fi
exports="$("$READOBJ" --coff-exports "$(native "$BUNDLE/Contents/Win64/Superview.ofx")")"
for symbol in OfxGetPlugin OfxGetNumberOfPlugins; do
  if printf '%s\n' "$exports" | grep -qx "  *Name: $symbol"; then
    echo "exported: $symbol"
  else
    echo "Superview.ofx does not export $symbol: Resolve would not load it" >&2
    failed=1
  fi
done
[ "$failed" -eq 0 ] || exit 1
echo "plugin archive: ok"

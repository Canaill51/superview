#!/usr/bin/env bash
# Vérifications Rust de Superview : format + clippy + tests.
# Recette de AGENTS.md § « Verifying a change ». Option --fast : sans les tests.
# À exécuter depuis la racine du projet (scripts/check.sh s'en charge).
set -euo pipefail

FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

# Les hooks tournent dans un shell non interactif où rustup (docs/ENVIRONNEMENT.md)
# n'est pas dans le PATH.
export PATH="$HOME/.cargo/bin:$PATH"

if ! command -v cargo >/dev/null 2>&1; then
  echo "Erreur : 'cargo' est introuvable. Installation : voir docs/ENVIRONNEMENT.md (rustup, sans sudo)." >&2
  exit 1
fi

cd "$(dirname "$0")/../.."

# superview-ofx : bindgen (crate ofx_sys) a besoin de stdbool.h, que libclang ne trouve pas
# toujours seul (Debian/Ubuntu sans libclang-dev). On lui donne le dossier d'en-têtes de gcc.
if [ "$(uname -s)" = Linux ] && [ -z "${BINDGEN_EXTRA_CLANG_ARGS:-}" ] && command -v gcc >/dev/null 2>&1; then
  GCC_INCLUDE="$(gcc -print-file-name=include)"
  [ -f "$GCC_INCLUDE/stdbool.h" ] && export BINDGEN_EXTRA_CLANG_ARGS="-I$GCC_INCLUDE"
fi

echo "==> Format (cargo fmt --check)"
cargo fmt --all --check

echo "==> Clippy (-D warnings)"
cargo clippy --workspace --all-targets --locked -- -D warnings

if [ "$FAST" = 1 ]; then
  echo "OK (rapide) : format et clippy Rust passés, tests sautés."
  exit 0
fi

# Sans SUPERVIEW_REQUIRE_FFMPEG=1, un test qui a besoin d'ffmpeg se saute sans bruit.
. scripts/lib/outils.sh
if ! outil_trouve ffmpeg || ! outil_trouve ffprobe; then
  echo "Erreur : ffmpeg et ffprobe sont requis pour les tests, dans SUPERVIEW_FFMPEG_DIR (chemin absolu) ou dans le PATH. Installation : sudo apt install ffmpeg (voir docs/ENVIRONNEMENT.md)." >&2
  exit 1
fi

echo "==> Tests (cargo test, ffmpeg requis)"
SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked

echo "OK : toutes les vérifications Rust sont passées."

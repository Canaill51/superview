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

cd "$(dirname "$0")/../../rust"

echo "==> Format (cargo fmt --check)"
cargo fmt --all --check

echo "==> Clippy (-D warnings)"
cargo clippy --workspace --all-targets --locked -- -D warnings

if [ "$FAST" = 1 ]; then
  echo "OK (rapide) : format et clippy Rust passés, tests sautés."
  exit 0
fi

# Sans SUPERVIEW_REQUIRE_FFMPEG=1, un test qui a besoin d'ffmpeg se saute sans bruit.
if ! command -v ffmpeg >/dev/null 2>&1 || ! command -v ffprobe >/dev/null 2>&1; then
  echo "Erreur : ffmpeg et ffprobe sont requis pour les tests. Installation : sudo apt install ffmpeg (voir docs/ENVIRONNEMENT.md)." >&2
  exit 1
fi

echo "==> Tests (cargo test, ffmpeg requis)"
SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked

echo "OK : toutes les vérifications Rust sont passées."

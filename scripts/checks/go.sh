#!/usr/bin/env bash
# Vérifications Go de Superview : format + build + vet + lint + tests.
# Recette de AGENTS.md § « Verifying a change ». Option --fast : sans les tests.
# À exécuter depuis la racine du projet (scripts/check.sh s'en charge).
set -euo pipefail

FAST=0
[ "${1:-}" = "--fast" ] && FAST=1

# Les hooks tournent dans un shell non interactif où une chaîne d'outils
# installée sous $HOME (docs/ENVIRONNEMENT.md) n'est pas dans le PATH.
export PATH="$HOME/.local/go/bin:$HOME/go/bin:$PATH"

if ! command -v go >/dev/null 2>&1; then
  echo "Erreur : 'go' est introuvable. Installation : https://go.dev/doc/install" >&2
  exit 1
fi
if ! command -v golangci-lint >/dev/null 2>&1; then
  echo "Erreur : 'golangci-lint' est introuvable (obligatoire ici, c'est une porte de la CI)." >&2
  echo "Installation : make install-tools" >&2
  exit 1
fi

echo "==> Format (gofmt -l)"
UNFORMATTED=$(gofmt -l . || true)
if [ -n "$UNFORMATTED" ]; then
  echo "Fichiers mal formatés (corriger avec 'gofmt -w .') :" >&2
  echo "$UNFORMATTED" >&2
  exit 1
fi

# Le paquet cmd/superview a besoin des en-têtes GUI pour compiler : sans eux, ce
# n'est pas une vérification de la GUI (voir docs/ENVIRONNEMENT.md).
echo "==> go build"
go build ./...

echo "==> go vet"
go vet ./...

echo "==> golangci-lint"
golangci-lint run ./... --timeout=5m

if [ "$FAST" -eq 0 ]; then
  if ! command -v ffmpeg >/dev/null 2>&1 || ! command -v ffprobe >/dev/null 2>&1; then
    echo "Erreur : ffmpeg/ffprobe introuvables : les tests d'intégration ne peuvent pas tourner." >&2
    echo "Installation : sudo apt install ffmpeg (voir docs/ENVIRONNEMENT.md)" >&2
    exit 1
  fi
  # SUPERVIEW_REQUIRE_FFMPEG=1 : sans lui les tests qui encodent se sautent en silence.
  echo "==> Tests (go test -race, ffmpeg requis)"
  SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1
fi
echo "OK : toutes les vérifications Go sont passées."

#!/usr/bin/env bash
# Vérifie que scripts/lib/outils.sh trouve ffmpeg là où les tests le trouvent.
# Jusqu'au 2026-10-26, rust.sh ne cherchait que dans le PATH et refusait de lancer les tests sur
# un poste Windows dont ffmpeg n'était que dans SUPERVIEW_FFMPEG_DIR, où la suite passe pourtant.
# Le PATH est vidé, pour qu'un ffmpeg installé sur la machine ne réponde pas à la place du
# dossier. Rapide : tourne aussi avec --fast.
set -euo pipefail
cd "$(dirname "$0")/../.."
. scripts/lib/outils.sh

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

nom=ffmpeg
case "${OSTYPE:-}" in msys* | cygwin*) nom=ffmpeg.exe ;; esac
mkdir -p "$tmp/avec" "$tmp/sans" "$tmp/path" "$tmp/vide"
for d in avec path; do printf '#!/bin/sh\n' >"$tmp/$d/$nom"; chmod +x "$tmp/$d/$nom"; done

echec=0
# attendu <oui|non> <description> <SUPERVIEW_FFMPEG_DIR> <PATH>
attendu() {
  local voulu="$1" desc="$2" vu=non
  if SUPERVIEW_FFMPEG_DIR="$3" PATH="$4" outil_trouve ffmpeg; then vu=oui; fi
  if [ "$vu" = "$voulu" ]; then
    echo "ok : $desc"
  else
    echo "ÉCHEC : $desc (trouvé : $vu, attendu $voulu)" >&2
    echec=1
  fi
}

echo "==> Recherche d'ffmpeg des vérifications (PATH vidé)"
attendu oui "trouvé dans SUPERVIEW_FFMPEG_DIR, absent du PATH" "$tmp/avec" "$tmp/vide"
attendu non "absent du dossier et du PATH" "$tmp/sans" "$tmp/vide"
attendu oui "trouvé dans le PATH quand le dossier ne l'a pas" "$tmp/sans" "$tmp/path"
attendu oui "trouvé dans le PATH sans dossier" "" "$tmp/path"
# Un dossier relatif désigne le dossier courant : ToolResolver l'ignore (L-16), ici aussi.
cd "$tmp"
attendu non "dossier relatif ignoré" "avec" "$tmp/vide"
cd - >/dev/null

if [ "$echec" -ne 0 ]; then
  echo "La recherche d'ffmpeg de scripts/lib/outils.sh ne suit plus ToolResolver." >&2
  exit 1
fi

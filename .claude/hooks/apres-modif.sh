#!/usr/bin/env bash
# Hook PostToolUse (Edit/Write) : vérification rapide après chaque modification de code.
# En cas d'échec : exit 2 → l'erreur est renvoyée à Claude, qui doit corriger.
# Coût : 0 token si tout va bien (aucune sortie), quelques lignes sinon.
set -uo pipefail
. "$(dirname "$0")/json.sh"
cd "${CLAUDE_PROJECT_DIR:-.}"

entree="$(cat)"
# Entrée illisible : on vérifie quand même plutôt que de sauter sans le dire.
if ! fichier="$(printf '%s' "$entree" | champ_json tool_input.file_path)"; then
  echo "⚠️ apres-modif.sh : entrée illisible (perl et JSON::PP sont requis, voir .claude/hooks/json.sh) ; vérification lancée quand même." >&2
  fichier="(fichier illisible)"
fi

# Ignorer les fichiers non-code (docs, config Claude…). Sous Windows, le chemin a des « \ ».
case "$fichier" in
  *.md|*.txt|*.json|*.yml|*.yaml|*.toml|*/.claude/*|*\\.claude\\*|"") exit 0 ;;
esac

if ! sortie="$(bash scripts/check.sh --fast 2>&1)"; then
  # Ne renvoyer que la fin utile pour économiser des tokens
  printf '%s\n' "$sortie" | tail -n 40 >&2
  echo "⚠️ Vérification rapide en échec après modification de $fichier. Corrige avant de continuer." >&2
  exit 2
fi
exit 0

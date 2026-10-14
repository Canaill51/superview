#!/usr/bin/env bash
# Hook PostToolUse (Edit/Write) : vérification rapide après chaque modification de code.
# En cas d'échec : exit 2 → l'erreur est renvoyée à Claude, qui doit corriger.
# Coût : 0 token si tout va bien (aucune sortie), quelques lignes sinon.
set -uo pipefail
cd "${CLAUDE_PROJECT_DIR:-.}"

entree="$(cat)"
fichier="$(printf '%s' "$entree" | python3 -c 'import sys,json; d=json.load(sys.stdin); print(d.get("tool_input",{}).get("file_path",""))' 2>/dev/null)"

# Ignorer les fichiers non-code (docs, config Claude…)
case "$fichier" in
  *.md|*.txt|*.json|*.yml|*.yaml|*.toml|*/.claude/*|"") exit 0 ;;
esac

if ! sortie="$(bash scripts/check.sh --fast 2>&1)"; then
  # Ne renvoyer que la fin utile pour économiser des tokens
  printf '%s\n' "$sortie" | tail -n 40 >&2
  echo "⚠️ Vérification rapide en échec après modification de $fichier. Corrige avant de continuer." >&2
  exit 2
fi
exit 0

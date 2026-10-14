#!/usr/bin/env bash
# Hook Stop : avant que Claude rende la main, lancer la vérification COMPLÈTE (tests inclus)
# si du code a été modifié. Si ça échoue, Claude est renvoyé au travail (exit 2).
set -uo pipefail
cd "${CLAUDE_PROJECT_DIR:-.}"

entree="$(cat)"
# Garde-fou anti-boucle : si on est déjà relancé par ce hook, ne pas bloquer à nouveau.
actif="$(printf '%s' "$entree" | python3 -c 'import sys,json; print(json.load(sys.stdin).get("stop_hook_active", False))' 2>/dev/null)"
[ "$actif" = "True" ] && exit 0

# Rien de modifié dans le code → rien à tester
if git rev-parse --git-dir >/dev/null 2>&1; then
  modifs="$(git status --porcelain | grep -vE '\.(md|txt)$' || true)"
  [ -z "$modifs" ] && exit 0
fi

if ! sortie="$(bash scripts/check.sh 2>&1)"; then
  printf '%s\n' "$sortie" | tail -n 60 >&2
  echo "❌ scripts/check.sh échoue : la tâche n'est pas terminée. Corrige (sans désactiver de test) ou explique le blocage." >&2
  exit 2
fi
exit 0

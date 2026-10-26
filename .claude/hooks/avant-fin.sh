#!/usr/bin/env bash
# Hook Stop : avant que Claude rende la main, lancer la vérification COMPLÈTE (tests inclus)
# si du code a été modifié. Si ça échoue, Claude est renvoyé au travail (exit 2).
set -uo pipefail
. "$(dirname "$0")/json.sh"
cd "${CLAUDE_PROJECT_DIR:-.}"

entree="$(cat)"
# Garde-fou anti-boucle : si on est déjà relancé par ce hook, ne pas bloquer à nouveau.
# Entrée illisible : on ne sait pas si on boucle, donc on prévient sans bloquer (exit 1).
if ! actif="$(printf '%s' "$entree" | champ_json stop_hook_active)"; then
  echo "⚠️ avant-fin.sh : entrée illisible (perl et JSON::PP sont requis, voir .claude/hooks/json.sh) ; vérification complète non lancée." >&2
  exit 1
fi
[ "$actif" = "true" ] && exit 0

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

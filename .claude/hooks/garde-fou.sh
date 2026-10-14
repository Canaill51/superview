#!/usr/bin/env bash
# Hook PreToolUse (Bash) : bloque les commandes dangereuses ou anti-régression.
set -uo pipefail
cmd="$(python3 -c 'import sys,json; print(json.load(sys.stdin).get("tool_input",{}).get("command",""))' 2>/dev/null)"

refuser() { echo "⛔ Bloqué par garde-fou : $1" >&2; exit 2; }

case "$cmd" in
  *"--no-verify"*) refuser "contourner le hook pre-commit (--no-verify) est interdit. Corriger les erreurs." ;;
  *"git push --force"*|*"git push -f"*) refuser "push forcé interdit sans accord explicite de l'utilisateur." ;;
  *"rm -rf /"*|*"rm -rf ~"*|*'rm -rf $HOME'*) refuser "suppression massive." ;;
  *"git reset --hard"*) refuser "git reset --hard efface du travail : demander d'abord à l'utilisateur." ;;
esac
exit 0

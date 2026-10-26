#!/usr/bin/env bash
# Hook PreToolUse (Bash) : bloque les commandes dangereuses ou anti-régression.
set -uo pipefail
. "$(dirname "$0")/json.sh"

refuser() { echo "⛔ Bloqué par garde-fou : $1" >&2; exit 2; }

# Une commande illisible est refusée : un garde qui laisse passer ce qu'il ne lit pas ne garde rien.
if ! cmd="$(champ_json tool_input.command)"; then
  refuser "commande illisible (perl et JSON::PP sont requis, voir .claude/hooks/json.sh)."
fi

case "$cmd" in
  *"--no-verify"*) refuser "contourner le hook pre-commit (--no-verify) est interdit. Corriger les erreurs." ;;
  *"git push --force"*|*"git push -f"*) refuser "push forcé interdit sans accord explicite de l'utilisateur." ;;
  *"rm -rf /"*|*"rm -rf ~"*|*'rm -rf $HOME'*) refuser "suppression massive." ;;
  *"git reset --hard"*) refuser "git reset --hard efface du travail : demander d'abord à l'utilisateur." ;;
esac
exit 0

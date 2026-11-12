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
  *"rm -rf /"*|*"rm -rf ~"*|*'rm -rf $HOME'*) refuser "suppression massive." ;;
esac

# Les règles de .claude/settings.json comparent le DÉBUT de la commande : `git push --force*` ne voit
# ni `git -C dossier push -f`, ni `git -c x=y push origin +master`, ni une commande glissée dans
# `sh -c "…"` ou après un `&&`. Ici, git est cherché partout, options globales comprises (la même
# expression que guard-git.sh d'OpenTuneBox), et chaque appel est lu jusqu'au premier séparateur
# (`;`, `&`, `|`, guillemet, accent grave, parenthèse). Faux positif possible (ces mots dans un
# message de commit) : le refus est alors pour rien, ce qui est le bon côté de l'erreur.
git_='git( +(-c +[^ ]+|-C +[^ ]+|--[a-z-]+(=[^ ]+)?))* +'
fin_="[^;&|\"'\`()]*"
# appels <sous-commande> : chaque appel de git à cette sous-commande, un par ligne, jusqu'au séparateur.
appels() { printf '%s\n' "$cmd" | grep -oE "${git_}$1([^a-z-]${fin_})?" || true; }

# Push forcé : --force, --force-with-lease, -f seul ou groupé (-uf), ou une refspec en +branche.
if appels push | grep -qE ' (--force[a-z-]*(=[^ ]*)?|-[a-zA-Z]*f[a-zA-Z]*|\+[^ ]+)( |$)'; then
  refuser "push forcé interdit sans accord explicite de l'utilisateur."
fi
if appels reset | grep -qE ' --hard( |$)'; then
  refuser "git reset --hard efface du travail : demander d'abord à l'utilisateur."
fi
# Une étiquette se pose par /publier et le workflow Release, jamais à la main (AGENTS.md) ;
# settings.json refuse déjà `git tag *`, mais seulement en début de commande.
if [ -n "$(appels tag)" ]; then
  refuser "git tag est réservé au workflow Release (/publier)."
fi
exit 0

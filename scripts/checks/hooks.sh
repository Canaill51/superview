#!/usr/bin/env bash
# Vérifie que les hooks Claude Code de .claude/hooks lisent vraiment leur entrée.
# Ils l'ont lue avec python3 jusqu'au 2026-10-26 : sous Windows, c'est le raccourci du Microsoft
# Store, qui échoue, et les trois hooks lisaient un champ vide sans rien dire (le garde-fou ne
# bloquait plus rien). Ici, python3 et python sont remplacés par un faux qui échoue de la même
# façon, pour que Linux reproduise ce poste. Rapide : tourne aussi avec --fast.
set -euo pipefail
cd "$(dirname "$0")/../.."
hooks="$PWD/.claude/hooks"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

mkdir "$tmp/bin"
for p in python3 python; do printf '#!/bin/sh\nexit 49\n' >"$tmp/bin/$p"; chmod +x "$tmp/bin/$p"; done
export PATH="$tmp/bin:$PATH"

# Un faux projet, hors de tout dépôt git : son scripts/check.sh note qu'il a été lancé et rend $CODE.
projet="$tmp/projet"
mkdir -p "$projet/scripts"
printf '#!/usr/bin/env bash\necho lancé >>"%s/lancé"\nexit "${CODE:-0}"\n' "$projet" >"$projet/scripts/check.sh"

echec=0
# attendu <code> <description> <hook> <entrée JSON>
attendu() {
  local code="$1" desc="$2" hook="$3" entree="$4" obtenu=0
  printf '%s' "$entree" | CLAUDE_PROJECT_DIR="$projet" bash "$hooks/$hook" >/dev/null 2>&1 || obtenu=$?
  if [ "$obtenu" = "$code" ]; then
    echo "ok : $desc"
  else
    echo "ÉCHEC : $desc (code $obtenu, attendu $code)" >&2
    echec=1
  fi
}
# lance <oui|non> <description> <entrée JSON> : apres-modif.sh a-t-il lancé la vérification ?
lance() {
  local voulu="$1" desc="$2" entree="$3" vu=non
  rm -f "$projet/lancé"
  printf '%s' "$entree" | CLAUDE_PROJECT_DIR="$projet" bash "$hooks/apres-modif.sh" >/dev/null 2>&1 || true
  [ -f "$projet/lancé" ] && vu=oui
  if [ "$vu" = "$voulu" ]; then
    echo "ok : $desc"
  else
    echo "ÉCHEC : $desc (vérification lancée : $vu, attendu $voulu)" >&2
    echec=1
  fi
}

echo "==> Hooks Claude Code (python3 indisponible)"
attendu 2 "garde-fou refuse git reset --hard" garde-fou.sh '{"tool_input":{"command":"git status && git reset --hard"}}'
attendu 2 "garde-fou refuse --no-verify dans une commande non ASCII" garde-fou.sh '{"tool_input":{"command":"git commit -m \"réglé\" --no-verify"}}'
attendu 0 "garde-fou laisse passer git status" garde-fou.sh '{"tool_input":{"command":"git status"}}'
attendu 2 "garde-fou refuse une entrée illisible" garde-fou.sh 'pas du JSON'

CODE=1 attendu 0 "avant-fin ne relance pas quand stop_hook_active est vrai" avant-fin.sh '{"stop_hook_active":true}'
CODE=1 attendu 2 "avant-fin bloque sur une vérification rouge" avant-fin.sh '{"stop_hook_active":false}'
attendu 1 "avant-fin prévient sans bloquer sur une entrée illisible" avant-fin.sh 'pas du JSON'

lance oui "apres-modif vérifie un fichier Rust" '{"tool_input":{"file_path":"/x/crates/a/src/lib.rs"}}'
lance non "apres-modif saute un fichier Markdown" '{"tool_input":{"file_path":"/x/docs/SUITE.md"}}'
lance non "apres-modif saute .claude sous Windows" '{"tool_input":{"file_path":"C:\\x\\.claude\\hooks\\a.sh"}}'
lance oui "apres-modif vérifie quand même sur une entrée illisible" 'pas du JSON'

if [ "$echec" -ne 0 ]; then
  echo "Un hook ne lit pas son entrée : voir .claude/hooks/json.sh." >&2
  exit 1
fi

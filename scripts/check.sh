#!/usr/bin/env bash
# Point d'entrée UNIQUE de vérification du projet.
# Utilisé par : hooks Claude Code, hook git pre-commit, CI GitHub Actions, et vous.
#   scripts/check.sh          → tout (format, lint, types, tests)
#   scripts/check.sh --fast   → rapide (sans les tests lents), après chaque modification
# Chaque stack présente dans scripts/checks/<stack>.sh est exécutée.
set -euo pipefail
cd "$(dirname "$0")/.."

mode="${1:-}"
shopt -s nullglob
checks=(scripts/checks/*.sh)

if [ ${#checks[@]} -eq 0 ]; then
  echo "ℹ️  Aucune vérification configurée dans scripts/checks/ (rien à tester)."
  exit 0
fi

echec=0
for c in "${checks[@]}"; do
  nom="$(basename "$c" .sh)"
  echo "── Vérification : $nom ${mode}"
  if ! bash "$c" $mode; then
    echo "❌ ÉCHEC : $nom"
    echec=1
  fi
done

if [ "$echec" -ne 0 ]; then
  echo "❌ Des vérifications ont échoué. Corriger AVANT de continuer (ne pas désactiver le test)."
  exit 1
fi
echo "✅ Toutes les vérifications passent."

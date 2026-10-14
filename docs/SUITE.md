# Où on en est
- État : treizième passe d'analyse close, tout fusionné (#78 à #93). Aucune question ouverte.
- Le dépôt suit le gabarit `claude-gabarit` : `scripts/check.sh` (point d'entrée unique), hooks git
  et Claude, code sous `cmd/superview/` + `internal/common/` (DEC-39), docs sous `docs/` (DEC-40).
- Avant de commencer : `git config core.hooksPath .githooks` (une fois par clone), puis `scripts/check.sh`.
- Toute modification de `.github/workflows/release.yml` : essai à blanc AVANT fusion (L-114,
  `gh workflow run release.yml --ref <branche>`, sans version).
- Prochaine étape : aucune planifiée. Mettre ce fichier à jour à la clôture d'une session (`/cloture`).

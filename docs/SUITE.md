# Où on en est
- État : treizième passe d'analyse close, tout fusionné (#78 à #96). Dernière version publiée : **v0.2.9**
  (2026-10-14), la première avec l'installateur Windows et l'AppImage Linux (DEC-41). Aucune PR ouverte.
  Non vérifié sur de vraies machines : l'installateur sous Windows, les encodeurs GPU.
- Le dépôt suit le gabarit `claude-gabarit` : `scripts/check.sh` (point d'entrée unique), hooks git
  et Claude, code sous `cmd/superview/` + `internal/common/` (DEC-39), docs sous `docs/` (DEC-40).
- Avant de commencer : `git config core.hooksPath .githooks` (une fois par clone), puis `scripts/check.sh`.
- Toute modification de `.github/workflows/release.yml` : essai à blanc AVANT fusion (L-114,
  `gh workflow run release.yml --ref <branche>`, sans version).
- Prochaine étape : portage Rust + Slint, planifié mais pas commencé. Plan approuvé :
  `~/.claude/plans/distributed-kindling-alpaca.md` (phases 1 à 8, porte GO/NO-GO après l'essai de la phase 3).
  Il prend la suite de cette PR : les scripts `packaging/` sont réutilisés tels quels.
- Publier une version : demander « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42), confirme le numéro, lance le workflow Release et vérifie les fichiers.
- `fyne package` lancé en local réécrit `cmd/superview/FyneApp.toml` : `git checkout cmd/superview/FyneApp.toml`.
- Mettre ce fichier à jour à la clôture d'une session (`/cloture`).

# Où on en est
- État : treizième passe d'analyse close, tout fusionné (#78 à #96). Dernière version publiée : **v0.2.9**
  (2026-10-14), la première avec l'installateur Windows et l'AppImage Linux (DEC-41). Aucune PR ouverte.
  Non vérifié sur de vraies machines : l'installateur sous Windows, les encodeurs GPU.
- Le dépôt suit le gabarit `claude-gabarit` : `scripts/check.sh` (point d'entrée unique), hooks git
  et Claude, code sous `cmd/superview/` + `internal/common/` (DEC-39), docs sous `docs/` (DEC-40).
- Avant de commencer : `git config core.hooksPath .githooks` (une fois par clone), puis `scripts/check.sh`.
- Toute modification de `.github/workflows/release.yml` : essai à blanc AVANT fusion (L-114,
  `gh workflow run release.yml --ref <branche>`, sans version).
- Portage Rust + Slint (DEC-43) : **phases 1 à 3 faites** (#99 #100, oracle de parité #101, port `video`/`pgm`/`error` #102, fenêtre Slint + `rfd` #103).
  **En attente : la décision GO/NO-GO de l'utilisateur.** Mesuré : JSON d'oracle reproduits champ par champ, 4 empreintes PGM identiques,
  génération des cartes ~2 fois plus rapide que Go (critère : ≤ 1,5 ×), fenêtre ouverte sous xvfb, sous Windows et dans une AppImage d'essai (CI).
  À savoir : winit charge `libxkbcommon-x11` à l'exécution sous X11 et l'AppImage ne l'embarque pas ; `rfd` a besoin de libdbus (portail) ou de zenity ;
  le sélecteur n'a pas de champ texte de secours (phase 6, étape 4). Si GO : phase 4 (cœur `superview-core`). Si NO-GO : DEC-43 *Rejetée*, `rust/` supprimé en une PR.
  Plan approuvé : `~/.claude/plans/distributed-kindling-alpaca.md`. Rust s'installe par rustup dans `~/.cargo` (`docs/ENVIRONNEMENT.md`) ;
  le pre-commit l'exige. Les jobs `rust.yml` ne sont pas des checks requis.
- Publier une version : demander « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42), confirme le numéro, lance le workflow Release et vérifie les fichiers.
- `fyne package` lancé en local réécrit `cmd/superview/FyneApp.toml` : `git checkout cmd/superview/FyneApp.toml`.
- Mettre ce fichier à jour à la clôture d'une session (`/cloture`).

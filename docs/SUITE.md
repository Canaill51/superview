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
  **GO donné le 2026-10-15** (JSON et 4 empreintes PGM identiques, cartes ~2 fois plus rapides que Go, fenêtre OK sous xvfb, Windows et AppImage d'essai).
  **Phase 4 en cours (cœur `superview-core`, 7 PR)** : PR 1/7 `config.rs` + `security.rs` (YAML par `serde-saphyr`, sans migration des clés retirées ni variables `SUPERVIEW_*`) ;
  PR 2/7 `tools.rs` + `process.rs` (`ToolResolver` : cache et environnement injectés, aucun global ; `check_ffmpeg` ; `isStopRequest`/`shouldNotRetry` restent pour la PR 6, avec la cascade) ;
  PR 3/7 `hardware.rs` (partie pure de `hardware.go` + `FindEncoder` : nombre de cœurs et profil injectés ; `decisions.json` reproduit en entier par `tests/parity_decisions.rs` ; `isHEVCEncoder`/`isHighBitDepth`/`sourcePixelFormat` y vivent en attendant la PR 4 ; la sonde reste pour la PR 5) ;
  suivantes : `encode/args`, `probe`/`session`/`memory`/`disk`/`health`, `encode/run`, `pipeline`+`logfile`.
  À savoir : winit charge `libxkbcommon-x11` à l'exécution sous X11 et l'AppImage ne l'embarque pas ; `rfd` a besoin de libdbus (portail) ou de zenity ;
  le sélecteur n'a pas de champ texte de secours (phase 6, étape 4).
  Plan approuvé : `~/.claude/plans/distributed-kindling-alpaca.md`. Rust s'installe par rustup dans `~/.cargo` (`docs/ENVIRONNEMENT.md`) ;
  le pre-commit l'exige. Les jobs `rust.yml` ne sont pas des checks requis.
- Publier une version : demander « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42), confirme le numéro, lance le workflow Release et vérifie les fichiers.
- `fyne package` lancé en local réécrit `cmd/superview/FyneApp.toml` : `git checkout cmd/superview/FyneApp.toml`.
- Mettre ce fichier à jour à la clôture d'une session (`/cloture`).

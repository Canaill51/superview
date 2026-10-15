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
  PR 3/7 `hardware.rs` (partie pure de `hardware.go` + `FindEncoder` : nombre de cœurs et profil injectés ; `decisions.json` reproduit en entier par `tests/parity_decisions.rs` ; `isHEVCEncoder`/`isHighBitDepth`/`sourcePixelFormat` y vivent (réutilisés par la PR 4) ; la sonde reste pour la PR 5) ;
  PR 4/7 `encode/args.rs` (`ValidateBitrate` à `mapVideoPresetForEncoder` ; argv complet reproduit par `tests/parity_encode_args.rs` sur les 237 cas d'`encode_args.json` ; nombre de cœurs injecté ; `probe_args.json` reste pour la PR 5) ;
  PR 5a/7 `session.rs` + `memory.rs` + `disk.rs` (session possédée dont le `Drop` supprime le dossier, sans global ; mémoire et lecteur de disque injectés ; `fs4` pour l'espace libre, sans `unsafe` ; `memory.json` reproduit par `tests/parity_memory.rs`) ; PR 5b/7 `probe.rs` + `health.rs` + `process::run_with_deadline` (sonde d'encodeur avec délai et annulation, cache possédé par un `Prober` durable ; `probe_args.json` reproduit par `tests/parity_probe_args.rs` ; Diagnostic réduit à ffmpeg, ffprobe et disque, sans les lignes mémoire et CPU ; limite héritée de Go : un `.bat`/`.cmd` sous Windows n'est tué que par son `cmd.exe`) ;
  PR 6a/7 `error.rs` (variantes `Cancelled`, `StoppedBySignal`, `FailedAfterOutput` + `is_stop_request`/`should_not_retry`) + `encode/failure.rs` (`classify_ffmpeg_failure`, mémoire injectée) + `encode/progress.rs` (ligne `out_time_ms=`, pourcentage) ; la PR 6 est coupée en 6a, 6b (un passage ffmpeg : `run_ffmpeg`, course annulation/signal/fin) et 6c (la cascade `encode_video`), plan dans `~/.claude/plans/playful-splashing-goose.md` ;
  PR 6b/7 `encode/run.rs` (`run_ffmpeg` : un passage ffmpeg, course fin/annulation/signal sans partager le `Child`, `StopControl` = deux drapeaux écrits par l'appelant, aucun gestionnaire de signal dans le cœur ; un ffmpeg tué par un signal qui n'est pas de nous garde son diagnostic mémoire même si SIGTERM suit, cas OOM + systemd) ;
  PR 6c/7 `encode/cascade.rs` (`encode_video` = `EncodeVideo` : décodage matériel avec repli audio copy→aac, décodage CPU, encodeur matériel→CPU par récursion avec garde mémoire ; résumé « Hardware: … » rendu en valeur, session/cœurs/résolveur/arrêt injectés ; un arrêt levé avant la garde mémoire est rendu tel quel, écart voulu avec Go) ;
  PR 7a/7 `logfile.rs` (journal plafonné 5 Mio, vidage par réouverture, dossier de cache injecté) + `video::check_video` (le lancement d'ffprobe, qui manquait au port) + `metrics.rs` (le strict nécessaire de l'interface : tailles, durées, temps restant ; ni bus d'événements ni `Summary`) + `output.rs` (sortie = entrée, fichier partiel dans le dossier de destination, espace de sortie) ;
  suivante : 7b `pipeline.rs` = `perform_encoding` (PR 7 : fichier partiel puis renommage, journal plafonné à 5 Mio ; l'application ou le banc d'essai enregistrera SIGINT/SIGTERM avec `ctrlc`).
  À savoir : winit charge `libxkbcommon-x11` à l'exécution sous X11 et l'AppImage ne l'embarque pas ; `rfd` a besoin de libdbus (portail) ou de zenity ;
  le sélecteur n'a pas de champ texte de secours (phase 6, étape 4).
  Plan approuvé : `~/.claude/plans/distributed-kindling-alpaca.md`. Rust s'installe par rustup dans `~/.cargo` (`docs/ENVIRONNEMENT.md`) ;
  le pre-commit l'exige. Les jobs `rust.yml` ne sont pas des checks requis.
- Publier une version : demander « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42), confirme le numéro, lance le workflow Release et vérifie les fichiers.
- `fyne package` lancé en local réécrit `cmd/superview/FyneApp.toml` : `git checkout cmd/superview/FyneApp.toml`.
- Mettre ce fichier à jour à la clôture d'une session (`/cloture`).

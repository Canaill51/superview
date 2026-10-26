# Architecture

> Claude : à lire avant toute nouvelle fonctionnalité. À mettre à jour quand la structure change.
> C'est une carte, pas un roman : les contrats détaillés sont dans [CONTRATS.md](CONTRATS.md).

## But du projet
Convertir une vidéo MP4 en 4:3 vers du 16:9 par distorsion dynamique : les bords sont étirés, le
centre garde son ratio. Une interface graphique (Rust + Slint, Windows et Linux, **pas de CLI**) qui
génère deux cartes de remappage PGM et les confie au filtre `remap` de FFmpeg. MP4 en entrée, MP4 en
sortie : contrainte produit, pas un oubli. Le programme a été écrit en Go + Fyne jusqu'à la v0.2.9 ;
Go a été retiré en DEC-49 (`git show v0.2.9:<chemin>` pour relire l'ancien code).

## Vue d'ensemble
Un workspace Cargo à la racine (`crates/`), quatre crates :

- **`superview-core`** — toute la logique, sans interface. Un module par responsabilité (voir le tableau).
- **`superview-app`** — la fenêtre Slint. Seul le fil de l'interface touche un widget ; les autres
  fils passent par `upgrade_in_event_loop`. La logique de la fenêtre (`AppState`, `ViewModel`) ne
  contient aucun type Slint, donc se teste sans écran.
- **`superview-harness`** — banc d'essai jamais livré, pas une CLI (DEC-44) : `convert` et `probe`
  (sortie JSON) et SIGINT/SIGTERM, pour les tests qui lancent une vraie conversion dans un processus
  séparé.

- **`superview-ofx`** — le plugin OpenFX pour DaVinci Resolve (DEC-65, DEC-66) : `warp` (le calcul, pur, qui
  réutilise `pgm::source_x_for_output_x`), `cuda` (le même calcul sur la carte : pilote CUDA chargé à l'exécution,
  noyau PTX, repli CPU), `journal` (fichier plafonné, une ligne par instance et par chemin, bavardage du fork filtré)
  et `plugin` (la colle avec l'hôte, `FullySafe`). Un bundle `Superview.ofx.bundle` par
  `packaging/ofx/build_bundle.sh` ; prototype, pas dans les versions.

### `superview-core`
| Module | Rôle |
|--------|------|
| `error` | erreurs typées (`InvalidVideoError`, `EncoderError`, `SessionError`) et variantes `Cancelled`, `StoppedBySignal`, `FailedAfterOutput` que lit la cascade |
| `config`, `security` | YAML par `serde-saphyr` ; validation des chemins et des noms d'encodeur |
| `video` | lecture et validation d'ffprobe (`check_video`, `parse_video_specs`) |
| `pgm` | géométrie de sortie, formule de distorsion (`source_x_for_output_x`, partagée avec l'aperçu), écriture des cartes PGM (`powi(2)`, jamais `powf`) |
| `tools`, `process` | recherche d'ffmpeg/ffprobe par un `ToolResolver` à environnement injecté ; commandes sans fenêtre, `killed_by_signal`, `run_with_deadline` (un processus borné par un délai ou une annulation) |
| `hardware` | choix de l'encodeur et du chemin matériel : fonctions pures, cœurs et profil injectés |
| `probe` | sonde d'un encodeur sur une vraie trame (délai, annulation), cache d'un `Prober` durable |
| `encode::args` | l'argv ffmpeg d'un encodage : chaîne de filtres, préréglages, fils, débits en bits/s, `RateControl` (DEC-48) |
| `encode::run` | un passage ffmpeg : progression, annulation, signal, pas de zombie |
| `encode::cascade` | `encode_video`, la cascade de repli à trois niveaux |
| `encode::failure`, `encode::progress` | classement d'un échec (mort par signal = mémoire) ; lecture de `out_time_ms=` |
| `pipeline` | `perform_encoding` : validations, fichier de travail, métriques ; le trait `PipelineUi` et les services sont injectés |
| `thumbnail` | une image brute RGBA de la vidéo (192×144) par ffmpeg, dossier temporaire privé, délai et annulation (DEC-60) |
| `session` | `EncodingSession` : dossier temporaire isolé, supprimé au `Drop` |
| `output`, `disk`, `memory` | sortie ≠ entrée et fichier partiel, espace libre (`fs4`), garde-fou mémoire (lecture injectée) |
| `health`, `metrics`, `logfile` | Diagnostic (ffmpeg, ffprobe, disque, encodeurs), tailles et durées, journal à plafond injecté et son `FileLogger`, partagés par `superview-app` (5 Mio) et le plugin (1 Mio) |

### `superview-app`
| Fichier | Rôle |
|---------|------|
| `ui/fonts/` | IBM Plex Sans et Sora, embarquées dans le binaire (OFL, DEC-63) ; leurs licences et `THIRD_PARTY_NOTICES.md` |
| `ui/theme.slint`, `ui/components.slint` | le global `Theme` (jeux Light et Dark, accent cyan, DEC-57) ; `AppButton`, `Card`, `ProgressBar` (barre rouge/verte, DEC-58), `SegmentedControl`, `Switch` et `Pill` |
| `ui/main.slint` | la fenêtre (1000×570, taille fixe, DEC-61 : la disposition de la maquette cyan, boutons en contexte), les popups (`Popup`, composant `Modal`) ; `ui/Icon.png` = l'icône |
| `preview.rs` | les deux grilles de l'aperçu de la distorsion (masques alpha, DEC-62), tirées de `pgm::source_x_for_output_x` ; fonctions pures |
| `state.rs`, `format.rs`, `report.rs`, `prefs.rs` | `AppState` + `ViewModel` ; textes et règles pures ; le rapport du Diagnostic lu ligne à ligne (aucun texte perdu) ; préférences JSON (DEC-46) |
| `app.rs` | la colle : callbacks, fils de travail, popups en file ; tout widget touché par `upgrade_in_event_loop` |
| `engine.rs`, `host.rs` | ffmpeg, ffprobe et pipeline derrière le trait `Engine` (un résolveur et un `Prober` par processus, en `static`) ; le bureau derrière `Host` (navigateur sans shell, fin de la boucle) |
| `dialogs.rs` | sélecteurs de fichiers : rfd, puis zenity/kdialog si rfd rend « rien » en moins de 300 ms, puis un champ texte (horloge et outils injectés) |
| `journal.rs`, `signals.rs`, `main.rs` | journal plafonné ; SIGINT/SIGTERM (`signal-hook` sous Unix, `ctrlc` sous Windows) ; démarrage |
| `repo_guards.rs` | tests sur des fichiers que le compilateur ne lit pas (ancre du README, fichiers de workflow) |
| `app_tests.rs`, `window_tests.rs` | scénarios de fenêtre avec une conversion simulée ; rendu sans écran (`i-slint-backend-testing`) |
| `build.rs` | version (`SUPERVIEW_VERSION`), marqueur de build, icône du `.exe`, `DT_RPATH` de l'AppImage |

## Arborescence
| Dossier | Rôle |
|---------|------|
| `Cargo.toml`, `crates/` | le workspace Cargo ci-dessus (à la racine) ; `rust-toolchain.toml` épingle la version |
| `crates/superview-core/tests/fixtures/` | fixtures enregistrées (`recorded/*.json`, `ffprobe/`) : enregistrées par l'oracle Go (`internal/common/parity_test.go`, #101, sur un code identique à la v0.2.9 ; `git show 915dc2c^:<chemin>`), **gelées**, ce sont la spécification que le Rust doit reproduire |
| `scripts/` | `check.sh` (point d'entrée unique de vérification), `checks/rust.sh`, `checks/hooks.sh` (les hooks de `.claude/hooks` sans `python3`, L-119) et `checks/outils.sh` (`lib/outils.sh` : ffmpeg cherché comme par `ToolResolver`, L-121) |
| `.github/` | CI (`rust.yml`, `lint.yml`, `release.yml`, `merge-on-label.yml`), `scripts/nvenc-driver-floor.sh` (plancher pilote NVENC) |
| `packaging/` | `windows/superview.iss` (installateur Inno Setup), `linux/build_appimage.sh` et `check_appimage_libs.sh` (AppImage) : DEC-41, DEC-47 ; `ofx/` (bundle du plugin Resolve, DEC-65) |
| `docs/` | contrats, décisions, journaux (voir ci-dessous), `archives/` |
| `superview.yaml` | configuration livrée |

## Flux de données
```
Démarrage : check_ffmpeg → probe_hardware_support → apply_encoder_probe
Fichier   : check_video → EncodingSession::new → generate_pgm → encode_video → fermeture de la session → MP4
```
Les cartes PGM vont dans le dossier de session ; ffmpeg lit l'entrée, applique `remap`, encode dans un
fichier de travail renommé au succès. Repli à trois niveaux (décodage matériel → CPU, encodeur matériel →
CPU, copie audio → AAC). L'annulation ne laisse rien : ni processus ffmpeg, ni sortie partielle, ni
dossier temporaire.

## Dépendances externes
| Service / lib | Pourquoi | Doc locale |
|---------------|----------|------------|
| FFmpeg / ffprobe | l'encodage ; livré dans l'installateur et l'AppImage, **épinglé** par plancher pilote NVENC | `RELEASING.md`, `docs/hardware-support.md` |
| Slint 1.18 (rendu logiciel, backend winit) | la fenêtre ; sans OpenGL ni Qt. winit charge `libxkbcommon-x11` à l'exécution (embarquée dans l'AppImage) | `docs/ENVIRONNEMENT.md`, DEC-43, DEC-47 |
| rfd (portail xdg, sans GTK) | sélecteurs de fichiers ; charge `libdbus` à l'exécution | DEC-43 |
| `serde-saphyr`, `serde_json` | YAML de configuration ; préférences et fixtures | DEC-43 |
| `fs4`, `tempfile`, `signal-hook` / `ctrlc`, `thiserror`, `log` | espace disque, dossier de session, signaux, erreurs, journal | — |
| Rust (rustup, `rust-toolchain.toml`) | la chaîne d'outils ; `cargo llvm-cov`, `cargo audit` et `cargo deny` pour le contrôle de couverture et des dépendances | `docs/ENVIRONNEMENT.md` |
| Inno Setup 6, `appimagetool` 1.9.1 | l'installateur Windows ; l'AppImage Linux (épinglé par SHA-256) | `RELEASING.md`, DEC-41 |

## Invariants (ce qui ne doit JAMAIS casser)
- Configuration passée explicitement (`check_ffmpeg(cfg, resolver)`, …) : pas de global ; horloge,
  environnement et mémoire sont injectés.
- Débits en **bits par seconde**, partout.
- `Command` avec arguments, jamais une chaîne shell ; chemins des sélecteurs = non fiables.
- Cartes PGM : `tests/pgm_golden.rs` fige la géométrie octet par octet.
- L'annulation ne laisse rien derrière elle.
- Toute PR passe par la CI ; les notes de version viennent des PR fusionnées.

## Où lire quoi
[CONTRATS.md](CONTRATS.md) (faits mesurés, à lire avant d'éditer un fichier Rust) · [DECISIONS.md](DECISIONS.md)
(ce qui ressemble à un défaut) · [PIEGES.md](PIEGES.md) (index des leçons) ·
[CORRECTIONS.md](CORRECTIONS.md) (historique des corrections) · [SUITE.md](SUITE.md) (où on en est).

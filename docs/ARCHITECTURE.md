# Architecture

> Claude : à lire avant toute nouvelle fonctionnalité. À mettre à jour quand la structure change.
> C'est une carte, pas un roman : les contrats détaillés sont dans [CONTRATS.md](CONTRATS.md).

## But du projet
Convertir une vidéo MP4 en 4:3 vers du 16:9 par distorsion dynamique : les bords sont étirés, le
centre garde son ratio. Une interface graphique (Go + Fyne, Windows et Linux, **pas de CLI**) qui
génère deux cartes de remappage PGM et les confie au filtre `remap` de FFmpeg. MP4 en entrée, MP4 en
sortie : contrainte produit, pas un oubli.

## Vue d'ensemble
- **`cmd/superview/`** — l'interface : fenêtre, sélecteurs de fichiers, état (`appState`), boîtes de
  dialogue natives (zenity/kdialog, PowerShell). L'encodage tourne dans une goroutine ; tout widget
  touché depuis elle passe par `fyne.Do`.
- **`internal/common/`** — toute la logique, sans dépendance à Fyne. Un fichier par responsabilité :

| Fichier | Rôle |
|---------|------|
| `pipeline.go` | `PerformEncoding` : validations, fichier de travail, métriques |
| `encode.go` | un passage ffmpeg : arguments, chaîne de filtres, cascade de repli |
| `pgm.go` | géométrie de sortie, formule de distorsion, écriture des cartes PGM |
| `session.go` | dossier temporaire isolé par conversion (`InitEncodingSession` / `CleanUp`) |
| `video.go`, `tools.go` | lecture et validation d'ffprobe ; recherche d'ffmpeg/ffprobe et de leurs capacités |
| `hardware.go`, `probe.go` | classement des encodeurs, accélération matérielle, sonde d'une trame réelle |
| `memory.go` | garde-fou mémoire, chiffré par pixel de sortie et par fil |
| `security.go`, `errors.go` | validation des chemins ; erreurs typées (`InvalidVideoError`, `EncoderError`, `SessionError`) |
| `config.go`, `health.go`, `metrics.go`, `observability.go` | configuration, bouton Diagnostic, métriques, journal |

## Arborescence
| Dossier | Rôle |
|---------|------|
| `cmd/superview/` | point d'entrée GUI, `FyneApp.toml`, `Icon.png` (lus par `fyne package`) |
| `internal/common/` | la logique ; `testdata/ffprobe/` = sorties ffprobe enregistrées |
| `scripts/` | `check.sh` (point d'entrée unique de vérification) et `checks/go.sh` |
| `.github/` | CI, release, `scripts/nvenc-driver-floor.sh` (plancher pilote NVENC) |
| `rust/` | **port en cours (DEC-43)** : workspace Cargo `superview-core` (logique : `error`, `video`, `pgm`, `config`, `security`, `tools` = recherche d'ffmpeg/ffprobe par un `ToolResolver` à environnement injecté, `process` = commandes sans fenêtre et `killed_by_signal`, `hardware` = choix de l'encodeur et du chemin matériel (fonctions pures, `find_encoder` compris ; cœurs et profil injectés) ; YAML par `serde-saphyr`), `superview-harness` (banc d'essai jamais livré, DEC-44), `superview-app` (fenêtre Slint d'essai : `ui/main.slint`, `selection.rs` = logique sans fenêtre, `main.rs` = colle Slint + sélecteur rfd). Go reste la référence |
| `packaging/` | `windows/superview.iss` (installateur Inno Setup), `linux/build_appimage.sh` (AppImage) : DEC-41 |
| `docs/` | contrats, décisions, journaux (voir ci-dessous), `archives/` |

## Flux de données
```
Démarrage : CheckFfmpeg → ProbeHardwareSupport → ApplyEncoderProbe
Fichier   : CheckVideo → InitEncodingSession → GeneratePGM → EncodeVideo → CleanUp → MP4
```
Les cartes PGM vont dans le dossier de session ; ffmpeg lit l'entrée, applique `remap`, encode.
Repli à trois niveaux (décodage matériel → CPU, encodeur matériel → CPU, copie audio → AAC). L'annulation ne laisse rien : ni
processus ffmpeg, ni sortie partielle, ni dossier temporaire.

## Dépendances externes
| Service / lib | Pourquoi | Doc locale |
|---------------|----------|------------|
| Fyne v2 (2.8) | interface graphique (cgo : en-têtes GL/X11/Wayland requis) | `docs/ENVIRONNEMENT.md` |
| FFmpeg / ffprobe | l'encodage ; livré dans l'installateur et l'AppImage, **épinglé** par plancher pilote NVENC | `RELEASING.md`, `docs/hardware-support.md` |
| `fyne.io/tools` | `fyne package` à la release (version tirée du tag) ; seul son binaire est repris | `RELEASING.md` |
| Rust (rustup, `rust/rust-toolchain.toml`), Slint | le port en cours ; Slint (rendu logiciel, backend winit) et rfd (portail xdg, sans GTK) pour la fenêtre d'essai (phase 3b) | `docs/ENVIRONNEMENT.md`, DEC-43 |
| Inno Setup 6, `appimagetool` 1.9.1 | l'installateur Windows ; l'AppImage Linux (épinglé par SHA-256) | `RELEASING.md`, DEC-41 |

## Invariants (ce qui ne doit JAMAIS casser)
- Configuration passée explicitement (`CheckFfmpeg(cfg)`, …) : pas de global de configuration.
- Débits en **bits par seconde**, partout.
- `exec.Command` avec arguments, jamais une chaîne shell ; chemins des sélecteurs = non fiables.
- Cartes PGM : `pgm_golden_test.go` fige la géométrie octet par octet.
- Toute PR passe par la CI ; les notes de version viennent des PR fusionnées.

## Où lire quoi
[CONTRATS.md](CONTRATS.md) (faits mesurés, à lire avant d'éditer un `.go`) · [DECISIONS.md](DECISIONS.md)
(ce qui ressemble à un défaut) · [PIEGES.md](PIEGES.md) (index des leçons) ·
[CORRECTIONS.md](CORRECTIONS.md) (historique des corrections) · [SUITE.md](SUITE.md) (où on en est).

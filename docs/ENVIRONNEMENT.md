# Reconstruire l'environnement de vérification

> **Ce fichier décrit un poste de travail, pas le projet.** Rien ici n'est un
> prérequis pour utiliser Superview. Pour compiler et tester, il faut Rust (installé par
> rustup, sans `sudo`), FFmpeg et ffprobe, et un seul paquet de développement.
>
> Il y avait ici, du temps de Go et Fyne, un montage de « sysroot » d'en-têtes GL/X11/Wayland pour
> compiler sans `sudo` (`/tmp/glue/sysroot`, `/tmp/guienv.sh`). Il est **inutile** depuis le retrait
> de Go (DEC-49) : la fenêtre Slint ne lie que `libfontconfig`. Les journaux anciens le mentionnent
> encore ; ne pas le reconstruire.

## Ce qui survit à un redémarrage, et ce qui n'y survit pas

| Composant | Emplacement | Survit au redémarrage |
| --- | --- | --- |
| Toolchain Rust | `~/.cargo`, `~/.rustup` | oui |
| `ffmpeg`, `ffprobe`, `zenity` | système | oui |
| `libfontconfig1-dev` (le seul paquet de développement) | système | oui |

> **Pourquoi FFmpeg reste nécessaire ici alors que les paquets de release (installateur, AppImage) en embarquent
> un.** `ToolResolver::resolve` cherche d'abord `SUPERVIEW_FFMPEG_DIR` si elle est définie, puis la copie livrée à côté de l'exécutable ; un
> `cargo build` n'en produit aucune, donc un poste de développement travaille avec celui du
> `PATH`. Conséquence à garder en tête : **les sondes d'encodeurs et les tests
> d'intégration mesurent le FFmpeg du système, pas celui que les utilisateurs
> recevront.** Pour éprouver le binaire empaqueté, pointer `SUPERVIEW_FFMPEG_DIR`
> dessus.

```bash
# Réflexe d'ouverture de session
command -v cargo >/dev/null && cargo --version || echo "toolchain à réinstaller (voir ci-dessous)"
command -v ffmpeg >/dev/null && command -v ffprobe >/dev/null || echo "ffmpeg/ffprobe manquent"
```

## Reconstruire depuis zéro

```bash
# 1. Toolchain Rust, sans sudo -- la version exacte est épinglée par rust/rust-toolchain.toml
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o /tmp/rustup-init.sh
sh /tmp/rustup-init.sh -y --profile minimal
. "$HOME/.cargo/env" && rustup component add rustfmt clippy rust-docs && cargo --version

# 2. Avec sudo : FFmpeg et le seul paquet de développement de la fenêtre
sudo apt install -y ffmpeg libfontconfig1-dev
```

Les hooks (pre-commit, Claude) tournent dans un shell non interactif : `scripts/checks/rust.sh`
ajoute `~/.cargo/bin` au `PATH` lui-même.

Sans `sudo` : `libfontconfig1-dev` est souvent déjà là (c'est le cas de ce poste, qui a aussi
`libxkbcommon-dev`, `libwayland-dev`, `libx11-dev` sans que le build les exige). FFmpeg se pose alors
dans un dossier du `PATH` ou derrière `SUPERVIEW_FFMPEG_DIR`.

## Fenêtre Slint

`superview-app` dépend de `slint` (features `std`, `backend-winit`, `renderer-software`, `compat-1-18`,
sans les défauts : ni OpenGL/femtovg, ni Qt, ni accessibilité) et de `rfd` (feature `xdg-portal` seule : le
sélecteur passe par le portail D-Bus, **pas de GTK**). Mesuré le 2026-10-14 sur ce poste :

- La CI Linux installe `ffmpeg` et `libfontconfig1-dev` pour compiler et tester
  (`.github/workflows/rust.yml`, jobs *Rust on ubuntu-latest* et *Rust coverage and audit*) ; pour
  lancer la fenêtre sous `xvfb`, elle ajoute `xvfb` et `libxkbcommon-x11-0`. La seule
  bibliothèque graphique à installer est `libfontconfig.so.1` (le binaire lie aussi ses
  dépendances : freetype, expat, zlib…) ; X11, Wayland et xkbcommon
  sont chargés à l'exécution.
- Pour **empaqueter** l'AppImage (`packaging/linux/build_appimage.sh`), le poste doit avoir
  `libxkbcommon0`, `libxkbcommon-x11-0` et `libxcb-xkb1` : ils sont copiés dans l'image (DEC-47).
- Build complet à froid (dépendances comprises) : environ 35 s en debug, 67 s en release (machine multicœur) ;
  binaire release : 18,4 Mo (LTO et symboles retirés, DEC-50 ; 24,8 Mo avant).
- Lancement : `timeout 5 cargo run -p superview-app --locked` sort avec le code 124 (fenêtre restée ouverte),
  sous Wayland comme sous X11 (`env -u WAYLAND_DISPLAY`). Sans session graphique, la CI utilise
  `xvfb-run -a timeout 10`.
- `rfd` 0.17.2 charge `libdbus-1.so.3` par `dlopen` (rien n'est lié à la compilation) et se rabat sur
  `zenity` si le portail échoue ; `dialogs.rs` essaie ensuite `zenity` et `kdialog` lui-même, puis offre
  un champ texte dans la fenêtre (DEC-43, phase 6.4).
- Les tests de la fenêtre utilisent `i-slint-backend-testing` : aucun écran requis.

## Outils d'analyse

Le contrôle de couverture et l'audit de la CI (job *Rust coverage and audit*) se rejouent en local avec
`make coverage-audit`. Les versions sont épinglées dans `.github/workflows/rust.yml` ; les installer
avec les mêmes :

```bash
rustup component add llvm-tools
cargo install --locked --version 0.9.1 cargo-llvm-cov
cargo install --locked --version 0.22.2 cargo-audit
cargo install --locked --version 0.20.2 cargo-deny
```

`cargo audit` lit la base d'avis la plus récente, qui n'est pas épinglée : un avis publié le matin
peut rougir la CI l'après-midi (DEC-47) ; la parade est un `ignore` motivé dans `rust/deny.toml` (`cargo deny` ; `cargo audit` n’a pas de fichier de configuration ici), pas le retrait de l'étape.

## Lancer la suite comme la CI la lance

```bash
cd rust && SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked
```

`SUPERVIEW_REQUIRE_FFMPEG=1` transforme en échec les tests qui sautent faute d'ffmpeg. Sans
elle, une suite verte peut n'avoir encodé aucune image.

# Sources à consulter avant toute modification du code

> Dernière mise à jour : 2026-09-04
> **À lire avant d'ouvrir un fichier `.go` de ce dépôt.** L'ordre importe : les sections 1 à 3
> conditionnent la validité de toute modification.

---

## 1. Environnement de vérification — **installé et éprouvé**

État au 2026-09-04 : **opérationnel**. Le module entier compile, se teste et se lint
localement. `sudo` n'étant pas disponible (authentification interactive impossible), tout a été
installé dans l'espace utilisateur.

### Rétablir l'environnement depuis zéro

```bash
# 1. Toolchain Go (sans sudo) — utiliser la dernière 1.26.x, comme la CI
cd /tmp && curl -sLO https://go.dev/dl/go1.26.8.linux-amd64.tar.gz
rm -rf ~/.local/go && mkdir -p ~/.local && tar -C ~/.local -xzf go1.26.8.tar.gz
export PATH=$HOME/.local/go/bin:$PATH && go version

# 2. En-têtes GUI sans sudo : télécharger les .deb et les extraire dans un sysroot
mkdir -p /tmp/glue/debs && cd /tmp/glue/debs
apt-get download libgl-dev libgl1-mesa-dev libglx-dev libx11-dev libxcursor-dev \
  libxrandr-dev libxinerama-dev libxi-dev libxxf86vm-dev libxext-dev \
  libxrender-dev libxfixes-dev x11proto-dev libglvnd-dev libegl-dev libopengl-dev
cd /tmp/glue && for d in debs/*.deb; do dpkg -x "$d" sysroot; done

# 3. Faire pointer les symlinks .so du sysroot vers les bibliothèques système
cd /tmp/glue/sysroot/usr/lib/x86_64-linux-gnu
for f in *.so; do t=$(readlink "$f"); [ -n "$t" ] && [ ! -e "$t" ] && \
  [ -e "/usr/lib/x86_64-linux-gnu/$t" ] && ln -sf "/usr/lib/x86_64-linux-gnu/$t" "$f"; done
```

### Le fichier d'environnement à sourcer avant tout travail

`/tmp/guienv.sh` — **sans lui, le paquet racine ne compile pas** :

```bash
export PATH=$HOME/.local/go/bin:$PATH
export SYSROOT=/tmp/glue/sysroot
export PKG_CONFIG_PATH=$SYSROOT/usr/lib/x86_64-linux-gnu/pkgconfig:$SYSROOT/usr/share/pkgconfig
export CGO_CFLAGS="-I$SYSROOT/usr/include"
export CGO_LDFLAGS="-L$SYSROOT/usr/lib/x86_64-linux-gnu -L/usr/lib/x86_64-linux-gnu"
```

> Si `sudo` redevient disponible, `sudo apt install -y libgl1-mesa-dev xorg-dev` rend tout ce
> montage inutile — c'est la voie à privilégier.

### Outils d'analyse

```bash
go install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@v2.13.2  # noter le /v2
go install honnef.co/go/tools/cmd/staticcheck@latest
go install golang.org/x/vuln/cmd/govulncheck@latest
```

> ⚠️ **`.../golangci-lint/cmd/golangci-lint@latest` (sans `/v2`) installe silencieusement la
> dernière v1**, en fin de vie. C'était le piège du constat X-04.

### Dépendances runtime — présentes

`ffmpeg` 8.0.1, `ffprobe`, `zenity` : déjà installés sur cette machine.

### Réflexe de début de session

```bash
command -v go >/dev/null && go version || echo "toolchain à réinstaller (voir ci-dessus)"
ls /tmp/glue/sysroot/usr/include/GL/gl.h 2>/dev/null || echo "sysroot à reconstruire"
```

Le sysroot vit dans `/tmp` : il **ne survit pas à un redémarrage**. La toolchain, dans
`~/.local/go`, oui.

## 2. Sources internes au dépôt — par ordre de priorité

| # | Source | Quand la consulter | Fiabilité |
| --- | --- | --- | --- |
| 1 | [`output_claude/ANALYSE_PROJET.md`](ANALYSE_PROJET.md) | Toujours. Constats numérotés B/S/C/X/O/T, priorités. | ✅ à jour |
| 2 | [`output_claude/LESSONS.md`](LESSONS.md) | Toujours. Corrections déjà appliquées, leçons permanentes, file d'attente. | ✅ à jour |
| 3 | `go.mod` | Version Go et dépendances exactes avant toute modification d'API. | ✅ fait foi |
| 4 | `common/*_test.go` | **Avant de modifier une fonction de `common/`** : le test décrit le contrat attendu. | ✅ fait foi |
| 4bis | `common/pgm_golden_test.go` | **Avant de toucher à `GeneratePGM`** : empreintes SHA-256 figeant la sortie exacte de l'algorithme. | ✅ fait foi |
| 4ter | `common/integration_test.go` | Contrat bout-en-bout avec un vrai FFmpeg (ratio de sortie, progression, annulation). | ✅ fait foi |
| 5 | `superview.yaml` | Options de configuration effectivement livrées. | ⚠️ `min_video_*` et `quality_preset` restent sans effet (C-03, non tranché) |
| 6 | `.golangci.yml` | Règles de lint appliquées. Schéma v2 ; staticcheck cadré sur `SA*`+`S1*` (`QF*` exclus à dessein, cf. L-09/L-12). | ✅ à jour |
| 7 | `.github/workflows/*.yml` | Ce qui est réellement vérifié en CI — désormais `./...`, seuil de couverture 50 %. | ✅ à jour |
| 8 | `README.md` | Comportement documenté côté utilisateur. | ✅ à jour |
| 9 | `Makefile` | Cibles de build locales. | ✅ corrigé (X-02) |
| 10 | `.github/copilot-instructions.md` | Conventions du projet. | ✅ corrigé (X-03) |
| 11 | `build.sh` | Script de release historique. | 🗑️ marqué obsolète, refuse de s'exécuter (X-08) |
| 12 | ~~`coverage.out`~~ | — | 🗑️ supprimé du dépôt et ignoré par git (X-06) |

### Fichiers à traiter avec la plus grande prudence

- **`common/common.go`** (1080 l.) — cœur du pipeline. Toute modification touche l'encodage réel.
  Sous-sections sensibles : `GeneratePGM` (mathématiques du remappage — ne pas « simplifier »
  sans vidéo de référence), `EncodeVideo` (cascade de repli à trois niveaux + gestion de
  signaux + goroutines).
- **`gui_main.go`** (602 l.) — non couvert par la CI ni par des tests. Une régression ici
  n'est détectée par personne.
- **`common/security.go`** — modifier une validation, c'est modifier une frontière de sécurité.
  Justifier chaque assouplissement.

---

## 3. Règles de vérification propres à ce dépôt

### Avant de modifier

1. Lire `LESSONS.md` — la correction a peut-être déjà été tentée et rejetée.
2. Lire le test correspondant dans `common/*_test.go`.
3. Vérifier que le symbole n'est pas du code mort (constats C-01, C-02, C-03, C-06) :
   ```bash
   grep -rn "NomDuSymbole" --include='*.go' . | grep -v '_test.go'
   ```
   Deux occurrences seulement (déclaration + commentaire) = jamais appelé.

### Après avoir modifié

```bash
source /tmp/guienv.sh            # sinon le paquet racine ne compile pas (voir § 1)

gofmt -l .                       # doit ne rien afficher — la CI échoue sinon
go build ./...
go vet ./...
go test ./... -race -count=1
"$(go env GOPATH)/bin/staticcheck" ./...
"$(go env GOPATH)/bin/golangci-lint" run ./... --timeout=5m
"$(go env GOPATH)/bin/govulncheck" ./...

go test ./... -coverprofile=/tmp/cov.out -covermode=atomic
go tool cover -func=/tmp/cov.out | grep total   # doit rester ≥ 50 % (seuil CI)
```

Ces sept commandes sont exactement ce que la CI exécute. Toutes doivent passer avant de
proposer une modification.

> Écrire le profil de couverture dans `/tmp`, **jamais** à la racine : `coverage.out` est
> désormais ignoré par git, mais un fichier généré à la racine reste du bruit.

### Vérification fonctionnelle réelle

Une compilation verte ne prouve pas que la GUI démarre ni que la conversion fonctionne :

```bash
go build -o /tmp/sv . && timeout 12 /tmp/sv   # code 124 = toujours vivant, donc OK
cat ~/.cache/superview/superview.log          # les diagnostics y atterrissent (O-02)

go test ./common -run TestIntegration -v      # conversion réelle 4:3 → 16:9
```

### Validation fonctionnelle sans vidéo réelle

```bash
# Génère un clip 4:3 de 5 s pour un test bout-en-bout
ffmpeg -f lavfi -i testsrc=size=640x480:rate=30 -t 5 -c:v libx264 -b:v 1M /tmp/in43.mp4
ffprobe -v error -select_streams v:0 \
        -show_entries stream=codec_name,width,height,duration,bit_rate \
        -print_format json /tmp/in43.mp4
```

Le second appel reproduit **exactement** celui de `CheckVideo` (`common/common.go:383`) : il
permet de vérifier qu'un fichier passera la validation avant de lancer la GUI.

### Cas limites à retester systématiquement

| Chemin | Pourquoi |
| --- | --- |
| FFmpeg absent du `PATH` | `CheckFfmpeg` retourne `nil` → la GUI manipule une map nulle (B-05) |
| Dialogue natif indisponible (pas de zenity/kdialog) | Bascule sur le repli Fyne, buggé sous Windows (B-03) |
| Annulation en cours d'encodage | Deux chemins d'annulation concurrents (B-06) |
| Encodeur matériel refusé par le pilote | Cascade de repli à trois niveaux dans `EncodeVideo` |
| Chemin contenant `..` ou un lien symbolique | Rejeté par `security.go` (S-01, S-02) |
| Lancement hors du répertoire du dépôt | `superview.yaml` introuvable (B-02) |

---

## 4. Références externes

### FFmpeg — autorité sur tout le comportement d'encodage

| Sujet | URL | Utile pour |
| --- | --- | --- |
| Filtre `remap` | https://trac.ffmpeg.org/wiki/RemapFilter | Format PGM P2, sémantique des cartes X/Y — référence citée dans `common.go:462` |
| Documentation des filtres | https://ffmpeg.org/ffmpeg-filters.html#remap | Contraintes exactes du filtre |
| Options principales | https://ffmpeg.org/ffmpeg.html#Main-options | **Position des options** : avant `-i` = entrée, après = sortie (cf. B-04) |
| `-progress` / `out_time_ms` | https://ffmpeg.org/ffmpeg.html#Advanced-options | ⚠️ `out_time_ms` est en **microsecondes** malgré son nom — c'est ce que suppose le calcul `duration*10000` de `common.go:788` |
| Accélération matérielle | https://trac.ffmpeg.org/wiki/HWAccelIntro | Jetons `-hwaccel` valides par plateforme |
| NVENC | https://docs.nvidia.com/video-technologies/video-codec-sdk/nvenc-video-encoder-api-prog-guide/ | Presets `h264_nvenc` / `hevc_nvenc` |
| AMD AMF | https://ffmpeg.org/ffmpeg-codecs.html#toc-AMD-AMF-video-encoders | Presets `speed`/`balanced`/`quality` — mappés dans `mapVideoPresetForEncoder` |
| Intel QSV | https://trac.ffmpeg.org/wiki/Hardware/QuickSync | Presets QSV |
| Format PGM | https://netpbm.sourceforge.net/doc/pgm.html | En-tête `P2 <w> <h> 65535` |

### Go

| Sujet | URL |
| --- | --- |
| `os/exec` | https://pkg.go.dev/os/exec |
| `log/slog` | https://pkg.go.dev/log/slog |
| `bufio.Reader.ReadLine` | https://pkg.go.dev/bufio#Reader.ReadLine — sémantique des erreurs, pertinente pour B-01 |
| Conventions de messages d'erreur | https://go.dev/wiki/CodeReviewComments#error-strings — cf. C-07 |
| Détecteur de courses | https://go.dev/doc/articles/race_detector |
| `path/filepath` | https://pkg.go.dev/path/filepath — `Clean`, `EvalSymlinks`, `IsAbs` (S-01, S-02) |

### Fyne (GUI)

| Sujet | URL |
| --- | --- |
| API v2 | https://pkg.go.dev/fyne.io/fyne/v2 |
| Threading — `fyne.Do` | https://docs.fyne.io/started/updating — **obligatoire** : toute mise à jour d'UI depuis une goroutine doit passer par `fyne.Do` |
| `storage` / URI | https://pkg.go.dev/fyne.io/fyne/v2/storage — alternative correcte au `ReplaceAll("file://")` de B-03 |
| Empaquetage | https://docs.fyne.io/started/packaging |
| Tests de widgets | https://pkg.go.dev/fyne.io/fyne/v2/test — `test.NewApp()`, pour T-01 |
| Dépendances Linux | https://docs.fyne.io/started/ — `libgl1-mesa-dev`, `xorg-dev` |

### Outillage CI

| Sujet | URL |
| --- | --- |
| golangci-lint — migration v1 → v2 | https://golangci-lint.run/product/migration-guide/ — nécessaire pour X-04 |
| Linters disponibles | https://golangci-lint.run/usage/linters/ |
| staticcheck (codes SA/ST) | https://staticcheck.dev/docs/checks/ |
| govulncheck | https://go.dev/blog/govulncheck |
| Épinglage des actions GitHub | https://docs.github.com/actions/security-guides/security-hardening-for-github-actions#using-third-party-actions — cf. X-05 |

### Algorithme d'origine

- Implémentation Python de Banelle : https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview
  — **source de vérité mathématique** pour `GeneratePGM`. Toute modification de la formule
  d'offset doit être confrontée à cette référence.
- Dépôt amont : https://github.com/Canaill51/superview

---

## 5. Journal des révisions de ce document

| Date | Modification |
| --- | --- |
| 2026-09-04 | Création. Pré-requis toolchain, hiérarchie des sources internes avec indice de fiabilité, procédure de vérification, références FFmpeg/Go/Fyne/CI. |
| 2026-09-04 | § 1 réécrit : environnement installé et éprouvé (Go dans `~/.local`, sysroot GUI sans sudo). Indices de fiabilité mis à jour après correction des fichiers. Procédure de vérification alignée sur la CI étendue à `./...`. |

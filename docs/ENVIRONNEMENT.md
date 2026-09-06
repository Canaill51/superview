# Reconstruire l'environnement de vérification

> **Ce fichier décrit un poste de travail, pas le projet.** Rien ici n'est un
> prérequis pour utiliser Superview, ni pour le compiler sur une machine où
> `sudo` fonctionne : là, `sudo apt install -y ffmpeg libgl1-mesa-dev xorg-dev
> libwayland-dev libxkbcommon-dev` suffit et rend tout ce qui suit inutile.
>
> Le montage décrit ici existe pour un poste sans `sudo`. Il est consigné parce
> que sans lui le paquet `main` ne compile pas, et qu'une modification de la GUI
> annoncée sans avoir compilé n'est pas une modification vérifiée ([L-01](LECONS.md)).

## Ce qui survit à un redémarrage, et ce qui n'y survit pas

| Composant | Emplacement | Survit au redémarrage |
| --- | --- | --- |
| Toolchain Go | `~/.local/go` | oui |
| Sysroot GUI (en-têtes GL/X11/Wayland) | `/tmp/glue/sysroot` | **non — à reconstruire** |
| `ffmpeg`, `ffprobe`, `zenity` | système | oui |

> **Pourquoi FFmpeg reste nécessaire ici alors que les archives de release en embarquent
> un.** `resolveToolBinary` cherche d'abord la copie livrée à côté de l'exécutable ; un
> `go build` n'en produit aucune, donc un poste de développement travaille avec celui du
> `PATH`. Conséquence à garder en tête : **les sondes d'encodeurs et les tests
> d'intégration mesurent le FFmpeg du système, pas celui que les utilisateurs
> recevront.** Pour éprouver le binaire empaqueté, pointer `SUPERVIEW_FFMPEG_DIR`
> dessus.

Conséquence en début de session : `./common` se compile, se teste sous `-race` et
se lint immédiatement ; le paquet `main` échoue sur
`wayland-client-core.h: No such file or directory` tant que le sysroot n'est pas
refait.

```bash
# Réflexe d'ouverture de session
command -v go >/dev/null && go version || echo "toolchain à réinstaller"
ls /tmp/glue/sysroot/usr/include/wayland-client-core.h 2>/dev/null || echo "sysroot à reconstruire"
```

## Reconstruire depuis zéro

```bash
# 1. Toolchain Go, sans sudo -- prendre la dernière 1.26.x, comme la CI
cd /tmp && curl -sLO https://go.dev/dl/go1.26.8.linux-amd64.tar.gz
rm -rf ~/.local/go && mkdir -p ~/.local && tar -C ~/.local -xzf go1.26.8.linux-amd64.tar.gz
export PATH=$HOME/.local/go/bin:$PATH && go version

# 2. En-têtes GUI sans sudo : télécharger les .deb et les extraire dans un sysroot.
#
#    Les quatre derniers paquets sont ceux de Wayland. Ils ne figuraient pas
#    dans la liste d'origine, écrite avant le passage à Fyne 2.8 : celle-ci
#    produisait un sysroot où GL et X11 étaient présents et où la compilation
#    échouait quand même, sur wayland-client-core.h. Fyne 2.8 est passé à
#    GLFW 3.4 et à son backend Wayland (cf. README, § Linux).
mkdir -p /tmp/glue/debs && cd /tmp/glue/debs
apt-get download libgl-dev libgl1-mesa-dev libglx-dev libx11-dev libxcursor-dev \
  libxrandr-dev libxinerama-dev libxi-dev libxxf86vm-dev libxext-dev \
  libxrender-dev libxfixes-dev x11proto-dev libglvnd-dev libegl-dev libopengl-dev \
  libwayland-dev libwayland-client0 libxkbcommon-dev wayland-protocols libffi-dev
cd /tmp/glue && rm -rf sysroot && for d in debs/*.deb; do dpkg -x "$d" sysroot; done

# 3. Faire pointer les symlinks .so du sysroot vers les bibliothèques système
cd /tmp/glue/sysroot/usr/lib/x86_64-linux-gnu
for f in *.so; do t=$(readlink "$f"); [ -n "$t" ] && [ ! -e "$t" ] && \
  [ -e "/usr/lib/x86_64-linux-gnu/$t" ] && ln -sf "/usr/lib/x86_64-linux-gnu/$t" "$f"; done
```

## Le fichier à sourcer avant tout travail

`/tmp/guienv.sh` — **sans lui, le paquet racine ne compile pas** :

```bash
export PATH=$HOME/.local/go/bin:$PATH
export SYSROOT=/tmp/glue/sysroot
export PKG_CONFIG_PATH=$SYSROOT/usr/lib/x86_64-linux-gnu/pkgconfig:$SYSROOT/usr/share/pkgconfig
export CGO_CFLAGS="-I$SYSROOT/usr/include"
export CGO_LDFLAGS="-L$SYSROOT/usr/lib/x86_64-linux-gnu -L/usr/lib/x86_64-linux-gnu"
```

## Outils d'analyse

Mêmes versions que la CI — `.github/workflows/lint.yml` et le `Makefile` les
épinglent tous deux, et `make install-tools` les installe :

```bash
go install github.com/golangci/golangci-lint/v2/cmd/golangci-lint@v2.13.2  # noter le /v2
go install golang.org/x/vuln/cmd/govulncheck@v1.7.0
```

> ⚠️ `.../golangci-lint/cmd/golangci-lint@latest`, **sans `/v2`**, installe
> silencieusement la dernière v1, en fin de vie. C'était le piège du constat X-04.

## Lancer la suite comme la CI la lance

```bash
. /tmp/guienv.sh
SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...
```

`SUPERVIEW_REQUIRE_FFMPEG=1` transforme en échec les `t.Skip` liés à ffmpeg. Sans
elle, une suite verte peut n'avoir encodé aucune image.

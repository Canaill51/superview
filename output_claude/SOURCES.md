# Sources à consulter avant toute modification du code

> Dernière mise à jour : 2026-09-04
> **À lire avant d'ouvrir un fichier `.go` de ce dépôt.** L'ordre importe : les sections 1 à 3
> conditionnent la validité de toute modification.

---

## 1. Environnement de vérification — **installé et éprouvé**

État au 2026-09-04 : **partiellement opérationnel**. `sudo` n'étant pas disponible
(authentification interactive impossible), tout a été installé dans l'espace utilisateur.

| Composant | Emplacement | Survit au redémarrage |
| --- | --- | --- |
| Toolchain Go 1.26.8 | `~/.local/go` | ✅ oui |
| Sysroot GUI (en-têtes GL/X11/Wayland) | `/tmp/glue/sysroot` | ❌ **non — à reconstruire** |
| `ffmpeg` 8.0.1, `ffprobe`, `zenity` | système | ✅ oui |

Conséquence pratique en début de session : `./common` se compile, se teste sous `-race` et se
lint immédiatement ; le paquet `main` échoue sur `wayland-client-core.h: No such file or
directory` tant que le sysroot n'est pas refait. **Ne pas annoncer une modification de la GUI
comme vérifiée dans cet état** (L-01).

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

### Contrat de configuration (depuis la 2ᵉ passe)

Il n'existe plus de configuration globale. `GetConfig()` et `SetConfig()` ont été supprimés.
`*Config` se passe explicitement à `CheckFfmpeg`, `InitEncodingSession`, `EncodeVideo`,
`PerformEncoding` et `CheckHealth` ; `nil` signifie « valeurs par défaut ».

> N'introduisez pas de nouveau global mutable pour contourner ce passage de paramètre : c'est
> précisément ce que la refactorisation C-05 a supprimé, et la cause de l'écrasement silencieux
> du `video_preset` utilisateur.

Règle de priorité du preset : un `video_preset` renseigné dans la configuration l'emporte sur
celui du profil qualité de la GUI. Testée par `TestConfiguredPresetWinsOverQualityProfile`.

### Contrat FFmpeg — faits établis par mesure (4ᵉ passe, 2026-09-04)

Vérifiés en exécutant FFmpeg 8.0.1, pas déduits de la documentation. À traiter comme acquis.

| Fait | Conséquence |
| --- | --- |
| `-filter_complex` sans `-map` ne conserve **qu'une piste audio** | Il faut `-map "[v]" -map "0:a?"`. Le `?` est **obligatoire** : sans lui, une source muette fait échouer FFmpeg. |
| Avec trois entrées, les métadonnées globales sont **perdues** | Il faut `-map_metadata 0` pour garder `creation_time`. |
| `format=yuv444p,format=yuv420p` **ramène le 10 bits à 8 bits** | Les variantes `yuv444p10le,yuv420p10le` préservent la profondeur — vérifié : `yuv420p10le` en entrée ressort `yuv420p10le`. |
| Le filtre `remap` accepte le **PGM P5 binaire** 16 bits | Sortie **identique au bit près** au P2 ASCII (même SHA-256 sur les trames décodées), pour 55 % d'empreinte en moins. |
| Le PGM binaire est **gros-boutiste** par spécification | Un écrivain petit-boutiste produirait des cartes fausses **sans erreur**. |
| `-b:v` seul est une cible **moyenne**, sans plafond | Dépassement mesuré à +56 % (`libx264`). `-maxrate` et `-bufsize` sont nécessaires. |
| Une carte de remappage d'une seule trame est réutilisée pour toutes les trames | Pas besoin de boucler l'entrée. |
| **`libx265` refuse `-threads` au-delà de 16** | FFmpeg le fait correspondre à `--frame-threads` de x265. 17 et plus : `Cannot open libx265 encoder`, échec dur. `runtime.NumCPU()` ne peut donc pas être passé tel quel — voir `clampEncoderThreads` (P-11). |
| **`-b:v` seul dépasse jusqu'à +83 %** | Mesuré sur bruit incompressible à 8 Mbps : 14,7 Mbps sans plafond, 10,0 Mbps avec `-maxrate` égal à la consigne et `-bufsize` au double (+24 %), 10,4 Mbps avec la recommandation habituelle de 1,5× (+29 %). Le plafond retenu est **1×** — la valeur « standard » était la moins bonne. |
| `ffprobe` donne la cadence en rationnel (`30000/1001`) | Et `0/0` quand il ne sait pas ; `parseFrameRate` rend 0, que tous les appelants lisent comme « inconnu ». |
| Le filtre `remap` accepte des cartes 16 bits pour une source 10 bits | La profondeur des cartes et celle de la vidéo sont indépendantes. |

Recette de vérification d'un changement de chaîne de filtres — comparer la **sortie décodée**,
jamais le fichier de sortie (les en-têtes de conteneur diffèrent toujours) :

```bash
ffmpeg -v error -i avant.mp4 -f rawvideo -pix_fmt yuv420p - | sha256sum
ffmpeg -v error -i apres.mp4 -f rawvideo -pix_fmt yuv420p - | sha256sum
```

> ⚠️ **Ne jamais figer une de ces empreintes dans un test.** Elle dépend du build de l'encodeur
> et rougirait sur un autre runner pour une raison étrangère au code. Le patron correct est
> `TestGeneratePGM_RemapOutputIsStable` : comparer **deux exécutions du FFmpeg présent** plutôt
> qu'inscrire le résultat de l'un d'eux. Voir [LESSONS.md](LESSONS.md) L-36.

### Contrat de la chaîne de filtres (depuis la 4ᵉ passe)

`buildEncodeBaseArgs` doit émettre, dans cet ordre, après les trois `-i` :

```
-filter_complex "[0:v:0][1:v:0][2:v:0]remap,format=<inter>,format=<sortie>[v]"
-map "[v]" -map "0:a?" -map_metadata 0
```

- Le label `[v]` et les trois options qui suivent forment un tout : retirer l'un casse les deux
  autres. Sans eux, une piste audio et la date de prise de vue disparaissent (N-04, N-05).
- Le `?` de `0:a?` rend la sélection audio facultative — sans lui, une source muette échoue.
- `<inter>`/`<sortie>` valent `yuv444p10le`/`yuv420p10le` **uniquement** si la source dépasse
  8 bits *et* l'encodeur est de la famille HEVC (`remapFilterChain`). `h264_nvenc` ne sait pas
  encoder en 10 bits, et le High 10 de `libx264` se lit mal (N-03).
- Un `pix_fmt` vide ou non reconnu vaut 8 bits : c'est la direction qui préserve l'existant.

### Contrat du pipeline (depuis les correctifs de la 4ᵉ passe)

Invariants à ne pas casser, chacun tenu par un test :

| Invariant | Où | Pourquoi |
| --- | --- | --- |
| L'annulation se transporte par `ErrCancelled`, testée avec `errors.Is` | `EncodeVideo` | Une erreur non typée relançait la cascade de repli : 3 ffmpeg au lieu de 0 après un Cancel |
| L'encodage écrit dans un fichier de travail, renommé en place au succès | `PerformEncoding` | Sinon une annulation laisse un `.mp4` tronqué à destination et détruit le fichier précédent |
| L'entrée ne peut pas être la sortie | `sameOutputAsInput` | **Conséquence directe du point précédent** : ffmpeg ne voit plus le conflit, rien n'empêcherait le renommage d'écraser la source |
| Les débits sont en **bits**/seconde | partout | ffprobe `bit_rate` et ffmpeg `-b:v` sont en bits ; le code était juste, les commentaires faux |
| `-threads` n'est pas émis pour un encodeur matériel, et est plafonné à 16 pour `libx265` | `encoderThreadArgs` | Au-delà de 16, `libx265` refuse de s'ouvrir (P-11) |
| La géométrie des cartes est calculée par `remapOutputSize`, jamais recopiée | `GeneratePGM`, `checkTempSpaceForMaps` | Deux calculs divergent ; la vérification d'espace réserverait pour une carte qui n'est pas celle qu'on écrit |
| Une cadence d'images inconnue laisse `EncodingSpeed` à zéro | `computeMetrics` | Une métrique dérivée d'une constante inventée trompe plus qu'une métrique absente |

> **Écrire un test d'annulation** : fermer le canal *avant* l'appel ne teste rien — ffmpeg est
> tué avant d'exécuter sa première ligne, donc avant de créer son fichier. Déclencher
> l'annulation depuis le **rappel de progression**, et compter les lancements depuis
> `commandStdoutPipe` plutôt que depuis le processus qu'on va tuer. Voir L-40 et L-41.

### Contrat de l'état de la GUI (depuis P-10)

Tout l'état de session vit dans `appState` (`gui_main.go`), et **rien ne doit revenir en
variable capturée par closure dans `main()`** : c'est ce qui rendait les décisions
inatteignables par les tests, et c'est là que P-01 et P-02 s'étaient logés.

| Règle | Pourquoi |
| --- | --- |
| `beginEncoding()` **retourne** le canal d'annulation ; la goroutine ne relit jamais `state.cancel` | La relecture était la course P-02. La signature est ce qui l'empêche de revenir (L-44) |
| `requestCancel()` est le seul endroit qui ferme le canal, et le met à `nil` dans le même geste | Le bouton *Cancel* et l'interception de fermeture peuvent tous deux tirer ; fermer deux fois panique |
| Un contrôle qui doit être inerte pendant un encodage va dans `state.locked`, jamais dans une séquence d'`Enable`/`Disable` écrite à la main | Deux séquences parallèles divergent toujours — c'est P-01, et le sélecteur de codec oublié dans le chemin « ffmpeg indisponible » |
| Les widgets sont nil-vérifiés dans les méthodes | Ils sont assignés au fil de la construction de la fenêtre ; une réorganisation doit dégrader en « pas de mise à jour », pas en panique |
| Toute nouvelle transition d'état est une **méthode**, avec son test | Les onze méthodes actuelles sont à 100 % de couverture ; ne pas laisser ce filet se percer |

> Pour tester une transition, `newTestAppState(t)` (dans `gui_main_test.go`) construit un
> `appState` complet avec de vrais widgets sous `test.NewApp()`. Un `appState` dont les widgets
> sont nuls ne prouve rien : les assertions porteraient sur des sorties inexistantes.

### Contrat des cartes de remappage (depuis la 4ᵉ passe)

`GeneratePGM` écrit du **PGM P5**, binaire, 16 bits, **gros-boutiste** — l'ordre des octets est
imposé par la spécification PGM, et un écrivain petit-boutiste produirait des cartes fausses
sans aucune erreur. `putMapSample` est le **seul** endroit qui encode un échantillon : les deux
cartes doivent y passer, sinon l'invariant est défini deux fois et le test d'ordre devient
inopérant (c'est arrivé, voir L-37).

Trois tests le verrouillent, et ils ne sont pas interchangeables :

| Test | Ce qu'il prouve |
| --- | --- |
| `TestGeneratePGM_Golden` | Les octets exacts des cartes n'ont pas bougé par accident |
| `TestGeneratePGM_MapsAreBigEndianP5` | Le format sur le fil : magie P5, maxval 65535, ordre des octets |
| `TestGeneratePGM_RemapOutputIsStable` | Ce que FFmpeg **fait** des cartes, seule mesure qui parle du rendu |

### Cas limites à retester systématiquement

| Chemin | Pourquoi |
| --- | --- |
| FFmpeg absent du `PATH` | `CheckFfmpeg` retourne `nil` → la GUI manipule une map nulle (B-05, corrigé — non-régression) |
| Dialogue natif indisponible (pas de zenity/kdialog) | Bascule sur le repli Fyne |
| **Second encodage dans la même session** | Les widgets doivent tous revenir actifs — `squeezeCheck` ne l'est pas (P-01) |
| **Annulation immédiate, avant le premier octet de progression** | Course sur `cancelEncoding` (P-02) ; la cascade de repli relance FFmpeg (P-03) ; le fichier partiel reste (P-04) |
| Encodeur matériel refusé par le pilote | Cascade de repli à trois niveaux dans `EncodeVideo` |
| **Source 10 bits, ou à plusieurs pistes audio, ou horodatée** | Trois pertes silencieuses (N-03, N-04, N-05) |
| **Source à 60/120/240 fps** | `EncodingSpeed` suppose 30 fps en dur (P-06) |
| Chemin contenant `..` ou un lien symbolique | Rejeté par `security.go` (S-01, S-02) |
| Lancement hors du répertoire du dépôt | Résolution du `superview.yaml` (B-02, corrigé — non-régression) |

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
| 2026-09-04 | Ajout du contrat de configuration explicite (C-05) : plus de global mutable, `*Config` passée en paramètre. |
| 2026-09-04 | § 1 réécrit : environnement installé et éprouvé (Go dans `~/.local`, sysroot GUI sans sudo). Indices de fiabilité mis à jour après correction des fichiers. Procédure de vérification alignée sur la CI étendue à `./...`. |
| 2026-09-04 | 4ᵉ passe : § 1 requalifié (le sysroot de `/tmp` ne survit pas au redémarrage, le paquet `main` ne compile pas en l'état). Ajout du « Contrat FFmpeg — faits établis par mesure » et de la recette de comparaison par empreinte de sortie décodée. Cas limites étendus aux chemins où vivent P-01 à P-04, P-06 et N-03/N-04/N-05. |
| 2026-09-04 | Correctifs N-03/N-04/N-05, P-08 et P-11 appliqués : ajout des contrats « chaîne de filtres » et « cartes de remappage », du plafond `-threads` de `libx265` au contrat FFmpeg, et de la mise en garde contre les empreintes figées dépendant d'un build externe. |
| 2026-09-04 | Second lot de correctifs : ajout du « Contrat du pipeline » (7 invariants tenus par des tests), de la mesure du dépassement de débit et du format de cadence au contrat FFmpeg, et de la recette d'écriture d'un test d'annulation. |
| 2026-09-04 | P-10 appliqué : ajout du « Contrat de l'état de la GUI » (5 règles) et du point d'entrée `newTestAppState` pour tester une transition. |

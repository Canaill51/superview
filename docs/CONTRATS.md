# Contrats techniques du dépôt

> **À lire avant d'ouvrir un fichier `.go` de ce dépôt.** Ce fichier dit ce sur
> quoi on peut s'appuyer et ce qu'il ne faut pas casser. Il est maintenu ; les
> deux journaux ([ANALYSE.md](ANALYSE.md), [LECONS.md](LECONS.md)) racontent
> comment on en est arrivé là.
>
> Pour compiler le paquet `main` sur un poste sans `sudo`, voir
> [ENVIRONNEMENT.md](ENVIRONNEMENT.md).

---

## 1. Sources internes au dépôt — par ordre de priorité

| # | Source | Quand la consulter | Fiabilité |
| --- | --- | --- | --- |
| 1 | `common/*_test.go` | **Avant de modifier une fonction de `common/`** : le test décrit le contrat attendu. | fait foi |
| 1bis | `common/pgm_golden_test.go` | **Avant de toucher à `GeneratePGM`** : empreintes SHA-256 figeant la sortie exacte de l'algorithme. | fait foi |
| 1ter | `common/integration_test.go` | Contrat bout-en-bout avec un vrai FFmpeg (ratio de sortie, progression, annulation). | fait foi |
| 1quater | `common/testdata/ffprobe/` | Sorties ffprobe enregistrées : la forme exacte de la seule entrée que le programme ne produit pas lui-même. | fait foi |
| 2 | `go.mod` | Version Go et dépendances exactes avant toute modification d'API. | fait foi |
| 3 | `.golangci.yml` | Règles de lint appliquées. Schéma v2 ; staticcheck cadré sur `SA*`+`S1*`, `QF*` et `ST*` exclus à dessein (cf. [L-09, L-12](LECONS.md)). | fait foi |
| 4 | `.github/workflows/*.yml` | Ce qui est réellement vérifié en CI : `./...`, seuil de couverture 50 %, `SUPERVIEW_REQUIRE_FFMPEG=1`. | fait foi |
| 5 | `RELEASING.md` | **La** procédure de release. Il n'y en a plus d'autre. | fait foi |
| 6 | `superview.yaml` | Options de configuration effectivement livrées. Attention : le fichier livre `performance_mode: safe_performance`, alors que le défaut interne, appliqué en son absence, est `safe`. |  |
| 7 | [`ANALYSE.md`](ANALYSE.md) | Constats numérotés B/S/C/X/O/T/N/P/R et leur état. | journal, voir son en-tête |
| 8 | [`LECONS.md`](LECONS.md) | Corrections appliquées et leçons permanentes. | journal, voir son en-tête |
| 9 | `README.md` | Comportement documenté côté utilisateur. |  |
| 10 | `Makefile` | Cibles de build et de qualité locales. Même portée que la CI (`./...`). |  |
| 11 | [`AGENTS.md`](../AGENTS.md) | Conventions du projet pour les agents, et les pièges qui coûtent le plus cher. `CLAUDE.md` ne fait que l'importer — Claude Code ne lit pas `AGENTS.md`. |  |

> Les entrées `build.sh`, `tools/` et `coverage.out` ont disparu de cette table avec
> les fichiers eux-mêmes. `.github/copilot-instructions.md` aussi : son contenu vit
> désormais dans `AGENTS.md`, que Copilot lit nativement.

### Fichiers à traiter avec la plus grande prudence

- **`common/common.go`** — cœur du pipeline, et de loin le plus gros fichier du dépôt. Toute
  modification touche l'encodage réel. Sous-sections sensibles : `GeneratePGM` (mathématiques
  du remappage — ne pas « simplifier » sans vidéo de référence), `EncodeVideo` (cascade de
  repli à trois niveaux + gestion de signaux + goroutines).
- **`gui_main.go`** — la CI le compile et le teste désormais (`./...`), mais la couverture du
  paquet reste basse : `main()` construit des widgets, ce qu'aucun test ne traverse. Les
  *décisions* sont ailleurs, dans le type `appState`, et sont couvertes.
- **`common/security.go`** — modifier une validation, c'est modifier une frontière de sécurité.
  Justifier chaque assouplissement.

---

## 2. Règles de vérification propres à ce dépôt

### Avant de modifier

1. Lire [`LECONS.md`](LECONS.md) — la correction a peut-être déjà été tentée et rejetée.
2. Lire le test correspondant dans `common/*_test.go`.
3. Vérifier que le symbole n'est pas du code mort (constats C-01, C-02, C-03, C-06) :
   ```bash
   grep -rn "NomDuSymbole" --include='*.go' . | grep -v '_test.go'
   ```
   Deux occurrences seulement (déclaration + commentaire) = jamais appelé.

### Après avoir modifié

```bash
. /tmp/guienv.sh                 # sinon le paquet racine ne compile pas (voir ENVIRONNEMENT.md)

gofmt -l .                       # doit ne rien afficher — la CI échoue sinon
go build ./...
go vet ./...
SUPERVIEW_REQUIRE_FFMPEG=1 go test ./... -race -count=1
"$(go env GOPATH)/bin/golangci-lint" run ./... --timeout=5m
"$(go env GOPATH)/bin/govulncheck" ./...

SUPERVIEW_REQUIRE_FFMPEG=1 go test ./... -coverprofile=/tmp/cov.out -covermode=atomic
go tool cover -func=/tmp/cov.out | grep total   # doit rester ≥ 50 % (seuil CI)
```

C'est ce que la CI exécute, et rien de plus : `go vet` et `staticcheck` ne figurent plus
séparément parce que `.golangci.yml` les active tous les deux — un outil lancé à côté de la
configuration finit par contredire la configuration.

`SUPERVIEW_REQUIRE_FFMPEG=1` transforme en échec les `t.Skip` liés à ffmpeg. Sans elle, une
suite verte peut n'avoir encodé aucune image ; la CI la positionne pour cette raison.

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

Le second appel reproduit celui de `CheckVideo` : il permet de vérifier qu'un fichier passera
la validation avant de lancer la GUI. Pour éprouver le *parsing* de cette sortie sans produire
de vidéo, les cas sont enregistrés dans `common/testdata/ffprobe/` et couverts par
`parseVideoSpecs` — y compris les rejets (durée `N/A`, `bit_rate` absent, JSON tronqué).

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
| **Le gain de l'accélération matérielle dépend fortement du débit de la source** | Sur une mire à 2 Mbps : décodage +3 %, encodage ×1,9. Sur une source type GoPro à 127 Mbps : décodage **+9,9 %**, encodage **×3,18**. Mesurer le matériel sur une mire mène à la conclusion inverse (N-07, L-46). |
| Rapatrier les trames du GPU vers la RAM annule presque le gain du décodage matériel | En transcodage simple à 127 Mbps : +7 % si les trames restent en VRAM, **+1 %** si elles redescendent. `remap` étant un filtre CPU, Superview est forcément dans le second cas — et y gagne pourtant +9,9 %, vraisemblablement en libérant du CPU pour le filtre. |
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
> qu'inscrire le résultat de l'un d'eux. Voir [LECONS.md](LECONS.md) L-36.

### Contrat de résolution des outils (depuis U-04, 2026-09-06)

`resolveToolBinary` cherche `ffmpeg` et `ffprobe` dans cet ordre, et **l'ordre est le contrat** :

1. `SUPERVIEW_FFMPEG_DIR`, la sortie de secours explicite de l'utilisateur ;
2. la copie livrée dans l'archive de release ;
3. le `PATH` ;
4. sous Windows, les répertoires d'installation de winget et scoop.

> **La copie empaquetée passe avant le `PATH`, délibérément.** Le FFmpeg installé sur la machine
> est la variable que ce programme ne contrôle pas et ne peut pas inspecter à l'avance. Ne pas
> inverser cet ordre pour « prendre le plus récent » : c'est précisément le plus récent qui a
> retiré NVENC à une RTX A1000 (U-03).

Deux dispositions sur disque, parce que les deux archives ne sont pas faites pareil :

| Plateforme | Emplacement des outils | Pourquoi |
| --- | --- | --- |
| Windows | à côté de l'exécutable | Le zip se déploie dans un dossier unique. |
| Linux | `../lib/superview/` relatif à l'exécutable | L'application s'installe dans `$PREFIX/bin` : un `ffmpeg` déposé à côté d'elle atterrirait dans `/usr/local/bin` et **masquerait celui du système pour toute la machine**. |

Le chemin Linux vaut aussi bien pour l'archive simplement décompressée que pour l'archive
installée, puisqu'il est relatif au binaire.

**Ce qui est épinglé est le plancher pilote, pas la version.** `FFMPEG_DRIVER_FLOOR` vaut `570.0`
dans les deux jobs de release, et `.github/scripts/nvenc-driver-floor.sh` le relit dans le binaire
téléchargé. Monter le pin vers un build compilé contre des en-têtes plus récentes retire
l'encodage matériel à toutes les machines sous le nouveau plancher — voir [RELEASING.md](../RELEASING.md).

### Contrat des capacités matérielles (depuis U-03, 2026-09-06)

**`ffmpeg -encoders` et `ffmpeg -hwaccels` ne décrivent pas la machine.** Ils décrivent le
binaire : ce avec quoi il a été compilé. Aucun des deux ne voit le pilote. Ne jamais rétablir une
décision de capacité fondée sur eux seuls.

La seule source de vérité est la sonde de `common/probe.go` : un encodage d'une trame par
encodeur, dont le code de retour fait foi. `ApplyEncoderProbe` retire ensuite du profil ffmpeg
les encodeurs qui ont refusé — **et uniquement ceux-là** : un encodeur non sondé n'est pas un
encodeur refusé.

| Fait | Conséquence |
| --- | --- |
| Sur la machine de développement, six encodeurs annoncés ne peuvent ouvrir aucun périphérique (`h264_qsv`, `hevc_qsv`, `h264_vaapi`, `hevc_vaapi`, `h264_v4l2m2m`, `hevc_v4l2m2m`) | L'écart entre « annoncé » et « utilisable » est la règle, pas l'exception. |
| Un encodeur VAAPI n'accepte que des trames déjà sur le périphérique | Sa sonde exige `-init_hw_device vaapi=…`, `-filter_hw_device` et `-vf format=nv12,hwupload`. Sans cela elle échoue dans le graphe de filtres, avant le pilote, et condamne un encodeur qui fonctionne. |
| FFmpeg imprime la cause en **premier** et les conséquences ensuite | Un résumé qui garde la fin de la sortie garde « Error opening output files » et jette « Unknown encoder 'x' ». |
| Le plancher pilote NVENC est fixé à la compilation par les `nv-codec-headers` | Deux binaires « FFmpeg 8.1.2 » peuvent exiger 570 et 610. Le numéro de version ne dit rien de la compatibilité. Table mesurée : [hardware-support.md](hardware-support.md). |
| Le SDK 13.1 exige un pilote ≥ 610 et retire Maxwell/Pascal/Volta | La branche pilote RTX Enterprise plafonne à 597.06 : un tel build ne peut jamais utiliser NVENC sur une carte professionnelle. |
| La sonde complète coûte ~0,95 s sur 10 encodeurs (243 ms pour un NVENC qui répond, ~35 ms pour un refus) | Elle tient dans une goroutine de démarrage ; elle n'a rien à faire sur le fil UI. |

Les sondes s'exécutent **en série**. Les encodeurs matériels ont un nombre limité de sessions
simultanées, et deux sondes en parallèle sur le même GPU peuvent faire échouer un encodeur qui
fonctionne.

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
- **Encodeurs à trames sur périphérique** (VAAPI, Vulkan, D3D12, depuis U-05) : la chaîne se
  termine par `,format=nv12,hwupload` — `,format=p010,hwupload` en 10 bits, les pools matériels
  étant semi-planaires — et `-init_hw_device <type>=sv -filter_hw_device sv` est émis **avant
  les `-i`**, avec les autres options globales. Après `-i`, ces options ne configurent rien et
  l'upload échoue en cherchant un périphérique jamais créé. `remap` étant un filtre CPU, les
  trames sont en mémoire système à ce point quel que soit l'encodeur : l'upload s'ajoute après,
  donc il ne déplace aucun pixel.
- **Le montage vient de `hwDeviceArgs` et `hwUploadFilters`, que la sonde emploie aussi.** Ne pas
  dupliquer : une sonde qui ouvrirait un périphérique que la conversion n'ouvre pas passerait
  puis échouerait à l'encodage ; l'inverse condamnerait un encodeur qui fonctionne.
  `TestProbeAndConversionAskTheSameQuestion` compare les deux.

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

### La formule de distorsion — ce qu'on sait d'elle (depuis P-12)

| Fait | Conséquence |
| --- | --- |
| Le décalage **squeeze** vaut **zéro au centre par construction** : ses deux termes se réduisent tous deux à `7/32 × outX × inv` | Toute couture au milieu de l'image est un défaut d'implémentation, pas une intention. Vérifiable en arithmétique exacte, sans mesure. |
| Le décalage **non-squeeze** vaut `tx² × diff/2`, donc 0 en `tx=0` | Le miroir de la moitié gauche y est continu ; ce mode n'a jamais eu de couture. |
| Les divisions doivent être **flottantes** | `outX/16` et `outX/7` en entiers tronquent et cassent l'annulation des deux termes (P-12). Une largeur multiple de 112 masque le défaut. |
| Le code est **identique caractère pour caractère** à `Niek/superview` | Fidèle ne veut pas dire correct : la couture y était aussi. Vérifier la conformité à la référence ne dispense pas de vérifier la référence (L-48). |
| L'algorithme **n'est pas** celui de GoPro | Le README amont le dit explicitement, et documente l'option squeeze pour des caméras comme la Caddx Tarsier. Ne pas promettre une compatibilité GoPro dans l'interface (P-13). |
| L'implémentation Python de Banelle est **inaccessible** | 403 sur intofpv.com, archive web refusée. Ne pas la citer comme vérifiable. |

> **Le test doré ne suffit pas.** `TestGeneratePGM_Golden` fige des octets : il certifie la
> non-régression, jamais la correction — il a figé la couture de P-12 pendant trois passes.
> Toute modification de la formule doit passer *aussi* par
> `TestGeneratePGM_SqueezeMapIsContinuous` et `TestGeneratePGM_SqueezeMapStaysWellFormed`, qui
> testent des propriétés : continuité, monotonie, bornes, centre préservé.

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

## 3. Références externes

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

# Registre des décisions

> **Pourquoi ce fichier existe.** Les décisions de ce dépôt étaient consignées en
> trois endroits : le tableau *Decisions that look like defects* d'[`AGENTS.md`](../AGENTS.md),
> les contrats de [`CONTRATS.md`](CONTRATS.md), et les arbitrages dispersés dans les
> 2 700 lignes d'[`ANALYSE.md`](ANALYSE.md). Un agent qui ouvre un fichier `.go` ne
> les retrouve que s'il sait déjà où chercher. Ce registre les met à plat, une par
> entrée, avec ce qui la rendrait caduque.
>
> **Ce qu'il n'est pas.** Ni un journal (c'est [`LECONS.md`](LECONS.md)), ni une
> liste de constats (c'est [`ANALYSE.md`](ANALYSE.md)). Une décision ici n'est pas
> un fait mesuré : c'est un choix, pris pour des raisons datées, et qu'on peut
> rouvrir — à condition de rouvrir **aussi** ses conséquences.
>
> **Comment l'utiliser.** Avant de « corriger » quelque chose qui ressemble à un
> défaut, chercher ici. Avant de prendre une décision nouvelle, ajouter une
> entrée. Quand une décision est renversée, ne pas la supprimer : passer son statut
> à *Remplacée par DEC-xx* et dire pourquoi.

## Gabarit

```
### DEC-xx — Titre, en une phrase affirmative
Date · Statut (Acceptée | Remplacée par DEC-yy | À réexaminer) · Source (constat, PR, leçon)
**Contexte** — ce qui a forcé le choix.
**Décision** — ce qui a été décidé.
**Conséquences** — ce qu'on paie, et ce qu'on ne doit plus faire.
**À rouvrir si** — la condition qui rendrait le choix faux.
```

## Index

| ID | Décision | Statut |
| --- | --- | --- |
| [DEC-01](#dec-01) | MP4 en entrée, MP4 en sortie, rien d'autre | Acceptée |
| [DEC-02](#dec-02) | Application graphique seulement, Windows et Linux, Go + Fyne | Acceptée |
| [DEC-03](#dec-03) | La configuration se passe en paramètre, jamais par un global | Acceptée |
| [DEC-04](#dec-04) | L'archive embarque un FFmpeg épinglé sur son plancher pilote NVENC, préféré à celui du système | Acceptée |
| [DEC-05](#dec-05) | Les capacités matérielles se mesurent par une sonde d'exécution, en série, puis à la taille réelle | Acceptée |
| [DEC-06](#dec-06) | Un encodeur matériel de l'autre famille de codec vaut mieux que le CPU | Acceptée |
| [DEC-07](#dec-07) | Le repli CPU retombe dans la famille de la **source** | Acceptée |
| [DEC-08](#dec-08) | Le 10 bits n'est conservé que pour les encodeurs HEVC | Acceptée |
| [DEC-09](#dec-09) | Le débit est plafonné à `-maxrate` = 1× la consigne, `-bufsize` = 2× | Acceptée |
| [DEC-10](#dec-10) | Les deux profils de qualité demandent 4/3 du débit source et ne diffèrent que par le preset | Acceptée |
| [DEC-11](#dec-11) | Le décodage matériel est conservé, les trames redescendent en RAM | Acceptée |
| [DEC-12](#dec-12) | Les cartes de remappage sont du PGM P5 binaire 16 bits gros-boutiste, écrites par un seul encodeur d'échantillon | Acceptée |
| [DEC-13](#dec-13) | La formule squeeze divise en flottant et garde la forme de la référence amont | Acceptée |
| [DEC-14](#dec-14) | L'interface ne promet pas la compatibilité GoPro | Acceptée |
| [DEC-15](#dec-15) | L'encodage écrit dans un fichier de travail renommé en place ; l'entrée ne peut pas être la sortie | Acceptée |
| [DEC-16](#dec-16) | L'annulation et l'arrêt système sont deux erreurs typées distinctes | Acceptée |
| [DEC-17](#dec-17) | Le garde-fou mémoire est chiffré au banc et ne chiffre pas les encodeurs matériels | Acceptée |
| [DEC-18](#dec-18) | Le journal va dans un fichier plafonné sous le cache utilisateur ; la progression y est en `debug` | Acceptée |
| [DEC-19](#dec-19) | Les liens symboliques sont résolus puis validés, pas rejetés | Acceptée |
| [DEC-20](#dec-20) | Une option sans effet se retire ; une fonctionnalité inatteignable s'expose | Acceptée |
| [DEC-21](#dec-21) | staticcheck est cadré sur `SA*` + `S1*` ; aucun second linter | Acceptée |
| [DEC-22](#dec-22) | La version vient du tag, jamais d'un fichier commité | Acceptée |
| [DEC-23](#dec-23) | `main()` reste long ; l'état est dans `appState` | Acceptée |
| [DEC-24](#dec-24) | Toute modification passe par une PR fusionnée sur label `merge` ; le workflow choisit la méthode | Acceptée |
| [DEC-25](#dec-25) | La release est un bouton ; `"*"` reste dans `release.yml` ; le `Makefile` de fyne est rustiné | Acceptée |
| [DEC-26](#dec-26) | Journaux internes en français, tout ce qu'un tiers lit en anglais ; README en deux langues à parité | Acceptée |
| [DEC-27](#dec-27) | La suite ne vaut qu'avec `SUPERVIEW_REQUIRE_FFMPEG=1`, `-race`, et un seuil de 50 % module entier | Acceptée |
| [DEC-28](#dec-28) | Tout test est prouvé par contre-épreuve | Acceptée |
| [DEC-29](#dec-29) | Les fichiers temporaires vivent dans un répertoire de session isolé sous `os.TempDir()` | Acceptée |
| [DEC-30](#dec-30) | `-threads` = `NumCPU()` plafonné à 16 pour `libx265`, jamais émis pour un encodeur matériel | Acceptée |
| [DEC-31](#dec-31) | Pas de `-re` : l'encodage n'est pas bridé au temps réel | Acceptée |
| [DEC-32](#dec-32) | Fenêtre à taille fixe, thème sombre forcé | Acceptée |
| [DEC-33](#dec-33) | Le mode audio par défaut est `safe` dans le code et `safe_performance` dans le YAML livré | À réexaminer |
| [DEC-34](#dec-34) | Les dialogues de fichiers sont natifs d'abord, Fyne en repli | Acceptée |
| [DEC-35](#dec-35) | La CI de `master` est relancée par le workflow de fusion via `workflow_dispatch` ; le ruleset reste non strict | Acceptée |
| [DEC-36](#dec-36) | `common/` est découpé par responsabilité ; aucun fichier fourre-tout | Acceptée |

---

### DEC-01
**MP4 en entrée, MP4 en sortie, rien d'autre.**
2026-09-04 · Acceptée · N-01

**Contexte** — Les sélecteurs offraient dix conteneurs ; cinq d'entre eux (MKV, WebM, FLV, MPEG-PS, ASF) ne portent ni durée ni `bit_rate` par flux, et `CheckVideo` les rejetait avec une erreur `strconv` illisible.
**Décision** — Les sélecteurs n'offrent que `.mp4` ; l'extension de sortie est forcée par `ensureMP4Extension`.
**Conséquences** — Une demande « MKV » est une demande produit, pas une correction. `AGENTS.md` le dit en tête.
**À rouvrir si** — `CheckVideo` apprend à lire la durée au niveau du format (`-show_entries format=...`) et à tolérer un `bit_rate` absent.

### DEC-02
**Application graphique seulement, Windows et Linux, Go + Fyne.**
origine du dépôt · Acceptée

**Contexte** — Le destinataire est un utilisateur de caméra d'action qui double-clique ; la CLI a été retirée.
**Décision** — Un seul binaire, Fyne, pas de mode ligne de commande, pas de macOS (aucun runner, aucun signataire).
**Conséquences** — Toute vérification de la GUI exige les en-têtes GL/X11/Wayland ([ENVIRONNEMENT.md](ENVIRONNEMENT.md)). La CLI n'est pas « à remettre ».
**À rouvrir si** — Un besoin de traitement par lots apparaît ; il se traiterait par un bouton « dossier », pas par une CLI.

### DEC-03
**La configuration se passe en paramètre, jamais par un global.**
2026-09-04 · Acceptée · C-05, L-22

**Contexte** — `SetConfig()` mutait un global que la GUI écrasait avant chaque run ; le `video_preset` de l'utilisateur était perdu, et la valeur était lue depuis la goroutine d'encodage pendant que le fil UI l'écrivait.
**Décision** — `CheckFfmpeg(cfg)`, `InitEncodingSession(cfg)`, `EncodeVideo(cfg, …)`, `PerformEncoding(cfg, …)`, `CheckHealth(cfg)` ; `nil` vaut défauts.
**Conséquences** — Un nouveau réglage s'ajoute à `Config` et se transporte par signature. `configOrDefault(nil)` rend l'instance partagée : la traiter en lecture seule.
**À rouvrir si** — Jamais pour « éviter de passer un paramètre ».

### DEC-04
**L'archive embarque un FFmpeg épinglé sur son plancher pilote NVENC, préféré à celui du système.**
2026-09-06 · Acceptée · U-03, U-04, L-72, L-74, [RELEASING.md](../RELEASING.md)

**Contexte** — gyan.dev 8.1.2 est compilé contre des `nv-codec-headers` exigeant un pilote 610 ; la branche pilote RTX Enterprise plafonne à 597.06. Un utilisateur a perdu NVENC en mettant FFmpeg à jour.
**Décision** — Les archives livrent un FFmpeg au plancher 570.0 : gyan.dev 8.1.1 sous Windows, BtbN n8.1.2 (autobuild 2026-08-31) sous Linux — la propriété épinglée est le plancher, pas le numéro de version, et `release.yml` lance la suite contre le binaire livré avant de l'empaqueter (E-11). `resolveToolBinary` cherche dans l'ordre : `SUPERVIEW_FFMPEG_DIR`, la copie livrée, le `PATH`, puis winget/scoop. `.github/scripts/nvenc-driver-floor.sh` fait échouer la release si le plancher bouge.
**Conséquences** — « Mettre à jour vers le dernier FFmpeg » est une régression déguisée. Sous Linux les outils vont dans `../lib/superview/`, jamais à côté du binaire dans `$PREFIX/bin`.
**À rouvrir si** — Un build à plancher ≤ 570 existe pour une version plus récente, **et** le script de plancher le confirme.

### DEC-05
**Les capacités matérielles se mesurent par une sonde d'exécution, en série, puis à la taille réelle.**
2026-09-06 / 2026-09-09 · Acceptée · U-03, U-13, L-71, L-73, L-93

**Contexte** — `ffmpeg -encoders` décrit le binaire, pas le pilote ; six encodeurs annoncés sur la machine de développement ne peuvent ouvrir aucun périphérique. Une sonde 256×256 ne prouve pas qu'une trame 5120×2880 passera (Intel HD 620).
**Décision** — Au démarrage, une trame par encodeur (`ProbeEncoders`), en série parce que les sessions GPU sont comptées ; avant chaque conversion, `verifyEncoderForOutput` resonde à la géométrie de sortie, résultat mis en cache pour la session. `ApplyEncoderProbe` ne retire que les encodeurs **refusés**.
**Conséquences** — La sonde pose **la même question** que l'encodage : `hwDeviceArgs`, `pixelFormatChain` et `encoderFacingArgs` sont partagés, et `TestProbeAndConversionAskTheSameQuestion` compare l'argv entier en ne tolérant que les différences qu'il liste (E-01, corrigé). Une option d'encodeur s'ajoute dans `encoderFacingArgs`, jamais dans un seul des deux constructeurs.
**À rouvrir si** — Jamais vers « lire la liste ». L'option (b) de E-01 — s'adapter à un pilote CQP-only en `-rc_mode CQP -qp N` — reste possible comme réglage « qualité constante » explicite.

### DEC-06
**Un encodeur matériel de l'autre famille de codec vaut mieux que le CPU.**
2026-09-08 · Acceptée · U-10, L-87

**Contexte** — Un Intel HD 620 refusait tout `hevc_*` et acceptait `h264_vaapi` ; la source étant HEVC, la liste H.264 n'était jamais consultée : un quart d'heure de `libx265` et 6 Go sur une machine de 8.
**Décision** — Quand la famille source n'a pas d'encodeur matériel utilisable, `FindEncoder` prend celui de l'autre famille et **le dit** (`describeCodecFamilySwitch`, dans la ligne *Hardware* et dans le journal).
**Conséquences** — Un utilisateur peut recevoir du H.264 pour une source HEVC, et un 10 bits stocké en 8 bits ; les deux sont annoncés avant de lancer.
**À rouvrir si** — Un retour utilisateur préfère la famille à la vitesse ; ajouter alors une case, pas renverser le défaut.

### DEC-07
**Le repli CPU retombe dans la famille de la source.**
2026-09-08 · Acceptée · U-10

**Contexte** — Suivre la famille de l'encodeur **échoué** transformait « H.264 sur le GPU » en « H.264 sur le CPU », codec que personne n'avait demandé une fois le GPU hors jeu.
**Décision** — `softwareEncoderForFallback` essaie `libx264`/`libx265` de la famille source, puis de la famille de l'encodeur échoué.
**Conséquences** — Le garde-fou mémoire se recalcule pour l'encodeur qui prend le relais (U-14).

### DEC-08
**Le 10 bits n'est conservé que pour les encodeurs HEVC.**
2026-09-04 · Acceptée · N-03

**Contexte** — La chaîne figée en 8 bits aplatissait toute source 10 bits (HERO 10+). `h264_nvenc` ne sait pas encoder en 10 bits ; le High 10 de `libx264` se lit mal.
**Décision** — `remapFilterChain` passe en `yuv444p10le,yuv420p10le` uniquement si la source dépasse 8 bits **et** que l'encodeur est HEVC. Au-delà de 10 bits, on ramène à 10.
**À rouvrir si** — La lecture du High 10 H.264 devient courante sur les cibles des utilisateurs.

### DEC-09
**Le débit est plafonné à `-maxrate` = 1× la consigne, `-bufsize` = 2×.**
2026-09-04 · Acceptée · N-06, L-42

**Contexte** — `-b:v` seul est une moyenne sans plafond : +83 % mesuré sur bruit incompressible. Le 1,5× « habituel » faisait moins bien (+29 %) que 1× (+24 %).
**Décision** — Les deux options sont émises pour tout encodeur.
**Conséquences** — Un pilote VAAPI qui ne connaît que CQP les refuse : voir E-01.

### DEC-10
**Les deux profils de qualité demandent 4/3 du débit source et ne diffèrent que par le preset.**
2026-09-04 · Acceptée · Q-01 (mesure XPSNR/SSIM, deux encodeurs), arbitrage utilisateur

**Contexte** — « Balanced » valait 1,6 = 4/3 + 20 % non documentés ; « Fast » ne relevait pas le débit et dégradait de 0,94 dB sans le dire.
**Décision** — `widenedPixelRatio = 4/3` pour les deux ; « Fast » = preset `fast`, « Balanced » = `medium`.
**À rouvrir si** — Une mesure sur du H.264 ou un autre type de contenu contredit le 1,19–1,30 trouvé.

### DEC-11
**Le décodage matériel est conservé, les trames redescendent en RAM.**
2026-09-04 · Acceptée · N-07 révisé, L-46

**Contexte** — Mesuré sur une mire à 2 Mb/s, le gain était de 3 % ; refait sur une source type GoPro à 127 Mb/s, +9,9 % en décodage et ×3,18 en encodage.
**Décision** — `-hwaccel` sans `-hwaccel_output_format` : `remap` est un filtre CPU, les trames doivent être en mémoire système.

### DEC-12
**Les cartes de remappage sont du PGM P5 binaire 16 bits gros-boutiste, écrites par un seul encodeur d'échantillon.**
2026-09-04 · Acceptée · P-08, L-37

**Contexte** — Le P2 ASCII pesait 146 Mo pour une trame 12 Mpx, en RAM sur un `/tmp` tmpfs. Le P5 est identique au bit près en sortie décodée.
**Décision** — `putMapSample` est le seul endroit qui encode un échantillon, pour les deux cartes. Trois tests non interchangeables le verrouillent (doré, format, stabilité remap).

### DEC-13
**La formule squeeze divise en flottant et garde la forme de la référence amont.**
2026-09-04 · Acceptée · P-12, L-48

**Contexte** — `outX/16` et `outX/7` en entiers laissaient une couture de 0,9 à 2,6 px au centre ; le code amont a la même couture.
**Décision** — Divisions flottantes, expression gardée reconnaissable, tests de propriété (continuité, monotonie, bornes) en plus du test doré. `QF*` de staticcheck reste désactivé pour ne pas réécrire ces `math.Pow`.

### DEC-14
**L'interface ne promet pas la compatibilité GoPro.**
2026-09-04 · Acceptée · P-13

**Décision** — La case s'appelle *Source already stretched to 16:9 (un-squeeze)* ; l'amont dit lui-même que l'algorithme n'est pas celui de GoPro.

### DEC-15
**L'encodage écrit dans un fichier de travail renommé en place ; l'entrée ne peut pas être la sortie.**
2026-09-04 · Acceptée · P-04, N-09

**Contexte** — `-y` écrivait directement à destination : une annulation laissait un `.mp4` tronqué et détruisait le précédent.
**Décision** — `newPartialOutputPath` dans le répertoire de destination (même système de fichiers, donc renommage atomique) ; `sameOutputAsInput` parce que FFmpeg ne voit plus le conflit.

### DEC-16
**L'annulation et l'arrêt système sont deux erreurs typées distinctes.**
2026-09-04 / 2026-09-08 · Acceptée · P-03, U-09, L-31, L-84

**Contexte** — Une chaîne non typée relançait la cascade après un Cancel ; un SIGTERM de systemd après un OOM était journalisé « interrupted by user ».
**Décision** — `ErrCancelled` et `ErrStoppedBySignal`, testés avec `errors.Is`.
**Conséquences** — **Tout court-circuit écrit pour l'un doit l'être pour l'autre.** La 13ᵉ passe a trouvé que la cascade ne teste que `ErrCancelled` (E-02) : la décision était bonne, son application incomplète.

### DEC-17
**Le garde-fou mémoire est chiffré au banc et ne chiffre pas les encodeurs matériels.**
2026-09-08 · Acceptée · U-11, U-14, L-88

**Décision** — 290 o/pixel (8 bits) et 455 o/pixel (10 bits) de sortie, mesurés avec `libx265` à `-threads 4`. Les encodeurs matériels rendent 0 : « refuser sur un chiffre que personne n'a mesuré serait pire que ne pas vérifier ».
**À rouvrir si** — La mesure est refaite à `-threads 16` (voir Q-02) ou pour un encodeur matériel.

### DEC-18
**Le journal va dans un fichier plafonné sous le cache utilisateur ; la progression y est en `debug`.**
2026-09-04 / 2026-09-08 · Acceptée · O-02, U-12, L-90

**Décision** — `~/.cache/superview/superview.log`, tronqué au-delà de 5 Mio à l'ouverture ; les événements de progression tirent plusieurs fois par seconde et restent en `debug`. Le chemin est affiché dans le dialogue *Diagnostic*.

### DEC-19
**Les liens symboliques sont résolus puis validés, pas rejetés.**
2026-09-04 · Acceptée · S-02, arbitrage utilisateur

**Contexte** — Le rejet bloquait `~/Videos → /media/...`. L'utilisateur choisit lui-même le fichier dans un dialogue : il n'y a pas de tiers à repousser.
**Décision** — `EvalSymlinks`, puis la **cible** doit être un fichier régulier absolu.

### DEC-20
**Une option sans effet se retire ; une fonctionnalité inatteignable s'expose.**
2026-09-04 · Acceptée · C-01, C-03, C-04, arbitrage utilisateur

**Décision** — Trois clés YAML sans effet retirées ; `health.go` branché sur le bouton *Diagnostic* ; le mode squeeze exposé par une case.
**Conséquences** — Ne pas réintroduire une option « pour plus tard » (L-22).

### DEC-21
**staticcheck est cadré sur `SA*` + `S1*` ; aucun second linter.**
2026-09-04 · Acceptée · X-04, L-09, L-12, L-62

**Décision** — `.golangci.yml` est le seul endroit qui décide. Un job `staticcheck` séparé a appliqué ce que la config désactive ; il a été retiré.

### DEC-22
**La version vient du tag, jamais d'un fichier commité.**
2026-09-05 · Acceptée · R-05, L-65

**Décision** — `FyneApp.toml` n'a pas de `Version` ; `fyne package --app-version` la prend du tag. Un `go build` dit `dev`, avec la révision VCS si elle existe.

### DEC-23
**`main()` reste long ; l'état est dans `appState`.**
2026-09-04 · Acceptée · P-10, L-45

**Décision** — Le défaut était l'état inatteignable, pas la longueur. Les transitions sont des méthodes testées à 100 % ; la construction des widgets ne gagne rien à être découpée.

### DEC-24
**Toute modification passe par une PR fusionnée sur label `merge` ; le workflow choisit la méthode.**
2026-09-11 / 2026-10-14 · Acceptée · M-03, M-06, L-97, L-100, [MERGING.md](MERGING.md)

**Décision** — Ruleset sur `master`, squash par défaut, merge commit si une PR est empilée dessus, suppression de branche par le workflow (le réglage du dépôt ne vaut pas pour le `GITHUB_TOKEN`), et relance de la CI de `master` par le même workflow (DEC-35, pour la même raison). Labelliser par `gh api`, pas `gh pr edit`.

### DEC-25
**La release est un bouton ; `"*"` reste dans `release.yml` ; le `Makefile` de fyne est rustiné.**
2026-09-05 / 2026-09-06 · Acceptée · R-xx, U-04, L-67, L-76

**Décision** — `workflow_dispatch` seul, jamais de tag manuel. La catégorie `"*"` est porteuse : GitHub jette toute PR sans catégorie. Le `Makefile` généré par fyne n'installe pas les outils embarqués et sa ligne d'icône est cassée ; le patch est clé sur les lignes exactes et `make install` est exécuté dans un répertoire jetable pour le prouver.

### DEC-26
**Journaux internes en français, tout ce qu'un tiers lit en anglais ; README en deux langues à parité.**
2026-09-07 · Acceptée · U-08, L-47, L-83

**Décision** — `docs/LECONS.md`, `ANALYSE.md`, `CONTRATS.md`, `ENVIRONNEMENT.md`, ce registre : français. `AGENTS.md`, `RELEASING.md`, `SECURITY.md`, `hardware-support.md`, la GUI, les erreurs, les titres de PR : anglais. `README.md` et `README_FR.md` bougent ensemble (job `readme-parity`), les libellés GUI restent en anglais dans la version française.

### DEC-27
**La suite ne vaut qu'avec `SUPERVIEW_REQUIRE_FFMPEG=1`, `-race`, et un seuil de 50 % module entier.**
2026-09-04 / 2026-09-11 · Acceptée · T-02, M-01, L-50, L-60, L-96

**Décision** — Sans la variable, quinze tests sautent en silence, dont tout ce qui encode. `make test` et la CI la posent. Le seuil mesure `./...`, pas `./common`.

### DEC-28
**Tout test est prouvé par contre-épreuve.**
2026-09-04 · Acceptée · L-37, L-49, L-54, L-78, L-86

**Décision** — Réintroduire le défaut, voir le test rougir, et vérifier que la contre-épreuve elle-même n'est pas muette (environnement, code partagé, compilation).

### DEC-29
**Les fichiers temporaires vivent dans un répertoire de session isolé sous `os.TempDir()`.**
2026-09-04 · Acceptée · N-08, S-03

**Décision** — `InitEncodingSession` → `os.MkdirTemp` ; `checkTempSpaceForMaps` vérifie l'espace avant d'écrire. Jamais le répertoire courant, jamais un chemin en dur. Une seule session à la fois.

### DEC-30
**`-threads` = `NumCPU()` plafonné à 16 pour `libx265`, jamais émis pour un encodeur matériel.**
2026-09-04 · Acceptée · P-11, L-39

**Décision** — ffmpeg mappe `-threads` sur `--frame-threads` de x265, qui refuse au-delà de 16 (échec dur). L'option est émise après `-c:v`, jamais avant `-i`.

### DEC-31
**Pas de `-re` : l'encodage n'est pas bridé au temps réel.**
2026-09-04 · Acceptée · O-01, L-14

**Décision** — `-re` n'a de sens que vers un puits temps réel ; `performance_mode` ne gouverne plus que l'audio.

### DEC-32
**Fenêtre à taille fixe, thème sombre forcé.**
2026-09-06 · Acceptée · U-01, U-02, L-20, L-70

**Décision** — `SetFixedSize(true)`, taille = max(constantes, `MinSize` du contenu) ; `forcedDarkTheme` plutôt que `theme.DarkTheme()` déprécié. Ajouter un widget est un changement de mise en page : `TestToolbarFitsWindow` mesure le rendu.

### DEC-33
**Le mode audio par défaut est `safe` dans le code et `safe_performance` dans le YAML livré.**
état constaté · **À réexaminer** · CONTRATS.md § 1 ligne 6

**Contexte** — `defaultConfig.PerformanceMode = "safe"` ; `superview.yaml` livre `safe_performance`. Un utilisateur sans fichier de configuration (cas des archives : le YAML n'est pas installé dans `../lib/superview/`) réencode donc l'audio en AAC, l'autre le copie.
**Décision (implicite, jamais tranchée)** — Les deux valeurs coexistent.
**À rouvrir** — Choisir une valeur et l'écrire aux deux endroits ; c'est l'item 9 du plan de la 13ᵉ passe.

### DEC-34
**Les dialogues de fichiers sont natifs d'abord, Fyne en repli.**
origine · Acceptée · B-03 (invalidé)

**Décision** — zenity puis kdialog sous Linux, PowerShell/WinForms sous Windows ; le dialogue Fyne si rien n'est disponible. ~~Les codes de sortie 1 et 255 valent « annulé », pas « erreur ».~~
**Amendée 2026-10-14 (E-20)** — Seul le code 1 vaut « annulé ». 255 est le −1 de zenity, « unexpected error » (zenity(1), *EXIT CODES*) : le prendre pour une annulation empêchait le repli Fyne, qui existe pour ce cas, de s'ouvrir. Le sélecteur tourne dans une goroutine et livre son résultat par `fyne.Do`.
**À rouvrir si** — Fyne livre un dialogue natif portable (suivi : `fyne.io/fyne/v2/dialog`).

### DEC-35
**La CI de `master` est relancée par le workflow de fusion via `workflow_dispatch` ; le ruleset reste non strict.**
2026-10-14 · Acceptée · E-06, L-104, L-107, DEC-24, [MERGING.md](MERGING.md)

**Contexte** — Depuis que `merge-on-label.yml` fusionne (M-03), la fusion est faite avec le `GITHUB_TOKEN`, et « events triggered by the `GITHUB_TOKEN`, with the exception of `workflow_dispatch` and `repository_dispatch`, will not create a new workflow run » (docs.github.com, *Triggering a workflow*). Cinq fusions (#73 à #77) sans un seul run sur `master`. Trois issues : rendre le ruleset strict (*Require branches to be up to date*), fusionner avec un jeton d'application ou personnel, ou utiliser l'exception.
**Décision** — L'exception. `test.yml` et `lint.yml` acceptent `workflow_dispatch` ; après la fusion, le job de `merge-on-label.yml` (qui a `actions: write` pour cela) les dispatche sur la branche par défaut, en meilleur effort — un échec de dispatch est un avertissement, pas un rouge sur une PR déjà fusionnée. Le ruleset garde `strict_required_status_checks_policy: false` : strict refuserait toute PR qui n'est pas au sommet de `master` — chaque membre d'une pile, chaque bump Dependabot derrière un autre — tant qu'un humain ne l'a pas rebasée, c'est-à-dire exactement le geste manuel que le label supprime. Un jeton d'application déclencherait tout nativement, au prix d'un secret à longue durée de vie dans un job qui fusionne du code.
**Conséquences** — Une PR est testée contre sa base au moment de son dernier push, pas contre le résultat fusionné ; une régression née de la combinaison de deux PR n'apparaît qu'après coup, sur le run dispatché de `master` — accepté, et c'est pour cela que ce run existe. Après une fusion, `gh run list --branch master --event workflow_dispatch --limit 2` doit montrer deux runs. **La première fusion après la PR qui introduit ce mécanisme ne dispatche rien** : `pull_request_target` exécute la version du workflow qui est sur `master`, donc l'ancienne ; relancer à la main cette fois-là (`gh workflow run test.yml --ref master`, idem `lint.yml`).
**À rouvrir si** — GitHub cesse d'exempter `workflow_dispatch` ; ou le dépôt accepte un jeton d'application (la suppression de branche et le dispatch redeviendraient alors natifs, et DEC-24 à réviser) ; ou les piles de PR disparaissent de la pratique, auquel cas le ruleset strict ne coûte plus rien et teste mieux.

### DEC-36
**`common/` est découpé par responsabilité ; aucun fichier fourre-tout.**
2026-10-14 · Acceptée · 13ᵉ passe, à la demande de l'utilisateur (« optimise la structure »)

**Contexte** — `common.go` avait 2 023 lignes et portait huit responsabilités : erreurs, outils,
session, lecture ffprobe, cartes PGM, garde-fou mémoire, un passage ffmpeg, orchestration.
`AGENTS.md` le notait « connu, pas urgent ». Deux commentaires de doc y avaient déjà dérivé
loin de leur déclaration (`CheckVideo`, `remapFilterChain`) — le symptôme d'un fichier où on
insère sans lire.
**Décision** — Huit fichiers, un en-tête d'une ligne chacun qui nomme sa responsabilité :
`errors.go`, `tools.go`, `session.go`, `video.go`, `pgm.go`, `memory.go`, `encode.go`,
`pipeline.go`. Déplacements purs : aucune déclaration modifiée, la suite entière et le test
doré passent inchangés sous `-race`. `hardware.go`, `probe.go`, `config.go`, `security.go`,
`health.go`, `metrics.go`, `observability.go` ne bougent pas.
**Conséquences** — Un nouveau symbole va dans le fichier dont l'en-tête décrit son travail ;
s'il n'en existe aucun, on crée le fichier, on n'allonge pas le voisin. Les références de
prose en `common.go:NNN` sont remplacées par le nom du fichier et du symbole, qui vieillissent
moins vite (L-98). `main()` de `gui_main.go` n'est **pas** concerné (DEC-23).
**À rouvrir si** — Jamais vers un retour à un fichier unique. Un fichier qui dépasse ~700
lignes (`encode.go` en est à 656) se redécoupe au prochain ajout.

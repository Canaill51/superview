# Registre des décisions

> **Pourquoi ce fichier existe.** Les décisions de ce dépôt étaient consignées en
> trois endroits : le tableau *Decisions that look like defects* d'[`AGENTS.md`](../AGENTS.md),
> les contrats de [`CONTRATS.md`](CONTRATS.md), et les arbitrages dispersés dans les
> 2 700 lignes d'[`ANALYSE.md`](ANALYSE.md). Un agent qui ouvre un fichier `.go` ne
> les retrouve que s'il sait déjà où chercher. Ce registre les met à plat, une par
> entrée, avec ce qui la rendrait caduque.
>
> **Go et Fyne.** Les noms de ce journal (`common/`, `internal/common/`, `cmd/superview/`, `gui_*.go`, `appState`, `go test`, `-race`…) décrivent le code Go retiré (DEC-49) : ils ne sont pas réécrits (DEC-55). Pour relire un fichier cité : `git show v0.2.9:<chemin>`, avec `common/` = `internal/common/` et `gui_*.go` = `cmd/superview/` (`common.go` y est découpé en huit fichiers, DEC-36) ; les numéros de ligne cités sont ceux du commit analysé, pas de v0.2.9.
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
| [DEC-02](#dec-02) | Application graphique seulement, Windows et Linux, Go + Fyne | Acceptée (partie « Go + Fyne » : *Remplacée par DEC-49*) |
| [DEC-03](#dec-03) | La configuration se passe en paramètre, jamais par un global | Acceptée |
| [DEC-04](#dec-04) | L'archive embarque un FFmpeg épinglé sur son plancher pilote NVENC, préféré à celui du système | Acceptée, amendée par DEC-41 (installateur et AppImage au lieu d'archives) |
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
| [DEC-17](#dec-17) | Le garde-fou mémoire est chiffré au banc et ne chiffre pas les encodeurs matériels | Remplacée par DEC-37 |
| [DEC-18](#dec-18) | Le journal va dans un fichier plafonné sous le cache utilisateur ; la progression y est en `debug` | Acceptée, amendée par DEC-49 |
| [DEC-19](#dec-19) | Les liens symboliques sont résolus puis validés, pas rejetés | Acceptée |
| [DEC-20](#dec-20) | Une option sans effet se retire ; une fonctionnalité inatteignable s'expose | Acceptée |
| [DEC-21](#dec-21) | staticcheck est cadré sur `SA*` + `S1*` ; aucun second linter | Remplacée par DEC-49 |
| [DEC-22](#dec-22) | La version vient du tag, jamais d'un fichier commité | Acceptée, amendée par DEC-47 et DEC-49 |
| [DEC-23](#dec-23) | `main()` reste long ; l'état est dans `appState` | Remplacée par DEC-49 |
| [DEC-24](#dec-24) | Toute modification passe par une PR fusionnée sur label `merge` ; le workflow choisit la méthode | Acceptée |
| [DEC-25](#dec-25) | La release est un bouton ; `"*"` reste dans `release.yml` ; le `Makefile` de fyne est rustiné | Acceptée (partie `Makefile` : *Remplacée* par DEC-41) |
| [DEC-26](#dec-26) | Journaux internes en français, tout ce qu'un tiers lit en anglais ; README en deux langues à parité | Acceptée |
| [DEC-27](#dec-27) | La suite ne vaut qu'avec `SUPERVIEW_REQUIRE_FFMPEG=1`, `-race`, et un seuil de 50 % module entier | Acceptée, amendée par DEC-47 et DEC-49 |
| [DEC-28](#dec-28) | Tout test est prouvé par contre-épreuve | Acceptée |
| [DEC-29](#dec-29) | Les fichiers temporaires vivent dans un répertoire de session isolé sous `os.TempDir()` | Acceptée |
| [DEC-30](#dec-30) | `-threads` = `NumCPU()` plafonné à 16 pour `libx265`, jamais émis pour un encodeur matériel | Acceptée |
| [DEC-31](#dec-31) | Pas de `-re` : l'encodage n'est pas bridé au temps réel | Acceptée |
| [DEC-32](#dec-32) | Fenêtre à taille fixe, thème sombre forcé | Acceptée ; la taille fixe est remplacée par DEC-51, le thème sombre forcé par le menu Theme de DEC-57 |
| [DEC-33](#dec-33) | Le mode audio par défaut est `safe_performance` partout : code, YAML livré, README | Acceptée |
| [DEC-34](#dec-34) | Les dialogues de fichiers sont natifs d'abord, Fyne en repli | Acceptée, amendée par DEC-43 (6.4) et DEC-49 |
| [DEC-35](#dec-35) | La CI de `master` est relancée par le workflow de fusion via `workflow_dispatch` ; le ruleset reste non strict | Acceptée |
| [DEC-36](#dec-36) | `internal/common/` est découpé par responsabilité ; aucun fichier fourre-tout | Remplacée par DEC-49 (le principe vaut pour les modules Rust) |
| [DEC-37](#dec-37) | Le garde-fou mémoire chiffre au nombre de fils réellement passé à ffmpeg : +4 % par fil au-delà de 4 | Acceptée |
| [DEC-38](#dec-38) | La télémétrie GoPro (piste `gpmd`) n'est pas conservée | Acceptée |
| [DEC-39](#dec-39) | Le code suit l'arborescence du gabarit : `cmd/superview/` et `internal/common/` | Remplacée par DEC-49 |
| [DEC-40](#dec-40) | Les docs suivent le gabarit par passerelles ; les journaux sont allégés, pas convertis | Acceptée |
| [DEC-41](#dec-41) | La release livre un installateur Windows et une AppImage Linux, pas des archives | Acceptée |
| [DEC-42](#dec-42) | Un agent peut déclencher la release, sur demande explicite, par `/publier` | Acceptée |
| [DEC-43](#dec-43) | Portage vers Rust + Slint : Go reste la référence, `rust/` cohabite jusqu'à la parité | Acceptée, GO le 2026-10-15 ; port terminé le 2026-10-20 (DEC-49) |
| [DEC-44](#dec-44) | `superview-harness` est un banc d'essai jamais livré, pas une CLI | Acceptée, amendée par DEC-49 |
| [DEC-45](#dec-45) | Sous Windows, la recherche d'ffmpeg/ffprobe sur `PATH` refuse `.bat` et `.cmd` (Rust) | Acceptée |
| [DEC-46](#dec-46) | Interface Rust : préférences neuves, aucune reprise des choix Fyne | Acceptée |
| [DEC-47](#dec-47) | La release livre le binaire Rust ; AppImage sans `LD_LIBRARY_PATH` (DT_RPATH) | Acceptée |
| [DEC-48](#dec-48) | Un pilote VAAPI qui n'accepte que CQP est utilisé en qualité constante, automatiquement (Rust seulement) | Acceptée |
| [DEC-49](#dec-49) | Go est retiré du dépôt ; le Rust est la seule base de code | Acceptée, amendée par DEC-55 |
| [DEC-50](#dec-50) | Le profil release est `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"` ; pas de `panic = "abort"` | Acceptée |
| [DEC-51](#dec-51) | Fenêtre redimensionnable : 980×470 à l'ouverture, minimum 760×440, pas de maximum | *Remplacée par DEC-54* |
| [DEC-52](#dec-52) | L'entrée est lue avec `-f mov` (ffprobe et ffmpeg), jamais devinée d'après le contenu | Acceptée |
| [DEC-53](#dec-53) | Une durée qui n'est pas un nombre fini (`inf`, `nan`) est refusée | Acceptée |
| [DEC-54](#dec-54) | La fenêtre reprend sa taille fixe : 980×470, non redimensionnable | *Remplacée par DEC-56* |
| [DEC-55](#dec-55) | Les journaux gardent Go dans leur texte : on archive, on ne réécrit pas | Acceptée (amende DEC-49) |
| [DEC-56](#dec-56) | La fenêtre est fixe, 860×332, avec un panneau de conversion à droite | *Remplacée par DEC-59* |
| [DEC-57](#dec-57) | Menu Theme (Light / Dark, sombre par défaut) et accent cyan par un global `Theme` | Acceptée (remplace le thème sombre forcé de DEC-32) |
| [DEC-58](#dec-58) | La barre de progression est rouge en cours, verte à la fin, grise si arrêtée, et glisse vers sa valeur | Acceptée |
| [DEC-59](#dec-59) | La fenêtre est fixe, 1000×520 : le fichier à gauche, réglages et conversion à droite, qualité en segments, interrupteur | *Remplacée par DEC-61* (la disposition) |
| [DEC-60](#dec-60) | La vignette de l'entrée est une image brute décodée par ffmpeg dans un dossier temporaire, jamais une erreur | Acceptée |
| [DEC-61](#dec-61) | La fenêtre suit la maquette cyan validée : boutons en contexte, carte de conversion à pastille, colonne droite sombre, 1000×570 | Acceptée (remplace DEC-59) |

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
origine du dépôt · Acceptée (partie « Go + Fyne » : *Remplacée par DEC-49*)

**Contexte** — Le destinataire est un utilisateur de caméra d'action qui double-clique ; la CLI a été retirée.
**Décision** — Un seul binaire, Fyne, pas de mode ligne de commande, pas de macOS (aucun runner, aucun signataire).
**Conséquences** — Toute vérification de la GUI exige les en-têtes GL/X11/Wayland ([ENVIRONNEMENT.md](ENVIRONNEMENT.md)). La CLI n'est pas « à remettre ».
**À rouvrir si** — Un besoin de traitement par lots apparaît ; il se traiterait par un bouton « dossier », pas par une CLI.
**Amendée par [DEC-49](#dec-49)** : la partie « Go + Fyne » est *Remplacée* ; l'application est en Rust + Slint (DEC-43), toujours graphique seulement, sans CLI, sans macOS. Les en-têtes GL/X11/Wayland de la Conséquence ne sont plus nécessaires (`ENVIRONNEMENT.md`).

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
**Amendée par [DEC-41](#dec-41)** : « archive » se lit désormais « installateur Windows et AppImage Linux » ; le FFmpeg épinglé y est embarqué comme avant, et le plancher 570.0 est inchangé. Sous Linux les outils sont dans `usr/lib/superview/` de l'AppImage, non plus dans `../lib/superview/`.

### DEC-05
**Les capacités matérielles se mesurent par une sonde d'exécution, en série, puis à la taille réelle.**
2026-09-06 / 2026-09-09 · Acceptée · U-03, U-13, L-71, L-73, L-93

**Contexte** — `ffmpeg -encoders` décrit le binaire, pas le pilote ; six encodeurs annoncés sur la machine de développement ne peuvent ouvrir aucun périphérique. Une sonde 256×256 ne prouve pas qu'une trame 5120×2880 passera (Intel HD 620).
**Décision** — Au démarrage, une trame par encodeur (`ProbeEncoders`), en série parce que les sessions GPU sont comptées ; avant chaque conversion, `verifyEncoderForOutput` resonde à la géométrie de sortie, résultat mis en cache pour la session. `ApplyEncoderProbe` ne retire que les encodeurs **refusés**.
**Conséquences** — La sonde pose **la même question** que l'encodage : `hwDeviceArgs`, `pixelFormatChain` et `encoderFacingArgs` sont partagés, et `TestProbeAndConversionAskTheSameQuestion` compare l'argv entier en ne tolérant que les différences qu'il liste (E-01, corrigé). Une option d'encodeur s'ajoute dans `encoderFacingArgs`, jamais dans un seul des deux constructeurs.
**À rouvrir si** — Jamais vers « lire la liste ». L'option (b) de E-01 — s'adapter à un pilote CQP-only en `-rc_mode CQP -qp N` — reste possible comme réglage « qualité constante » explicite. Voir DEC-48 (2026-10-19) : faite en automatique côté Rust, VAAPI seulement.

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
2026-09-08 · Remplacée par DEC-37 · U-11, U-14, L-88

**Décision** — 290 o/pixel (8 bits) et 455 o/pixel (10 bits) de sortie, mesurés avec `libx265` à `-threads 4`. Les encodeurs matériels rendent 0 : « refuser sur un chiffre que personne n'a mesuré serait pire que ne pas vérifier ».
**À rouvrir si** — La mesure est refaite à `-threads 16` (voir Q-02) ou pour un encodeur matériel.
*Rouverte le 2026-10-14 : Q-02 mesurée, voir DEC-37. Les encodeurs matériels restent non chiffrés.*

### DEC-18
**Le journal va dans un fichier plafonné sous le cache utilisateur ; la progression y est en `debug`.**
2026-09-04 / 2026-09-08 · Acceptée, amendée par DEC-49 · O-02, U-12, L-90

**Décision** — `~/.cache/superview/superview.log`, tronqué au-delà de 5 Mio à l'ouverture ; les événements de progression tirent plusieurs fois par seconde et restent en `debug`. Le chemin est affiché dans le dialogue *Diagnostic*.
**Amendée par [DEC-49](#dec-49)** : le Rust (`logfile.rs`, `journal.rs`) tient la même règle (même chemin, plafond 5 Mio, progression en `debug`) ; seule l'implémentation a changé (crate `log` à la place de `slog`).

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
2026-09-04 · Remplacée par [DEC-49](#dec-49) · X-04, L-09, L-12, L-62

**Décision** — `.golangci.yml` est le seul endroit qui décide. Un job `staticcheck` séparé a appliqué ce que la config désactive ; il a été retiré.

### DEC-22
**La version vient du tag, jamais d'un fichier commité.**
2026-09-05 · Acceptée, amendée par DEC-47 et DEC-49 · R-05, L-65

**Décision** — `FyneApp.toml` n'a pas de `Version` ; `fyne package --app-version` la prend du tag. Un `go build` dit `dev`, avec la révision VCS si elle existe.
**Amendée par [DEC-47](#dec-47)** : pour le binaire Rust, la version vient de la variable d'environnement `SUPERVIEW_VERSION` lue par `build.rs` (tirée du tag par `prepare`), `dev` si elle est absente.
**Amendée par [DEC-49](#dec-49)** : `FyneApp.toml` n'existe plus ; seule la voie `SUPERVIEW_VERSION` reste. Le principe — jamais de numéro dans un fichier commité — tient.

### DEC-23
**`main()` reste long ; l'état est dans `appState`.**
2026-09-04 · Remplacée par [DEC-49](#dec-49) · P-10, L-45

**Décision** — Le défaut était l'état inatteignable, pas la longueur. Les transitions sont des méthodes testées à 100 % ; la construction des widgets ne gagne rien à être découpée.

### DEC-24
**Toute modification passe par une PR fusionnée sur label `merge` ; le workflow choisit la méthode.**
2026-09-11 / 2026-10-14 · Acceptée · M-03, M-06, L-97, L-100, [MERGING.md](MERGING.md)

**Décision** — Ruleset sur `master`, squash par défaut, merge commit si une PR est empilée dessus, suppression de branche par le workflow (le réglage du dépôt ne vaut pas pour le `GITHUB_TOKEN`), et relance de la CI de `master` par le même workflow (DEC-35, pour la même raison). Labelliser par `gh api`, pas `gh pr edit`.

### DEC-25
**La release est un bouton ; `"*"` reste dans `release.yml` ; le `Makefile` de fyne est rustiné.**
2026-09-05 / 2026-09-06 · Acceptée · R-xx, U-04, L-67, L-76

**Décision** — `workflow_dispatch` seul, jamais de tag manuel. La catégorie `"*"` est porteuse : GitHub jette toute PR sans catégorie. Le `Makefile` généré par fyne n'installe pas les outils embarqués et sa ligne d'icône est cassée ; le patch est clé sur les lignes exactes et `make install` est exécuté dans un répertoire jetable pour le prouver. **La partie `Makefile` est *Remplacée* par [DEC-41](#dec-41)** : il n'y a plus de `Makefile` fyne livré.

### DEC-26
**Journaux internes en français, tout ce qu'un tiers lit en anglais ; README en deux langues à parité.**
2026-09-07 · Acceptée · U-08, L-47, L-83

**Décision** — `docs/LECONS.md`, `ANALYSE.md`, `CONTRATS.md`, `ENVIRONNEMENT.md`, ce registre : français. `AGENTS.md`, `RELEASING.md`, `SECURITY.md`, `hardware-support.md`, la GUI, les erreurs, les titres de PR : anglais. `README.md` et `README_FR.md` bougent ensemble (job `readme-parity`), les libellés GUI restent en anglais dans la version française.

### DEC-27
**La suite ne vaut qu'avec `SUPERVIEW_REQUIRE_FFMPEG=1`, `-race`, et un seuil de 50 % module entier.**
2026-09-04 / 2026-09-11 · Acceptée, amendée par DEC-47 et DEC-49 · T-02, M-01, L-50, L-60, L-96

**Décision** — Sans la variable, quinze tests sautent en silence, dont tout ce qui encode. `make test` et la CI la posent. Le seuil mesure `./...`, pas `./internal/common`.
**Amendée par [DEC-47](#dec-47)** : le job `test` de la release exécute la suite Rust avec la variable et un seuil de 50 % de lignes (`cargo llvm-cov`) ; `-race` n'a pas d'équivalent.
**Amendée par [DEC-49](#dec-49)** : seule la partie Rust vit. `SUPERVIEW_REQUIRE_FFMPEG=1` est posée par `scripts/checks/rust.sh`, `make test` et la CI ; le seuil de 50 % de lignes est contrôlé à **chaque PR** par le job *Rust coverage and audit* (et plus seulement à la release).

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

**Amendée par [DEC-51](#dec-51), puis [DEC-54](#dec-54)** : la taille fixe, remplacée par DEC-51, est rétablie par DEC-54 ; le thème sombre forcé est remplacé par le menu Theme de [DEC-57](#dec-57).

### DEC-33
**Le mode audio par défaut est `safe_performance` partout : code, YAML livré, README.**
2026-10-14 · **Acceptée** · CONTRATS.md § 1 ligne 6

**Contexte** — `defaultConfig.PerformanceMode = "safe"` ; `superview.yaml` livre `safe_performance`. Un utilisateur sans fichier de configuration (cas des archives : le YAML n'est pas installé dans `../lib/superview/`) réencode donc l'audio en AAC, l'autre le copie.
**Décision** — Le défaut est `safe_performance` partout : code, YAML livré, README. Copier la piste audio est sans perte et plus rapide, et le repli sur l'AAC existe déjà si la copie est refusée ; `safe` reste accepté comme choix explicite, et la chaîne vide vaut `safe_performance`.
**À rouvrir si** — un conteneur courant refuse la copie audio au point que le repli devienne la règle.

### DEC-34
**Les dialogues de fichiers sont natifs d'abord, Fyne en repli.**
origine · Acceptée, amendée par DEC-43 (6.4) et DEC-49 · B-03 (invalidé)

**Décision** — zenity puis kdialog sous Linux, PowerShell/WinForms sous Windows ; le dialogue Fyne si rien n'est disponible. ~~Les codes de sortie 1 et 255 valent « annulé », pas « erreur ».~~
**Amendée 2026-10-14 (E-20)** — Seul le code 1 vaut « annulé ». 255 est le −1 de zenity, « unexpected error » (zenity(1), *EXIT CODES*) : le prendre pour une annulation empêchait le repli Fyne, qui existe pour ce cas, de s'ouvrir. Le sélecteur tourne dans une goroutine et livre son résultat par `fyne.Do`.
**À rouvrir si** — Fyne livre un dialogue natif portable (suivi : `fyne.io/fyne/v2/dialog`).
**Amendée par [DEC-43](#dec-43) (phase 6.4) puis [DEC-49](#dec-49)** : en Rust, rfd (portail xdg sous Linux, sélecteur natif sous Windows) passe d'abord ; sous Linux, si rfd rend « rien » en moins de 300 ms, `zenity` puis `kdialog` (code de sortie 1 = annulé, tout autre = échec) ; enfin un champ texte dans la fenêtre remplace le dialogue Fyne. PowerShell/WinForms n'existe plus : sous Windows, rfd natif seul. La condition « à rouvrir » est sans objet.

### DEC-35
**La CI de `master` est relancée par le workflow de fusion via `workflow_dispatch` ; le ruleset reste non strict.**
2026-10-14 · Acceptée · E-06, L-104, L-107, DEC-24, [MERGING.md](MERGING.md)

**Contexte** — Depuis que `merge-on-label.yml` fusionne (M-03), la fusion est faite avec le `GITHUB_TOKEN`, et « events triggered by the `GITHUB_TOKEN`, with the exception of `workflow_dispatch` and `repository_dispatch`, will not create a new workflow run » (docs.github.com, *Triggering a workflow*). Cinq fusions (#73 à #77) sans un seul run sur `master`. Trois issues : rendre le ruleset strict (*Require branches to be up to date*), fusionner avec un jeton d'application ou personnel, ou utiliser l'exception.
**Décision** — L'exception. `test.yml` (aujourd'hui `rust.yml`) et `lint.yml` acceptent `workflow_dispatch` ; après la fusion, le job de `merge-on-label.yml` (qui a `actions: write` pour cela) les dispatche sur la branche par défaut, en meilleur effort — un échec de dispatch est un avertissement, pas un rouge sur une PR déjà fusionnée. Le ruleset garde `strict_required_status_checks_policy: false` : strict refuserait toute PR qui n'est pas au sommet de `master` — chaque membre d'une pile, chaque bump Dependabot derrière un autre — tant qu'un humain ne l'a pas rebasée, c'est-à-dire exactement le geste manuel que le label supprime. Un jeton d'application déclencherait tout nativement, au prix d'un secret à longue durée de vie dans un job qui fusionne du code.
**Conséquences** — Une PR est testée contre sa base au moment de son dernier push, pas contre le résultat fusionné ; une régression née de la combinaison de deux PR n'apparaît qu'après coup, sur le run dispatché de `master` — accepté, et c'est pour cela que ce run existe. Après une fusion, `gh run list --branch master --event workflow_dispatch --limit 2` doit montrer deux runs. **La première fusion après la PR qui introduit ce mécanisme ne dispatche rien** : `pull_request_target` exécute la version du workflow qui est sur `master`, donc l'ancienne ; relancer à la main cette fois-là (`gh workflow run test.yml --ref master`, idem `lint.yml`).
**À rouvrir si** — GitHub cesse d'exempter `workflow_dispatch` ; ou le dépôt accepte un jeton d'application (la suppression de branche et le dispatch redeviendraient alors natifs, et DEC-24 à réviser) ; ou les piles de PR disparaissent de la pratique, auquel cas le ruleset strict ne coûte plus rien et teste mieux.

### DEC-36
**`internal/common/` est découpé par responsabilité ; aucun fichier fourre-tout.**
2026-10-14 · Remplacée par [DEC-49](#dec-49) · 13ᵉ passe, à la demande de l'utilisateur (« optimise la structure »)

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
moins vite (L-98). `main()` de `cmd/superview/gui_main.go` n'est **pas** concerné (DEC-23).
**À rouvrir si** — Jamais vers un retour à un fichier unique. Un fichier qui dépasse ~700
lignes (`encode.go` en est à 656) se redécoupe au prochain ajout.

### DEC-37
**Le garde-fou mémoire chiffre au nombre de fils réellement passé à ffmpeg : +4 % par fil au-delà de 4.**
2026-10-14 · Acceptée · Q-02, L-113 · remplace DEC-17

**Contexte** — Les 290 et 455 o/pixel d'U-11 avaient été mesurés à `-threads 4`, alors que l'application encode avec un fil par cœur, jusqu'à 16 pour `libx265`. x265 alloue par fil de trames : la question Q-02 soupçonnait une sous-estimation sur les machines à nombreux cœurs.
**Décision** — Mesurée le 2026-10-14 sur 16 cœurs (`libx265`, preset `medium`, pic de mémoire résidente, plafond cgroup) : 5120×2880 en 8 bits, 3,54 / 4,15 / 5,19 Gio à 4 / 8 / 16 fils ; en 10 bits, 5,30 / 6,14 Gio à 4 / 8 fils et plus de 7 Gio à 16 (tué au plafond) ; 2560×1440 donne les mêmes rapports. La pente est une droite, ×1,16 à 8 fils et ×1,47 à 16 : `besoin × (100 + 4 × (fils − 4)) / 100`, avec les constantes d'U-11 comme base à 4 fils. En dessous de 4 fils on ne crédite rien (non mesuré). Le compte de fils est celui qu'ffmpeg reçoit (`effectiveEncoderThreads` : valeur configurée ou un par cœur, plafonnée à 16 pour `libx265`), aux deux endroits qui appellent le garde-fou (`PerformEncoding` et le repli matériel → CPU). Les encodeurs matériels restent non chiffrés (DEC-17).
**Conséquences** — Une machine à 16 cœurs se voit refuser plus tôt une conversion 10 bits en 5120×2880 qui aurait fini tuée par le noyau. Le banc utilisait une mire sans filtre `remap` : son niveau à 4 fils est 11 à 15 % sous les constantes d'U-11, seule sa pente est retenue. Toute nouvelle valeur de `-threads` ou tout nouvel encodeur logiciel passe par `effectiveEncoderThreads`, jamais par `runtime.NumCPU()` en direct.
**À rouvrir si** — Le banc est refait avec le filtre `remap` et une vraie source (la pente pourrait différer), ou un encodeur matériel est chiffré, ou x265 change sa politique de fils de trames.

### DEC-38
**La télémétrie GoPro (piste `gpmd`) n'est pas conservée.**
2026-10-14 · Acceptée · Q-03, décision de l'utilisateur

**Contexte** — `-map "[v]" -map "0:a?"` ne garde aucun flux de données : la piste GPS, IMU et température d'un fichier HERO est perdue, de la même famille que N-04/N-05. `-map 0:d? -c:d copy` fonctionnerait pour `gpmd` en MP4, au prix de pistes inconnues pour certains lecteurs.
**Décision** — Ne pas la conserver. L'outil existe pour la vidéo ; l'utilisateur n'a pas besoin de la télémétrie.
**Conséquences** — La perte est voulue : ne pas la rouvrir comme défaut. Une conversion ne porte ni piste de données, ni métadonnées de télémétrie.
**À rouvrir si** — Une demande d'utilisateur réelle de télémétrie dans la sortie ; à mesurer alors sur un vrai fichier HERO (`-copy_unknown`, lecteurs qui s'en plaignent).

### DEC-39
**Le code suit l'arborescence du gabarit : `cmd/superview/` et `internal/common/`.**
2026-10-14 · Remplacée par [DEC-49](#dec-49) · à la demande de l'utilisateur (alignement sur `claude-gabarit`)

**Contexte** — Le gabarit range le point d'entrée dans `cmd/<binaire>/` et la logique dans `internal/`. Superview avait `main` à la racine et `common/` exporté, donc importable par n'importe quel module alors qu'aucune autre interface n'existe.
**Décision** — Déplacements purs (`git mv`) : `common/` → `internal/common/` (le **nom de paquet `common` ne change pas**, donc `common.CleanUp`, etc. restent vrais partout), et les fichiers `gui_*.go`, `readme_link_test.go`, `FyneApp.toml`, `Icon.png` → `cmd/superview/`. `fyne package` lit `FyneApp.toml` et `Icon.png` dans son dossier de travail et y écrit l'archive (vérifié dans le source de `fyne.io/tools` v1.7.2, `package.go`) : `release.yml` l'exécute donc dans `cmd/superview` puis ramène l'archive à la racine, ce qui laisse les étapes suivantes intactes. Le dossier s'appelle `superview` et non `app` : l'exécutable intermédiaire de `fyne package` prend le nom du module.
**Conséquences** — `.gitignore` ancre `/superview` (la règle nue aurait ignoré `cmd/superview/` entier) et ajoute `/cmd/superview/superview`. Les journaux `LECONS.md` et `ANALYSE.md` parlent de `common/` et de la racine : ils décrivent l'état d'avant et ne sont pas réécrits. Le test de chaque contrat reste là où il était, à côté du code. Écarts assumés par rapport au gabarit : pas de `CHANGELOG.md` (les notes de version viennent des PR fusionnées), pas de `ci-go.yml` (`test.yml` et `lint.yml` sont plus complets et épinglés par SHA), un seul `DECISIONS.md` au lieu de `docs/decisions/NNNN-*.md`.
**À rouvrir si** — Une seconde interface (CLI) apparaît et doit importer `common` depuis un autre module : il faudrait alors sortir le paquet d'`internal/`.

### DEC-40
**Les docs suivent le gabarit par passerelles ; les journaux sont allégés, pas convertis.**
2026-10-14 · Acceptée · à la demande de l'utilisateur (alignement sur `claude-gabarit`)

**Contexte** — Le gabarit attend `ARCHITECTURE.md`, `PIEGES.md` (+ `pieges/`), `SUITE.md`, `decisions/NNNN-*.md`, `CHANGELOG.md` et `tests/caracterisation/`. Superview a un registre `DECISIONS.md` (40 entrées), deux journaux de 3 300 lignes dont les identifiants (`L-xx`, `E-xx`, `DEC-xx`…) sont cités par le code, et des notes de version générées depuis les PR.
**Décision** — Ajouter `ARCHITECTURE.md` (importée par `CLAUDE.md`), `PIEGES.md` (index d'une ligne par leçon de `LECONS.md` § 2), `pieges/_modele.md` et `SUITE.md` (lu au démarrage par le hook `debut-session`). Alléger les journaux **sans réécrire une ligne** : les passes 3 à 13 d'`ANALYSE.md` vont dans `archives/ANALYSE-passes-3-a-13.md`, le § 3 de `LECONS.md` dans `CORRECTIONS.md` (journal vivant, pas une archive : on y ajoute encore). Chaque section déplacée laisse un titre-renvoi, pour que les 87 renvois « § 3bis … § 3duodecies » restent vrais.
**Conséquences** — Écarts assumés : pas de `CHANGELOG.md` (les notes de version viennent des PR, et un second journal serait saisi deux fois), pas de `ci-go.yml` (`test.yml` et `lint.yml` sont plus complets, épinglés par SHA, et couvrent Windows), un seul `DECISIONS.md` au lieu de `decisions/NNNN-*.md` (les numéros `DEC-xx` sont cités partout), des tests de caractérisation `*_test.go` à côté du code plutôt que dans `tests/caracterisation/` (Go ne teste pas l'interne depuis un autre dossier). `PIEGES.md` n'est **pas** importé par `CLAUDE.md` : 114 lignes à chaque session pour un index qu'on interroge par mot-clé. Un constat archivé se retrouve par `grep -rn "U-03" docs/`.
**À rouvrir si** — Un outil lit `docs/decisions/` ou `CHANGELOG.md` en dehors de Claude Code, ou l'archive redevient la partie qu'on consulte le plus (alors la remonter).

### DEC-41
**La release livre un installateur Windows et une AppImage Linux, pas des archives.**
2026-10-14 · Acceptée · à la demande de l'utilisateur (modèle : OpenTuneBox)

**Contexte** — Les releases étaient un `.zip` (à extraire, avec trois fichiers à garder ensemble) et un `.tar.xz` (avec le `Makefile` de fyne rustiné, DEC-25). L'utilisateur voulait, comme OpenTuneBox, un fichier prêt à lancer par système.
**Décision** — Windows : `superview-<étiquette>-windows-x86_64-setup.exe` (l'étiquette est le tag, `v0.3.0`, ou `RC-0.3.0` pour une pré-version), un installateur Inno Setup 6 (`packaging/windows/superview.iss`), par utilisateur sans droits d'administrateur, qui dépose `superview.exe`, `ffmpeg.exe` et `ffprobe.exe` dans le même dossier. Linux : `superview-<version>-linux-x86_64.AppImage` (`packaging/linux/build_appimage.sh`), avec `appimagetool` 1.9.1 épinglé par SHA-256, les outils dans `usr/lib/superview/`. Les anciens formats sont retirés, pas gardés en double. `fyne package` reste l'outil qui compile et tamponne la version ; seul le binaire en est repris. Chaque paquet est essayé en CI : installation, outils exécutés et désinstallation sous Windows ; extraction, outils exécutés et lancement sous `xvfb` (code 124 de `timeout` = resté ouvert) sous Linux.
**Conséquences** — Le patch du `Makefile` de fyne et son `make install` jetable disparaissent (DEC-25). L'`AppId` de l'installateur ne doit jamais changer : c'est lui qui permet la mise à jour par-dessus. L'AppImage exige FUSE 2 sur le poste de l'utilisateur (`libfuse2t64` sous Ubuntu 24.04) ou `--appimage-extract-and-run`, noté dans les deux README. Pas de signature de code : SmartScreen avertit toujours (le README l'explique). `fyne package` réécrit `cmd/superview/FyneApp.toml` quand on le lance en local (`Build` incrémenté, commentaires perdus) : `git checkout cmd/superview/FyneApp.toml` après.
**Amendée par [DEC-47](#dec-47)** : le binaire emballé est celui de Rust (`build_appimage.sh <binaire> <outils> <sortie>`), plus de `fyne package` ; l'AppImage embarque trois bibliothèques X11 (le trio cohérent).
**À rouvrir si** — Un paquet `.deb`/Flatpak est demandé, si la signature de code devient possible, ou si les AppImage cessent de fonctionner sur Ubuntu LTS (FUSE).

### DEC-42
**Un agent peut déclencher la release, sur demande explicite de l'utilisateur, par `/publier` et nulle part ailleurs.**
2026-10-14 · Acceptée · à la demande de l'utilisateur (comme pour OpenTuneBox)

**Contexte** — La release est le bouton *Run workflow* avec une version (DEC-25). `.claude/settings.json` n'autorisait pas `gh workflow run`, et refuse `git tag` et `gh release create`. Publier est public et dépense un numéro de version pour toujours.
**Décision** — `settings.json` autorise `gh workflow run release.yml *`, `gh run watch *` et `gh release view *` ; `git tag` et `gh release create` restent refusés (c'est le workflow qui crée le tag, une fois les deux plateformes construites). Le skill `.claude/skills/publier/SKILL.md` encadre la procédure : demande explicite dans la conversation, `master` à jour et vert, aucune PR attendue encore ouverte, essai à blanc postérieur au dernier changement de la release (L-114), version proposée puis **confirmée par l'utilisateur**, lancement, suivi, vérification des trois fichiers publiés.
**Conséquences** — L'autorisation de `settings.json` ne distingue pas un essai à blanc d'une vraie release (seul `-f version=` les sépare) : le garde-fou est la règle d'`AGENTS.md` et le skill, pas le moteur de permissions. Une version publiée ne se reprend pas ; un échec avant la fin ne dépense pas le numéro.
**À rouvrir si** — Un agent publie sans demande, ou si les droits d'un jeton permettaient de contourner le workflow.

### DEC-43
**Superview est porté vers Rust + Slint ; la version Go reste la référence et continue d'être livrée jusqu'à la parité ; `rust/` cohabite avec le code Go.**
2026-10-14 · Acceptée, GO donné le 2026-10-15, port terminé le 2026-10-20 · à la demande de l'utilisateur

**Contexte** — Le code Go pèse 7 200 lignes de code et 11 100 de tests, et l'utilisateur le trouve difficile à maintenir. Les paquets de release (DEC-41) sont en place. FFmpeg et ffprobe ne changent pas : mêmes binaires embarqués, mêmes pins, mêmes arguments (DEC-04, plancher NVENC).
**Décision** — Un workspace Cargo sous `rust/` (edition 2024) : `superview-core` (logique, sans GUI), `superview-harness` (banc d'essai, DEC-44) et `superview-app` (Slint, licence GPLv3 compatible avec celle de Superview). Une PR par étape, `scripts/check.sh` vert sur Go **et** Rust tant qu'ils cohabitent, `scripts/checks/rust.sh` en est la moitié Rust. Un oracle de parité (`testdata/parity/*.json`, écrit côté Go) fixe ce que Rust doit reproduire. La phase 3 est un essai (cartes PGM aux empreintes dorées sur les deux OS, fenêtre Slint, AppImage d'essai) suivi d'une porte GO/NO-GO.
**Conséquences** — Pendant le portage, le dépôt a deux chaînes d'outils : le pre-commit exige Rust installé (`docs/ENVIRONNEMENT.md`). Les `math.Pow(x, 2)` de `pgm.go` se portent en `powi(2)`, jamais `powf(2.0)` (dépend de la libm). Aucun test Rust ne modifie l'environnement du processus (`std::env::set_var` est `unsafe` en edition 2024 et les tests tournent en parallèle) : la résolution des outils reçoit l'environnement en paramètre (L-117). Un NO-GO coûte une PR de suppression de `rust/`, rien de plus. À la bascule, les DEC liées à Go/Fyne (21, 23, 36, 39, partie « Go + Fyne » de DEC-02) seront *Remplacées* et d'autres amendées.
**Porte GO/NO-GO (2026-10-15)** — L'essai de la phase 3 a donné **GO** : JSON d'oracle reproduits champ par champ et 4 empreintes PGM identiques (Linux et Windows), génération des cartes environ 2 fois plus rapide que Go (critère : au plus 1,5 fois plus lent), fenêtre Slint ouverte sous xvfb, sous Windows et dans l'AppImage d'essai. La phase 4 (cœur `superview-core`) est lancée. Choix de la phase 4, PR 1 : le YAML se lit avec `serde-saphyr` (Rust sûr, sans RUSTSEC), pas `serde_yaml_ng` (qui repose sur `unsafe-libyaml`, signalé non maintenu, RUSTSEC-2024-0449) ; la migration des clés retirées et les variables `SUPERVIEW_*` (sauf `SUPERVIEW_FFMPEG_DIR`) ne sont pas portées. Divergences voulues du décodeur YAML avec Go : un nombre entre guillemets (`"5"`) est accepté dans un champ entier ; plusieurs documents (`---`) sont refusés (Go lisait le premier) ; un décimal dans un champ entier (`1e6`, `1.5`) est refusé (yaml.v3 acceptait `1e6` et tronquait `1.5` en 1, ce qui était un défaut). Le fichier est lu en UTF-8 (BOM toléré) ou UTF-16 avec BOM, comme yaml.v3.
**Phase 4 terminée (2026-10-15)** — Le cœur `superview-core` convertit de bout en bout (`perform_encoding`, PR #105 à #115), chaque morceau vérifié contre l'oracle Go ou contre ses tests portés, la CI Windows comprise. Écarts voulus avec Go, tous écrits dans l'en-tête de leur module : pas de globaux ni de bus d'événements, Diagnostic réduit à ffmpeg/ffprobe/disque, un arrêt demandé s'arrête avant de lancer quoi que ce soit, un chemin non UTF-8 est refusé. Reste : banc d'essai (phase 5), interface Slint (phase 6).

**Phase 5 terminée (2026-10-17)** — Banc d'essai `superview-harness` (#117), tests à vrai ffmpeg portés (#118), croisement Go↔Rust sur le même clip (L-116 : libx264 comparé bit à bit à un fil, libx265 par PSNR, car non reproductible).

**Phase 6, PR 6.1 (2026-10-17)** — La logique de la fenêtre (`state.rs`, `format.rs`, `prefs.rs`, sans type Slint) est portée méthode pour méthode depuis `appState`. Écarts voulus avec Go : (a) en fin de conversion ratée ou annulée la ligne « Hardware: … » est vidée au lieu de rester sur un plan que personne n'a exécuté, et une vérification matérielle qui échoue la remplace par un texte neutre ; (b) un chemin d'entrée ou de sortie non UTF-8 est refusé avec un message clair (`check_video` prend un `&str`) ; (c) les dialogues deviennent des valeurs (`Completion`, `ChosenOutcome`) que la fenêtre affiche, et la vérification vidéo et la vérification matérielle sont injectées ; (d) la génération de la ligne matérielle avance à chaque rafraîchissement, au début d'une conversion et quand la fin l'écrit, pas seulement quand le plan est calculé. Préférences : voir DEC-46.

**Phase 6, PR 6.3 (2026-10-18)** — Le câblage (`app.rs`) porte le corps de `main` de Go. Écarts voulus : (a) les dialogues sont le composant `Modal` du `.slint`, un seul à l'écran, les suivants en file (jamais deux fois le même) ; (b) un panic de l'encodeur, d'un sélecteur, d'ffprobe, d'une sonde ou du Diagnostic est attrapé sur son fil et devient une erreur affichée, une annulation ou un texte neutre, la fenêtre n'est jamais laissée verrouillée ; (c) `check_video` (ffprobe) tourne sur le fil du sélecteur, bouton tenu enfoncé, et non sur le fil de l'interface ; (d) le lien d'installation s'ouvre par `xdg-open` / `explorer.exe` (jamais une chaîne shell) et seulement pour l'adresse constante du programme ; (e) un `Prober` et un résolveur par processus, en `static` (`OnceLock`), sans `Box::leak` ; (f) SIGINT/SIGTERM : `signal-hook` (Unix) et `ctrlc` (Windows) comme le banc, installés par l'application et jamais par le cœur ; le signal termine la boucle d'événements, la conversion en cours le voit par son drapeau et se déroule seule. Le trajet signal → fin de boucle → attente n'est pas couvert par un test de fenêtre (les tests simulent seulement la réponse `StoppedBySignal` du moteur) ; seul le banc teste un vrai signal sur une vraie conversion. Les fils de sonde, de vérification et de Diagnostic ne sont pas attendus à la sortie. Le dossier de Diagnostic est le dossier temporaire du système (les tests passent un dossier jetable et vérifient qu'il reste vide).

**Phase 6, PR 6.4 (2026-10-18)** — Les sélecteurs de fichiers (`dialogs.rs`) : rfd d'abord (portail xdg puis zenity, tous deux dans rfd), filtre « MP4 Video » (`*.mp4 *.MP4`), titres « Select input video » / « Save output video », nom par défaut `output.mp4`, `.mp4` toujours imposé à la sortie. **Pourquoi un délai** : rfd 0.17 renvoie `None` pour une annulation comme pour « ni portail ni zenity », et ne journalise que la différence ; l'API ne permet pas de les distinguer, et rfd ne connaît pas kdialog (la machine de l'utilisateur, sous KDE, a kdialog et pas zenity). Sous Linux, un `None` rendu en **moins de 300 ms** (horloge injectée, jamais de `sleep` dans les tests) est donc lu comme « le sélecteur n'a pas pu s'ouvrir », parce qu'un humain n'annule pas en 300 ms ; on essaie alors nos propres `zenity` puis `kdialog` (portés de `gui_native_dialog_linux.go` : mêmes arguments, code de sortie 1 = annulé, tout autre code = échec donc outil suivant, `exec` avec arguments, jamais une chaîne shell), puis, si aucun n'existe, un champ texte dans la fenêtre (popup `PathEntry`, un seul à la fois, en file comme les autres). Le chemin tapé passe par la même validation qu'un fichier choisi (ffprobe pour l'entrée, `.mp4` imposé pour la sortie) et doit être absolu ; vide ou relatif : le champ reste ouvert avec la raison. Prix assumé : quelqu'un qui annule en moins de 300 ms obtient le second sélecteur. Écarts voulus : (a) sous Windows, rfd natif seul, un `None` est une annulation (pas de repli, pas de PowerShell, voir plus haut) ; (b) le filtre de kdialog liste aussi `*.MP4` (celui de Go l'oubliait, et un filtre est sensible à la casse) ; (c) l'icône de la fenêtre est `cmd/superview/Icon.png`, lue là où elle est (`@image-url` relatif, intégrée au binaire par `build.rs` : par défaut Slint la chercherait à son chemin de compilation au lancement), sans seconde copie ; l'option qui décode à la compilation intègre aussi les glyphes et ne compile pas. Pas encore faits (phase 7) : l'icône du `.exe` et du `.desktop`. **Repli collant** : après un premier `None` rapide, rfd n'est plus interrogé de la session (un second clic ne rattend pas un portail qui ne répond pas). **Mode d'échec qui reste** : un portail installé mais sans backend FileChooser, ou un démarrage à froid du portail qui dépasse 300 ms, est pris pour une annulation (le bouton ne fait rien, sans message) ; le seuil ne peut pas faire mieux tant que rfd ne distingue pas les cas. Écarts avec Go : (d) Go n'essaie que le premier outil trouvé (zenity avant kdialog), Rust passe au suivant si le premier échoue ; (e) le dernier repli de Go est le navigateur de fichiers de Fyne, celui de Rust un champ texte : régression d'ergonomie assumée (plus de navigation ni de filtre MP4, un chemin absolu à taper) ; (f) un chemin rendu par zenity/kdialog garde ses octets (seul le saut de ligne final part) et l'état refuse ensuite un chemin non UTF-8 avec son message ; (g) seules les entrées absolues de `PATH` servent à chercher zenity/kdialog. **Dette** : l'icône de la fenêtre lit `cmd/superview/Icon.png`, dans le dossier du code Go ; au retrait de Go (phase 8) le fichier doit passer sous `rust/` et le chemin de `ui/main.slint` être corrigé. La checklist à passer à la main avant la pré-version est dans `docs/TESTS-MANUELS.md`.

**Phase 6, correctif du Diagnostic (2026-10-18)** — Le rapport s'affichait dans un `ScrollView` de 520x300 px sans ascenseur visible, et un rapport long démarrait au milieu : on ne pouvait pas tout lire (vu sous Xvfb). Il est maintenant dans un `TextEdit` en lecture seule (ascenseurs, molette, sélection et Ctrl+C pour joindre le rapport à un bug), dans une carte qui prend la hauteur de la fenêtre et jusqu'à 900 px de large. Écart avec Go (Fyne) : le rapport est sélectionnable et copiable. Les marques ✅/❌ du rapport du cœur (texte de Go, figé par la parité) ne s'affichaient pas du tout chez l'utilisateur : le rendu logiciel de Slint ne dessine pas les emoji en couleur. La fenêtre les remplace par `[OK]` / `[FAIL]` à l'affichage (`plain_marks`, `app.rs`) ; le texte du cœur ne change pas.

**Phase 6, icônes du Diagnostic (2026-10-18)** — L'utilisateur voulait retrouver de vraies icônes. Le rapport s'affiche par défaut en **liste de lignes** : `report.rs` (pur, testé) lit le texte du cœur, emoji compris, et en tire des lignes typées (titre, contrôle avec verdict, valeur, statut global, simple, vide) ; une ligne inconnue reste une ligne simple, et un test recompose le rapport à l'identique pour garantir qu'aucun texte n'est perdu. `main.slint` dessine l'icône (disque vert à coche, disque rouge à croix, `accessible-label` « OK » / « Failed ») : aucun emoji, aucune police. Slint n'a pas d'API presse-papiers : le bouton « Show as text » bascule vers le `TextEdit` en lecture seule (`[OK]` / `[FAIL]`, Ctrl+C), « Show as list » revient. Piège de mise en page, vu sous Xvfb : une ligne faite de layouts (`HorizontalLayout` + `Text` qui passe à la ligne) annonce la hauteur d'une seule ligne, et le `ScrollView` s'arrêtait quelques lignes avant la fin du rapport (« Log file » inatteignable) ; un `ListView` fait pire (lignes qui se chevauchent, hauteurs supposées uniformes). `ReportLine` place donc ses éléments à la main et prend la `preferred-height` du `Text` à la largeur donnée. `plain_marks` s'applique au rapport entier (y compris « Superview build » et « Log file »), pas au seul bloc du cœur : plus simple, et ces deux lignes ne contiennent jamais d'emoji (un chemin de journal avec ✅ serait le seul cas). Le rapport est reposé dans la fenêtre seulement s'il a changé : un dialogue mis en file derrière lui ne reconstruit pas la liste. La liste ne défile pas au clavier (molette et ascenseur). Le texte du cœur ne change toujours pas.

**Port terminé (2026-10-20)** — Phase 7 faite (RC-0.3.0 livre le binaire Rust, DEC-47) et phase 8 : Go est retiré (DEC-49, #136 et #137), la prose suit en 8.3. Les DEC liées à Go/Fyne annoncées ci-dessus (21, 23, 36, 39, partie « Go + Fyne » de DEC-02) sont *Remplacées* ; 18, 22, 27, 34 et 44 sont amendées. Les fixtures de parité (`testdata/parity/*.json`) ont été déplacées sous `rust/crates/superview-core/tests/fixtures/` et sont gelées ; le « croisement Go↔Rust » (phase 5) a disparu avec Go. Les en-têtes de modules Rust qui disent « écart voulu avec Go » restent des faits historiques sur le code retiré.

**À rouvrir si** — Une PR de la phase 4 rend la parité impossible à tenir (oracle ou empreintes divergents), ou si les PR restent jugées illisibles.

### DEC-44
**`superview-harness` est un banc d'essai, jamais livré ; ce n'est pas une CLI et DEC-02 tient.**
2026-10-14 · Acceptée, amendée par DEC-49 · conséquence de DEC-43

**Contexte** — Les tests croisés Go↔Rust et les tests FFmpeg réels ont besoin d'un exécutable sans fenêtre (`convert`, `probe`, sortie JSON). DEC-02 interdit un mode ligne de commande pour l'utilisateur.
**Décision** — Le binaire `superview-harness` (`publish = false`) n'est ni empaqueté ni mentionné dans les README, ni installé par l'installateur ou l'AppImage. Il sert aux tests et à la comparaison avec Go, rien d'autre.
**Conséquences** — Le produit reste une application graphique seule. Aucun argument de `superview-harness` n'est un contrat public.
**À rouvrir si** — Le harnais est référencé dans un README ou un paquet : c'est alors une CLI, et DEC-02 doit être rouverte explicitement.
**Amendée par [DEC-49](#dec-49)** : il ne sert plus à comparer avec Go (le croisement et le job `cross-check` ont disparu). Il reste un banc d'essai pour les tests qui lancent une vraie conversion dans un processus séparé : sortie JSON de `convert` et `probe`, et les tests de signal (SIGINT/SIGTERM, Unix) qui vérifient aussi qu'aucun dossier temporaire ne reste. Rien d'autre n'a changé : jamais livré, jamais documenté pour l'utilisateur.

### DEC-45
**Le cœur Rust cherche ffmpeg et ffprobe sur `PATH` parmi les seuls `.com` et `.exe` ; Go, lui, accepte aussi `.bat` et `.cmd`.**
2026-10-17 · Acceptée · à la demande de l'utilisateur

**Contexte** — Un `.bat` se lance par `cmd.exe`. Annuler tue `cmd.exe` et laisse ffmpeg tourner, ce qui viole « l'annulation ne laisse rien derrière elle ». Un vrai ffmpeg est un `.exe`.
**Décision** — `lookup_in_path` (`tools.rs`) ne garde dans `PATHEXT` que `.com` et `.exe`. Seuls les faux outils des tests Windows sont des `.bat` : ils passent par `ToolResolver::with_batch_stand_ins`, réservé aux tests.
**Conséquences** — Écart voulu avec Go, qui reste la référence mais n'est plus corrigé sur ce point. Quelqu'un qui n'aurait qu'un `.bat` nommé `ffmpeg` sur `PATH` n'est plus servi : il doit pointer `SUPERVIEW_FFMPEG_DIR` vers un vrai `ffmpeg.exe`.
**À rouvrir si** — Un besoin réel d'un lanceur `.bat` apparaît ; il faudrait alors tuer l'arbre de processus (job object), pas seulement `cmd.exe`.

### DEC-46
**L'interface Rust garde ses préférences dans un fichier neuf, `preferences.json` ; elle ne reprend aucun choix de la version Go/Fyne.**
2026-10-17 · Acceptée · conséquence de DEC-43

**Contexte** — La version Go enregistre trois choix (profil de qualité, codec, case « source déjà étirée ») dans les préférences de Fyne. Fyne disparaît avec Go, et leur format n'est pas un contrat.
**Décision** — Trois clés (`quality_profile`, `encoder_selection`, `squeeze_source`) dans `<dossier de configuration utilisateur>/superview/preferences.json`, écrites de façon atomique. Un fichier absent, illisible ou invalide donne les valeurs par défaut (« Balanced », codec de la source, case décochée) et un avertissement dans le journal, jamais une erreur. Aucune lecture des préférences Fyne.
**Conséquences** — Au premier lancement de la version Rust, quelqu'un qui avait choisi « Fast » ou un codec le choisit de nouveau, une fois. Une clé de type faux invalide tout le fichier (défauts), elle n'est pas rattrapée clé par clé.
**À rouvrir si** — Des utilisateurs se plaignent de la perte de leurs choix à la bascule : une lecture unique des préférences Fyne serait alors à écrire.

### DEC-47
**La release livre le binaire Rust ; l'AppImage embarque trois bibliothèques X11 (le trio cohérent) sans `LD_LIBRARY_PATH`.**
2026-10-18 · Acceptée · PR 7.2 du port (DEC-43)

**Contexte** — `release.yml` compilait et emballait le programme Go par `fyne package`. Le binaire Rust (`rust/target/release/superview`, DEC-43) est livrable depuis la PR 7.1. winit charge `libxkbcommon-x11.so.0` par `dlopen` sous X11, et celle-ci dépend de `libxcb-xkb.so.1` : un poste minimal n'a ni l'une ni l'autre.
**Décision** — (1) **Bascule franche** : `release.yml` ne livre plus que Rust ; Go reste dans le dépôt (tests, référence) jusqu'à la phase 8. (2) Les deux build jobs font `cargo build --release --locked -p superview-app` avec `SUPERVIEW_VERSION` ; le marqueur `SUPERVIEW-BUILD version=… modified=…`, lu dans le binaire, remplace `go version -m` : échec si `modified=true` ou si la version n'est pas celle attendue. **Pas de cache de `target/`** dans ces jobs : le marqueur doit décrire ce build. (3) `build_appimage.sh <binaire> <outils> <sortie>` copie les trois bibliothèques du poste de build dans `usr/lib/` ; il échoue clairement si elles manquent. (4) **Pas de `LD_LIBRARY_PATH` dans `AppRun`** : il serait hérité par ffmpeg, zenity et kdialog (un outil Qt chargerait nos copies). À la place, `build.rs` lie le binaire (Linux, `rustc-link-arg-bins` seulement : ni tests ni scripts de build, et `RUSTFLAGS` ne peut pas l'écraser) avec `-Wl,--disable-new-dtags -Wl,-rpath,$ORIGIN/../lib`. (5) Le job `test` de la release : `cargo fmt/clippy`, puis `cargo llvm-cov` (0.9.1, un seul passage, `SUPERVIEW_REQUIRE_FFMPEG=1`) avec un plancher de 50 % de lignes, puis `cargo audit` (0.22.2) ; les outils sont épinglés et installés `--locked`. Mesuré le 2026-10-18 : 95,5 % de lignes, audit sans vulnérabilité (5 avertissements « non maintenu »). (6) Les pins FFmpeg, le plancher NVENC 570.0, l'`AppId`, l'installation silencieuse sous Windows ne changent pas.
**Mesures (`LD_DEBUG=libs`, AppImage extraite, sous `xvfb`)** — Avec DT_RPATH `$ORIGIN/../lib` : `libxkbcommon-x11.so.0` **et** `libxcb-xkb.so.1` viennent de `usr/lib/` de l'image. Sans rpath (contre-preuve) : les deux viennent de `/usr/lib/x86_64-linux-gnu`. Avec un **RUNPATH** (le défaut) : `libxkbcommon-x11` vient de l'image mais `libxcb-xkb` du système, car un RUNPATH ne s'applique pas aux dépendances d'une bibliothèque chargée par `dlopen` ; d'où `--disable-new-dtags`. `packaging/linux/check_appimage_libs.sh` rejoue cette mesure en CI (rust.yml à chaque PR, release.yml) après avoir retiré `libxkbcommon-x11-0` du poste.
**Piège (mesuré le 2026-10-18 avec l'AppImage du dry run)** — Embarquer seulement `libxkbcommon-x11` (1.6, Ubuntu 24.04) et `libxcb-xkb` : sur un poste dont la `libxkbcommon.so.0` système est plus récente (1.13), les deux se chargent ensemble et l'application plante au démarrage (SIGSEGV, code 139). Le DT_RPATH résout tout depuis `usr/lib` : on embarque donc le trio cohérent `libxkbcommon.so.0` + `libxkbcommon-x11.so.0` + `libxcb-xkb.so.1` (paquets `libxkbcommon0`, `libxkbcommon-x11-0`, `libxcb-xkb1`) ; `check_appimage_libs.sh` exige les trois depuis l'image. Avec le trio 24.04 complet sur ce poste : code 124 (reste ouverte), `LD_DEBUG` montre les trois copies de l'image. **La CI ne peut pas détecter ce défaut** (le runner a les mêmes versions que l'image) : **test à la main** avant chaque version, lancer l'AppImage sur un poste dont la `libxkbcommon` système est plus récente que celle de l'image.
**Conséquences** — Le poste de build doit avoir `libxkbcommon0`, `libxkbcommon-x11-0` et `libxcb-xkb1` au moment de `build_appimage.sh`. Les bibliothèques embarquées sont celles d'Ubuntu 24.04 ; seule `libxcb.so.1`, que tout poste de bureau a, reste prise au système. Le test de lancement ne prouve l'embarquement que si la bibliothèque système est absente : l'étape échoue si le paquet est encore là. `cargo audit` lit la base d'avis la plus récente, qui n'est **pas épinglée** : un avis publié le matin peut bloquer la release l'après-midi ; la parade est un `ignore` motivé dans `rust/.cargo/audit.toml`, pas le retrait de l'étape. Le contrôle du marqueur exige `revision=<HEAD> modified=false` (le positif), car sans git `build.rs` écrit `modified=false` sans révision. L'AppImage et `THIRD_PARTY_NOTICES.md` portent les licences des trois bibliothèques. Le seuil de 50 % est très en dessous du mesuré : à remonter quand on le décide, pas avant.
**À rouvrir si** — winit cesse de passer par `dlopen` (liaison statique), si le DT_RPATH est refusé par un outil de packaging, ou si une autre bibliothèque chargée à l'exécution manque sur des postes minimaux.

### DEC-48
**Un pilote VAAPI qui n'accepte que le contrôle de débit CQP est utilisé en « qualité constante » (`-rc_mode CQP -qp N`) au lieu d'être refusé ; Go, lui, le refuse toujours.**
2026-10-19 · Acceptée · rouvre l'option (b) de DEC-05 / E-01, en automatique

**Contexte** — Un pilote Intel VAAPI sans firmware HuC répond à tout débit par `Driver does not support any RC mode compatible with selected options (supported modes: CQP)`. DEC-05 (arbitrage (a), E-01) a fait refuser cet encodeur avec ses propres mots, et laissait l'option (b) — s'adapter — « possible comme réglage explicite ». Mesuré le 2026-10-19 avec le FFmpeg embarqué sur un Tiger Lake : `-init_hw_device vaapi=va:/dev/dri/renderD128 -filter_hw_device va … -vf format=nv12,hwupload -c:v h264_vaapi -rc_mode CQP -qp 23`, et `hevc_vaapi … -qp 25`, produisent un MP4 valide ; avec `-b:v` les deux échouent.
**Décision** — (1) `RateControl { Bitrate, ConstantQuality }` (`encode/args.rs`), `Bitrate` par défaut : son argv est celui que les oracles Go figent, octet pour octet. `ConstantQuality` donne `-rc_mode CQP -qp N` et **aucun** `-b:v`, `-maxrate`, `-bufsize`. QP : H.264 23, HEVC 25 au profil Balanced (preset `medium` ou vide), +2 si le preset configuré est `fast`. (2) Le mode passe par `encoder_facing_args` et `ProbeSpec` (y compris `cache_key`) : la sonde pose la même question que la conversion (DEC-05), `probe_and_conversion_ask_the_same_question` est rejoué avec les deux modes. (3) La sonde (`probe_encoder_at_size`, donc le balayage du démarrage comme la vérification à la taille de sortie) resonde en `ConstantQuality` **seulement** si l'encodeur est `h264_vaapi` ou `hevc_vaapi` **et** que la raison du refus contient `supported modes: CQP)` (parenthèse fermante juste après CQP : un pilote à plusieurs modes, `supported modes: CQP CBR VBR)`, n'est pas concerné). Pas pour QSV (son échec est un firmware absent), pas pour une autre raison. Le refus en débit reste dans le journal (avertissement). (4) `verify_encoder_with_rate_control` rend le mode avec l'encodeur ; le pipeline le passe à la cascade, dont le repli vers un encodeur logiciel repasse en `Bitrate`. (5) `validate_bitrate` et `check_output_space` gardent le débit demandé comme estimation : en qualité constante la taille réelle suit le contenu et peut s'en écarter. (6) Le Diagnostic dit `works, constant quality only`, la ligne « Hardware: » ajoute « (constant quality) ».
**Conséquences** — Écart voulu avec Go, qui n'est plus corrigé sur ce point : il est retiré en phase 8. Un utilisateur dans ce cas obtient un fichier dont le poids ne suit pas son débit demandé. Le « débit » de l'interface n'a plus d'effet sur lui, et il n'en est averti que par le journal et la ligne « Hardware: ». QSV a toujours besoin du firmware HuC.
**À rouvrir si** — Un pilote CQP-only donne des fichiers aberrants (trop lourds, ou d'une qualité visiblement trop basse) avec ces QP ; ou si un réglage « qualité constante » explicite que l'utilisateur choisit devient demandé.

**Amendement du 2026-10-20** — Le résumé d'un refus (`summarize_probe_failure`) ignore les lignes `libva info:` (trois lignes d'information que libva imprime avant toute session VAAPI ou QSV) : elles remplissaient les trois lignes gardées et chassaient la vraie erreur (RC-0.3.0 : un refus QSV s'affichait « libva info: … ; libva info: … ; libva info: Found init function »). Une ligne `libva error:` reste, c'est une cause. Écart voulu avec Go, qui les garde. Le marqueur du pilote CQP seul survit ainsi à ce bruit placé devant lui.

### DEC-49
**Go est retiré du dépôt ; le Rust (Slint) est la seule base de code.**
2026-10-20 · Acceptée · phase 8 du port, #136 et #137 · remplace DEC-21, DEC-23, DEC-36, DEC-39 et la partie « Go + Fyne » de DEC-02

**Contexte** — Depuis RC-0.3.0 (2026-10-20, DEC-47) la release livre le binaire Rust ; Go ne servait plus qu'à la CI, aux oracles de parité et de référence. Deux chaînes d'outils coûtaient chaque commit (le pre-commit exigeait Go **et** cargo) et la prose décrivait un produit qui n'était plus celui livré. Le plan DEC-43 prévoyait ce retrait comme dernière phase.
**Décision** — (1) Le code Go (`cmd/superview/`, `internal/common/`, `go.mod`, `.golangci.yml`, `FyneApp.toml`) et ses jobs CI sont supprimés (#137), après que #136 a porté en Rust ce qu'ils protégeaient : les gardes de géométrie des cartes PGM, la source portrait refusée avant tout, l'annulation avant démarrage, l'ancre `#download-and-install` du README et l'interdiction d'un `| head` qui coupe la sortie d'un outil dans les workflows (`superview-app/src/repo_guards.rs`, L-115). (2) Les fixtures de parité (`parity/*.json`, `ffprobe/`) sont **gelées** sous `rust/crates/superview-core/tests/fixtures/` : enregistrées sur Go, elles sont désormais la spécification, on ne les régénère pas. (3) Le contrôle de couverture (50 % de lignes, `cargo llvm-cov`) et l'audit (`cargo audit`) tournent à **chaque PR** dans le job *Rust coverage and audit* de `rust.yml`, avec les versions épinglées de la release. (4) Les checks requis du ruleset de `master` sont les checks Rust (*Rust on ubuntu-latest*, *Rust on windows-latest*, *Rust coverage and audit*) et *README parity (EN/FR)* ; plus aucun check Go n'y figure (ruleset modifié avant la fusion de #137 et vérifié le 2026-10-20 par `gh api repos/Canaill51/superview/rulesets/22935151`). (5) La dernière version Go, **v0.2.9**, reste publiée et dans l'historique git. (6) La prose est réécrite pour Rust seul (`AGENTS.md`, `ARCHITECTURE.md`, `CONTRATS.md`, `ENVIRONNEMENT.md`, READMEs, `RELEASING.md`) ; les journaux (`CORRECTIONS.md`, `LECONS.md`, `ANALYSE.md`, `archives/`) et le texte des DEC antérieures ne sont pas réécrits : ils décrivent l'état d'avant.
**Conséquences** — Un seul `scripts/check.sh` (format, clippy, tests Rust). Les noms de fonctions Go cités dans les DEC et les journaux (`GeneratePGM`, `PerformEncoding`…) sont ceux du code retiré ; leur équivalent Rust est en `snake_case` (même nom) dans `superview-core`. Les « écarts voulus avec Go » écrits dans les en-têtes de modules Rust deviennent de l'histoire : on n'« aligne » plus rien sur Go. `go.sum`, `govulncheck` et `golangci-lint` n'existent plus ; `cargo audit` les remplace pour les dépendances. Pour relire le comportement Go : `git show v0.2.9:<chemin>`.
**Amendée par [DEC-55](#dec-55)** : les journaux ne sont toujours pas réécrits, mais ils sont bandeautés et les corrections de l'ère Go archivées.
**À rouvrir si** — Jamais vers une seconde chaîne d'outils dans le même dépôt ; si une régression exige de comparer au Go, on ouvre le tag `v0.2.9` dans un répertoire jetable, pas le dépôt courant.

### DEC-50
**Le profil release est `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"` ; pas de `panic = "abort"`.**
2026-10-20 · Acceptée · session 2 de la 14ᵉ passe (F-02)

**Contexte** — Sans `[profile.release]`, le binaire Linux pesait 28,9 Mo, symboles compris ; l'installateur et l'AppImage portaient ce poids pour rien.
**Décision** — Les trois réglages ci-dessus : 18,4 Mo mesurés (Linux). `panic = "abort"` est refusé : `app.rs` transforme la panique d'un fil de travail en erreur affichée (`catch_unwind`), et l'abandon fermerait la fenêtre sans un mot. Le marqueur `SUPERVIEW-BUILD` (une chaîne de `.rodata`) et le `DT_RPATH` de `build.rs` survivent à `strip`.
**Conséquences** — Les jobs de build de la release sont plus longs (2 min 20 s ici, contre une compilation courte avant) ; noté dans `RELEASING.md`. `release.yml` n'est pas modifié.
**À rouvrir si** — Le temps de build devient un frein, ou si les panics des fils sont un jour traitées autrement que par `catch_unwind` (alors `panic = "abort"` redevient possible).

### DEC-51
**La fenêtre est redimensionnable : 980×470 à l'ouverture, 760×440 au minimum, pas de maximum.**
2026-10-20 · *Remplacée par [DEC-54](#dec-54)* · session 7 de la 14ᵉ passe (F-27) · remplace la taille fixe de DEC-32

**Contexte** — Fixe à 980×470 px logiques, la fenêtre dépassait un 1366×768 à 150 % (910×512) : coupée, non redimensionnable.
**Décision** — `preferred-width/height` 980×470, `min-width/height` 760×440, plus de maximum. `the_window_fits_its_smallest_size` vérifie la barre d'outils et la dernière ligne au minimum ; les tests de la taille d'ouverture restent.
**Conséquences** — Le test tourne sur le backend de test, sans police : le retour à la ligne des textes n'y est pas mesuré, et le minimum n'a pas été vu sous xvfb.
**À rouvrir si** — Une capture réelle à 760×440 montre un texte coupé (remonter le minimum), ou si un widget ajouté ne tient plus dans ce minimum.

### DEC-52
**L'entrée est lue avec `-f mov`, par ffprobe et par ffmpeg : un fichier qui n'est pas du MP4 est refusé, quelle que soit son extension.**
2026-10-20 · Acceptée · session 9 de la 14ᵉ passe (Q-04)

**Contexte** — Sans `-f`, ffmpeg choisit le démuxeur d'après le contenu. Mesuré (ffmpeg 8.0.1) : un « .mp4 » contenant `ffconcat version 1.0` + `file clip.mp4` est lu comme un script `concat` et mène à un autre fichier local (chemin relatif ; les chemins absolus et `..` sont refusés par le mode sûr). Une playlist HLS portant l'extension `.mp4` est déjà refusée par ffmpeg 8 (« Not detecting m3u8/hls with non standard extension »). Avec `-f mov`, le script est refusé (« moov atom not found ») et un vrai MP4 passe.
**Décision** — `"-f", "mov"` juste avant le `-i` du fichier source dans `check_video_command` (`video.rs`) et dans `build_encode_base_args` (`encode/args.rs`), pas devant les cartes PGM, qui sont les nôtres. Le démuxeur `mov` couvre MP4, MOV et 3GP. La fixture gelée `encode_args.json` n'est **pas** régénérée : `parity_encode_args.rs` attend la fixture plus `-f mov` avant le premier `-i` (`with_forced_demuxer`), de sorte que l'écart avec Go est écrit dans le code et que les 237 cas restent comparés champ par champ. La comparaison sonde/conversion de `probe/tests.rs` range `-f` parmi les options propres à la conversion.
**Conséquences** — Un fichier qui n'est pas du MP4/MOV (MKV renommé, par exemple) échoue à la vérification avec l'erreur d'ffprobe, au lieu d'être converti : c'est la règle « MP4 en entrée » appliquée au contenu. `check_video_refuses_a_concat_script_named_mp4` est rouge sans l'option (mesuré). Le message d'ffprobe change pour un fichier invalide, pas la forme « ffprobe failed, output: … ».
**À rouvrir si** — Un MP4 ou MOV légitime est refusé par `-f mov` (un conteneur que le démuxeur `mov` ne lit pas), ou si le produit accepte un jour d'autres conteneurs.

### DEC-53
**Une durée qui n'est pas un nombre fini est refusée : `VideoSpecs::validate` dit « invalid or missing duration ».**
2026-10-20 · Acceptée · session 9 de la 14ᵉ passe (Q-05)

**Contexte** — `parse_go_float` accepte `inf` et `nan` comme `strconv.ParseFloat`, et le `duration_float <= 0.0` hérité de Go ne les attrape pas : `inf` faisait refuser la sortie pour « needs about 8796093022207 MB », `NaN` faisait passer le contrôle d'espace disque à zéro.
**Décision** — `!duration_float.is_finite() || duration_float <= 0.0`. Le message, déjà visible par l'utilisateur, ne change pas. `parse_go_float` et `parse_frame_rate` ne changent pas (le premier sert aussi à la progression ; la cadence n'est lue nulle part ensuite). Aucune fixture gelée n'est touchée : `specs.json` n'a aucun cas `inf` ou `nan`.
**Conséquences** — `validate_duration_limits` attendait « NaN passe, comme en Go » : **test modifié par décision**. `a_duration_of_inf_or_nan_in_ffprobes_json_is_refused` le prouve de bout en bout sur le JSON d'ffprobe.
**À rouvrir si** — ffprobe se met à rendre légitimement `inf` pour un flux sans fin (un MP4 n'en a pas).

### DEC-54
**La fenêtre reprend sa taille fixe : 980×470, non redimensionnable.**
2026-10-21 · *Remplacée par [DEC-56](#dec-56)* · à la demande de l'utilisateur, après essai de la fenêtre redimensionnable · remplace DEC-51

**Contexte** — DEC-51 (#150) avait rendu la fenêtre redimensionnable (980×470 à l'ouverture, 760×440 au minimum) pour qu'elle tienne sur un 1366×768 à 150 % (910×512 logiques). À l'essai, l'utilisateur ne veut pas d'une fenêtre qui bouge : il préfère la taille fixe d'avant.
**Décision** — `min-width` = `max-width` = 980 px et `min-height` = `max-height` = 470 px dans `main.slint`, comme avant #150 (DEC-32). Le garde `the_window_has_one_size_so_it_cannot_be_resized` (`repo_guards.rs`) lit le fichier `.slint` et exige min = max sur les deux axes, car le backend de test n'applique pas les contraintes (un `set_size` y passe) : rouge avec la fenêtre de DEC-51, vert avec celle-ci (mesuré). `the_window_fits_its_smallest_size` est retiré, il n'a plus de minimum à vérifier ; `the_window_has_the_designed_size` reste. Le reste de #150 (Échap et Entrée dans les dialogues, « Cancelling... », menu de qualité) est conservé.
**Conséquences** — Retour du défaut que DEC-51 corrigeait : sur un écran de 910×512 logiques ou moins (1366×768 à 150 %), la fenêtre est coupée et ne peut pas être réduite. Accepté par l'utilisateur.
**À rouvrir si** — Un utilisateur signale une fenêtre coupée sur son écran : la solution est alors un minimum plus petit, pas un retour à la taille libre sans réflexion.

### DEC-55
**Les journaux gardent Go dans leur texte : on archive, on ne réécrit pas.**
2026-10-21 · Acceptée · plan « traces de Go », PR 5 · amende DEC-49

**Contexte** — DEC-49 retirait Go du code et de la prose vivante, et laissait les journaux (`LECONS.md`, `ANALYSE.md`, `CORRECTIONS.md`, `DECISIONS.md`, `CONTRATS.md`) « pas réécrits ». Il en restait plus de 2 000 lignes d'histoire mêlées au vivant (surtout les corrections d'avant le 2026-10-17), des bandeaux qui disaient que `internal/common/` existait encore, 22 liens vers des fichiers supprimés et sept lignes de `PIEGES.md` sur des outils disparus.
**Décision** — Un bandeau « Go et Fyne » en tête de chaque journal : les noms y décrivent le code retiré, relisible par `git show v0.2.9:<chemin>`. Les liens vers `cmd/` et `internal/` deviennent du texte (`chemin`, v0.2.9). Les sept pièges Go de `PIEGES.md` passent dans une section « Historique (Go) ». Les corrections antérieures au 2026-10-17 quittent `CORRECTIONS.md` pour `docs/archives/CORRECTIONS-ere-go.md`, texte inchangé, avec un titre-renvoi laissé en place (même procédé que DEC-40) pour que les renvois « CORRECTIONS … » restent vrais.
**Conséquences** — `CORRECTIONS.md` ne tient plus que l'ère Rust. Un nom de fonction Go dans un journal n'est ni une erreur ni une dette : ne pas le « corriger ». Le contenu des entrées n'est jamais réécrit, seulement déplacé ou précédé d'un bandeau.
**À rouvrir si** — Les journaux deviennent illisibles sans le contexte Go (alors les condenser, pas les réécrire), ou si `v0.2.9` cesse d'être joignable (tag supprimé).

### DEC-56
**La fenêtre est fixe, 860×332, avec un panneau de conversion à droite.**
2026-10-21 · *Remplacée par [DEC-59](#dec-59)* · étude d'interface, scénario C variante « Resserrée », choisie par l'utilisateur · remplace DEC-54

**Contexte** — La fenêtre de 980×470 laissait beaucoup d'espace vide : menus étirés sur toute la colonne, valeurs loin de leurs libellés, Status, Results et Hardware empilés au-dessus de la barre. L'utilisateur a comparé des maquettes et retenu deux colonnes : les réglages à gauche, un panneau de conversion à droite.
**Décision** — `min-` = `max-` à 860×332 px logiques dans `main.slint`. À gauche, Input file, Output file, Quality profile, Video codec et Input format, puis la ligne Hardware sous Input format ; les deux menus ont une largeur fixe (170 et 300 px), pas celle de la colonne. À droite, un panneau de 290 px : « Progress », la barre, puis Status et Results dessous. Les textes ne changent pas. Le garde `the_window_has_one_size_so_it_cannot_be_resized` lit toujours `min = max` ; `the_conversion_panel_sits_beside_the_settings` (`window_tests.rs`) fige la disposition (rouge avec l'ancienne mise en page : un menu dépassait le panneau, mesuré).
**Conséquences** — La fenêtre est plus petite que 980×470 et tient donc sur plus d'écrans, mais reste non redimensionnable (DEC-54). La hauteur de 332 px laisse trois lignes à Hardware (un refus de sonde peut être long) ; Status et Results, mesurés sur capture avec leurs textes les plus longs, tiennent dans le panneau. Le backend de test ne mesure pas la vraie police : l'ajustement se juge sur capture. Une carte de popup est plus haute que la fenêtre dès qu'elle dépasse ~330 px : le Diagnostic dispose d'une liste d'environ 190 px, et le popup Error, dont le message peut reprendre le stderr d'ffmpeg, déborderait avant sans défilement (défaut déjà présent à 470 px, aggravé ; à traiter à part).
**À rouvrir si** — Un texte coupé ou un écran trop petit est signalé : agrandir ou rendre la fenêtre redimensionnable avec un minimum, pas retirer le panneau.

### DEC-57
**Un menu Theme (Light / Dark, sombre par défaut) et un accent cyan portés par un global `Theme`.**
2026-10-21 · Acceptée · refonte de l'interface, PR 2 · remplace le thème sombre forcé de DEC-32

**Contexte** — L'utilisateur a choisi la palette cyan sur la planche des maquettes, avec un thème clair et un thème sombre. `Palette` de Slint est en lecture seule (sauf `color-scheme`) : l'accent ne s'y règle pas.
**Décision** — `ui/theme.slint` : un global `Theme` (`name` = « Light » ou « Dark », `dark` en dérive, puis fond, panneau, surface, bordure, texte, texte atténué, accent, couleur sur accent, rouge « en cours », vert « terminé », erreur, voile des popups), avec les deux jeux de la planche. `ui/components.slint` : `AppButton` (normal ou primaire, rôle accessible bouton, Entrée et Espace, anneau de focus) et `Card`, qui remplacent `Button` et le rectangle du panneau. `main.slint` recopie le choix dans `Theme.name` et dans `Palette.color-scheme` (les menus, champs et barres de défilement de Slint suivent le thème, sans l'accent) ; le menu **Theme** est dans l'en-tête. La préférence `theme` de `preferences.json` vaut « Dark » par défaut, une valeur inconnue y revient (`sanitized`) et un ancien fichier sans la clé reste lisible. Le bouton accessible d'`AppButton` ne regarde pas `enabled`, comme le `Button` de Slint (des scénarios de `app_tests.rs` appuient sur un bouton derrière un popup par cette voie) ; la souris et le clavier, eux, le respectent.
**Conséquences** — Les widgets que Slint dessine (ComboBox, CheckBox, ProgressIndicator) gardent son style et non l'accent cyan : le décalage est visible sur capture. La barre est reprise en PR 3 ; les menus le seront dans une PR dédiée si le décalage gêne. Le `changed theme` de la fenêtre s'exécute au tour suivant de la boucle d'événements : un thème écrit par l'hôte n'est donc visible qu'après ce tour (le test le fait avancer par `drive(async {})`).
**À rouvrir si** — Le décalage des widgets Slint gêne, ou un thème de plus est demandé (alors une liste de jeux, pas un troisième booléen).

### DEC-58
**La barre de progression est rouge en cours, verte à la fin, grise si arrêtée, et glisse vers sa valeur.**
2026-10-21 · Acceptée · refonte de l'interface, PR 3 · demande de l'utilisateur

**Contexte** — Le `ProgressIndicator` de Slint a un seul aspect et avance par à-coups à chaque rapport d'ffmpeg.
**Décision** — `BarState { Idle, Running, Done, Stopped }` dans `ViewModel` (`state.rs`), fixé là où l'état change déjà : `set_encoding(true)` → Running, succès → Done, annulation, échec et demande d'annulation → Stopped, nouveau fichier d'entrée → Idle (et remise à zéro ; le choix d'un fichier de sortie ne touche pas la barre). `app.rs` le pousse à la fenêtre (`bar-state`). `ProgressBar` (`components.slint`) remplace `ProgressIndicator` : couleur d'accent au repos, `Theme.running` en cours, `Theme.done` à la fin, `Theme.text-muted` si arrêtée ; la largeur du remplissage glisse en 400 ms (`ease-out`) seulement en Running et au-dessus de 0, donc la remise à zéro d'un départ ou l'arrivée à 100 % ne s'animent pas. Les textes ne changent pas.
**Conséquences** — Le lissage ne se prouve pas par le backend de test (pas d'horloge d'animation utilisée ici) : il se juge à la main sur une vraie conversion. Les couleurs et chaque transition sont testées. La barre garde le rôle `progress-indicator` et sa valeur (0 à 1), comme celle de Slint ; un test les lit.
**À rouvrir si** — Le rouge « en cours » est pris pour une erreur par des utilisateurs (alors accent en cours, vert à la fin), ou si une barre verte et pleine après le choix d'un autre fichier de sortie gêne (alors remise à zéro aussi dans `apply_chosen_output`).

### DEC-59
**La fenêtre est fixe, 1000×520 : le fichier à gauche, les réglages et la conversion à droite.**
2026-10-21 · *Remplacée par [DEC-61](#dec-61)* · refonte de l'interface, PR 4 · remplace DEC-56

**Contexte** — Avec la palette cyan (DEC-57) et la barre d'état (DEC-58), l'utilisateur a choisi sur la planche une fenêtre d'environ 1000 px, avec une carte de fichier et deux fonctions à venir (vignette, aperçu).
**Décision** — `min-` = `max-` à 1000×520 px logiques dans `main.slint`. Colonne gauche (600 px) : une carte du fichier (cadre de vignette vide, « Input file », le nom ; un clic dessus choisit l'entrée, sous le libellé accessible « Input video » pour que « Choose input file » reste unique), puis « Output file ». Colonne droite : Quality profile en `SegmentedControl` (rôle radio, modèle `prefs::QUALITY_PROFILES`), Video codec (le `ComboBox` de Slint, 300 px), Input format en `Switch` (rôle interrupteur, libellé inchangé), la ligne Hardware, puis la carte de conversion. La barre d'actions reste en haut, centrée. Le popup Error met son texte dans un `ScrollView` (300 px au plus) : un message de plusieurs dizaines de lignes (stderr d'ffmpeg) ne repousse plus OK hors de la fenêtre. Les textes ne changent pas.
**Conséquences** — La colonne gauche a de la place libre sous « Output file » jusqu'à la PR 6 (aperçu) ; la hauteur de 520 px est une estimation à ajuster alors. Le menu de qualité n'est plus un `ComboBox` : les tests le lisent par ses segments (`Fast`, `Balanced`) et `selectors()` ne rend plus que le menu du codec. Les segments et l'interrupteur se prennent par Tab (un anneau montre le focus) et répondent à Entrée et Espace ; Entrée et Espace ne sont pas testés (le backend de test n'expose pas le focus). Leur action d'accessibilité respecte `enabled`, contrairement à celle d'`AppButton` (DEC-57), et un clic sur le segment déjà choisi ne signale rien.
**À rouvrir si** — Un écran trop petit pour 1000×520 est signalé (alors fenêtre redimensionnable avec minimum, pas retour à 860), ou si le test clavier devient possible (alors l'écrire).

### DEC-60
**La vignette de l'entrée est une image brute décodée par ffmpeg dans un dossier temporaire, et son absence n'est jamais une erreur.**
2026-10-21 · Acceptée · refonte de l'interface, PR 5

**Contexte** — La carte du fichier (DEC-59) a un cadre pour une image de la vidéo. Rien dans le cœur n'extrayait d'image, et `process.rs` ne capture que du texte.
**Décision** — `superview-core::thumbnail` : `extract_thumbnail(resolver, fichier, secondes, annulation)` lance ffmpeg avec `-f mov` avant l'entrée (DEC-52), `-ss` sur l'entrée, `-frames:v 1`, un `scale` + `pad` qui tient dans 192×144 (une vidéo plus large reçoit des bandes, jamais étirée) et `-f rawvideo -pix_fmt rgba` vers un fichier d'un `TempDir` privé, relu puis supprimé au retour quelle que soit l'issue ; à la fermeture de la fenêtre, le fil est attendu 2 s au plus après la levée de `leaving` (qui tue ffmpeg en une dizaine de ms), pour ne laisser ni processus ni dossier. Pas de décodeur d'image à ajouter. Le processus passe par `run_with_deadline` : 15 s au plus, annulation par le drapeau `leaving` de la fenêtre. Un fichier de taille inattendue (un `-ss` après la fin décode zéro image et sort proprement) est une erreur. Le moteur de la fenêtre choisit l'instant : un dixième de la durée, 5 s au plus. L'application décode sur un fil `thumbnail` après chaque entrée acceptée, vide le cadre d'abord, et jette l'image si une autre entrée a été choisie entre-temps (compteur `thumbnail_gen`). Elle affiche aussi sous le nom une ligne « codec · largeur×hauteur · durée » (`format::input_specs`). Un échec de décodage est journalisé au niveau debug : la carte reste vide, sans popup.
**Conséquences** — Un fichier refusé laisse l'image et la ligne du fichier précédent. L'image d'une vidéo en 16:9 a des bandes noires. Les fils des images périmées ne sont pas annulés un à un (seul `leaving` les arrête) : plusieurs choix rapprochés font tourner plusieurs ffmpeg, bornés à 15 s chacun, ce que le sélecteur modal rend rare. Le rendu de l'image dans la vraie fenêtre n'a pas de capture : le sélecteur de fichiers ne se pilote pas sous Xvfb ; le test de fenêtre lit les pixels de l'`Image` posée.
**À rouvrir si** — Une vidéo où la première image à 10 % est noire ou un générique gêne (alors choisir une image plus loin ou la plus nette), ou si le décodage d'un fichier sur réseau gêne l'ouverture (le délai de 15 s serait à baisser).

### DEC-61
**La fenêtre suit la maquette cyan validée : boutons en contexte, carte de conversion à pastille, colonne droite sombre, 1000×570.**
2026-10-21 · Acceptée · refonte de l'interface, PR d'alignement · remplace la disposition de DEC-59

**Contexte** — L'utilisateur a essayé la fenêtre de DEC-59 et signalé qu'elle s'écartait de la maquette validée (planche cyan) : la barre de six boutons en haut, que le plan avait gardée à tort, n'existe pas dans la maquette.
**Décision** — Dans `main.slint` : plus de titre « Superview » ni de barre de boutons ; l'en-tête ne garde que la phrase et le menu Theme (le nom est dans la barre de la fenêtre). Colonne gauche (600 px) : « Input file » et une carte au bord et au fond teintés d'accent, avec la vignette (96×72), le nom, la ligne de spécifications et **Choose input file** ; puis « Output file », un champ et **Choose output file**. Colonne droite (400 px) sur le fond sombre, séparée par un trait : qualité, codec, interrupteur, ligne Hardware, puis la carte de conversion (pastille d'état *Ready*, *Converting*, *Completed* ou *Stopped* de la couleur de la barre, pourcentage en grand, barre, Status, Results, **Start transformation** et **Cancel**), enfin **Diagnostic** et **Quit**. Le fond de la fenêtre est `Theme.panel` ; les boutons ordinaires ont le fond `Theme.surface`. La barre garde son rôle accessible, désormais avec l'étiquette « Progress » (le texte « Progress » a disparu). La carte du fichier n'est plus cliquable (le bouton qu'elle contient suffit) : l'étiquette « Input video » est retirée. Fenêtre 1000×570, avec les espacements de la maquette (étiquette à 6 px de son champ, champs à 12 px, carte de 14 px de marge, pourcentage de 44 px de haut) ; elle était de 620 px quand les espacements étaient trop larges, ce qui laissait un grand vide sous Hardware. Pour que la colonne tienne dans le pire cas sans vide, les textes variables sont plafonnés : Hardware et Results à deux lignes, Status à une, tronqués par « … » (une raison de sonde fait jusqu'à 300 caractères, deux dans le cas VAAPI « qualité constante seule », DEC-48 ; le Diagnostic et le journal les donnent en entier ; test `the_right_column_fits_with_the_longest_texts`). L'étiquette « Input format » est retirée : la maquette n'en a pas, l'interrupteur porte son texte.
**Conséquences** — Les tests de la barre d'outils (`the_toolbar_*`) sont remplacés par `the_buttons_and_the_settings_sit_where_the_mockup_puts_them`, qui fige la place de chaque bouton. La pastille dit *Stopped* après une annulation comme après un échec (le Status dit lequel) et *Ready* tant qu'aucun fichier n'est choisi. Un bouton désactivé n'a pas de fond (un bouton actif a celui de la surface) : sur la colonne claire, un fond de surface ne se distingue pas de celui d'un bouton actif. Différences assumées avec la maquette : pas de « Drop a video here » (le glisser-déposer est refusé), le bord de la carte est plein et non en pointillés (Slint ne trace pas de pointillés), les étiquettes ne sont pas en capitales (les textes de l'interface ne changent pas), les polices Sora et IBM Plex viennent à la PR 7 et l'aperçu de la distorsion à la PR 6. La colonne gauche est vide sous « Output file » jusqu'à l'aperçu.
**À rouvrir si** — Un écran trop petit pour 1000×570 est signalé, ou si la maquette change.

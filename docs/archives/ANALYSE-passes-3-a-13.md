# Superview — Analyse : passes 3 à 13 (archive)

> **Archive, pas documentation de référence.** Ces onze passes (2026-09-04 → 2026-10-14)
> sont sorties d'[ANALYSE.md](../ANALYSE.md) par la PR « docs du gabarit » : texte
> **inchangé**, seule la place a changé. Le tableau d'état (§ 4) et les questions
> ouvertes (§ 5) restent dans ANALYSE.md, qui est le seul endroit qui tienne l'état
> d'avancement. Les renvois « § 3bis … § 3duodecies » des autres documents désignent
> les sections ci-dessous. Chercher un constat : `grep -rn "U-03" docs/`.
>
> Les chemins `common/` et les fichiers `gui_*.go` de la racine y sont ceux d'avant
> DEC-39 : aujourd'hui `internal/common/` et `cmd/superview/`.

## 3bis. Troisième passe d'analyse (2026-09-04) — axes non couverts

Passe menée **empiriquement**, à `e3269e7` : corpus de fichiers difficiles fabriqué avec FFmpeg, mesures de
mémoire et de débit, tests de fuite de processus, et — fait nouveau — validation du chemin
d'accélération matérielle sur une **NVIDIA RTX 4070 avec NVENC fonctionnel**.

Convention : **N-xx** pour les constats de cette passe.

> **Périmètre produit** : Superview ne traite que des fichiers **MP4**, en entrée comme en
> sortie (cible : enregistrements GoPro). Les constats ci-dessous sont calibrés sur ce périmètre.
> Ceux qui portaient sur d'autres conteneurs ont été reclassés en conséquence.

### Ce qui s'est révélé sain (hypothèses de départ invalidées)

Ces points étaient sur ma liste de suspects et ne posent **aucun problème** :

| Hypothèse | Mesure |
| --- | --- |
| Consommation mémoire sur 4K | **Constante, ~2 Mo alloués** quelle que soit la résolution (640×480 comme 4064×3048). L'optimisation O-03 a supprimé le problème avant qu'il n'existe. Heap stable à 7,3 Mo. |
| Chemin d'accélération matérielle | **Fonctionne réellement.** Plan annoncé (`h264_nvenc + CUDA decode`) et exécution effective concordent, résumé affiché correct. |
| Formats exotiques | HDR/10 bits, dimensions impaires, absence d'audio, clip de 0,2 s, pistes audio multiples : **tous traités sans erreur**. |
| Entrée = sortie | **Aucune perte de données** : FFmpeg refuse lui-même (code 234), le fichier source reste intact. |

### N-01 ✅ — ~~La GUI propose dix formats alors que seul le MP4 est supporté~~ — **CORRIGÉ**

`gui_main.go`, `gui_native_dialog_linux.go`, `gui_native_dialog_windows.go`

**Contrainte produit (confirmée par l'utilisateur le 2026-09-04) : Superview ne traite que du
MP4, en entrée comme en sortie.** La sortie l'applique déjà (`ensureMP4Extension`), mais les
trois sélecteurs de fichiers — Fyne, zenity/kdialog, PowerShell — proposent toujours **dix
extensions**.

Vérifié en exécutant le vrai `CheckVideo` sur un fichier de chaque type : **5 des 10 échouent**,
faute de `duration` ou de `bit_rate` au niveau du flux vidéo.

| Accepté | Rejeté |
| --- | --- |
| `.mp4`, `.mov`, `.m4v`, `.avi` | `.mkv`, `.webm`, `.flv` *(durée absente)* · `.mpg`/`.mpeg`, `.wmv` *(débit absent)* |

Les messages sont par ailleurs incompréhensibles :
`invalid duration value '': strconv.ParseFloat: parsing "": invalid syntax`.

*Correction* : **restreindre les filtres au seul `.mp4`**, dans les trois sélecteurs. Cela aligne
l'entrée sur la sortie et sur le périmètre réel, et fait disparaître le problème plutôt que de
le contourner.

> **Deux recalibrages successifs de ce constat.** Je l'avais d'abord classé 🔴 sur le seul cas
> MKV. L'utilisateur a signalé que l'application ne recevrait jamais de MKV : sévérité revue à
> 🟠, mais en vérifiant plus largement le périmètre s'est révélé plus étendu (5 formats, pas 1).
> Il a ensuite précisé que **seul le MP4 est supporté**, ce qui tranche la correction : restreindre,
> et non ajouter un repli sur les métadonnées conteneur comme je le proposais. Voir [LECONS.md](../LECONS.md) L-26.

### N-02 ✅ — ~~Chaque annulation laisse un processus zombie~~ — **CORRIGÉ**

`common/common.go` (`EncodeVideo`, branche `<-cancel`)

```go
case <-cancel:
    if cmd.Process != nil { _ = cmd.Process.Kill() }
    <-readDone
    return errors.New("encoding interrupted by user")   // cmd.Wait() jamais appelé
```

Sans `Wait()`, le processus tué n'est jamais moissonné et la goroutine interne que `os/exec`
crée pour recopier stderr n'est jamais libérée.

**Mesuré : 5 annulations → 5 zombies.** Correctif prouvé dans les deux sens — l'ajout de
`_ = cmd.Wait()` ramène le compte à **0**. Dans une GUI où l'utilisateur peut annuler
plusieurs fois, ils s'accumulent jusqu'à la fermeture de l'application.

### N-03 ✅ — ~~Les sources 10 bits sont ramenées à 8 bits sans avertissement~~ — **CORRIGÉ**

`common/common.go` — `"-filter_complex", "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p"`

Le format de sortie est codé en dur. **Mesuré** : une entrée `yuv420p10le` ressort en
`yuv420p`. Les GoPro récentes (HERO 10 et suivantes) enregistrent en 10 bits, et c'est
précisément le public visé. La perte est silencieuse.

Le filtre `remap` impose une conversion, mais elle pourrait viser `yuv444p10le,yuv420p10le`
quand la source est en 10 bits.

> ✅ **Correctif écrit et vérifié le 2026-09-04**, commun à N-03, N-04 et N-05 : voir
> § 3ter, « N-03 / N-04 / N-05 — un correctif unique, vérifié ».

### N-04 ✅ — ~~Les pistes audio supplémentaires sont supprimées~~ — **CORRIGÉ**

**Mesuré** : 2 pistes AAC en entrée, **1 seule** en sortie. Faute de `-map`, FFmpeg applique sa
sélection automatique et ne retient qu'un flux audio par type. Une caméra enregistrant plusieurs
pistes, ou un fichier post-produit, y perd du contenu sans message.

> ✅ Correctif commun à N-03/N-04/N-05, vérifié : voir
> § 3ter, « N-03 / N-04 / N-05 — un correctif unique, vérifié ».

### N-05 ✅ — ~~La date de prise de vue est perdue~~ — **CORRIGÉ**

**Mesuré** : `creation_time=2026-01-15T10:30:00Z` présent en entrée, **absent** en sortie (le
tag `comment`, lui, survit). Avec trois entrées dans le `-filter_complex`, FFmpeg ne sait pas de
laquelle hériter les métadonnées globales. `-map_metadata 0` réglerait le cas. Pour de la vidéo
d'action-cam, trier une bibliothèque par date devient impossible.

> ✅ Correctif commun à N-03/N-04/N-05, vérifié : voir
> § 3ter, « N-03 / N-04 / N-05 — un correctif unique, vérifié ».

### N-06 ✅ — ~~Le débit demandé n'est pas borné~~ — **CORRIGÉ**

`-b:v` est posé sans `-maxrate` ni `-bufsize`, c'est donc une cible **moyenne** sans limite
haute. Mesuré sur contenu incompressible (bruit), consigne 40 Mbps :

| Encodeur | Débit obtenu | Écart |
| --- | --- | --- |
| `libx264` | 62,5 Mbps | **+56 %** |
| `h264_nvenc` | 45,0 Mbps | +12 % |
| `hevc_nvenc` | 46,4 Mbps | +16 % |

> **Correction d'une conclusion hâtive.** Ma première mesure, faite sur une mire `testsrc`
> trivialement compressible, donnait 13 % / 5 % / 4 % de la consigne et m'a fait soupçonner que
> NVENC dégradait silencieusement la qualité. C'était **faux** : le contenu ne nécessitait
> simplement pas ces bits. Sur contenu exigeant, NVENC suit correctement la consigne et c'est
> `libx264` qui dérape. Une mesure sur le mauvais échantillon menait à la conclusion inverse.

### N-07 🔄 — ~~Le décodage matériel n'apporte que ~5 %~~ — **MESURE RÉVISÉE : ~10 %**

> **La mesure d'origine était faite sur le mauvais échantillon.** Elle sous-estimait le gain
> d'un facteur deux, et la recommandation qui en découlait s'en trouve inversée.
> Refaite le 2026-09-04 sur le même matériel (RTX 4070) et sur le code actuel.

#### Mesure d'origine (3ᵉ passe) — à ne plus utiliser

Source 2880×2160 de 15 s, `testsrc`, ~2 Mbps :

| Chemin | Durée |
| --- | --- |
| `h264_nvenc` + décodage CUDA | 4,31 s |
| `h264_nvenc` + décodage CPU | 4,53 s |
| `libx264` + décodage CPU | 7,54 s |

D'où « décodage matériel : ~5 %, encodage matériel : ×1,7 ».

#### Mesure révisée (2026-09-04)

Une mire `testsrc` à 2 Mbps se décode quasi gratuitement : il n'y a presque rien à déporter,
donc presque rien à gagner. Une GoPro 5,3K produit du **100-120 Mbps** de contenu détaillé.
Refait sur les deux échantillons, deux exécutions concordantes, meilleur temps retenu :

| Source | Décodage matériel | Encodage matériel |
| --- | --- | --- |
| Mire `testsrc`, 2 Mbps *(l'échantillon d'origine)* | +3,1 % | ×1,92 |
| **Type GoPro, 2880×2160 à 127 Mbps** | **+9,9 %** | **×3,18** |

Les deux chiffres du constat initial étaient bas : le décodage matériel vaut **le double** de ce
qui était consigné, et l'encodage matériel presque **le double** aussi.

#### Pourquoi le gain reste inférieur à ce qu'on attendrait

Le filtre `remap` tourne sur CPU, donc les trames décodées doivent redescendre en mémoire
système quoi qu'il arrive. Mesuré sur la même source de 127 Mbps, en transcodage **sans**
`remap` :

| Chemin | Durée | Gain |
| --- | --- | --- |
| Décodage GPU, trames gardées en VRAM | 2,34 s | +7 % |
| Décodage GPU, trames rapatriées en RAM — *ce que fait Superview* | 2,47 s | **+1 %** |
| Décodage CPU | 2,51 s | — |

Le rapatriement en RAM annule donc presque tout le gain… en transcodage simple. Avec `remap`,
le même décodage matériel rapporte pourtant **+9,9 %**.

L'explication la plus cohérente avec ces deux mesures : le gain ne vient pas du décodage plus
rapide mais du **CPU libéré pour le filtre**. En transcodage simple le CPU est largement
inoccupé — NVENC fait l'encodage — donc lui retirer le décodage ne change presque rien. Avec
`remap` le CPU est saturé, et chaque cycle rendu au filtre compte. *C'est une inférence à partir
de deux mesures, pas une propriété vérifiée directement.*

#### Arbitrage révisé — recommandation : **conserver**

- **Bénéfice** : ~10 % sur du contenu réel, pas 5 %.
- **Risque** : c'est le chemin tenté en premier, et son échec relance l'encodage. Mais l'échec
  courant — pilote absent, GPU occupé, format non supporté — survient à l'**initialisation** et
  coûte une fraction de seconde. Un échec tardif existe mais reste rare.

À 5 % pour un risque flou, l'abandon se défendait. À **10 % pour un risque essentiellement
limité à l'initialisation**, le conserver est le meilleur choix. Aucune modification de code
n'est donc nécessaire.

*Décision finale : à l'utilisateur.*

### N-08 ✅ — ~~Jusqu'à 146 Mo de fichiers temporaires, en RAM, sans vérification préalable~~ — **CORRIGÉ**

Empreinte mesurée des deux cartes PGM :

| Source | Sur disque |
| --- | --- |
| 640×480 | 2,9 Mo |
| 1440×1080 | 16,2 Mo |
| 2880×2160 (4K 4:3) | 71,6 Mo |
| 4064×3048 (GoPro 4:3, 12 Mpx) | **146,6 Mo** |

Sur cette machine — et par défaut sur beaucoup de distributions récentes — **`/tmp` est un
tmpfs**, donc de la mémoire vive : `tmpfs 16G /tmp`. Ces 146 Mo sont de la RAM, invisiblement.

`checkDiskSpaceHealth` existe et vérifie un seuil de 10 Go, mais n'est appelé **que** par le
bouton Diagnostic — jamais avant un encodage. `InitEncodingSession` crée son répertoire sans
rien vérifier. Sur une machine modeste au tmpfs réduit, l'échec surviendra en cours de route,
avec un message d'écriture peu parlant.

> ✅ **Le passage au PGM P5 binaire réduit ces 146 Mo à 66 Mo**, sortie identique au bit près.
> Mesure et vérification en § 3ter, constat P-08.

### N-09 ✅ — ~~Entrée identique à la sortie : message incompréhensible~~ — **CORRIGÉ**

Aucune garde dans le code. **Mesuré** : FFmpeg refuse de lui-même et **le fichier source reste
intact** — pas de perte de données. Mais l'utilisateur reçoit `ffmpeg failed: exit status 234`.
Une comparaison des chemins avant lancement produirait un message clair.

### N-10 ✅ — ~~Retour utilisateur pauvre pendant un encodage long~~ — **CORRIGÉ**

- Aucune estimation de temps restant ni de débit : une barre de pourcentage seule, pour une
  opération qui peut durer plusieurs minutes en 4K. Les métriques nécessaires (`EncodingSpeed`,
  temps écoulé) sont pourtant déjà calculées dans `metrics.go`.
  → Précisé en 4ᵉ passe : `EstimatedRemaining` est **déjà calculé et stocké** à chaque tick
  (P-07), et `EncodingSpeed` est **faux** car il suppose 30 fps en dur (P-06).
- Le chemin du fichier de journal n'est écrit **que dans le journal lui-même**
  (`gui_logger.Info("Superview starting", slog.String("log_file", logPath))`). Un utilisateur qui
  veut joindre ses logs à un rapport d'incident ne peut pas les trouver. Le dialogue Diagnostic
  serait l'endroit naturel pour l'afficher.

---

## 3ter. Quatrième passe d'analyse (2026-09-04) — relecture complète à `001d250`

Passe menée sur `master` @ `001d250`, soit **trois commits après** l'état analysé en § 3 et
§ 3bis (`e3269e7`). Relecture intégrale des 9 049 lignes du module, puis vérification empirique
avec le FFmpeg local (8.0.1-3ubuntu2) de chaque hypothèse retenue.

Convention : **P-xx** pour les constats de cette passe.

> **Suite donnée le 2026-09-04, sur décision de l'utilisateur.**
>
> *Premier lot* : le `-filter_complex` unique (N-03 + N-04 + N-05) et le passage des cartes de
> remappage en PGM P5 binaire (P-08). Le travail a fait apparaître **P-11**, corrigé dans la
> foulée.
>
> *Second lot* : **P-01 à P-07, P-09** et **N-06, N-08, N-09, N-10**.
>
> *Troisième lot* : **P-10**, la refactorisation de `main()`.
>
> *Quatrième lot* : **N-07**, dont la mesure a été refaite — le décodage matériel vaut ~10 % et
> non ~5 %, ce qui inverse l'arbitrage vers « conserver ». Aucun code touché.
>
> **Plus aucun constat ouvert, toutes passes confondues.**

### État vérifié du dépôt au moment de la passe

| Contrôle | Résultat |
| --- | --- |
| `go test -race -count=1 ./common` | ✅ `ok superview/common 3,9 s` |
| Couverture de `./common` | **81,5 %** (la moyenne module est plus basse, tirée vers le bas par `main`) |
| `gofmt -l .` | ✅ aucun fichier |
| `TODO` / `FIXME` dans le code | ✅ aucun (les deux occurrences sont des `context.TODO()`) |
| `go build ./...` | ❌ **non vérifiable** : le sysroot GUI de `/tmp` n'a pas survécu au redémarrage. `./common` compile. |

### N-03 / N-04 / N-05 — un correctif unique, vérifié

Les trois pertes de contenu relevées en § 3bis proviennent de la **même ligne**
([`common/common.go:685`](../../internal/common/common.go#L685)) et se corrigent ensemble. Reproduction et
correction validées ce jour :

| Entrée | Sortie actuelle | Avec le correctif |
| --- | --- | --- |
| `yuv420p10le` | `yuv420p` — 8 bits (N-03) | `yuv420p10le` |
| 2 pistes audio | 1 seule piste (N-04) | 2 pistes |
| `creation_time` présent | **absent** (N-05) | préservé |

Chaîne corrigée, exécutée et vérifiée :

```
-filter_complex "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p10le,format=yuv420p10le[v]"
-map "[v]" -map "0:a?" -map_metadata 0
```

Le `?` de `0:a?` est indispensable : sans lui, une source muette fait échouer FFmpeg. Le choix
entre la variante 8 bits et la variante 10 bits doit suivre le `pix_fmt` de la source, que
`CheckVideo` **ne demande pas encore** à ffprobe : il faut l'ajouter au `-show_entries`
([`common/common.go`, `CheckVideo`](../../internal/common/common.go)).

### P-01 ✅ — ~~La case « squeeze » n'est jamais réactivée après un encodage réussi~~ — **CORRIGÉ**

[`gui_main.go:469`](../../cmd/superview/gui_main.go#L469) (échec) contre
[`gui_main.go:487-492`](../../cmd/superview/gui_main.go#L487-L492) (succès)

Les deux branches de fin d'encodage réactivent les widgets une par une. La branche d'échec
appelle `squeezeCheck.Enable()` ; **la branche de succès l'a oublié**. Après un premier
encodage réussi, la case *Source already stretched (GoPro SuperView)* reste grisée jusqu'au
redémarrage de l'application — c'est-à-dire que la fonctionnalité exposée par C-04 devient
inatteignable dès le second fichier traité.

C'est une conséquence directe de P-10 : la transition d'état est dupliquée à la main dans deux
branches, sans rien pour garantir qu'elles restent symétriques.

### P-02 ✅ — ~~Course de données sur le canal d'annulation~~ — **CORRIGÉ**

`cancelEncoding` est écrit depuis le fil UI ([`gui_main.go:397`](../../cmd/superview/gui_main.go#L397),
`cancelEncoding = nil`) et lu depuis la goroutine d'encodage
([`gui_main.go:459`](../../cmd/superview/gui_main.go#L459)) :

```go
if err := common.PerformEncoding(&effectiveCfg, video.File, uri, handler, ffmpeg, cancelEncoding); err != nil {
```

C'est une course au sens du modèle mémoire Go. Conséquence fonctionnelle : une annulation
déclenchée avant que la goroutine n'ait lu la variable peut faire passer `nil` à
`PerformEncoding`, et le run devient **définitivement non annulable** — le bouton *Cancel*
sort immédiatement par la garde `cancelEncoding == nil`.

*Correction* : capturer le canal dans une variable locale **avant** de lancer la goroutine, et
passer cette locale.

### P-03 ✅ — ~~L'annulation ne court-circuite pas la cascade de repli~~ — **CORRIGÉ**

[`common/common.go:903-955`](../../internal/common/common.go#L903-L955)

`run` signale l'annulation par `errors.New("encoding interrupted by user")` — une erreur
indistinguable d'un échec d'encodeur. La cascade la traite donc comme un motif de repli et
relance FFmpeg : décodage CPU, puis récursion sur l'encodeur CPU équivalent, chacun avec sa
propre retentative audio. **Jusqu'à trois processus FFmpeg supplémentaires sont démarrés après
que l'utilisateur a demandé l'arrêt.**

Chaque relance ressort vite (le canal est déjà fermé, le `select` la coupe au démarrage), donc
il n'y a pas de blocage — mais il y a démarrage de processus et réécriture du fichier de sortie.

*Correction* : un type d'erreur de domaine (`ErrCancelled`, ou un `errCancelled` sentinelle)
testé avec `errors.Is` avant chaque repli, dans `runWithAudioFallback` comme dans `EncodeVideo`.

### P-04 ✅ — ~~Le fichier de sortie partiel survit à une annulation ou à un échec~~ — **CORRIGÉ**

Aucun chemin d'erreur de `PerformEncoding` ne supprime `outputFile`. FFmpeg tourne avec `-y` et
écrit directement à destination : après une annulation, l'utilisateur retrouve un `.mp4`
tronqué, d'apparence valide, à l'emplacement qu'il avait choisi.

Recoupe N-09 : là, FFmpeg se protégeait lui-même (entrée = sortie, code 234, source intacte).
Ici rien ne protège la sortie.

*Correction* : encoder vers un fichier temporaire voisin puis renommer en fin de course
(`os.Rename` est atomique sur le même système de fichiers), ou à défaut supprimer la sortie sur
les chemins d'annulation et d'échec.

### P-05 ✅ — ~~Les débits sont en bits/s, documentés en octets/s partout~~ — **CORRIGÉ**

Le code est **cohérent** : ffprobe renvoie `bit_rate` en bits/s, FFmpeg attend `-b:v` en bits/s,
et les valeurs par défaut sont correctes comme bits (209 715 200 → 209,7 Mbps, ce que dit bien
`Config.String()`). Mais le mot « bytes » est écrit partout :

| Emplacement | Texte |
| --- | --- |
| [`common/config.go:19-23`](../../internal/common/config.go#L19-L23) | `// Bitrate constraints in bytes/second`, `// 100k bytes/sec (~0.1 Mbps)` |
| [`common/config.go:293-295`](../../internal/common/config.go#L293-L295) | `"  Min Bitrate: %d bytes/sec (%.2f Mbps)"` |
| [`common/common.go:189`](../../internal/common/common.go#L189) | `// GetBitrate returns the desired output bitrate in bytes/second.` |
| [`common/common.go:618-621`](../../internal/common/common.go#L618-L621) | messages d'erreur **montrés à l'utilisateur** : `"bitrate %d is below minimum %d bytes/second"` |
| [`common/metrics.go:12`](../../internal/common/metrics.go#L12), `:26`, `:34` | `All bitrates in bytes/sec` |
| [`superview.yaml`](../../superview.yaml) | `# Bitrate constraints in bytes/second` |

Un utilisateur qui règle `max_bitrate` en croyant écrire des octets se trompe d'un facteur 8.
Correction purement textuelle, sans aucun changement de comportement — mais elle touche aussi
`common/common_test.go:29` et `common/config_test.go:26`.

### P-06 ✅ — ~~`EncodingSpeed` repose sur un 30 fps codé en dur~~ — **CORRIGÉ**

[`common/metrics.go:261`](../../internal/common/metrics.go#L261)

```go
totalFrames := m.InputDuration * 30 // Assuming ~30fps average
```

La cadence réelle n'est jamais lue. Sur du 120 fps GoPro — cas courant pour la cible produit —
la « vitesse d'encodage » rapportée dans le journal et dans les événements d'observabilité est
fausse d'un facteur 4. Une métrique dérivée d'une constante inventée induit en erreur plus
qu'une métrique absente.

`r_frame_rate` est disponible auprès de ffprobe et se récupère dans le même appel que le
`pix_fmt` du correctif N-03.

### P-07 ✅ — ~~`EstimatedRemaining` est calculé à chaque tick et jamais affiché~~ — **CORRIGÉ**

[`common/metrics.go:125`](../../internal/common/metrics.go#L125) calcule le temps restant à chaque mise à
jour de progression. Aucun appelant ne le lit. La GUI n'affiche qu'un pourcentage, pour une
opération qui dure plusieurs minutes en 4K.

Précision apportée à N-10 : la donnée n'est pas seulement « calculable », elle est **déjà
calculée et stockée**. Il ne manque qu'un accesseur et un `SetText` dans le rappel de
progression.

### P-08 ✅ — ~~Les cartes PGM peuvent passer en P5 binaire~~ — **CORRIGÉ**

`GeneratePGM` écrit du **P2 ASCII**. Le format **P5 binaire** 16 bits est accepté tel quel par
le filtre `remap`. Vérifié ce jour en produisant les deux jeux de cartes pour la même
transformation et en comparant les trames décodées :

```
out_p2  a63565f315931b2caad0b851765b313f0bcd86180f0c04707446e1b08df50487
out_p5  a63565f315931b2caad0b851765b313f0bcd86180f0c04707446e1b08df50487
```

**Même SHA-256** — le changement de format est neutre sur le résultat.

| Source GoPro 4064×3048 (12 Mpx) | Deux cartes |
| --- | --- |
| P2 ASCII (actuel) | **146,6 Mo** |
| P5 binaire | **66,1 Mo** (−55 %) |

Gain double : l'empreinte (en RAM quand `/tmp` est un tmpfs, cf. N-08) et le temps de
génération, puisqu'on supprime le formatage décimal de ~33 millions d'entiers.

À coupler avec N-08 : appeler `checkDiskSpaceHealth` — qui existe déjà et n'est utilisé que par
le bouton Diagnostic — **avant** `InitEncodingSession`, avec le besoin réel
(`outX × outY × 2 × 2` octets en P5) plutôt qu'un seuil fixe de 10 Go.

> ⚠️ Le PGM binaire est **gros-boutiste** par spécification. Une implémentation qui écrirait
> du petit-boutiste produirait des cartes silencieusement fausses, pas une erreur.

### P-09 ✅ — ~~Verrouillage trop large et coût par événement de progression~~ — **PARTIELLEMENT CORRIGÉ**

- [`common/observability.go:232`](../../internal/common/observability.go#L232) : `RecordEvent` prend
  `r.mu.Lock()` (verrou d'**écriture**) pour une simple itération en lecture sur `r.handlers`.
  `RLock` suffit, et c'est déjà ce que font `RecordProgress`, `RecordError` et
  `RecordCompletion` juste en dessous.
- Chaque ligne de progression FFmpeg — plusieurs par seconde — traverse
  `RecordEncodingProgress`, qui alloue une `map[string]interface{}`, formate une chaîne et
  prend deux verrous, pour finir le plus souvent ignorée par un handler réglé sur `info`.
  Un court-circuit sur `logger.Enabled(ctx, slog.LevelDebug)` supprime ce coût.
- [`common/common.go`](../../internal/common/common.go) : `-threads runtime.NumCPU()` est posé même quand
  l'encodeur est NVENC, AMF ou QSV, où l'option n'a pas de sens.

> ✅ **Corrigé le 2026-09-04** : `RecordEvent` prend un `RLock`, et `encoderThreadArgs` n'émet
> plus `-threads` pour un encodeur matériel.
>
> ⏸️ **Volontairement non fait** : le court-circuit d'allocation sur les événements de
> progression. Le coût réel — une `map` et deux verrous quelques fois par seconde — est
> négligeable devant un encodage vidéo, et l'optimisation risquait de faire disparaître des
> événements du journal. Optimiser ce qui ne coûte rien, au prix d'un comportement observable,
> n'est pas un gain.

### P-10 ✅ — ~~`main()` reste une fonction de 600 lignes : l'état de la GUI n'est pas testable~~ — **CORRIGÉ**

`gui_main.go` fait 798 lignes dont ~600 dans `main()`. Tout l'état applicatif — `video`,
`outputPath`, `encodingInProgress`, `cancelEncoding`, la liste des widgets à réactiver — vit en
variables capturées par closure. Rien de tout cela n'est atteignable depuis un test.

`gui_main_test.go` compte 14 tests, mais **tous** portent sur des fonctions pures
(`ensureMP4Extension`, `qualityProfileSettings`, `encoderOptionsFor`, `formatResultsPanel`…).
C'est exactement pour cela que **P-01 et P-02 ont survécu à T-01** : ils vivent dans la zone
qu'aucun test ne peut atteindre.

*Correction appliquée le 2026-09-04* : l'état est extrait dans un type `appState` porteur de
onze méthodes — `canStart`, `refreshStart`, `setInput`, `setOutput`, `setFFmpeg`,
`refreshHardwareStatus`, `setEncoding`, `beginEncoding`, `finishEncoding`, `requestCancel`,
`isEncoding` — **toutes couvertes à 100 %**.

Deux choix de conception portent la correction au-delà d'un simple déplacement :

- `beginEncoding()` **retourne** le canal d'annulation au lieu de laisser l'appelant le relire
  dans la structure. La goroutine d'encodage ne *peut plus* commettre P-02 : la course n'est pas
  corrigée, elle est rendue inexprimable.
- `requestCancel()` est le seul endroit autorisé à fermer le canal et le met à `nil` dans le
  même geste, donc l'appeler deux fois — bouton *Cancel* puis interception de fermeture — est
  inoffensif au lieu de paniquer.

La dernière séquence de `Disable` écrite à la main a disparu au passage : le chemin « ffmpeg
indisponible » en oubliait le sélecteur de codec, créé plus loin dans `main()`, qui restait donc
actif parmi quatre contrôles grisés.

| | Avant | Après |
| --- | --- | --- |
| `main()` | 600 lignes | **524 lignes** — le reste est de la construction de widgets et de la mise en page, ce qui est le rôle légitime de cette fonction |
| État applicatif | 6 variables capturées par closure | **1 type, 11 méthodes** |
| Tests atteignant l'état | 0 | **18 sous-tests**, chacun vérifié en contre-épreuve |
| Couverture du paquet `main` | 12,6 % | **21,8 %** |

> Ce que la refactorisation ne fait **pas** : `main()` reste longue. Ce n'est pas l'objet du
> constat — le défaut n'était pas la longueur mais le fait que les *décisions* soient
> inatteignables. La construction de widgets, elle, n'a rien à gagner à être découpée.

### P-11 ✅ — ~~`libx265` échoue systématiquement au-delà de 16 cœurs~~ — **DÉCOUVERT ET CORRIGÉ**

[`common/common.go`](../../internal/common/common.go) — `buildEncodeBaseArgs`, option `-threads`

Découvert en écrivant le test de non-régression de N-03 : il exige un encodeur HEVC, et
`libx265` refusait de démarrer sur cette machine.

`encoderThreads` vaut `runtime.NumCPU()` par défaut. FFmpeg fait correspondre `-threads` à
`--frame-threads` de x265, qui **refuse toute valeur supérieure à 16** :

```
x265 [error]: frameNumThreads (--frame-threads) must be [0 .. X265_MAX_FRAME_THREADS)
[libx265] Cannot open libx265 encoder.
```

Mesuré sur cette machine (24 cœurs logiques, FFmpeg 8.0.1) :

| `-threads` | Résultat |
| --- | --- |
| 4, 8, 16 | ✅ ok |
| 17, 24, 32 | ❌ **l'encodeur ne s'ouvre pas** |

Gravité 🔴 : ce n'est pas une dégradation, c'est un **échec dur**. Et comme `libx265` est un
encodeur CPU, `isHardwareEncoder` est faux, donc la cascade de repli d'`EncodeVideo` ne
s'applique pas — l'utilisateur reçoit directement un mur de texte x265. Sur toute machine à 17
cœurs logiques ou plus, **aucun encodage H.265 ne pouvait aboutir**. Les machines à 16 cœurs et
moins ne voient rien : c'est pourquoi ni la CI (runners GitHub à 4 cœurs) ni les passes
précédentes n'avaient pu l'observer.

*Correctif appliqué* : `clampEncoderThreads` plafonne `-threads` à 16 pour `libx265` seulement.
Les autres encodeurs se limitent eux-mêmes au lieu d'échouer, on ne les touche pas.
Tests : `TestClampEncoderThreads`, `TestBuildEncodeBaseArgs_Libx265ThreadsAreCapped`.

### P-12 🟠 — ~~Couture de 1 à 2,6 px au centre de l'image en mode squeeze~~ — **CORRIGÉ**

`common/common.go` — `GeneratePGM`, branche `squeeze`

Trouvé le 2026-09-04 en cherchant ce qui pouvait être vérifié du mode squeeze **sans fichier
GoPro réel**. Le test doré fige des octets, pas des propriétés : personne n'avait jamais vérifié
que la carte de remappage était géométriquement saine.

#### Le défaut

Le décalage squeeze est bâti sur deux termes qui, **en arithmétique exacte, sont rigoureusement
égaux au centre** — tous deux valent `7/32 × outX × inv` :

| | |
| --- | --- |
| terme 1 | `inv × (outX/16) × 7 / 2` = `7/32 × outX × inv` |
| terme 2 | `((inv/16)×7)² × (outX/7) × 16 / 2` = `7/32 × outX × inv` |

L'auteur a donc conçu une courbe continue, ancrée à zéro au centre comme aux bords. Mais
`outX/16` et `outX/7` étaient des **divisions entières** : la troncature laissait un résidu, que
le miroir de la moitié gauche transformait en saut au milieu de l'image.

| Largeur | Couture avant | Après |
| --- | --- | --- |
| 1440 px | 2,19 px | **0,00 px** |
| 1920 px | 0,88 px | 0,00 px |
| 2880 px | 1,31 px | 0,00 px |
| 4064 px | 1,75 px | 0,00 px |
| **multiple de 112** (16×7) | **0,00 px** | 0,00 px |

C'est la dernière ligne qui a identifié la cause : quand aucune troncature n'a lieu, la couture
disparaît. Le mode non-squeeze n'était pas touché — son décalage vaut `tx² × diff/2`, soit
exactement 0 en `tx=0`, donc le miroir y est continu.

#### Ce n'est pas un défaut du portage

Le code du dépôt amont `Niek/superview`, dont ce projet est un fork, porte la ligne
**identique caractère pour caractère**. Le défaut est hérité et présent en amont depuis
l'origine ; aucune issue ne l'y mentionne. L'implémentation Python de Banelle est inaccessible
(403 sur intofpv.com, archive web refusée), mais cela n'a plus d'importance pour la décision :
**l'intention mathématique se démontre depuis la formule elle-même**, et le code s'en écartait.

#### Correctif

Diviser en flottant. La forme de l'expression est conservée pour rester lisible face à
l'implémentation de référence, comme l'exige `.golangci.yml`. Amplitude maximale du décalage
inchangée à 0,1 % près (157,66 → 157,50 px) : c'est un raffinement sous-pixel, pas un changement
d'intention. Corrige aussi un décalage d'un pixel du centre en 1440 px de large.

Deux tests ajoutés, tous deux vérifiés en contre-épreuve — ils rougissent avec l'ancienne
formule (« saut de 4 px contre 1 px de part et d'autre », « centre échantillonne 721, attendu
720 ») :

- `TestGeneratePGM_SqueezeMapIsContinuous` — largeurs délibérément non multiples de 112
- `TestGeneratePGM_SqueezeMapStaysWellFormed` — monotonie, bornes, centre préservé

### P-13 🟡 — ~~Le libellé de la case squeeze promet du GoPro que le code ne tient pas~~ — **CORRIGÉ**

`gui_main.go`, `README.md`

La case s'intitulait *« Source already stretched (GoPro SuperView) »*. Or le README du dépôt
amont documente la même option ainsi :

> `-s, --squeeze    Squeeze 4:3 video stretched to 16:9 (e.g. **Caddx Tarsier 2.7k60**)`

et précise par ailleurs que l'algorithme **n'est pas une copie 1:1 de celui de GoPro**, seulement
une approximation. L'étiquette promettait donc une compatibilité GoPro que ni l'amont ni la
mesure ne soutiennent — et elle était visible par l'utilisateur, dans l'interface.

*Correction* : « Source already stretched to 16:9 (un-squeeze) ». Le README explique désormais
le périmètre réel (GoPro SuperView, Caddx Tarsier et similaires) et que la courbe est une
approximation de l'étirement inverse, pas la reproduction de l'algorithme d'une caméra.

### Priorités de cette passe

| Ordre | Constats | Justification |
| --- | --- | --- |
| 1 | **N-03 + N-04 + N-05** | Trois pertes de contenu produit, un seul correctif de quelques lignes, déjà validé bout en bout |
| 2 | **P-01**, **P-02** | Deux défauts visibles par l'utilisateur, correctifs triviaux |
| 3 | **P-03**, **P-04** | L'annulation relance FFmpeg et laisse un fichier corrompu à l'emplacement choisi par l'utilisateur |
| 4 | **P-05** | Correction textuelle sans risque, évite une mauvaise configuration d'un facteur 8 |
| 5 | **P-08** + **N-08**, **N-06** | Robustesse : empreinte mémoire et respect de la consigne de débit |
| 6 | **P-10** | Filet durable — à poser avant d'ajouter d'autres fonctionnalités à la GUI |
| 7 | **P-06**, **P-07** + **N-10**, **P-09**, **N-09** | Confort utilisateur et hygiène |
| — | **N-07** | Arbitrage produit : conserver ou non le décodage matériel, qui ne vaut que ~5 % |

---

## 3quater. Cinquième passe (2026-09-04) — vérification d'après-release

Passe courte, déclenchée par la publication de v0.2.0 : confronter aux faits ce que les
correctifs #32 et #33 avaient annoncé, au lieu de les croire sur parole. Marquage du § 3ter
(titre barré, statut en fin de titre).

### R-01 🟠 — ~~Le faux ffmpeg saute sur Windows, et avec lui le test qui épingle P-03~~ — **CORRIGÉ**

`common/common_test.go`

Depuis #32 le runner Windows installe ffmpeg et 258 tests s'y exécutent. Deux sautent encore,
dont `TestEncodeVideo_CancellationStopsTheFallbackCascade` :

```
common_test.go:1409: the stand-in ffmpeg relies on a POSIX shell
```

C'est le test qui épingle P-03 — une annulation ne doit lancer qu'un seul ffmpeg, pas dérouler
toute la cascade de repli. Il ne tournait donc que sur la moitié Linux, et sur le chemin dont le
comportement diffère le plus entre les deux systèmes : kill de processus et propagation d'erreur.

Le helper `fakeHangingFFmpeg` n'avait pourtant aucune raison de sauter. Au même moment, dans le
même fichier, `TestEncodeVideo_InterruptedByUser` installait le même stand-in sous forme de
`.bat` et **passait** sur Windows : `LookPath` honore `PATHEXT`, donc le nom nu « ffmpeg » se
résout vers `ffmpeg.bat`. Le mécanisme était démontré à quinze lignes de là.

*Correction* : `fakeHangingFFmpeg` écrit un `.bat` sur Windows, et `TestEncodeVideo_InterruptedByUser`
appelle le helper au lieu de dupliquer les deux scripts et le montage de `PATH` (−20 lignes).
L'autre saut restant, `/dev/zero unavailable` dans `security_test.go:257`, est légitime : Windows
n'a pas d'équivalent d'un fichier non régulier à ouvrir ainsi.

*Contre-épreuve* : stand-in renommé, les deux tests rougissent — mais **seulement dans un
environnement où ffmpeg est réellement introuvable**. La première contre-épreuve concluait à tort
que `TestEncodeVideo_InterruptedByUser` passait à vide, voir L-49.

### R-02 🟡 — ~~Le workflow de release ne produit aucune note de version~~ — **CORRIGÉ**

`.github/workflows/release.yml`

L'étape « Create release summary » écrit toujours le même corps : le titre, « Windows: native
runner arch », « Linux: ... », et le bloc `sha256sum -c`. Rien sur ce qui change. Pour v0.2.1,
dont l'objet **est** un correctif d'image, un utilisateur lisant les notes n'avait aucune raison
de mettre à jour. Les notes de v0.2.0 ne tiennent que par le changelog que GitHub ajoute à la
publication : une liste de titres de PR, dont « Make the finding status consistent between
headings and the status table » — vrai, et sans intérêt pour qui convertit des vidéos.

Les deux releases ont donc été rédigées à la main dans le draft. C'est exactement le motif que
#32 a corrigé pour les checksums : un artefact de release réparé à la main à chaque fois finit
par ne pas l'être.

*Correction* : le message du tag annoté devient le corps de la release — `%(contents:subject)`
en titre, `%(contents:body)` en corps. Le job `create-release` ne faisait aucun `checkout` (il ne
téléchargeait que des artefacts) ; il en a un maintenant, en `fetch-depth: 0`, pour disposer de
l'objet tag. Un tag **léger** n'a pas de message propre — git rendrait celui du commit, qui n'est
pas des notes de version — donc ce cas retombe sur l'ancien gabarit **et l'annonce par un
`::warning::`** plutôt que de publier une release vide.

*Vérification, sans taguer.* L'étape ne tourne que sur un tag (c'est tout le problème), donc le
script a été **extrait du YAML et exécuté en local** contre trois cas réels du dépôt : `v0.2.1`
(annoté) rend ses notes complètes sans avertissement, `v0.1.5` (léger) et un tag inexistant
retombent sur le gabarit avec l'avertissement.

*Défaut attrapé par cette vérification* : le `::warning::` était d'abord **à l'intérieur** du
groupe redirigé vers `/tmp/release.md`, donc il aurait été écrit dans les notes publiées au lieu
du journal d'exécution. Sorti du groupe. C'est précisément ce qu'un essai à blanc sert à trouver,
et il n'aurait pas été possible sans extraire le script du YAML.

*Confirmé en conditions réelles avec v0.2.2* : les notes publiées sont le message du tag, sans
retouche — première release du dépôt dont le corps décrit ce qui change. Aucun `::warning::` dans
le corps, tableau de mesures préservé en bloc de code (indentation de six espaces), sections
« Downloads » et « Verify integrity » ajoutées, `sha256sum -c` vert sur les assets publics.

### R-03 ✅ — ~~Le journal annonce `bitrate_bytes_sec` pour une valeur en bits~~ — **CORRIGÉ**

`common/common.go`

P-05 avait remplacé « bytes/second » par « bits/second » partout — sauf à une ligne, la clé du
journal de fin d'encodage. Elle annonçait donc un facteur 8 dans **le seul journal que le README
demande de joindre aux rapports de bug**. Trouvé en lisant la sortie d'un encodage lancé pour
Q-01, pas par relecture : le défaut ne se voit qu'à l'exécution.

*Correction* : `bitrate_bits_sec`. Aucun test ni document ne s'y référait, et un nom de clé de
journal ne se garde pas utilement par un test — c'est dit ici plutôt que d'ajouter un test qui
n'épingle rien.

### R-04 ✅ — ~~Rien ne dit quel binaire tourne~~ — **CORRIGÉ**

`FyneApp.toml`, `gui_main.go`, `.github/workflows/release.yml`

`FyneApp.toml` n'avait ni `Version` ni `Build`, la fenêtre s'intitulait « Superview » tout court,
et le rapport *Diagnostic* — celui que le README demande de joindre à un rapport de bug — ne
disait pas quelle version l'avait produit. Supportable tant qu'une seule release existait ; trois
coexistent désormais, leurs sorties diffèrent en taille depuis Q-01, et l'une d'elles porte la
couture du mode squeeze.

**Deux sources, et il en faut deux.** La métadonnée Fyne porte ce que
`fyne package --app-version` a estampillé, ce que le workflow alimente depuis le tag. Mais un
`go build` simple n'estampille rien, et Fyne répond alors **`0.0.1`**, sa valeur par défaut — une
chaîne qui ressemble à une vraie version. Lancé depuis un dépôt de travail, il lit `FyneApp.toml`
et répond ce que le fichier contient, qui dérive du tag dès que master avance. L'estampille VCS de
Go tranche : la révision est exacte et `vcs.modified` dit si l'arbre était propre.
`buildIdentity` rend donc `0.2.3 (85b6671)`, ou `0.2.3 (85b6671, modified)`, ou `dev (85b6671)`
quand rien n'a été estampillé — Fyne reconnaissant ce cas au fait que ni l'ID ni le nom ne sont
renseignés, test réutilisé tel quel plutôt que de comparer à `0.0.1`, qui leur appartient.

*Où elle apparaît* : titre de la fenêtre, ligne de journal au démarrage, et **première ligne du
rapport Diagnostic**, avant tout le reste — c'est la seule qui dise à quel binaire se rapporte
ce qui suit.

*La version vient du tag, jamais d'un fichier.* Les deux jobs de build passent
`--app-version "${GITHUB_REF_NAME#v}"`. Le `Version` de `FyneApp.toml` n'est qu'un repli pour une
compilation hors workflow ; il ne peut pas faire mentir un binaire publié.

*Vérifications.* Six cas unitaires, contre-épreuvés un par un — dont une contre-épreuve **qui
n'en était pas une** : changer le seuil de troncature de 7 à 12 laisse le résultat identique
puisque la longueur découpée reste 7. Muter la longueur elle-même fait bien rougir (L-54). Et
bout en bout, sur le chemin réel : un paquet construit par `fyne package --app-version 9.9.9`
puis exécuté journalise `build="9.9.9 (85b6671, modified)"` ; un `go build` lancé hors du dépôt
journalise `build="dev (85b6671, modified)"`.

*Constaté en chemin* : `fyne package` **réécrit `FyneApp.toml`** — il le reformate et y ajoute
`Build`. ~~Sans conséquence dans un checkout de CI~~, mais l'outil s'approprie ce fichier.
**Rectifié le 2026-09-05** : l'observation était vraie *à ce moment-là* et a cessé de l'être avec
le commit même qui la notait — `FyneApp.toml` n'avait alors ni `Version` ni `Build`, l'outil les
ajoutait ; complété, le fichier n'est plus touché. Et la conclusion « sans conséquence » a coûté
un faux diagnostic : c'est bien la propreté de l'arbre qui décide de `vcs.modified`, mais par
d'autres fichiers. Voir R-06.

### R-05 ✅ — ~~Le `workflow_dispatch` de Release échoue avant de compiler~~ — **CORRIGÉ**

`.github/workflows/release.yml`

R-04 passait `VERSION="${GITHUB_REF_NAME#v}"` à `fyne package`. Sur un `workflow_dispatch` le ref
est la branche : la version demandée valait `master`, que l'outil refuse (`invalid --app-version
parameter, integer and '.' characters only up to x.y.z`). Les deux jobs de build sont morts en
moins d'une minute. Retirer un `v` initial ne dérive une version que pour la forme `v*` ; les deux
autres refs que le bloc `on:` accepte — une branche et un tag `RC-*` — donnent une chaîne
invalide. Le cas `RC-*` aurait échoué de même, en pleine release candidate.

*Correctif* : extraire le premier `x.y.z` que le ref contient, repli `0.0.0` hors tag de version.
Dérivation extraite du YAML et essayée sur cinq refs. Leçon L-55.

### R-06 ✅ — ~~Tout binaire de release s'annonce `, modified`~~ — **CORRIGÉ**

`.gitignore`, `.github/workflows/release.yml`

v0.2.3 s'est publiée en disant `0.2.3 (9c1a8c5, modified)`. La version et la révision étaient
exactes ; `, modified` ne l'était pas — le binaire venait d'un checkout de CI immaculé. Or ce
drapeau existe pour distinguer un binaire bricolé d'une release propre : allumé sur toutes les
releases, il ne distingue plus rien, et vide de sa substance la fonction pour laquelle v0.2.3
avait été publiée.

**Cause, après un premier diagnostic faux.** J'avais désigné la réécriture de `FyneApp.toml`
(note de R-04) sans la vérifier. Elle n'a pas lieu : `fyne package` ne complète que ce qui manque,
et le fichier est complet depuis #39. En observant `git status --porcelain` **en boucle pendant**
le packaging, les vrais coupables apparaissent — et ils diffèrent selon la plateforme :

| Plateforme | Fichiers créés puis effacés | Rôle |
| --- | --- | --- |
| Linux | `fyne_metadata_init.go` | injecte la version dans le binaire |
| Windows | `fyne_metadata_init.go`, `fyne.syso`, `superview.exe` | plus la ressource icône/version, et le binaire intermédiaire |

Tant qu'ils sont là, `git status --porcelain` n'est pas vide — et c'est exactement ce que lit
`cmd/go` pour décider `vcs.modified`. Aucun n'est visible une fois l'outil terminé, ce qui
explique qu'une inspection après coup ne montre rien.

*Mécanisme démontré isolément*, sur un dépôt jetable de deux lignes : arbre propre →
`vcs.modified=false` ; un `fyne_metadata_init.go` non suivi → `true` ; le même fichier avec une
entrée `.gitignore` **commitée** → `false`. Les fichiers ignorés n'apparaissent pas dans
`git status --porcelain`, et c'est tout le ressort du correctif.

*Correctif* : `.gitignore` couvre `fyne_metadata_init.go`, `*.syso`, `superview.exe` et les
archives que l'outil laisse dans le répertoire de travail.

*Garde-fou* : les deux jobs de build vérifient désormais le **résultat** plutôt que la cause — ils
dépaquettent (Linux) ou prennent (Windows) le binaire sur le point d'être expédié et échouent s'il
porte `vcs.modified=true`. C'est lui qui a révélé que le correctif ne valait d'abord que pour
Linux. Leçons L-57, L-58.

*Vérification* : essai à blanc complet, les deux garde-fous franchis, artefact téléchargé,
extrait et **exécuté** — `build="0.0.0 (71e04a9)"`, sans mention `modified`.

### R-07 ✅ — ~~L'essai à blanc ne fonctionne que depuis `master`~~ — **CORRIGÉ**

`.github/workflows/release.yml`

Un essai à blanc étiquette ses archives avec le nom de la branche. Les branches de ce dépôt
s'appellent `fix/…`, `feat/…`, `docs/…`, et une barre oblique au milieu d'un nom de fichier
désigne un répertoire qui n'existe pas :

```
mv: cannot move 'superview-gui-linux-amd64.tar.xz' to
    'superview-gui-fix/r-06-…-linux-x86_64.tar.xz': No such file or directory
```

L'essai à blanc — dont toute la raison d'être est d'éprouver une modification du workflow avant
qu'elle ne gâche une release — ne marchait donc que depuis `master`, la seule branche sur laquelle
personne n'a besoin d'éprouver quoi que ce soit. Trouvé en s'en servant.

*Correctif* : les barres obliques deviennent des tirets ; rien en aval n'a besoin du nom exact.

### Deux prédictions confrontées aux faits

**Le rouge Windows annoncé par #32 n'a pas eu lieu.** Le message de la PR pariait sur un échec de
`TestIntegration_CancelLeavesNoPartialOutput` : Windows peut garder le descripteur ouvert après un
kill, et `PerformEncoding` ne journalise qu'un avertissement si la suppression échoue. Le job
Windows du run `33916850689` passe, sous-cas « no working file is left in the directory » compris.
L'hypothèse était explicite et raisonnable ; elle est infirmée, et c'est le bon résultat.

**Les checksums de v0.2.0 sont bons.** Le fichier avait été corrigé à la main avant publication ;
`sha256sum -c` sur les deux assets réellement téléchargés depuis la release passe. L'étape
corrigée du workflow a été rejouée localement sur un faux arbre d'artefacts : elle produit des
noms nus, et la vérification depuis un dossier plat passe. Mais elle **n'a jamais tourné en CI** —
`create-release` est gardé par `startsWith(github.ref, 'refs/tags/v')`, donc un
`workflow_dispatch` la saute entièrement. **Son premier vrai passage a eu lieu avec v0.2.1** :
`checksums.txt` sort avec des noms nus et `sha256sum -c` passe sur les assets téléchargés depuis
la release, sans aucune retouche manuelle. Le correctif de #32 est validé en production, cinq
heures après avoir été écrit.

---

## 3quinquies. Sixième passe (2026-09-06) — structure, tests de validation, release

Passe demandée sur trois axes : documentation, tests de validation, release. Les
constats portent le préfixe `D-` (documentation et outillage) et `V-` (validation).
Ceux marqués **consigné** n'ont délibérément pas été corrigés : la release a été
mise hors périmètre pour ce chantier.

### Ce qui s'est révélé sain

Vérifié avant de chercher des défauts, pour ne pas « corriger » ce qui va bien :

- Le workflow de release est solide : décision centralisée dans `prepare`, entrées
  passées par l'environnement et jamais interpolées dans un `run:`, tag créé
  seulement après que les deux plateformes ont construit, garde-fou sur le binaire
  expédié, `notify` capable de rapporter un échec.
- `.gitignore` était complet et argumenté côté Windows (voir D-06 pour l'asymétrie
  Linux). L'arbre de travail est propre : aucun binaire commité, aucun artefact.
- Aucun `TODO`/`FIXME` dans le code Go. Aucun symbole exporté sans appelant.
- La géométrie de remap reste la partie la mieux protégée du dépôt.
- P-10 est légitimement clos : `main()` reste longue **et c'est assumé**, le constat
  portait sur l'inatteignabilité de l'état, pas sur la longueur.

### D-01 ✅ — ~~Le README documente une API qui n'existe plus~~ — **CORRIGÉ**

Le § *API Documentation* listait `GetConfig()`, `SetConfig()` et
`CreateDefaultConfig()`, supprimés par `81c14a7`, et donnait `CheckFfmpeg()`,
`InitEncodingSession()` et `PerformEncoding()` sans leur premier paramètre `*Config`.
Le § *Example* vingt lignes plus bas, lui, était correct.

C'est exactement le défaut que **L-29** décrit et dont elle prescrit le remède
(`grep -rn "NomFonction" --include='*.md' .`). La leçon avait été écrite, le remède
n'avait jamais été appliqué jusqu'au bout : l'exemple avait été corrigé, la liste
au-dessus non.

*Correctif* : la section est supprimée plutôt que corrigée. L'application est
GUI-only et n'a aucun consommateur externe ; une API publiée dans un README est une
promesse que rien ne vérifie, et le compilateur ne lit pas le Markdown.

### D-02 ✅ — ~~Le README n'explique l'installation que sous Windows~~ — **CORRIGÉ**

`release.yml` publie une archive Linux et un `checksums.txt` depuis `#32`, et le
README annonce Linux comme officiellement supporté depuis `76a8341`. Le § *Installation*
ne parlait que de l'archive Windows, et les notes de release invitaient à un
`sha256sum -c checksums.txt` que le README ne mentionnait nulle part.

*Correctif* : les deux plateformes documentées, avec la vérification d'empreinte. La
structure de l'archive Linux a été **vérifiée en produisant une archive**
(`superview/usr/local/bin/superview` plus un `Makefile` d'installation) et non déduite
du workflow — une première rédaction, écrite d'après `release.yml`, donnait un chemin
faux.

### D-03 ✅ — ~~Trois procédures de release concurrentes~~ — **CORRIGÉ**

`RELEASING.md` posait que le message du tag **est** `RELEASE_NOTES.md`. À côté :

- `make release-prepare VERSION=x.y.z` créait un tag annoté avec
  `-m "Release v$(VERSION)"`. Ce message devient le corps de la release publiée : qui
  passait par le `Makefile` publiait une release dont les notes disaient
  « Release v0.2.4 » et rien d'autre. `RELEASING.md` ne mentionnait pas la cible.
- `build.sh`, neutralisé mais dont l'en-tête décrivait encore le flux d'avant `#41`.

*Correctif* : les deux supprimés. `RELEASING.md` est la seule procédure.

### D-04 ✅ — ~~`copilot-instructions.md` annonçait un seuil de couverture de 30 %~~ — **CORRIGÉ**

Réel : 50 % sur `./...` dans `test.yml` et `release.yml`. Le fichier recommandait aussi
`go test ./common` « pour la validation de routine », ce qui saute les 28 tests du
paquet `main`. Le reste du fichier était exact.

### D-05 ✅ — ~~`go.yml` et deux jobs de `lint.yml` refont le travail des autres~~ — **CORRIGÉ**

`go.yml` (`go build -v ./...` sur Ubuntu) est couvert par le job `native-build-matrix`
de `test.yml`, qui construit sur Windows **et** Linux. C'est le workflow du gabarit
GitHub, jamais élagué.

`lint.yml` lançait cinq jobs, chacun dépensant un `apt-get` en en-têtes GUI. Les jobs
`vet` et `staticcheck` réexécutaient des linters que `.golangci.yml` active déjà. Pire,
le job `staticcheck` tournait **sans** le filtre `SA*`/`S1*` de la configuration : il
appliquait donc les familles `ST*` et `QF*` que la configuration désactive à dessein
(une réécriture `QF` toucherait les `math.Pow` de `GeneratePGM`, cf. L-09/L-12). Un job
qui contredit la configuration n'est pas un second avis, c'est une seconde source de
vérité.

*Correctif* : `go.yml` supprimé, `lint.yml` ramené à trois jobs. Vérifié que
`staticcheck` lancé seul ne rapporte rien de plus aujourd'hui.

### D-06 ✅ — ~~L'asymétrie Linux du correctif R-06~~ — **CORRIGÉ**

`#43` a établi que `fyne package` salit l'arbre pendant qu'il construit, et a ignoré
`fyne_metadata_init.go` (Linux), `*.syso` et `superview.exe` (Windows).

En observant `git status --porcelain` toutes les 0,2 s pendant un packaging Linux sur
un arbre propre, deux artefacts apparaissent qui ne sont couverts par aucun de ces
motifs : **`superview`** — le binaire intermédiaire nommé d'après le module, exactement
ce que `superview.exe` est côté Windows — et **`tmp-pkg/`**. Le raisonnement de `#43`
avait bien identifié le mécanisme mais n'en avait tiré la conséquence que d'un côté.

Le garde-fou sur le binaire expédié aurait fait échouer la release plutôt que de la
laisser mentir — c'est précisément ce pour quoi L-58 l'a introduit — mais un échec de
release au moment de publier reste un coût. Les deux motifs sont ajoutés.

> Constaté au passage, sans conséquence : `fyne package` réécrit aussi `FyneApp.toml`,
> qui est suivi — il en retire les commentaires et incrémente `Build`. La réécriture a
> lieu **après** la construction (le fichier n'apparaît modifié qu'une fois l'outil
> terminé, jamais pendant), donc elle ne tamponne pas le binaire. C'est ce que `#42`
> avait supposé à tort être la cause de R-06.

### D-07 ✅ — ~~Le `Makefile` et la CI ne mesuraient pas le même périmètre~~ — **CORRIGÉ**

`make test`, `make vet` et `make coverage` portaient sur `./common`, la CI sur `./...` :
le chiffre local ne correspondait jamais au seuil, et les tests du paquet `main` ne
tournaient jamais en local. `install-tools` installait par ailleurs `golangci-lint` par
le chemin **v1** (sans `/v2`), le piège de X-04.

### D-08 ✅ — ~~`tools/` n'était référencé nulle part~~ — **CORRIGÉ**

`tools/gen_icons.py` écrit dans un `assets/icons` qui n'a jamais existé.
`tools/install_linux_launcher.sh` affichait `go build superview-gui.go`, fichier disparu
depuis `76a8341` — c'était la **dernière occurrence vivante** de cette référence dans le
dépôt. Ni l'un ni l'autre n'était cité par le README, le `Makefile` ou un workflow.

### D-09 ✅ — ~~Trois chiffres de couverture contradictoires dans ce document~~ — **CORRIGÉ**

54,6 %, 59,3 % et 71,6 % coexistaient pour le même « après », dont deux dans la même
section. Ils mesuraient trois choses différentes — module sans ffmpeg, module avec, et
le seul paquet `common` — sans que rien ne le dise. La cartographie des fichiers était
par ailleurs périmée de 400 lignes (`common.go` annoncé à 1 155 l., réel 1 587).

### V-01 ✅ — ~~Une suite verte pouvait n'avoir encodé aucune image~~ — **CORRIGÉ**

Seize gardes terminaient un test par `t.Skip` quand ffmpeg, un de ses encodeurs ou une
fixture manquait. Elles couvrent les quatre tests d'intégration et le test
d'équivalence du remap — c'est-à-dire **tout ce qui vérifie une conversion réelle**.

La CI installe ffmpeg sur les deux runners, mais installer et utiliser sont deux
affirmations différentes : `choco` a déjà rapporté un succès en ne laissant rien sur le
`PATH`, ce pour quoi `test.yml` lance `ffmpeg -version` derrière. Le seul signal
restant que les tests avaient tourné était la couverture, et indirectement : 59,3 %
avec ffmpeg contre 51,4 % sans, de part et d'autre d'un seuil à 50 %.

*Correctif* : `skipWithoutFFmpeg` saute par défaut et **échoue** sous
`SUPERVIEW_REQUIRE_FFMPEG=1`, que `test.yml` et `release.yml` positionnent. Vérifié dans
les deux sens : ffmpeg retiré du `PATH`, les cinq tests sautent sans la variable et
échouent avec.

### V-02 ✅ — ~~Le parsing de la sortie ffprobe n'avait aucun test~~ — **CORRIGÉ**

`CheckVideo` mêlait l'appel au processus et le parsing, donc éprouver le parsing
supposait de trouver un fichier vidéo provoquant chaque erreur. Ses cinq branches de
rejet n'avaient jamais été exécutées. C'est pourtant la **seule entrée que le programme
ne produit pas lui-même**.

*Correctif* : `parseVideoSpecs` extraite et couverte à 100 % depuis
`common/testdata/ffprobe/` — y compris deux formes qui ne doivent **pas** être rejetées
(29,97 fps ; un ffprobe assez ancien pour ne rapporter ni `pix_fmt` ni `r_frame_rate`).

### V-03 ✅ — ~~Les deux pièces réclamées pour un rapport de bug étaient à 0 %~~ — **CORRIGÉ**

Le README demande de joindre le rapport *Diagnostic* et le log. `CheckHealth`,
`checkFFmpegHealth` et `checkFFprobeHealth` n'avaient jamais tourné de bout en bout —
seules les sondes CPU, mémoire et disque avaient un test, et `GetHealthReport` n'était
alimenté que par des structures écrites à la main dans le test. Le chemin « ffmpeg
absent », qui est le cas pour lequel le rapport existe, n'était pas couvert.

Même chose pour `DefaultObservabilityHandler` : les tests exerçaient `EventRecorder`
avec un mock, jamais le handler qui écrit réellement le log. La rotation par taille de
`OpenLogFile`, seule chose entre une installation ancienne et un fichier sans borne,
n'avait jamais été exercée non plus.

### V-04 ✅ — ~~Un test recopiait à la main des constantes de `main()`~~ — **CORRIGÉ**

`TestToolbarFitsWindow` portait ses propres copies de la largeur des boutons et de la
fenêtre, sous un commentaire demandant au lecteur de les tenir en phase. Elles sont des
constantes de paquet : les porter hors des clous fait désormais échouer le test. (`actionButtonWidth`
a depuis été remplacée par `actionButtonMinWidth`, un plancher et non une taille : voir U-01.)

`fakeHangingFFmpeg` utilisait par ailleurs `os.Setenv` avec restauration différée, qui
fuit sur panique et ne peut pas refuser un `t.Parallel` — remplacé par `t.Setenv`.

### D-10 ✅ — ~~La release exigeait un fichier de notes tenu à la main~~ — **SIMPLIFIÉ**

Constat de l'utilisateur, le 2026-09-06 : `RELEASE_NOTES.md` n'a pas lieu d'être ; si
`RELEASING.md` documente le processus, cela suffit.

Le fichier coûtait plus qu'il ne rapportait :

- il fallait le réécrire avant **chaque** release, sans quoi la nouvelle version sortait
  avec le texte de la précédente — assez probable pour que le workflow ait dû se doter
  d'un contrôle sur sa première ligne ;
- entre deux releases, il vivait dans `master` en contenant les notes de la version
  **déjà publiée**, sans marqueur. Il ressemblait donc en permanence à un résidu périmé
  alors qu'il était dans son état normal ;
- il portait par ricochet le bump de `FyneApp.toml`, que `RELEASING.md` demandait de
  faire « dans la même PR que les notes ».

*Correctif* : `generate_release_notes: true`. GitHub construit la liste des PR fusionnées
depuis le tag précédent, plus un lien de comparaison. Le run continue d'écrire la section
téléchargements et le bloc `sha256sum` ; l'action **préfixe** ce corps aux notes générées
(vérifié dans `action.yml` de la v3.0.3 avant d'y toucher).

Ce que ça supprime : le fichier, ses deux contrôles dans `prepare`, l'étape de préparation
avant chaque release, et le bump de `FyneApp.toml`.

Ce que ça préserve, vérifié chemin par chemin en sortant le script du YAML (L-53) et en
l'exécutant contre de vrais tags du dépôt :

| Chemin | Corps produit |
| --- | --- |
| Bouton (`workflow_dispatch`) | Téléchargements + checksums, puis la liste générée |
| Tag annoté poussé à la main (`v0.2.3`) | **Son message en tête**, puis téléchargements, puis la liste |
| Tag léger poussé à la main (`v0.1.6`) | Avertissement dans le journal du run, téléchargements, liste — et **pas** le message du commit, qui est ce que `#37` avait corrigé |

Le chemin manuel reste donc la façon d'écrire des notes soi-même, et le tag reste annoté :
son message n'est plus la source des notes, mais un tag léger renverrait le message du
commit à quiconque interroge le tag.

**Contrepartie assumée** : la qualité des notes devient celle des titres de PR. Ce dépôt
les écrit déjà en phrases complètes — « Stop release binaries from announcing themselves as
modified » se lit très bien dans une release —, donc le coût est nul ici et le resterait
tant que cette habitude tient.

### D-11 ✅ — ~~`FyneApp.toml` portait une version que rien ne devait lire~~ — **CORRIGÉ**

`RELEASING.md` affirmait « la version vit dans le tag, rien ne la lit dans l'arbre », et
`FyneApp.toml` contenait `Version = "0.2.3"`. Mesuré : un `go build` nu **lit bien ce
champ** et annonçait `0.2.3` depuis un arbre qui n'était pas cette version.

*Correctif* : le champ est retiré. Vérifié que `fyne package --app-version 0.2.4` fonctionne
sans lui et produit un binaire annonçant `0.2.4`, et que l'outil ne réécrit pas de `Version`
dans le fichier. `buildIdentity` fusionne ses deux cas non tamponnés — « aucune métadonnée »
et « métadonnée sans version » — en un seul `dev` : ils disent la même chose au lecteur d'un
rapport de bug. Un build local annonce désormais `dev (7a74276)`.

### D-12 ✅ — ~~Rien n'ordonnait les notes générées, et `gh` visait le mauvais dépôt~~ — **CORRIGÉ**

Suite de D-10, une fois la convention posée par l'utilisateur le 2026-09-06 : « pour chaque
modification, même mineure, je pousserai un commit + PR ». Deux choses manquaient pour que
cette convention produise le résultat attendu.

**Les notes n'étaient pas triées.** `dependabot` tourne quotidiennement sur `gomod` : sur une
release un peu longue, les montées de version noieraient le travail qu'on veut lire.
`.github/release.yml` sépare donc `Changes` et `Dependencies`, et le label `dependencies` est
rendu explicite dans `dependabot.yml` plutôt que laissé au comportement par défaut de l'outil.

> **Le `"*"` de ce fichier est porteur.** La règle GitHub est qu'*une PR ne correspondant à
> aucune catégorie n'apparaît pas du tout dans les notes*. Ce dépôt ne pose aucun label sur ses
> propres PR — vérifié sur les douze dernières, toutes sans label. Sans l'attrape-tout, une
> configuration en apparence raisonnable viderait chaque release. La catégorie `Changes` porte
> donc `"*"` en premier, avec `exclude: dependencies` pour laisser les bumps tomber dans la
> seconde section.

**`gh` résolvait vers le dépôt parent.** `Canaill51/superview` est un fork de `Niek/superview`,
et aucun dépôt par défaut n'était configuré : `gh pr list` renvoyait les PR de l'amont — d'où
un premier relevé de labels entièrement faux, corrigé en repassant avec `-R`. Conséquence
sérieuse pour la convention qui vient d'être adoptée : `gh pr create` aurait proposé les
modifications **à l'auteur d'origine**. `gh repo set-default Canaill51/superview` le règle, une
fois par clone. `git push` n'était pas concerné : `origin` pointait déjà au bon endroit.

**Vérifié** : l'endpoint `releases/generate-notes` — qui ne crée ni ne publie rien — produit
bien, pour une v0.2.4 hypothétique, la liste des PR #42 et #43 avec `v0.2.3` choisie
automatiquement comme base. **Non vérifié** : l'effet de `.github/release.yml` lui-même, que
GitHub ne lit que depuis la branche par défaut ; il faut donc l'avoir fusionné. Le même
endpoint le dira, sans rien publier.

### R-08 🟠 **consigné** — Le correctif R-06 n'est pas publié

`HEAD` est plusieurs commits après le tag `v0.2.3`. Les binaires actuellement en ligne
annoncent donc toujours `0.2.3 (9c1a8c5, modified)` — le défaut même que `#43` corrige.
Le correctif existe et n'est pas entre les mains des utilisateurs.

Depuis D-10, une v0.2.4 ne demande **plus aucune préparation** : Actions → Release → Run
workflow, taper `0.2.4`. Les notes se génèrent, il n'y a plus de fichier à réécrire ni de
`FyneApp.toml` à bumper.

### R-09 🟡 **consigné** — `v0.1.1` à `v0.1.6` sont des tags légers

`git cat-file -t` renvoie `commit` et non `tag` : ces six tags n'ont pas de message.
C'est la forme qui produit une release vide, décrite dans `#41`. Les versions à partir
de `v0.2.0` sont correctement annotées. Sans conséquence tant que personne ne rejoue une
release depuis ces tags.

### R-10 🟡 **consigné** — Aucune signature de code

Ni sous Windows ni ailleurs ; seuls des SHA-256 non signés. C'est un arbitrage produit
(certificat, coût, SmartScreen), pas un défaut d'implémentation.

### R-11 🟡 **consigné** — La vérification de la version publiée reste manuelle

`RELEASING.md` l'assume explicitement : rien ne contrôle automatiquement que le binaire
publié annonce la version qu'il prétend. C'est le seul angle mort du flux, et il est
documenté comme tel.

### Dette assumée, non corrigée

- `findWindowsToolBinary` (~60 l.) et les enveloppes de `gui_native_dialog_*.go`
  (~120 l.) restent à 0 %. Ce sont des sondes de système de fichiers et des appels
  PowerShell/zenity dont le coût de mise sous test dépasse le bénéfice.
- `common.go` (1 587 l.) mêle six responsabilités. Le découpage en `pgm.go` et
  `tools.go` est identifié, non fait : le fichier est couvert à 81,5 % et
  `pgm_golden_test.go` fige déjà la géométrie.

---

## 3sexies. Septième passe (2026-09-06) — signalement utilisateur : la barre d'outils

Pas une passe d'analyse : deux défauts d'affichage rapportés par l'utilisateur,
capture d'écran à l'appui, sur le binaire de `2cf0020`. Préfixe `U-` (utilisateur).

Ce qu'ils ont en commun est plus intéressant que chacun d'eux : la rangée de boutons
était **la partie de la fenêtre qui avait un test dédié**, `TestToolbarFitsWindow`,
écrit précisément pour cela (constat C-01, leçon L-20) et resserré une fois depuis
(V-04). Il était vert pendant que les deux défauts étaient visibles à l'œil nu.

### U-01 ✅ — ~~Trois boutons sur six affichaient un libellé tronqué~~ — **CORRIGÉ**

`main()` posait chaque bouton dans un `container.NewGridWrap` de 150 × 34 px, taille
identique pour tous. Un `GridWrap` **impose** sa cellule : il ne s'agrandit pas pour
son contenu, il le rogne. Or les boutons portent une icône, qui coûte une trentaine de
pixels. Mesuré à `2cf0020`, sous le thème par défaut :

| Bouton | Minimum réclamé | Cellule reçue |
| --- | --- | --- |
| `Choose input file` | 158 × 36 | 150 × 34 |
| `Choose output file` | 168 × 36 | 150 × 34 |
| `Start transformation` | 186 × 36 | 150 × 34 |
| `Cancel` | 86 × 36 | 150 × 34 |
| `Diagnostic` | 114 × 36 | 150 × 34 |
| `Quit` | 47 × 36 | 150 × 34 |

Trois libellés débordaient horizontalement, et les six manquaient de 2 px en hauteur.

**Pourquoi le test ne l'a pas vu** — il mesurait `toolbar.MinSize()`, c'est-à-dire la
somme des cellules. Un `GridWrap` rapporte la taille qu'on lui a donnée : la réponse
était 920 px quoi que contiennent les boutons, et 920 ≤ 980 concluait « ça tient ».
Le test construisait de surcroît ses boutons **sans icône**, donc même une mesure
correcte du contenu aurait porté sur des boutons plus étroits que les vrais.

*Correctif* — `actionButtonCell` part du minimum du bouton et l'élargit à
`actionButtonMinWidth` (140 px), au lieu de l'écraser. La cellule ne peut plus être
plus petite que ce qu'elle contient. Le test compare désormais chaque cellule au
minimum du bouton qu'elle porte, sur des boutons construits avec leurs icônes.

Deuxième garde-fou, à l'autre bout : `window.Resize` prend le maximum entre la
géométrie voulue et `content.MinSize()`. Un libellé plus long élargit la fenêtre au
lieu de sortir du cadre — la fenêtre étant à taille fixe, rien à l'écran ne dirait
qu'un bouton manque. La rangée mesure 952 px pour 980 disponibles : la fenêtre garde
sa taille d'origine aujourd'hui.

### U-02 ✅ — ~~La rangée de boutons était collée au bord gauche~~ — **CORRIGÉ**

La barre était un `container.NewHBox` nu. Un `HBox` empile depuis la gauche et laisse
tout l'espace restant à droite ; il n'y a pas de réglage d'alignement à activer.
L'asymétrie était de l'ordre de 60 px avant correction, et aurait grandi à chaque
élargissement de la fenêtre.

*Correctif* — la rangée est enveloppée dans `container.NewCenter`. Mesuré après
correction dans une fenêtre de 980 px : 14 px de marge de chaque côté.

`TestToolbarIsCentred` épingle le résultat. Il est écrit à travers un canevas de test
(`test.NewWindow` puis `Driver().AbsolutePositionForObject`) et non sur l'arbre de
conteneurs : sa première version interrogeait la forme de l'arbre, et sa contre-épreuve
rougissait sur « ce n'est plus un `Center` » au lieu de « la rangée est à gauche » —
elle aurait donc validé n'importe quel `Center` mal placé. Leçon L-70.

---

## 3septies. Huitième passe (2026-09-06) — signalement utilisateur : l'accélération matérielle

Signalement : « en fonction de la version de FFmpeg installée, l'accélération
matérielle GPU peut ne plus être prise en charge ; la 8.1.2 ne la permet plus sur
une Quadro A1000 ». Enquête demandée sur les documentations officielles.

### U-03 ✅ — ~~Les capacités matérielles étaient déduites d'une liste qui ne voit pas le pilote~~ — **CORRIGÉ**

**Ce que l'enquête a établi.** L'attribution de départ est fausse et la cause est
ailleurs — mais le signalement est exact.

FFmpeg 8.1 n'a rien changé : `libavcodec/nvenc.c` est **identique** entre
`release/8.0` et `release/8.1` sur toute la logique de vérification du pilote (diff
vide ; le diff complet du fichier fait 51 lignes, toutes des conversions de type).
Le plancher est fixé **à la compilation**, par les `nv-codec-headers` employées, et
lu à l'exécution :

```c
if ((NVENCAPI_MAJOR_VERSION << 4 | NVENCAPI_MINOR_VERSION) > nvenc_max_ver) {
    av_log(avctx, AV_LOG_ERROR, "Driver does not support the required nvenc API version. …");
    nvenc_print_driver_requirement(avctx, AV_LOG_ERROR);
    return AVERROR(ENOSYS);
}
```

Le plancher est un littéral compilé dans le binaire, donc mesurable sans exécuter
Windows. Mesures du 2026-09-06 — le binaire de l'utilisateur a été identifié par
l'empreinte SHA-256 du manifeste winget `Gyan.FFmpeg` 8.1.2, qui correspond :

| Build | FFmpeg | Plancher |
| --- | --- | --- |
| Ubuntu `libavcodec62` 7:8.0.1-3ubuntu2 | 8.0.1 | 530.41.03 |
| gyan.dev (winget) | 8.1.1 | 570.0 |
| **gyan.dev (winget) — le binaire en cause** | **8.1.2** | **610.00** |
| gyan.dev | 9.0.1 | 610.00 |
| BtbN `win64-gpl-8.1` | 8.1.2 | 570.0 |
| BtbN `win64-gpl-9.0` | 9.0.1 | 610.00 |

Entre 8.1.1 (2026-05-04) et 8.1.2 (2026-06-27), même empaqueteur, gyan.dev est passé
du SDK 13.0 au SDK 13.1. Or la branche pilote **RTX Enterprise plafonne à 597.06**
(R595, 27 août 2026) : sur une carte professionnelle, aucune mise à jour de pilote ne
peut satisfaire une exigence de 610. Ce n'était pas une machine mal configurée, c'était
une exigence insatisfiable.

**Le défaut qui nous appartient.** Superview déduisait ses capacités de
`ffmpeg -encoders` et `ffmpeg -hwaccels`, qui répondent à une question de compilation
et ne voient pas le pilote. Conséquences, toutes vérifiées :

1. la ligne « Hardware » annonçait `planned h264_nvenc encode + CUDA decode` sans que
   rien ne l'ait vérifié ;
2. le repli fonctionnait — l'encodage sortait sur `libx264` — mais restait muet, donc
   l'utilisateur voyait un encodage anormalement lent sans explication ;
3. le rapport Diagnostic, que le README demande de joindre à tout rapport de bug, ne
   disait **rien** des encodeurs. C'est ce qui a rendu cette enquête nécessaire.

Mesuré sur la machine de développement : `ffmpeg -encoders` y annonce six encodeurs
(`h264_qsv`, `hevc_qsv`, `h264_vaapi`, `hevc_vaapi`, `h264_v4l2m2m`, `hevc_v4l2m2m`)
qui ne peuvent ouvrir aucun périphérique. L'écart est la règle, pas l'exception.

*Correctif* — `common/probe.go` : chaque encodeur annoncé se voit demander d'encoder
une trame de 256×256, et c'est le code de retour qui décide. `ApplyEncoderProbe` retire
du profil ffmpeg ceux qui ont refusé, et **uniquement ceux-là** — un encodeur non sondé
n'est pas un encodeur refusé. La ligne « Hardware » ne nomme plus d'encodeur avant que
la sonde soit revenue, le menu déroulant n'offre plus l'impossible, et le rapport
Diagnostic porte une section *Encoders* avec, pour chaque refus, les mots de ffmpeg.

Coût mesuré : 0,95 s pour dix encodeurs, dans une goroutine de démarrage. Sondes en
série (des sondes concurrentes sur un même GPU se disputent les sessions NVENC) et
bornées par un délai propre, sans quoi un pilote bloqué retiendrait la goroutine.

*Ce qui n'a pas été fait dans cette PR* — l'empaquetage d'un FFmpeg épinglé, décidé avec
l'utilisateur (plancher 570, Windows et Linux, binaire embarqué prioritaire) et traité
séparément ; et les chemins matériels supplémentaires `*_vulkan` et `*_d3d12va`, qui
contournent entièrement la négociation NVENC.

---

### U-04 ✅ — ~~Le comportement dépendait du FFmpeg que l'utilisateur avait installé~~ — **CORRIGÉ**

Suite décidée avec l'utilisateur après U-03 : « je souhaite que l'application ne soit
pas dépendante d'une version installée par l'utilisateur ». Paramètres arrêtés par lui :
plancher **570**, Windows **et** Linux, binaire embarqué **prioritaire** sur le `PATH`.

**Ce que l'empaquetage achète, et ce qu'il n'achète pas.** Il supprime une variable —
quel FFmpeg tourne — et rien d'autre : un GPU absent, un pilote sous le plancher, une
limite de sessions NVENC restent des causes de repli. La garantie réelle s'énonce ainsi :
*le FFmpeg que nous livrons n'exigera jamais un pilote que la machine ne peut pas avoir.*
La sonde U-03 dit le reste.

**Le choix des builds.** Ce qui est épinglé est le **plancher**, pas la version. Mesures :

| Plateforme | Build retenu | Plancher | Charge utile |
| --- | --- | --- | --- |
| Windows | gyan.dev `ffmpeg-8.1.1-essentials_build` | 570.0 | +71 Mo dans le zip |
| Linux | BtbN `linux64-gpl-8.1`, autobuild de fin de mois | 570.0 | +80 Mo dans le tar.xz |

Écartés, et pourquoi : gyan 8.1.2 et 9.0.1 (plancher 610, insatisfiable sur carte
professionnelle) ; les variantes `lgpl` (sans libx264/libx265, donc sans notre repli
CPU) ; gyan `full_build-shared` (92 Mo zippés contre 71, `avfilter` y pèse 105 Mo à lui
seul) ; BtbN `win64-gpl` statique (106 Mo, le choix du fork).

**La durabilité des URL, mesurée.** Le pin du fork (`autobuild-2026-07-20-14-10`) est
déjà en **404** : BtbN purge ses builds de milieu de mois. Mais il **conserve celui de
chaque fin de mois** — celui du 31 octobre 2024 répond encore aujourd'hui (HTTP 200).
Le pin Linux vise donc une fin de mois. Côté Windows, les releases gyan sont versionnées,
donc permanentes par construction.

*Correctif* — `resolveToolBinary` cherche dans l'ordre `SUPERVIEW_FFMPEG_DIR`, la copie
empaquetée, le `PATH`, puis les répertoires winget/scoop. Sous Linux les outils vont dans
`../lib/superview/` et non à côté de l'application : celle-ci s'installe dans `$PREFIX/bin`,
où un `ffmpeg` masquerait celui du système pour toute la machine.

Le workflow de release télécharge, vérifie l'empreinte SHA-256, **relit le plancher dans le
binaire** (`.github/scripts/nvenc-driver-floor.sh`) et échoue s'il a bougé. Côté Linux il
patche en plus le `Makefile` généré par fyne, puis lance `make install` dans un répertoire
jetable pour vérifier que les fichiers atterrissent là où l'application les cherche.

**Défaut découvert par cette vérification** — `make install` d'un paquet fyne 1.7.2
**échoue** : le `Makefile` référence `usr/local/share/pixmaps/$(Icon)` alors que le fichier
livré est `$(Icon).png`. L'échec survient à la dernière ligne, après l'installation du
binaire, ce qui explique qu'il soit passé inaperçu — **toutes les archives Linux publiées
par ce projet sont concernées**. Le workflow corrige la ligne à partir du nom du fichier
réellement présent, et la vérification d'installation couvre désormais l'icône.

*Vérification fonctionnelle* — l'application **installée** a été lancée sur une machine
disposant d'un `/usr/bin/ffmpeg` valide, après remplacement du ffmpeg empaqueté par un
témoin : 15 appels au binaire empaqueté (version, capacités, chaque sonde), zéro au
`PATH`.

---

### U-05 ✅ — ~~Deux chemins matériels sans plancher pilote restaient inutilisés~~ — **CORRIGÉ**

Troisième volet du chantier ouvert par U-03. FFmpeg 8.1 expose `h264_vulkan`,
`hevc_vulkan` et, sous Windows, `h264_d3d12va` / `hevc_d3d12va`. Ces encodeurs
passent par le **pilote d'affichage** et non par l'API NVENC : la négociation de
version qui a coûté l'accélération matérielle à la RTX A1000 n'existe pas pour eux.
`ffmpeg -encoders` les listait déjà chez nous — `candidateEncodersForCodec` ne les
proposait jamais.

**Ce que leur ajout a révélé.** Ils n'acceptent que des trames déjà sur le
périphérique, comme VAAPI. Or `remap` est un filtre CPU : les trames sont en mémoire
système à la sortie de la chaîne, quel que soit l'encodeur. Mesuré :

| Commande | Résultat |
| --- | --- |
| `-c:v h264_vulkan`, sans périphérique ni upload | `Impossible to convert between the formats supported by the filter 'Parsed_null_0'…` |
| `-vf format=nv12,hwupload` **sans** `-init_hw_device` | `A hardware device reference is required to upload frames to.` |
| `-init_hw_device vulkan=sv -filter_hw_device sv` **et** `-vf format=nv12,hwupload` | **OK** |

Donc `h264_vaapi`, déjà dans la liste des candidats depuis longtemps, **n'a jamais pu
fonctionner** : le pipeline ne lui a jamais fourni ni périphérique ni upload. Sur une
machine où VAAPI marche, l'encodage échouait et repliait sur le CPU.

*Correctif* — `hwDeviceArgs` émet `-init_hw_device <type>=sv -filter_hw_device sv`
avant les `-i`, et `hwUploadFilters` ajoute `,format=nv12,hwupload` à la fin de la
chaîne (`,format=p010,hwupload` en 10 bits : les pools matériels sont semi-planaires).
L'upload s'ajoutant après le `remap`, la géométrie est inchangée.

**La sonde et la conversion partagent ces deux fonctions**, et c'est le point plutôt
qu'une commodité : une sonde qui ouvrirait un périphérique que la conversion n'ouvre
pas passerait puis échouerait à l'encodage ; l'inverse condamnerait un encodeur qui
fonctionne. `TestProbeAndConversionAskTheSameQuestion` compare les deux.

*Vérification* — conversion réelle 640×480 → 853×480 par `h264_vulkan` sur RTX 4070,
géométrie conforme, et le résumé confirme le chemin employé plutôt qu'un repli.

**Deux contre-épreuves fausses, corrigées.** Casser le code partagé neutralisait la
contre-épreuve : la sonde échouait aussi, donc le test se *sautait* au lieu d'échouer
(L-49). Et l'assertion du test cherchait le nom de l'encodeur dans le résumé — or le
message de repli, `Hardware: h264_vulkan failed; used CPU encode (libx264)`, le
contient également : le test passait sur un repli CPU. Assertion refaite sur le sens du
message, pas sur la présence du nom.

*Non testable ici* — `*_d3d12va` est spécifique à Windows. La sonde rend l'ajout sans
risque : un chemin qui ne fonctionne pas n'est jamais retenu.

---

### U-06 ✅ — ~~Le README enseignait encore le défaut que le chantier venait de corriger~~ — **CORRIGÉ**

Relevé par l'utilisateur après la fusion de #54 : « le README n'a pas été mis à jour ».
Exact, et pire que cela.

Les trois PR du chantier avaient bien modifié le README — mais seulement sa section
*Hardware acceleration*. Les autres sections, écrites avant, disaient encore le
contraire :

| Passage | Ce qu'il disait |
| --- | --- |
| § *Requirements* | `winget install -e --id Gyan.FFmpeg` — **le build 8.1.2 à plancher 610**, exactement celui qui a coûté NVENC à la carte du signalement |
| § *Requirements* | « Check FFmpeg NVENC support : `ffmpeg -encoders \| grep nvenc` » — la vérification dont U-03 a établi qu'elle ne prouve rien |
| § *Requirements* | FFmpeg présenté comme un prérequis, alors que les archives en embarquent un |
| § *Installation* | Contenu des archives décrit sans `ffmpeg`, `ffprobe` ni les notices ; nom d'exécutable faux ; le zip Windows a maintenant un dossier |
| § *Usage* | « fix your `PATH` and retry », sans un mot de `SUPERVIEW_FFMPEG_DIR` |
| § *Configuration* | Liste des variables d'environnement sans `SUPERVIEW_FFMPEG_DIR` |
| § *Architecture* | Cartographie sans `probe.go`, `THIRD_PARTY_NOTICES.md` ni `.github/scripts/` |

Deux erreurs **préexistantes** ont été trouvées au passage, sans rapport avec le
chantier : les étapes de la GUI nommaient des boutons qui n'existent pas (« 1) Choose
input file », « 3) Start Superview transform » ; les libellés réels n'ont pas de numéro
et le troisième est « Start transformation »), et `.\superview-gui.exe` ne correspondait
au nom d'aucun exécutable livré.

*Cause* — le balayage prescrit par AGENTS.md porte sur les **symboles** :
`grep -rn "FunctionName" --include='*.md'`. Il a été fait, et il ne pouvait rien
trouver : aucun nom de fonction n'avait changé. Ce qui avait changé, ce sont des
**affirmations** — « installez ffmpeg », « voici ce que contient l'archive », « voici
comment vérifier votre GPU » — qu'aucun grep sur un identifiant ne relie au correctif.

*Correctif* — § *Requirements* réécrite autour de « si vous utilisez une archive, il n'y
a rien à installer », avec l'avertissement sur `winget` et le renvoi à *Diagnostic* pour
vérifier le GPU ; contenu réel des archives et disposition d'installation ;
`SUPERVIEW_FFMPEG_DIR` documenté là où on le cherche ; cartographie à jour. Corrigés
aussi : `docs/ANALYSE.md` § 1 (cartographie et compteurs à `ec7d753`, couverture
68,7 %), `docs/CONTRATS.md` § 1 (préfixes de constats, script de plancher ajouté aux
sources), `docs/ENVIRONNEMENT.md` (un build source n'embarque pas de FFmpeg, donc les
sondes mesurent celui du système), et `superview.yaml` (pourquoi le choix du binaire
n'est pas une clé de configuration).


---

## 3octies. Neuvième passe (2026-09-07) — signalement utilisateur : l'installation

Signalement : « en partageant mon projet à un utilisateur lambda, je lui ai demandé
qu'il utilise la dernière version de l'application (Windows). Il n'a pas été capable
de comprendre la partie installation. » Demande jointe : que le README existe en
français **et** en anglais.

La 8ᵉ passe avait corrigé ce que le README *affirmait* (U-06). Celle-ci porte sur ce
qu'il *demande de faire*, et sur le fait qu'un lecteur l'a suivi sans y arriver.

### U-07 ✅ — ~~Le README n'était pas suivable par son destinataire~~ — **CORRIGÉ**

Quatre défauts distincts, tous sur le chemin d'un premier lancement sous Windows.

| Passage | Ce qu'il faisait |
| --- | --- |
| Ordre des sections | `Quick Links`, `Overview`, puis un `Requirements` de treize lignes sur FFmpeg, NVENC et le pilote 610.00 — qui ne concerne que les compilations depuis les sources. L'installation n'arrivait qu'à la ligne 125. |
| Forme des instructions | `cd superview-<version>-windows-x86_64` puis `.\superview-gui-windows-amd64.exe`, en PowerShell. L'utilisateur visé double-clique ; il n'ouvre pas de terminal. |
| SmartScreen | **Rien.** `.github/workflows/release.yml` ne signe pas les binaires : Windows interpose donc « Windows a protégé votre ordinateur », une fenêtre qui ne propose que *Ne pas exécuter* et cache *Exécuter quand même* derrière *Informations complémentaires*. Le README n'en disait pas un mot. |
| Nom de l'archive | `superview-gui-<version>-windows-x86_64.zip`, alors que `<version>` est le **tag**, avec le `v` : l'asset réel est `superview-gui-v0.2.6-windows-x86_64.zip`. |

*Cause* — le README décrivait ce que fait un développeur, qui lance ses propres
compilations et n'a jamais vu un fichier marqué du web. Ce que le système
d'exploitation interpose entre le téléchargement et le premier lancement ne faisait
partie d'aucune section, parce que personne dans le projet ne le rencontre.

*Correctif* — `## Download and install` devient la première section H2, en six étapes
numérotées sans terminal, avec l'écran SmartScreen décrit avec les libellés exacts de
ses boutons et un `#### Why Windows warns about Superview` qui dit honnêtement
pourquoi (aucun certificat de signature) et ce qu'on peut vérifier à la place.
`## Requirements` disparaît, ses morceaux répartis entre l'installation, un nouveau
`### If something goes wrong` et `## Development`. Aucun numéro de version n'est plus
écrit : « the file ending in `-windows-x86_64.zip` » ne périme pas. Les avertissements
mesurés (winget 8.1.1, plancher 610) descendent en `## Development`, seul endroit où
quelqu'un tape `winget`.

*Défaut trouvé en chemin* — `gui_main.go` et `common/common.go` codaient en dur
`?tab=readme-ov-file#requirements`, affiché à l'utilisateur privé de FFmpeg.
Supprimer la section l'aurait déposé en haut du README, sans erreur. Constante
renommée `installURL`, et `readme_link_test.go` interdit désormais à ce fragment de
ne désigner aucun titre.

### U-08 ✅ — ~~La documentation n'existait qu'en anglais~~ — **CORRIGÉ**

Le dépôt s'adresse à un public qui n'est pas nécessairement anglophone, et la
personne qui n'a pas su installer l'application est francophone.

*Correctif* — `README_FR.md` à la racine, miroir complet, avec une ligne de sélection
de langue en tête des deux fichiers. Trois règles de traduction, écrites dans
`AGENTS.md` : les **libellés de l'interface restent en anglais** dans leur casse
exacte, l'interface n'étant pas traduite — traduire *Choose input file* enverrait le
lecteur chercher un bouton inexistant ; à l'inverse, les chaînes de **Windows** sont
celles qu'affiche un Windows français ; et les deux fichiers se modifient ensemble.

*Cause de fond* — la convention de langue du dépôt (journaux d'audit en français,
tout ce qui est vu de l'extérieur en anglais) n'était **écrite nulle part**, cas de
figure de L-47. Elle l'est maintenant, dans `AGENTS.md`.

*Ce qui garde le miroir* — le job `readme-parity` de `.github/workflows/lint.yml`
échoue quand une PR modifie un README sans l'autre, et quand leurs structures de
titres divergent. `.all-contributorsrc` liste désormais les deux fichiers, sans quoi
la PR du bot en toucherait un seul et échouerait à chaque nouveau contributeur.

---

## 3nonies. Dixième passe (2026-09-08) — signalement utilisateur : mémoire épuisée

Signalement : « Je viens de tester l'application sur un portable sous Kubuntu 26.04.1
LTS — i5-7200U, 8 Gio de RAM, Intel HD Graphics 620. Il y a eu le fallback CPU, puis un
crash de l'application car il manquait de mémoire vive. Détermine si c'est normal. »

Journal fourni (`~/.cache/superview/superview.log`, build `0.2.6 (3b9357c)`) et
`journalctl` du poste. La réponse est oui, c'est arithmétique — et trois défauts la
rendaient illisible.

### Ce que la machine a réellement fait

Clip d'entrée : `DJI_…_D.MP4`, HEVC, **3840×2880**, 43 s. Sortie remappée
**5120×2880**, soit 14,7 Mpx. Encodeur retenu : `libx265`, 4 threads, preset `medium`.

```
13:57:44  rustdesk déclenche l'OOM killer global
13:57:45  le noyau tue ffmpeg (pid 51447) — anon-rss 5 497 312 kB, total-vm 7 444 860 kB
13:57:45  systemd : « The kernel OOM killer killed some processes in this unit »
          → OOMPolicy (défaut : stop) arrête app-com.canaill51.superview@….service
          → SIGTERM à superview
13:57:45  le gestionnaire de signaux attrape le SIGTERM et journalise
          « encoding interrupted by user »
13:57:46  Failed with result 'oom-kill' — l'unité est démontée, la fenêtre disparaît
```

`systemd-oomd`, `earlyoom` et `nohang` sont tous `inactive` sur ce poste : c'est bien
le noyau, puis systemd.

### Mesures — pic de RSS de ffmpeg, géométrie reproduite à l'identique

Banc : maps produites par le vrai `GeneratePGM`, chaîne de `buildEncodeBaseArgs`,
`-threads 4` et `pools=4` pour simuler les 4 cœurs du poste.

| Chaîne | Pic |
| --- | --- |
| 8 bits, `yuv444p` → `yuv420p`, `libx265 medium` | 3,97 Gio |
| 8 bits, même chaîne, `libx264` | 3,99 Gio |
| **10 bits, `yuv444p10le` → `yuv420p10le`, `libx265 medium`** | **6,25 Gio** |
| 10 bits, `preset veryfast` | 3,69 Gio (8 bits) |
| relevé du noyau à l'instant du kill | 5,24 Gio, en cours de montée |

Deux conclusions. **Ce n'est pas x265** : à taille d'image égale, x264 demande autant.
C'est la géométrie — ≈ **0,27 Gio par mégapixel de sortie en 8 bits**, ≈ **0,43 en
10 bits**, le facteur 1,6 venant de la bascule 10 bits de `remapFilterChain`, qui est
délibérée (N-03) et juste. Et la sortie fait 4/3 des pixels de l'entrée par
construction. **Et l'ordre de grandeur est indépendant de la durée du clip** : ce sont
des tampons d'images, pas un flux accumulé.

Pour situer : une Hero 11/12/13 en 5,3K 4:3 (5312×3984) produit 7082×3984 = 28,2 Mpx,
soit ~7,6 Gio en 8 bits et ~12 Gio en 10 bits. Une machine à 16 Gio y passe mal.

### Ce que la sonde ne pouvait pas voir

`h264_vaapi` **n'apparaît pas** dans la liste des refus du journal, alors que tous les
encodeurs matériels annoncés sont sondés (`encodersToProbe`). Il a donc passé la sonde :
**le HD 620 savait encoder en H.264 sur ce poste.** Seule la famille HEVC a échoué —
`hevc_vaapi : No usable encoding entrypoint found for profile VAProfileHEVCMain (17)`,
signature d'un pilote `i965` là où `intel-media-va-driver` serait attendu.

La source étant du HEVC, `FindEncoder` n'a parcouru que `candidateEncodersForCodec("hevc")`,
n'y a trouvé aucun encodeur matériel utilisable, et est tombé sur `libx265`. Choisir
`h264_vaapi` réglait les trois problèmes d'un coup : encodage matériel, 8 bits, et
quelques minutes au lieu d'un quart d'heure.

### U-09 ✅ — ~~Un arrêt décidé par le système était rapporté comme une annulation de l'utilisateur~~ — **CORRIGÉ**

`ErrCancelled` couvrait deux choses sans rapport : la fermeture du canal `cancel`
(bouton *Cancel*, ou la boîte « cancel and quit » de la croix) et le gestionnaire de
signaux sur `SIGINT`/`SIGTERM`. Les deux passaient par un même canal d'entiers vides.
Conséquences, toutes vérifiées sur le journal du poste :

1. **Le message était faux.** « encoding interrupted by user » pour un arrêt que
   l'utilisateur n'a pas demandé — c'est ce qui a rendu le journal incompréhensible et
   a fait perdre le premier tour de diagnostic.
2. **La cause réelle n'était nulle part.** Aucune ligne ne portait l'état de la mémoire
   au moment de l'arrêt.
3. **Un `SIGKILL` du noyau sur ffmpeg produisait « ffmpeg failed: signal: killed »**,
   sans le mot mémoire, alors que c'est le seul diagnostic qui compte.
4. **L'application survivait au SIGTERM.** systemd arrêtait l'unité, l'application
   refusait de mourir, et le SIGKILL qui suit ne laisse rien nettoyer : le fichier de
   travail `.superview-partial-*.mp4` et le répertoire de session restaient derrière.

Corrigé : sentinelle `ErrStoppedBySignal` distincte, portant le nom du signal ;
`classifyFfmpegFailure` nomme un processus tué de l'extérieur comme un manque de
mémoire probable, avec le relevé `MemAvailable` ; la GUI quitte sur
`ErrStoppedBySignal`, après que `PerformEncoding` a déroulé ses nettoyages.

**Laissé délibérément** : un signal reçu *hors* encodage tue toujours le processus sans
un mot, le gestionnaire n'étant armé que pendant `run()`. Rien n'est alors en vol, et
armer un gestionnaire permanent ferait courir la fermeture de l'application contre les
nettoyages en cours.

### U-10 ✅ — ~~Le repli CPU ignore un encodeur matériel disponible dans l'autre famille~~ — **CORRIGÉ**

`h264_vaapi` était utilisable, `FindEncoder` ne l'a pas regardé : la liste parcourue
était `candidateEncodersForCodec("hevc")` et rien d'autre. Le critère « conserver le
codec de la source » l'emportait sur « utiliser le matériel de la machine », sans que
rien ne le dise à l'utilisateur.

**Arbitrage utilisateur du 2026-09-08 : basculer, et l'annoncer.** Le choix n'est pas
gratuit — passer une source HEVC 10 bits sur un encodeur H.264 matériel fait perdre les
10 bits, `remapFilterChain` ne les gardant que pour la famille HEVC (N-03). La bascule
se fait donc, et la ligne « Hardware: » de la fenêtre comme le journal disent ce qui a
été échangé, la perte des 10 bits comprise.

Corrigé en trois pièces :

1. `FindEncoder` ne bascule que depuis un encodeur **logiciel** — la liste étant
   ordonnée matériel d'abord, y atterrir signifie qu'aucun matériel de la famille source
   n'est utilisable — et seulement vers un encodeur **matériel** de l'autre famille.
   Échanger `libx265` contre `libx264` n'achèterait rien et coûterait le codec.
2. `describeCodecFamilySwitch` formule le coût en une clause, et ne mentionne les 8 bits
   que quand la source est effectivement en 10 bits.
3. **Le repli logiciel revient à la famille de la source.** La règle suivait la famille
   de l'encodeur échoué : « votre GPU ne sait pas faire du H.265, donc du H.264 sur le
   GPU » serait devenu « … donc du H.264 sur le processeur », c'est-à-dire un codec que
   personne n'avait de raison de vouloir une fois le GPU écarté. C'est ce qui rend la
   bascule sans regret : si le matériel refuse la vraie image, on atterrit où la machine
   serait allée sans elle.

**Non traité, et consigné en U-13** : la sonde encode du 256×256, ce qui ne prouve rien
sur une image de 5120×2880. Un encodeur peut passer la sonde et refuser la conversion.

### U-13 ✅ — ~~La sonde ne prouve rien sur la taille réellement encodée~~ — **CORRIGÉ**

`probeFrameSize` vaut `256x256`, et son propre commentaire le dit : « A probe at this
size cannot prove that a 4K frame will also encode: some encoders have an upper bound
too. » Le cas est concret — l'encodeur H.264 des Intel d'avant la 11ᵉ génération plafonne
à 4096 px de côté, et la sortie du signalement fait 5120 px de large. Un encodeur peut
donc être déclaré utilisable, être choisi, et échouer sur la vraie image : le coût est un
run ffmpeg complet avant que la cascade de repli ne s'en aperçoive.

Sonder à la géométrie réelle répond à la question qui compte, sans table de limites par
constructeur — c'est la même philosophie que U-03.

**Confirmé sur le matériel du signalement**, le 2026-09-09, en posant la question du
pipeline à la vraie taille :

```
Hardware does not support encoding at size 5120x2880
(constraints: width 32-4096 height 32-4096)
```

Le pilote **donne ses bornes**. Ce que la machine faisait avant ce correctif : sonde à
256×256 acceptée → `h264_vaapi` choisi → fenêtre annonçant H.264 et la perte des 10 bits →
deux démarrages ffmpeg refusés → repli sur `libx265`, dont le garde-fou mémoire avait
justement refusé de chiffrer le coût, puisqu'on lui avait montré un encodeur matériel.
Autrement dit, U-10 n'apportait rien à cette machine et sa promesse était fausse.

**Mesure préalable, comme demandé** : une sonde qui *réussit* coûte ~0,30 s et **la taille
n'y change rien** — 0,29 s à 256×256, 0,30 s à 5120×2880, 0,33 s à 7082×3984 sur
`h264_nvenc`. Ce qui se paie est le démarrage du processus et la poignée de main avec le
pilote, pas les pixels. Un refus revient en 0,02 s. Le coût est donc une constante par
conversion, pas une fonction de la résolution.

Correctif en trois pièces :

1. `probeArgs` et `probeEncoderAtSize` prennent la taille en paramètre. La sonde de
   démarrage garde 256×256 : quand elle tourne, aucun fichier n'est choisi.
2. `verifyEncoderForOutput`, appelée dans `PerformEncoding` **avant** le contrôle disque
   et le garde-fou mémoire, pose la question à la géométrie réelle et recule sur
   l'encodeur logiciel de la famille source si le matériel refuse. L'annonce, l'estimation
   mémoire et l'encodeur qui tourne deviennent alors la même chose.
3. La fenêtre ne promet plus ce que la machine refusera : `DescribeVerifiedHardwarePlan`
   recalcule la ligne « Hardware » hors du fil d'interface après le chargement d'un
   fichier, un changement de case *squeeze* ou de codec. Un cache par (encodeur, taille)
   rend gratuits les appels répétés ; un compteur de génération empêche une réponse lente
   d'écraser une plus récente.

### U-14 ✅ — ~~Le garde-fou mémoire n'était pas reconsulté sur le chemin de repli~~ — **CORRIGÉ**

Trouvé en instruisant U-13, et c'est un trou dans U-11 tel que livré. `checkMemoryForEncode`
n'était appelé qu'une fois, dans `PerformEncoding`, **avec l'encodeur choisi au départ**.
Pour un encodeur matériel il s'abstient — c'est correct, aucun n'a été mesuré. Mais la
cascade de repli vit à l'intérieur d'`EncodeVideo`, qui se rappelle elle-même avec
`libx265` : le garde-fou n'était jamais reconsulté, et la machine retombait sur les 6,25 Go
sans le refus censé l'en protéger.

Corrigé : le repli chiffre l'encodeur qui prend le relais avant de démarrer. La géométrie
vient de l'en-tête du `x.pgm` de la session (`sessionOutputSize`) plutôt que d'un recalcul
— `EncodeVideo` ne reçoit pas `squeeze`, et les cartes sont la déclaration que la
conversion fait elle-même de l'image qu'elle produit. Une lecture impossible ne bloque
rien, comme partout ailleurs ici.

Avec U-13, ce chemin ne devrait plus être atteint par une limite de taille ; il le reste
pour tout autre échec matériel à l'exécution.

### U-11 ✅ — ~~Rien ne vérifie la mémoire disponible avant un encodage~~ — **CORRIGÉ**

`checkTempSpaceForMaps` refusait de démarrer quand le tmpfs ne pouvait pas tenir 57 Mo
de cartes ; rien ne regardait les ~6 Gio que l'encodage allait demander, dont l'échec est
bien plus brutal. `checkMemoryHealth` existe mais ne teste qu'un seuil fixe de 1 Gio,
sans rapport avec la résolution, et n'est appelé que depuis le bouton *Diagnostic*.

`checkMemoryForEncode` est l'équivalent mémoire du contrôle disque, posé juste après lui
dans `PerformEncoding` — après le choix de l'encodeur, parce que le coût en dépend.

**Le chiffre vient du banc, pas d'un raisonnement** : 290 octets par pixel de sortie en
8 bits, 455 en 10 bits, mesurés sur la géométrie du signalement. Trois propriétés
justifient de le traiter comme une constante : il appartient à l'image et non au codec
(libx264 à 1 % de libx265 sur la même image), il ne croît pas avec la durée du clip, et
il dérive légèrement *vers le haut* sur les petites images, là où la valeur absolue ne
décide rien.

Trois choix à noter :

1. **Aucune estimation pour les encodeurs matériels.** Rien ici ne les a mesurés, et les
   tampons qui dominent le chiffre appartiennent à l'encodeur logiciel. Refuser sur un
   nombre inventé serait pire que ne pas vérifier.
2. **Un refus en dessous du besoin, un avertissement jusqu'à un quart au-dessus.**
   L'estimation est une moyenne mesurée ; refuser une machine qui s'en serait tirée est
   un échec en soi.
3. **Une lecture impossible n'est pas un refus** — même règle que le contrôle disque.
   Sans `/proc/meminfo`, donc sous Windows, la vérification ne se déclenche jamais.

`encodesInTenBits` est extrait pour que la chaîne de filtres et l'estimation décident de
la profondeur au même endroit : deux copies de ce test auraient fait dériver l'estimation
d'une moitié en plus sans que rien ne le signale.

### U-12 ✅ — ~~Bruit et seuils faux dans le journal~~ — **CORRIGÉ**

- `Failed to parse progress value raw_value=N/A` en `WARN`, **45 fois en 81 secondes** :
  `out_time_ms=N/A` est ce que ffmpeg émet avant la première image sortie. Ce n'est pas
  une anomalie, et pendant ces 81 s la barre est restée à 0 % — c'est vraisemblablement
  ce qui a motivé l'annulation du premier essai.
- `Insufficient temp disk space: 3.8 GB (minimum 10GB recommended)` : le seuil de 10 Go
  porte sur le répertoire temporaire, qui est un tmpfs dimensionné à la moitié de la
  RAM. Sur une machine de 8 Gio il est **structurellement insatisfiable** — `UNHEALTHY`
  à vie — alors que le besoin réel (57 Mo) est vérifié ailleurs et passe.
- Les événements `start`, `error` et `encoding failed` sont journalisés **en double**.
- Le commentaire de `EncodeVideo` est orphelin : il est posé au-dessus de
  `sourcePixelFormat`, que godoc documente donc avec le texte d'une autre fonction.

Les quatre corrigés :

1. **`out_time_ms=N/A` est la réponse normale de ffmpeg** tant qu'aucune image n'est
   sortie du graphe de filtres. Reconnu comme tel et journalisé en `Debug` ; une valeur
   qui n'est ni `N/A` ni un nombre reste un `WARN`, parce qu'elle voudrait dire que le
   format de progression a changé. Et la fenêtre dit désormais ce qu'elle attend
   (`Starting the encoder... (no progress until the first frame)`) au lieu d'afficher une
   barre à zéro.
2. **Le seuil disque passe de 10 Go à 1 Go**, avec un message qui dit à quoi sert le
   répertoire. Les cartes du plus grand format traité font 113 Mo ; c'est
   `checkTempSpaceForMaps` qui décide vraiment, en connaissant la géométrie.
3. **La journalisation en double venait de `main()`** : le logger est construit deux fois
   — une fois pour en avoir un, une fois le niveau configuré connu — et chaque
   construction *ajoutait* un gestionnaire. `SetObservabilityHandler` remplace au lieu
   d'ajouter, ce qui rend le défaut inexprimable ; `RegisterObservabilityHandler`, dont
   la sémantique d'ajout était le piège et que rien d'autre n'utilisait, est retirée.
4. Le commentaire orphelin est revenu au-dessus de `EncodeVideo`.

**Non fait, et assumé** : un compteur de temps écoulé pendant l'attente de la première
image. Le texte d'attente dit ce qui se passe ; un compteur vivant demanderait un
ticker, sa goroutine et son annulation, pour un gain de confort sur une phase qui dure
des secondes. À reprendre si un utilisateur le redemande.

---

## 3decies. Onzième passe (2026-09-11) — entretien : ce que `make` fait, comment on fusionne, ce que les documents comptent

Aucun signalement. Revue d'entretien du dépôt à `08dae40`, à la recherche de
résidus : code mort, dérive doc/code, cibles périmées. `go mod tidy` ne bouge rien,
aucun `TODO`, aucun helper de test dupliqué, aucun nom de fonction périmé cité dans
la prose. Deux constats, tous deux dans le `Makefile` — c'est-à-dire dans le seul
endroit du dépôt censé rendre les pièges difficiles.

### M-01 ✅ — ~~`make test` peut être vert sans avoir encodé une seule image~~ — **CORRIGÉ**

🔴 `make test` faisait `go test -v ./...` et `make coverage` `go test ./...
-coverprofile=…`, ni l'un ni l'autre ne posant `SUPERVIEW_REQUIRE_FFMPEG=1`.

C'est exactement le défaut que `skipWithoutFFmpeg` et le § *Verifying a change*
d'`AGENTS.md` décrivent, resté dans la commande qu'un contributeur tape réellement.
`test.yml` pose la variable depuis le chantier D ; le `Makefile` ne l'a jamais fait.
Le paragraphe d'`AGENTS.md` qui met en garde et la cible qui tombe dans le piège
cohabitaient depuis.

**Mesuré** — PATH reconstruit sans `ffmpeg` ni `ffprobe` (2 588 liens vers
`/usr/bin`, les deux outils exclus), `make test` :

| | Sauts silencieux | Échecs |
| --- | --- | --- |
| Avant | **15**, dont les 7 `TestIntegration_*` et `TestGeneratePGM_RemapOutputIsStable` | 2 |
| Après | **0** | 17 |

Les quinze deviennent des échecs nommés — `SUPERVIEW_REQUIRE_FFMPEG=1 but this test
cannot run: ffmpeg not installed`. Avec `ffmpeg` présent : 0 saut, 0 échec, 0 course.

`-race` et `-count=1` ajoutés au passage, pour que la cible soit la recette
d'`AGENTS.md` et non une approximation : l'encodage tourne dans une goroutine et
rend compte à l'interface par une autre, et un résultat vert mis en cache ne dit
rien de l'arbre qu'on a sous la main.

**Balayage prescrit par L-96, exécuté** — `grep -rn "go test" --include='Makefile'
--include='*.yml' --include='*.md' .` sur tout le dépôt : treize invocations. Les quatre
de `test.yml` et `release.yml` portent la variable par un `env:` d'étape, que le grep ne
montre pas mais qui est bien là. Une manquait, et à l'endroit qui compte —
[`docs/CONTRATS.md`](../CONTRATS.md) § *Vérification fonctionnelle réelle* donnait
`go test ./common -run TestIntegration -v` comme moyen de vérifier une conversion réelle,
c'est-à-dire la commande qui saute les sept tests en question et affiche `ok`. Corrigée
et rejouée : 7 PASS.

**Découvert en chemin, non corrigé** — `TestEncodeVideo_ProgressNotAvailableIsNotAWarning`
et `TestEncodeVideo_UnparsableProgressIsStillAWarning`
([`common/lognoise_test.go`](../../internal/common/lognoise_test.go)) remplacent
`commandStdoutPipe` mais appellent le vrai `EncodeVideo`, donc le vrai `ffmpeg` : sans
lui, elles échouent au lieu de se sauter, n'étant pas gardées par `skipWithoutFFmpeg`.
Ce sont les deux « échecs avant » du tableau. Troisième cas de la même famille, hors
du périmètre arbitré pour cette passe.

### M-02 ✅ — ~~`make clean` nettoie un répertoire qui n'existe pas et laisse ceux qui existent~~ — **CORRIGÉ**

🟡 La dernière ligne de la cible supprimait `dist/`, produit par rien dans ce dépôt —
aucune cible du `Makefile`, aucune étape du workflow de publication, et pas même une
entrée dans `.gitignore`. En face, elle ignorait tout ce que `fyne package` écrit dans
la racine : `fyne_metadata_init.go`, `*.syso`, `superview`, `superview.exe`,
`tmp-pkg/`, `superview-gui-*.tar.xz|zip`.

Ces noms-là ne sont pas cosmétiques : c'est la liste que R-06 a fait apprendre à
`.gitignore` une publication à la fois, parce que `cmd/go` lit `git status
--porcelain` pour décider de `vcs.modified`. Un fichier ignoré est caché, pas
absent — et la cible censée l'enlever ne le connaissait pas.

**Contre-épreuve** — les onze résidus fabriqués à la racine, plus `tmp-pkg/` ; après
`make clean`, `git status --porcelain --ignored` ne montre plus que le `Makefile`
modifié, et `superview.yaml` est intact.

### M-03 ✅ — ~~La règle de fusion d'`AGENTS.md` est fausse pour la PR du bas d'une pile~~ — **CORRIGÉ**

🟠 Relevé par l'utilisateur devant le bouton de fusion de #66 : « quelle option dois-je
choisir ? ». La question n'aurait pas dû se poser, et la réponse écrite était piégeuse.

`AGENTS.md` concluait : « The `base` field says which case you are in: if it is not
`master`, you are stacked. » **La PR du bas d'une pile a `master` pour base** et doit
pourtant passer par un commit de fusion, puisque le squash réécrit le SHA sur lequel la
suivante est posée. #59, #61 et #62 ont toutes été fusionnées par commit de fusion contre
ce que le fichier disait — le constat était donc déjà démontré par l'historique, et
consigné nulle part.

**Le vrai critère** — « une PR ouverte est-elle basée sur ma branche ? », pas « quelle est
ma base ». Une requête y répond :
`gh pr list --base "$(gh pr view <NN> --json headRefName -q .headRefName)" --state open`.

**Correctif, en deux temps.** La phrase est corrigée ; puis, l'arbitrage utilisateur étant
« que l'utilisateur n'ait pas à se poser la question »,
[`merge-on-label.yml`](../../.github/workflows/merge-on-label.yml) applique la règle : label
`merge` → interrogation de l'API, choix de la méthode, armement de la fusion, et un
commentaire sur la PR disant quoi et pourquoi. Le déclenchement reste manuel : l'utilisateur
décide **quand**, plus jamais **comment**.

**Mesuré, et contraire à l'intuition** — `mergeStateStatus` n'est pas fiable à la demande.
GitHub calcule la fusionnabilité paresseusement et a répondu `UNKNOWN` sur #66 pendant
plusieurs minutes et une douzaine de lectures, pendant que #67 répondait `CLEAN`. Le
workflow lit donc `UNKNOWN` comme « armer et attendre » plutôt que « fusionner maintenant »,
avec fusion directe seulement si GitHub refuse l'armement *parce qu'*il n'y a plus rien à
attendre.

**L'état du dépôt était plus faible que ce que la documentation laissait croire** :
`master` n'avait **aucune protection ni ruleset** — rien n'obligeait les douze contrôles à
passer avant une fusion, et rien n'empêchait le push direct que la règle « toute
modification passe par une PR » interdit sur le papier. Créé : un ruleset qui refuse le
push direct et impose sept contrôles. Contre-épreuve — commit vide poussé sur `master`,
refusé : *« Changes must be made through a pull request. 7 of 7 required status checks are
expected. »*

**Sept, et pas douze, délibérément** : `Coverage Report & Gate` et les deux `Native Build`
déclarent `needs: test`, donc les exiger exige transitivement les deux jobs de test **sans
nommer la version de Go**. Exiger `Test Go 1.26.x on ubuntu-latest` aurait fait qu'une
montée de version renomme un contrôle requis et bloque toute fusion, en silence.
Contournement administrateur limité aux PR (`can_bypass: pull_requests_only`) : pas de
push direct possible, mais pas de verrouillage possible non plus.

**Deux réglages retirent une sous-question au lieu de la documenter** : « Rebase and merge »
est retiré du menu — il n'écrit ni `Title (#NN)` ni commit de fusion et n'a jamais servi
ici — et le titre de squash est épinglé au titre de la PR. Il valait `COMMIT_OR_PR_TITLE` :
sur une PR à **un seul commit**, il prenait le sujet du *commit*, ce qu'`AGENTS.md` interdit
(« Commit messages do not appear there »). Défaut réel, jamais visible parce que les deux
textes ont toujours coïncidé jusqu'ici.

### M-04 ✅ — ~~L'en-tête de ce document et sa numérotation de sections avaient dérivé~~ — **CORRIGÉ**

🟡 Trouvé en corrigeant les deux chiffres périmés d'`AGENTS.md` : les mêmes symptômes ici,
en pire, parce que ce sont les lignes qui servent à **naviguer** le document.

| Ce qui était écrit | Réalité |
| --- | --- |
| « consigne **cinq** passes et les **67** constats » | onze passes, **105** constats — le compte n'a plus bougé depuis la 5ᵉ |
| « Dernière passe : **9ᵉ**, close le 2026-09-07 » | 11ᵉ, close le 2026-09-11 |
| Ordre de lecture s'arrêtant à § 3octies | § 3nonies et § 3decies manquaient |
| `## 5bis` placé **avant** `## 5` | *bis* veut dire « après » ; il était avant |
| Deux lignes « 11ᵉ passe » distinctes au § 4 | une passe, une ligne |
| `LECONS.md` : « 80 règles », « les 67 constats sont clos » | 98 règles, 105 constats |
| `LECONS.md` : légende des préfixes s'arrêtant à `R-xx` | `D/V`, `U` et `M` manquaient |

Le `5bis` n'était pas seulement inesthétique : le § 4 et le § 2 y renvoient, et le journal
des révisions doit rester la dernière section, comme dans
[`LECONS.md`](../LECONS.md). La renumérotation « Questions ouvertes » → § 5 et « Journal des
révisions » → § 6 remet l'ordre et les renvois d'accord sans déplacer le journal.

**Pourquoi personne ne l'avait vu** — rien ne compile un document. Les chiffres
d'`AGENTS.md` (`common.go` « ~1600 lines » alors qu'il en fait 2023, `main()` « ~540 »
pour 585) et ceux-ci ont vieilli exactement pareil : ils étaient vrais à l'écriture, et
aucune passe ne relit l'en-tête qu'elle périme. Le balayage prescrit par L-80 porte sur ce
que la doc *promet* ; celui-ci porte sur ce qu'elle *compte*.

Les deux dernières lignes viennent du balayage que prescrit L-98 — relire l'en-tête et le
sommaire du fichier qu'on vient d'allonger. `CONTRATS.md`, `ENVIRONNEMENT.md`,
`hardware-support.md` et les deux README ne portent aucun compte, donc rien à y dériver.

**Correctif** — chiffres refaits, sections renumérotées, renvois suivis, lignes du § 4
fusionnées, et l'en-tête porte désormais la phrase qui dit qu'il périme et qu'ajouter une
passe oblige à le rouvrir. Les chiffres d'`AGENTS.md` sont arrondis à la centaine (« ~2000
lines », « ~600 lines ») pour vieillir plus lentement que des valeurs précises et fausses.

---

## 3undecies. Douzième passe (2026-10-14) — entretien : la CI rouge sur trois PR qui n'y étaient pour rien

Aucun signalement. Les trois PR Dependabot ouvertes (#73, #74, #75) restaient bloquées,
toutes sur la même vérification obligatoire.

### M-05 ✅ — ~~`'1.26.x'` désignait la version en cache du runner, pas la dernière~~ — **CORRIGÉ**

🟠 `Vulnerability Scan` échouait sur les trois PR : `govulncheck` remontait neuf
vulnérabilités de la bibliothèque standard (GO-2026-6603 à GO-2026-6617, dans `net/http`,
`crypto/tls` et `net/textproto`), toutes corrigées dans Go 1.26.9. Le journal du job dit
pourquoi elles y étaient encore : `Setup go version spec 1.26.x` → `Found in cache @
/opt/hostedtoolcache/go/1.26.8`. Sans `check-latest`, `actions/setup-go` se contente d'une
version en cache qui satisfait le motif et ne consulte pas le manifeste, où 1.26.9 figurait
déjà.

| Ce qu'on croyait | Ce qui tournait |
| --- | --- |
| `go-version: '1.26.x'` = le dernier correctif de la branche 1.26 | celui de l'image du runner : 1.26.8 sous Linux comme sous Windows |
| La PR Dependabot a introduit le rouge | `master` était vert le 2026-10-06 ; les vulnérabilités sont publiées depuis, et n'importe quelle PR aurait rougi |
| Il faut aussi monter `golang.org/x/net` (v0.55.0, cinq des neuf identifiants le citent) | non : avec Go 1.26.9 et x/net inchangé, `govulncheck ./...` rend 0 vulnérabilité, mesuré en local |

Le même motif est utilisé par `release.yml` : **un binaire publié ce jour-là aurait embarqué
la bibliothèque standard vulnérable**, sans qu'aucune vérification ne le signale, puisque la
release ne lance pas `govulncheck`.

**Correctif** — `check-latest: true` sur les neuf étapes `setup-go` des trois workflows,
plutôt qu'un `go-version` épinglé à `1.26.9` : l'épinglage referait le même défaut au
prochain correctif de Go, en sens inverse. `ENVIRONNEMENT.md` télécharge 1.26.9.

**Trouvé en chemin, non traité ici** — sur la machine de vérification (Intel UHD Tiger Lake,
FFmpeg 8.0.1 système), `TestIntegration_DeviceFrameEncoderRunsTheWholePipeline` rougit :
le pilote VAAPI n'accepte que le mode de débit CQP, la sonde l'accepte parce qu'elle ne
demande aucun débit, et la conversion, qui en demande un, retombe sur le CPU. Le test est
sauté en CI, faute de GPU. À consigner dans sa propre PR.

### M-06 ✅ — ~~Les branches fusionnées par `merge-on-label.yml` ne sont pas supprimées~~ — **CORRIGÉ**

🟡 Relevé par l'utilisateur après la fusion de #76 : `origin/ci/go-check-latest` existait
encore, alors que le dépôt a `delete_branch_on_merge: true`.

| PR | Fusionnée par | Branche supprimée par |
| --- | --- | --- |
| #70, #72 | l'utilisateur, à la main | le réglage du dépôt |
| #74, #75 | `github-actions[bot]` | Dependabot, qui nettoie ses propres branches |
| #76 | `github-actions[bot]` | personne |

Depuis M-03, c'est `merge-on-label.yml` qui fusionne, avec son `GITHUB_TOKEN` — y compris
quand la fusion est seulement armée : GitHub la termine au nom de qui l'a armée. Et GitHub
ne déclenche aucun de ses automatismes sur ce que fait ce jeton. #76 est la première PR
fusionnée par le workflow que Dependabot ne possédait pas ; le défaut existait depuis M-03,
masqué par le fait que les seules PR concernées jusque-là nettoyaient derrière elles.
Établi sur cette seule observation, cohérente avec le tableau, et non sur une documentation.

**Correctif** — un second pas dans `merge-on-label.yml` attend la fusion (au plus une heure)
et supprime la branche par l'API. Il s'abstient quand une PR ouverte est basée sur elle : le
réglage du dépôt aurait reciblé la PR du dessus sur `master`, alors que la suppression par
l'API la **ferme**. Il le dit alors en commentaire. Il s'abstient aussi pour une branche de
fork, que le jeton ne peut pas toucher, quand la fusion est désarmée, et tient une branche
déjà supprimée pour un succès.

---

## 3duodecies. Treizième passe (2026-10-14) — état des lieux complet, à `f925ee5`

Aucun signalement. Demande de l'utilisateur : une analyse complète du projet, de sa
structure et de ses tests, confrontée aux documentations officielles, avec un plan
d'actions — et un registre des décisions, qui manquait ([DECISIONS.md](../DECISIONS.md)).

**Méthode.** Baseline d'abord : `gofmt`, `go build ./...`, `go vet`, `golangci-lint`
(0 alerte), `govulncheck` (0 vulnérabilité appelée), puis
`SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` — **un seul test rouge**,
`TestIntegration_DeviceFrameEncoderRunsTheWholePipeline`, sur le défaut VAAPI/CQP déjà
consigné en M-05 ; couverture 71,1 % module entier, 86,0 % sur `common`. Puis six revues
indépendantes en parallèle (pipeline, GUI/Fyne, encodeurs/FFmpeg, utilitaires Go, CI/release
et outillage agent, suite de tests), chacune tenue de citer la documentation officielle et de
vérifier par exécution ce qui pouvait l'être, sur FFmpeg 8.0.1, Fyne 2.8.2 (code lu dans le
cache de modules) et Go 1.26.9. Chaque constat ci-dessous a été **relu contre les journaux**
avant d'être retenu, et les plus lourds ont été **reproduits une seconde fois** par
l'auteur de la passe (E-02 par un test jetable, E-06 par l'API GitHub, E-07 à E-09 par
`ffmpeg` en direct, E-17 par un test jetable).

Convention : **E** (état des lieux). Les constats sont **ouverts** ; le § 4 les reprend. Le
plan d'actions est en fin de section.

### Ce qui s'est révélé sain

Les revues ont testé, et écarté, une bonne part des hypothèses de départ. Pour qu'on ne les
refasse pas : l'ordre lecture du pipe / `Wait` et le drain de stderr (os/exec), le tueur de
processus sur les deux plateformes, le parseur `-progress` (`out_time_ms` est bien en
microsecondes sur 8.0.1, vérifié : `out_time_us` et `out_time_ms` impriment la même valeur),
le renommage sur un même système de fichiers, l'arithmétique de `GeneratePGM` pour 4:3,
16:9, 1:1 et 21:9, l'en-tête PGM, l'idempotence de `CleanUp`, les positions de
`-init_hw_device`, `-hwaccel`, `-threads` et `-filter_threads`, le repli doux des `-hwaccel`
vaapi et cuda sur une source non décodable, l'étape `format=yuv444p` (sortie identique au
bit près avec ou sans, gardée parce qu'elle interdit un choix `gbrp` qui, lui, change les
pixels), le plafond x265, le cache de résolution des outils, `exec.LookPath` et `ErrDot`,
la recherche du YAML, les huit `uses:` épinglés par SHA (tous vérifiés contre leur tag),
`merge-on-label.yml` (pas de checkout, données d'événement par `env:`, label réservé aux
collaborateurs), les archives FFmpeg épinglées par URL **et** SHA-256, `fyne.Do` jamais
appelé depuis le fil principal, les sentinelles `ErrCancelled` transportées par `%w`, les
variables injectées des tests toutes restaurées, et les scripts ffmpeg factices qui ont bien
leurs variantes `.bat`.

### E-01 — La sonde pose une question plus étroite que l'encodage, et le test qui doit les comparer ne compare que deux options

🔴 `common/probe.go:130-142`, `common/common.go:1291-1350`, `common/probe_test.go:65-91`.
C'est le défaut VAAPI/CQP trouvé en M-05, qui reçoit ici son identifiant, et il est plus
large que VAAPI. La sonde émet les options de périphérique, `format=nv12,hwupload` et
`-c:v` ; l'encodage émet **en plus** `-b:v`/`-maxrate`/`-bufsize`, `-preset`, `-threads`
et, pour une source 10 bits vers HEVC, la chaîne `yuv420p10le`/`p010`.
`TestProbeAndConversionAskTheSameQuestion` ne compare que `hwDeviceArgs` et la présence
de `hwupload`. Trois conséquences établies : (a) un pilote VAAPI qui ne sait que CQP passe
la sonde et refuse l'encodage (`Driver does not support any RC mode compatible with
selected options (supported modes: CQP)`), l'interface annonce `h264_vaapi` et tout tourne
sur le CPU — mesuré sur la machine de vérification, et `h264_vulkan` a le même `rc_mode
auto` ; (b) un `video_preset` que NVENC ne connaît pas (E-08) n'est pas vu par la sonde ;
(c) un GPU qui sait l'HEVC 8 bits mais pas le Main10 (Skylake, Maxwell, VCE 3.x) passe la
sonde 8 bits, échoue l'encodage 10 bits et retombe sur `libx265` — le scénario OOM exact
d'U-13.

Doc : ffmpeg-codecs.html § *VAAPI encoders* — « `rc_mode` … A given driver may only
support a subset of modes » ; l'ordre de `auto` dans `vaapi_encode.c` est : avec un débit,
CBR puis AVBR, VBR, CBR ; **sans** débit, ICQ puis CQP. Avec un débit, `auto` ne retombe
jamais sur CQP. Vérifié : `-rc_mode CQP -b:v …` est accepté mais le débit est ignoré
(« Buffering settings are ignored in CQP RC mode ») — **une consigne de débit ne peut pas
être honorée quand seul CQP existe** ; `-qp` est le seul réglage.

**Arbitrage attendu de l'utilisateur** (déjà posé le 2026-10-14) : **(a)** la sonde pose la
vraie question — extraire `encoderRateArgs(bitrate)`, `encoderPresetArgs` et le choix de
format d'un seul endroit, utilisé par `probeArgs` et `buildEncodeBaseArgs`, et faire passer
le `pix_fmt` source à la sonde de taille ; les pilotes CQP-only sont alors refusés au
démarrage avec les mots de ffmpeg dans *Diagnostic*, et la machine passe à l'autre famille
ou au CPU ; **(b)** l'encodage s'adapte — relancer en `-rc_mode CQP -qp N` quand ffmpeg dit
`supported modes: CQP`, au prix d'un débit demandé qui devient fictif et d'un `N` sans
correspondance principielle. **Recommandation : (a)**, qui est L-93 appliquée ; (b) plus
tard, seulement comme option « qualité constante » explicite. Note pour le README : un
iHD qui n'annonce que CQP est presque toujours un HuC non chargé (`i915.enable_guc=2`).

### E-02 — Un signal du système ne court-circuite pas la cascade : six ffmpeg pour un seul SIGTERM, et le signal se perd

🔴 `common/common.go:1602`, `:1631`, `:1647`. Les trois portes de repli testent
`errors.Is(err, ErrCancelled)` et rien d'autre. Le chemin signal rend `fmt.Errorf("%w
(signal: …)", ErrStoppedBySignal)`, qu'aucune porte ne reconnaît : `runWithAudioFallback`
relance en AAC, le bloc `-hwaccel` « retombe sur le décodage CPU », le bloc matériel
recourt à l'encodeur CPU — chaque fois un ffmpeg neuf avec un `signal.Notify` neuf. Si une
relance aboutit, `EncodeVideo` rend **nil** : le signal est perdu, le `app.Quit()` de
`gui_main.go:894` ne tire jamais, et la fenêtre affiche *Transform complete*.

**Reproduit deux fois.** Par un test jetable (ffmpeg factice qui pend, `safe_performance`,
`h264_nvenc` + `cuda`, un seul `os.Interrupt` injecté par le `signalNotify` de test, puis un
par ffmpeg relancé) : **6 lancements** pour une conversion signalée une fois. Par un
programme autonome avec un vrai `syscall.Kill(SIGTERM)` depuis le rappel de progression :

| mode | encodeur | lancements | erreur rendue |
| --- | --- | --- | --- |
| `safe` | `libx264` | 1 | `ErrStoppedBySignal` — **le seul cas que le test existant épingle** |
| `safe` | `h264_nvenc` | 2 | nil |
| `safe_performance` (le YAML livré) | `libx264` | 2 | nil |
| `safe_performance` | `h264_nvenc` | 2 | nil |

Scénario : c'est l'incident d'U-09 lui-même — systemd arrête l'unité après un OOM, envoie
SIGTERM, l'application relance ffmpeg, SIGKILL après `TimeoutStopSec`, fichier de travail et
répertoire de session laissés derrière : le point 4 d'U-09, que le correctif donnait pour
clos. `TestEncodeVideo_SystemSignalIsNotReportedAsUserCancellation` utilise `nil` comme
config, `libx264` et aucun accélérateur : l'unique configuration sans relance. Il est vert
à vide pour la configuration livrée (L-37). Même famille que P-03.

**Correctif** — un prédicat `isStopRequest(err)` = `ErrCancelled` ∨ `ErrStoppedBySignal`,
employé aux trois portes ; un test de comptage de lancements sur le modèle de
`TestEncodeVideo_CancellationStopsTheFallbackCascade` avec `h264_nvenc`/`cuda` et
`safe_performance` (contre-épreuve : retirer le prédicat → 6).

### E-03 — Le bouton *Quit* contourne l'interception de fermeture, et Fyne quitte sur SIGTERM avant que le pipeline ait nettoyé

🔴 `gui_main.go:1186-1188`, `:725-748`, `:736-739` ;
`fyne.io/fyne/v2@v2.8.2/internal/driver/glfw/driver_desktop.go:278-284`. `quitBtn` appelle
`app.Quit()` directement ; la confirmation « Cancel and quit » ne vit que dans
`SetCloseIntercept`, que Fyne n'invoque que pour la demande de fermeture du système
(`window.go:284-291`) ; `fyneApp.Quit` passe par `window.Close()` → `driver.Quit()` sans
jamais la traverser. `Run()` rend, `main()` rend, le processus sort. Rien ne lie ffmpeg à
son parent (`command-other.go:8` est vide, pas de `Pdeathsig` ; Windows ne pose que
`HideWindow`), et les `defer` de `PerformEncoding` (`os.Remove(partialOutput)`, `CleanUp()`)
ne tournent pas. Scénario : lancer un 4K, cliquer *Quit* — la fenêtre disparaît, ffmpeg
continue jusqu'au bout dans `.superview-partial-*.mp4`, le répertoire de session reste.
Viole le contrat « l'annulation ne laisse rien derrière ». `quitBtn` n'est pas non plus dans
`state.locked`.

Deuxième moitié, vérifiée dans le code de Fyne : `catchTerm` enregistre son propre
`signal.Notify(SIGINT, SIGTERM)` et appelle `fyne.Do(d.Quit)` aussitôt. Go livre le signal à
**tous** les canaux enregistrés, donc quand systemd envoie SIGTERM, Fyne démonte la boucle
pendant qu'`EncodeVideo` tue encore ffmpeg et que `PerformEncoding` retire encore le fichier
partiel. U-09 ne raisonnait que sur son propre gestionnaire. Et le `time.Sleep(1 s)` de
l'interception avant `app.Quit()` est une estimation, pas une attente : la sonde de
`verifyEncoderForOutput` qui précède l'encodage a un délai de 15 s.

**Correctif** — `beginEncoding` rend aussi un canal `done` fermé par la goroutine ;
`window.SetOnClosed` (exécuté de façon synchrone dans `window.Close()`, que `app.Quit` et
`catchTerm` traversent tous deux) appelle `requestCancel()` et attend `done` avec une borne.
Un seul crochet règle le bouton, le `Sleep` et le chemin signal de Fyne. Accessoirement,
`quitBtn` dans `state.locked` ou passant par la même fonction de confirmation.

### E-04 — Un renommage final qui échoue **supprime** l'encodage terminé

🔴 `common/common.go:1997-2003`, `:1971-1981`. `os.Rename` échoue → `keepPartial` reste
faux → le `defer` efface `partialOutput`, c'est-à-dire des minutes ou des heures de travail ;
l'erreur dit seulement « failed to move the finished file ». Avant P-04, ffmpeg échouait à
ouvrir la destination dans la première seconde ; le correctif a déplacé l'échec à la fin et
l'a rendu destructeur (L-105). Cas où `Rename` échoue dans le même répertoire : la
destination est un répertoire (doc `os.Rename` : « If newpath already exists and is a
directory, Rename returns an error » ; reproduit : `EISDIR` — `isValidOutputPath` ne vérifie
que le parent) ; sous Windows la destination est ouverte par un lecteur, l'aperçu de
l'Explorateur ou un antivirus sans `FILE_SHARE_DELETE`, ou en lecture seule ; répertoire à
sticky bit avec une destination d'un autre propriétaire (`EPERM`).

**Correctif** — à l'échec du renommage, `keepPartial = true` et nommer le fichier partiel
dans l'erreur ; rejeter dès `isValidOutputPath` une destination qui est un répertoire.

### E-05 — Une source plus haute que large sort pliée, sans erreur

🔴 `common/common.go:905-913`, `:1032-1038`. Le commentaire de `putMapSample` affirme que
« pour toute taille d'entrée la distorsion atterrit dans [0, inputWidth−1] ». La carte X
non-squeeze vaut `f(x) = x − d/2 ∓ tx²·d/2` et n'est monotone que si `d ≤ outX/2`, soit
`H/W ≤ 9/8`. Rien ne rejette les autres géométries (`Validate` ne vérifie que `> 0`).
Mesuré avec le vrai `GeneratePGM` sur 1080×1920 : carte 3412×1920, échantillon X maximal
**1164** pour une largeur de 1080, **915 colonnes ≥ largeur, 919 colonnes écrêtées à 0**.
Doc du filtre `remap` : « If mapping values are out of range, zero value for pixel will be
used » — bandes noires et un ruban miroir à chaque bord, sans erreur ni avertissement.
`TestGeneratePGM_SqueezeMapStaysWellFormed` ne borne que la carte squeeze.

**Correctif** — refuser (`InvalidVideoError`) une source `H/W > 9/8` en mode non-squeeze,
avec un message qui dit pourquoi ; ajouter le test de propriété jumeau pour la carte
non-squeeze (bornes, monotonie, centre). *Décision produit à confirmer* : refuser, ou
pivoter ? Refuser est le choix sûr.

### E-06 — Plus aucun workflow ne tourne sur `master` depuis que le workflow fusionne

🔴 `.github/workflows/test.yml:4-5`, `lint.yml:4-5`, `merge-on-label.yml`. Doc : « events
triggered by the `GITHUB_TOKEN` will not create a new workflow run »
(docs.github.com, *Triggering a workflow*). Vérifié par l'API : le dernier run `push` sur
`master` est `801660d` (#72, 2026-10-06, fusionné à la main) ; les cinq fusions suivantes
(#73 à #77) n'ont produit **aucun** run. Le ruleset a
`strict_required_status_checks_policy: false` : une PR testée contre une base périmée
fusionne, et le commit de `master` qui en résulte n'est jamais testé. L-100 énonçait le
mécanisme (« ni suppression de branche, ni workflow ») et seule la première conséquence a
été corrigée (M-06).

**Correctif** — activer « Require branches to be up to date before merging » sur le ruleset
(la vérification de PR porte alors sur le résultat fusionné), ou fusionner avec un jeton
d'application GitHub plutôt que `GITHUB_TOKEN` dans `merge-on-label.yml` (le contournement
documenté). Le premier est une case à cocher ; le second rétablit aussi la suppression de
branche native et les runs sur `master`.

### E-07 — Deux jetons de décodage matériel ne sont pas des `-hwaccel` : `qsv` livre des trames GPU à `remap`, `drm` échoue toujours

🟠 `common/hardware.go:150-151`, `:163-164`, `:168-172`. Doc ffmpeg.html, `-hwaccel` :
« qsv — Unlike most other values, this option does not enable accelerated decoding (that is
used automatically whenever a qsv decoder is selected), but accelerated transcoding, without
copying the frames into the system memory. » Vérifié : `-hwaccel qsv -i clip` imprime
`Selecting decoder 'h264_qsv' because of requested hwaccel method qsv` et `defaulting
hwaccel_output_format to qsv` — les trames décodées sont des trames `qsv`, et une trame
matérielle entrant dans le graphe CPU de `remap` échoue durement (vérifié avec l'équivalent
cuda : `Impossible to convert between the formats …`, code 218). Sur une machine Intel où
QSV fonctionne, le niveau 1 de la cascade échoue donc **à chaque conversion** dans le graphe
de filtres, le niveau 2 réussit, et le journal porte un WARN « Hardware decode path failed ».
CONTRATS (« les trames redescendent en RAM ») est vrai pour vaapi/cuda/d3d11va, faux pour
qsv. Et `drm` n'est pas un `-hwaccel` mais un type de périphérique : `-hwaccel drm` →
`Device creation failed: -14`, code 242 ; sur un Raspberry Pi (seul endroit où v4l2m2m passe
la sonde) chaque conversion gaspille un lancement.

**Correctif** — retirer `qsv` des candidats de décodage QSV (garder d3d11va/dxva2) ;
`accelForEncoder` rend `""` pour `_v4l2m2m`, et `canUseEncoderWithProfile` n'exige alors
plus d'accélérateur pour lui ; un test qui interdit `qsv` et `drm` comme jetons de décodage.

### E-08 — Un `video_preset` hors `fast`/`medium`/`slow` tue NVENC et bascule en silence sur le CPU ; et « Fast » sur NVENC est le preset le plus grossier

🟠 `common/common.go:1384`, `common/config.go:263`, `gui_main.go:122-129`, `README.md:268`.
`mapVideoPresetForEncoder` passe les noms x264 tels quels à nvenc/vaapi/vulkan/v4l2m2m/
d3d12va. Mesuré sur `h264_nvenc` : `default slow medium fast hq ll p4` → 0 ;
`ultrafast superfast veryfast faster slower veryslow placebo` → `[Eval] Undefined constant
or missing '(' in 'veryslow'`, **code 234**, avant tout appel au pilote. La cascade tourne
alors niveau 1, niveau 2, et atterrit sur `libx264` avec le résumé « h264_nvenc failed; used
CPU encode ». La GUI n'émet que `fast`/`medium`, le fichier de configuration émet les dix
noms que le README annonce. Pour VAAPI/Vulkan/v4l2m2m, un `-preset` inconnu n'est qu'un
avertissement (code 0).

Second point, mesuré par empreinte de flux : sur NVENC `fast` = `p1` (« fastest, lowest
quality ») et `medium` = `p4`. Le profil « Fast » est donc **plus grossier** sur NVIDIA,
contrairement à ce que dit le README (« quicker, not coarser », DEC-10).

**Correctif** — une table propre à `_nvenc` (`ultrafast/superfast/veryfast → p1`,
`faster → p2`, `fast → p3`, `medium → p4`, `slow → p5`, `slower → p6`,
`veryslow/placebo → p7`) et `""` pour les familles à trames sur périphérique ; la sonde de
E-01 (a) attrape ensuite tout nouveau cas. *Décision produit* : `fast → p3` rend la promesse
du README vraie sur NVENC.

### E-09 — Les sorties HEVC sont taguées `hev1`, que les lecteurs Apple refusent

🟠 `common/common.go:1291-1350` — aucun `-tag:v hvc1`. Vérifié : `… -c:v libx265 out.mp4`
→ `codec_tag_string=hev1`. QuickTime, Photos iOS et Final Cut ne lisent que `hvc1`. Le cas
le plus dur est la bascule de famille (DEC-06) : un clip H.264 devenu HEVC pour atteindre le
GPU donne un fichier que le téléphone de l'utilisateur n'ouvre pas — il a été prévenu de la
bascule, pas du tag.

**Correctif** — `-tag:v hvc1` dès que `isHEVCEncoder(encoder)` ; un test d'intégration
qui lit `codec_tag_string` sur une sortie HEVC.

### E-10 — Le fichier de sortie est créé en mode 0600 au lieu de 0644

🟠 `common/common.go:1744`. `os.CreateTemp` : « The file is created with mode 0o600 (before
umask) » (pkg.go.dev/os). ffmpeg `-y` tronque l'inode existant, `rename` conserve le mode.
Reproduit : création 0600, après ffmpeg 0600, après renommage 0600 ; ffmpeg créant lui-même
le fichier → 0644. Régression de P-04 : une sortie dans un dossier partagé, un export NAS
ou une bibliothèque de serveur multimédia est illisible pour tout autre utilisateur.
Windows non concerné (ACL héritées).

**Correctif** — `os.Chmod(name, 0o666 &^ umask)` après `CreateTemp`, ou
`os.OpenFile(…, O_CREATE|O_EXCL, 0o666)` sur un nom aléatoire ; un test Unix qui lit le mode.

### E-11 — La CI ne lance jamais le détecteur de courses, ne teste jamais le FFmpeg livré, et dépend d'un `choco install` sans relance

🟠 `test.yml:70,108`, `release.yml:178,184` : quatre `go test ./...` **sans `-race`**,
alors que le `Makefile` (`TEST_FLAGS := -race -count=1`, « the suite is only meaningful under
the detector ») et `AGENTS.md` l'exigent ; une course de la classe P-02 passe la CI.
`release.yml:173,405-422` : la suite tourne contre le `ffmpeg 6.1.1` d'apt (journal) et le
`9.0.2` de choco, jamais contre les 8.1.1/n8.1.2 téléchargés deux étapes plus loin —
`SUPERVIEW_FFMPEG_DIR` existe exactement pour cela. `test.yml:57` : `choco install ffmpeg`
sans version ni relance, dans le chemin critique du contrôle obligatoire ; le run
38249403781 (2026-10-12) a échoué sur un 504 (déjà noté en M-05, sans remède).

**Correctif** — `-race` sur les invocations Linux (Windows exige une chaîne C) ; dans
`build-gui-linux`, après « Fetch and verify », `SUPERVIEW_REQUIRE_FFMPEG=1
SUPERVIEW_FFMPEG_DIR="$RUNNER_TEMP/ffbin" go test ./...` ; sous Windows, télécharger
l'archive gyan.dev 8.1.1 déjà épinglée par SHA plutôt que choco.

### E-12 — Le sélecteur natif Windows casse sur tout chemin non ASCII

🟠 `gui_native_dialog_windows.go:40,56`. Le chemin sort par `[Console]::Out.Write`, que
Windows PowerShell 5.1 encode avec `Console.OutputEncoding`, « determined by the system
locale » (learn.microsoft.com, `Console.OutputEncoding`) — page de code OEM (850 sur un
Windows français), pas UTF-8. Go lit les octets en UTF-8, `é` devient U+FFFD, `CheckVideo`
ne trouve pas le fichier → *Status: Invalid input* sur un fichier que l'utilisateur vient de
cliquer. `C:\Users\Cédric\…` le déclenche. Plausible (mécanisme documenté, non exécutable
ici) ; le fichier est « non testé, assumé » depuis la 1ʳᵉ passe.

**Correctif** — écrire par un flux UTF-8 explicite :
`$w = New-Object IO.StreamWriter([Console]::OpenStandardOutput(), (New-Object
Text.UTF8Encoding $false)); $w.Write($dialog.FileName); $w.Flush()`.

### E-13 — Un échec tardif relance jusqu'à cinq encodages complets

🟠 `common/common.go:1592-1699`. La cascade ne distingue pas « a échoué à l'ouverture » de
« a échoué à 95 % ». Un disque de destination plein, un fichier d'entrée corrompu à la fin,
une coupure réseau sur un NAS : chaque cause fait rejouer la relance audio, le repli de
décodage et le repli d'encodeur, **chacun un encodage complet** qui échouera de la même
façon. Pour un 4K de vingt minutes, c'est une heure et demie perdue avant le message.
Cousin de E-02 : la cascade n'est juste que pour les échecs d'ouverture.

**Correctif** — relancer seulement si l'échec est survenu avant la première progression
(le rappel sait quand ffmpeg a produit une trame) ; sinon rendre l'erreur telle quelle.
Un test avec un ffmpeg factice qui progresse puis échoue : 1 lancement, pas 6.

### E-14 — Une annulation est rapportée comme un échec, et *Diagnostic* reste cliquable pendant un encodage

🟠 `gui_main.go:901-907` : toute erreur, `ErrCancelled` compris, écrit *Status: Failed*,
*Results: last run failed* et ouvre un dialogue **Error** (« encoding interrupted by
user »). U-09 a corrigé le message, pas la branche. `gui_main.go:1154-1184` :
`diagnosticBtn` n'est pas dans `state.locked`, écrit *Running diagnostic…* puis *Ready* —
pendant la phase « Starting the encoder… » (jusqu'à une minute) la fenêtre dit *Ready* alors
que ffmpeg travaille, et il écrase *Cancelling…* ; et `CheckHealth` lance
`ProbeHardwareSupport`, dix sondes GPU **en parallèle de l'encodage matériel**, ce que
CONTRATS interdit (« deux sondes en parallèle sur le même GPU peuvent faire échouer un
encodeur qui fonctionne ») : le rapport que le README demande d'attacher peut montrer un
NVENC fonctionnel comme refusé.

**Correctif** — `errors.Is(err, common.ErrCancelled)` → *Status: Cancelled*, pas de
dialogue ; `diagnosticBtn` dans `state.locked` ; restaurer le statut seulement si
`!state.isEncoding()`.

### E-15 — La sélection d'encodeur est lue dans le widget depuis la goroutine, et une sélection programmée écrase la préférence sauvegardée

🟠 `gui_main.go:375`, `:1088-1111`. `GUIHandler.GetEncoder` lit `h.encoder.Selected` depuis
la goroutine de `PerformEncoding` (`common.go:1843`) pendant que le fil principal peut
l'écrire par `refreshEncoderOptions` → `SetSelected` quand la sonde de démarrage ou de
relance atterrit (`widget/select.go:333-341`). Doc Fyne, *goroutines* : « Any time your app
invokes Fyne APIs from a goroutine your code created, you should use the fyne.Do function. »
Course (fenêtre étroite, après `CheckVideo`), et l'encodeur utilisé peut différer de celui
affiché au moment de *Start* — le débit et la case squeeze, eux, sont capturés sur le fil UI.
Second point : `refreshEncoderOptions` appelle `SetSelected(options[0])` quand le choix
sauvegardé est absent, et `OnChanged` le persiste. Un démarrage sans ffmpeg, ou avec un
pilote qui refuse `hevc_nvenc`, remplace définitivement « hevc_nvenc encoder » par « Use
same video codec… ».

**Correctif** — capturer `common.ParseEncoderSelection(encoder.Selected)` avant le
`go func()` ; ne persister que sur interaction utilisateur (drapeau autour des
`SetSelected` programmés).

### E-16 — Aucune vérification d'espace sur le disque de destination, et une estimation de temps restant absurde en début d'encodage

🟠 `common/health.go:142-143` dit « Checks both temp directory and output directory » ; seul
`os.TempDir()` est vérifié (`getFreeDiskGB` a deux appelants, tous deux sur le temporaire).
La destination reçoit `débit × durée / 8` — 100 Mb/s × 30 min = 22 Go — et rien ne le
compare au volume cible : ENOSPC après vingt minutes, fichier partiel retiré, erreur opaque.
Et `metrics.go:74-79` pose `StartTime` **avant** validation, génération des cartes et
l'attente documentée de la première trame (jusqu'à 81 s) ; `RecordProgress` calcule
`elapsed/percent×100` : première progression d'un clip de 600 s à 0,5 s (0,083 %) après 25 s
de préparation → « about 8h20m left » ; reproduit numériquement : 81 s à 0,000926 % →
2 429 h. Même gonflement après un repli matériel → CPU, qui repart de 0 % avec un `elapsed`
déjà grand.

**Correctif** — après `ValidateBitrate`, `getFreeDiskGB(filepath.Dir(outputFile))` contre
`bitrate × DurationFloat / 8` (+ marge audio), avertir ou refuser comme
`checkTempSpaceForMaps`, et corriger le commentaire ; dater la première progression non
nulle et calculer l'estimation depuis cette origine, ne rien afficher sous ~2 %.

### E-17 — Un parent symlinké contourne le contrôle des répertoires système ; Windows lit le total libre, pas l'espace disponible à l'utilisateur

🟠 `common/security.go:184-192` : `isSensitiveSystemPath(cleanPath)` s'applique au chemin
lexical, puis `os.Stat(dir)` suit le lien. **Reproduit** : `<tmp>/out → /usr`,
`isValidOutputPath(<tmp>/out/clip.mp4)` passe le contrôle système et n'est arrêté que par le
test d'écriture (« permission denied ») — donc **pas arrêté sous root**, le cas que le
commentaire des lignes 11-13 dit couvrir. Doc : `EvalSymlinks` « returns the path name after
the evaluation of any symbolic links », et échoue sur le fichier pas encore créé (vérifié),
d'où la nécessité de résoudre le **parent**. `common/health_disk_windows.go:16-20` rend
`freeBytes` (4ᵉ argument, total libre) au lieu de `freeBytesAvailable` (1ᵉʳ, « available to
the user who is associated with the calling thread … If per-user quotas are being used, this
value may be less ») ; le côté Unix emploie correctement `Bavail`.

**Correctif** — `resolvedDir, _ := filepath.EvalSymlinks(dir)` puis relancer
`isSensitiveSystemPath(filepath.Join(resolvedDir, base))` ; rendre `freeBytesAvailable`.
Et un test qui exerce enfin le contrôle système (E-19).

### E-18 — L'envoi Codecov échoue à chaque run et rien ne le montre

🟠 `test.yml:75-82`. Journal du job 112144479507 (master, 2026-10-06) : `Token length: 0`
… `Upload queued for processing failed: {"message":"Token required - not valid tokenless
upload"}`, étape conclue `success`. Doc Codecov : un jeton est requis « if the upload is
for a commit on a protected branch ». `fail_ci_if_error: false` + `continue-on-error: true`
le cachent ; pas de secret `CODECOV_TOKEN`, pas d'`id-token: write` pour l'OIDC.

**Correctif** — secret `CODECOV_TOKEN` (ou `use_oidc: true` + `id-token: write`),
`fail_ci_if_error: true`, retirer `continue-on-error` — ou supprimer l'étape et le badge.

### E-19 — Six trous de la suite qui laisseraient passer une régression

🟠 Vérifiés sur le profil de couverture de la baseline (`go tool cover -func`).

| Trou | Preuve | Régression qui passerait |
| --- | --- | --- |
| Aucun test ne rejette un répertoire système en sortie | `isSensitiveSystemPath` 63,6 % : la boucle tourne, ne rend jamais vrai ; `sensitiveSystemDirectories` 20 %, `containsPath` 0 % ; le cas `/etc/output.mp4` de `TestIsValidOutputPath` est attrapé par le contrôle `..` d'abord | supprimer l'appel ; une faute dans la branche Windows (`EqualFold`), jamais exécutée sur aucun runner |
| Le chemin audio de la configuration **livrée** n'est pas testé | `runWithAudioFallback` branches `copy` et relance AAC à 0 % : tous les tests passent `nil` (→ `safe` → `aac`) ; l'assertion `launches == 3` de `fallbackmemory_test.go:126` n'est vraie que pour la config que personne ne livre | relance supprimée ; relance tirée sur annulation ; E-02 |
| Deux tests `appState` passent sur la ligne d'attente | `newTestAppState` laisse `encoderProbe` nil → `refreshHardwareStatus` écrit « checking which encoders… », qui satisfait `HasPrefix("Hardware: ")` ; `DescribeHardwareAccelerationPlan` jamais atteint, `gui_main.go:521-523` à 0 % | passer le libellé brut de la liste au plan (« not in approved list ») |
| `TestGUIHandler_GetSqueezeAndBitrate` affirme « the GUI never enables squeeze today » | faux depuis C-04 (`gui_main.go:805-814`) ; et squeeze ne traverse jamais `PerformEncoding` (`integrationUI.GetSqueeze` → false) | `PerformEncoding` ignorant `GetSqueeze()` pour les cartes tout en l'utilisant pour la sonde (L-43) |
| `lognoise_test.go:28,101` exigent ffmpeg sans garde | confirmé : avec un `PATH` sans ffmpeg, FAIL en 0,00 s au lieu de SKIP (report de la 11ᵉ passe) | aucun, mais le contrat L-60 est rompu |
| Chemins jamais exercés | `findWindowsToolBinary` 0 % sur **les deux** runners (choco n'installe dans aucun candidat ; les deux tests de résolution sautent sous Windows) ; `PerformEncoding` 69,6 % — toutes les sorties précoces sauf mémoire/même fichier, **et l'échec du `Rename` final** ; `CheckFfmpeg` branches d'erreur ; refus de `checkTempSpaceForMaps` ; `GUIHandler.Show*` 0 %, `showPrerequisiteDialog` 0 %, `verifyHardwareInBackground` 0 % ; surcharges d'environnement de `config.go` à moitié ; pas de test Windows du contrat « aucun ffmpeg orphelin » | `return nil` après une erreur enregistrée ; fichier partiel laissé à l'échec du renommage (E-04) ; `ShowProgress` divisant par le mauvais facteur |

Les dix tests à ajouter, avec leur contre-épreuve, sont dans le plan (item 5).

### E-20 — zenity « unexpected error » est pris pour une annulation, et les sélecteurs natifs bloquent la boucle Fyne

🟡 `common/gui_helpers.go:19` : code 255 → `("", nil)`. zenity(1), *EXIT CODES* : 0 OK,
1 annulé/fermé, **−1 erreur inattendue** (255 non signé), 5 délai. Un zenity cassé donne un
clic sans effet avec un journal en `debug`, et le repli Fyne, qui existe pour ce cas, ne
s'ouvre jamais. `gui_native_dialog_linux.go:19`, `_windows.go:17` : `cmd.Output()` tourne
dans le rappel du bouton, sur la goroutine UI ; pendant que zenity/PowerShell est ouvert la
fenêtre ne repeint pas et ne répond pas aux pings du compositeur ; PowerShell ajoute 1 à 2 s
de gel au clic.

**Correctif** — seul le code 1 vaut annulation ; lancer le sélecteur dans une goroutine et
livrer le chemin par `fyne.Do`.

### E-21 — Clés YAML inconnues ignorées en silence, `min > max` accepté, `SUPERVIEW_ENCODER_CODECS` non épuré

🟡 `common/config.go:165` : `yaml.Unmarshal` sans `KnownFields(true)` (doc yaml.v3 :
« KnownFields ensures that the keys in decoded mappings to exist as fields in the struct »).
Vérifié : `max_bitrat: 5` → nil, champ intact. Ce projet a déjà perdu un `video_preset`
utilisateur (C-05) ; `video-preset:` le refait sans bruit. `min_bitrate > max_bitrate` est
accepté au chargement ; l'écrêtage de `gui_main.go:853-858` produit alors un débit que
`ValidateBitrate` refuse à chaque run (« below minimum »). `config.go:206` :
`strings.Split(encoders, ",")` sans `TrimSpace` — `"264, 265"` donne `" 265"`, qui ne
correspond à aucun nom, et tous les encodeurs HEVC disparaissent de la liste ;
`encoder_codecs: []` est accepté et vide la liste.

**Correctif** — `yaml.NewDecoder` + `KnownFields(true)` ; rejeter `min > max` ; épurer et
ignorer les vides ; tests pour chaque cas (E-19).

### E-22 — Quatre latences : verrou récursif, chemin relatif Windows, plafond du journal, mémoire du cgroup

🟡 `metrics.go:187-191`, `239-243` : `RLock` puis `ElapsedTime()` qui reprend `RLock` —
doc `sync.RWMutex` : « It should not be used for recursive read locking; a blocked Lock call
excludes new readers ». Inatteignable aujourd'hui (le lecteur de progression ferme `readDone`
avant `Wait`), interblocage garanti au premier bouton « résumé en direct ». `common.go:183-186` :
`filepath.Join(os.Getenv("LOCALAPPDATA"), …)` avec une variable vide donne un chemin
**relatif**, que `os.Stat` résout contre le répertoire courant — ce que Go refuse dans
`exec.LookPath` depuis 1.19. `observability.go:38-46` : le plafond de 5 Mio n'est appliqué
qu'à l'ouverture ; en `debug`, deux lignes par tick de progression (~1,8 Mo/h) ; et
`0o644`/`0o755` rend lisible à tous un journal qui porte les chemins du dossier personnel.
`health.go:199-205` : `MemAvailable` de `/proc/meminfo` décrit l'hôte, pas un cgroup limité
(`MemoryMax=`), où l'OOM d'U-11 reviendrait.

**Correctif** — `elapsedLocked()` partagé ; sauter un candidat dont la variable est vide ou
exiger `IsAbs` ; un `io.Writer` qui recoupe périodiquement, `0o600`/`0o700` ; lire
`/sys/fs/cgroup/memory.max` quand il existe.

### E-23 — Quatre écarts aux contrats écrits

🟡 `SessionError` est documenté pour « problems with temporary directory management » mais
`InitEncodingSession` et `getSessionPaths` rendent des `errors.New`/`fmt.Errorf` ; seul
`checkTempSpaceForMaps` le construit. `hardware.go:500-517` met en cache ce que rend
`probeEncoderAtSize`, et `summarizeProbeFailure` traite `context.Canceled` comme un refus :
tous les appelants passent `context.Background()` aujourd'hui, mais le premier contexte
annulable démoterait un encodeur qui fonctionne pour toute la session. `gui_main.go:766,787,
843` et `:804-806,877` : `selectedQualityProfile` et `squeezeSource` sont des variables
capturées par closure dans `main()`, que CONTRATS interdit (`squeezeSource` double
`state.squeeze`). `FyneApp.toml` n'a pas `[Migrations] fyneDo = true` : Fyne garde alors son
chemin de déduplication hérité qui masque un usage comme E-15.

**Correctif** — envelopper en `&SessionError{}` ; ne pas mettre en cache si `ctx.Err() != nil` ;
replier les deux variables dans `appState` ; ajouter les deux lignes de migration.

### E-24 — Hygiène de la CI et de la release, sept points

🟡 `test.yml:31-39` double le cache que `setup-go` fait déjà (les deux sont restaurés dans le
journal ; la clé manuelle embarque le littéral `1.26.x`). `release.yml:46-47` accorde
`contents: write` à tous les jobs, y compris ceux qui exécutent du code tiers (`fyne@v1.7.2`,
cgo) avec le jeton persisté dans `.git/config` par `actions/checkout`. `test.yml:125-140` :
le commentaire de couverture avec `pull-requests: write` ferait échouer le contrôle
obligatoire sur une PR de fork (jeton en lecture seule), et poste un commentaire par push.
`release.yml:178` puis `:184`, et `test.yml` `test` puis `coverage-report` : la suite tourne
deux fois pour un même profil. `release.yml:733,735` interpole le tag dans `run:` (le job
`prepare` fait déjà bien par `env:`). `release.yml:340` dit « the October 2024 one still
answers today » pour un pin `autobuild-2026-08-31` ; `AGENTS.md` dit « pinned to 8.1.1 »,
vrai pour Windows seulement (Linux : BtbN n8.1.2). Dependabot ne couvre ni `fyne@v1.7.2`,
ni `GOLANGCI_LINT_VERSION`, ni `GOVULNCHECK_VERSION`, ni les URL FFmpeg.

**Correctif** — supprimer l'étape de cache et poser `cache-dependency-path: go.sum` ;
`contents: read` en tête, `write` sur `create-release` seul ; condition `head.repo.full_name
== github.repository` + `continue-on-error` sur le commentaire ; publier `coverage.out` en
artefact et en faire le gate ; passer le tag par `env:` ; corriger les deux commentaires et
lister les pins manuels dans `RELEASING.md`.

### E-25 — L'outillage agent : une justification fausse, et aucun `.claude/`

🟡 `CLAUDE.md:4` et `CONTRATS.md` § 1 ligne 11 disent que « Claude Code ne lit pas
`AGENTS.md` ». Doc (code.claude.com/docs/en/memory) : Claude Code **peut** lire `AGENTS.md`
comme instructions de projet ; l'import reste le bon choix parce que « in some sessions
Claude can't read `AGENTS.md` », mais la raison écrite est fausse. L'import `@AGENTS.md` est
correct (chemins relatifs au fichier, quatre sauts maximum) et 129 lignes tiennent sous les
200 recommandées. Aucun `.claude/` : pas de liste d'autorisations pour les commandes de
vérification (chaque session redemande `go test`, `golangci-lint`…), pas de crochet
`gofmt` après édition (alors que `gofmt -l` est un gate CI), pas de compétence `/verify` qui
rende explicites les deux échecs silencieux qu'`AGENTS.md` décrit en prose (en-têtes GUI
absents, `SUPERVIEW_REQUIRE_FFMPEG` absent).

**Correctif** — cette passe ajoute `.claude/settings.json` (autorisations, refus de `git
tag`/`gh release create`/`git push --force`, crochet), `.claude/hooks/gofmt.sh` et
`.claude/skills/verify/SKILL.md`, et reformule les deux phrases.

### E-26 — Un test rouge n'a jamais bloqué une fusion

🔴 Trouvé après la clôture des 25 constats, en lisant la CI des PR #85 à #87 : toutes trois
ont fusionné avec un job Windows rouge. `Coverage Report & Gate`, le check requis qui porte le
verdict des deux jobs de test, déclarait `needs: [test, test-windows]` sans condition ; quand
l'un échoue, le gate est **sauté**, et GitHub compte un check requis sauté comme réussi. Avant
la séparation Linux/Windows, la matrice avait le même trou : depuis M-03 et la fusion par
label, aucun test rouge n'a jamais empêché une fusion.

**Correctif** (PR #88) — `if: always()` sur le gate, et une première étape qui échoue si l'un
des deux résultats n'est pas `success`. **Prouvé** sur la PR même : Windows rouge → gate
rouge. Aucun autre check requis n'a de `needs:` (ruleset relu, sept contextes).

### E-27 — Quatre défauts Windows masqués par E-26

🟠 Révélés dès que le gate a pu rougir :

1. **Production** — `cappedLogWriter` vidait un fichier ouvert en `O_APPEND`, ce que Windows
   refuse (pas de `FILE_WRITE_DATA`) : le plafond du journal ne s'appliquait jamais sous
   Windows. Le fichier est rouvert en `O_TRUNC`.
2. **Tests** — l'isolation du cache ne posait que `XDG_CACHE_HOME`, que Windows ignore : les
   tests du journal écrivaient dans le vrai cache du runner et lisaient les lignes les uns des
   autres. `LocalAppData` est posé aussi.
3. **Tests** — `TestOpenLogFile_TruncatesOversized` laissait sa quatrième ouverture sans
   `Close` ; sous Windows le nettoyage de `t.TempDir` ne peut pas supprimer un fichier ouvert.
4. **Tests** — le sous-test « copy refused, AAC accepted » ne lançait rien : `t.TempDir`
   reprend le nom du test, et `cmd.exe` coupe le chemin du `.bat` factice à la virgule
   (« `'…AACcopy_refused' is not recognized` »). Le vrai ffmpeg est un `.exe`, non concerné.
   Sous-test renommé, et la fixture refuse désormais un répertoire contenant `,` `;` `=`
   avec un message qui dit quoi faire.

### Plan d'actions

Ordonné par ce que coûte l'absence de correction, groupé par PR pour que chaque PR tienne
en une phrase de titre. Les items marqués **(décision)** attendent un arbitrage de
l'utilisateur avant d'être codés ; les autres sont des corrections sans alternative.

| # | PR (titre proposé) | Constats | Décision ? |
| --- | --- | --- | --- |
| 0 | *Record the thirteenth review pass, the decision register and the agent tooling* | E-25, DECISIONS.md, L-101 à L-106 | — (cette PR) |
| 1 | *Stop the encode when the system says so, and never delete a finished file* | E-02, E-13, E-04, E-10 | non |
| 2 | *Make Quit, the window close and SIGTERM all wait for the encode to unwind* | E-03, E-14, E-15, E-23 (closures, fyneDo) | non |
| 3 | *Ask the probe the same question as the encode, and map presets per encoder* | E-01 (a), E-08, E-07, E-09 | **E-01 a/b · E-08 `fast→p3`** |
| 4 | *Run CI on master again, under the race detector, against the shipped FFmpeg* | E-06, E-11, E-18, E-24 | **E-06 ruleset strict ou jeton d'application** |
| 5 | *Close the test gaps a regression would walk through* | E-19 (dix tests, chacun avec sa contre-épreuve) | non |
| 6 | *Refuse what the pipeline cannot do, and say how long it will take* | E-05, E-16, E-17, E-21, E-12 | **E-05 refuser ou pivoter** |
| 7 | *Housekeeping* | E-20, E-22, E-23 (reste), DEC-33 | **DEC-33 `safe` ou `safe_performance`** |

Les dix tests de la PR 5, chacun ~25-70 lignes : rejet des répertoires système (table sur
`sensitiveSystemDirectories()` + un voisin `/etcetera/x.mp4` qui doit passer) ; relance
audio en `safe_performance` (2 lancements, le second en `aac`, et 1 sur annulation) ;
`refreshHardwareStatus` avec rapport de sonde et liste déroulante ; squeeze à travers
`PerformEncoding` (largeur de sortie = largeur source) ; échec du `Rename` final sans
fichier partiel laissé (destination devenue répertoire pendant l'encodage) ; sorties
précoces de `PerformEncoding` enregistrées avec leur `stage` ; `ShowProgress`/
`ShowProgressDetail` sur de vrais widgets ; `findWindowsToolBinary` sur une arborescence
winget factice (tourne aussi sous Linux) ; surcharges d'environnement invalides ignorées et
YAML malformé en erreur ; propriétés de la carte non-squeeze (bornes, monotonie, centre) et
identité de la carte Y. Chaque test se prouve par la contre-épreuve indiquée dans le rapport
de la revue (retirer l'appel, inverser le signe, changer `/100` en `/1000`…).

**Deux questions ouvertes** consignées au § 5 : Q-02 (le garde-fou mémoire est chiffré à
`-threads 4` alors que l'encodage tourne à `NumCPU()` jusqu'à 16) et Q-03 (la télémétrie
GoPro `gpmd` est perdue par `-map "[v]" -map "0:a?"`).

---


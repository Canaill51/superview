# Superview — Analyse complète du projet

> **Chemins.** Ce journal date d'avant le 2026-10-14 (DEC-39) : `common/` y désigne
> aujourd'hui `internal/common/`, et les fichiers `gui_*.go` de la racine sont dans `cmd/superview/`.
>
> **Journal d'audit, pas documentation de référence.** Ce fichier consigne quatorze
> passes d'analyse et les 162 constats qu'elles ont produits, avec ce qui a été
> décidé pour chacun. Ce sur quoi on peut s'appuyer aujourd'hui est dans
> [CONTRATS.md](CONTRATS.md) ; ce qu'il faut avoir lu avant de corriger est dans
> [LECONS.md](LECONS.md) ; ce qu'il ne faut pas « corriger » est dans
> [DECISIONS.md](DECISIONS.md).
>
> Dernière passe : 14ᵉ (F-01 à F-30), ouverte le 2026-10-20 — analyse complète du produit
> Rust après v0.3.0, plan d'exécution en sessions ; **30 constats ouverts**, deux questions
> ouvertes, Q-04 et Q-05 (§ 5). La 13ᵉ (E-01 à E-27) est close.
> Les § 3 et § 3bis portaient sur `e3269e7`, le § 3ter sur `001d250`, le § 3duodecies
> sur `f925ee5`, le § 3terdecies sur `4efae3a`.
>
> **Ordre de lecture** : § 3 (1ʳᵉ passe, statique) → § 3bis (3ᵉ passe, empirique,
> N-xx) → § 3ter (4ᵉ passe, P-xx) → § 3quater (5ᵉ passe, R-xx) → § 3quinquies
> (6ᵉ passe, D-xx/V-xx) → § 3sexies (7ᵉ), § 3septies (8ᵉ), § 3octies (9ᵉ) et
> § 3nonies (10ᵉ), U-xx → § 3decies (11ᵉ) et § 3undecies (12ᵉ), M-xx → § 3duodecies
> (13ᵉ), E-xx → **§ 4 (état d'avancement, le seul tableau qui fasse foi)**.
>
> **Ce compte et cette liste périment dès qu'une passe s'ajoute**, et ils l'ont
> fait : le total est resté à « cinq passes, 67 constats » de la 5ᵉ à la 10ᵉ
> (M-04). Ajouter une passe, c'est aussi rouvrir ce paragraphe.

---

## 1. Vue d'ensemble

| Élément | Valeur |
| --- | --- |
| Module Go | `superview` |
| Version Go ciblée | `1.26.0` |
| Nature | Application **GUI uniquement** (Fyne v2.8.1) |
| Plateformes officielles | Windows, Linux (Ubuntu 24.04 LTS+) |
| Dépendance runtime | FFmpeg + FFprobe (binaires externes) |
| Lignes de Go | 10 254, dont 5 652 de tests (55 %) — mesuré à `a672bc6` |
| Licence | voir `LICENSE` |

### Rôle fonctionnel

Superview convertit une vidéo 4:3 en 16:9 par **distorsion dynamique** : les bords sont étirés
agressivement, le centre conserve son ratio. L'implémentation génère deux cartes de remappage
au format PGM P2 (`x.pgm`, `y.pgm`) puis les passe au filtre `remap` de FFmpeg.

### Cartographie des fichiers

Comptes de lignes mesurés à `ec7d753` (la CI qualité couvre `./...`, cf. X-01) :

```
/                            paquet main (GUI) — couverture 25,8 %
├── gui_main.go             1140 l. — fenêtre Fyne, orchestration, état d'encodage
│                                     dont ~540 l. dans main() : construction de
│                                     widgets, délibérément non découpée (P-10)
├── gui_main_test.go         990 l.
├── gui_native_dialog_linux.go    81 l. — zenity / kdialog       (non testé, assumé)
├── gui_native_dialog_windows.go  60 l. — PowerShell WinForms    (non testé, assumé)
│
common/                      paquet métier — couverture 83,8 %, vert sous -race
├── common.go               1699 l. — pipeline complet (ffprobe, PGM, ffmpeg, session)
│                                     et résolution des binaires : override, copie
│                                     empaquetée, PATH (U-04)
├── observability.go         391 l. — bus d'événements + dernier état publié
├── config.go                308 l. — Config YAML + surcharges SUPERVIEW_*
├── probe.go                 308 l. — sonde d'encodeurs à l'exécution (U-03)
├── health.go                304 l. — diagnostics système, branché sur le bouton Diagnostic
├── metrics.go               296 l. — métriques d'encodage
├── hardware.go              277 l. — classification des encodeurs, montage périphérique (U-05)
├── security.go              231 l. — validation de chemins, whitelist encodeur
├── gui_helpers.go            40 l. — helpers parsing GUI
├── command-{windows,other}.go     — SysProcAttr HideWindow
├── health_disk_{unix,windows}.go  — espace disque libre par plateforme
└── testdata/ffprobe/              — sorties ffprobe enregistrées (12 cas)

.github/scripts/
└── nvenc-driver-floor.sh          — lit le plancher pilote dans un binaire FFmpeg (U-04)
```

> Couverture module **68,7 %** pour un seuil CI de 50 %.

> `common.go` porte à lui seul un tiers du code de production et mêle au moins six
> responsabilités : découverte des binaires externes, logger, interfaces UI, cycle de
> vie de la session temporaire, géométrie des cartes PGM, orchestration ffmpeg. Le
> découpage en `pgm.go` et `tools.go` est identifié, non fait, et sans urgence : le
> fichier est couvert à 81,5 % et `pgm_golden_test.go` fige déjà la géométrie.

### Flux principal

```
main() ──► ResolveConfigPath() ──► LoadConfig()   ✅ exécutable / ~/.config / cwd (B-02)
       ──► CheckFfmpeg(cfg)               → version / accels / encoders (compilation)
       ──► goroutine ──► ProbeHardwareSupport() → encode une trame par encodeur (exécution)
                          └─► ApplyEncoderProbe() retire les encodeurs qui ont refusé  (U-03)
       ──► [UI] choix entrée ──► CheckVideo()  → VideoSpecs + Validate()
       ──► [UI] choix sortie ──► ensureMP4Extension()
       ──► [UI] profil qualité ──► copie locale de *Config   ✅ plus de global (C-05)
       └─► goroutine ──► common.PerformEncoding(cfg, ..., cancelEncoding)   ⚠️ lecture en course (P-02)
                          ├─ isValidInputPath / isValidOutputPath      (security.go)
                          ├─ CheckVideo                                (ffprobe)
                          ├─ SanitizeEncoderInput                      (whitelist)
                          ├─ ValidateBitrate                           (min/max)
                          ├─ FindEncoder ──► AnalyzeMachineProfile     (hardware.go)
                          ├─ InitEncodingSession  (os.MkdirTemp)
                          ├─ GeneratePGM          (x.pgm / y.pgm)
                          ├─ EncodeVideo          (ffmpeg + cascade de repli)
                          └─ defer CleanUp        (RemoveAll temp)
```

### Cascade de repli de `EncodeVideo`

1. Décodage matériel (`-hwaccel cuda|d3d11va|dxva2|qsv`) + encodeur matériel
2. → échec : décodage CPU + encodeur matériel
3. → échec : **récursion** avec l'encodeur CPU équivalent (`libx264` / `libx265`)
4. Chaque niveau retente en interne avec `-c:a aac` si `-c:a copy` échoue (mode `safe_performance`)

Le résultat effectif est publié via `SetLastHardwareAccelerationSummary()` et affiché dans la GUI.

---

## 2. Ce qui est solide

- **Séparation `main` / `common` nette.** Le pipeline est agnostique de l'UI grâce à
  l'interface `UIHandler`. La logique est testable sans Fyne.
- **Gestion de session temporaire correcte.** `InitEncodingSession` / `CloseEncodingSession`
  sous mutex, `os.MkdirTemp` (pas d'écriture dans le répertoire courant), `defer CleanUp()`
  systématique dans `PerformEncoding`.
- **Types d'erreur du domaine** (`InvalidVideoError`, `EncoderError`, `SessionError`) et
  enveloppement `%w` cohérent dans la plupart des chemins.
- **Détection matérielle non figée** : aucune liste blanche de GPU en dur, la capacité réelle
  de FFmpeg fait autorité. Le mapping des presets par famille d'encodeur
  (`mapVideoPresetForEncoder`) évite de passer `-preset medium` à un encodeur AMF.
- **Injection de paramètres FFmpeg verrouillée** : `SanitizeEncoderInput` valide contre la liste
  effectivement retournée par `ffmpeg -encoders`, et tous les appels passent par
  `exec.Command` sans shell — pas de surface d'injection de commande.
- **CI substantielle** : build, tests multi-OS, couverture avec seuil à 50 % sur `./...`,
  `go vet`, `staticcheck`, `govulncheck`, `gofmt`, toutes les actions épinglées par SHA.
- **Tests nombreux sur `common/`** — 81,5 % de couverture, verts sous `-race`, 4 benchmarks,
  avec des points d'injection prévus pour la testabilité (`signalNotify`, `signalStop`,
  `commandStdoutPipe`). Le paquet `main` reste l'angle mort (P-10).

---

## 3. Constats

Convention : **B** bug/correction · **S** sécurité · **C** qualité/architecture ·
**X** build/CI/doc · **O** observabilité/performance · **T** tests.
Sévérité : 🔴 haute · 🟠 moyenne · 🟡 basse.

> **Marquage du statut — deux conventions, et c'est délibéré.**
>
> Ce § 3 **conserve les sévérités telles qu'évaluées à l'époque** : c'est l'analyse d'origine,
> pas un tableau de bord. Un constat n'y est restylé que s'il s'est révélé **faux**, parce que
> le laisser paraître valide induirait en erreur (B-03). Tous les constats de cette section sont
> corrigés ; leur pastille dit ce qu'ils pesaient, pas où ils en sont.
>
> Les § 3bis et § 3ter, plus récents, sont ceux qu'on ouvre pour connaître l'état courant : ils
> **marquent la correction dans le titre**, sous la forme
> `### X-nn ✅ — ~~titre barré~~ — **CORRIGÉ**`.
>
> **La source de vérité sur l'état d'un constat reste le § 4bis, « État d'avancement ».**
>
> Enfin, le préfixe **Q-xx** (§ 5) ne désigne **pas** un constat mais une **question
> ouverte** : quelque chose dont on ne sait pas encore si c'est un défaut. Ne pas les mélanger
> aux N-xx et P-xx, qui sont des défauts établis.
> Ces deux conventions n'étaient pas écrites, et quatre titres des § 3bis/3ter avaient dérivé en
> conséquence : corrigés le 2026-09-04, voir L-47.

### B — Bugs et correction

#### 🔴 B-01 — Boucle infinie sur erreur de lecture non-EOF du pipe de progression
`common/common.go:770-791`

```go
line, _, err := rd.ReadLine()
if err == io.EOF {
    logger.Debug("Encoding complete")
    break
}
// aucune autre erreur n'est traitée → on reboucle
```

Toute erreur autre que `io.EOF` (`os.ErrClosed`, `ErrUnexpectedEOF`, erreur de pipe) fait
reboucler indéfiniment : `readDone` n'est jamais fermé, le `select` en aval bloque pour
toujours, et la goroutine consomme 100 % d'un cœur. L'application se fige sans message.
La comparaison directe `err == io.EOF` ignore de surcroît un EOF encapsulé.

*Correction attendue* : sortir de la boucle sur **toute** erreur non nulle, en journalisant
le cas non-EOF ; utiliser `errors.Is(err, io.EOF)`.

#### 🔴 B-02 — Le fichier de configuration est introuvable en installation réelle
`gui_main.go:156` — `common.LoadConfig("superview.yaml")`

Le chemin est **relatif** : il est résolu depuis le répertoire de travail courant. Lancée
depuis un lanceur `.desktop`, un raccourci Windows ou le menu Démarrer, l'application a un CWD
arbitraire (`/` ou `%USERPROFILE%`). `superview.yaml` n'est alors jamais lu et toute la
configuration documentée dans le README est silencieusement ignorée — y compris
`performance_mode: safe_performance`, qui repasse donc à `safe` (cf. O-01).

*Correction attendue* : chercher dans l'ordre le répertoire de l'exécutable
(`os.Executable()`), puis `os.UserConfigDir()/superview/`, puis le CWD ; journaliser le
chemin retenu.

#### ⬜ B-03 — ~~Le repli de dialogue Fyne produit un chemin invalide sur Windows~~ — **INVALIDÉ**

**Ce constat était faux.** Vérification faite le 2026-09-04 dans le code de Fyne v2.7.3
(`storage/repository/parse.go`, `uri.go`) : `NewFileURI` stocke le chemin Windows **sans**
barre oblique initiale (`C:/Users/...`) et `String()` reconstruit `scheme + "://" + path`,
soit `file://C:/Users/...` — et non `file:///C:/...` comme je l'avais écrit. Retirer `file://`
donne donc `C:/Users/...`, chemin parfaitement valide sous Windows. Le décodage pourcent que
j'évoquais n'est pas nécessaire non plus : `String()` n'encode pas le chemin.

Le code d'origine était correct. Le remplacement par `file.URI().Path()` a tout de même été
appliqué, mais comme **simplification** (accesseur documenté plutôt que manipulation de
chaîne), pas comme correction de bug.

*Leçon* : ne pas déduire le comportement d'une bibliothèque de la forme canonique d'un
standard — lire son code. Voir [LECONS.md](LECONS.md) L-10.

#### 🟠 B-04 — Option `-threads` positionnée avant `-i` : elle configure le décodeur, pas l'encodeur
`common/common.go:627-632`

```go
baseArgs = append(baseArgs,
    "-threads", strconv.Itoa(encoderThreads),
    "-i", video.File, "-i", xPath, "-i", yPath,
    ...
    "-c:v", encoder, ...)
```

En FFmpeg, une option placée **avant** `-i` s'applique au fichier d'entrée suivant. Ici
`-threads` paramètre donc le nombre de threads de **décodage** du premier input, alors que
l'intention (nom de la variable, journal `slog.Int("threads", encoderThreads)`) est de régler
l'encodeur. Les threads d'encodage restent au défaut FFmpeg.

*Correction attendue* : déplacer `-threads` après `-c:v <encoder>`. À valider avec un vrai
encodage avant/après, l'impact sur le débit étant l'enjeu.

#### 🟠 B-05 — Option « seuil de fichier » : l'entrée `" encoder"` fantôme dans la liste déroulante
`gui_main.go:522-524`

```go
for _, enc := range strings.Split(ffmpeg["encoders"], ",") {
    encoderOptions = append(encoderOptions, enc+" encoder")
}
```

`strings.Split("", ",")` retourne `[""]`, pas une tranche vide. Quand FFmpeg est absent
(`ffmpeg` est `nil`) ou qu'aucun encodeur ne correspond aux `encoder_codecs` configurés, le
menu contient une option vide libellée `" encoder"`. La sélectionner donne un encodeur vide,
qui retombe silencieusement sur le codec d'entrée.

*Correction attendue* : filtrer les entrées vides (réutiliser la logique de `splitCSV`
dans `hardware.go:38`, qui fait déjà exactement cela).

#### 🟡 B-06 — Double fermeture possible du canal d'annulation → panique
`gui_main.go:186` (intercepteur de fermeture) vs `gui_main.go:266` (`requestCancel`)

L'intercepteur de fermeture fait `close(cancelEncoding)` **sans** remettre la variable à `nil`,
contrairement à `requestCancel`. Si l'utilisateur confirme « Cancel and quit » puis clique sur
le bouton *Cancel* dans la seconde qui précède `app.Quit()`, `requestCancel` referme un canal
déjà fermé → `panic: close of closed channel`.

*Correction attendue* : factoriser une unique fonction d'annulation idempotente
(mettre le canal à `nil` immédiatement après fermeture), ou utiliser `context.CancelFunc`.

#### 🟡 B-07 — Accès non protégés par index sur la sortie de FFmpeg
`common/common.go:276` et `common/common.go:305`

```go
ret["version"] = strings.Split(string(version), " ")[2]   // panique si < 3 champs
...
enc := strings.Split(encodersArr[i], " ")
... enc[2] ...                                            // panique si < 3 champs
```

Le parsing suppose un format de sortie FFmpeg précis, ainsi qu'un en-tête de `-encoders`
d'exactement 10 lignes (`for i := 10; ...`). Une build FFmpeg non standard (Termux, distribution
patchée, sortie localisée) fait paniquer l'application au démarrage plutôt que d'afficher une
erreur. Le saut de 10 lignes en dur peut aussi masquer ou dupliquer des encodeurs.

*Correction attendue* : vérifier les longueurs avant indexation ; remplacer le saut de
10 lignes par un scan jusqu'au séparateur `------`.

#### 🟡 B-08 — Erreurs de `Close()` ignorées sur les fichiers PGM
`common/common.go:477-478` — `defer fX.Close()` / `defer fY.Close()`

Les écritures étant directes (pas de `bufio`), le risque de perte est faible, mais une erreur
de `close()` (quota dépassé, disque plein détecté au flush) est avalée : FFmpeg recevra alors
une carte de remappage tronquée et produira une erreur incompréhensible.

### S — Sécurité

#### 🟠 S-01 — Le rejet de `..` est appliqué à la chaîne brute et rejette des chemins légitimes
`common/security.go:52` et `common/security.go:103`

```go
if strings.Contains(filePath, "..") {
    return fmt.Errorf("path traversal detected: %s", filePath)
}
```

Le test porte sur la chaîne entière, pas sur les composants du chemin. Sont rejetés à tort :
`/home/user/Vidéos/vacances..2024.mp4`, `/home/user/GoPro..raw/clip.mp4`, tout nom de dossier
contenant deux points consécutifs. Comme le chemin est déjà exigé absolu et que
`filepath.Clean` est appliqué juste après, ce contrôle n'apporte aucune sécurité
supplémentaire pour un chemin fourni par un sélecteur de fichiers natif.

*Correction attendue* : contrôler les composants via `strings.Split(cleanPath, string(os.PathSeparator))`
et ne rejeter que le composant exactement égal à `".."`.

#### 🟠 S-02 — Le rejet inconditionnel des liens symboliques bloque des usages normaux
`common/security.go:83`

Tout lien symbolique en entrée est refusé. Sous Linux c'est un cas courant et légitime :
`~/Vidéos` pointant vers un disque monté, un montage `/media/...` symlinké, un répertoire de
projet synchronisé. Le modèle de menace est ici faible (l'utilisateur choisit lui-même le
fichier via un dialogue natif — il ne peut pas s'attaquer lui-même), et le coût en
utilisabilité est réel.

*Correction attendue* : résoudre via `filepath.EvalSymlinks` et valider la **cible**, plutôt
que rejeter. À arbitrer avec le mainteneur : c'est un choix de politique, pas un bug.

#### 🟡 S-03 — Test d'écriture par création de fichier dans le répertoire de sortie
`common/security.go:135-137`

`isValidOutputPath` crée puis supprime `.superview_write_test` dans le répertoire cible.
Trois inconvénients : écriture d'un fichier parasite dans un dossier utilisateur, fenêtre
TOCTOU entre le test et l'écriture réelle par FFmpeg, et fichier résiduel si le processus est
tué entre `WriteFile` et `Remove`. `os.CreateTemp(dir, ".superview-*")` serait plus sûr,
ou simplement laisser FFmpeg échouer avec son propre message.

#### 🟡 S-04 — Liste des répertoires système Windows en dur sur `C:`
`common/security.go:15-22`

`C:\Windows`, `C:\Program Files`… sont codés en dur. Sur une installation dont le disque
système n'est pas `C:`, la protection ne s'applique pas. Utiliser `os.Getenv("SystemRoot")`
et `os.Getenv("ProgramFiles")` (déjà utilisés ailleurs, `common/common.go:73`).

#### 🟡 S-05 — Le paquet `main` échappe entièrement à l'analyse de sécurité de la CI
Cf. X-01. `govulncheck`, `staticcheck` et `golangci-lint` ne sont exécutés que sur `./common`.
`gui_main.go` et les deux fichiers de dialogue natif — dont celui qui construit et exécute
un **script PowerShell** — ne sont jamais analysés.

### C — Qualité, architecture, code mort

#### 🟠 C-01 — Le module `health.go` (277 lignes) n'est jamais appelé
`CheckHealth`, `LogHealth`, `GetHealthReport` n'ont aucun appelant hors tests. Le README
et les instructions Copilot présentent pourtant les diagnostics système comme une
fonctionnalité. Soit on le branche dans la GUI (un bouton « Diagnostic » serait utile,
surtout pour trier les rapports d'incident FFmpeg), soit on le supprime.

#### 🟠 C-02 — `EncodingOptions` : structure exportée totalement inutilisée
`common/common.go:174-182`. Aucune référence dans tout le dépôt, tests compris. Elle duplique
la signature de `EncodeVideo` et fait diverger la documentation de l'API réelle.

#### 🟠 C-03 — Trois options de configuration documentées mais sans effet
| Option | Statut |
| --- | --- |
| `min_video_width` / `min_video_height` | Déclarées, défaut 320×240, **jamais lues**. `VideoSpecs.Validate()` ne vérifie que `> 0`. |
| `quality_preset` | Lue depuis `SUPERVIEW_QUALITY_PRESET`, **jamais consommée**. La GUI applique ses propres profils « Fast »/« Balanced » codés en dur (`gui_main.go:286-296`). |

Ces options apparaissent dans `superview.yaml` livré au projet : l'utilisateur croit les régler.

#### 🟠 C-04 — La fonctionnalité « squeeze » est inatteignable
`GUIHandler.GetSqueeze()` retourne `false` en dur (`gui_main.go:112-114`). Toute la branche
`squeeze` de `GeneratePGM` (`common/common.go:517-529`), qui implémente le cas « 4:3 déjà
étiré en 16:9 » — typiquement les modes SuperView natifs GoPro — est du code mort côté
application. Soit exposer une case à cocher, soit documenter que c'est une API bibliothèque.

#### 🟠 C-05 — `SetConfig` mute l'état global et n'est jamais restauré
`gui_main.go:282-305`. À chaque démarrage d'encodage, une copie de `cfg` est modifiée
(`VideoPreset` forcé à `fast`/`medium`) puis installée globalement via `common.SetConfig`.
Conséquences : (a) la valeur `video_preset` du YAML utilisateur est **toujours écrasée**,
contredisant la documentation ; (b) le global n'est jamais restauré, donc l'état diverge après
le premier encodage ; (c) `currentConfig` et `logger` sont des globales non synchronisées lues
depuis la goroutine d'encodage — pas de course avérée dans le flux actuel (le bouton *Start*
est désactivé pendant l'encodage), mais la propriété n'est garantie par aucune structure.

*Correction attendue* : passer la configuration en paramètre de `PerformEncoding` plutôt que
par un global mutable.

#### 🟡 C-06 — API exportée sans appelant hors tests
`ValidateVideoFile`, `CreateDefaultConfig`, `GetHeader`, `EncodingMetrics.ToJSON`,
`RecordEncodingEvent`, `EventRecorder.GetEventHistory` / `ClearHistory`. Pour un module `main`
non publié comme bibliothèque, c'est de la surface d'API à maintenir sans bénéfice.
`GetHeader` en particulier formate un en-tête texte hérité de l'ancienne CLI supprimée.

#### 🟡 C-07 — Messages d'erreur non conformes aux conventions Go
`common/common.go:387`, `:734`, `:810`… : `"Error running ffmpeg, output is:\n%s"`,
`"Error starting ffmpeg..."`. La convention Go (et `staticcheck ST1005`) veut une minuscule
initiale et pas de ponctuation finale, car ces chaînes sont destinées à être enveloppées.
Non détecté parce que la famille `stylecheck`/`ST*` n'est pas activée (`.golangci.yml:12-19`).

#### 🟡 C-08 — `-1` comme code de sortie magique
`metrics.RecordError(-1, ...)` est répété 9 fois dans `PerformEncoding` pour signifier
« pas de code de sortie FFmpeg ». Une constante nommée clarifierait.

### X — Build, CI, outillage, documentation

#### 🔴 X-01 — La CI qualité ignore le paquet `main` (743 lignes, dont toute la GUI)
`.github/workflows/lint.yml` : `golangci-lint run ./common`, `go vet ./common`,
`staticcheck ./common`, `govulncheck ./common`.
`.github/workflows/test.yml` : `go test ./common`.

Le paquet racine — `gui_main.go` (602 l.), `gui_native_dialog_linux.go`,
`gui_native_dialog_windows.go` — n'est **ni vérifié, ni linté, ni analysé, ni testé**.
Seul `go build -v ./...` (workflow `go.yml`) le compile. C'est la cause structurelle qui
laisse passer B-03, B-05, B-06 et S-05.

*Correction attendue* : passer à `./...`. Attention : cela exige les dépendances GUI
(`libgl1-mesa-dev xorg-dev`) sur les jobs `lint`, `vet`, `staticcheck` et `govulncheck`, qui
ne les installent pas aujourd'hui — c'est probablement la raison historique de la restriction.

#### 🔴 X-02 — Le `Makefile` référence un fichier qui n'existe pas
`Makefile:39-49`

```make
build-gui:
	go build -o superview-gui superview-gui.go        # ce fichier n'existe pas
build-gui-windows:
	go build -ldflags="-H=windowsgui" -o ... superview-gui.go
```

Le point d'entrée s'appelle `gui_main.go` depuis le portage Linux (commit `76a8341`).
`make build`, `make build-gui` et `make build-gui-windows` échouent donc tous les trois.
Seul `build-gui-linux` est correct (il compile `.`). Il faut compiler le **paquet** (`.`),
pas un fichier isolé — sinon `gui_native_dialog_*.go` ne serait pas inclus.

#### 🟠 X-03 — `.github/copilot-instructions.md` est obsolète sur quatre points
| Affirmation | Réalité |
| --- | --- |
| « `go.mod` uses Go 1.25.0 » | `go.mod:3` → `go 1.26.0` |
| « `superview-gui.go`: desktop UI » (×4 mentions) | Le fichier est `gui_main.go` |
| « Build GUI: `go build superview-gui.go` » | Échoue |
| « `ValidateBitrate()` … (100k-50M bytes/sec) » | `config.go:22` → max 209 715 200 (200 M) |

Ce fichier pilote les suggestions d'agents de code : ses erreurs se propagent.

#### 🟡 X-04 — golangci-lint silencieusement figé sur la v1 (fin de vie) — *sévérité revue à la baisse*

**Correction de mon constat initial** : j'avais écrit que le job `lint` était « très
probablement en échec ». Vérification faite le 2026-09-04 en exécutant réellement l'outil :
**le job passait**, sans aucune alerte.

Ce qui se produit vraiment : `go install github.com/golangci/golangci-lint/cmd/golangci-lint@latest`
résout l'ancien chemin de module, celui de la ligne v1 — golangci-lint v2 vit sous
`github.com/golangci/golangci-lint/v2/cmd/golangci-lint`. La CI installait donc **v1.64.8**,
dernière v1, sans plus jamais progresser, et `@latest` masquait ce gel.

Le passage en v2.13.2 a fait apparaître **5 alertes `errcheck` réelles** invisibles en v1,
dont trois correspondaient exactement au constat B-08.

#### 🟠 X-05 — Une action GitHub non épinglée par SHA
`.github/workflows/release.yml` : `softprops/action-gh-release@v3` alors que les 8 autres
actions du dépôt sont épinglées par empreinte. C'est justement l'action qui dispose de
`contents: write` et publie les binaires — la plus sensible du lot.

#### 🟠 X-06 — `coverage.out` est versionné et périmé
Le fichier à la racine ne contient que `common.go`, `config.go`, `security.go` et
`command-other.go`. Il ne référence ni `hardware.go`, ni `health.go`, ni `metrics.go`, ni
`observability.go`, ni `gui_helpers.go` — pourtant tous couverts par des tests aujourd'hui.
Il date donc d'un état antérieur du dépôt et annonce **33,7 %** (117/347 instructions), un
chiffre trompeur. `.gitignore` ne l'exclut pas, ni `coverage.html` que `make coverage-html`
génère.

#### 🟡 X-07 — Reliquat de code mort dans le workflow de release
`.github/workflows/release.yml`, job `notify` : les lignes `echo ... >> /tmp/release.md`
écrivent dans un fichier d'un job différent, sur une machine différente. Le contenu est perdu
et n'a aucun effet. Vestige d'un copier-coller depuis le job `create-release`.

#### 🟡 X-08 — `build.sh` diverge de la réalité du projet
Le script ne cible que Windows (`windows/amd64`, `windows/386`) alors que Linux est une
plateforme officielle depuis le commit `76a8341`, exige `fyne-cross` **v2** (dont la commande
d'installation indiquée, `go get`, n'installe plus de binaire depuis Go 1.18), et pousse des
tags git automatiquement. La CI `release.yml` fait déjà le travail correctement : ce script
devrait être supprimé ou clairement marqué comme obsolète.

### O — Observabilité et performance

#### 🟠 O-01 — Le mode par défaut `safe` impose `-re` : l'encodage ne peut pas dépasser le temps réel
`common/common.go:623-625`

```go
if !safePerformanceMode {
    baseArgs = append(baseArgs, "-re")
}
```

`-re` demande à FFmpeg de lire l'entrée **à la vitesse de lecture native**. Une vidéo de
10 minutes prendra donc au minimum 10 minutes à convertir, quel que soit le GPU. Le défaut
compilé (`config.go:36`, `PerformanceMode: "safe"`) active ce comportement ; le `superview.yaml`
livré le désactive (`safe_performance`) — mais ce fichier n'est pas lu en installation réelle
(**B-02**). Le résultat net est que la plupart des utilisateurs subissent l'encodage bridé au
temps réel.

`-re` n'a de sens que pour du streaming vers une sortie temps réel ; pour une transcodification
vers un fichier, il n'apporte aucune sécurité. Le nom « safe » est trompeur.

*Correction attendue* : traiter en même temps que B-02 ; envisager d'inverser le défaut.

#### 🟡 O-02 — Les journaux sont écartés dans `io.Discard`
`gui_main.go:149-152`. Le logger GUI écrit dans `io.Discard` au niveau `Debug`. Toute
l'infrastructure `observability.go` (351 l.) et `metrics.go` (304 l.) alimente donc un puits
sans fond : aucun diagnostic n'est récupérable après un échec chez un utilisateur. Écrire dans
un fichier tournant sous `os.UserCacheDir()` rendrait ces 650 lignes utiles et les rapports
d'incident exploitables.

#### 🟡 O-03 — La carte `y.pgm` est intégralement redondante
`common/common.go:538-540` : chaque ligne `y` répète la même valeur `outX` fois. Pour une
entrée 4K, cela produit un fichier d'environ 50 Mo, réécrit à chaque encodage. C'est le format
attendu par le filtre `remap` de FFmpeg, donc ce n'est pas incorrect — mais la ligne peut être
construite une seule fois hors de la boucle `y` et réutilisée (seule la valeur change), et
l'écriture peut passer par un `bufio.Writer`.

#### 🟡 O-04 — Cache de résolution des binaires sans invalidation
`common/common.go:47-66`. En cas d'échec, `resolveToolBinary` met en cache le nom nu (`"ffmpeg"`)
pour toute la durée du processus. Si l'utilisateur installe FFmpeg après avoir lancé
l'application — cas très probable, puisque la GUI lui affiche justement un lien d'installation
(`gui_main.go:43-61`) — il doit redémarrer sans qu'on le lui dise. Ne pas mettre en cache les
échecs, ou proposer un bouton « Réessayer ».

#### 🟠 O-05 — Chaque événement d'observabilité était dispatché dans sa propre goroutine
`common/observability.go` (constat ajouté le 2026-09-04, découvert en traitant C-06)

```go
for _, handler := range r.handlers {
    // Non-blocking dispatch
    go handler.OnEvent(event)
}
```

Le même motif existait sur les quatre méthodes (`OnEvent`, `OnProgress`, `OnError`,
`OnComplete`). Or `RecordEncodingProgress` est appelé depuis le callback de progression, soit
plusieurs fois par seconde pendant tout l'encodage : une goroutine par événement, et surtout
**aucune garantie d'ordre**. Les lignes du journal pouvaient donc s'écrire dans le désordre, et
les derniers événements être perdus si le processus se terminait avant l'exécution de leur
goroutine.

Sans conséquence tant que les journaux partaient dans `io.Discard` ; devenu un vrai problème
depuis O-02, puisque ce fichier est précisément ce qu'on lira pour diagnostiquer un incident.

*Correction* : dispatch synchrone. Les handlers ne font que journaliser, l'appel direct est
donc peu coûteux et rend l'enregistrement fidèle.

#### 🟡 O-06 — Historique d'événements en écriture seule
`EventRecorder` conservait jusqu'à 1000 événements que **rien ne lisait** : `GetEventHistory`
et `ClearHistory` n'avaient aucun appelant. Depuis O-02, le fichier de journal contient déjà
la même information, persistée. L'historique en mémoire a été supprimé.

### T — Tests et couverture

#### 🟠 T-01 — Zéro test sur le paquet `main`
92 fonctions de test couvrent `common/`, aucune ne couvre les 743 lignes du paquet racine.
Des éléments sont pourtant testables sans afficher de fenêtre : `containsString`,
`formatResultsPanel`, `showPrerequisiteDialog` (logique de décision), la normalisation
d'extension `.mp4`, la conversion URI→chemin (B-03), les seuils de profil qualité.
Fyne fournit `test.NewApp()` pour les widgets.

#### 🟠 T-02 — Seuil de couverture à 30 %, très bas pour un projet à 92 tests
`test.yml` et `release.yml` imposent 30 %. Le chiffre réel (mesuré sur `common/` seul, avec
tous les tests actuels) est vraisemblablement bien au-dessus. Un seuil aussi bas ne protège
contre aucune régression : il faudrait le recalculer puis le relever au niveau constaté
moins quelques points.

#### 🟡 T-03 — Aucun test d'intégration bout-en-bout
Aucun test ne fait tourner un vrai FFmpeg sur une vidéo générée (`ffmpeg -f lavfi -i testsrc`).
Les bugs B-01 et B-04 sont exactement le genre de défaut qu'un tel test attraperait. Faisable
en job CI conditionnel (`if: runner.os == 'Linux'`), FFmpeg étant préinstallé sur les runners
GitHub Ubuntu.

---

## 3bis. Troisième passe d'analyse (2026-09-04) — axes non couverts

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3ter. Quatrième passe d'analyse (2026-09-04) — relecture complète à `001d250`

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3quater. Cinquième passe (2026-09-04) — vérification d'après-release

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3quinquies. Sixième passe (2026-09-06) — structure, tests de validation, release

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3sexies. Septième passe (2026-09-06) — signalement utilisateur : la barre d'outils

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3septies. Huitième passe (2026-09-06) — signalement utilisateur : l'accélération matérielle

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3octies. Neuvième passe (2026-09-07) — signalement utilisateur : l'installation

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3nonies. Dixième passe (2026-09-08) — signalement utilisateur : mémoire épuisée

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3decies. Onzième passe (2026-09-11) — entretien : ce que `make` fait, comment on fusionne, ce que les documents comptent

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3undecies. Douzième passe (2026-10-14) — entretien : la CI rouge sur trois PR qui n'y étaient pour rien

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3duodecies. Treizième passe (2026-10-14) — état des lieux complet, à `f925ee5`

> Déplacée, texte inchangé, dans [archives/ANALYSE-passes-3-a-13.md](archives/ANALYSE-passes-3-a-13.md).

## 3terdecies. Quatorzième passe (2026-10-20) — analyse complète après v0.3.0, à `4efae3a`

Première passe sur le produit Rust seul, à la demande de l'utilisateur : structure, code, sécurité,
performances, organisation du dépôt. Quatre relectures parallèles (pipeline, modules de validation,
interface, dépôt), chaque constat retenu vérifié ensuite dans le code. Mesures à `4efae3a` :
environ 8 000 lignes de code pour 21 000 de tests (632 `#[test]`), un seul `expect` hors tests,
aucun `unsafe`, `cargo audit` sans faille (cinq crates transitifs non maintenus via Slint et rfd),
binaire Linux release de 28,9 Mo qui tombe à 18,4 Mo avec LTO et `strip`.

**Verdict** : aucun défaut ne bloque une version. Les défauts réels sont des tâches de fond non
sérialisées avec l'encodage, des héritages du port Go, et des flux non bornés. Plan d'exécution en
sessions : `~/.claude/plans/declarative-beaming-lobster.md` (une PR par session, test de
contre-preuve d'abord, fixtures de parité intouchées). Les constats sont numérotés dans l'ordre des
sessions, **F-01 à F-30** ; les deux durcissements qui toucheraient une fixture gelée sont des
questions, Q-04 et Q-05 (§ 5). **Aucune modification de code dans cette passe.**

### F-01 ✅ — ~~Les tests inline font des fichiers de 2 000 lignes~~ — **CORRIGÉ**

🟢 `rust/crates/superview-app/src/state.rs`, `superview-core/src/probe.rs`, `pipeline.rs`, `encode/cascade.rs`, `tools.rs`…

Les blocs `#[cfg(test)]` pèsent de 60 à 97 % des gros modules (`state.rs` : 2 387 lignes pour
moins de 1 000 de code). Lire le code coûte la lecture des tests. Déplacement mécanique vers
`src/<module>/tests.rs`, même nombre de tests avant et après.

**Corrigé (session 1)** : 24 modules, corps de `mod tests` déplacé tel quel (au blanc et à la virgule
finale près, vérifié) ; 443 + 137 tests avant et après, zéro `ignored` de plus.

### F-02 — Pas de `[profile.release]` : binaire de 28,9 Mo, non `strip`

✅ — CORRIGÉ (session 2, DEC-50) 🟡 `rust/Cargo.toml`

Mesuré : `lto = "fat"`, `codegen-units = 1`, `strip = "symbols"` donnent 18,4 Mo, donc un
installateur et une AppImage plus légers. Pas de `panic = "abort"` : `app.rs` transforme les panics
des fils de travail en erreur affichée par `catch_unwind`.

### F-03 — `sync_all` sur les cartes PGM

✅ — CORRIGÉ (session 2) 🟢 `rust/crates/superview-core/src/pgm.rs:194`

De 66 à 113 Mo de fichiers temporaires sont forcés sur le disque alors que seul le ffmpeg qui suit
les lit. `flush` + `into_inner()` voit toujours l'erreur d'écriture ; `tests/pgm_golden.rs` fige les octets.

### F-04 — Le stderr de ffmpeg est accumulé sans plafond, puis copié dans l'erreur

✅ — CORRIGÉ (session 3) 🟠 `rust/crates/superview-core/src/encode/run.rs:247-260`, `encode/failure.rs:36`

`-loglevel error` ne borne rien : un H.264 abîmé écrit une ligne par trame. Le `Vec` grossit en
mémoire, puis le texte entier part dans `Error::Plain`, le journal et la popup. Un collecteur
commun à `run.rs` et `process.rs` (début + fin, marqueur de troncature) supprime aussi la
duplication du fil lecteur.

### F-05 — `check_video` analyse stdout et stderr mélangés comme du JSON

✅ — CORRIGÉ (session 3) 🟠 `rust/crates/superview-core/src/video.rs:186`, `process.rs:65-70`

`combined_output` entrelace les deux flux. Un avertissement d'ffprobe sur un fichier lisible
(`partial file`) casse l'analyse avec « failed to parse video metadata », la vraie cause perdue.
Hérité de `CombinedOutput` en Go. Lire stdout seul pour le JSON, garder stderr pour le message.

### F-06 — Le journal peut entrelacer deux messages sur une ligne

✅ — CORRIGÉ (session 3) 🟡 `rust/crates/superview-core/src/logfile.rs:178-182`, `superview-app/src/journal.rs:74`

`writeln!` fait deux `write` (la ligne, puis `\n`) et le verrou est pris à chaque `write` : un autre
fil peut s'intercaler, et le plafond peut vider le fichier entre les deux. Une ligne = un `write_all`.

### F-07 — L'aperçu matériel et la conversion ne posent pas la même question à la sonde
✅ — CORRIGÉ (session 4) 
🟠 `rust/crates/superview-core/src/probe.rs:706-713`, `pipeline.rs:438`, `superview-app/src/app.rs:935`

`describe_verified_hardware_plan` sonde avec un preset vide, la conversion avec `cfg.video_preset`,
et le preset fait partie de `ProbeSpec::cache_key`. Chaque conversion relance donc une sonde
(0,3 s, jusqu'à 15 s sur un pilote bloqué), et l'aperçu peut dire « OK » pour un encodeur que la
conversion refusera avant de basculer vers le CPU sans bruit. C'est l'écart qu'E-01 voulait fermer.

### F-08 — La sonde de vérification n'est pas annulable alors que l'API le permet
✅ — CORRIGÉ (session 4) 
🟡 `rust/crates/superview-core/src/pipeline.rs:433-440, 485-488`

`None` est passé pour le drapeau ; le commentaire admet jusqu'à deux `PROBE_TIMEOUT` (30 s) avant
que Cancel soit vu. « Comme en Go » n'est plus une contrainte, et une sonde annulée n'est jamais
mise en cache (E-23).

### F-09 — La cause d'un repli n'est jamais journalisée
✅ — CORRIGÉ (session 4) 
🟡 `rust/crates/superview-core/src/encode/cascade.rs:216, 252`

`Err(_) => log::warn!(...)` jette l'erreur ; celle du passage CPU est perdue si le repli réussit.
Le journal joint aux rapports de bug dit « fallback » sans dire pourquoi. Ajouter `error={err}` en
fin des lignes existantes, que `tests/cascade_logs.rs` fige par leur préfixe.

### F-10 — Un échec de lancement de la sonde est mis en cache comme un refus définitif
✅ — CORRIGÉ (session 4) 
🟢 `rust/crates/superview-core/src/probe.rs:520, 592`

Une erreur passagère de `spawn` (EAGAIN, ffmpeg remplacé pendant une mise à jour) rétrograde
l'encodeur pour toute la session. Ne mettre en cache que les verdicts d'un processus terminé.

### F-11 — La sonde de démarrage peut finir pendant un encodage et lancer un essai GPU à côté

🔴 `rust/crates/superview-app/src/state.rs:506, 655-672`, `app.rs:645-655`

`can_start` ignore `encoder_probe` ; quand le rapport arrive, `verify_hardware_in_background` ne
regarde pas `is_encoding()`. Avec six encodeurs ou un pilote lent, l'utilisateur lance Start avant
la fin de la sonde ; à son arrivée, la ligne Hardware est réécrite et un vrai encodage d'essai part
sur le même GPU. Aucun test : `FakeEngine::probe_hardware` répond tout de suite. Différer le rapport
jusqu'à la fin de l'encodage.

### F-12 — Le Diagnostic ne grise ni Start ni lui-même

🟠 `rust/crates/superview-app/src/app.rs:1046-1106`, `ui/main.slint:282-285`

`check_health` sonde tous les encodeurs avec un `Prober` neuf. Diagnostic puis Start : la sonde
complète tourne à côté d'un encodage matériel. Deux clics : deux balayages GPU en parallèle, deux
rapports empilés. Un état « diagnostic en cours » sur le modèle de `begin_pick`.

### F-13 — Un sélecteur encore ouvert pendant la question « Overwrite » peut changer la sortie

🟡 `rust/crates/superview-app/src/app.rs:524-527, 875-876`

La question porte sur le chemin lu au clic sur Start, mais `begin_conversion` relit l'état au
« Yes », et `chosen_file_arrives_too_late` ne bloque qu'en cours d'encodage. Figer `(input, output)`
dans `PopupKind::Overwrite` et comparer au « Yes ».

### F-14 — Pas de hook de panic : un panic sur le fil UI disparaît sans trace

🟡 `rust/crates/superview-app/src/main.rs`

Les fils de travail sont sous `catch_unwind`, pas les callbacks Slint ni les closures `post`. Sous
Windows en release (`windows_subsystem = "windows"`), stderr n'existe pas : le journal que le README
demande de joindre ne dit rien. `set_hook` qui journalise puis `flush`, chaîné au hook par défaut.

### F-15 — ffmpeg reçoit le lien symbolique, pas la cible validée

🟢 `rust/crates/superview-core/src/security.rs:136-153`, `pipeline.rs:331-354`

`is_valid_input_path` résout le lien et valide la cible, mais c'est le chemin non résolu qui part
vers ffprobe et ffmpeg. Fenêtre TOCTOU, faible sur un bureau mono-utilisateur. Rendre le `PathBuf`
résolu et le passer.

### F-16 — Répertoires protégés incomplets sous Unix

🟢 `rust/crates/superview-core/src/security.rs:55-58`

`/dev`, `/run`, `/proc`, `/sys` manquent. Lancé en root (cas prévu par le commentaire), une sortie
`/dev/sda` passe et le renommage final remplace le nœud. `/var` reste ouvert : `/var/tmp` est légitime.

### F-17 — Aucun délai sur ffprobe ni sur `ffmpeg -version`, `-encoders`, `-hwaccels`

🟢 `rust/crates/superview-core/src/video.rs:186`, `tools.rs:418-426`, `health.rs:181`

Tous via `combined_output`, sans borne. Une entrée sur un partage réseau figé bloque le fil sans
fin, et `shutdown` laisse un ffprobe orphelin. `run_with_deadline` existe déjà.

### F-18 — `run_with_deadline` attend sans borne un processus terminé seul

🟢 `rust/crates/superview-core/src/process.rs:196`

`done_rx.recv()` sans délai : un petit-fils qui hérite du tube bloque l'appelant, ce que la branche
« kill » évite avec `READER_GRACE`. `recv_timeout` dans les deux branches.

### F-19 — La limite cgroup n'est lue qu'à la racine

🟢 `rust/crates/superview-core/src/memory.rs:224-225`

Sur un bureau systemd (cgroup v2), la racine n'a pas de `memory.max` ; un `MemoryMax` posé sur
l'unité ou la slice est ignoré. Lire `/proc/self/cgroup` et prendre le minimum en remontant.

### F-20 — `min_bitrate` et `max_bitrate` négatifs sont acceptés en silence

🟢 `rust/crates/superview-core/src/config.rs:144`

`max_bitrate: -1` vaut « pas de plafond » sans avertissement, alors que les nombres de fils sont
normalisés avec un `warn`. Même traitement.

### F-21 — Règles de recherche d'outils incohérentes

🟢 `rust/crates/superview-core/src/tools.rs:140-148, 233-235`

Override, copie embarquée et chemins winget/scoop acceptent un fichier sans bit exécutable, le
`PATH` l'exige ; un `SUPERVIEW_FFMPEG_DIR` relatif n'est pas refusé (E-22, L-16). `is_executable_file`
partout, override relatif ignoré avec avertissement.

### F-22 — Petits défauts : cause affichée deux fois, étiquette d'étape, `\r`, permissions du journal

🟢 `rust/crates/superview-core/src/error.rs:44-50, 77-80`, `pipeline.rs:517`, `health.rs:185`, `logfile.rs:55, 65`

`SessionError` et `FailedAfterOutput` mettent la cause dans le `Display` et en `#[source]` : tout
outil qui parcourt `source()` l'affiche deux fois (retirer `#[source]`, le texte vient de l'oracle).
Un échec de `new_partial_output_path` est enregistré sous `stage "encoding"`. La première ligne de
`ffprobe -version` garde son `\r` sous Windows. Les modes 0o700/0o600 ne s'appliquent qu'à la création.

### F-23 — « Cancelling... » peut être écrasé par une progression en file

🟢 `rust/crates/superview-app/src/app.rs:905-915`, `state.rs:826-832`

`WindowSink::status` ne vérifie que `is_encoding()` : après Cancel, un `post` déjà en file remet
« Transforming... 45% ». Ignorer statut et barre quand l'annulation est demandée.

### F-24 — La question « Cancel and quit » reste à l'écran après la fin de la conversion

🟢 `rust/crates/superview-app/src/app.rs:216-221`

`close()` ne purge le `Quit` que s'il attend derrière un autre popup. Affiché, il continue de dire
« An encoding is in progress » et le Done passe derrière.

### F-25 — La liste des profils qualité existe en double entre Slint et Rust

🟢 `rust/crates/superview-app/ui/main.slint:168-171, 311`, `src/prefs.rs` (`QUALITY_PROFILES`)

Un profil ajouté côté Rust n'apparaît pas dans le menu, et `sanitized()` le rejette au chargement
suivant. Passer la liste en propriété `in`.

### F-26 — Les modaux ignorent Échap et Entrée

🟢 `rust/crates/superview-app/ui/main.slint:122-128`

Le `Modal` prend le focus mais n'a pas de `key-pressed` : rien ne se pilote au clavier, sauf le
champ texte.

### F-27 — Fenêtre fixe 980×470 en pixels logiques

🟢 `rust/crates/superview-app/ui/main.slint:159-162`

À 150 % sur un 1366×768, la fenêtre fait 1 470 px physiques de large, non redimensionnable, donc
coupée. À valider par capture avant de toucher la disposition.

### F-28 — Aucun `timeout-minutes` sur les jobs de `rust.yml` et `lint.yml`

🟢 `.github/workflows/rust.yml`, `lint.yml`

Défaut GitHub : 360 min. Un ffmpeg bloqué en CI occupe un runner six heures. `release.yml` n'est
pas concerné par ce plan (essai à blanc obligatoire, L-114).

### F-29 — Pas de `cargo-deny` ; `THIRD_PARTY_NOTICES.md` annonce 471 crates pour 548 dans `Cargo.lock`

🟢 `THIRD_PARTY_NOTICES.md:57`, `rust/Cargo.lock`, job *Rust coverage and audit*

Les cinq crates non maintenus (`bincode`, `ttf-parser`, `zbus_names`, `zvariant`, `zvariant_derive`)
ne sont ignorés nulle part : le commentaire de `rust.yml` renvoyait à un `audit.toml` absent
(corrigé dans cette passe). `cargo-deny` vérifierait aussi la compatibilité des licences avec
GPL-3.0-or-later.

### F-30 — La couverture est vérifiée mais jamais publiée

🟢 `.github/workflows/rust.yml`, job *Rust coverage and audit*

Le seuil de 50 % est un gate sans trace : pas d'artefact, pas de chiffre lisible après coup. Publier
le `lcov` en artefact suffit (pas de service externe, E-18).

**Consigné, hors périmètre** (bonne facture mais pas de gain fonctionnel, ou décision à part) :
regroupement des treize paramètres de `encode_video_with_memory` (`#[allow(too_many_arguments)]`
justifié par « port 1:1 », caduc depuis DEC-49) ; struct `FfmpegCapabilities` à la place du
`HashMap<String, String>` de CSV ; découpage de `state.rs` en `lifecycle`/`view`/`handler` ;
rotation du journal au lieu de la troncature ; préférences écrites sur le fil UI ; `version()` qui
rend toujours `0.0.0` ; `remap_output_size` `pub` qui panique sur une vidéo sans flux ; aller-retour
octets → Go → octets dans `disk.rs` ; fusion des trois journaux (`ANALYSE`, `LECONS`, `CORRECTIONS`)
et des deux systèmes de pièges (`PIEGES.md`, `pieges/`) ; `LICENSE` GPL-3.0 face au
`GPL-3.0-or-later` de Cargo ; `.github/sample.gif` de 5,3 Mo dans le dépôt.

## 4. État d'avancement

| Statut | Constats |
| --- | --- |
| ✅ **Corrigé et vérifié — 1ʳᵉ passe** (32) | B-01, B-02, B-04, B-05, B-06, B-07, B-08, S-01, S-02, S-03, S-04, C-01, C-02, C-03, C-04, C-07, C-08, X-01, X-02, X-03, X-04, X-05, X-06, X-07, X-08, O-01, O-02, O-03, O-04, T-01, T-02, T-03 |
| ⬜ **Invalidé** (1) | B-03 — le constat était faux, voir ci-dessus |
| ✅ **Corrigé et vérifié — 2ᵉ passe** (4) | C-05, C-06, O-05, O-06 |
| ✅ **Corrigé et vérifié — 3ᵉ passe** (9) | N-01 à N-06, N-08, N-09, N-10 |
| 🔄 **Révisé — 3ᵉ passe** (1) | N-07 — mesure refaite, le gain est de ~10 % et non ~5 % ; recommandation : **conserver**, donc aucun changement de code |
| ✅ **Corrigé et vérifié — 4ᵉ passe** (13) | P-01 à P-13 *(P-09 partiellement, voir ci-dessus)* |
| ✅ **Corrigé et vérifié — 5ᵉ passe** (7) | R-01 à R-07 |
| ✅ **Corrigé et vérifié — 6ᵉ passe** (16) | D-01 à D-12, V-01 à V-04 |
| ✅ **Corrigé et vérifié — 7ᵉ passe** (2) | U-01, U-02 — barre d'outils : libellés rognés, rangée non centrée |
| ✅ **Corrigé et vérifié — 8ᵉ passe** (4) | U-03 — capacités matérielles déduites d'une liste de compilation ; sonde à l'exécution. U-04 — FFmpeg empaqueté, plancher pilote épinglé et vérifié en CI. U-05 — chemins Vulkan et D3D12 ajoutés, et VAAPI réparé au passage. U-06 — la documentation utilisateur contredisait les trois correctifs |
| 📌 **Consigné, hors périmètre — 6ᵉ passe** (4) | R-08 à R-11 — la release a été mise hors périmètre pour ce chantier. **R-08 est le seul qui appelle une action** : le correctif R-06 n'est pas publié. |
| ✅ **Corrigé et vérifié — 9ᵉ passe** (2) | U-07 — le README n'était pas suivable par un utilisateur lambda sous Windows : ordre des sections, instructions en forme de terminal, SmartScreen passé sous silence. U-08 — documentation publiée en français à côté de l'anglais, parité tenue par la CI |
| ✅ **Corrigé et vérifié — 10ᵉ passe** (6) | U-09 — un arrêt décidé par le système était rapporté comme une annulation de l'utilisateur, et un ffmpeg tué par le noyau ne nommait jamais la mémoire. U-10 — le repli CPU ignorait un encodeur matériel disponible dans l'autre famille de codec ; bascule et annonce, arbitrage utilisateur. U-11 — garde-fou mémoire avant encodage, chiffré au banc. U-12 — bruit du journal : `N/A` en `WARN`, seuil disque insatisfiable, événements en double, commentaire orphelin. U-13 — sonde à la géométrie réellement encodée, confirmée sur le matériel du signalement. U-14 — le garde-fou mémoire suit l'encodeur qui prend le relais |
| ✅ **Corrigé et vérifié — 11ᵉ passe** (4) | M-01 — `make test` et `make coverage` ne posaient pas `SUPERVIEW_REQUIRE_FFMPEG=1` : quinze tests se sautaient en silence, dont tous ceux qui vérifient une conversion réelle. M-02 — `make clean` supprimait un `dist/` que rien ne produit et laissait les six résidus d'empaquetage que `.gitignore` a dû apprendre un par un. M-03 — la règle de fusion d'`AGENTS.md` était fausse pour la PR du bas d'une pile ; corrigée, puis appliquée par un workflow sur label, et `master` doté du ruleset qu'il n'avait pas. M-04 — l'en-tête de ce document et sa numérotation de sections avaient dérivé |
| ✅ **Corrigé et vérifié — 12ᵉ passe** (2) | M-05 — `go-version: '1.26.x'` sans `check-latest` désignait le Go en cache du runner (1.26.8), pas le dernier correctif : `govulncheck` rouge sur toutes les PR, et une release aurait embarqué neuf vulnérabilités de la bibliothèque standard. M-06 — les branches fusionnées par le workflow restaient, le réglage du dépôt ne s'appliquant pas à ce que fait `GITHUB_TOKEN` |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR pipeline** (4) | E-02 — un signal système ne relance plus la cascade (`isStopRequest`). E-13 — un échec après production n'est plus rejoué (`errFailedAfterOutput`). E-04 — le renommage final en échec garde le fichier terminé et le nomme ; une destination-répertoire est refusée d'emblée. E-10 — fichier de travail créé en 0666 sous umask |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR sonde** (4 + ½) | E-01 — arbitrage (a) : `encoderFacingArgs` et `pixelFormatChain` partagés par la sonde et la conversion, `probeSpec` avec profondeur, débit et preset ; le pilote CQP-only est refusé au démarrage avec ses propres mots. *Rouvert côté Rust par DEC-48 (2026-10-19) : l'option (b) y est faite en automatique pour VAAPI, le pilote CQP-only est utilisé en qualité constante ; Go l'ignore.* E-07 — `qsv` et `drm` retirés des jetons de décodage. E-08 — table NVENC `p1`–`p7`, `fast → p3` ; aucun preset pour les familles à trames sur périphérique. E-09 — `-tag:v hvc1`. E-23 — une sonde annulée n'est plus mise en cache |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR CI** (4) | E-06 — `merge-on-label.yml` dispatche `test.yml` et `lint.yml` sur `master` après la fusion (`workflow_dispatch`, l'exception à la règle du `GITHUB_TOKEN`) ; ruleset laissé non strict, DEC-35. E-11 — `-race -count=1` sur toutes les exécutions Linux ; la release rejoue la suite contre le FFmpeg qu'elle livre (`SUPERVIEW_FFMPEG_DIR`) ; Windows teste l'archive gyan.dev 8.1.1 épinglée par URL et SHA-256 au lieu de `choco`. E-18 — envoi Codecov retiré : le dépôt n'a jamais été activé sur codecov.io (`Repository not found`, vu par OIDC sur la PR même), aucun rapport n'est jamais arrivé ; le gate interne décide. E-24 — cache manuel retiré, `contents: write` sur `create-release` seul, commentaire de couverture protégé des forks, profil passé en artefact, une seule exécution de la suite par job, tag par `env:`, § *Manual pins* de `RELEASING.md` ; et le job `fmt` ne sauve plus un cache vide sous la clé commune (L-108) |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR robustesse** (5) | E-05 — arbitrage : refuser ; `checkGeometry` refuse une source H/W > 9/8 en mode non-squeeze, test de propriété jumeau de la carte squeeze. E-16 — `checkOutputSpace` compare la taille attendue au volume de destination ; l'estimation part de la première progression et ne dit rien sous 2 %. E-17 — le parent est résolu avant le contrôle des répertoires système, enfin testé ; Windows lit `freeBytesAvailable`. E-21 — clés inconnues refusées et nommées, `min > max` refusé, codecs épurés avec repli. E-12 — le sélecteur Windows écrit le chemin en UTF-8 (non exécuté sous Windows) |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR GUI** (4 + partiel) | E-03 — `appState.shutdown` : *Quit*, la croix et SIGTERM annulent et attendent `done`, borné à 20 s ; deux crochets, `SetOnClosed` et le retour de `ShowAndRun()`, parce que le `catchTerm` de Fyne ne ferme aucune fenêtre (vérifié dans 2.8.2). E-14 — `completeEncoding` : annulation = *Status: Cancelled*, sans dialogue ; *Diagnostic* dans la liste verrouillée, calculée par `windowControls.locked()` que le test appelle. E-15 — `GUIHandler.encoderName` capturé sur le fil UI ; `restoringSelection` autour des `SetSelected` programmés. E-20 — seul le code 1 vaut annulation (DEC-34 amendée) ; sélecteurs natifs dans une goroutine. E-23 — closures `selectedQualityProfile`/`squeezeSource` repliées dans `appState`, `[Migrations] fyneDo = true` ; **le reste d'E-23 demeure ouvert**. Six tests, huit contre-épreuves |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR ménage** (1 + ½ + DEC-33) | E-22 — `elapsedLocked()` partagé par `ElapsedTime`, `Summary` et `LogMetrics` (l'interblocage se reproduit 3 fois sur 3 sans lui) ; `findWindowsToolBinary` saute les variables vides ; `cappedLogWriter` recoupe le journal pendant l'écriture, `0o600`/`0o700` ; `availableMemoryBytes` prend le minimum avec `memory.max − memory.current`. E-23 partiel — `SessionError` rendu par `InitEncodingSession` et `getSessionPaths` (champ `Err`, `Unwrap`) ; restent le cache de sonde annulée (fait, PR sonde) et les closures/`fyneDo` (autre PR). DEC-33 — défaut `safe_performance` partout |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR tests** (1) | E-19 partiel — côté `common` : relance audio de `safe_performance` (relance AAC, pas de relance après sortie ni après annulation), `findWindowsToolBinary` (arbre winget `Packages`, `Links` prioritaire, tous OS), sorties précoces de `PerformEncoding` avec leur `stage`, branches d'erreur de `CheckFfmpeg`, refus de `checkTempSpaceForMaps` (désormais via `readFreeDiskGB`), squeeze traversant le pipeline, gardes ffmpeg de `lognoise_test.go`. Couverture totale 72,8 → 75,2 %, `common` 87,1 → 90,3 %. **Laissé à la PR GUI** : `appState` avec un rapport de sonde, `GUIHandler.Show*`, l'assertion squeeze périmée de `TestGUIHandler_GetSqueezeAndBitrate` — E-19 reste donc compté parmi les ouverts |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR tests GUI** (1 + clôtures) | E-19 fin — côté GUI : les deux tests `appState` posent un rapport de sonde (et une sélection `libx264 encoder`) et n'acceptent plus la ligne « checking » ; `TestGUIHandler_GetSqueezeAndBitrate` teste la présence de squeeze dans les deux sens au lieu de « never » ; `GUIHandler.Show*` et `showPrerequisiteDialog` testés sur une vraie fenêtre ; garde de génération déjà tenue par deux tests existants. Treize contre-épreuves ; paquet racine 36,3 → 40,3 %. Avec la ligne « PR tests », **E-19 est clos**. **E-23 est clos** : cache de sonde annulée (PR sonde), closures et `fyneDo` (PR GUI), `SessionError` (PR ménage). **E-25 est clos** par la PR #78 (`.claude/`, phrase corrigée dans `CLAUDE.md` et `CONTRATS.md`) |
| ✅ **Corrigé et vérifié — 13ᵉ passe, PR gate** (2) | E-26 — le gate requis était *sauté*, donc compté réussi, quand un job de test échouait : `if: always()` et vérification explicite des deux résultats, prouvée sur la PR même. E-27 — quatre défauts Windows qu'E-26 masquait : plafond du journal jamais appliqué (`O_APPEND`), isolation du cache par `XDG_CACHE_HOME` seul, poignée de journal non fermée, chemin du `.bat` factice coupé à la virgule par `cmd.exe` |
| ⏸️ **Ouvert — 14ᵉ passe** (20) | F-11 à F-30 (F-01 à F-10 ✅ corrigés), § 3terdecies — plan en sessions `~/.claude/plans/declarative-beaming-lobster.md` ; aucune modification de code dans la passe |
| ⏸️ **Ouvert — 13ᵉ passe** (0) | *aucun* — les 27 constats E-01 à E-27 sont clos par les lignes ✅ ci-dessus. Les questions Q-02 et Q-03, § 5, ont été tranchées ensuite (DEC-37, DEC-38) |
| ✅ **Tranchées** (3) | Q-01 — mesurée : 1,6 → 4/3, § 5. Q-02 — mesurée : +4 % de mémoire par fil au-delà de 4, DEC-37. Q-03 — décision : pas de télémétrie GoPro, DEC-38 |
| ❓ **Ouvertes** (2) | Q-04 — forcer le démuxeur MP4 (`-f mov` ou `format_whitelist`) sur l'entrée, § 5. Q-05 — refuser une durée `inf`/`nan`, § 5 |

Vérification, module entier, sysroot GUI reconstruit : `gofmt` · `go build ./...` ·
`go vet ./...` · `golangci-lint run ./...` 0 alerte · `go test -race ./...` · GUI démarrée
et vivante.

**Sur la couverture, trois chiffres circulaient dans ce document pour le même « après » :**
54,6 %, 59,3 % et 71,6 %. Ils mesuraient trois choses différentes sans le dire — le module
sans ffmpeg, le module avec, et le seul paquet `common`. Mesures refaites à `a672bc6`, avec
ffmpeg présent et `SUPERVIEW_REQUIRE_FFMPEG=1` :

| Périmètre | Couverture |
| --- | --- |
| Module entier (`./...`) — ce que mesure le seuil CI de 50 % | **66,0 %** |
| Paquet `common` | **81,5 %** |
| Paquet `main` — construction de widgets, non traversée par les tests | **25,2 %** |

Sans ffmpeg, le module tombait à 51,4 % : c'est ce delta qui portait toute l'information sur
le fait que les tests d'intégration avaient réellement tourné. `SUPERVIEW_REQUIRE_FFMPEG=1`
le remplace par un échec franc, et la couverture n'a plus à jouer ce rôle.

**Chaque correctif a été validé en contre-épreuve** : le défaut est réintroduit et le test doit
rougir. Deux tests ont ainsi été réécrits parce qu'ils passaient à vide (L-37) — un compteur
d'invocations qui ne discriminait pas, et une annulation déclenchée trop tôt pour que ffmpeg ait
créé son fichier.

Les quatre arbitrages de périmètre ont été soumis à l'utilisateur le 2026-09-04 et tranchés :
S-02 → résoudre les liens symboliques puis valider la cible · C-01 → brancher `health.go` sur un
bouton « Diagnostic » · C-03 → retirer les options sans effet · C-04 → exposer « squeeze » par
une case à cocher.

Détail des mesures obtenues :

| Indicateur | Avant | Après |
| --- | --- | --- |
| Couverture (module entier) | 33,7 % | **66,0 %** |
| Seuil de couverture en CI | 30 % sur `./common` | **50 % sur `./...`** |
| Portée de l'analyse CI | `./common` uniquement | **`./...`** (paquet GUI inclus) |
| golangci-lint | v1.64.8 (fin de vie), 0 alerte | **v2.13.2, 0 alerte** après 9 corrections |
| `GeneratePGM` (1440×1080) | 67,2 ms | **16,9 ms** (×4, sortie identique au bit près) |
| Encodage bridé au temps réel | oui, par défaut (`-re`) | **non** |
| Tests du paquet `main` | 0 | **28** |
| Tests d'intégration FFmpeg | 0 | **4**, et qui ne peuvent plus sauter en silence |
| Fonctionnalités inatteignables | squeeze, `health.go` | **exposées dans la GUI** |
| Options de config sans effet | 3 | **0** |
| Configuration | globale mutable, écrasée à chaque run | **passée en paramètre** |
| API exportée sans appelant | 9 symboles | **0** |
| Dispatch des événements | 1 goroutine par événement, ordre non garanti | **synchrone et ordonné** |

---

## 5. Questions ouvertes

Convention : **Q-xx**. Ce ne sont **pas** des constats. Une question ouverte est un point dont on
ignore encore s'il constitue un défaut — la consigner évite qu'elle se reperde, sans l'inscrire
au passif du projet.

### Q-01 — D'où vient le facteur 1,6 du profil de qualité « Balanced » ?

[`gui_main.go:58-65`](../cmd/superview/gui_main.go#L58-L65)

```go
case "Fast":
    return inputBitrate, "fast"
default: // "Balanced"
    return int(float64(inputBitrate) * 1.6), "medium"
```

Le commentaire déclare l'intention : *« the output is widened from 4:3 to 16:9, so Balanced
raises the bitrate to keep the perceived quality of the source »*. Or l'élargissement 4:3 → 16:9
**à hauteur constante** multiplie le nombre de pixels par exactement **4/3 ≈ 1,333**, pas par 1,6.

Une décomposition rend la question plus intéressante qu'un simple écart :

```
1,6 = 4/3 × 1,2   (exactement)
```

Le facteur est donc le ratio géométrique **plus 20 %**. Cela ressemble davantage à un choix
délibéré qu'à une constante posée au hasard — mais rien ne le documente, et la marge de 20 %
n'est justifiée nulle part.

**Sous-questions, aucune tranchée :**

1. Les 20 % compensent-ils quelque chose de réel ? La distorsion étire les bords, et du contenu
   agrandi demande plus de bits pour ne pas s'adoucir visiblement. C'est plausible ; ce n'est pas
   mesuré.
2. Le profil **« Fast » ne relève pas le débit du tout**, pour 4/3 de pixels en plus — soit
   **0,75 fois** les bits par pixel de la source. À qualité perçue constante, il dégrade
   forcément. Est-ce assumé, et l'utilisateur le sait-il ?
3. **Interaction avec N-06.** Depuis l'ajout de `-maxrate`, ce débit n'est plus une cible moyenne
   dépassable mais un **plafond effectif**. Le sens de la constante a changé sans qu'elle soit
   revue.
4. Le résultat est ensuite écrêté par `cfg.MinBitrate` / `cfg.MaxBitrate` dans la callback
   *Start*, ce qui peut masquer l'effet du facteur sur les sources à haut débit.

**Méthode employée** — celle qui a fait tomber P-12 : confronter la constante à l'**intention
déclarée**. Ici l'intention est mesurable, donc elle a été mesurée.

### Q-01 — Réponse, mesurée le 2026-09-04 — **TRANCHÉE**

**Dispositif.** Un pilote Go appelant `common.PerformEncoding`, donc le chemin réel de
l'application et non une ligne ffmpeg recopiée ; cartes de remappage produites par
`common.GeneratePGM`. Source : un clip FPV DJI de 18 s, 3840×2880 (4:3 exact), HEVC 10 bits,
59,94 fps, 99,9 Mb/s — intérieur, faible lumière, mouvement rapide, détail fin répétitif au
plafond et au sol, donc un cas défavorable. Métriques SSIM et XPSNR : **le ffmpeg de la machine
n'a pas libvmaf**, ce qui est dit plutôt que contourné.

**La référence qui rend la question décidable.** Chercher un coude sur la courbe débit/qualité ne
donne rien — le contenu est exigeant, la qualité monte encore de +1,26 dB entre 1,6 et 2,0. La
bonne référence est l'intention elle-même : *« keep the perceived quality of the source »*. On la
matérialise en réencodant la source **sans remappage**, même encodeur, à son propre débit, et en
mesurant l'écart à la source. C'est la qualité que cet encodeur atteint sur ce contenu à ce coût.

**Résultat** (`hevc_nvenc`, 18 s, référence à 43,350 dB XPSNR / 0,981918 SSIM) :

| k | débit | XPSNR Y | écart | images au-dessus de la référence |
| --- | --- | --- | --- | --- |
| 1,00 « Fast » | 99,9 Mb/s | 42,414 | **−0,94 dB** | 2,6 % |
| 4/3 | 133,2 Mb/s | 43,964 | +0,61 dB | 92,2 % |
| 1,60 « Balanced » | 159,8 Mb/s | 45,008 | **+1,66 dB** | 99,4 % |
| 2,00 | 199,8 Mb/s | 46,267 | +2,92 dB | — |

Par interpolation logarithmique, le multiplicateur qui **égale** la référence vaut **1,190**
(XPSNR) et **1,304** (SSIM).

**Contre-essai sur un second encodeur.** Le même protocole avec `libx265` sur un segment de 4 s —
contrôle de débit sans rapport avec celui de NVENC — donne **1,189** et **1,285**. Deux
encodeurs, l'un matériel l'autre logiciel, s'accordent au millième sur XPSNR. Le résultat est une
propriété de la transformation, pas de l'encodeur.

**Conclusion.** L'intention déclarée est tenue autour de **1,19 à 1,30**. Le ratio géométrique
**4/3 = 1,333 la couvre déjà avec une petite marge**. Les 20 % supplémentaires du 1,6 ne
compensaient rien de mesurable : ils achetaient de la qualité **au-delà** de la source, pour 20 %
de fichier en plus (365 Mo contre 304 Mo sur cet échantillon). Et le sens de l'écart confirme
l'intuition géométrique — la sortie remappée demande un peu *moins* de bits par pixel que la
source, parce que l'étirement magnifie la périphérie et en abaisse les fréquences spatiales.

Sur « Fast » : il dégradait réellement, de 0,94 dB sous la qualité de la source sur 97,4 % des
images, sans que rien dans l'interface le signale.

**Arbitrage rendu par l'utilisateur le 2026-09-04** : les deux profils passent à 4/3 et ne
diffèrent plus que par le préréglage d'encodage — ce que « Fast » laisse entendre, plus rapide
et non plus grossier. Constante nommée `widenedPixelRatio`, test mis à jour et vérifié en
contre-épreuve (les quatre cas rougissent sur 1,6), README corrigé.

**Portée.** Un seul clip, un seul type de contenu, deux encodeurs HEVC ; H.264 non testé. La marge
réelle est probablement plus large que mesurée, le contenu choisi étant défavorable.

### Q-02 — Le garde-fou mémoire est-il juste au nombre de fils réellement utilisé ? — **TRANCHÉE**

[`common/common.go:771-790`](../internal/common/common.go#L771-L790), [`:1443`](../internal/common/common.go#L1443)

Les 290 et 455 octets par pixel (U-11) ont été mesurés avec `libx265 -threads 4`
(`pools=4`). L'encodage réel passe `-threads runtime.NumCPU()`, plafonné à 16 (P-11). Or
x265 alloue par *frame thread* : chaque fil tient une trame en vol avec ses buffers de
prédiction. Sur une machine à 16 cœurs et une sortie 5120×2880 10 bits, le pic réel est
probablement bien au-dessus de l'estimation, et le garde-fou dirait « ça passe » à une
conversion qui finira tuée par le noyau — l'incident exact qu'il a été écrit pour
empêcher. **Non mesuré.** À mesurer avant de toucher la constante : même protocole qu'U-11,
`-threads 8` et `-threads 16`, et si la pente est nette, chiffrer par fil plutôt que par
pixel.

### Q-03 — Faut-il conserver la télémétrie GoPro ? — **TRANCHÉE**

[`common/common.go:1314`](../internal/common/common.go#L1314)

`-map "[v]" -map "0:a?"` ne garde aucun flux de données : la piste `gpmd` (GPS, IMU,
température) d'un fichier HERO est perdue, de la même famille que N-04/N-05. `-map 0:d?
-c:d copy` fonctionne pour `gpmd` dans le muxeur mov ; un codec de données inconnu exige
`-copy_unknown`, et un lecteur qui ne connaît pas la piste peut s'en plaindre. **Question
produit, pas défaut** : l'outil existe pour la vidéo, et personne n'a demandé la télémétrie.
À mesurer sur un vrai fichier HERO avant d'adopter.

### Q-02 — Réponse, mesurée le 2026-10-14 — **TRANCHÉE**

**Dispositif.** `ffmpeg -f lavfi -i testsrc2=size=WxH:rate=30 -frames:v N -pix_fmt P -c:v libx265
-preset medium -threads T -x265-params log-level=error -f null -`, sous
`systemd-run --user --scope -p MemoryMax=… -p MemorySwapMax=0` et `/usr/bin/time -f %M` pour le
pic de mémoire résidente. Le plafond cgroup protège la session : un dépassement tue le
processus mesuré, pas la machine (16 cœurs, 15 Gio). Limite dite plutôt que contournée : la mire
n'a ni décodage de la source ni filtre `remap`, donc son niveau à 4 fils est 11 à 15 % sous les
290 et 455 d'U-11 ; **la pente, pas le niveau**, est ce que ce banc apporte.

| Image | Profondeur | 4 fils | 8 fils | 16 fils |
| --- | --- | --- | --- | --- |
| 5120×2880 | 8 bits | 3,54 Gio (257 o/px) | 4,15 (302) | 5,19 (378) |
| 5120×2880 | 10 bits | 5,30 Gio (385) | 6,14 (446) | tué à 7 Gio (> 509) |
| 2560×1440 | 8 bits | 1,11 Gio (322) | 1,28 (373) | 1,64 (476) |
| 2560×1440 | 10 bits | 1,60 Gio (465) | 1,84 (535) | 2,34 (681) |

**Réponse : non, l'estimation était fausse au-delà de 4 fils.** Droite nette, ×1,16 à 8 fils et
×1,47 à 16, sur les deux profondeurs et les deux tailles. À 16 cœurs, 5120×2880 en 8 bits
demandait ~5,2 Gio pour ~4,0 annoncés ; en 10 bits, plus de 7 pour 6,25 annoncés. **Correctif** :
DEC-37, `besoin × (100 + 4 × (fils − 4)) / 100`, branché sur les fils réellement passés à
ffmpeg aux deux appelants (PR #90). Cinq contre-épreuves (une par défaut réintroduit).

### Q-03 — Réponse, décision de l'utilisateur le 2026-10-14 — **TRANCHÉE**

Pas de télémétrie GoPro dans la sortie. Aucune mesure nécessaire : la question était de produit,
et la réponse est non. DEC-38.

### Q-04 — Faut-il forcer le démuxeur MP4 sur l'entrée ?

Ouverte le 2026-10-20 (14ᵉ passe). ffmpeg choisit le démuxeur d'après le contenu : un « .mp4 » qui
est en fait une playlist (HLS, concat) pourrait faire lire d'autres fichiers locaux, incrustés dans la
sortie. `-f mov` ou `-format_whitelist` sur ffprobe et ffmpeg fermerait la porte, cohérent avec
« MP4 in ». Mais cela change l'argv figé par `tests/fixtures/parity/encode_args.json` : à trancher
par une DEC, avec les fixtures régénérées et relues, pas par un correctif.

### Q-05 — Faut-il refuser une durée `inf` ou `nan` dans `VideoSpecs::validate` ?

Ouverte le 2026-10-20 (14ᵉ passe). `parse_go_float` accepte `inf` et `nan`, et ni `inf <= 0.0` ni
`NaN <= 0.0` n'est vrai, « comme en Go ». En aval, `inf` fait refuser la sortie pour « needs about
8796093022207 MB » et `NaN` fait passer le contrôle d'espace à zéro. Refuser `!is_finite()` touche
les fixtures de specs gelées : même traitement que Q-04.

## 6. Journal des révisions de ce document

| Date | Modification |
| --- | --- |
| 2026-09-04 | Création. Analyse statique complète de `Fix-Claude-Code` @ `e3269e7`. 30 constats. |
| 2026-09-04 | Application des correctifs par priorité. Toolchain installée, tout vérifié en exécution. B-03 invalidé, X-04 revu à la baisse. Ajout du § 4bis. |
| 2026-09-04 | 3ᵉ passe empirique : § 3bis, 10 constats N-01 à N-10. |
| 2026-09-04 | N-01 et N-02 corrigés. |
| 2026-09-04 | **4ᵉ passe, à `001d250`** : ajout du § 3ter, 10 constats P-01 à P-10. Correctif unique N-03/N-04/N-05 écrit et vérifié bout en bout ; passage PGM P5 binaire vérifié identique au bit près (P-08). § 1 (carte des fichiers, flux, métriques) rafraîchi sur le code réel. Aucune modification de code. |
| 2026-09-04 | Application des deux correctifs demandés : N-03+N-04+N-05 (chaîne de filtres, `-map`, `-map_metadata`) et P-08 (cartes PGM en P5 binaire). **P-11 découvert et corrigé en chemin** — `libx265` échouait sur toute machine de plus de 16 cœurs. 3 tests unitaires de format, 3 tests PGM et 1 test d'intégration bout en bout ajoutés, chacun vérifié en contre-épreuve. Leçons L-36 à L-39. |
| 2026-09-04 | Second lot : P-01 à P-07 et P-09, plus N-06, N-08, N-09, N-10. `-maxrate`/`-bufsize` calibrés par la mesure (+83 % → +24 % de dépassement). Leçons L-40 à L-43. Restent ouverts : **P-10** et **N-07** seulement. |
| 2026-09-04 | **P-10** : état de `main()` extrait dans `appState` (11 méthodes, toutes à 100 % de couverture), 18 sous-tests, chacun vérifié en contre-épreuve. `beginEncoding()` rend P-02 inexprimable. Leçons L-44, L-45. **Plus aucun constat technique ouvert ; reste N-07, arbitrage produit.** |
| 2026-09-04 | **N-07 révisé.** La mesure d'origine (~5 %) était faite sur une mire à 2 Mbps, qui se décode quasi gratuitement. Refaite sur une source type GoPro à 127 Mbps : décodage matériel **+9,9 %**, encodage matériel **×3,18**. L'arbitrage s'inverse — recommandation : conserver. Leçon L-46. |
| 2026-09-04 | Cohérence du document : quatre titres (N-03, N-04, N-05, P-11) portaient encore leur pastille de sévérité alors que le § 4bis les donnait corrigés. Restylés, et les **deux conventions de marquage sont maintenant écrites** en tête du § 3 — elles ne l'étaient pas, d'où la dérive. Leçon L-47. |
| 2026-09-04 | **P-12** et **P-13**, trouvés en cherchant ce qui du mode squeeze était vérifiable sans fichier GoPro. La formule squeeze est conçue pour valoir zéro au centre — démontré algébriquement — mais des divisions entières y laissaient une couture de 1 à 2,6 px. Défaut hérité de l'amont, identique caractère pour caractère. Le libellé de la case promettait par ailleurs une compatibilité GoPro que l'amont dément. Leçon L-48. |
| 2026-09-04 | Ajout du § 5bis « Questions ouvertes » *(renuméroté § 5 le 2026-09-11, M-04)* et de la convention **Q-xx**, distincte des constats. Première entrée : **Q-01**, le facteur 1,6 du profil « Balanced », qui vaut exactement le ratio géométrique 4/3 majoré de 20 % sans que cette marge soit documentée. |
| 2026-09-06 | **7ᵉ passe, à `2cf0020`** : deux défauts d'affichage signalés par l'utilisateur, § 3sexies. `U-01` — un `GridWrap` de 150 × 34 rognait trois libellés sur six, et `TestToolbarFitsWindow` mesurait la cellule imposée au lieu du bouton, sur des boutons sans icône : il était vert. `U-02` — un `HBox` nu collait la rangée à gauche. Cellule dérivée du minimum du bouton, rangée centrée, fenêtre élargie à son contenu. Leçons L-69, L-70 ; L-20 mise à jour. |
| 2026-09-06 | **8ᵉ passe, à `7bb715e`** : § 3septies, constat `U-03`. Enquête sur un signalement d'accélération matérielle perdue : FFmpeg 8.1 n'y est pour rien (`nvenc.c` identique à 8.0), le plancher pilote est fixé par les `nv-codec-headers` de compilation et gyan.dev l'a porté de 570 à 610 entre 8.1.1 et 8.1.2, au-dessus du maximum atteignable par une carte professionnelle (597.06). Correctif : sonde à l'exécution, section *Encoders* dans le Diagnostic, ligne « Hardware » qui ne promet plus rien d'invérifié. Leçons L-71 à L-73. |
| 2026-09-06 | **U-04**, suite de U-03 : les archives Windows et Linux embarquent un FFmpeg épinglé sur son **plancher pilote** (570.0), relu dans le binaire par la CI. Sources choisies pour la permanence de leurs URL — gyan.dev versionne ses releases, BtbN conserve ses builds de fin de mois (celui d'octobre 2024 répond encore). Découvert en route : `make install` d'un paquet fyne 1.7.2 échoue sur la ligne de l'icône, donc **aucune archive Linux publiée n'était installable**. Leçons L-74 à L-76. |
| 2026-09-06 | **U-05**, fin du chantier U-03 : ajout des encodeurs `*_vulkan` et `*_d3d12va`, qui passent par le pilote d'affichage et n'ont donc aucun plancher NVENC à manquer. Leur ajout a montré que `h264_vaapi`, candidat de longue date, n'avait jamais pu fonctionner faute de périphérique et d'upload dans le pipeline. Conversion réelle vérifiée par `h264_vulkan`. Deux contre-épreuves fausses corrigées en chemin. Leçons L-77, L-78. |
| 2026-09-06 | Hygiène du journal des corrections : le gabarit de [LECONS.md § 3](LECONS.md) demandait le sha de fusion, valeur qui n'existe pas encore quand l'entrée s'écrit — dix entrées sur treize affichaient encore « non commité ». Champ remplacé par le **numéro de PR**, connu dès l'ouverture, et les dix-huit entrées renseignées après identification de leur PR par l'historique du code (`git log -S`) plutôt que par déduction. Leçon L-79. |
| 2026-09-06 | **U-06**, relevé par l'utilisateur : les trois PR du chantier matériel n'avaient mis à jour que la section *Hardware acceleration* du README. Le § *Requirements* prescrivait toujours `winget install Gyan.FFmpeg` — le build à plancher 610 à l'origine du signalement — et donnait `ffmpeg -encoders \| grep nvenc` comme moyen de vérifier son GPU. Cause : le balayage prescrit porte sur les symboles, or aucun symbole n'avait changé ; ce sont des affirmations qui étaient devenues fausses. Leçon L-80. |
| 2026-09-07 | **9ᵉ passe**, relevée par l'utilisateur : un lecteur lambda n'a pas su installer l'application sous Windows. `U-07` — le README ouvrait sur treize lignes de FFmpeg/NVENC réservées aux compilations depuis les sources, donnait ses instructions en PowerShell à quelqu'un qui double-clique, et ne mentionnait nulle part l'écran SmartScreen que provoque un binaire non signé. `U-08` — la documentation n'existait qu'en anglais. Correctifs : `## Download and install` en première section, six étapes sans terminal, SmartScreen décrit avec les libellés de ses boutons ; `README_FR.md` et le job `readme-parity`. Trouvé en chemin : l'ancre `#requirements` était codée en dur dans `gui_main.go` et `common/common.go`, désormais pinnée par un test. Leçons L-81, L-82. |
| 2026-09-11 | **11ᵉ passe, à `08dae40`** : passe de propreté, sans signalement. § 3decies, constats `M-01` et `M-02`, tous deux dans le `Makefile`. `M-01` — `make test` et `make coverage` ne posaient pas `SUPERVIEW_REQUIRE_FFMPEG=1`, donc la commande qu'un contributeur tape pouvait être verte en ayant sauté quinze tests, dont les sept `TestIntegration_*` et l'équivalence du remap ; mesuré sur un PATH sans ffmpeg, 15 sauts silencieux → 0. `-race` et `-count=1` alignés sur la recette d'`AGENTS.md`. `M-02` — `make clean` supprimait `dist/`, que rien ne produit, et laissait les six résidus de `fyne package` que `.gitignore` a appris un par un via R-06. Trouvé et non corrigé : deux tests de `lognoise_test.go` ont besoin de ffmpeg sans être gardés. Leçon L-96. |
| 2026-09-11 | **M-03**, relevé par l'utilisateur devant le bouton de fusion de #66 : la règle d'`AGENTS.md` concluait « if it is not `master`, you are stacked », faux pour la PR du bas d'une pile — #59, #61 et #62 l'avaient déjà démontré. Phrase corrigée, puis règle rendue inutile à retenir : `merge-on-label.yml` choisit la méthode et fusionne sur label `merge`, en commentant sa décision. Trouvé en chemin : `master` n'avait **aucune protection** malgré la règle « toute modification passe par une PR », et le titre de squash valait `COMMIT_OR_PR_TITLE`, qui prend le sujet du commit sur une PR à un seul commit. Ruleset créé (7 contrôles, choisis pour ne pas nommer la version de Go), « Rebase and merge » retiré. Leçon L-97. |
| 2026-09-11 | **M-04**, trouvé en corrigeant les chiffres périmés d'`AGENTS.md` : l'en-tête de ce document annonçait encore « cinq passes et 67 constats » et « dernière passe : 9ᵉ », l'ordre de lecture s'arrêtait à § 3octies, `## 5bis` était placé **avant** `## 5`, et le § 4 portait deux lignes « 11ᵉ passe ». Compte refait (105), sections renumérotées (Questions ouvertes → § 5, Journal des révisions → § 6) et renvois suivis, lignes fusionnées. L'en-tête porte maintenant la phrase qui dit qu'il périme. Côté `AGENTS.md` : `common.go` « ~1600 » → ~2000 lignes (2023 réelles), `main()` « ~540 » → ~600 (585), couverture 81 % → 83 %, arrondis à la centaine pour vieillir moins vite. Leçon L-98. |
| 2026-10-14 | **12ᵉ passe, M-05**, partie des trois PR Dependabot bloquées (#73, #74, #75) : `Vulnerability Scan` rouge sur toutes, à cause de Go 1.26.8 pris dans le cache du runner alors que 1.26.9 corrigeait neuf vulnérabilités de la bibliothèque standard. `check-latest: true` sur les neuf étapes `setup-go`. Mesuré en local : Go 1.26.9 suffit, `golang.org/x/net` n'a pas à bouger. L'échec Windows de #74 était un 504 de Chocolatey, sans rapport. Trouvé en chemin : le défaut VAAPI/CQP, consigné au § 3undecies, à traiter à part. Leçon L-99. |
| 2026-10-14 | **M-06**, relevé par l'utilisateur : la branche de #76 restait après sa fusion. Le réglage *delete branch on merge* ne vaut pas pour une fusion faite avec le `GITHUB_TOKEN` de `merge-on-label.yml`, ce qui était vrai depuis M-03 et masqué par Dependabot, qui supprime ses propres branches. Le workflow attend la fusion et supprime la branche, sauf si une PR est empilée dessus. Leçon L-100. |
| 2026-10-14 | **13ᵉ passe, à `f925ee5`**, à la demande de l'utilisateur : état des lieux complet, six revues parallèles confrontées aux documentations officielles (FFmpeg 8.0.1, Fyne 2.8.2, Go 1.26.9, GitHub Actions), baseline mesurée (un seul rouge, E-01). § 3duodecies, **25 constats E-01 à E-25**, tous ouverts, les plus lourds reproduits deux fois. Les quatre familles : correctifs appliqués à moitié (E-02, E-06, E-14), défauts introduits par un correctif (E-04, E-10), faits FFmpeg jamais mesurés (E-07 à E-09), tests verts à vide (E-19). Créé : [DECISIONS.md](DECISIONS.md) (34 décisions) et `.claude/`. Q-02 et Q-03 ajoutées. Plan d'actions en sept PR, quatre arbitrages attendus. Leçons L-101 à L-106. **Aucune modification de code.** |
| 2026-10-14 | **PR 2 du plan — E-03, E-14, E-15, E-20 corrigés, E-23 en partie.** Une seule méthode de sortie, `appState.shutdown`, attend la goroutine d'encodage depuis les trois chemins ; en l'écrivant, l'hypothèse d'E-03 « `SetOnClosed`, que `app.Quit` et `catchTerm` traversent tous deux » s'est révélée fausse pour `catchTerm` (il appelle `driver.Quit`, aucune fenêtre fermée) — d'où un second crochet après `ShowAndRun()` et L-109. `completeEncoding` est la branche de fin unique, testée aux quatre issues sur un vrai canvas. § 4 mis à jour ; DEC-34 amendée (255 n'est pas une annulation). |
| 2026-10-14 | **E-26 et E-27**, trouvés après la clôture de la 13ᵉ passe : #85 à #87 avaient fusionné avec Windows rouge, le gate requis étant *sauté* — donc compté réussi — dès qu'un job de test échouait. Gate corrigé et prouvé ; les quatre défauts Windows qu'il cachait corrigés dans la même PR (#88). Leçons L-111, L-112. |
| 2026-10-14 | **Q-02 mesurée, Q-03 tranchée.** Q-02 : le garde-fou mémoire, chiffré à 4 fils, sous-estimait de ×1,47 à 16 fils ; pente de +4 % par fil mesurée sur 5120×2880 et 2560×1440, 8 et 10 bits, branchée sur les fils réels (DEC-37, DEC-17 remplacée). Q-03 : pas de télémétrie GoPro, sur décision de l'utilisateur (DEC-38). Leçon L-113. |
| 2026-10-20 | **14ᵉ passe, à `4efae3a`**, à la demande de l'utilisateur : analyse complète du produit Rust seul après v0.3.0 (structure, code, sécurité, performances, dépôt), quatre relectures parallèles confrontées au code. § 3terdecies, **30 constats F-01 à F-30**, Q-04 et Q-05. Les plus lourds : sonde de démarrage et Diagnostic non sérialisés avec l'encodage (F-11, F-12), aperçu sondé sans le preset de la conversion (F-07), stderr ffmpeg et JSON d'ffprobe non bornés ou mélangés (F-04, F-05). Plan en sessions, une PR par session. **Aucune modification de code.** |

# Superview — Analyse complète du projet

> Dernière mise à jour : 2026-09-04
> Branche analysée : `Fix-Claude-Code` (identique à `master` @ `e3269e7`)
> **Statut de vérification : VÉRIFIÉ.** Go 1.26.8 a été installé le 2026-09-04
> (`~/.local/go`, sans `sudo`), ainsi que les en-têtes GUI dans un sysroot local — le module
> entier compile, se teste et se lint localement. Les constats ci-dessous ont été confrontés au
> code en exécution : **B-03 s'est révélé faux** et **X-04 surévalué**, les deux sont corrigés
> ci-dessous. Voir [SOURCES.md](SOURCES.md) § 1 pour reproduire l'environnement.

---

## 1. Vue d'ensemble

| Élément | Valeur |
| --- | --- |
| Module Go | `superview` |
| Version Go ciblée | `1.26.0` |
| Nature | Application **GUI uniquement** (Fyne v2.7.3) |
| Plateformes officielles | Windows, Linux (Ubuntu 24.04 LTS+) |
| Dépendance runtime | FFmpeg + FFprobe (binaires externes) |
| Lignes de Go | ~5 880 (dont ~2 220 de tests) |
| Licence | voir `LICENSE` |

### Rôle fonctionnel

Superview convertit une vidéo 4:3 en 16:9 par **distorsion dynamique** : les bords sont étirés
agressivement, le centre conserve son ratio. L'implémentation génère deux cartes de remappage
au format PGM P2 (`x.pgm`, `y.pgm`) puis les passe au filtre `remap` de FFmpeg.

### Cartographie des fichiers

```
/                            paquet main (GUI) — NON couvert par la CI qualité
├── gui_main.go              602 l. — fenêtre Fyne, orchestration, état d'encodage
├── gui_native_dialog_linux.go    81 l. — zenity / kdialog
├── gui_native_dialog_windows.go  60 l. — PowerShell WinForms
│
common/                      paquet métier — seul paquet testé/linté
├── common.go               1080 l. — pipeline complet (ffprobe, PGM, ffmpeg, session)
├── config.go                290 l. — Config YAML + surcharges SUPERVIEW_*
├── security.go              185 l. — validation de chemins, whitelist encodeur
├── hardware.go              200 l. — profil machine, choix encodeur/hwaccel
├── health.go                277 l. — diagnostics système  ⚠️ jamais appelé
├── metrics.go               304 l. — métriques d'encodage
├── observability.go         351 l. — bus d'événements + état global partagé
├── gui_helpers.go            40 l. — helpers parsing GUI
└── command-{windows,other}.go     — SysProcAttr HideWindow
```

### Flux principal

```
main() ──► LoadConfig("superview.yaml")   ⚠️ chemin relatif (voir B-07)
       ──► CheckFfmpeg()                  → version / accels / encoders
       ──► [UI] choix entrée ──► CheckVideo()  → VideoSpecs + Validate()
       ──► [UI] choix sortie
       ──► [UI] profil qualité ──► SetConfig(effectiveCfg)   ⚠️ mute le global (C-05)
       └─► goroutine ──► common.PerformEncoding(...)
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
- **CI substantielle** : build, tests multi-OS, couverture avec seuil, `go vet`, `staticcheck`,
  `govulncheck`, `gofmt`, actions épinglées par SHA (à une exception près, cf. X-05).
- **92 fonctions de test + 4 benchmarks** sur `common/`, avec des points d'injection prévus pour
  la testabilité (`signalNotify`, `signalStop`, `commandStdoutPipe`).

---

## 3. Constats

Convention : **B** bug/correction · **S** sécurité · **C** qualité/architecture ·
**X** build/CI/doc · **O** observabilité/performance · **T** tests.
Sévérité : 🔴 haute · 🟠 moyenne · 🟡 basse.

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
standard — lire son code. Voir [LESSONS.md](LESSONS.md) L-10.

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

## 3bis. Deuxième passe d'analyse (2026-09-04) — axes non couverts

Passe menée **empiriquement** : corpus de fichiers difficiles fabriqué avec FFmpeg, mesures de
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
> et non ajouter un repli sur les métadonnées conteneur comme je le proposais. Voir [LESSONS.md](LESSONS.md) L-26.

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

### N-03 🟠 — Les sources 10 bits sont ramenées à 8 bits sans avertissement

`common/common.go` — `"-filter_complex", "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p"`

Le format de sortie est codé en dur. **Mesuré** : une entrée `yuv420p10le` ressort en
`yuv420p`. Les GoPro récentes (HERO 10 et suivantes) enregistrent en 10 bits, et c'est
précisément le public visé. La perte est silencieuse.

Le filtre `remap` impose une conversion, mais elle pourrait viser `yuv444p10le,yuv420p10le`
quand la source est en 10 bits.

### N-04 🟠 — Les pistes audio supplémentaires sont supprimées

**Mesuré** : 2 pistes AAC en entrée, **1 seule** en sortie. Faute de `-map`, FFmpeg applique sa
sélection automatique et ne retient qu'un flux audio par type. Une caméra enregistrant plusieurs
pistes, ou un fichier post-produit, y perd du contenu sans message.

### N-05 🟡 — La date de prise de vue est perdue

**Mesuré** : `creation_time=2026-01-15T10:30:00Z` présent en entrée, **absent** en sortie (le
tag `comment`, lui, survit). Avec trois entrées dans le `-filter_complex`, FFmpeg ne sait pas de
laquelle hériter les métadonnées globales. `-map_metadata 0` réglerait le cas. Pour de la vidéo
d'action-cam, trier une bibliothèque par date devient impossible.

### N-06 🟡 — Le débit demandé n'est pas borné : dépassement mesuré jusqu'à +56 %

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

### N-07 🟡 — Le décodage matériel n'apporte que ~5 %, pour une complexité et un risque réels

Mesuré sur une source 2880×2160 de 15 s, deux exécutions concordantes :

| Chemin | Durée |
| --- | --- |
| `h264_nvenc` + décodage CUDA | **4,31 s / 4,37 s** |
| `h264_nvenc` + décodage CPU | 4,53 s / 4,58 s |
| `libx264` + décodage CPU | 7,54 s / 7,56 s |

L'**encodage** matériel vaut largement son existence : **×1,7**. Le **décodage** matériel, lui,
n'apporte que **~5 %**, parce que le filtre `remap` tourne sur CPU et impose des transferts
GPU→CPU→GPU qui annulent le gain.

Or c'est le chemin tenté **en premier**, et son échec déclenche une réexécution complète de
l'encodage. Le rapport bénéfice/risque est défavorable : 5 % à gagner, jusqu'à 100 % d'un
encodage à perdre si l'échec survient tardivement (un échec d'initialisation, le cas courant,
coûte peu). À arbitrer : conserver, ou ne tenter le décodage matériel que si un gain réel est
constaté sur la plateforme.

### N-08 🟡 — Jusqu'à 146 Mo de fichiers temporaires, en RAM, sans vérification préalable

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

### N-09 🟡 — Entrée identique à la sortie : message incompréhensible

Aucune garde dans le code. **Mesuré** : FFmpeg refuse de lui-même et **le fichier source reste
intact** — pas de perte de données. Mais l'utilisateur reçoit `ffmpeg failed: exit status 234`.
Une comparaison des chemins avant lancement produirait un message clair.

### N-10 🟡 — Retour utilisateur pauvre pendant un encodage long

- Aucune estimation de temps restant ni de débit : une barre de pourcentage seule, pour une
  opération qui peut durer plusieurs minutes en 4K. Les métriques nécessaires (`EncodingSpeed`,
  temps écoulé) sont pourtant déjà calculées dans `metrics.go`.
- Le chemin du fichier de journal n'est écrit **que dans le journal lui-même**
  (`gui_logger.Info("Superview starting", slog.String("log_file", logPath))`). Un utilisateur qui
  veut joindre ses logs à un rapport d'incident ne peut pas les trouver. Le dialogue Diagnostic
  serait l'endroit naturel pour l'afficher.

---

## 4. Priorités recommandées

| Ordre | Constats | Justification |
| --- | --- | --- |
| 1 | **X-02**, **X-06**, **X-03** | Corrections triviales, sans risque, qui rétablissent l'outillage local et la fiabilité de la documentation d'agent. À faire avant tout le reste. |
| 2 | **B-01** | Seul défaut pouvant figer l'application sans diagnostic. Correctif localisé (~5 lignes). |
| 3 | **B-02** + **O-01** | Ensemble : la configuration n'étant pas lue, le bridage temps réel s'applique à tout le monde. Le gain de performance perçu est majeur. |
| 4 | **X-01** | Débloque la détection automatique de B-03, B-05, B-06 et prévient les régressions futures dans la GUI. |
| 5 | **B-03**, **B-05**, **B-06** | Bugs GUI réels, chacun sur un chemin de repli ou un cas limite. |
| 6 | **S-01**, **S-02** | Faux positifs de validation qui bloquent des fichiers légitimes. S-02 demande un arbitrage produit. |
| 7 | **C-01** à **C-05** | Nettoyage : soit brancher, soit supprimer. Décision de périmètre à prendre avec le mainteneur. |
| 8 | **T-01**, **T-03**, **T-02** | Filet de sécurité durable, à installer une fois les correctifs ci-dessus posés. |

**Bloquant transverse levé le 2026-09-04** : Go 1.26.8 est installé, le module entier
compile et se vérifie localement. Voir [SOURCES.md](SOURCES.md) § 1.

## 4bis. État d'avancement au 2026-09-04

| Statut | Constats |
| --- | --- |
| ✅ **Corrigé et vérifié** (32) | B-01, B-02, B-04, B-05, B-06, B-07, B-08, S-01, S-02, S-03, S-04, C-01, C-02, C-03, C-04, C-07, C-08, X-01, X-02, X-03, X-04, X-05, X-06, X-07, X-08, O-01, O-02, O-03, O-04, T-01, T-02, T-03 |
| ⬜ **Invalidé** (1) | B-03 — le constat était faux, voir ci-dessus |
| ✅ **Corrigé et vérifié — 2ᵉ passe** (4) | C-05, C-06, O-05, O-06 |
| ⏸️ **Restant** | aucun |

Les quatre arbitrages de périmètre ont été soumis à l'utilisateur le 2026-09-04 et tranchés :
S-02 → résoudre les liens symboliques puis valider la cible · C-01 → brancher `health.go` sur un
bouton « Diagnostic » · C-03 → retirer les options sans effet · C-04 → exposer « squeeze » par
une case à cocher.

Détail des mesures obtenues :

| Indicateur | Avant | Après |
| --- | --- | --- |
| Couverture (module entier) | 33,7 % *(chiffre périmé et faux)* | **54,6 %** |
| Seuil de couverture en CI | 30 % sur `./common` | **50 % sur `./...`** |
| Portée de l'analyse CI | `./common` uniquement | **`./...`** (paquet GUI inclus) |
| golangci-lint | v1.64.8 (fin de vie), 0 alerte | **v2.13.2, 0 alerte** après 9 corrections |
| `GeneratePGM` (1440×1080) | 67,2 ms | **16,9 ms** (×4, sortie identique au bit près) |
| Encodage bridé au temps réel | oui, par défaut (`-re`) | **non** |
| Tests du paquet `main` | 0 | **9** |
| Tests d'intégration FFmpeg | 0 | **2** |
| Fonctionnalités inatteignables | squeeze, `health.go` | **exposées dans la GUI** |
| Options de config sans effet | 3 | **0** |
| Configuration | globale mutable, écrasée à chaque run | **passée en paramètre** |
| API exportée sans appelant | 9 symboles | **0** |
| Dispatch des événements | 1 goroutine par événement, ordre non garanti | **synchrone et ordonné** |

---

## 5. Journal des révisions de ce document

| Date | Modification |
| --- | --- |
| 2026-09-04 | Création. Analyse statique complète de `Fix-Claude-Code` @ `e3269e7`. 30 constats. |
| 2026-09-04 | Application des correctifs par priorité. Toolchain installée, tout vérifié en exécution. B-03 invalidé, X-04 revu à la baisse. Ajout du § 4bis. |

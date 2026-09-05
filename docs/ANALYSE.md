# Superview — Analyse complète du projet

> **Journal d'audit, pas documentation de référence.** Ce fichier consigne cinq
> passes d'analyse et les 67 constats qu'elles ont produits, avec ce qui a été
> décidé pour chacun. Ce sur quoi on peut s'appuyer aujourd'hui est dans
> [CONTRATS.md](CONTRATS.md) ; ce qu'il faut avoir lu avant de corriger est dans
> [LECONS.md](LECONS.md).
>
> Dernière passe : 5ᵉ (R-01 à R-07), close le 2026-09-05 sur `5d3b6f9`.
> Les § 3 et § 3bis portaient sur `e3269e7`, le § 3ter sur `001d250`.
>
> **Ordre de lecture** : § 3 (1ʳᵉ passe, statique) → § 3bis (3ᵉ passe, empirique,
> N-xx) → § 3ter (4ᵉ passe, P-xx) → § 3quater (5ᵉ passe, R-xx) → **§ 4 (état
> d'avancement, le seul tableau qui fasse foi)**.

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

Comptes de lignes mesurés à `a672bc6` (la CI qualité couvre `./...`, cf. X-01) :

```
/                            paquet main (GUI) — 28 tests, couverture 25,2 %
├── gui_main.go             1070 l. — fenêtre Fyne, orchestration, état d'encodage
│                                     dont ~540 l. dans main() : construction de
│                                     widgets, délibérément non découpée (P-10)
├── gui_main_test.go         843 l.
├── gui_native_dialog_linux.go    81 l. — zenity / kdialog       (non testé, assumé)
├── gui_native_dialog_windows.go  60 l. — PowerShell WinForms    (non testé, assumé)
│
common/                      paquet métier — couverture 81,5 %, vert sous -race
├── common.go               1587 l. — pipeline complet (ffprobe, PGM, ffmpeg, session)
├── observability.go         391 l. — bus d'événements + dernier état publié
├── config.go                308 l. — Config YAML + surcharges SUPERVIEW_*
├── metrics.go               296 l. — métriques d'encodage
├── health.go                278 l. — diagnostics système, branché sur le bouton Diagnostic
├── security.go              231 l. — validation de chemins, whitelist encodeur
├── hardware.go              200 l. — profil machine, choix encodeur/hwaccel
├── gui_helpers.go            40 l. — helpers parsing GUI
├── command-{windows,other}.go     — SysProcAttr HideWindow
├── health_disk_{unix,windows}.go  — espace disque libre par plateforme
└── testdata/ffprobe/              — sorties ffprobe enregistrées (12 cas)
```

> `common.go` porte à lui seul un tiers du code de production et mêle au moins six
> responsabilités : découverte des binaires externes, logger, interfaces UI, cycle de
> vie de la session temporaire, géométrie des cartes PGM, orchestration ffmpeg. Le
> découpage en `pgm.go` et `tools.go` est identifié, non fait, et sans urgence : le
> fichier est couvert à 81,5 % et `pgm_golden_test.go` fige déjà la géométrie.

### Flux principal

```
main() ──► ResolveConfigPath() ──► LoadConfig()   ✅ exécutable / ~/.config / cwd (B-02)
       ──► CheckFfmpeg(cfg)               → version / accels / encoders
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
> Enfin, le préfixe **Q-xx** (§ 5bis) ne désigne **pas** un constat mais une **question
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
> et non ajouter un repli sur les métadonnées conteneur comme je le proposais. Voir [LECONS.md](LECONS.md) L-26.

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
([`common/common.go:685`](../common/common.go#L685)) et se corrigent ensemble. Reproduction et
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
([`common/common.go`, `CheckVideo`](../common/common.go)).

### P-01 ✅ — ~~La case « squeeze » n'est jamais réactivée après un encodage réussi~~ — **CORRIGÉ**

[`gui_main.go:469`](../gui_main.go#L469) (échec) contre
[`gui_main.go:487-492`](../gui_main.go#L487-L492) (succès)

Les deux branches de fin d'encodage réactivent les widgets une par une. La branche d'échec
appelle `squeezeCheck.Enable()` ; **la branche de succès l'a oublié**. Après un premier
encodage réussi, la case *Source already stretched (GoPro SuperView)* reste grisée jusqu'au
redémarrage de l'application — c'est-à-dire que la fonctionnalité exposée par C-04 devient
inatteignable dès le second fichier traité.

C'est une conséquence directe de P-10 : la transition d'état est dupliquée à la main dans deux
branches, sans rien pour garantir qu'elles restent symétriques.

### P-02 ✅ — ~~Course de données sur le canal d'annulation~~ — **CORRIGÉ**

`cancelEncoding` est écrit depuis le fil UI ([`gui_main.go:397`](../gui_main.go#L397),
`cancelEncoding = nil`) et lu depuis la goroutine d'encodage
([`gui_main.go:459`](../gui_main.go#L459)) :

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

[`common/common.go:903-955`](../common/common.go#L903-L955)

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
| [`common/config.go:19-23`](../common/config.go#L19-L23) | `// Bitrate constraints in bytes/second`, `// 100k bytes/sec (~0.1 Mbps)` |
| [`common/config.go:293-295`](../common/config.go#L293-L295) | `"  Min Bitrate: %d bytes/sec (%.2f Mbps)"` |
| [`common/common.go:189`](../common/common.go#L189) | `// GetBitrate returns the desired output bitrate in bytes/second.` |
| [`common/common.go:618-621`](../common/common.go#L618-L621) | messages d'erreur **montrés à l'utilisateur** : `"bitrate %d is below minimum %d bytes/second"` |
| [`common/metrics.go:12`](../common/metrics.go#L12), `:26`, `:34` | `All bitrates in bytes/sec` |
| [`superview.yaml`](../superview.yaml) | `# Bitrate constraints in bytes/second` |

Un utilisateur qui règle `max_bitrate` en croyant écrire des octets se trompe d'un facteur 8.
Correction purement textuelle, sans aucun changement de comportement — mais elle touche aussi
`common/common_test.go:29` et `common/config_test.go:26`.

### P-06 ✅ — ~~`EncodingSpeed` repose sur un 30 fps codé en dur~~ — **CORRIGÉ**

[`common/metrics.go:261`](../common/metrics.go#L261)

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

[`common/metrics.go:125`](../common/metrics.go#L125) calcule le temps restant à chaque mise à
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

- [`common/observability.go:232`](../common/observability.go#L232) : `RecordEvent` prend
  `r.mu.Lock()` (verrou d'**écriture**) pour une simple itération en lecture sur `r.handlers`.
  `RLock` suffit, et c'est déjà ce que font `RecordProgress`, `RecordError` et
  `RecordCompletion` juste en dessous.
- Chaque ligne de progression FFmpeg — plusieurs par seconde — traverse
  `RecordEncodingProgress`, qui alloue une `map[string]interface{}`, formate une chaîne et
  prend deux verrous, pour finir le plus souvent ignorée par un handler réglé sur `info`.
  Un court-circuit sur `logger.Enabled(ctx, slog.LevelDebug)` supprime ce coût.
- [`common/common.go`](../common/common.go) : `-threads runtime.NumCPU()` est posé même quand
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

[`common/common.go`](../common/common.go) — `buildEncodeBaseArgs`, option `-threads`

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
constantes de paquet : porter `actionButtonWidth` à 175 fait désormais échouer le test.

`fakeHangingFFmpeg` utilisait par ailleurs `os.Setenv` avec restauration différée, qui
fuit sur panique et ne peut pas refuser un `t.Parallel` — remplacé par `t.Setenv`.

### R-08 🟠 **consigné** — Le correctif R-06 n'est pas publié

`HEAD` est deux commits après le tag `v0.2.3`. Les binaires actuellement en ligne
annoncent donc toujours `0.2.3 (9c1a8c5, modified)` — le défaut même que `#43` corrige.
Le correctif existe et n'est pas entre les mains des utilisateurs. Une v0.2.4 est due,
et `RELEASE_NOTES.md` doit être réécrit avant tout `workflow_dispatch` (le run refuse de
démarrer sinon, ce qui est le garde-fou voulu).

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
| ✅ **Corrigé et vérifié — 6ᵉ passe** (13) | D-01 à D-09, V-01 à V-04 |
| 📌 **Consigné, hors périmètre — 6ᵉ passe** (4) | R-08 à R-11 — la release a été mise hors périmètre pour ce chantier. **R-08 est le seul qui appelle une action** : le correctif R-06 n'est pas publié. |
| ⏸️ **Ouvert** | *aucun.* |
| ✅ **Tranchée** (1) | Q-01 — mesurée : 1,6 → 4/3, § 5bis |

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

## 5bis. Questions ouvertes

Convention : **Q-xx**. Ce ne sont **pas** des constats. Une question ouverte est un point dont on
ignore encore s'il constitue un défaut — la consigner évite qu'elle se reperde, sans l'inscrire
au passif du projet.

### Q-01 — D'où vient le facteur 1,6 du profil de qualité « Balanced » ?

[`gui_main.go:58-65`](../gui_main.go#L58-L65)

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

## 5. Journal des révisions de ce document

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
| 2026-09-04 | Ajout du § 5bis « Questions ouvertes » et de la convention **Q-xx**, distincte des constats. Première entrée : **Q-01**, le facteur 1,6 du profil « Balanced », qui vaut exactement le ratio géométrique 4/3 majoré de 20 % sans que cette marge soit documentée. |

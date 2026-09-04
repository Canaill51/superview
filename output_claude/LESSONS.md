# Superview — Journal des corrections et leçons

> Dernière mise à jour : 2026-09-04
> **Ce fichier se met à jour après CHAQUE correction appliquée au code.**
> Procédure : (1) ajouter une entrée en § 3 avec le gabarit ci-dessous, (2) cocher la ligne
> correspondante dans la file d'attente § 4, (3) si la correction révèle une règle réutilisable,
> l'ajouter en § 2.

Les identifiants `B-xx`, `S-xx`, `C-xx`, `X-xx`, `O-xx`, `T-xx` renvoient à
[ANALYSE_PROJET.md § 3](ANALYSE_PROJET.md).

---

## 1. Gabarit d'entrée

```markdown
### [AAAA-MM-JJ] ID-xx — Titre court

| | |
| --- | --- |
| **Constat** | ID-xx (lien vers ANALYSE_PROJET.md) |
| **Fichiers** | `chemin:lignes` |
| **Commit** | `sha` ou « non commité » |
| **Vérification** | `go build` ✅/❌ · `go vet` ✅/❌ · `go test` ✅/❌ · test manuel ✅/❌/n.a. |

**Symptôme** — ce qui n'allait pas, observable.

**Cause racine** — pourquoi, pas seulement où.

**Correctif** — ce qui a été changé, et ce qui a été délibérément laissé.

**Leçon** — la règle généralisable. Rien à généraliser → écrire « aucune ».
```

---

## 2. Leçons permanentes

Règles issues de l'analyse du dépôt, à appliquer à toute modification. Chacune est datée ;
elles s'enrichissent au fil des corrections.

### L-01 — Aucune modification n'est validée sans toolchain — 2026-09-04
`go` n'est pas installé sur la machine de travail. Tant que ce n'est pas corrigé, toute
livraison de code est **non vérifiée** et doit être annoncée explicitement comme telle à
l'utilisateur. Ne jamais écrire « corrigé » quand on veut dire « écrit mais non compilé ».
Procédure d'installation : [SOURCES.md § 1](SOURCES.md).

### L-02 — La CI de qualité ne couvre pas le paquet `main` — 2026-09-04
`lint.yml` et `test.yml` ciblent `./common` uniquement. Une modification de `gui_main.go` ou
des fichiers de dialogue natif n'est vérifiée par **rien** hormis `go build ./...`.
→ Toujours lancer `go vet ./...` (et non `./common`) en local après avoir touché la GUI.
Constat X-01.

### L-03 — L'ordre des arguments FFmpeg change leur sens — 2026-09-04
Une option placée avant `-i` s'applique au fichier d'entrée qui suit ; placée après, à la
sortie. C'est l'origine de B-04 (`-threads` configure le décodeur alors que le code croit
régler l'encodeur). Ne jamais réordonner `buildEncodeBaseArgs` « pour la lisibilité » sans
consulter https://ffmpeg.org/ffmpeg.html#Main-options.

### L-04 — La documentation du dépôt n'est pas une source fiable — 2026-09-04
`.github/copilot-instructions.md`, le `Makefile` et `build.sh` référencent tous
`superview-gui.go`, fichier qui n'existe plus depuis le portage Linux (`76a8341`).
`copilot-instructions.md` annonce aussi Go 1.25 (réalité : 1.26) et un plafond de bitrate de
50 M (réalité : 200 M). → Vérifier dans le code, jamais dans la doc. Constats X-02, X-03, X-08.

### L-05 — Vérifier qu'un symbole est vivant avant de le corriger — 2026-09-04
Le dépôt contient une part notable de code exporté sans appelant : `health.go` en entier
(277 l.), `EncodingOptions`, la branche `squeeze`, `min_video_*`, `quality_preset`.
Réflexe avant d'investir dans un correctif :
```bash
grep -rn "Symbole" --include='*.go' . | grep -v '_test.go'
```
Deux résultats (déclaration + commentaire) = code mort : la bonne correction est peut-être la
suppression, ou le branchement — c'est une décision de périmètre à soumettre à l'utilisateur,
pas à trancher seul. Constats C-01 à C-06.

### L-06 — Toute mise à jour d'UI depuis une goroutine passe par `fyne.Do` — 2026-09-04
Fyne v2.7 l'exige. Le code respecte la règle (`GUIHandler.ShowError`, `ShowInfo`,
`ShowProgress`, et les blocs dans la goroutine d'encodage). Une modification qui touche un
widget depuis `PerformEncoding` ou un callback de progression doit conserver cet
enveloppement, sinon le comportement est indéfini (crash ou corruption d'affichage).

### L-07 — Ne pas écrire le profil de couverture à la racine — 2026-09-04
`coverage.out` est versionné à tort et périmé (X-06). Utiliser `-coverprofile=/tmp/cov.out`
pour éviter de polluer le diff avec un artefact généré.

### L-08 — `strings.Split("", sep)` retourne `[""]`, pas une tranche vide — 2026-09-04
Origine de B-05 (option fantôme `" encoder"` dans la liste déroulante). `hardware.go:38`
contient déjà le helper correct, `splitCSV`, qui filtre les entrées vides — le réutiliser
plutôt que d'appeler `strings.Split` directement sur une valeur issue de la map `ffmpeg`.

### L-09 — Les mathématiques de `GeneratePGM` ont une source de vérité externe — 2026-09-04
La formule d'offset vient de l'implémentation Python de Banelle
(https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview). Les divisions
entières `(outX/16)*7` **font partie de l'algorithme d'origine** : les « corriger » en
arithmétique flottante changerait le rendu visuel. Toute modification doit être validée sur
une vidéo de référence, pas seulement par lecture du code.

### L-10 — Lire le code d'une bibliothèque, pas la forme canonique du standard — 2026-09-04
J'avais affirmé que Fyne produit `file:///C:/...` sous Windows (forme RFC canonique) et donc
qu'en retirer `file://` laissait un chemin invalide. **C'était faux** : `NewFileURI` stocke
`C:/Users/...` sans barre initiale et `String()` concatène `scheme + "://" + path`, soit
`file://C:/...`. Le code d'origine était correct.
→ Avant de déclarer un bug dans l'usage d'une bibliothèque, ouvrir son source dans
`$(go env GOMODCACHE)`. Constat B-03, invalidé.

### L-11 — Exécuter l'outil plutôt que présumer son état — 2026-09-04
J'avais écrit que le job `lint` était « très probablement en échec ». Exécution faite : il
**passait**. Le vrai problème était plus discret — `@latest` sur l'ancien chemin de module
figeait golangci-lint sur la dernière v1 sans jamais le signaler. Le diagnostic présumé et le
diagnostic réel menaient à des correctifs différents. Constat X-04.

### L-12 — Une refactorisation d'algorithme se prouve par empreinte, pas par relecture — 2026-09-04
L'optimisation de `GeneratePGM` (×4) reposait sur une propriété non évidente : la carte X est
invariante par ligne. La seule preuve acceptable était de comparer les SHA-256 des fichiers
produits avant/après, sur 4 jeux de dimensions dont deux impaires. Ces empreintes sont
désormais figées dans `common/pgm_golden_test.go` : toute modification future de la formule
échouera bruyamment. Voir [[L-09]]. Constat O-03.

### L-13 — Une vérification locale complète vaut mieux qu'attendre la CI — 2026-09-04
`sudo` étant indisponible, Go a été installé dans `~/.local/go` et les en-têtes GUI extraits
de paquets `.deb` vers un sysroot local (`/tmp/glue/sysroot`) via `apt-get download` + `dpkg -x`,
sans droits root. Cela a permis de compiler, tester et linter **le paquet GUI**, ce qui a
révélé 4 défauts réels qu'aucun outil ne voyait. Sans cela, étendre la CI à `./...` aurait été
un pari à l'aveugle susceptible de faire rougir la CI du projet.
Procédure complète : [SOURCES.md § 1](SOURCES.md).

### L-22 — Ne pas introduire d'API en prévision d'un usage futur — 2026-09-04
J'ai ajouté `ResetToolResolutionCache` en 1ʳᵉ passe « pour un futur bouton Réessayer », avec un
test mais aucun appelant. Résultat : elle est ressortie comme code mort à la passe suivante, et
surtout le correctif O-04 dont elle faisait partie était **incomplet** — cesser de cacher les
échecs ne sert à rien si rien ne re-sonde. Soit on branche l'API dans le même changement, soit
on ne l'écrit pas. Constat C-06.

### L-23 — Un test de concurrence se conçoit pour échouer sous `-race` — 2026-09-04
Pour figer le dispatch synchrone (O-05), le handler de test accumule les appels dans une slice
**sans verrou**. Si quelqu'un rétablit `go handler.OnEvent(...)`, le test ne se contente pas de
devenir instable : `go test -race` le signale de façon déterministe. Un test qui se protège
lui-même de la concurrence ne détecterait pas la régression.

### L-24 — Supprimer du code mort testé fait baisser la couverture — 2026-09-04
Retirer les neuf symboles sans appelant a fait passer la couverture de 54,7 % à 53,8 % : leurs
tests disparaissaient avec eux. C'est sain, mais à anticiper quand un seuil de CI est proche —
ici la marge restait de 3,8 points. Ne pas conserver du code mort pour flatter une métrique.

### L-20 — Dans une fenêtre à taille fixe, ajouter un widget est un changement de mise en page — 2026-09-04
`window.SetFixedSize(true)` n'adapte rien : un contenu trop large est rogné sans erreur ni
avertissement. Le 6e bouton de la barre d'outils portait celle-ci à ~1220 px pour 900 px
disponibles. Après tout ajout dans une rangée, recalculer la largeur minimale — ou mieux, la
figer dans un test (`TestToolbarFitsWindow`). Constat C-01.

### L-21 — Un test qui contredit une décision produit se réécrit, ne se contourne pas — 2026-09-04
`TestSymlinkRejection` encodait l'ancienne politique de rejet des liens symboliques. Après
l'arbitrage inverse, la bonne réponse était de le remplacer par des tests décrivant la nouvelle
politique **et ses limites** (lien cassé, lien vers un répertoire, fichier non régulier), pas de
l'assouplir ni de le supprimer. Constat S-02.

### L-15 — Traiter le cas d'erreur générique avant le cas particulier — 2026-09-04
Dans une boucle de lecture, `if err == io.EOF { break }` sans branche pour les autres erreurs
est un blocage en puissance : toute erreur non prévue fait reboucler à l'infini. Sortir sur
`err != nil`, puis distinguer EOF pour le journal. Utiliser `errors.Is`, jamais `==`.
Constat B-01.

### L-16 — Aucun chemin relatif dans une application graphique — 2026-09-04
Le répertoire de travail d'une application lancée par un environnement de bureau est
arbitraire. Toute ressource doit être résolue depuis `os.Executable()` ou
`os.UserConfigDir()` / `os.UserCacheDir()`, le CWD ne servant que de repli pour le
développement. Constat B-02.

### L-17 — Prouver l'équivalence d'un parsing sur une entrée réelle — 2026-09-04
Avant de raisonner sur les cas limites d'un nouveau parseur, exécuter l'ancien et le nouveau
côte à côte sur une sortie réelle de l'outil et comparer. Cela a confirmé 13 encodeurs
identiques dans le même ordre sur FFmpeg 8.0.1, transformant une refactorisation risquée en
changement démontré sans effet. Constat B-07.

### L-18 — Un canal ne se ferme qu'à un seul endroit — 2026-09-04
Deux chemins d'annulation fermaient le même canal ; l'un remettait la variable à `nil`, l'autre
non, d'où une `panic: close of closed channel` possible. Quand plusieurs chemins peuvent
annuler, ils appellent tous la même fonction idempotente. Constat B-06.

### L-19 — Une validation qui rejette du légitime coûte sans rien acheter — 2026-09-04
`strings.Contains(path, "..")` rejetait `vacances..2024.mp4` sans rien empêcher de plus qu'un
contrôle par composant, le chemin étant déjà absolu et passé par `filepath.Clean`. Valider la
**structure**, pas la forme textuelle. Constat S-01.

### L-14 — `-re` n'a pas sa place dans une transcodification vers fichier — 2026-09-04
Ce drapeau FFmpeg bride la lecture de l'entrée à sa cadence native. Utile pour alimenter un
flux temps réel, il ne fait que plafonner un encodage fichier à la durée du clip. Il était
actif dans le mode nommé « safe », ce qui suggérait à tort une garantie de robustesse.
Constat O-01.

---

## 3. Corrections appliquées

<!-- Les entrées les plus récentes vont en tête de cette section, juste sous ce commentaire. -->

### [2026-09-04] 2ᵉ passe — C-05, C-06, et deux constats découverts en chemin

Vérifié en exécution : suite CI complète, `go test ./... -race`, démarrage de la GUI avec
inspection du journal, conversion réelle de bout en bout.

#### C-05 — La configuration ne transite plus par un global mutable

| | |
| --- | --- |
| **Fichiers** | `common/config.go`, `common/common.go`, `common/health.go`, `gui_main.go` |

**Cause racine** — `var currentConfig` avec `GetConfig()`/`SetConfig()`. La GUI y écrivait avant
chaque encodage, ce qui produisait trois défauts distincts : (1) le `video_preset` de
l'utilisateur était **systématiquement écrasé** par le profil qualité, sans que rien ne le
signale ; (2) le global n'était jamais restauré, donc l'état divergeait après le premier
encodage ; (3) il était lu depuis la goroutine d'encodage pendant que le fil UI pouvait encore
y écrire.

**Correctif** — `Config` passée en paramètre à `CheckFfmpeg`, `InitEncodingSession`,
`EncodeVideo`, `PerformEncoding` et `CheckHealth`. `configOrDefault` évite aux appelants de
tester `nil`. Les trois symboles globaux ont disparu.

**Règle de résolution du preset, désormais explicite et testée** : un `video_preset` renseigné
dans la configuration l'emporte sur celui du profil qualité ; le profil ne s'applique que si
l'utilisateur n'a rien précisé.

#### C-06 — Élaguer, mais aussi brancher

Neuf symboles exportés sans appelant. La bonne réponse n'était pas « tout supprimer » : trois
d'entre eux comblaient un vrai manque une fois branchés.

| Symbole | Décision |
| --- | --- |
| `GetHeader` | supprimé — vestige de la CLI retirée |
| `ValidateVideoFile` | supprimé — redondant avec ce que fait déjà `PerformEncoding` |
| `EncodingMetrics.ToJSON` | supprimé — `LogMetrics` écrit déjà dans le journal |
| `CreateDefaultConfig` | supprimé — `LoadConfig("")` fournit déjà les défauts |
| `GetEventHistory`, `ClearHistory` | supprimés avec l'historique lui-même (voir O-06) |
| `LogHealth` | **branché** — le bouton Diagnostic écrit aussi le rapport dans le journal |
| `ResetToolResolutionCache` | **branché** — bouton « Retry » sur le dialogue d'absence de ffmpeg |
| `RecordEncodingEvent` | **branché** — émet enfin l'événement « start », absent du cycle de vie |

Le cas `ResetToolResolutionCache` mérite d'être noté : je l'avais **moi-même introduite sans
appelant** lors de la 1ʳᵉ passe (O-04), en prévision d'un bouton « Réessayer ». C'était de
l'API spéculative. Sans re-sondage, cesser de mettre en cache les échecs ne servait à rien : le
correctif O-04 était incomplet. Le dialogue propose désormais « Retry », qui vide le cache,
re-sonde ffmpeg et réactive l'interface si l'outil vient d'être installé.

**Leçon** — [[L-22]].

#### O-05 — Dispatch des événements en goroutines *(constat découvert en traitant C-06)*

Les quatre méthodes de `EventRecorder` faisaient `go handler.OnX(...)`. Comme
`RecordEncodingProgress` est appelé plusieurs fois par seconde, cela créait une goroutine par
événement et **ne garantissait aucun ordre** ; les derniers événements pouvaient être perdus si
le processus se terminait avant leur exécution. Inoffensif tant que les journaux allaient dans
`io.Discard`, gênant depuis O-02.

Dispatch rendu synchrone. `TestEventRecorder_DispatchIsSynchronousAndOrdered` enregistre les
appels **sans verrou** : un retour à l'asynchrone déclencherait le détecteur de courses.

#### O-06 — Historique d'événements en écriture seule

`EventRecorder` retenait 1000 événements que rien ne lisait. Le fichier de journal contient la
même information, persistée. Supprimé.


### [2026-09-04] Arbitrages utilisateur — S-02, C-01, C-03, C-04

Quatre décisions de périmètre soumises à l'utilisateur, qui a tranché. Toutes vérifiées en
exécution (suite CI complète + démarrage GUI + conversion réelle).

#### S-02 — Liens symboliques : résolus puis validés *(décision : résoudre la cible)*

`isValidInputPath` appelle désormais `filepath.EvalSymlinks` et valide la **destination**.
Ajout au passage d'un contrôle `Mode().IsRegular()` : un périphérique caractère comme
`/dev/zero` ferait lire FFmpeg indéfiniment.

Le test `TestSymlinkRejection`, qui encodait l'ancienne politique, a été **remplacé** par quatre
tests décrivant la nouvelle : lien valide accepté, lien cassé rejeté, lien vers un répertoire
rejeté, fichier non régulier rejeté. Un test qui contredit une décision produit doit être
réécrit, jamais contourné.

#### C-01 — `health.go` branché *(décision : bouton « Diagnostic »)*

Bouton ajouté à la barre d'outils : il exécute `CheckHealth()` **hors du fil UI** (les contrôles
lancent `ffmpeg`), puis affiche `GetHealthReport()` dans une boîte défilante monospace. Les
277 lignes du module deviennent utiles et un rapport d'incident devient exploitable.

**Effet de bord attrapé** — passer de 5 à 6 boutons portait la barre à ~1220 px dans une fenêtre
fixée à 900 px : les boutons auraient été silencieusement rognés (`SetFixedSize(true)` n'ajuste
rien). Boutons ramenés à 150 px, fenêtre à 980×470, et `TestToolbarFitsWindow` ajouté pour que
le prochain bouton ne repasse pas inaperçu. Mesure actuelle : 920 px pour 980 disponibles.

**Leçon** — [[L-20]] : dans une fenêtre à taille fixe, ajouter un widget dans une rangée est un
changement de mise en page à vérifier, pas une simple addition.

#### C-03 — Options mortes retirées *(décision : les retirer)*

`min_video_width`, `min_video_height` et `quality_preset` supprimées de `Config`, de
`superview.yaml`, du `README` et des tests. La GUI conserve ses profils « Fast »/« Balanced ».
Plus aucune promesse non tenue dans la configuration.

#### C-04 — Squeeze exposé *(décision : case à cocher)*

Case « Source already stretched (GoPro SuperView) » ajoutée au formulaire d'options, préférence
persistée (`ui.squeeze_source`), désactivée pendant l'encodage comme les autres contrôles.
`GUIHandler.GetSqueeze()` retourne enfin une vraie valeur.

**Vérifié fonctionnellement** : conversion réelle avec squeeze activé — une entrée 640×480
ressort en 640×480 (largeur conservée, centre dé-étiré), ce qui est le comportement attendu de
cette branche, par opposition au 852×480 du mode normal.


### [2026-09-04] Passe d'application par priorité — 28 constats traités

Environnement de vérification établi au préalable (L-13) : Go 1.26.8 dans `~/.local/go`,
sysroot GUI local, FFmpeg 8.0.1 système. **Tous les correctifs ci-dessous ont été vérifiés en
exécution** : `gofmt`, `go build ./...`, `go vet ./...`, `staticcheck ./...`,
`golangci-lint v2 ./...`, `govulncheck ./...`, `go test ./... -race`, plus un lancement réel de
la GUI et une conversion 4:3 → 16:9 de bout en bout.

---

#### B-01 — Boucle infinie sur erreur de lecture non-EOF

| | |
| --- | --- |
| **Fichiers** | `common/common.go:770-791` |
| **Vérification** | build ✅ · vet ✅ · test ✅ · **garde de régression prouvée** ✅ |

**Symptôme** — toute erreur autre que `io.EOF` sur le pipe de progression faisait tourner la
goroutine de lecture indéfiniment ; `readDone` n'était jamais fermé, le `select` en aval
bloquait pour toujours, l'application se figeait en consommant 100 % d'un cœur.

**Cause racine** — `if err == io.EOF { break }` ne traitait qu'un seul cas d'erreur et ne
prévoyait aucune sortie pour les autres.

**Correctif** — sortie sur **toute** erreur non nulle, avec `errors.Is(err, io.EOF)` pour
distinguer la fin normale (journal `Debug`) d'un échec (journal `Warn`).

**Preuve** — `TestEncodeVideo_ProgressReaderNonEOFErrorDoesNotHang` injecte un lecteur en
erreur permanente. Correctif retiré : le test se fige puis échoue à 20 s. Correctif remis : il
passe en 0,06 s. La garde a donc été validée dans les deux sens.

**Leçon** — [[L-15]] : dans une boucle de lecture, traiter le cas d'erreur générique avant le
cas particulier. Un `break` sur une seule valeur d'erreur est un blocage en puissance.

---

#### B-02 — Configuration introuvable en installation réelle

| | |
| --- | --- |
| **Fichiers** | `common/config.go` (+`ResolveConfigPath`), `gui_main.go` |
| **Vérification** | build ✅ · test ✅ · **test fonctionnel depuis `/`** ✅ |

**Symptôme** — `LoadConfig("superview.yaml")` résolvait un chemin relatif contre le répertoire
de travail. Lancée depuis un lanceur `.desktop` ou le menu Démarrer, l'application ne lisait
jamais sa configuration.

**Correctif** — `ResolveConfigPath()` sonde dans l'ordre : `$SUPERVIEW_CONFIG`, le répertoire
de l'exécutable, `os.UserConfigDir()/superview/`, puis le répertoire courant. Le chemin retenu
est journalisé.

**Preuve** — binaire et `superview.yaml` copiés dans `/tmp/installdir`, lancement avec
`cd / && /tmp/installdir/superview-gui`. Journal :
`Configuration loaded from file path=/tmp/installdir/superview.yaml`. Avant le correctif, rien
n'était trouvé.

**Leçon** — [[L-16]] : un chemin relatif dans une application graphique est un bug latent ; le
CWD d'une application lancée par l'environnement de bureau est arbitraire.

---

#### O-01 — Encodage bridé au temps réel par défaut

| | |
| --- | --- |
| **Fichiers** | `common/common.go` (`buildEncodeBaseArgs`), `common/config.go`, `superview.yaml`, `README.md` |
| **Vérification** | test ✅ · **conversion réelle** ✅ |

**Symptôme** — le mode par défaut `safe` ajoutait `-re`, plafonnant tout encodage à la durée
du clip. Combiné à B-02, cela s'appliquait à la quasi-totalité des utilisateurs.

**Correctif** — `-re` supprimé des deux modes, avec un commentaire expliquant pourquoi.
`performance_mode` ne pilote plus que la stratégie audio (AAC vs copie), documentation mise à
jour partout. Le paramètre `safePerformanceMode`, devenu inutile dans `buildEncodeBaseArgs`, a
été retiré de la signature plutôt que neutralisé par un `_ =`.

**Leçon** — [[L-14]].

---

#### O-03 — `GeneratePGM` 4× plus rapide, sortie identique au bit près

| | |
| --- | --- |
| **Fichiers** | `common/common.go`, `common/pgm_golden_test.go` (nouveau) |
| **Vérification** | **SHA-256 identiques sur 4 jeux de dimensions** ✅ · benchmark ✅ |

**Cause racine** — deux redondances : (1) la carte X est **invariante par ligne** — `sx`, `tx`
et `offset` ne dépendent que de `x` — mais était recalculée 1080 fois à l'identique ;
(2) chaque ligne de la carte Y appelait `strconv.AppendInt` une fois par pixel pour formater
toujours le même nombre. Les écritures étaient de plus non tamponnées.

**Correctif** — ligne X construite une seule fois puis écrite `outY` fois ; nombre de la ligne
Y formaté une fois par ligne ; `bufio.Writer` de 1 MiB sur les deux fichiers, avec `Flush`
vérifié.

**Mesure** — 1440×1080 : **67,2 ms → 16,9 ms**.

**Preuve** — empreintes SHA-256 et tailles capturées **avant** modification sur 4 cas
(1440×1080 et 641×481, avec et sans squeeze), comparées après : strictement identiques. Elles
sont figées dans `TestGeneratePGM_Golden`, accompagné de
`TestGeneratePGM_XMapIsRowInvariant` qui documente la propriété exploitée.

**Leçon** — [[L-12]].

---

#### B-07 — Parsing de la sortie FFmpeg résistant aux formats inattendus

| | |
| --- | --- |
| **Fichiers** | `common/common.go` (`parseFFmpegVersion`, `parseFFmpegEncoders`) |
| **Vérification** | **test différentiel contre FFmpeg 8.0.1 réel** ✅ |

**Symptôme** — `strings.Split(version, " ")[2]` et `enc[2]` paniquaient sur une sortie FFmpeg
non standard ; le saut d'en-tête était figé à 10 lignes en dur.

**Correctif** — deux fonctions pures et testables : version repliée sur `"unknown"` si moins de
3 champs ; liste d'encodeurs scannée à partir du séparateur `------`, avec repli sur l'ensemble
des lignes bien formées s'il est absent.

**Preuve** — test différentiel exécutant l'ancien et le nouvel algorithme sur la sortie réelle
de FFmpeg 8.0.1 : **13 encodeurs identiques dans le même ordre**, version identique. Puis
tests unitaires permanents sur les entrées malformées (chaîne vide, `" V"` seul, CRLF, absence
de séparateur).

**Leçon** — [[L-17]] : quand on remplace un parsing, prouver l'équivalence sur une entrée
réelle avant de raisonner sur les cas limites.

---

#### B-04 — `-threads` configurait le décodeur, pas l'encodeur

| | |
| --- | --- |
| **Fichiers** | `common/common.go` |
| **Vérification** | test ✅ · **mesure avant/après** ✅ |

**Correctif** — `-threads` déplacé après `-c:v <encoder>`.

**Mesure honnête** — aucun écart significatif sur le clip de test (785/782 ms avant,
767/778/774 ms après) : 20 s en 640×480 est trop court et trop petit pour être limité par les
threads. **C'est une correction de justesse, pas de performance** — le code configure
désormais ce qu'il annonce configurer. Un gain reste plausible sur des sources 4K longues,
non mesuré ici.

**Leçon** — [[L-03]], et : rapporter une mesure nulle telle quelle plutôt que de supposer un
gain.

---

#### B-05, B-06, B-08 — Bugs GUI et flux

| Constat | Correctif | Fichier |
| --- | --- | --- |
| **B-05** | Entrées vides filtrées dans la liste d'encodeurs — `strings.Split("", ",")` retourne `[""]`, d'où une option fantôme `" encoder"` quand FFmpeg est absent | `gui_main.go` |
| **B-06** | Chemin d'annulation unique et idempotent, partagé par le bouton *Cancel* et l'intercepteur de fermeture ; un seul `close(cancelEncoding)` subsiste, sous garde. Élimine la `panic: close of closed channel` | `gui_main.go` |
| **B-08** | Erreurs de `Close()` des fichiers PGM remontées via un retour nommé — un fichier tronqué au flush produisait une erreur FFmpeg incompréhensible bien plus tard | `common/common.go` |

**Leçon (B-06)** — [[L-18]] : un canal ne doit être fermé qu'à un seul endroit ; si deux
chemins peuvent annuler, ils appellent la même fonction idempotente.

---

#### S-01, S-03, S-04 — Validation d'entrées

| Constat | Correctif |
| --- | --- |
| **S-01** | `hasTraversalComponent` teste les **composants** du chemin, plus la sous-chaîne. `vacances..2024.mp4` et `GoPro..raw/` étaient rejetés à tort ; `../../etc/passwd` l'est toujours |
| **S-03** | `os.CreateTemp` remplace le fichier sonde à nom fixe `.superview_write_test` — plus de collision entre exécutions concurrentes ni de résidu prévisible |
| **S-04** | Répertoires système Windows lus depuis `%SystemRoot%` / `%ProgramFiles%`, littéraux `C:\...` conservés en repli ; comparaison insensible à la casse sous Windows |

Tests ajoutés pour chacun, dont la non-régression du rejet de traversée réelle.

**Leçon** — [[L-19]] : une validation de sécurité qui rejette des entrées légitimes se paie en
utilisabilité sans rien acheter. Valider la structure (composants), pas la forme textuelle.

---

#### X-01, X-04 — La CI voit enfin le paquet GUI

| | |
| --- | --- |
| **Fichiers** | `.github/workflows/lint.yml`, `test.yml`, `release.yml`, `.golangci.yml` |

**Correctif** — portée passée de `./common` à `./...` pour `golangci-lint`, `go vet`,
`staticcheck`, `govulncheck` et `go test`, avec installation de `libgl1-mesa-dev xorg-dev`
(et `ffmpeg`) sur les jobs concernés. `.golangci.yml` migré en schéma v2, golangci-lint
épinglé à **v2.13.2** via le chemin de module `/v2`.

**staticcheck cadré volontairement** sur `SA*` + `S1*`, équivalent fidèle de l'ancienne paire
`staticcheck` + `gosimple`. La famille `QF*` est exclue : ce sont des suggestions de
refactorisation d'éditeur, et l'une d'elles réécrirait les `math.Pow` de `GeneratePGM`, dont la
forme reflète l'algorithme de référence ([[L-09]]).

**Ce que l'extension a immédiatement révélé** — 4 défauts jamais vus par la CI :
`updateHardwareStatus` assigné sans effet, `dialog.ProgressDialog` déprécié **et toujours nil**
(champ mort supprimé), `theme.DarkTheme()` déprécié (remplacé par un thème personnalisé qui
force la variante sombre — même rendu, testé), et `profileBitrate` initialisé puis écrasé dans
chaque branche.

**Leçon** — [[L-02]], [[L-11]], [[L-13]].

---

#### O-02 — Les diagnostics deviennent exploitables

| | |
| --- | --- |
| **Fichiers** | `common/observability.go` (`OpenLogFile`, `ParseLogLevel`), `gui_main.go` |
| **Vérification** | **lancement réel de la GUI, journal inspecté** ✅ |

**Symptôme** — le logger écrivait dans `io.Discard`. Les 650 lignes de `observability.go` +
`metrics.go` alimentaient un puits sans fond : aucun diagnostic récupérable après un échec.

**Correctif** — journal dans `os.UserCacheDir()/superview/superview.log`, plafonné à 5 MiB
(remise à zéro au-delà), niveau piloté par `log_level` une fois la configuration chargée.
Repli silencieux sur `io.Discard` si le fichier ne peut pas s'ouvrir : la journalisation ne
doit jamais empêcher le démarrage.

**Preuve** — GUI lancée, contenu de `~/.cache/superview/superview.log` vérifié.

---

#### O-04 — Plus de cache sur les échecs de résolution

**Symptôme** — `resolveToolBinary` mémorisait l'échec pour toute la durée du processus. La GUI
affiche justement un lien d'installation quand FFmpeg manque ; l'utilisateur installait puis
devait redémarrer sans qu'on le lui dise.

**Correctif** — les échecs ne sont plus mis en cache (les succès le restent).
`ResetToolResolutionCache()` exposée pour un futur bouton « Réessayer ». Deux tests couvrent
les deux comportements.

---

#### C-02, C-07, C-08 — Nettoyage

- **C-02** : `EncodingOptions` supprimée (aucune référence dans tout le dépôt, tests compris).
- **C-07** : messages d'erreur conformes aux conventions Go — `"Error running ffmpeg, output is:\n%s"`
  devient `"ffmpeg failed: %w"`, avec `%w` au lieu de `%s` pour préserver la chaîne d'erreurs.
- **C-08** : constante `exitCodeUnavailable` à la place des 9 occurrences de `-1`.

---

#### T-01, T-02, T-03 — Filet de sécurité

- **T-01** — premiers tests du paquet `main` : `ensureMP4Extension` et `isMissingFFmpegError`
  extraits pour être testables, plus `containsString`, `formatResultsPanel` (dont la division
  par zéro), le thème sombre forcé et les accesseurs de `GUIHandler`. **9 tests** (dont la garde de mise en page de la barre d'outils).
- **T-03** — deux tests d'intégration autonomes : ils génèrent leur propre clip 4:3 via
  `ffmpeg -f lavfi -i testsrc`, exécutent tout le pipeline et **vérifient que la sortie est bien
  en 16:9** (640×480 → 852×480), que la progression remonte et que les métriques sont
  enregistrées ; le second vérifie que l'annulation interrompt réellement FFmpeg.
  Ils s'ignorent proprement si FFmpeg est absent.
- **T-02** — couverture réelle **54,6 %**, seuil CI relevé de **30 % → 50 %** et mesuré sur
  `./...` au lieu de `./common`.

---

#### X-02, X-03, X-05 à X-08 — Outillage et documentation

| Constat | Correctif |
| --- | --- |
| **X-02** | `Makefile` compile le paquet (`.`) au lieu du fichier inexistant `superview-gui.go` |
| **X-03** | `.github/copilot-instructions.md` corrigé : `gui_main.go`, Go 1.26, bitrate max 200 M, règle `fyne.Do` ajoutée |
| **X-05** | `softprops/action-gh-release` épinglée par SHA (`efb3536…`, v3.0.3) — c'était la seule action non épinglée, et celle qui détient `contents: write` |
| **X-06** | `coverage.out` retiré du suivi git ; `coverage.out` et `coverage.html` ajoutés au `.gitignore` |
| **X-07** | Lignes `echo >> /tmp/release.md` mortes retirées du job `notify` (elles écrivaient sur une autre machine) |
| **X-08** | `build.sh` marqué obsolète et refuse de s'exécuter sans `SUPERVIEW_ALLOW_LEGACY_BUILD=1` |

Le `README.md` a également été aligné : plus aucune référence à `superview-gui.go` dans le
dépôt, et la procédure de résolution du fichier de configuration y est documentée.

---

#### B-03 — Constat invalidé, aucune correction nécessaire

Voir [ANALYSE_PROJET.md](ANALYSE_PROJET.md) B-03 et [[L-10]]. Le remplacement par
`file.URI().Path()` a été conservé comme simplification, pas comme correctif.

---

## 4. File d'attente

Ordre issu de [ANALYSE_PROJET.md § 4](ANALYSE_PROJET.md). Cocher au fur et à mesure et
renseigner la date + le lien vers l'entrée § 3.

### Palier 0 — débloquer la vérification

- [x] **L-01** Installer Go 1.26+, `libgl1-mesa-dev`, `xorg-dev`, `ffmpeg`, `zenity`
      → *pré-requis de tout le reste*

### Palier 1 — outillage et documentation (sans risque)

- [x] **X-02** `Makefile` : remplacer `superview-gui.go` par `.` dans `build-gui`,
      `build-gui-windows`
- [x] **X-06** Retirer `coverage.out` du suivi git ; ajouter `coverage.out` et `coverage.html`
      au `.gitignore`
- [x] **X-03** Mettre à jour `.github/copilot-instructions.md` : `gui_main.go`, Go 1.26,
      bitrate max 200 M
- [x] **X-07** Supprimer les `echo >> /tmp/release.md` morts du job `notify`
- [x] **X-08** Supprimer `build.sh` ou le marquer obsolète en tête de fichier
- [x] **X-05** Épingler `softprops/action-gh-release` par SHA

### Palier 2 — correction bloquante

- [x] **B-01** Sortir de la boucle de lecture sur toute erreur non nulle ; `errors.Is(err, io.EOF)`

### Palier 3 — configuration et performance (à traiter ensemble)

- [x] **B-02** Résoudre `superview.yaml` depuis le répertoire de l'exécutable puis
      `os.UserConfigDir()`, avec repli sur le CWD
- [x] **O-01** Réexaminer `-re` en mode `safe` — brider l'encodage au temps réel n'a pas de
      sens pour une sortie fichier

### Palier 4 — filet CI

- [x] **X-01** Étendre lint/vet/staticcheck/govulncheck à `./...` + installer les dépendances
      GUI sur ces jobs
- [x] **X-04** Vérifier l'état réel du job `lint` sur GitHub Actions, puis épingler ou migrer
      `.golangci.yml` en v2

### Palier 5 — bugs GUI

- [x] **B-03** Utiliser `file.URI().Path()` au lieu de `ReplaceAll("file://", "")` (×2)
- [x] **B-05** Filtrer les entrées vides de la liste d'encodeurs (`splitCSV`)
- [x] **B-06** Rendre l'annulation idempotente (canal remis à `nil`, ou `context`)
- [x] **B-07** Protéger les accès par index sur la sortie de FFmpeg
- [x] **B-08** Vérifier les erreurs de `Close()` sur les fichiers PGM

### Palier 6 — validation d'entrées (arbitrage produit requis)

- [x] **S-01** Contrôler les composants de chemin plutôt que la sous-chaîne `..`
- [x] **S-02** ⚠️ *décision à soumettre à l'utilisateur* : résoudre les liens symboliques via
      `EvalSymlinks` plutôt que les rejeter
- [x] **S-03** Remplacer le test d'écriture par `os.CreateTemp`
- [x] **S-04** Utiliser `%SystemRoot%` / `%ProgramFiles%` au lieu de `C:\...` en dur

### Palier 7 — dette et code mort (arbitrage de périmètre requis)

- [x] **C-01** ⚠️ *décision* : brancher `health.go` dans la GUI (bouton « Diagnostic ») ou le
      supprimer
- [x] **C-02** Supprimer `EncodingOptions`
- [x] **C-03** Appliquer `min_video_*` / `quality_preset`, ou les retirer de `superview.yaml`
      et du README
- [x] **C-04** ⚠️ *décision* : exposer « squeeze » dans la GUI ou documenter son absence
- [x] **C-05** Passer la configuration en paramètre plutôt que par le global mutable
- [x] **C-06** Élaguer l'API exportée sans appelant
- [x] **C-07** Messages d'erreur en minuscule initiale (+ activer `stylecheck`)
- [x] **C-08** Constante nommée à la place du code de sortie `-1`
- [x] **B-04** Déplacer `-threads` après `-c:v` — **mesurer le débit avant/après**
- [x] **O-02** Journaliser dans un fichier sous `os.UserCacheDir()` au lieu de `io.Discard`
- [x] **O-03** Construire la ligne `y.pgm` une seule fois ; passer par `bufio.Writer`
- [x] **O-04** Ne pas mettre en cache les échecs de résolution de `ffmpeg`/`ffprobe`

### Palier 8 — tests

- [x] **T-01** Tests du paquet `main` (`test.NewApp()`, helpers purs)
- [x] **T-03** Test d'intégration FFmpeg de bout en bout en CI Linux
- [x] **T-02** Recalculer la couverture réelle, puis relever le seuil au-dessus de 30 %

---

## 5. Journal des révisions de ce document

| Date | Modification |
| --- | --- |
| 2026-09-04 | Création. Gabarit, 9 leçons permanentes issues de l'analyse initiale, file d'attente de 30 constats en 9 paliers. Aucune correction de code encore appliquée. |
| 2026-09-04 | Passe d'application par priorité : 28 constats traités et vérifiés en exécution, 1 invalidé (B-03), 5 en attente d'arbitrage, 1 reporté (C-05). Leçons L-10 à L-19 ajoutées. |
| 2026-09-04 | Arbitrages utilisateur appliqués (S-02, C-01, C-03, C-04). Leçons L-20, L-21. Restent C-05 (reporté) et C-06 (non tranché). |
| 2026-09-04 | 2ᵉ passe : C-05 et C-06 traités, plus O-05 et O-06 découverts en chemin. Leçons L-22 à L-24. **File d'attente vide.** |

# Superview — Journal des corrections et leçons

> **Go et Fyne.** Les noms de ce journal (`common/`, `internal/common/`, `cmd/superview/`, `gui_*.go`, `appState`, `go test`, `-race`…) décrivent le code Go retiré (DEC-49) : ils ne sont pas réécrits (DEC-55). Pour relire un fichier cité : `git show v0.2.9:<chemin>`, avec `common/` = `internal/common/` et `gui_*.go` = `cmd/superview/` (`common.go` y est découpé en huit fichiers, DEC-36) ; les numéros de ligne cités sont ceux du commit analysé, pas de v0.2.9.
>
> **Ce fichier se met à jour après CHAQUE correction appliquée au code.**
> Procédure : (1) ajouter une entrée dans [CORRECTIONS.md](CORRECTIONS.md) (ex-§ 3) avec le gabarit ci-dessous, (2) si la
> correction révèle une règle réutilisable, l'ajouter en § 2.
>
> Le § 2 est la partie à lire avant de corriger quoi que ce soit : 110 règles tirées
> de défauts réels de ce dépôt. [CORRECTIONS.md](CORRECTIONS.md) est l'historique, à consulter pour savoir si
> une correction a déjà été tentée. Les **décisions** — ce qui ressemble à un défaut
> et n'en est pas un — sont dans [DECISIONS.md](DECISIONS.md).
>
> La file d'attente qui figurait ici a disparu avec son dernier item le 2026-10-14 ;
> la 13ᵉ passe en a rouvert une de 25 constats (E-01 à E-25). L'état d'avancement vit
> dans [ANALYSE.md § 4](ANALYSE.md), seul endroit qui le tienne. **Ces deux nombres
> périment à chaque leçon et à chaque passe** — ils étaient restés à 80 et 67
> jusqu'au 2026-09-11 (M-04).

Les identifiants `Q-xx` renvoient aux **questions ouvertes** de
[ANALYSE.md § 5](ANALYSE.md) : ce ne sont pas des constats, mais des points dont
on ignore encore s'ils constituent un défaut.

Les identifiants `B-xx`, `S-xx`, `C-xx`, `X-xx`, `O-xx`, `T-xx` renvoient à
[ANALYSE.md § 3](ANALYSE.md) (1ʳᵉ et 2ᵉ passes), `N-xx` à
[§ 3bis](ANALYSE.md) (3ᵉ passe, empirique), `P-xx` à [§ 3ter](ANALYSE.md)
(4ᵉ passe), `R-xx` à [§ 3quater](ANALYSE.md) (5ᵉ passe), `D-xx` et `V-xx` à
[§ 3quinquies](ANALYSE.md) (6ᵉ passe), `U-xx` aux [§ 3sexies à § 3nonies](ANALYSE.md)
(7ᵉ à 10ᵉ passes, toutes parties d'un signalement utilisateur), `M-xx` aux
[§ 3decies et § 3undecies](ANALYSE.md) (11ᵉ et 12ᵉ passes, entretien) et `E-xx` au
[§ 3duodecies](ANALYSE.md) (13ᵉ passe, état des lieux). `DEC-xx` renvoie au registre
[DECISIONS.md](DECISIONS.md).

---

## 1. Gabarit d'entrée

```markdown
### [AAAA-MM-JJ] ID-xx — Titre court

| | |
| --- | --- |
| **Constat** | ID-xx (lien vers ANALYSE.md) |
| **Fichiers** | `chemin:lignes` |
| **PR** | #NN |
| **Vérification** | `scripts/check.sh` ✅/❌ · contre-preuve (le test rougit sans le correctif) ✅/❌ · test manuel ✅/❌/n.a. |

**Symptôme** — ce qui n'allait pas, observable.

**Cause racine** — pourquoi, pas seulement où.

**Correctif** — ce qui a été changé, et ce qui a été délibérément laissé.

**Leçon** — la règle généralisable. Rien à généraliser → écrire « aucune ».
```

> **Le numéro de PR, pas le sha de fusion.** L'entrée s'écrit avant la fusion, donc le sha
> n'existe pas encore : le champ portait « non commité » et le restait — dix entrées sur treize
> l'affichaient encore. Le numéro de PR est connu dès l'ouverture, ne périme pas, et le sha se
> retrouve d'un `git log --grep '(#NN)'`. Voir L-79.
>
> Une entrée groupée, qui porte plusieurs sous-corrections avec leurs propres tableaux, place la
> ligne `**PR** — #NN` sous son titre.



---

## 2. Leçons permanentes

Règles issues de l'analyse du dépôt, à appliquer à toute modification. Chacune est datée ;
elles s'enrichissent au fil des corrections.

### L-01 — Aucune modification n'est validée sans toolchain — 2026-09-04
Ne jamais écrire « corrigé » quand on veut dire « écrit mais non compilé ». Toute livraison
non vérifiée doit être annoncée explicitement comme telle à l'utilisateur.
*Mise à jour 2026-09-04* : Go 1.26.8 est désormais installé dans `~/.local/go` et y survit aux
redémarrages. Le **sysroot GUI vit dans `/tmp` et n'y survit pas** : après un redémarrage,
`./common` se vérifie normalement mais le paquet `main` ne compile plus tant que le sysroot
n'est pas reconstruit. Procédure : [ENVIRONNEMENT.md](ENVIRONNEMENT.md).

### L-02 — La CI de qualité ne couvre pas le paquet `main` — 2026-09-04
*Résolu par X-01 : `lint.yml` et `test.yml` ciblent maintenant `./...`.* La règle de travail
reste : toujours lancer `go vet ./...` (et non `./common`) en local après avoir touché la GUI.
→ **Mais la couverture n'est pas la portée** : les 14 tests du paquet `main` ne touchent que
des fonctions pures, et l'état de `main()` reste hors d'atteinte. Voir L-35 et P-10.

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
Procédure complète : [ENVIRONNEMENT.md](ENVIRONNEMENT.md).

### L-14 — `-re` n'a pas sa place dans une transcodification vers fichier — 2026-09-04
Ce drapeau FFmpeg bride la lecture de l'entrée à sa cadence native. Utile pour alimenter un
flux temps réel, il ne fait que plafonner un encodage fichier à la durée du clip. Il était
actif dans le mode nommé « safe », ce qui suggérait à tort une garantie de robustesse.
Constat O-01.

---

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

### L-20 — Dans une fenêtre à taille fixe, ajouter un widget est un changement de mise en page — 2026-09-04
`window.SetFixedSize(true)` n'adapte rien : un contenu trop large est rogné sans erreur ni
avertissement. Le 6e bouton de la barre d'outils portait celle-ci à ~1220 px pour 900 px
disponibles. Après tout ajout dans une rangée, recalculer la largeur minimale — ou mieux, la
figer dans un test (`TestToolbarFitsWindow`). Constat C-01.
*Mise à jour 2026-09-06* : ce test-là était vert pendant que trois boutons sur six affichaient
un libellé rogné — il mesurait la cellule imposée, pas le bouton. La règle tient, le garde-fou
qu'elle nommait ne tenait pas : voir L-69 et le constat U-01.

### L-21 — Un test qui contredit une décision produit se réécrit, ne se contourne pas — 2026-09-04
`TestSymlinkRejection` encodait l'ancienne politique de rejet des liens symboliques. Après
l'arbitrage inverse, la bonne réponse était de le remplacer par des tests décrivant la nouvelle
politique **et ses limites** (lien cassé, lien vers un répertoire, fichier non régulier), pas de
l'assouplir ni de le supprimer. Constat S-02.

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

### L-25 — Mesurer sur le mauvais échantillon mène à la conclusion inverse — 2026-09-04
Mesurant la fidélité du débit sur une mire `testsrc` (trivialement compressible), j'ai obtenu
13 % / 5 % / 4 % de la consigne et conclu que NVENC dégradait silencieusement la qualité. Refaite
sur du bruit incompressible, la même mesure donne 156 % / 112 % / 116 % : NVENC suit
correctement, et c'est `libx264` qui dépasse. **La première conclusion était l'inverse exact de
la vraie.** En VBR, `-b:v` est une cible moyenne : mesurer la fidélité d'un débit exige un
contenu qui sature réellement l'encodeur. Constat N-06.

### L-26 — Demander le domaine métier avant de calibrer une sévérité — 2026-09-04
J'ai classé le rejet des MKV en 🔴 sans savoir que l'application ne reçoit en pratique que des
fichiers GoPro `.mp4`. L'utilisateur l'a signalé et la sévérité est tombée à 🟠. En vérifiant
plus largement, le constat s'est révélé **plus étendu** que ma formulation initiale — 5 des 10
formats proposés échouent, pas seulement le MKV. La correction de portée a donc joué dans les
deux sens : sévérité revue à la baisse, périmètre revu à la hausse. Constat N-01.

### L-27 — Vérifier une hypothèse avant d'écrire le constat — 2026-09-04
Cette passe visait quatre axes présumés problématiques. **Quatre hypothèses se sont révélées
fausses** : la mémoire est constante quelle que soit la résolution (~2 Mo), le chemin matériel
fonctionne réellement (validé sur RTX 4070), les formats exotiques passent tous, et l'écrasement
entrée=sortie ne détruit rien (FFmpeg refuse). Les consigner explicitement comme non-constats a
autant de valeur que les vrais constats : cela évite qu'un futur lecteur refasse le travail.

### L-28 — Ne pas se fier au code de sortie d'un script shell ad hoc — 2026-09-04
Mon test VAAPI affichait « OK » alors que l'initialisation échouait : le `&&` en fin de pipeline
capturait le code de `head`, pas celui de `ffmpeg`. Même famille d'erreur que L-11 : la sortie
de l'outil, pas son code de retour supposé, fait foi.

### L-29 — Une refactorisation de signature doit balayer la documentation — 2026-09-04
La refactorisation C-05 a changé les signatures de `CheckFfmpeg` et `PerformEncoding`. J'ai mis à
jour le code, les tests et `CONTRATS.md`, mais pas le bloc d'exemple du README, resté avec les
anciennes signatures pendant deux PR. Aucun outil ne le détecte : les extraits de code en
Markdown ne sont pas compilés.
→ Après tout changement de signature exportée : `grep -rn "NomFonction" --include='*.md' .`
Constat relevé pendant la correction de N-01.

### L-30 — Une transition d'état recopiée dans deux branches finit par diverger — 2026-09-04
`gui_main.go` réactive ses widgets un par un, à la main, dans la branche de succès **et** dans
la branche d'échec. La branche de succès a oublié `squeezeCheck.Enable()` : la fonctionnalité
squeeze devient inatteignable après le premier encodage réussi (P-01). Deux listes recopiées
divergent toujours.
→ Une transition d'état s'écrit **une fois** : une liste de widgets parcourue, ou une méthode
`setBusy(bool)` appelée depuis les deux branches. Jamais deux suites d'appels parallèles.

### L-31 — Une annulation doit être une erreur typée, jamais une chaîne — 2026-09-04
`EncodeVideo` signale l'annulation par `errors.New("encoding interrupted by user")`. Pour la
cascade de repli, c'est un échec comme un autre : elle relance FFmpeg jusqu'à trois fois après
que l'utilisateur a demandé l'arrêt (P-03).
→ Toute condition qui doit **interrompre** une logique de retentative se transporte par un
type ou une sentinelle testable avec `errors.Is`. Une chaîne d'erreur ne se teste pas, donc
elle ne s'arrête pas.

### L-32 — Une unité mal nommée dans un commentaire est un bug latent — 2026-09-04
Le pipeline manipule des bits/s de bout en bout et il est correct. Mais « bytes/second » est
écrit dans `config.go`, `metrics.go`, `superview.yaml` et jusque dans les messages d'erreur
montrés à l'utilisateur (P-05). Un utilisateur qui suit la documentation se trompe d'un
facteur 8.
→ L'unité d'une grandeur fait partie de son contrat. Quand la source de vérité est un outil
externe (ici `ffprobe bit_rate` et `ffmpeg -b:v`, tous deux en bits/s), c'est cet outil qui
fixe le nom, pas l'intuition de celui qui commente.

### L-33 — Une métrique dérivée d'une constante inventée nuit plus qu'une métrique absente — 2026-09-04
`EncodingSpeed` fait `InputDuration * 30 // Assuming ~30fps average` (P-06). Sur du 120 fps
GoPro, la vitesse rapportée est fausse d'un facteur 4 — et elle est rapportée avec la même
assurance que les autres. À l'inverse `EstimatedRemaining`, lui, est exact et n'est **jamais
affiché** (P-07).
→ Ne pas publier une métrique dont une entrée est devinée : soit on lit la vraie valeur
(`r_frame_rate` est à un champ ffprobe de distance), soit on ne publie rien.

### L-34 — Un changement de format de fichier se valide sur la sortie décodée, pas sur le fichier — 2026-09-04
Le passage des cartes de remappage du PGM P2 ASCII au P5 binaire (P-08) a été validé en
comparant le **SHA-256 des trames décodées** des deux encodages, pas la taille ou l'en-tête
des cartes. Les deux empreintes sont identiques : le changement est prouvé neutre.
→ Corollaire de L-12, appliqué à un format d'échange : la seule preuve d'équivalence est
l'empreinte de ce que consomme l'étage suivant. Attention aussi au boutisme — le PGM binaire
est gros-boutiste par spécification, et un écrivain petit-boutiste produirait des cartes
silencieusement fausses plutôt qu'une erreur.

### L-35 — Un test qui ne peut pas atteindre l'état ne prouve rien sur l'état — 2026-09-04
T-01 a été coché avec 14 tests sur le paquet `main`. Tous portent sur des fonctions pures.
Les deux bugs GUI de la 4ᵉ passe (P-01, P-02) vivent dans les ~600 lignes de `main()` où
l'état est en variables capturées par closure — hors de portée de tout test.
→ « Le paquet est testé » n'est pas la même affirmation que « l'état du paquet est testé ».
Avant de clore un constat de couverture, vérifier que les tests atteignent la zone où les
défauts de ce paquet peuvent réellement se loger.

### L-36 — Ne pas figer une empreinte qui dépend d'un outil externe — 2026-09-04
Pour certifier le passage des cartes PGM en binaire, la tentation était de figer le SHA-256 des
trames que FFmpeg produit. Cette empreinte dépend du build de `libx264` : elle aurait été verte
en local et rouge sur le runner Ubuntu, pour une bonne raison qui n'aurait rien eu à voir avec
le code.
→ `TestGeneratePGM_RemapOutputIsStable` est **auto-référentiel** : il retranscrit les cartes
binaires en ASCII dans le test, lance deux fois le FFmpeg présent, et compare les deux sorties
entre elles. Aucune constante externe, donc aucune fragilité de version. Quand une valeur de
référence dépend d'un outil qu'on ne contrôle pas, comparer deux exécutions du même outil plutôt
que d'inscrire son résultat.

### L-37 — Toujours exécuter la contre-épreuve : un test peut passer à vide — 2026-09-04
`TestGeneratePGM_MapsAreBigEndianP5` **passait alors que le boutisme était inversé**. En cause :
la carte Y n'appelait pas `putMapSample` et réencodait l'octet à la main, donc l'invariant était
défini à deux endroits et le test n'en contrôlait qu'un.
→ Le bon réflexe a été de corriger **le code** (router la carte Y par le même helper), pas
d'élargir le test. Un invariant exprimé à deux endroits n'est pas testable, il est seulement
vérifié à moitié. Et un test de non-régression ne vaut que si on l'a vu échouer : casser
délibérément le code et vérifier que le test rougit fait partie de son écriture, pas de sa
relecture. Généralise L-23.

### L-38 — Déduire une propriété d'un nom exige de lister les contre-exemples d'abord — 2026-09-04
Détecter le 10 bits en cherchant « 10 » dans le nom du format lisait `yuv410p`, du 8 bits, comme
du 10 bits. La deuxième tentative — retirer le suffixe d'endianness puis regarder les chiffres
finaux — lisait `nv12` comme du 12 bits. C'est le test qui l'a signalé, pas la relecture.
→ La règle retenue croise deux conditions (suffixe `le`/`be` **et** profondeur dans 9..16), et
le tableau de cas inclut explicitement les pièges : `nv12`, `nv16`, `yuv410p`, `rgb565le`. Avant
d'écrire une heuristique sur des noms, écrire la liste de ce qui doit la faire échouer.

### L-39 — `runtime.NumCPU()` n'est pas une valeur valide pour un outil externe — 2026-09-04
`-threads runtime.NumCPU()` faisait échouer **tout** encodage `libx265` sur les machines de plus
de 16 cœurs (P-11), parce que FFmpeg fait correspondre cette option à `--frame-threads` de x265,
qui a un plafond. Invisible en CI : les runners GitHub ont 4 cœurs.
→ Une valeur dérivée de la machine passée à un outil tiers doit être confrontée aux limites de
cet outil, pas seulement à sa propre plausibilité. Et un défaut qui ne se manifeste que
au-dessus d'un seuil matériel ne sera jamais vu par une CI modeste : il faut le chercher.

### L-40 — Un test d'annulation doit interrompre un travail réellement commencé — 2026-09-04
Deux tests de cette session passaient pour la mauvaise raison. Un canal d'annulation fermé
**avant** l'appel tue ffmpeg dans les microsecondes qui suivent `Start` : le processus n'a pas
exécuté sa première ligne, n'a pas créé son fichier de sortie, et les assertions « aucun fichier
partiel » ou « une seule invocation » sont vertes avec **ou sans** le correctif.
→ Déclencher l'annulation depuis le **rappel de progression** : c'est le seul point qui prouve
que ffmpeg produit vraiment quelque chose. Et vérifier la contre-épreuve, systématiquement — les
deux tests n'ont été démasqués que par elle (L-37).

### L-41 — Observer depuis l'extérieur du processus qu'on va tuer — 2026-09-04
Compter les lancements de ffmpeg en faisant écrire le processus lui-même dans un fichier ne
fonctionne pas quand le test consiste précisément à le tuer aussitôt. Le compteur est le reflet
d'une course, pas du comportement.
→ Le point d'observation doit être **en amont du processus**. Ici `commandStdoutPipe`, la
variable d'injection déjà présente, est appelée une fois par lancement : la mesure devient
déterministe. Chercher le point d'injection existant avant d'inventer un mécanisme.

### L-42 — Calibrer un paramètre par la mesure, sur le contenu qui le sollicite — 2026-09-04
Pour borner le débit (N-06), le réflexe est `maxrate = 1,5 × consigne`, la recommandation
habituelle. Mesuré sur du bruit incompressible à 8 Mbps : sans plafond **+83 %**, avec
`1,5×` **+29 %**, avec **`1×` +24 %**. La valeur « standard » était la moins bonne des deux.
→ Trois commandes ffmpeg ont tranché ce qu'une discussion n'aurait pas tranché. Et le contenu
compte autant que le paramètre : sur une mire, les trois configurations auraient donné le même
résultat et n'auraient rien appris (L-25).

### L-43 — Une même géométrie calculée à deux endroits finit par diverger — 2026-09-04
La vérification d'espace de N-08 doit réserver exactement ce que `GeneratePGM` va écrire. Recopier
le calcul de `outX`/`outY` dans le vérificateur aurait créé deux sources de vérité : le jour où
la formule change, la vérification réserve pour une carte qui n'est plus celle qu'on écrit, et
elle bloque des encodages viables ou laisse passer ceux qui ne le sont pas.
→ `remapOutputSize` est partagé, et `TestRemapMapBytes_MatchesWhatGeneratePGMWrites` compare
l'estimation aux octets réellement produits. Même famille que L-30 et L-37 : un invariant
n'existe qu'à un seul endroit.

### L-44 — Rendre un bug inexprimable vaut mieux que le corriger — 2026-09-04
P-02 était une course : la goroutine d'encodage relisait `cancelEncoding` pendant que le fil UI
le mettait à `nil`. Le correctif immédiat — capturer le canal dans une locale — marchait, mais
rien n'empêchait le prochain appelant de refaire exactement la même erreur.
→ `beginEncoding()` **retourne** le canal. L'appelant ne peut plus le relire dans la structure
puisque la méthode le lui donne, et la signature est ce qui l'en dissuade. Quand un correctif
repose sur « penser à faire X », chercher la forme d'API où X est la seule chose possible.

### L-45 — Extraire l'état, pas les lignes — 2026-09-04
P-10 disait « `main()` fait 600 lignes ». Après refactorisation elle en fait 524 : le gain de
lignes est marginal, et ce n'était pas le sujet. Ce qui a changé, c'est que les **décisions** —
« peut-on démarrer ? », « que réactiver à la fin ? », « comment annuler proprement ? » — sont
sorties des closures pour devenir onze méthodes, toutes couvertes à 100 % et toutes vérifiées en
contre-épreuve.
→ Le reste de `main()` est de la construction de widgets et de la mise en page ; la découper
n'achèterait rien. Mesurer une refactorisation en lignes déplacées conduit à découper ce qui va
bien ; la mesurer en « surface désormais atteignable par un test » conduit au bon découpage.

### L-46 — Une mesure héritée se revérifie avant d'en tirer une décision — 2026-09-04
J'ai relayé à l'utilisateur les « ~5 % » de N-07 comme un fait établi. Ce chiffre venait d'une
passe précédente et je ne l'avais pas revérifié — alors que le RTX 4070 était disponible et que
la mesure prend cinq minutes. Refaite, elle donne **+9,9 %** : la mesure d'origine portait sur
une mire `testsrc` à 2 Mbps, qui se décode quasi gratuitement. L'arbitrage que j'en tirais
— « le gain ne justifie pas le risque » — s'inverse.
→ Une mesure consignée dans un document n'est pas plus vraie que l'échantillon sur lequel elle a
été faite, et un document ne transporte pas cet échantillon. Avant de **fonder une décision** sur
un chiffre hérité, le refaire, ou au minimum vérifier sur quoi il portait. Deuxième occurrence de
L-25 dans ce dépôt, et cette fois j'ai failli la propager jusqu'à une recommandation produit.

### L-47 — Une convention non écrite n'est pas une convention — 2026-09-04
Ce document marque le statut d'un constat de deux façons : le § 3 garde les sévérités d'origine
(c'est une analyse datée, le § 4bis fait foi), les § 3bis et 3ter barrent le titre et le passent
en ✅. Les deux se défendent, mais **aucune des deux n'était écrite**. Résultat : quatre titres
ont dérivé — N-03, N-04, N-05 et P-11 annonçaient encore 🟠/🔴 alors que le tableau
d'avancement, dix lignes plus bas, les donnait corrigés.

Le document a traversé deux PR relues dans cet état. Personne ne lit un fichier de 1 100 lignes
en entier : **on le parcourt par les titres.** Un titre qui contredit le tableau est plus nuisible
qu'un titre absent, parce qu'il a l'air de répondre à la question.
→ Quand un document porte deux conventions, l'écrire en tête coûte cinq lignes et supprime la
classe entière de dérives. Et vérifier la cohérence titres ↔ tableau fait partie de la relecture,
au même titre que faire tourner les tests. Même famille que L-30 et L-43 : une information
exprimée à deux endroits finit par diverger.

### L-48 — Une empreinte fige des octets ; elle ne dit rien des propriétés — 2026-09-04
`TestGeneratePGM_Golden` verrouillait la carte de remappage au bit près depuis trois passes. Elle
contenait pourtant une couture de 1 à 2,6 px au milieu de l'image (P-12), et le test ne pouvait
pas la voir : il certifiait que les octets ne changeaient pas, pas qu'ils étaient bons. Le défaut
a été trouvé en posant des questions que l'empreinte ne pose jamais — la courbe est-elle
monotone ? les bords tombent-ils sur les bords ? le centre reste-t-il le centre ?
→ Une empreinte est un test de **non-régression**, pas un test de **correction**. Elle a toute
sa valeur — elle a certifié le passage en PGM P5 — mais elle doit être accompagnée de tests de
propriétés, sinon elle fige aussi fidèlement les défauts que le reste.

Corollaire utile : quand une formule a une **forme fermée**, la calculer en arithmétique exacte
tranche ce qu'aucune mesure ne tranche. Ici les deux termes valent tous deux `7/32 × outX × inv`
au centre : l'intention de l'auteur était démontrable, et l'écart du code avec elle aussi. Sans
cela je serais resté sur « je ne peux pas savoir si c'est fidèle à la référence ».

Et une chose à ne pas confondre : le code était **fidèle à l'amont**, caractère pour caractère.
Fidèle ne veut pas dire correct. Vérifier la conformité à une référence ne dispense pas de
vérifier la référence.

### L-49 — Une contre-épreuve peut être fausse par contamination de l'environnement — 2026-09-04
En vérifiant que les deux tests dépendaient bien du faux ffmpeg, je l'ai renommé et relancé avec
`PATH=/usr/bin:/bin`. `TestEncodeVideo_InterruptedByUser` a continué de passer, et j'en ai conclu
à voix haute qu'il passait à vide. C'était faux : `/usr/bin/ffmpeg` existe sur cette machine, le
test retombait dessus. Avec un `PATH` réellement vide de ffmpeg il échoue en trois secondes sur
« timeout waiting for signal registration ». Le test était porteur ; c'est la contre-épreuve qui
ne l'était pas.
→ Une contre-épreuve n'est valide que si l'environnement rend le défaut **observable**. Quand
elle consiste à retirer une dépendance, vérifier que la dépendance a bien disparu — ici un
`which ffmpeg` avant de conclure. Même piège que L-37 d'un cran plus haut : là c'était le test
qui passait à vide, ici c'est la vérification du test. Rien ne dispense de se demander *pourquoi*
un résultat tombe comme il tombe, surtout quand il confirme ce qu'on soupçonnait.

### L-50 — Un `t.Skip` est une plateforme entière qui ne teste rien — 2026-09-04
`fakeHangingFFmpeg` sautait sur Windows avec un motif qui sonnait comme un détail de fixture :
« the stand-in ffmpeg relies on a POSIX shell ». Ce qu'il disait vraiment, c'est que le test
épinglant P-03 — l'annulation ne doit pas relancer la cascade de repli — ne s'exécutait que sur
Linux, sur un chemin (kill de processus) dont le comportement Windows est justement celui dont
on doute. Et la solution dormait à quinze lignes de là : un autre test du même fichier installait
le même stand-in en `.bat` et passait sur Windows depuis toujours.
→ Un saut se lit comme un constat, pas comme une note de bas de page : **quel comportement
n'est plus vérifié, et sur quelle plateforme ?** Avant d'accepter un `t.Skip`, chercher si le
fichier ne contient pas déjà le contournement. Deux sauts restaient ici ; l'un était de la dette,
l'autre (`/dev/zero`) est réel. Les distinguer demande de les lire un par un.

### L-51 — Un renommage « partout » se vérifie par `grep`, pas par relecture — 2026-09-04
P-05 devait remplacer « bytes/second » par « bits/second » dans tout le projet. Une occurrence
avait survécu — `slog.Int("bitrate_bytes_sec", bitrate)` — dans le journal de fin d'encodage,
c'est-à-dire dans le **seul** journal que le README demande de joindre aux rapports de bug. Un
facteur 8 annoncé à qui viendrait diagnostiquer.
→ Elle n'a pas été trouvée par relecture mais en regardant défiler la sortie d'un encodage lancé
pour autre chose. Un correctif dont l'énoncé contient « partout » se solde par un `grep` de
l'ancien terme avant de le déclarer fait ; ça prend cinq secondes et c'est la seule vérification
qui corresponde à ce qui est promis. Même famille que L-47 : ce qui n'est pas vérifié dérive.

### L-52 — Quand l'intention est mesurable, la mesurer bat toute relecture — 2026-09-04
Le facteur 1,6 avait traversé quatre passes d'analyse. Le lire une cinquième fois n'aurait rien
donné de plus : le code était clair, le commentaire cohérent avec lui-même, les tests verts —
ils figeaient 16 000 000 comme attendu, ce qui certifiait la constante sans jamais l'interroger.
Ce qui l'a tranché, c'est d'avoir construit la **référence que l'intention désigne** : le
commentaire promettait de « conserver la qualité perçue de la source », donc il fallait produire
cette qualité-là — réencoder la source sans remappage, au même débit, avec le même encodeur — et
regarder à quel multiplicateur la chaîne remappée la rejoint. Réponse : 1,19–1,30, quand le code
demandait 1,6.
→ Chercher un coude sur la courbe débit/qualité n'aurait rien donné : sur du contenu exigeant il
n'y en a pas, la qualité monte encore à 2,0. **Une courbe sans coude n'est pas une absence de
réponse, c'est le signe qu'on interroge la mauvaise référence.** La bonne référence est celle
qu'énonce l'intention, et il faut souvent la fabriquer exprès.

Corollaire de méthode : la conclusion n'a été publiable qu'après un **second encodeur**.
`libx265` et `hevc_nvenc` n'ont rien de commun dans leur contrôle de débit, et ils donnent 1,189
contre 1,190. Une mesure isolée aurait pu n'être qu'une propriété de NVENC ; deux qui convergent
au millième désignent la transformation. Reproduire avec un dispositif indépendant coûte quinze
minutes et change le statut de « mesuré » à « établi ».

### L-53 — Un script enfermé dans un YAML s'extrait pour être essayé — 2026-09-05
L'étape qui écrit les notes de release ne tourne que sur un tag poussé : l'essayer « pour de
vrai » coûterait un tag, et c'est exactement ce qui avait laissé le workflow cassé de juillet à
septembre. La sortie a été d'extraire le `run:` du YAML avec un parseur, puis de l'exécuter
localement sur trois tags réels du dépôt — un annoté, un léger, un inexistant.

Ça a payé immédiatement : le `::warning::` du cas dégradé était **à l'intérieur** du groupe
redirigé vers le fichier de notes, donc il serait parti dans le corps publié de la release au
lieu du journal d'exécution. Relire le YAML ne l'aurait pas montré — la ligne est correcte en
soi, c'est sa **place dans une redirection** qui ne l'est pas.
→ Un script dans un YAML de CI reste un script. Il s'extrait (`yaml.safe_load`, puis le champ
`run`), il s'exécute, et il se vérifie sur les cas dégradés autant que sur le cas nominal. Cela
vaut particulièrement quand le déclencheur réel est coûteux ou irréversible — un tag, un déploiement,
un envoi. Même famille que L-48 : ce qui n'est jamais exécuté n'est pas vérifié, quelle que soit
l'attention portée à sa lecture.

### L-54 — Une contre-épreuve qui ne change pas le résultat n'est pas une contre-épreuve — 2026-09-05
En vérifiant `buildIdentity`, j'ai muté trois choses ; deux ont fait rougir les tests, la
troisième non. Elle portait sur la troncature de la révision : le seuil est passé de
`len(revision) > 7` à `> 12`, mais la **longueur découpée** restait `revision[:7]`. Sur une
révision de 40 caractères le résultat est identique dans les deux cas, et sur une révision de 7
aucun des deux ne tronque. J'avais muté une condition sans muter le comportement, puis conclu que
le test ne couvrait pas la troncature. En mutant `revision[:7]` en `revision[:10]`, trois cas
rougissent.
→ Une contre-épreuve n'a de valeur que si la mutation produit **un comportement observablement
différent** sur au moins un cas du test. Avant de conclure « le test ne couvre pas ceci »,
vérifier que la version cassée fait bien autre chose que la version saine — sinon on n'a rien
mesuré. Suite directe de L-49 : là, l'environnement rendait la contre-épreuve muette ; ici, c'est
la mutation elle-même qui l'était.

### L-55 — Un paramètre dérivé du ref doit valoir pour tous les refs qui déclenchent le workflow — 2026-09-05
R-04 fait passer la version au paquet depuis `GITHUB_REF_NAME`, et la vérification a porté sur un
paquet construit à la main : correcte pour un tag `v0.2.2`, muette sur tout le reste. Or `Release`
se déclenche sur trois formes de ref — `v*`, `RC-*` et un `workflow_dispatch` qui part d'une
branche — et `fyne package` n'accepte que `x.y.z`. Le premier essai manuel est mort sur
`--app-version master`, dans les deux jobs de build, avant d'avoir compilé quoi que ce soit ; un
tag `RC-*` aurait échoué pareil, en cours de release cette fois.
→ Quand une valeur vient du ref, énumérer les refs que le bloc `on:` autorise et vérifier la
dérivation sur chacun, pas seulement sur celui qu'on a en tête. Ironie utile : le dispatch a été
ajouté précisément pour essayer ce workflow sans couper de release — il a fait son travail.

### L-56 — Une observation notée en passant doit être reliée à ses conséquences — 2026-09-05
Le message de #39 se termine par : « Noted on the way: fyne package rewrites FyneApp.toml -- it
reformats the file and adds Build. Harmless in a CI checkout, but the tool owns that file. » Le
fait était exact, la conclusion fausse. Réécrire un fichier suivi **salit l'arbre de travail**, et
`go build` y appose alors `vcs.modified=true`. Le binaire de v0.2.3 s'annonce donc
`0.2.3 (9c1a8c5, modified)` : le drapeau censé distinguer un binaire bricolé d'une release propre
s'allume sur toutes les releases. #39 a cassé, d'une ligne notée puis classée sans suite, la
fonction même qu'il ajoutait.
→ Quand une vérification met au jour un comportement inattendu d'un outil, ne pas le ranger sous
« sans conséquence » sans avoir cherché laquelle. Ici la question tenait en une ligne : qu'est-ce
qui, dans la construction, dépend de la propreté de l'arbre ? Constat R-06.

### L-57 — Un état transitoire ne s'observe pas après coup — 2026-09-05
Pour R-06, j'ai d'abord désigné une cause — la réécriture de `FyneApp.toml` — parce qu'une note de
#39 la mentionnait. Elle était fausse : l'outil ne complète que ce qui manque, et le fichier est
complet depuis #39 précisément. L'inspection de l'arbre **après** le packaging ne montrait rien non
plus, et pour cause : les fichiers en question sont créés puis effacés par l'outil lui-même.
Ce qui a tranché tient en quatre lignes — lancer le packaging en arrière-plan et échantillonner
`git status --porcelain` toutes les 0,2 s :

    ( go run .../fyne package ... ) &
    PKG=$!
    while kill -0 $PKG 2>/dev/null; do git status --porcelain >> seen.txt; sleep 0.2; done
    sort -u seen.txt

Réponse immédiate, et **différente selon la plateforme** — ce qu'aucun raisonnement n'aurait donné.
→ Quand un défaut dépend d'un état qui n'existe que pendant l'exécution, échantillonner pendant.
Le dispositif est trivial et se transporte tel quel dans un job de CI, ce qui a été fait pour
Windows, faute de pouvoir compiler pour cette cible en local.

### L-58 — Un garde-fou qui vérifie le résultat attrape ce que la correction de la cause a manqué — 2026-09-05
Le correctif de R-06 — ignorer `fyne_metadata_init.go` — était juste, et **incomplet** : il ne
valait que pour Linux, Windows créant deux fichiers de plus. Rien dans le raisonnement ne pouvait
le dire ; j'avais observé une plateforme et généralisé. Ce qui l'a dit, c'est le garde-fou ajouté
dans le même changement : il dépaquette le binaire sur le point d'être expédié et échoue s'il porte
`vcs.modified=true`. Il a laissé passer Linux et arrêté Windows, dans le même run.
→ Quand on corrige une cause, ajouter dans le même changement une vérification qui porte sur le
**résultat**, pas sur la cause. Elle survit à un diagnostic incomplet, et à la prochaine cause,
qu'on n'a pas encore rencontrée. Sans elle, la moitié de R-06 repartait en release.

### L-59 — Une leçon non appliquée jusqu'au bout ne protège de rien — 2026-09-06
**L-29** dit « une refactorisation de signature doit balayer la documentation » et prescrit
`grep -rn "NomFonction" --include='*.md' .`. Écrite après avoir laissé le bloc d'exemple du
README derrière, elle a servi à corriger **ce bloc-là**. Le § *API Documentation* vingt lignes
plus haut, qui portait les mêmes signatures périmées et trois fonctions supprimées, est resté
faux pendant cinq passes d'audit — sous une leçon qui décrivait exactement son cas.
→ Une leçon dont le remède est une commande se termine en **exécutant la commande**, pas en
corrigeant l'occurrence qui l'a inspirée. Et la première fois qu'on l'écrit, la lancer une fois
sur tout le dépôt : les autres occurrences sont déjà là.

### L-60 — Un `t.Skip` conditionné à l'environnement doit pouvoir devenir un échec — 2026-09-06
Seize gardes sautaient un test quand ffmpeg manquait. Ensemble, elles couvraient tout ce qui
vérifie une conversion réelle. Sur une machine sans ffmpeg, la suite passait au vert en n'ayant
rien encodé — et en CI, le seul indice que les tests avaient vraiment tourné était un écart de
couverture de huit points au-dessus d'un seuil de cinquante.
→ Un skip est un jugement sur l'environnement, et l'environnement de la CI est connu. Le rendre
conditionnel à une variable que la CI positionne (`SUPERVIEW_REQUIRE_FFMPEG=1`) garde le confort
en local et supprime le silence là où il coûte. Se vérifie dans les deux sens : retirer la
dépendance du `PATH`, la suite doit sauter sans la variable et échouer avec.

### L-61 — Séparer l'appel du processus et le parsing de sa sortie — 2026-09-06
`CheckVideo` lançait `ffprobe` et interprétait sa réponse dans la même fonction. Éprouver ses
cinq branches de rejet supposait donc de trouver un fichier vidéo provoquant chacune — ce que
personne n'avait fait. C'était la seule entrée que le programme ne produit pas lui-même, et la
moins testée.
→ Extraire le parsing en fonction pure prenant les octets. Le test devient hermétique, rapide, et
les cas tordus s'écrivent à la main au lieu de se chercher. Les sorties réelles s'enregistrent
une fois dans `testdata/` — capturées depuis l'outil, jamais rédigées de mémoire.

### L-62 — Un outil lancé à côté de sa configuration finit par la contredire — 2026-09-06
`lint.yml` avait un job `staticcheck` distinct, en plus de `golangci-lint` qui active déjà
staticcheck. Le job séparé tournait **sans** le filtre `SA*`/`S1*` de `.golangci.yml` : il
appliquait donc les familles `ST*` et `QF*` que la configuration désactive avec justification
écrite. La configuration disait une chose, la CI en imposait une autre.
→ Un analyseur a un seul point de réglage. S'il est déjà activé par l'agrégateur, ne pas le
relancer à côté : ce n'est pas un second avis, c'est une seconde source de vérité, et celle qui
gagne est celle que personne n'a écrite.

### L-63 — Une observation faite sur une plateforme se rejoue sur l'autre — 2026-09-06
L-58 disait déjà qu'un correctif observé sur une plateforme peut être incomplet sur l'autre.
R-06 l'a redémontré dans l'autre sens : `#43` avait ignoré `superview.exe` côté Windows, sans
ignorer son équivalent Linux, le binaire `superview` — pourtant produit par le même outil, au
même endroit, pour la même raison. Le mécanisme était compris ; la conséquence n'avait été tirée
que d'un côté.
→ Après avoir compris **pourquoi** un artefact apparaît, énumérer les plateformes et vérifier
chacune, plutôt que de corriger celle qu'on regardait. Ici : rejouer la boucle
`git status --porcelain` sur l'autre système d'exploitation.

### L-64 — Une instruction d'installation se vérifie en installant — 2026-09-06
La procédure Linux ajoutée au README donnait `./usr/local/bin/superview-gui`, déduite du chemin
`*/bin/*` que cherche `release.yml`. En produisant réellement une archive, le chemin est
`superview/usr/local/bin/superview` : un préfixe de plus, et pas le nom attendu. Une commande
fausse dans un README échoue chez l'utilisateur, à l'endroit exact où il découvre le projet.
→ Toute commande écrite dans une documentation d'installation s'exécute avant d'être publiée. La
déduire d'un fichier de CI produit une commande plausible, ce qui est pire qu'une commande
absente.


### L-65 — Un artefact qu'il faut tenir à jour avant chaque release finira périmé — 2026-09-06
`RELEASE_NOTES.md` devait être réécrit avant chaque publication. Assez oubliable pour que le
workflow ait dû se doter d'un contrôle vérifiant que sa première ligne nommait bien la version
demandée : le garde-fou est l'aveu que l'étape ne tient pas toute seule. Et entre deux releases
le fichier contenait les notes de la version **déjà sortie**, donc il ressemblait en permanence
à un résidu — au point qu'on se demande à quoi il sert, ce qui est exactement la question qui a
mené à sa suppression.
→ Avant d'ajouter un garde-fou qui protège une étape manuelle récurrente, se demander si
l'étape peut disparaître. Ici, la liste des PR fusionnées depuis le tag précédent dit ce qui a
changé sans que personne l'écrive. Un garde-fou qui n'a plus rien à garder est le meilleur des
garde-fous.

### L-66 — Un principe énoncé dans la doc se vérifie en l'exécutant — 2026-09-06
`RELEASING.md` affirmait « la version vit dans le tag, rien ne la lit dans l'arbre », et
`FyneApp.toml` portait `Version = "0.2.3"` sous un commentaire la qualifiant de simple repli.
Un `go build` nu annonçait `0.2.3` depuis un arbre qui n'était pas cette version : le repli
était le cas courant, et le principe était faux dans la seule situation où il servait.
→ Quand une documentation affirme une propriété du système, produire l'objet et l'interroger.
Ici : construire et lire la ligne de version. Le commentaire disait « repli », la mesure disait
« comportement par défaut ».


### L-67 — Une catégorisation qui n'attrape pas tout supprime le reste — 2026-09-06
`.github/release.yml` trie les notes générées par label. La règle de GitHub, écrite dans sa
documentation, est qu'une PR ne correspondant à **aucune** catégorie n'apparaît pas du tout.
Ce dépôt ne pose aucun label sur ses PR : une configuration en apparence raisonnable —
« Features », « Fixes », « Dependencies » — aurait donc vidé chaque release au lieu de
l'organiser, et le symptôme aurait été une release vide plutôt qu'une erreur.
→ Avant d'introduire un filtre sur des données existantes, mesurer combien d'entre elles le
traversent. Ici : lister les labels des douze dernières PR, constater qu'il n'y en a aucun, et
en déduire que l'attrape-tout n'est pas un ornement mais la règle principale.

### L-68 — `gh` vise le dépôt parent quand on travaille dans un fork — 2026-09-06
`gh pr list` a renvoyé des PR dont les numéros ne correspondaient pas à l'historique de
`master` : `#35` était « Bump golang.org/x/image » côté outil et « Record what publishing
v0.2.1 established » côté git. `Canaill51/superview` est un fork de `Niek/superview`, et sans
dépôt par défaut `gh` résout vers le parent. Un relevé de labels entièrement faux en est sorti,
et surtout : `gh pr create` aurait proposé le travail à l'auteur d'origine.
→ Quand une sortie d'outil contredit `git log`, soupçonner la cible avant les données.
`gh repo view --json nameWithOwner` le dit en une commande, et
`gh repo set-default <owner>/<repo>` le fige. `git remote -v` ne suffit pas à détecter le
problème : `origin` peut être correct pendant que `gh` regarde ailleurs.


### L-69 — Mesurer le conteneur ne mesure pas ce qu'il contient — 2026-09-06
`TestToolbarFitsWindow` demandait sa largeur à une rangée de `container.NewGridWrap(150x34, btn)`.
Un `GridWrap` **rapporte la taille qu'on lui a donnée** : la réponse était 920 px quels que
soient les boutons dedans, y compris quand ils en réclamaient 186. Le test comparait donc une
constante à une autre constante, et sa contre-épreuve — allonger un libellé — ne le faisait pas
bouger d'un pixel.
→ Quand un conteneur impose une géométrie à son contenu, l'assertion utile va **du contenant
vers le contenu** : cellule ≥ `MinSize()` du widget qu'elle porte, une comparaison par bouton.
Mesurer la somme des cellules répond à une autre question que « est-ce que ça s'affiche ».
Corollaire : un test de mise en page construit les widgets **comme le fait le code** — celui-ci
les créait sans leurs icônes, soit une trentaine de pixels de moins que les vrais.

### L-70 — Un test de mise en page s'écrit sur le rendu, pas sur l'arbre de widgets — 2026-09-06
Première version de `TestToolbarIsCentred` : descendre l'arbre de conteneurs, vérifier que le
`Center` a un seul enfant, lire `Position()` dessus. Contre-épreuve — repasser à un `HBox` nu —
elle rougissait bien, mais sur `expected the toolbar to wrap a single row, got *fyne.Container`.
Elle constatait un changement de structure, pas un défaut d'affichage : un `Center` mal placé
l'aurait laissée verte, et une réécriture équivalente de la mise en page l'aurait fait rougir
pour rien.
→ Poser le composant dans un canevas (`test.NewWindow`) et interroger la position **absolue**
des widgets visibles (`Driver().AbsolutePositionForObject`). La contre-épreuve dit alors
« la rangée commence à x=4 dans une fenêtre de 980 px », ce qui est le défaut lui-même.
Prolonge L-54 : une contre-épreuve doit rougir pour la bonne raison, pas seulement rougir.

### L-71 — Une liste de capacités décrit le binaire, pas la machine — 2026-09-06
`ffmpeg -encoders` et `ffmpeg -hwaccels` répondent à « avec quoi ai-je été compilé ». Superview
en déduisait ce que la machine savait faire. Sur la machine de développement, six des encodeurs
annoncés ne peuvent ouvrir aucun périphérique ; chez l'utilisateur, `h264_nvenc` était annoncé
par un binaire dont le pilote refusait chaque trame. La fenêtre promettait alors une
accélération qu'aucune couche n'avait vérifiée.
→ Quand une capacité dépend d'une ressource extérieure au programme — un pilote, un
périphérique, un service — la seule question honnête est de la solliciter. Ici : encoder une
trame de 256×256 et lire le code de retour, 0,95 s pour dix encodeurs. Une déclaration
d'intention n'est pas une mesure. Constat U-03.

### L-72 — Un numéro de version n'est pas un contrat de compatibilité — 2026-09-06
« FFmpeg 8.1.2 ne permet plus l'accélération matérielle » était vrai pour l'utilisateur et faux
en général : `nvenc.c` est identique entre 8.0 et 8.1, et le plancher pilote est fixé par les
`nv-codec-headers` de compilation. Mesuré : deux binaires « 8.1.2 » exigent l'un 570, l'autre
610. Le même empaqueteur a franchi la marche entre 8.1.1 et 8.1.2 sans que le numéro le dise.
→ Avant d'attribuer un comportement à une version, chercher la propriété qui le détermine
vraiment, et la mesurer sur l'artefact. Ici elle était lisible dans le binaire :
`strings ffmpeg.exe | grep -E '^(610\.00|570\.0|530\.41\.03)$'`. Corollaire pour l'utilisateur
d'un signalement : demander **d'où vient** le binaire, pas seulement sa version.

### L-73 — Une sonde mal formée accuse ce qu'elle mesure — 2026-09-06
La première sonde VAAPI envoyait des trames logicielles à un encodeur qui n'accepte que des
trames déjà sur le périphérique. Elle échouait dans le graphe de filtres, **avant** d'atteindre
le pilote, et rendait « inutilisable » pour un encodeur parfaitement fonctionnel. Une telle
sonde est pire que pas de sonde : elle retire du matériel aux machines qui en ont.
→ Avant de tirer une conclusion d'un échec, vérifier que la question posée pouvait recevoir une
réponse. Le symptôme à reconnaître : un message d'erreur qui parle d'autre chose que du sujet —
ici `Impossible to convert between the formats supported by the filter`, qui ne mentionne ni
pilote ni périphérique. Voir aussi L-49, où c'était l'environnement qui rendait la mesure muette.

### L-74 — Épingler une dépendance, c'est épingler une propriété, pas un numéro — 2026-09-06
Le fork épinglait « FFmpeg 8.1.2 ». Ce qui compte n'est pas ce nom : c'est le plancher pilote
NVENC compilé dedans. Deux builds portant ce même nom exigent 570 et 610, et le second est
inutilisable sur une carte professionnelle. Un pin sur la version aurait donc pu être « monté »
un jour par routine et casser exactement les machines qu'il protégeait, sans que rien ne bouge
dans le fichier.
→ Nommer la propriété recherchée, l'extraire de l'artefact et la vérifier en CI. Ici :
`.github/scripts/nvenc-driver-floor.sh` relit le littéral dans le binaire téléchargé et fait
échouer la release s'il a changé. Un pin sans garde-fou n'épingle que l'intention.

### L-75 — Une URL épinglée doit être choisie pour sa durée de vie — 2026-09-06
Le pin du fork visait un autobuild BtbN de milieu de mois : il est en **404** aujourd'hui,
donc son workflow de release ne pouvait plus aboutir. Mesuré avant de choisir : BtbN conserve
le dernier build de **chaque fin de mois** — celui d'octobre 2024 répond toujours — et gyan.dev
publie des releases versionnées, permanentes par construction.
→ Avant d'épingler une URL externe, vérifier ce que l'amont conserve, pas ce qu'il publie.
Deux commandes suffisaient : lister les tags et interroger le plus ancien.

### L-76 — Vérifier l'installation, pas seulement la construction — 2026-09-06
L'étape ajoutée pour contrôler l'empaquetage installe le paquet dans un répertoire jetable. Elle
a immédiatement échoué — pas sur le code ajouté, mais sur une ligne amont : le `Makefile` de
fyne 1.7.2 référence `usr/local/share/pixmaps/$(Icon)` alors qu'il livre `$(Icon).png`.
`make install` échoue donc à sa dernière ligne, **après** avoir installé le binaire, ce qui rend
la panne invisible à qui ne lit pas la sortie jusqu'au bout. Toutes les archives Linux publiées
par ce projet étaient concernées.
→ Un artefact de release se juge à ce qu'il produit une fois installé, pas à ce qu'il contient.
La vérification tient en trois lignes — `make install DESTDIR=…` puis un test d'existence — et
elle trouve ce qu'aucune relecture de l'archive ne montre.

### L-77 — Chercher un nom dans un message n'est pas vérifier ce qu'il dit — 2026-09-06
Le test d'intégration du chemin Vulkan vérifiait que le résumé d'après-encodage contenait
`h264_vulkan`. Or le message de repli est `Hardware: h264_vulkan failed; used CPU encode
(libx264)` : il contient le nom, précisément parce qu'il rapporte son échec. Le test passait donc
sur un repli CPU — c'est-à-dire sur le défaut même qu'il devait attraper — et il l'a prouvé en
restant vert alors que tout le montage du périphérique avait été retiré du pipeline.
→ Assertion sur le sens, pas sur la présence d'un jeton : `strings.HasPrefix(summary,
"Hardware: used "+encoder+" encode")`. Un vocabulaire d'échec réutilise toujours le vocabulaire
du succès ; un `Contains` sur un nom propre est presque toujours une assertion trop faible.

### L-78 — Une contre-épreuve appliquée au code partagé se neutralise elle-même — 2026-09-06
La sonde et la conversion emploient volontairement les mêmes fonctions, pour ne pas poser deux
questions différentes. Résultat : casser `hwUploadFilters` cassait aussi la sonde, l'encodeur
n'était plus retenu, et le test d'intégration se **sautait** en annonçant `ok`. Deux contre-épreuves
ont ainsi paru muettes alors que le défaut était bien réintroduit.
→ Saboter au **site d'appel** que le test exerce, pas dans la fonction commune : ici
`remapFilterChain` et `buildEncodeBaseArgs`, pas `hwUploadFilters` ni `hwDeviceArgs`. Et lire la
sortie `-v` : un `ok` qui vient d'un `SKIP` n'est pas un test qui passe. Prolonge L-49.

### L-79 — Un champ rempli avant que sa valeur existe est périmé par construction — 2026-09-06
Le gabarit du § 3 demandait le **sha de fusion**. Une entrée s'écrit avec le correctif, donc avant
la fusion : le sha n'existe pas encore et l'auteur écrit « non commité », en se promettant de
revenir. Personne ne revient. Dix entrées sur treize le portaient encore, dont quatre écrites en
sachant que c'était faux.
→ Un champ doit demander une valeur **connue au moment où on le remplit**. Le numéro de PR l'est
dès l'ouverture, ne périme jamais, et rend le sha d'un `git log --grep '(#NN)'` — on garde donc la
traçabilité en supprimant la dette. Même famille que L-65 : quand un artefact exige une mise à
jour ultérieure récurrente, chercher la formulation qui rend cette mise à jour inutile plutôt que
le garde-fou qui la rappelle.

### L-80 — Balayer les symboles ne balaie pas les affirmations — 2026-09-06
AGENTS.md prescrit, après un changement d'API, `grep -rn "FunctionName" --include='*.md' .`.
Fait, et sans résultat : le chantier matériel n'a renommé aucune fonction. Ce qu'il a rendu faux,
ce sont des **phrases** — « installez FFmpeg », « voici ce que contient l'archive », « vérifiez
votre GPU avec `ffmpeg -encoders | grep nvenc` ». Le README a donc continué à prescrire
`winget install Gyan.FFmpeg`, c'est-à-dire précisément le build qui avait causé le signalement,
à travers trois PR qui modifiaient ce même fichier. C'est l'utilisateur qui l'a vu.
→ Après un correctif, se demander **quelles promesses la documentation faisait sur le
comportement qui vient de changer**, et relire les sections concernées en entier. Trois questions
suffisent à les trouver : qu'est-ce que le README dit d'**installer**, qu'est-ce qu'il dit de
**vérifier**, et qu'est-ce qu'il dit que l'utilisateur **recevra** ? Un `grep` sur un identifiant
ne peut répondre à aucune des trois.
*Corollaire mesuré ici* : modifier un fichier dans une PR ne le met pas à jour. Les trois PR
avaient toutes touché `README.md` — une seule section chacune.

### L-81 — Un binaire non signé a une étape d'installation que le README ne voit pas — 2026-09-07
Un utilisateur lambda n'a pas su installer l'application sous Windows. Le README
donnait pourtant le bon nom d'archive et la bonne commande. Ce qu'il ne donnait pas,
c'est ce que Windows interpose entre le téléchargement et le premier lancement :
`.github/workflows/release.yml` ne signe pas les binaires, donc SmartScreen affiche
« Windows a protégé votre ordinateur », une fenêtre qui ne propose que *Ne pas
exécuter* et cache *Exécuter quand même* derrière un lien *Informations
complémentaires*. Personne dans le projet ne l'avait jamais vu : on lance ses propres
compilations, jamais un fichier marqué du web.
→ **La documentation d'installation décrit le trajet du lecteur, pas celui de
l'auteur.** Tout ce que le système d'exploitation, le navigateur ou l'antivirus
interpose entre le clic de téléchargement et la fenêtre de l'application *est* une
étape d'installation, et doit y figurer avec le libellé exact de ses boutons. Trois
questions le trouvent : que voit le lecteur **avant** que le fichier arrive, que voit-il
**quand il l'ouvre**, et qu'est-ce qui, dans son environnement mais pas dans le nôtre,
lui refuse quelque chose ?
*Corollaire* : une instruction en forme de commande de terminal (`cd`, `.\prog.exe`)
adressée à un utilisateur final est une instruction fausse — non parce qu'elle ne
marche pas, mais parce qu'il ne l'exécutera pas. Prolonge L-64.

### L-82 — Une ancre de documentation citée par le code est une API — 2026-09-07
Restructurer le README a failli casser `#requirements`, codé en dur dans
`gui_main.go` et répété dans le texte d'erreur de `common/common.go` — l'URL montrée
précisément à l'utilisateur qui n'a pas FFmpeg. Un fragment qui ne désigne aucun titre
ne produit pas d'erreur : il dépose le lecteur en haut du document, en silence. Le
balayage de L-29 ne l'aurait pas vu, puisqu'il porte sur les noms de symboles et qu'ici
c'est une chaîne de caractères qui pointe vers du Markdown.
→ **Avant de renommer ou de supprimer un titre, chercher son ancre dans le code** :
`grep -rn "readme-ov-file\|#nom-du-titre" --include='*.go' .`. Et quand le code cite
une ancre, la pinner par un test qui relit le fichier Markdown — c'est fait ici par
`readme_link_test.go`, contre-éprouvé dans les deux sens (constante périmée, titre
renommé).

### L-83 — Publier en deux langues, c'est doubler la surface d'une affirmation fausse — 2026-09-07
`README_FR.md` a été ajouté à côté de `README.md`. Le risque n'est pas la traduction :
c'est que l'un des deux cesse d'être vrai sans que l'autre bouge — le défaut U-06, en
double. Une règle écrite ne suffit pas : U-06 était déjà la démonstration qu'on oublie
de balayer la prose.
→ **Un miroir se garde par la CI, pas par la discipline.** Le job `readme-parity`
refuse qu'une PR touche un README sans l'autre, et que leurs structures de titres
divergent. Il compare les **niveaux** de titres, jamais les intitulés, qui sont
traduits. Deux pièges à connaître : `.all-contributorsrc` doit lister les deux fichiers,
sinon la PR du bot en touche un seul et échoue à chaque nouveau contributeur ; et sous
`set -o pipefail`, `printf | grep -q` renvoie 141 quand `grep` sort à la première
correspondance, ce qui **inverse le test au moment précis où le fichier est présent** —
d'où la here-string.
*Second volet* : la convention de langue du dépôt — journaux d'audit en français, tout
ce qui est lu de l'extérieur en anglais — n'était écrite nulle part (L-47). Elle l'est
dans `AGENTS.md`, avec la règle de co-modification. Dans le fichier français, les
**libellés de l'interface restent en anglais** — l'interface n'est pas traduite, et
traduire *Choose input file* enverrait le lecteur chercher un bouton inexistant —
tandis que les chaînes de **Windows** sont celles d'un Windows français.

### L-84 — Un signal reçu n'est pas une action de l'utilisateur — 2026-09-08
`ErrCancelled` couvrait le bouton *Cancel* et un `SIGTERM` du système. Sur le poste d'un
utilisateur, le noyau a tué ffmpeg par manque de mémoire, systemd a arrêté l'unité, et le
journal a écrit « encoding interrupted by user » pour un arrêt que personne n'avait
demandé. Le premier tour de diagnostic est parti dans la mauvaise direction à cause de
cette seule ligne, et la cause réelle n'apparaissait dans aucune autre.
→ **Un message d'erreur nomme l'acteur qu'il connaît, pas celui qui est commode.** Deux
causes qui n'ont ni le même responsable ni le même remède ne partagent pas une
sentinelle, même quand elles empruntent le même chemin de code. Et un arrêt subi se
journalise avec l'état de la machine au moment où il arrive : ici `MemAvailable`, la
seule mesure qui aurait permis de conclure sans enquête.

### L-85 — Un processus tué de l'extérieur porte un diagnostic, pas seulement un échec — 2026-09-08
Un ffmpeg tué par l'OOM killer ressortait en « ffmpeg failed: signal: killed ». Tout est
exact et rien n'est utile : le mot mémoire n'y figure pas, alors que c'est la seule
chose que l'utilisateur puisse corriger.
→ **Quand tout ce que le programme envoie à un enfant est déjà comptabilisé, un enfant
tué l'a été par la machine, et le statut d'attente le dit.** Le distinguer d'un code de
sortie non nul demande un fichier par plateforme — le signal vit dans un champ Unix du
`WaitStatus` — et c'est ce qui sépare un diagnostic d'une supposition attachée à tous
les échecs.

### L-86 — Une contre-épreuve qui ne compile pas n'a rien éprouvé — 2026-09-08
Une des quatre contre-épreuves de U-09 a produit `declared and not used: status`. La
suite était rouge, la case était cochée — et le test n'avait pas tourné une seule fois.
→ **Lire ce qui a rougi, pas seulement qu'il a rougi.** Un `FAIL` de compilation et un
`FAIL` d'assertion s'affichent presque pareil ; seul le second prouve quelque chose. La
contre-épreuve avait été réécrite pour que le défaut réintroduit laisse le code
compilable, et c'est seulement là qu'elle a démontré ce qu'elle prétendait.

### L-87 — Une préférence ne l'emporte pas sur une capacité — 2026-09-08
`FindEncoder` ne consultait que la liste du codec de la source. Sur un HD 620 qui refuse
tout le HEVC et accepte le H.264, cela a coûté un quart d'heure d'encodage logiciel et
6 Go de mémoire sur une machine qui en avait 8 — alors qu'un encodeur matériel attendait
dans l'autre liste.
→ **Conserver le codec de la source est une préférence ; utiliser le matériel de la
machine est une capacité.** Quand les deux s'opposent, c'est la capacité qui décide, et
ce que la préférence abandonne se dit à l'utilisateur au lieu de se découvrir dans le
fichier produit. Corollaire vérifié en contre-épreuve : la bascule ne se justifie que
par le matériel — échanger deux encodeurs logiciels ne ferait que changer de codec pour
rien — et le repli, si le matériel refuse ensuite la vraie image, doit revenir à la
famille de la source, sans quoi la bascule laisse une trace après avoir échoué.

### L-88 — Un garde-fou se chiffre au banc, et refuse ce qu'il a mesuré — 2026-09-08
Le contrôle disque veillait sur 57 Mo de cartes ; les 6 Go de l'encodage n'étaient
gardés par rien, alors que leur échec tue le processus. Écrire le garde-fou manquant
demandait un nombre, et un nombre plausible n'aurait rien valu : c'est le banc qui a dit
290 octets par pixel de sortie en 8 bits et 455 en 10 bits, et qui a montré que la
dépense appartient à l'image — libx264 à 1 % de libx265 — et non au codec ni à la durée
du clip.
→ **Un garde-fou ne s'étend pas au-delà de ce qui a été mesuré.** Les encodeurs matériels
n'ont pas de chiffre ici, donc ils n'ont pas de refus : refuser sur un nombre inventé est
pire que ne pas vérifier. Et une lecture impossible n'est pas un refus — sans
`/proc/meminfo`, la vérification s'abstient, exactement comme le contrôle disque quand
l'espace libre ne se lit pas.

### L-89 — Deux endroits qui décident de la même chose finissent par diverger — 2026-09-08
La chaîne de filtres choisit la profondeur de travail, et l'estimation mémoire doit
retomber sur la même réponse : dix bits coûtent une moitié de plus. Recopier la
condition `isHighBitDepth(pixFmt) && isHEVCEncoder(encoder)` aurait suffi à faire dériver
l'estimation d'autant, sans qu'aucun test ne s'en aperçoive — les deux copies restant
vraies séparément.
→ **Une décision partagée s'extrait en fonction dès la deuxième lecture, pas à la
troisième copie.** `encodesInTenBits` est ce que la contre-épreuve a validé : lui faire
ignorer l'encodeur rougit le test de l'estimation, ce qu'une condition recopiée n'aurait
pas permis d'éprouver d'un seul geste.

### L-90 — Un état normal journalisé en avertissement noie le journal — 2026-09-08
`out_time_ms=N/A` est ce que ffmpeg répond tant qu'aucune image n'est sortie du graphe
de filtres. Le programme le traitait comme un échec d'analyse : 45 `WARN` en 81 secondes
dans le journal d'un utilisateur, pour une conversion qui fonctionnait — et c'est le
fichier que le README demande de joindre à un rapport de bug.
→ **Avant de journaliser un cas en avertissement, vérifier qu'il est anormal.** Ici la
distinction est nette et vaut d'être codée : `N/A` est prévu et part en `Debug`, une
valeur qui n'est ni `N/A` ni un nombre reste un avertissement, parce qu'elle signifierait
que le format de progression a changé sous nos pieds. Corollaire côté interface : quand
un état normal dure — ici des secondes à une minute sans le moindre chiffre — la fenêtre
doit le dire, sans quoi l'utilisateur annule une conversion qui marche.

### L-91 — Un seuil de santé doit se dériver du besoin, pas d'un chiffre rond — 2026-09-08
Le contrôle de santé exigeait 10 Go libres dans le répertoire temporaire, qui n'accueille
que les cartes de remappage — 113 Mo pour le plus grand format traité. Sur une machine de
8 Gio, où `/tmp` est un tmpfs dimensionné à la moitié de la RAM, le seuil était
**structurellement insatisfiable** : `UNHEALTHY` à vie, en tenant trente fois ce que la
conversion demandait. Un utilisateur a joint ce diagnostic à son signalement, et il
pointait dans la mauvaise direction.
→ **Un seuil se justifie par ce que l'espace sert à contenir.** Sinon il n'informe pas :
il crie en permanence, et un contrôle qui échoue toujours ne se lit plus.

### L-92 — Une API dont la mauvaise utilisation est invisible se remplace, pas se documente — 2026-09-08
`main()` construit son logger deux fois — une fois pour en avoir un, une fois le niveau
configuré connu — et appelait `RegisterObservabilityHandler` à chaque fois. L'appel
*ajoutait*, donc les deux gestionnaires restaient et chaque événement d'encodage était
écrit deux fois dans le journal, pour toute la vie du processus. Aucun test ne pouvait le
voir : `main()` n'est pas testable, et chaque appel pris isolément était correct.
→ **Quand le mésusage ne produit ni erreur ni test rouge, changer la sémantique plutôt
que d'ajouter un commentaire.** `SetObservabilityHandler` remplace, ce qui rend le défaut
inexprimable et, cette fois, testable dans `common` — le test rejoue la séquence exacte
de `main()`. L'ancienne fonction, dont rien d'autre ne se servait, est retirée plutôt que
laissée en piège.

### L-93 — Une sonde répond à la question qu'elle pose, pas à celle qu'on se pose — 2026-09-09
La sonde de démarrage encode du 256×256 et prouve que le pilote répond. Le code le disait
lui-même — « A probe at this size cannot prove that a 4K frame will also encode » — et
c'est resté une remarque pendant trois passes, jusqu'à ce qu'un Intel HD 620 réponde
`Hardware does not support encoding at size 5120x2880 (constraints: width 32-4096)` à la
conversion, après avoir accepté la sonde.
→ **Un commentaire qui décrit une limite de la vérification est un constat, pas une
excuse.** Et poser la vraie question s'est révélé gratuit : une sonde qui réussit coûte
~0,30 s **quelle que soit la taille** (0,29 s à 256×256, 0,33 s à 7082×3984), parce qu'on
paie le démarrage du processus, pas les pixels. Mesurer avant d'écrire a transformé « c'est
peut-être trop cher au début de chaque conversion » en une constante connue.

### L-94 — Un garde-fou doit suivre l'objet qu'il garde — 2026-09-09
Le garde-fou mémoire chiffrait l'encodeur choisi au départ. Deux chemins pouvaient en
changer ensuite — le recul devant une taille refusée, et la cascade de repli interne à
`EncodeVideo` — et sur les deux il ne disait plus rien, alors que c'est précisément là que
l'encodeur devient logiciel et cher. Un garde-fou qui protège la première décision et pas
la dernière est une demi-vérité, et une demi-vérité rassure.
→ **Vérifier au dernier point où la décision peut encore changer, pas au premier où on la
connaît.** Corollaire pratique : quand la fonction qui décide n'a pas le contexte pour
recalculer (`EncodeVideo` ne reçoit pas `squeeze`), lire l'artefact déjà produit — ici
l'en-tête des cartes de remappage — vaut mieux que d'élargir une signature pour refaire un
calcul qui pourrait diverger.

### L-95 — Rendre asynchrone une méthode existante casse ses lecteurs — 2026-09-09
`refreshHardwareStatus` écrivait une étiquette et rendait la main. Y avoir glissé une
goroutine a produit une vraie course détectée par `-race` : huit tests lisent
`hardwareStatus.Text` à la ligne suivante. La tentation immédiate est un drapeau « ne pas
faire ça dans les tests » ; il aurait fait diverger le code testé du code livré.
→ **Séparer la moitié lente dans sa propre méthode plutôt que de conditionner
l'asynchronisme.** La méthode synchrone reste ce que ses appelants croient, la lente est
invoquée aux endroits où ses *entrées* changent, et son point d'application
(`applyVerifiedHardwareLine`) redevient testable de façon synchrone — compteur de
génération compris, qui sans cela n'aurait été éprouvé par rien.

### L-96 — Un garde-fou que la CI seule applique n'en est pas un — 2026-09-11
`SUPERVIEW_REQUIRE_FFMPEG=1` transforme en échec les `t.Skip` liés à ffmpeg, sans quoi
quinze tests — dont les sept d'intégration et l'équivalence du remap — rapportent succès
sans avoir encodé une image. `test.yml` la posait, `AGENTS.md` y consacrait un paragraphe,
et `make test`, la commande qu'un contributeur tape réellement, ne la posait pas. Le
contributeur voit vert, la CI voit rouge, et c'est la CI qui a raison — au mieux. Au pire
il n'ouvre jamais de PR et ne le saura pas.
→ **Poser le garde-fou dans le point d'entrée local, pas seulement dans le workflow.** La
règle documentée et la commande qui l'enfreint ont cohabité dix jours ; le paragraphe
d'avertissement ne suffit pas, parce qu'il faut le lire *avant* de croire un vert.
Corollaire de balayage : après avoir écrit une variable d'environnement obligatoire dans
un workflow, `grep -rn "go test" --include='Makefile' --include='*.yml' .` et vérifier
que chaque invocation la porte.

### L-97 — Une règle qu'il faut se rappeler au moment de cliquer finit par être mal appliquée — 2026-09-11
La règle de fusion était écrite, argumentée, et **fausse** sur son cas le plus piégeux :
« si votre base n'est pas `master`, vous êtes empilé » — or la PR du bas d'une pile a
justement `master` pour base et doit quand même passer par un commit de fusion. #59, #61 et
#62 l'ont toutes prouvé le même jour, chacune fusionnée à la main contre ce que le fichier
disait. Corriger la phrase ne suffit pas : la prochaine fois, la question se reposera devant
le bouton vert, à un moment où personne ne relit `AGENTS.md`.
→ **Quand la règle est décidable par une requête, la faire décider par la machine et ne
laisser à l'humain que le déclenchement.** `merge-on-label.yml` interroge
`gh pr list --base <tête>`, choisit la méthode, fusionne et **écrit sur la PR ce qu'il a
choisi et pourquoi** — la trace vaut autant que l'automatisme, parce qu'une décision
automatique muette n'est pas vérifiable. Corollaire : ce qui reste réglable en configuration
doit l'être plutôt que redit en prose — « Rebase and merge » retiré du menu vaut mieux qu'un
paragraphe qui demande de ne pas le cliquer.

### L-98 — Un document ne compile pas : ce qu'il compte vieillit sans rien casser — 2026-09-11
L'en-tête d'`ANALYSE.md` annonçait « cinq passes et les 67 constats » alors qu'il en portait
onze et 105 ; son ordre de lecture s'arrêtait deux passes plus tôt ; `AGENTS.md` donnait
`common.go` à « ~1600 lines » pour 2023 et `main()` à « ~540 » pour 585. Aucune de ces
phrases n'était fausse quand elle a été écrite, et aucune n'a jamais rougi. Le balayage de
L-80 ne les attrape pas : il porte sur ce que la doc **promet**, pas sur ce qu'elle
**compte**.
→ **Un chiffre dans de la prose est un test sans assertion.** Trois remèdes, par ordre de
solidité : ne pas l'écrire quand la phrase tient sans lui ; l'arrondir à un ordre de
grandeur qui survit à la dérive (« ~2000 lignes » plutôt que « 1600 ») ; et, quand il est
là pour naviguer, écrire à côté **ce qui l'invalide** — l'en-tête d'`ANALYSE.md` dit
désormais qu'ajouter une passe oblige à le rouvrir. Corollaire de balayage : en ajoutant une
passe ou une section, relire l'en-tête et le sommaire du même fichier, jamais seulement le
corps.

### L-99 — Un motif de version n'est pas une version : `1.26.x` veut dire « ce qui est déjà là » — 2026-10-14
`go-version: '1.26.x'` se lisait comme « le dernier correctif de Go 1.26 ». `actions/setup-go`
le lit autrement : la première version **en cache sur l'image du runner** qui satisfait le
motif, sans consulter le manifeste. Go 1.26.9 corrigeait neuf vulnérabilités de la
bibliothèque standard ; les images portaient 1.26.8 ; `govulncheck` a rougi sur trois PR
Dependabot qui n'y étaient pour rien, et la release aurait compilé avec la version
vulnérable sans que rien ne le dise.
→ **Un motif de version dans un workflow s'accompagne de ce qui le force à se résoudre au
plus récent** — `check-latest: true` pour `setup-go`. Épingler `1.26.9` n'est pas le remède :
ce serait le même défaut au correctif suivant. Corollaire de diagnostic : quand une
vérification rougit sur une PR qui ne touche pas ce qu'elle vérifie, lire dans le journal du
job **quelle version a réellement tourné** avant de lire le diff.

```bash
grep -n -A3 "uses: actions/setup-go" .github/workflows/*.yml | grep -c "check-latest: true"
grep -c "uses: actions/setup-go" .github/workflows/*.yml | awk -F: '{n+=$2} END {print n}'
```
Les deux nombres doivent être égaux.

### L-100 — Un réglage du dépôt ne vaut pas pour ce que fait le jeton d'un workflow — 2026-10-14
*Delete branch on merge* était activé, et vérifié quand on fusionnait à la main. M-03 a
confié la fusion à `merge-on-label.yml`, et le réglage a cessé de s'appliquer sans rien dire :
GitHub ne déclenche aucun automatisme sur ce que fait `GITHUB_TOKEN`, ni suppression de
branche, ni workflow. Le défaut est resté invisible un mois parce que les seules PR fusionnées
par le workflow étaient celles de Dependabot, qui supprime ses propres branches.
→ **Quand un workflow reprend un geste humain, relire la liste des réglages qui réagissaient
à ce geste** : chacun cesse de s'appliquer. Et une vérification sur une population qui
nettoie derrière elle ne prouve rien pour les autres. Pour lister ce qui reste :

```bash
git fetch --prune && for b in $(git branch -r --format='%(refname:lstrip=3)' | grep -v '^HEAD$\|^master$'); do
  s=$(gh pr list --head "$b" --state merged --json number --jq 'length'); [ "$s" -gt 0 ] && echo "fusionnée, encore là : $b"
done
```

### L-101 — Une sentinelle créée pour distinguer deux causes doit être relue partout où la première est testée — 2026-10-14
U-09 a scindé `ErrCancelled` en deux : l'annulation de l'utilisateur et l'arrêt demandé par
le système. Le nouveau sentinel a reçu son test, son message, son `app.Quit()` dans la GUI —
et aucune des trois portes de la cascade de repli, qui ne testent que `errors.Is(err,
ErrCancelled)`. Résultat mesuré : six ffmpeg pour un seul SIGTERM, et un signal perdu quand
la relance aboutit (E-02). Le test de U-09 était vert à vide : il exerçait la seule
configuration sans relance (L-37).
→ **Quand on dédouble une erreur, la commande à lancer est `grep -rn "errors.Is(err,
AncienSentinel)"` et chaque occurrence se décide** : l'une, l'autre, ou les deux. Un
prédicat nommé (`isStopRequest`) vaut mieux que la liste, parce que la troisième cause
n'aura pas non plus à la retrouver.

### L-102 — Un bouton qui fait la même chose qu'un geste du système doit passer par le même chemin — 2026-10-14
L'interception de fermeture demandait confirmation pendant un encodage et annulait proprement.
Le bouton *Quit*, trois lignes plus bas, appelait `app.Quit()` directement : la même intention,
sans la confirmation ni l'annulation, et un ffmpeg orphelin (E-03). Fyne lui-même a un
troisième chemin, son `catchTerm` sur SIGTERM, qui ne passe par aucun des deux.
→ **Pour toute sortie de l'application, lister les chemins qui y mènent — bouton, croix de
la fenêtre, signal, menu — et les faire converger sur une seule fonction.** `SetOnClosed`
est ce point de convergence chez Fyne ; un `time.Sleep` avant `Quit` n'est pas une attente.
*Corrigé en appliquant E-03* : `SetOnClosed` ne converge que les chemins qui passent par
`window.Close()`, c'est-à-dire `app.Quit()`. Le `catchTerm` de Fyne appelle `driver.Quit`
directement (`driver_desktop.go:278-284`, `driver.go:132`), et aucune fenêtre n'est fermée :
le chemin signal se rattrape **après le retour de `ShowAndRun()`**, voir L-109.

### L-103 — Un test « les deux posent la même question » doit comparer la question entière — 2026-10-14
`TestProbeAndConversionAskTheSameQuestion` comparait les options de périphérique et la
présence de `hwupload`, et c'est tout ce qu'il garantissait. L'encodage a gagné un contrôle
de débit, un preset et une chaîne 10 bits que la sonde n'a jamais reçus, et le test est
resté vert pendant qu'un pilote CQP-only passait la sonde et refusait l'encodage (E-01).
→ **Construire les deux argv à partir des mêmes fonctions et comparer tout ce qui n'est pas
l'entrée** ; un test qui ne compare que ce qu'on a pensé à comparer protège exactement
contre ce qu'on a déjà vu.

### L-104 — Automatiser un geste, c'est aussi perdre ce que ce geste déclenchait — 2026-10-14
Suite de L-100, dont une moitié seulement a été appliquée. La fusion par `GITHUB_TOKEN` ne
supprime plus la branche (corrigé, M-06) **et ne déclenche plus aucun workflow sur `master`**
(E-06) : cinq fusions sans un seul run, et un ruleset non strict qui laisse fusionner une PR
testée contre une base périmée. L-100 l'avait écrit, « ni suppression de branche, ni
workflow », et personne n'a relu la seconde moitié de sa propre phrase.
→ **Après une automatisation, vérifier chaque effet secondaire listé, pas seulement celui
qui a fait ouvrir le ticket** : `gh run list --branch master --event push --limit 3` dit en
une ligne si la CI vit encore.

### L-105 — Déplacer un échec de l'ouverture vers la fin le rend destructeur — 2026-10-14
Avant P-04, ffmpeg échouait à ouvrir la destination dans la première seconde. Avec le fichier
de travail, la même cause (destination devenue répertoire, verrouillée, interdite) frappe au
renommage final, et le `defer` de nettoyage efface alors l'encodage terminé (E-04). Même
famille : E-13, où la cascade de repli rejoue cinq encodages complets après un échec à 95 %.
→ **Quand on reporte une opération, relire ce que font les chemins d'erreur à son nouvel
emplacement** : un nettoyage juste pour un échec précoce est une perte de données pour un
échec tardif. Distinguer « a échoué avant de produire » de « a échoué après ».

### L-106 — Une assertion « jamais » dans un test périme sans bruit — 2026-10-14
`TestGUIHandler_GetSqueezeAndBitrate` affirmait « the GUI never enables squeeze today ;
update this test if that changes ». C-04 a exposé la case squeeze le jour même ; le test est
resté, vert, faux, pendant douze passes (E-19). Il ne pouvait pas rougir : il testait une
valeur par défaut, pas un comportement.
→ **Un test qui code une absence (« jamais », « pas encore ») doit être remplacé par un test
de la présence le jour où elle arrive, et le commentaire qui le demande ne le fera pas.**
Lister les « never » : `grep -rn "never\|jamais\|not yet" --include='*_test.go' .`

### L-107 — `workflow_dispatch` est l'exception à la règle du `GITHUB_TOKEN` — 2026-10-14
Suite de L-104, qui disait quoi vérifier et pas quoi faire. La règle complète de GitHub est :
« events triggered by the `GITHUB_TOKEN`, **with the exception of `workflow_dispatch` and
`repository_dispatch`**, will not create a new workflow run ». Les deux autres issues — ruleset
strict, jeton d'application — coûtent chacune quelque chose que ce dépôt a déjà refusé de
payer (DEC-35) ; l'exception ne coûte qu'un `actions: write` et un `on: workflow_dispatch`.
→ **Quand une automatisation remplace un geste humain, ce que ce geste déclenchait se relance
par l'exception documentée, pas en élargissant le jeton.** Et lire la phrase de la doc
jusqu'à sa subordonnée : la solution était dans la même ligne que le problème.

### L-108 — Une clé de cache immuable appartient au premier job qui finit — 2026-10-14
`setup-go` sauve module et build caches sous une clé par image, version de Go et `go.sum`,
commune à tous les jobs Linux des deux workflows. Le job `fmt`, sept secondes, sans un
module téléchargé, finissait toujours premier : il écrivait la clé avec 7,5 Ko dedans
(journal de `801660d`), et chaque autre job la « touchait » puis recompilait le cgo de Fyne
à froid — tous les runs, depuis des mois, sous un `Cache restored successfully`.
→ **Un job qui ne construit rien ne doit pas sauver de cache (`cache: false`), et un cache
« restauré » se juge à sa taille, pas à son message.** Vérifier :
`gh api repos/$REPO/actions/caches --jq '.actions_caches[] | "\(.size_in_bytes) \(.key)"'`.
### L-109 — Un crochet de framework ne couvre que les chemins qui le traversent — 2026-10-14
Le plan d'E-03 posait `SetOnClosed` comme le point par lequel passent `app.Quit()` et le
gestionnaire de signaux de Fyne. Lu dans le code de fyne 2.8.2 avant d'y accrocher quoi que
ce soit : `app.Quit()` ferme chaque fenêtre, et `window.Close()` appelle bien `onClosed` ;
`catchTerm`, lui, fait `fyne.Do(d.Quit)` sur le **driver**, qui ferme son canal `done` et ne
touche à aucune fenêtre. Le crochet n'aurait jamais vu SIGTERM, et le test unitaire du
crochet aurait été vert. Le point qui voit vraiment tout est le retour de `ShowAndRun()` :
quand la boucle s'arrête, par quelque chemin que ce soit, `main()` reprend la main.
→ **Avant d'accrocher un crochet, suivre chaque chemin de sortie dans le code du framework
jusqu'au crochet, ou jusqu'à la preuve qu'il n'y passe pas.** Et vérifier ce que le
framework fait des appels en attente une fois arrêté : Fyne exécute en ligne, sur la
goroutine appelante, les `fyne.Do` postés après le drain (`runOnMainWithWait`) — une
branche de fin doit donc savoir que la fenêtre n'existe plus.

### L-110 — Un signal de fin se libère avant tout appel qui peut mener à celui qui l'attend — 2026-10-14
La goroutine d'encodage fermait `done` par un `defer`, donc après la branche de fin. Or
cette branche peut, sur un arrêt système, quitter l'application, ce qui ferme la fenêtre,
dont le crochet attend `done` ; et une fois la boucle Fyne vidée, tout cela s'exécute en
ligne sur la goroutine elle-même. Elle s'attendait elle-même jusqu'à l'expiration du délai.
Un `defer close(done)` « en première ligne » paraît la forme la plus sûre ; il ne l'est que
si rien, entre-temps, ne peut remonter jusqu'à l'attente.
→ **Fermer un canal de fin dès que ce qu'il promet est vrai, avant d'appeler quoi que ce
soit qui puisse mener à celui qui l'attend ; garder le `defer` (via `sync.Once`) pour la
panique seulement.** Pour chaque canal attendu à l'arrêt, suivre ce qu'appelle la goroutine
après le travail : `grep -n "close(done)\|<-done" *.go common/*.go`.

### L-111 — Un check requis sauté compte comme réussi — 2026-10-14
`Coverage Report & Gate` était le seul check requis à porter le verdict des tests, par
`needs: [test, test-windows]`. Un job en `needs` qui échoue fait **sauter** le job qui en
dépend, et GitHub traite un check requis sauté comme satisfait : #85, #86 et #87 ont fusionné
avec Windows rouge, sans que rien ne bloque. Le gate avait l'air d'un verrou et n'en était un
que tant que tout était vert.
→ **Tout check requis qui a des `needs:` porte `if: always()` et échoue lui-même si l'un des
`needs.<job>.result` n'est pas `success`.** Et un gate se prouve en le voyant rougir sur un
test rouge, pas en le voyant vert. Contrôle :
`grep -n "needs:" -A8 .github/workflows/*.yml | grep -E "needs:|if: always"` — le seul job
requis concerné ici, `coverage-report`, est couvert (contrôle fait le 2026-10-14).

### L-112 — Le nom d'un test finit dans un chemin, et `cmd.exe` coupe un chemin à la virgule — 2026-10-14
Le faux ffmpeg `.bat` de `stopcascade_test.go` vivait dans `t.TempDir()`, nommé d'après le
sous-test « copy refused, AAC accepted ». `cmd.exe` sépare ses arguments à `,` `;` `=` même
dans le chemin du script lancé : le `.bat` n'a jamais tourné, la seule trace était
« `'…AACcopy_refused' is not recognized` », et le registre d'appels vide faisait chercher la
cause dans `%*`. Les deux autres sous-tests, sans virgule, passaient.
→ **Une fixture `.bat` refuse un répertoire contenant `,` `;` `=` avec un message qui nomme le
nom du test.** Contrôle : `grep -nE 't\.Run\("[^"]*[,;=]' *_test.go common/*_test.go` dans
les fichiers qui écrivent un `.bat` (`grep -l '\.bat"' *_test.go common/*_test.go`) — un seul
cas trouvé le 2026-10-14, celui-ci.

### L-113 — Une constante mesurée appartient à la configuration où elle a été mesurée — 2026-10-14
Les 290 et 455 octets par pixel d'U-11 sortaient d'un banc à `-threads 4`, sur le portable du
signalement. Le code les appliquait à toute machine, qui encode avec un fil par cœur : à 16 fils
le besoin réel est ×1,47, et le garde-fou, écrit pour empêcher l'arrêt de ffmpeg par le noyau,
laissait passer exactement ces conversions. Le commentaire de la constante nommait ses
conditions (`-threads 4`) ; rien ne forçait le code à les respecter, et la question Q-02 est
restée ouverte jusqu'à ce qu'on la mesure.
→ **Une constante issue d'un banc se lit avec ses paramètres : tout paramètre du banc qui varie
en production devient un argument de la fonction qui l'utilise, pas une valeur oubliée.** Et
l'argument se branche à l'endroit où la production le décide — ici `effectiveEncoderThreads`,
la même fonction que la ligne de commande — avec un test par appelant : sans lui, deux appels
sur trois passaient encore la valeur du banc (contre-épreuve 3a/3b). Contrôle :
`grep -n "runtime.NumCPU" common/*.go | grep -v _test` — chaque occurrence doit passer par
`requestedEncoderThreads`, sauf `health.go` et `hardware.go`, qui ne décident rien.

### L-114 — Une étape ajoutée à `release.yml` n'est pas vérifiée tant qu'un essai à blanc ne l'a pas fait tourner — 2026-10-14
L'étape « Run the tests against the bundled FFmpeg » (#82) a été fusionnée sans essai à blanc.
Le premier qui l'a exécutée, après le déplacement vers `cmd/` (DEC-39), a échoué : le runner de
cette tâche n'a pas de `ffmpeg` dans le `PATH`, et les tests qui sondent par `exec.LookPath`
échouent sous `SUPERVIEW_REQUIRE_FFMPEG=1` au lieu de se sauter. Mettre le FFmpeg embarqué
dans le `PATH` a révélé le second défaut : `SUPERVIEW_FFMPEG_DIR`, positionné pour tout le
processus, court-circuite les faux `ffmpeg` que les tests écrivent eux-mêmes et les tests
d'ordre de résolution (11 échecs, reproduits en local). Rien de tout cela n'avait de lien avec
le déplacement, mais la première vraie release aurait échoué, et `test.yml` ne peut pas le
voir : elle installe ffmpeg par apt. Le dernier essai réussi datait
du 8 septembre, avant l'étape.
→ **Toute modification de `release.yml` se vérifie par un essai à blanc (`gh workflow run
release.yml --ref <branche>`, sans version) avant d'être fusionnée, pas après.** Les PR qui
touchent ce fichier ou ses chemins le disent dans leur description. Contrôle :
`gh run list --workflow=release.yml --limit 1 --json conclusion,headSha` — le dernier essai
réussi doit être postérieur au dernier commit qui a touché `release.yml`
(`git log -1 --format=%H -- .github/workflows/release.yml`).

### L-115 — Couper la sortie d'un outil dans un tube (`| head`, `| Select-Object -First 1`) le fait échouer — 2026-10-14
L'étape d'essai de l'installateur Windows (DEC-41) lançait `ffmpeg.exe -version | Select-Object -First 1`
puis testait `$LASTEXITCODE`. Le journal affichait « ffmpeg version 8.1.1 … » **et**
« the installed ffmpeg.exe does not run » : `Select-Object -First 1` ferme le tube après la première
ligne, l'outil écrit dans un tube fermé et sort avec un code non nul. Sous bash avec `set -o pipefail`,
`| head -n 1` donne de même le code 141 (SIGPIPE) quand la sortie est plus grosse que le tampon du tube ;
l'étape Linux n'a passé que parce que la sortie de `-version` tient dans ce tampon.
→ **Capturer toute la sortie avant d'en garder la première ligne** (`$out = & outil -version`, puis
`$out[0]` ; sous bash, vers un fichier, puis `head -n 1 fichier`), et tester le code de sortie sur la
commande elle-même. Contrôle : le test `workflows_do_not_cut_a_tools_output_short`
(`rust/crates/superview-app/src/repo_guards.rs`, repris du test Go `cmd/superview/workflow_pipes_test.go`
retiré avec Go, DEC-49) refuse ce motif dans `.github/workflows/*.yml` ; contre-preuve faite en
le réintroduisant dans `release.yml`. Trouvé par l'essai à blanc de la release (L-114), qui a donc payé.

### L-116 — Deux encodages multi-fil du même clip ne donnent pas les mêmes images — 2026-10-17
Le croisement Go↔Rust (phase 5, `internal/common/crosscheck_test.go`) comparait les images décodées
de deux conversions aux arguments ffmpeg identiques (prouvés par `parity_encode_args`). Les empreintes
divergeaient. Ce n'était pas un écart Go/Rust : `ffmpeg -threads 16` seul, lancé deux fois sur le
même clip, donne deux empreintes différentes avec libx264 sous `-b:v`/`-maxrate` ; libx265 est
non reproductible **même à un fil** (1 empreinte différente sur 5 à 8 lancements).
→ **Pour comparer deux encodages, fixer les fils** (`-threads 1` des deux côtés ; le banc Rust tourne
sous `taskset -c 0`, ce qui ramène `available_parallelism` à 1) : libx264 devient reproductible
(6 lancements identiques) et se compare bit à bit. Pour libx265, comparer un PSNR (seuil 50 dB,
écarts mesurés 69 à 75 dB) **et** les métadonnées ffprobe, qui ne dépendent pas du PSNR. Ne jamais
conclure à une régression d'un seul écart d'empreinte avant d'avoir lancé la même commande deux fois.
Limite connue : un PSNR ne voit pas un débit faux sur un clip court qui sature (1 Mbit/s contre
2 Mbit/s donne 63 dB). Prolonge L-36 (ne pas figer une empreinte qui dépend d'un outil externe).

### L-117 — Un test ne modifie pas l'environnement du processus : on le passe en paramètre — 2026-10-21
Les tests d'un crate Rust tournent en parallèle dans **un seul** processus. Un test qui pose une variable
(`std::env::set_var`, `unsafe` depuis l'édition 2024) change ce que voient tous les autres : un résultat
qui dépend de l'ordre d'exécution, vert sur une machine et rouge sur une autre. C'est la même famille
que le global de configuration, interdit par `AGENTS.md` : une dépendance cachée rend le test dépendant de la machine.
→ **Toute fonction qui lit l'environnement le reçoit en paramètre** (`&dyn Fn(&str) -> Option<String>`) ;
le code de production lui passe le vrai (`ToolResolver`, `user_config_dir` dans `superview-core`), le test lui
passe un dictionnaire. Aucun test ne touche `std::env::set_var`. La règle est dans `AGENTS.md`
(« The Rust workspace »).

### L-118 — Dans un hôte, ce que le plugin déclare peut coûter plus que ce qu'il calcule : mesurer aux deux bords — 2026-10-25
Le plugin OpenFX lisait à 6 à 10 i/s dans Resolve. Trois optimisations ont été tentées dans l'ordre où le profil du
plugin les désignait : seize fils CPU (la lecture a *baissé*, le décodage de l'hôte en manquait), le rendu CUDA (le
plugin est passé de 15 ms à 0,06 ms par trame, la lecture de 6 à 8 i/s seulement), puis la déclaration `FullySafe`
au lieu de `InstanceSafe` : 29,97 i/s, alors que Resolve n'a **jamais** lancé deux rendus à la fois. Le temps
perdu était dans l'hôte, et c'est une promesse faite à l'hôte qui l'a libéré.
→ **Chronométrer le plugin *et* le rythme auquel l'hôte l'appelle** (l'intervalle entre deux appels) : quand le
plugin ne pèse qu'une petite part de l'intervalle, l'optimiser ne sert plus ; chercher alors ce que l'hôte fait de ce
que le plugin *déclare* (sûreté des fils, GPU, tuiles, multi-résolution), en comparant avec un plugin de référence
rapide dans le même hôte (ici Gyroflow, même fork). Et vérifier l'hypothèse avant d'y croire : un compteur de rendus
simultanés a montré que « plusieurs trames à la fois », l'explication attendue, était fausse (DEC-66).

### L-119 — Un hook qui avale l'échec de son lecteur ne garde plus rien : il échoue fort, et se teste sans l'outil — 2026-10-26
Les trois hooks de `.claude/hooks` lisaient leur entrée JSON par `python3 … 2>/dev/null`. Sous Windows, `python3`
est le raccourci du Microsoft Store : il échoue (code 49), le champ lu est vide, et chaque hook prend ce vide pour
une réponse. `garde-fou.sh` laissait passer `git reset --hard`, `apres-modif.sh` ne lançait jamais la vérification
rapide, et le garde-fou anti-boucle d'`avant-fin.sh` ne se déclenchait jamais. Rien ne l'a signalé : sous Linux,
où les hooks ont été écrits, `python3` existe.
→ **Un hook distingue « champ vide » de « je n'ai pas pu lire »**, et dans le second cas il échoue du côté sûr :
un garde refuse, une vérification se lance quand même, et un garde qui ne peut pas trancher prévient sans
bloquer. Le lecteur est Perl et `JSON::PP` (`.claude/hooks/json.sh`), présents dans Git Bash comme sous Linux.
**Et le test retire l'outil qui manque ailleurs** : `scripts/checks/hooks.sh` met un faux `python3` qui échoue
en tête du `PATH`, ce qui fait reproduire le poste Windows par Linux, en CI aussi (`lint.yml`). Lancer
`bash scripts/checks/hooks.sh`.

### L-120 — Une sonde d'une trame ne voit pas ce qui casse à la troisième : sonder ce que la conversion fait, pas son début — 2026-10-26
`h264_d3d12va` passait la sonde et échouait à chaque conversion. La sonde partageait l'argv de la conversion
(DEC-05), mais pas sa durée : une trame, quand D3D12 avec images B en encode deux et échoue à la troisième, dès
qu'une trame en référence d'autres. « La même question » portait sur les options, pas sur ce que l'encodeur fait
d'un flux. Le test de bout en bout, lui, encode deux secondes : c'est lui qui a vu l'écart, sur le seul poste qui
avait l'encodeur.
→ **Une sonde reproduit ce qui distingue la conversion de son début** : plusieurs trames (huit,
`PROBE_DURATION`), pas seulement les mêmes options. Et pour trouver la cause, bisecter le nombre de trames
avant les options : `-frames:v 2` passe, `3` échoue, c'est une référence ; puis une option à la fois (`-bf 0`
passe, `-async_depth 1` non). Un écart avec une fixture gelée s'écrit en littéral dans le test : recopié de la
constante du code, il approuve n'importe quelle valeur (contre-preuve muette, corrigée ici). Lancer
`cargo test -p superview-core the_probe_sends_enough_frames`.

### L-121 — Un pré-contrôle cherche un outil là où le code le cherchera, pas là où on l'installe d'habitude — 2026-10-26
`scripts/checks/rust.sh` vérifiait `command -v ffmpeg` avant de lancer les tests, et refusait de les lancer sur le
poste Windows de l'utilisateur, dont ffmpeg n'est que dans `SUPERVIEW_FFMPEG_DIR`. La suite, elle, passait :
mesuré, sans ffmpeg dans le `PATH`, 0 échec, parce que les tests trouvent l'outil par `ToolResolver`. Le
pré-contrôle avait été écrit pour Linux, où ffmpeg est dans le `PATH` ; c'était une seconde recherche, plus
étroite que la vraie, et rien ne les comparait.
→ **Un contrôle préalable reprend l'ordre de recherche du code** (ici `scripts/lib/outils.sh` : le dossier s'il
est absolu, `.exe` sous Windows, puis le `PATH`) et **son test retire la réponse facile** : `scripts/checks/outils.sh`
vide le `PATH`, pour que le ffmpeg de la machine ne réponde pas à la place du dossier. Avant d'écrire la
recherche, mesurer ce que la suite trouve seule (la lancer sans le pré-contrôle). Lancer `bash scripts/checks/outils.sh`.

### L-122 — Un écart mesuré dans l'export d'un hôte n'accuse le plugin qu'après un témoin sans l'effet — 2026-10-26
Les essais manuels du plugin OpenFX (Lot 1 de `docs/PLAN-OFX.md`) ont donné deux fausses alarmes et un test qui
ne tranchait rien. Une colonne noire au bord gauche d'une timeline de 1921 : Resolve exporte en 1922 et l'ajoute
lui-même, témoin compris. Des exports CUDA et OpenCL qui ne sont pas identiques : Resolve diffère déjà d'un mode
à l'autre sans l'effet. Et un export *crop* identique à l'octet près à l'export *fit* : avec une source de même
hauteur que la timeline, les deux réglages donnent le même cadre, et le test ne pouvait rien montrer. Chaque fois,
c'est l'export du témoin, pas un raisonnement, qui a tranché ; le premier témoin, fait dans une autre timeline
que l'export avec l'effet, ne tranchait pas non plus.
→ **Pour accuser le plugin, exporter la même chose sans l'effet, dans les mêmes conditions** (timeline, réglages,
codec), et comparer l'écart avec l'effet à l'écart du témoin. Exporter en DNxHR, pas en H.264, qui noie un écart de
quelques LSB dans 52 dB. Regarder **où** est l'écart : uniforme, c'est la compression ou l'hôte ; concentré aux
bords, là où le warp agit, c'est la géométrie. Et avant l'essai, vérifier que le réglage essayé change le cadre
reçu par le plugin : une source plus grande que la timeline pour *crop*.

### L-123 — Une ligne de commande qui contient un chemin aléatoire ne se teste pas par sous-chaîne — 2026-10-26
Le test `a_driver_that_refuses_both_modes_leaves_the_cpu_encoder_a_bitrate` vérifiait que l'encodage CPU ne passe
pas `-qp` par `!ligne.contains("-qp")`. La ligne contient le dossier de session, au nom aléatoire : un jour il
s'est appelé `superview-qpPNs1`, et la CI Linux a échoué sur une PR de documentation. Environ une fois sur 3 800,
sur Unix seulement : un tel test passe des mois avant de rougir sans rapport avec la PR.
→ **Tester un drapeau comme argument entier** (`split(' ').any(|a| a == flag)`), jamais comme sous-chaîne d'une
ligne qui porte un chemin, un nom de fichier temporaire ou toute autre partie aléatoire. Pour trouver les autres :
`grep -rnE '!\s*[a-z_\[\]0-9.()&]*\.contains\("-' crates --include=*.rs` — passé le 2026-10-26 : seuls restent les
tests d'`encode/args/tests.rs`, dont les chemins sont fixes.

## 3. Corrections appliquées

> Déplacé, texte inchangé, dans [CORRECTIONS.md](CORRECTIONS.md). **Les nouvelles entrées s'ajoutent là-bas.**

## 4. Journal des révisions de ce document

| Date | Modification |
| --- | --- |
| 2026-09-04 | Création. Gabarit, 9 leçons permanentes issues de l'analyse initiale, file d'attente de 30 constats en 9 paliers. Aucune correction de code encore appliquée. |
| 2026-09-04 | Passe d'application par priorité : 28 constats traités et vérifiés en exécution, 1 invalidé (B-03), 5 en attente d'arbitrage, 1 reporté (C-05). Leçons L-10 à L-19 ajoutées. |
| 2026-09-04 | Arbitrages utilisateur appliqués (S-02, C-01, C-03, C-04). Leçons L-20, L-21. Restent C-05 (reporté) et C-06 (non tranché). |
| 2026-09-04 | 2ᵉ passe : C-05 et C-06 traités, plus O-05 et O-06 découverts en chemin. Leçons L-22 à L-24. **File d'attente vide.** |
| 2026-09-04 | 3ᵉ passe, empirique : 10 constats N-01 à N-10, aucune correction appliquée. Leçons L-25 à L-28. |
| 2026-09-04 | N-02 et N-01 corrigés et vérifiés. Dérive de doc du README rattrapée (L-29). N-03 à N-10 restent ouverts. |
| 2026-09-04 | **4ᵉ passe, analytique, à `001d250`** : 10 constats P-01 à P-10, dont 4 nouveaux défauts fonctionnels (P-01 à P-04) que la CI et les tests existants ne pouvaient pas voir. Correctif N-03/N-04/N-05 et passage PGM P5 vérifiés hors dépôt. Leçons L-30 à L-35. Paliers 9 et 10 ouverts. **Aucune correction de code appliquée.** |
| 2026-09-04 | Correctifs appliqués : N-03+N-04+N-05 et P-08, sur demande de l'utilisateur. **P-11 découvert et corrigé en chemin** (`libx265` inutilisable au-delà de 16 cœurs). 7 tests ajoutés, tous vérifiés en contre-épreuve. Leçons L-36 à L-39. Restent ouverts : N-06 à N-10, P-01 à P-07, P-09, P-10. |
| 2026-09-04 | Second lot : P-01 à P-07, P-09 et N-06, N-08, N-09, N-10. Deux tests réécrits parce qu'ils passaient à vide (L-40, L-41). Leçons L-40 à L-43. **File d'attente réduite à P-10 et N-07.** |
| 2026-09-04 | **P-10** : `appState`, 11 méthodes couvertes à 100 %, 18 sous-tests. `beginEncoding()` rend P-02 inexprimable (L-44). Découvert en chemin : le sélecteur de codec restait actif quand ffmpeg manque. Leçons L-44, L-45. **File d'attente vide ; reste N-07, arbitrage produit.** |
| 2026-09-04 | PR #28 fusionnée (`ab8b8a8`). **N-07 révisé** : la mesure d'origine sous-estimait le gain d'un facteur deux ; l'arbitrage s'inverse, recommandation « conserver », aucun code touché. Leçon L-46. **Plus aucun constat ouvert.** |
| 2026-09-04 | PR #29 fusionnée (`50bd9e9`). Cohérence du document : quatre titres contredisaient le tableau d'avancement ; restylés, et les deux conventions de marquage enfin écrites. Leçon L-47. |
| 2026-09-04 | v0.2.0 publiée. PR #30, #31, #32 fusionnées (docs, workflow de release, CI). **P-12 et P-13** trouvés en explorant ce qui du mode squeeze était vérifiable sans fichier GoPro : couture de 1 à 2,6 px au centre, et libellé promettant du GoPro que l'amont dément. Leçon L-48. |
| 2026-09-04 | Ouverture du palier 11 et de la convention **Q-xx** pour les questions ouvertes, distinctes des constats. Première entrée : Q-01 (le facteur 1,6). Aucun code touché. |
| 2026-09-04 | **5ᵉ passe, vérification d'après-release** : les checksums de v0.2.0 vérifiés sur les assets publiés (bons), le rouge Windows annoncé par #32 infirmé sur pièces, et **R-01** corrigé — le faux ffmpeg sautait sur Windows et emportait le test qui épingle P-03. Leçons L-49, L-50. **v0.2.1 publiée**, avec le premier passage réel de l'étape des checksums corrigée : noms nus, `sha256sum -c` vert sur les assets publics, sans retouche. PR #34 fusionnée. |
| 2026-09-04 | **Q-01 tranchée par la mesure** : l'intention déclarée du facteur de débit est tenue à k ≈ 1,19–1,30, `hevc_nvenc` et `libx265` d'accord au millième. Les deux profils passent à 4/3 (arbitrage utilisateur). **R-03** trouvé en chemin : reliquat de P-05 dans le journal. Leçons L-51, L-52. |
| 2026-09-05 | **R-02** corrigé : le message du tag annoté devient le corps de la release, avec repli bruyant sur un tag léger. Script extrait du YAML et essayé en local sur trois tags, ce qui a démasqué un `::warning::` qui serait parti dans les notes publiées. Leçon L-53. |
| 2026-09-05 | **v0.2.2 publiée** : la baisse de débit de Q-01 atteint les utilisateurs (Balanced ~20 % plus léger, Fast qui cesse de dégrader). Première release dont le corps décrit ce qui change, généré depuis le tag sans retouche. |
| 2026-09-05 | **R-04** : le binaire dit enfin quelle version il est — titre, journal, et première ligne du rapport Diagnostic. Version issue du tag via `fyne package --app-version`, jamais d'un fichier ; repli `dev` quand rien n'a estampillé le binaire, pour ne pas laisser passer le `0.0.1` par défaut de Fyne. Vérifié bout en bout sur un paquet réel. Leçon L-54. |
| 2026-09-05 | **R-05** : premier essai manuel du workflow de release, rouge sur les deux builds. R-04 dérivait la version du ref en retirant un `v`, ce qui ne vaut que pour les tags `v*` — ni pour une branche, ni pour un tag `RC-*`, que `fyne` refuse tous deux. Version extraite en `x.y.z`, repli `0.0.0` sur un essai à blanc. Leçon L-55. |
| 2026-09-05 | **Release en un clic** : le bouton *Run workflow* prend une version, teste, construit, tague et publie. Les notes vivent dans `RELEASE_NOTES.md`, relu en PR, et deviennent le message du tag annoté — le tag reste donc la source des notes (#37). Le tag n'est posé qu'après les deux builds, pour qu'un échec Windows ne brûle pas un numéro. Trois garde-fous refusent de démarrer : version hors `x.y.z`, tag déjà existant, notes qui ne mentionnent pas la version demandée. Script de planification essayé hors YAML sur huit cas (L-53). `RELEASING.md` créé. |
| 2026-09-05 | **v0.2.3 publiée**, première release en un clic : le bouton *Run workflow* a testé, construit, tagué depuis `RELEASE_NOTES.md` et publié. Vérification menée jusqu'à l'exécution du binaire téléchargé — `sha256sum -c` vert, notes conformes, version et commit exacts. **R-06 découvert là** : le `, modified` que le binaire affiche vient de la réécriture de `FyneApp.toml` par `fyne package`, observation que #39 avait notée puis classée « harmless ». Leçon L-56. |
| 2026-09-05 | **R-06 corrigé, et son diagnostic de la veille rectifié** : la cause n'était pas la réécriture de `FyneApp.toml` mais les fichiers que `fyne package` crée puis efface, trouvés en échantillonnant `git status --porcelain` pendant le packaging — `fyne_metadata_init.go` sur les deux plateformes, plus `fyne.syso` et `superview.exe` sur Windows. Mécanisme démontré isolément sur un dépôt jetable. Garde-fou ajouté sur le binaire produit : c'est lui qui a révélé que le correctif ne valait d'abord que pour Linux. **R-07 trouvé en chemin** : l'essai à blanc échouait depuis toute branche au nom contenant une barre oblique. Vérifié en exécutant le binaire d'un essai à blanc complet — `build="0.0.0 (71e04a9)"`. Leçons L-57, L-58. |
| 2026-09-08 | **10ᵉ passe, signalement utilisateur : mémoire épuisée sur un portable de 8 Gio.** Le journal du poste et son `journalctl` établissent la chaîne complète — noyau, ffmpeg, `OOMPolicy` de systemd, SIGTERM. Mesures du pic de RSS à géométrie reproduite : ≈ 0,27 Gio par mégapixel de sortie en 8 bits, ≈ 0,43 en 10 bits, indépendamment de x264 ou x265. **U-09 corrigé** ; U-10, U-11, U-12 ouverts. Leçons L-84 à L-86. |
| 2026-09-08 | **U-10 corrigé** (arbitrage utilisateur : basculer et l'annoncer) : Superview prend l'encodeur matériel de l'autre famille de codec quand celle de la source n'en a aucun, dit ce que cela coûte, et replie sur le codec de la source si le matériel échoue. **U-13 ouvert** en chemin : la sonde encode du 256×256 et ne prouve rien sur une image de 15 Mpx. Leçon L-87. |
| 2026-09-08 | **U-11 corrigé** : garde-fou mémoire avant encodage, chiffré au banc (290 o/px en 8 bits, 455 en 10 bits), refus sous le besoin et avertissement jusqu'à un quart au-dessus. Abstention sur le matériel, non mesuré, et quand `/proc/meminfo` n'existe pas. Leçons L-88, L-89. |
| 2026-09-08 | **U-12 corrigé**, et la 10ᵉ passe close côté corrections : `N/A` n'est plus un avertissement, le seuil disque descend de 10 Go à 1 Go, la journalisation en double disparaît avec la sémantique d'ajout qui la causait, et le commentaire de `EncodeVideo` retrouve sa fonction. Reste **U-13** ouvert : sonder à la taille réellement encodée. Leçons L-90 à L-92. |
| 2026-09-09 | **U-13 et U-14 corrigés**, sur retour d'essai de l'utilisateur : le pilote du HD 620 refuse `5120x2880` et donne ses bornes, ce que la sonde de 256×256 ne pouvait pas voir. Sonde à la géométrie réelle (mesurée gratuite : ~0,30 s quelle que soit la taille), recul propre vers l'encodeur logiciel de la famille source, fenêtre qui cesse de promettre, et garde-fou mémoire qui suit l'encodeur jusqu'au repli. Leçons L-93 à L-95. **Plus aucun constat ouvert.** |
| 2026-10-14 | **M-05 corrigé** : la CI résolvait `1.26.x` sur le Go en cache des runners (1.26.8) au lieu du correctif 1.26.9, et `Vulnerability Scan` bloquait les trois PR Dependabot ouvertes. `check-latest: true` sur les neuf étapes `setup-go`. Leçon L-99. |
| 2026-10-14 | **M-06 corrigé** : les branches fusionnées par `merge-on-label.yml` restaient, parce que le réglage du dépôt ne s'applique pas à ce que fait `GITHUB_TOKEN`. Le workflow les supprime lui-même, sauf empilement. Leçon L-100. |
| 2026-10-14 | **E-03, E-14, E-15, E-20 corrigés, E-23 en partie** (PR 2 du plan) : `appState.shutdown` attend la goroutine d'encodage depuis les trois sorties ; `completeEncoding` distingue l'annulation ; *Diagnostic* verrouillé ; encodeur capturé sur le fil UI et préférence préservée ; zenity 255 = erreur ; sélecteurs natifs hors du fil UI ; closures de `main()` repliées ; `fyneDo = true`. Leçon L-109, L-102 corrigée (le chemin signal de Fyne ne passe pas par `SetOnClosed`). |
| 2026-10-14 | **E-26, E-27 corrigés** (#88) : le gate requis, sauté dès qu'un test échouait, comptait comme réussi ; quatre défauts Windows qu'il cachait. Leçons L-111, L-112. |
| 2026-10-14 | **Q-02 mesurée, Q-03 tranchée** (#90) : garde-fou mémoire à +4 % par fil au-delà de 4 (DEC-37) ; pas de télémétrie GoPro (DEC-38). Leçon L-113. |
| 2026-10-14 | **Essai à blanc de la release** après le déplacement vers `cmd/` (#92) : Windows vert ; Linux rouge à l'étape ajoutée par #82 (pas de ffmpeg dans le `PATH`), sans lien avec le déplacement. Leçon L-114. |
| 2026-10-14 | **Docs alignées sur le gabarit** (DEC-39, DEC-40) : le § 3 devient [CORRECTIONS.md](CORRECTIONS.md) (texte inchangé, les nouvelles entrées vont là-bas) ; index [PIEGES.md](PIEGES.md) ; chemins `internal/common/` et `cmd/superview/`. |
| 2026-10-21 | **Docs remises d'aplomb après la 14ᵉ passe** : L-115 pointe le garde Rust (`repo_guards.rs`) et non plus le test Go retiré ; leçon L-117 (environnement injecté, que `AGENTS.md` citait sous un mauvais numéro). |

# Superview — Journal des corrections et leçons

> Dernière mise à jour : 2026-09-04
> **Ce fichier se met à jour après CHAQUE correction appliquée au code.**
> Procédure : (1) ajouter une entrée en § 3 avec le gabarit ci-dessous, (2) cocher la ligne
> correspondante dans la file d'attente § 4, (3) si la correction révèle une règle réutilisable,
> l'ajouter en § 2.

Les identifiants `Q-xx` renvoient aux **questions ouvertes** de
[ANALYSE_PROJET.md § 5bis](ANALYSE_PROJET.md) : ce ne sont pas des constats, mais des points dont
on ignore encore s'ils constituent un défaut.

Les identifiants `B-xx`, `S-xx`, `C-xx`, `X-xx`, `O-xx`, `T-xx` renvoient à
[ANALYSE_PROJET.md § 3](ANALYSE_PROJET.md) (1ʳᵉ et 2ᵉ passes), `N-xx` à
[§ 3bis](ANALYSE_PROJET.md) (3ᵉ passe, empirique) et `P-xx` à
[§ 3ter](ANALYSE_PROJET.md) (4ᵉ passe).

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
Ne jamais écrire « corrigé » quand on veut dire « écrit mais non compilé ». Toute livraison
non vérifiée doit être annoncée explicitement comme telle à l'utilisateur.
*Mise à jour 2026-09-04* : Go 1.26.8 est désormais installé dans `~/.local/go` et y survit aux
redémarrages. Le **sysroot GUI vit dans `/tmp` et n'y survit pas** : après un redémarrage,
`./common` se vérifie normalement mais le paquet `main` ne compile plus tant que le sysroot
n'est pas reconstruit. Procédure : [SOURCES.md § 1](SOURCES.md).

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
Procédure complète : [SOURCES.md § 1](SOURCES.md).

### L-29 — Une refactorisation de signature doit balayer la documentation — 2026-09-04
La refactorisation C-05 a changé les signatures de `CheckFfmpeg` et `PerformEncoding`. J'ai mis à
jour le code, les tests et `SOURCES.md`, mais pas le bloc d'exemple du README, resté avec les
anciennes signatures pendant deux PR. Aucun outil ne le détecte : les extraits de code en
Markdown ne sont pas compilés.
→ Après tout changement de signature exportée : `grep -rn "NomFonction" --include='*.md' .`
Constat relevé pendant la correction de N-01.

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

### L-51 — Un renommage « partout » se vérifie par `grep`, pas par relecture — 2026-09-04
P-05 devait remplacer « bytes/second » par « bits/second » dans tout le projet. Une occurrence
avait survécu — `slog.Int("bitrate_bytes_sec", bitrate)` — dans le journal de fin d'encodage,
c'est-à-dire dans le **seul** journal que le README demande de joindre aux rapports de bug. Un
facteur 8 annoncé à qui viendrait diagnostiquer.
→ Elle n'a pas été trouvée par relecture mais en regardant défiler la sortie d'un encodage lancé
pour autre chose. Un correctif dont l'énoncé contient « partout » se solde par un `grep` de
l'ancien terme avant de le déclarer fait ; ça prend cinq secondes et c'est la seule vérification
qui corresponde à ce qui est promis. Même famille que L-47 : ce qui n'est pas vérifié dérive.

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

### [2026-09-04] P-12, P-13 — Couture au centre en mode squeeze, et libellé trompeur

| | |
| --- | --- |
| **Constats** | P-12, P-13 ([§ 3ter](ANALYSE_PROJET.md)) |
| **Fichiers** | `common/common.go` · `common/pgm_golden_test.go` · `gui_main.go` · `gui_main_test.go` · `README.md` |
| **Commit** | non commité |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · GUI démarrée ✅ · contre-épreuve ✅ |

**Contexte** — l'utilisateur n'a pas de fichier GoPro en mode squeeze sous la main. Plutôt que
d'attendre, j'ai cherché ce qui restait vérifiable sans : la santé géométrique de la carte de
remappage, que le test doré ne couvre pas.

**P-12, symptôme** — une couture verticale de 1 à 2,6 px au milieu de l'image, en mode squeeze
uniquement, d'amplitude erratique selon la résolution.

**P-12, cause racine** — les deux termes du décalage squeeze valent tous deux `7/32 × outX × inv`
au centre : la courbe est **conçue** pour y passer par zéro. Mais `outX/16` et `outX/7` étaient
des divisions entières, dont la troncature laissait un résidu que le miroir de la moitié gauche
transformait en saut. Preuve directe : sur une largeur multiple de 112 (16×7), aucune troncature
n'a lieu et la couture disparaît entièrement.

**P-12, correctif** — diviser en flottant, en conservant la forme de l'expression pour rester
lisible face à l'implémentation de référence (contrainte inscrite dans `.golangci.yml`).
Amplitude maximale inchangée à 0,1 % près : raffinement sous-pixel, pas changement d'intention.

**Une fausse piste, corrigée en chemin** — j'avais d'abord annoncé une compression de 2,09× au
centre. C'était une erreur de mesure : ma différence centrée enjambait l'axe de symétrie et
intégrait la discontinuité. La pente réelle y est de 1,43. L'erreur a néanmoins mis le défaut au
jour.

**P-13** — la case s'intitulait « Source already stretched (GoPro SuperView) », alors que le
README amont documente l'option pour des caméras comme la Caddx Tarsier et déclare l'algorithme
« not a 1-1 copy of the GoPro algorithm ». Renommée « Source already stretched to 16:9
(un-squeeze) », README aligné sur le périmètre réel.

**Ce que je n'ai pas pu trancher** — l'implémentation Python de Banelle est inaccessible (403,
archive web refusée), donc j'ignore si la couture y existait déjà ou si elle est née au portage
Python → Go. Sans importance pour la décision : l'intention est démontrable depuis la formule.

**Leçon** — L-48.

---


### [2026-09-04] Cohérence du document — quatre titres contredisaient le tableau d'avancement

| | |
| --- | --- |
| **Constat** | aucun ; dérive de documentation relevée en clôture |
| **Fichiers** | `output_claude/ANALYSE_PROJET.md` · `output_claude/LESSONS.md` — **aucun code** |
| **Commit** | non commité |
| **Vérification** | audit systématique des 22 titres N-xx/P-xx contre le § 4bis, plus les cases de la file d'attente |

**Symptôme** — `### N-03 🟠 — Les sources 10 bits sont ramenées à 8 bits`, alors que le § 4bis
compte N-03 parmi les corrigés et que le corps du constat porte « ✅ correctif vérifié ». Idem
N-04, N-05 et P-11. Un lecteur qui parcourt les titres conclut que trois pertes de contenu sont
toujours actives.

**Cause racine** — le document porte **deux conventions de marquage**, aucune écrite. Le § 3
conserve les sévérités d'origine parce que c'est une analyse datée et non un tableau de bord ;
les § 3bis et 3ter barrent le titre et le passent en ✅ parce qu'on les ouvre pour connaître
l'état courant. Les deux se défendent, mais rien ne les énonçait, donc rien ne rappelait de
restyler en corrigeant.

**Correctif** — les quatre titres sont restylés, et **les deux conventions sont écrites en tête
du § 3**, avec la mention explicite que le § 4bis fait foi. C'est la seconde partie qui compte :
corriger les quatre titres sans écrire la règle laissait la dérive se reproduire au constat
suivant.

Deux formulations périmées corrigées au passage : l'intro du § 3ter décrivait encore N-07 comme
en attente avec l'ancien chiffre de 5 %, et un titre de palier annonçait des constats « encore
ouverts » alors que toutes ses cases sont cochées. Les entrées du **journal des révisions** sont
laissées telles quelles : elles disent ce qui était vrai à leur date, c'est leur rôle.

**Leçon** — L-47.

---


### [2026-09-04] N-07 — Mesure révisée, arbitrage inversé (aucun changement de code)

| | |
| --- | --- |
| **Constat** | N-07 ([§ 3bis](ANALYSE_PROJET.md)) |
| **Fichiers** | `output_claude/ANALYSE_PROJET.md` · `output_claude/SOURCES.md` — **aucun fichier de code** |
| **Commit** | non commité |
| **Vérification** | mesures refaites sur RTX 4070, code actuel, deux exécutions concordantes par cas |

**Symptôme** — le constat annonçait « le décodage matériel n'apporte que ~5 %, pour un risque
réel », et j'ai relayé cette conclusion à l'utilisateur comme un fait. Il a demandé une
explication, ce qui m'a conduit à refaire la mesure.

**Cause racine** — la mesure d'origine portait sur une mire `testsrc` à ~2 Mbps. Un tel flux se
décode quasi gratuitement : il n'y a presque rien à déporter sur le GPU, donc presque rien à
gagner. Une GoPro 5,3K produit du 100-120 Mbps de contenu détaillé.

**Mesures** — même matériel, code actuel, meilleur de deux exécutions :

| Source | Décodage matériel | Encodage matériel |
| --- | --- | --- |
| Mire `testsrc`, 2 Mbps *(échantillon d'origine)* | +3,1 % | ×1,92 |
| Type GoPro, 2880×2160 à 127 Mbps | **+9,9 %** | **×3,18** |

Le mécanisme a aussi été isolé, en transcodage sans `remap` sur la source de 127 Mbps :
trames gardées en VRAM 2,34 s (+7 %), trames rapatriées en RAM — ce que fait Superview —
2,47 s (+1 %), décodage CPU 2,51 s. Le rapatriement annule donc le gain *en transcodage simple*,
alors qu'avec `remap` le même décodage matériel rapporte +9,9 %. L'explication cohérente avec ces
deux mesures est que le gain vient du **CPU libéré pour le filtre**, pas d'un décodage plus
rapide — inférence, non vérifiée directement.

**Conclusion** — l'arbitrage s'inverse : à ~10 % pour un risque essentiellement limité à
l'initialisation, conserver le décodage matériel est le meilleur choix. **Aucune modification de
code.** Décision finale à l'utilisateur.

**Leçon** — L-46.

---


### [2026-09-04] P-10 — L'état de `main()` extrait dans `appState`

| | |
| --- | --- |
| **Constat** | P-10 ([§ 3ter](ANALYSE_PROJET.md)) |
| **Fichiers** | `gui_main.go` · `gui_main_test.go` |
| **Commit** | non commité |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · GUI démarrée ✅ · contre-épreuve sur les trois propriétés clés ✅ |

**Symptôme** — aucun test ne pouvait atteindre l'état de la GUI. Les 14 tests du paquet `main`
ne touchaient que des fonctions pures, et c'est exactement pour cela que P-01 et P-02 avaient
survécu à T-01 : ils vivaient dans les ~600 lignes de `main()` où `video`, `outputPath`,
`encodingInProgress` et `cancelEncoding` étaient des variables capturées par closure.

**Cause racine** — l'état et les décisions qui le lisent étaient au même endroit que la
construction des widgets, dans une seule fonction. Rien n'était nommé, donc rien n'était
appelable.

**Correctif** — un type `appState` porte les six éléments d'état et les widgets qu'ils pilotent,
avec onze méthodes : `canStart`, `refreshStart`, `setInput`, `setOutput`, `setFFmpeg`,
`refreshHardwareStatus`, `setEncoding`, `beginEncoding`, `finishEncoding`, `requestCancel`,
`isEncoding`. Toutes à **100 % de couverture**.

Deux décisions vont plus loin qu'un simple déplacement :

- `beginEncoding()` **retourne** le canal d'annulation. La goroutine ne peut plus le relire dans
  la structure : P-02 devient inexprimable plutôt que corrigé (L-44).
- `requestCancel()` est le seul endroit qui ferme le canal et le met à `nil` dans le même geste ;
  l'appeler deux fois est inoffensif au lieu de paniquer. Testé explicitement.

Trouvé en chemin : le chemin « ffmpeg indisponible » désactivait quatre contrôles à la main et
**oubliait le sélecteur de codec**, créé plus loin dans `main()` — il restait actif parmi quatre
contrôles grisés. C'était la dernière séquence d'`Enable`/`Disable` écrite à la main du fichier ;
elle passe maintenant par la même liste que la transition d'encodage.

**Laissé de côté délibérément** — `main()` reste longue (524 lignes contre 600). Le défaut
n'était pas la longueur mais l'inaccessibilité des décisions ; la construction de widgets et la
mise en page n'ont rien à gagner à être découpées (L-45).

| | Avant | Après |
| --- | --- | --- |
| État applicatif | 6 variables en closure | 1 type, 11 méthodes |
| Tests atteignant l'état | 0 | 18 sous-tests |
| Couverture du paquet `main` | 12,6 % | **21,8 %** |
| Couverture du module | 56,6 % | **59,3 %** |

**Leçons** — L-44, L-45.

---


### [2026-09-04] Second lot — P-01 à P-07, P-09, N-06, N-08, N-09, N-10

| | |
| --- | --- |
| **Constats** | P-01 à P-07, P-09 ([§ 3ter](ANALYSE_PROJET.md)) · N-06, N-08, N-09, N-10 ([§ 3bis](ANALYSE_PROJET.md)) |
| **Fichiers** | `common/common.go` · `common/metrics.go` · `common/observability.go` · `common/config.go` · `gui_main.go` · `superview.yaml` · `.github/copilot-instructions.md` · 5 fichiers de test |
| **Commit** | non commité |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · GUI démarrée ✅ · contre-épreuve sur chaque correctif ✅ |

**P-01 — la case squeeze restait grisée.** Les deux branches de fin d'encodage réactivaient les
widgets une par une et avaient divergé. Remplacées par un type `encodingControls` porteur d'un
`setEnabled(bool)` : une seule liste, parcourue dans les deux sens. C'est aussi ce qui rend la
transition testable, ce qu'elle n'était pas — les 14 tests du paquet `main` ne touchaient que des
fonctions pures, et c'est exactement pour cela que P-01 avait survécu à T-01 (L-35).

**P-02 — course sur le canal d'annulation.** Le canal est capturé dans une locale avant le
lancement de la goroutine, au lieu d'être relu depuis la variable partagée que le fil UI met à
`nil`.

**P-03 — l'annulation relançait la cascade.** `ErrCancelled` remplace `errors.New("encoding
interrupted by user")`, et les trois points de repli d'`EncodeVideo` la testent avec
`errors.Is`. Mesuré par `TestEncodeVideo_CancellationStopsTheFallbackCascade` : 1 lancement de
ffmpeg au lieu de 3.

**P-04 — fichier de sortie partiel.** L'encodage écrit dans un fichier de travail créé par
`os.CreateTemp` dans le répertoire de destination, renommé en place seulement en cas de succès.
Le renommage est atomique (même système de fichiers) et un échec ne détruit plus le fichier
existant. **N-09 devenait obligatoire** dans le même mouvement : ffmpeg ne voyant plus de
conflit entrée/sortie, plus rien n'empêchait le renommage final d'écraser la source.

**P-05 — unités.** « bytes/second » → « bits/second » dans le code, le YAML, les tests, la
documentation d'agent et les messages d'erreur montrés à l'utilisateur. Aucun changement de
comportement : les valeurs étaient déjà des bits.

**P-06 — vitesse d'encodage inventée.** `r_frame_rate` est lu auprès de ffprobe et parsé
(`parseFrameRate`, qui gère les cadences NTSC rationnelles). Quand la cadence est inconnue, la
métrique reste à zéro plutôt que d'être calculée sur un 30 fps supposé (L-33).

**P-07 + N-10 — retour utilisateur.** `ProgressDetailHandler`, interface optionnelle détectée
par assertion de type, transporte le temps restant que `metrics.go` calculait déjà sans que
personne ne le lise. Le statut affiche « Transforming... 42% - about 1m20s left ». Le chemin du
fichier de journal apparaît dans le dialogue Diagnostic.

**P-09 — partiellement.** `RecordEvent` prend un `RLock` ; `-threads` n'est plus émis pour les
encodeurs matériels. Le court-circuit d'allocation sur les événements de progression a été
**délibérément écarté** : le coût est négligeable devant un encodage vidéo et l'optimisation
risquait de faire disparaître des événements du journal.

**N-06 — débit non borné.** `-maxrate` et `-bufsize` ajoutés, calibrés par la mesure et non par
la recommandation d'usage (L-42).

**N-08 — espace disque.** `checkTempSpaceForMaps` s'exécute avant `InitEncodingSession` avec le
besoin **réel** (`remapMapBytes`, + 20 % de marge) et non un seuil fixe de 10 Go. Le message
mentionne que le répertoire temporaire est souvent en RAM et suggère `TMPDIR`. Une sonde qui
échoue ne bloque pas l'encodage.

**Leçons** — L-40 à L-43.

---


### [2026-09-04] N-03 + N-04 + N-05 — Chaîne de filtres : profondeur, pistes audio, métadonnées

| | |
| --- | --- |
| **Constat** | N-03, N-04, N-05 ([ANALYSE_PROJET.md § 3bis](ANALYSE_PROJET.md)) |
| **Fichiers** | `common/common.go` (`VideoStream`, `CheckVideo`, `buildEncodeBaseArgs`, + 4 helpers) · `common/common_test.go` · `common/integration_test.go` |
| **Commit** | non commité |
| **Vérification** | `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · contre-épreuve ✅ |

**Symptôme** — trois pertes silencieuses sur un encodage réussi, toutes mesurées : une source
`yuv420p10le` ressortait en `yuv420p` ; deux pistes audio en entrée donnaient une seule piste en
sortie ; `creation_time` disparaissait du fichier produit.

**Cause racine** — une seule ligne. `-filter_complex` figeait le format de sortie à
`format=yuv444p,format=yuv420p`, et aucun `-map` n'accompagnait les **trois** entrées du graphe
(vidéo + carte X + carte Y). Sans `-map`, FFmpeg applique sa sélection automatique et ne retient
qu'un flux par type ; et avec trois entrées il ne sait de laquelle hériter les métadonnées
globales, donc il n'en hérite d'aucune.

**Correctif** — la chaîne devient
`remap,format=yuv444p10le,format=yuv420p10le[v]` suivie de `-map "[v]" -map "0:a?"
-map_metadata 0`. Le `?` est ce qui empêche une source muette d'échouer.

Les 10 bits ne sont conservés que si **la source les porte et l'encodeur est de la famille
HEVC** : `h264_nvenc` ne sait pas encoder en 10 bits du tout, et le profil High 10 de `libx264`
se lit mal sur du matériel grand public. Les modes 10 bits des GoPro enregistrent en HEVC, donc
le cas qui compte est couvert sans risquer de transformer une conversion qui marchait en échec.
`pix_fmt` est désormais demandé à ffprobe ; absent, il est lu comme du 8 bits — le comportement
antérieur.

Laissé de côté délibérément : les sources au-delà de 10 bits sont ramenées à 10, rien dans ce
projet ne vise Main12.

**Leçon** — L-38.

---

### [2026-09-04] P-08 — Cartes de remappage en PGM P5 binaire

| | |
| --- | --- |
| **Constat** | P-08 ([ANALYSE_PROJET.md § 3ter](ANALYSE_PROJET.md)) |
| **Fichiers** | `common/common.go` (`GeneratePGM`, `putMapSample`) · `common/pgm_golden_test.go` |
| **Commit** | non commité |
| **Vérification** | `go test -race ./...` ✅ · équivalence prouvée sur la sortie décodée ✅ · contre-épreuve boutisme ✅ |

**Symptôme** — les deux cartes de remappage pèsent jusqu'à **146,6 Mo** pour une source GoPro
4064×3048. `/tmp` étant un tmpfs sur la plupart des distributions récentes, ce sont des
mégaoctets de RAM.

**Cause racine** — le format PGM P2 écrit chaque coordonnée en décimal ASCII suivi d'un
séparateur, soit ~4,4 octets par échantillon là où la valeur en fait 2.

**Correctif** — passage au PGM P5, la variante binaire, en 16 bits gros-boutistes (l'ordre est
imposé par la spécification). **146,6 Mo → 66,1 Mo**, et la génération ne formate plus 33
millions d'entiers. `putMapSample` est l'unique endroit qui décide de l'ordre des octets ; la
carte Y y passe aussi, précisément parce que la première version l'inlinait et rendait le test
d'ordre inopérant (L-37). Le plafonnement dans `putMapSample` est un garde-fou : la
distorsion atterrit toujours dans `[0, largeur-1]`, vérifié sur sept géométries.

**Preuve d'équivalence** — les empreintes SHA-256 des trames décodées par le vrai filtre
`remap` sont **identiques** avant et après, sur quatre cas (squeeze et non-squeeze, deux
géométries). Les empreintes figées de `TestGeneratePGM_Golden` ne pouvaient pas certifier ce
changement, qui réécrit tous les octets par construction : c'est
`TestGeneratePGM_RemapOutputIsStable` qui le fait, et il est auto-référentiel pour ne pas
dépendre du build de FFmpeg (L-36).

**Leçon** — L-36, L-37.

---

### [2026-09-04] P-11 — `libx265` échouait au-delà de 16 cœurs

| | |
| --- | --- |
| **Constat** | P-11 ([ANALYSE_PROJET.md § 3ter](ANALYSE_PROJET.md)) — découvert en écrivant le test de N-03 |
| **Fichiers** | `common/common.go` (`clampEncoderThreads`, `x265MaxFrameThreads`) · `common/common_test.go` |
| **Commit** | non commité |
| **Vérification** | reproduit à 17/24/32 threads ✅ · `go test -race ./...` ✅ |

**Symptôme** — sur cette machine (24 cœurs logiques), **aucun encodage H.265 ne pouvait
aboutir**. `x265 [error]: frameNumThreads (--frame-threads) must be
[0 .. X265_MAX_FRAME_THREADS)`, puis `Cannot open libx265 encoder`.

**Cause racine** — `encoderThreads` vaut `runtime.NumCPU()` par défaut ; FFmpeg fait
correspondre `-threads` à `--frame-threads` de x265, plafonné à 16. Mesuré : 4, 8, 16 passent,
17 et au-delà échouent. Aggravant : `libx265` étant un encodeur CPU, `isHardwareEncoder` est
faux et la cascade de repli d'`EncodeVideo` ne se déclenche pas — l'échec est terminal.

**Correctif** — `clampEncoderThreads` ramène `-threads` à 16 pour `libx265` uniquement. Les
autres encodeurs se limitent eux-mêmes plutôt que d'échouer, on ne les touche pas.

**Pourquoi personne ne l'avait vu** — il faut plus de 16 cœurs logiques pour le déclencher. Les
runners GitHub en ont 4, et les trois passes d'analyse précédentes n'avaient jamais exécuté
`libx265` sur cette machine.

**Leçon** — L-39.

---


<!-- Les entrées les plus récentes vont en tête de cette section, juste sous ce commentaire. -->

### [2026-09-04] Correction de N-02 et N-01

Deux constats de la 3ᵉ passe traités, sur décision de l'utilisateur. Le reste (N-03 à N-10)
demeure documenté et non traité.

#### N-02 — Processus zombie à chaque annulation

| | |
| --- | --- |
| **Fichiers** | `common/common.go`, `common/cancel_leak_test.go` (nouveau) |
| **Vérification** | garde de régression **prouvée dans les deux sens** ✅ |

La branche `<-cancel` tuait le processus puis retournait sans appeler `cmd.Wait()`. Le processus
n'était donc jamais moissonné, et la goroutine interne que `os/exec` crée pour vider stderr
jamais libérée. Mesuré avant correction : **3 annulations → 3 zombies**, persistant jusqu'à la
fermeture de l'application.

`TestEncodeVideo_CancelReapsProcess` compte les enfants à l'état `Z` en lisant directement
`/proc` — pas de dépendance à `ps`. Correctif retiré : le test échoue en signalant 3 zombies.
Correctif remis : il passe.

#### N-01 — Les sélecteurs proposaient dix formats pour un produit MP4 uniquement

Les trois sélecteurs (Fyne, zenity/kdialog, PowerShell) sont restreints au `.mp4`, ce qui aligne
l'entrée sur la sortie — déjà contrainte par `ensureMP4Extension`. Cela supprime le problème
plutôt que de le contourner : les 5 formats que `CheckVideo` rejetait ne sont plus proposés.

`supportedInputExtensions` centralise la liste, et deux tests la verrouillent, dont un qui
vérifie la **cohérence des deux bouts** : toute extension acceptée en entrée doit être laissée
intacte par `ensureMP4Extension`.

#### Dérive de documentation corrigée au passage

Le bloc « API Documentation » du README montrait encore `common.CheckFfmpeg()` et
`common.PerformEncoding("input.mp4", ...)` — les signatures d'avant la refactorisation C-05.
**J'avais introduit cette dérive dans la 2ᵉ passe sans la corriger.** Voir [[L-29]].


### [2026-09-04] 3ᵉ passe — analyse empirique, aucune correction encore appliquée

Dix constats N-01 à N-10 consignés dans [ANALYSE_PROJET.md § 3bis](ANALYSE_PROJET.md).
**Aucun code modifié** : cette passe est un diagnostic, les corrections restent à arbitrer.

Méthode : corpus de fichiers difficiles fabriqué avec FFmpeg, mesures de mémoire, de débit et de
durée, comptage de processus zombies, et validation du chemin matériel sur une **RTX 4070 avec
NVENC fonctionnel** — jamais testé jusqu'ici.

Quatre hypothèses de départ invalidées par la mesure (voir [[L-27]]), deux conclusions corrigées
en cours de route ([[L-25]], [[L-26]]).

Constats les plus solides, tous mesurés :
- **N-02** — 5 annulations produisent 5 zombies ; `cmd.Wait()` les ramène à 0 (prouvé dans les
  deux sens).
- **N-01** — 5 des 10 formats proposés par la GUI sont rejetés.
- **N-07** — l'encodage matériel vaut ×1,7 ; le décodage matériel seulement ~5 %, alors qu'il est
  tenté en premier et qu'un échec tardif coûte un encodage entier.
- **N-03/N-04/N-05** — 10 bits ramenés à 8, pistes audio surnuméraires supprimées, date de prise
  de vue perdue.


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

### Palier 9 — 3ᵉ passe : constats N-xx

- [x] **N-01** Restreindre les trois sélecteurs de fichiers au seul `.mp4`
- [x] **N-02** Moissonner le processus FFmpeg tué (`cmd.Wait()`) sur la branche d'annulation
- [x] **N-03 + N-04 + N-05** Correctif unique du `-filter_complex` : profondeur 10 bits,
      `-map "[v]" -map "0:a?"`, `-map_metadata 0`, `pix_fmt` demandé à `CheckVideo`
      → appliqué le 2026-09-04, test d'intégration bout en bout
- [x] **N-06** Borner le débit : `-maxrate` et `-bufsize` à côté de `-b:v` → appliqué le 2026-09-04, plafond calibré par la mesure (L-42)
- [x] **N-08** Vérifier l'espace avec le besoin réel avant `InitEncodingSession` → appliqué le 2026-09-04 (`checkTempSpaceForMaps`)
- [x] **N-09** Comparer entrée et sortie avant lancement, message clair → appliqué le 2026-09-04, **rendu obligatoire par P-04**
- [x] **N-10** Afficher l'ETA et le chemin du fichier de journal → appliqué le 2026-09-04
- [x] **N-07** *(arbitrage produit)* Conserver ou non le décodage matériel
      → **mesure refaite le 2026-09-04 : ~10 %, pas ~5 %.** Recommandation : conserver,
      donc aucun changement de code (L-46)

### Palier 10 — 4ᵉ passe : constats P-xx

Ordre issu de [ANALYSE_PROJET.md § 3ter](ANALYSE_PROJET.md).

- [x] **P-01** Une seule liste de widgets, type `encodingControls` (L-30) → appliqué le 2026-09-04
- [x] **P-02** Capturer `cancelEncoding` dans une locale avant de lancer la goroutine → appliqué le 2026-09-04
- [x] **P-03** `ErrCancelled` testée avec `errors.Is`, cascade court-circuitée (L-31) → appliqué le 2026-09-04
- [x] **P-04** Encoder vers un fichier de travail puis renommer → appliqué le 2026-09-04
- [x] **P-05** « bytes/second » → « bits/second » partout (L-32) → appliqué le 2026-09-04
- [x] **P-08** Écrire les cartes de remappage en PGM P5 binaire 16 bits **gros-boutiste**
      (−55 % d'empreinte, sortie identique au bit près) — L-34 → appliqué le 2026-09-04
- [x] **P-11** Plafonner `-threads` à 16 pour `libx265` — L-39 → appliqué le 2026-09-04
      *(découvert en corrigeant N-03 : échec dur de tout encodage H.265 au-delà de 16 cœurs)*
- [x] **P-10** Extraire l'état de `main()` dans un type `appState` testable (L-35)
      → appliqué le 2026-09-04 : 11 méthodes, toutes à 100 % de couverture (L-44, L-45)
- [x] **P-06** Lire `r_frame_rate` plutôt que supposer 30 fps (L-33) → appliqué le 2026-09-04
- [x] **P-07** Exposer `EstimatedRemaining` dans la GUI (`ProgressDetailHandler`) → appliqué le 2026-09-04
- [x] **P-09** `RLock` dans `RecordEvent` · pas de `-threads` sur les encodeurs matériels
      → appliqué le 2026-09-04. Court-circuit des événements de progression **écarté
      volontairement** : coût négligeable, risque de perdre des événements du journal.


### Palier 11 — questions ouvertes (pas des constats)

À traiter en session dédiée. Une question n'est cochée que lorsqu'elle est **tranchée** — que la
réponse débouche sur un correctif ou sur « rien à changer, voici pourquoi ».

- [x] **Q-01** **Tranchée le 2026-09-04 par la mesure** : l'intention déclarée est tenue à
      k ≈ 1,19–1,30, le ratio géométrique 4/3 la couvre, le 1,6 dépassait la source de 1,66 dB
      pour 20 % de fichier en plus. Deux encodeurs d'accord au millième. Les deux profils
      passent à 4/3. Question d'origine ci-dessous.
- [ ] ~~**Q-01** D'où vient le facteur 1,6 du profil « Balanced » ?~~ Il vaut exactement le ratio
      géométrique 4/3 majoré de 20 %, marge que rien ne documente. Voir aussi le profil « Fast »,
      qui ne relève pas le débit malgré 4/3 de pixels en plus, et l'interaction avec N-06 qui a
      transformé cette cible en plafond. Détail en [ANALYSE_PROJET.md § 5bis](ANALYSE_PROJET.md).

### Palier 12 — 5ᵉ passe : vérification d'après-release

- [x] **R-01** Faux ffmpeg en `.bat` sur Windows, helper partagé avec
      `TestEncodeVideo_InterruptedByUser` (L-49, L-50) → appliqué le 2026-09-04
- [ ] **R-02** Faire du message du tag annoté le corps de la release, au lieu du gabarit vide
- [x] **R-03** `bitrate_bytes_sec` → `bitrate_bits_sec`, reliquat de P-05 (L-51)
      → appliqué le 2026-09-04
- [x] **v0.2.1** Taguée sur `90b02a8`, draft vérifié, **publiée** le 2026-09-04 →
      checksums générés corrects du premier coup, notes rédigées à la main parce que le
      workflow n'en produit aucune (R-02)

---

## 5. Journal des révisions de ce document

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

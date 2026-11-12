# Pièges par domaine

Règle courte de chaque leçon permanente de [lecons.md § 2](lecons.md), rangée par domaine ; son histoire,
sa cause et son contrôle sont là-bas, sous le numéro indiqué (`grep -n "### L-NN" docs/lecons.md`).
**Avant de toucher un domaine, lire sa section** ; les trois dernières (tests, documentation, méthode)
valent pour presque toute tâche.

Ce fichier n'est pas importé par `CLAUDE.md` : on le lit au moment de toucher un domaine (DEC-40).
**Un nouveau piège** : une leçon `L-NN` dans `lecons.md` § 2, puis sa ligne ici, dans la section de son
domaine. Les pièges valables pour tous les projets vont aussi dans `~/.claude/pieges/`.

## FFmpeg, encodage et sondes

- L'ordre des arguments change leur sens : avant `-i`, une option vise l'entrée qui suit ; après, la sortie. L-03
- `-re` n'a rien à faire dans un encodage vers un fichier : il bride la lecture au temps réel. L-14
- Le nombre de cœurs de la machine n'est pas une valeur valide pour un outil externe : la confronter à ses limites. L-39
- Calibrer un paramètre par la mesure, sur un contenu qui le sollicite (pas une mire). L-42
- Garder le codec de la source est une préférence ; le matériel disponible est une capacité, qui l'emporte. L-87
- Un garde-fou (mémoire) se chiffre au banc et ne s'étend pas au-delà de ce qui a été mesuré. L-88
- Une constante mesurée appartient à la configuration du banc : tout paramètre qui varie la rend fausse. L-113
- Deux encodages multi-fil du même clip ne donnent pas les mêmes images : fixer les fils pour comparer. L-116
- Une sonde d'une trame ne voit pas ce qui casse à la troisième : sonder ce que fait la conversion (DEC-67). L-120
- Une liste d'encodeurs décrit le binaire ffmpeg, pas la machine : la capacité réelle se sonde. L-71
- Un numéro de version n'est pas un contrat : chercher la propriété qui détermine le comportement. L-72
- Une sonde mal formée accuse ce qu'elle mesure : vérifier que la question pouvait recevoir un oui. L-73
- Une sonde répond à la question qu'elle pose ; une limite de la vérification se consigne comme constat. L-93
- Un garde-fou suit l'objet qu'il garde : vérifier au dernier point où le choix d'encodeur peut changer. L-94
- L'unité fait partie du contrat : les débits sont en **bits** par seconde, partout. L-32
- Une métrique dont une entrée est devinée nuit plus qu'une métrique absente. L-33
- Un seuil de santé (espace disque) se dérive de ce qu'il doit contenir, pas d'un chiffre rond. L-91
- La géométrie des cartes PGM a une source externe (le script de Banelle) : ses divisions entières en font partie. L-09
- Un changement de format (PGM) se valide sur la sortie décodée par ffmpeg, pas sur le fichier. L-34
- Une géométrie calculée à deux endroits finit par diverger : une seule fonction, partagée. L-43
- Deux argv censés poser la même question se construisent par les mêmes fonctions et se comparent en entier. L-103
- Un drapeau se teste comme argument entier, jamais comme sous-chaîne d'une ligne qui contient un chemin. L-123
- Déduire une propriété d'un nom (format de pixel) exige d'abord de lister les contre-exemples. L-38
- Prouver l'équivalence d'un parseur sur une vraie sortie de l'outil, ancien et nouveau côte à côte. L-17
- Séparer l'appel d'un processus du parsing de sa sortie : le parsing devient une fonction pure, testable. L-61

## Processus, annulation et erreurs

- Une annulation est une erreur typée (`Error::Cancelled`), jamais une chaîne. L-31
- Un test d'annulation interrompt un travail réellement commencé : annuler depuis le rappel de progression. L-40
- Observer depuis l'extérieur du processus qu'on va tuer. L-41
- Un état transitoire (processus vivant, fichier partiel) s'échantillonne pendant, pas après. L-57
- Un signal reçu n'est pas une action de l'utilisateur : le message nomme l'acteur réel. L-84
- Un processus tué de l'extérieur porte un diagnostic (la mémoire), pas seulement un échec. L-85
- Reporter un échec de l'ouverture vers la fin le rend destructeur : relire les chemins d'erreur au nouvel endroit. L-105
- Un signal de fin se libère avant tout appel qui peut mener à celui qui l'attend. L-110
- Traiter le cas d'erreur générique avant le cas particulier (une erreur de lecture ≠ fin de fichier). L-15
- Une erreur dédoublée pour distinguer deux causes se relit partout où la première était testée. L-101
- Un test ne modifie pas l'environnement du processus : la fonction le reçoit en paramètre. L-117
- Une fixture `.bat` : `cmd.exe` coupe un chemin à la virgule, et le nom du test finit dans le chemin. L-112

## Interface

- Aucun chemin relatif dans une application graphique : le dossier courant y est arbitraire. L-16
- Une fenêtre à taille fixe rogne sans prévenir : ajouter un widget est un changement de mise en page. L-20
- Une transition d'état s'écrit une fois, pas recopiée dans deux branches. L-30
- Rendre un bug inexprimable vaut mieux que le corriger (une valeur rendue, pas relue dans une structure). L-44
- Extraire l'état de la fenêtre, pas des lignes de construction de widgets. L-45
- Mesurer le conteneur ne mesure pas son contenu : l'assertion va du contenant au contenu. L-69
- Deux endroits qui décident de la même chose divergent : extraire la décision dès la deuxième lecture. L-89
- Rendre asynchrone une méthode existante casse ses lecteurs : séparer la moitié lente. L-95
- Un bouton qui fait ce que fait un geste du système (fermer) passe par le même chemin. L-102
- Un crochet de framework ne couvre que les chemins qui le traversent : les suivre tous. L-109
- Une API dont le mésusage est invisible se remplace, pas se documente. L-92
- Un état normal journalisé en avertissement noie le journal joint aux signalements. L-90
- Une validation de chemin qui rejette du légitime ne protège de rien de plus : valider la structure. L-19

## Plugin OpenFX (Resolve)

- Dans un hôte, ce que le plugin déclare peut coûter plus que son calcul : chronométrer le plugin et le rythme des appels. L-118
- Un écart dans l'export de l'hôte n'accuse le plugin qu'après un témoin sans l'effet, mêmes conditions. L-122

## Empaquetage et publication

- Toute modification de `release.yml` se vérifie par un essai à blanc avant fusion. L-114
- Un paquet se juge installé et lancé, pas construit. L-76
- Une instruction d'installation se vérifie en installant. L-64
- Un binaire non signé ajoute une étape d'installation (SmartScreen, antivirus) : le README décrit le trajet du lecteur. L-81
- Épingler une dépendance (ffmpeg), c'est épingler une propriété (le plancher pilote NVENC), pas un numéro. L-74
- Une URL épinglée se choisit pour sa durée de vie : vérifier ce que l'amont conserve. L-75
- Une étape manuelle à refaire avant chaque version finira oubliée : la supprimer (notes tirées des PR) plutôt que la garder. L-65
- Une observation faite sur une plateforme se rejoue sur l'autre. L-63

## CI et GitHub

- Couper la sortie d'un outil dans un tube (`| head`, `| Select-Object -First 1`) le fait échouer. L-115
- Un check requis sauté compte comme réussi : `if: always()` et échec explicite si un `needs:` a échoué. L-111
- Une clé de cache immuable appartient au premier job qui finit : un job qui ne construit rien ne sauve pas de cache. L-108
- Un paramètre dérivé du ref vaut pour tous les refs que le bloc `on:` autorise. L-55
- Un script enfermé dans un YAML s'extrait pour être essayé. L-53
- Un réglage du dépôt ne vaut pas pour ce que fait le jeton d'un workflow. L-100
- `workflow_dispatch` est l'exception à la règle du `GITHUB_TOKEN` : ce qu'un geste humain déclenchait se relance. L-107
- Automatiser un geste, c'est perdre ce qu'il déclenchait : vérifier chaque effet secondaire. L-104
- Une règle qu'il faut se rappeler au moment de cliquer se fait décider par la machine. L-97
- Un garde-fou que la CI seule applique n'en est pas un : le poser aussi dans `scripts/check.sh`. L-96
- Un analyseur a un seul point de réglage : ne pas le relancer à côté de sa configuration. L-62
- `gh` vise le dépôt parent dans un fork : passer `-R Canaill51/superview` en cas de doute. L-68
- Un champ rempli avant que sa valeur existe (numéro de PR) est périmé par construction. L-79

## Outillage et poste

- Aucune modification n'est validée sans chaîne d'outils : « écrit mais non compilé » se dit tel quel. L-01
- Exécuter l'outil plutôt que présumer son état. L-11
- Une vérification locale complète vaut mieux qu'attendre la CI. L-13
- Ne pas se fier au code de sortie d'un script shell ad hoc : un tube rend celui de sa dernière commande. L-28
- Ne pas écrire un fichier produit (couverture) à la racine du dépôt *(exemple Go, principe actif)*. L-07
- Un hook distingue « champ vide » de « je n'ai pas pu lire », échoue du côté sûr, et se teste sans l'outil. L-119
- Un pré-contrôle cherche un outil là où le code le cherchera (`scripts/lib/outils.sh`). L-121

## Tests et contre-preuves (toute tâche)

- Toujours exécuter la contre-épreuve : un test peut passer à vide. L-37
- Une contre-épreuve qui ne change pas le résultat observable n'en est pas une. L-54
- Une contre-épreuve qui ne compile pas n'a rien éprouvé : lire ce qui a rougi. L-86
- Une contre-épreuve appliquée au code partagé se neutralise : saboter au site d'appel. L-78
- Une contre-épreuve peut être faussée par l'environnement (un ffmpeg resté dans le `PATH`). L-49
- Un saut de test est une plateforme entière qui ne teste rien *(exemple Go, principe actif)*. L-50
- Un saut conditionné à l'environnement doit pouvoir devenir un échec (`SUPERVIEW_REQUIRE_FFMPEG=1`). L-60
- Un test qui ne peut pas atteindre l'état ne prouve rien sur l'état. L-35
- Un test qui contredit une décision produit se réécrit, il ne se contourne pas. L-21
- Une assertion « jamais » ou « pas encore » périme sans bruit. L-106
- Chercher un nom dans un message n'est pas vérifier ce qu'il dit : asserter le sens. L-77
- Une refactorisation d'algorithme se prouve par empreinte, pas par relecture. L-12
- Une empreinte fige des octets ; elle ne dit rien des propriétés. L-48
- Ne pas figer une empreinte qui dépend d'un outil externe. L-36
- Un garde-fou qui vérifie le résultat attrape ce que la correction de la cause a manqué. L-58

## Documentation (toute tâche)

- La documentation du dépôt n'est pas une source fiable : vérifier dans le code. L-04
- Un changement de signature balaie la prose : `grep -rn "nom" --include='*.md' .` L-29
- Un renommage « partout » se vérifie par `grep`, pas par relecture. L-51
- Balayer les symboles ne balaie pas les affirmations : quelles promesses la doc faisait-elle ? L-80
- Un principe énoncé dans la doc se vérifie en l'exécutant. L-66
- Une ancre de documentation citée par le code est une API. L-82
- Publier en deux langues double la surface d'une erreur : le miroir se garde par la CI (`readme-parity`). L-83
- Un chiffre dans de la prose est un test sans assertion : il vieillit sans rien casser. L-98
- Une convention non écrite n'est pas une convention. L-47
- Une leçon dont le remède est une commande se termine en exécutant la commande. L-59

## Méthode (toute tâche)

- Vérifier qu'un symbole est vivant avant de le corriger. L-05
- Lire le code d'une bibliothèque, pas la forme canonique du standard. L-10
- Ne pas introduire d'API en prévision d'un usage futur. L-22
- Supprimer du code mort testé fait baisser la couverture : ne pas le garder pour autant. L-24
- Mesurer sur le mauvais échantillon mène à la conclusion inverse. L-25
- Demander le domaine métier (des MP4 de GoPro) avant de calibrer une sévérité. L-26
- Vérifier une hypothèse avant d'écrire le constat. L-27
- Une mesure héritée se revérifie avant d'en tirer une décision. L-46
- Quand l'intention est mesurable, la mesurer bat toute relecture. L-52
- Une observation notée en passant se relie à ses conséquences. L-56
- Une catégorisation qui n'attrape pas tout supprime le reste : mesurer ce que le filtre laisse. L-67

## Historique (Go)

Leçons propres à Go, à Fyne ou à leurs outils (DEC-49) : gardées pour l'histoire, elles ne s'appliquent
plus telles quelles ; leur principe a souvent un équivalent Rust (L-06 : `upgrade_in_event_loop`, AGENTS.md).

- La CI de qualité ne couvrait pas le paquet `main`. L-02
- Toute mise à jour d'interface depuis une goroutine passait par `fyne.Do`. L-06
- `strings.Split("", sep)` rend `[""]`, pas une tranche vide. L-08
- Un canal ne se ferme qu'à un seul endroit. L-18
- Un test de concurrence se conçoit pour échouer sous `-race`. L-23
- Un test de mise en page s'écrit sur le rendu, pas sur l'arbre de widgets. L-70
- Un motif de version (`1.26.x`) veut dire « ce qui est déjà là ». L-99

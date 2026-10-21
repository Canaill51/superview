# Pièges rencontrés — index

> Une ligne par leçon permanente de [LECONS.md § 2](LECONS.md) (le texte complet, avec sa cause
> et son contrôle, est là-bas). **Claude : avant de toucher un domaine, cherche ici**
> (`grep -i <mot> docs/PIEGES.md`) puis lis la leçon : `grep -n "### L-NN" docs/LECONS.md`.
> Un nouveau piège : une entrée L-NN dans LECONS.md § 2 (skill `/piege`), puis une ligne ici.
> Le modèle `docs/pieges/_modele.md` sert aux pièges qui méritent un fichier à eux.
> Les pièges valables pour TOUS les projets vont aussi dans `~/.claude/pieges/`.
>
> Ce fichier n'est volontairement **pas** importé dans `CLAUDE.md` : plus de cent lignes à chaque
> session pour un index qu'on consulte par mot-clé (DEC-40).
>
> **Nouvelle ligne : dans le premier tableau**, au-dessus de la section « Historique (Go) ».

| Date | Leçon | Règle |
|------|-------|-------|
| 2026-09-04 | L-01 | Aucune modification n'est validée sans toolchain |
| 2026-09-04 | L-03 | L'ordre des arguments FFmpeg change leur sens |
| 2026-09-04 | L-04 | La documentation du dépôt n'est pas une source fiable |
| 2026-09-04 | L-05 | Vérifier qu'un symbole est vivant avant de le corriger |
| 2026-09-04 | L-07 | Ne pas écrire le profil de couverture à la racine *(exemple Go, principe actif)* |
| 2026-09-04 | L-09 | Les mathématiques de `GeneratePGM` ont une source de vérité externe |
| 2026-09-04 | L-10 | Lire le code d'une bibliothèque, pas la forme canonique du standard |
| 2026-09-04 | L-11 | Exécuter l'outil plutôt que présumer son état |
| 2026-09-04 | L-12 | Une refactorisation d'algorithme se prouve par empreinte, pas par relecture |
| 2026-09-04 | L-13 | Une vérification locale complète vaut mieux qu'attendre la CI |
| 2026-09-04 | L-14 | `-re` n'a pas sa place dans une transcodification vers fichier |
| 2026-09-04 | L-15 | Traiter le cas d'erreur générique avant le cas particulier |
| 2026-09-04 | L-16 | Aucun chemin relatif dans une application graphique |
| 2026-09-04 | L-17 | Prouver l'équivalence d'un parsing sur une entrée réelle |
| 2026-09-04 | L-19 | Une validation qui rejette du légitime coûte sans rien acheter |
| 2026-09-04 | L-20 | Dans une fenêtre à taille fixe, ajouter un widget est un changement de mise en page |
| 2026-09-04 | L-21 | Un test qui contredit une décision produit se réécrit, ne se contourne pas |
| 2026-09-04 | L-22 | Ne pas introduire d'API en prévision d'un usage futur |
| 2026-09-04 | L-24 | Supprimer du code mort testé fait baisser la couverture |
| 2026-09-04 | L-25 | Mesurer sur le mauvais échantillon mène à la conclusion inverse |
| 2026-09-04 | L-26 | Demander le domaine métier avant de calibrer une sévérité |
| 2026-09-04 | L-27 | Vérifier une hypothèse avant d'écrire le constat |
| 2026-09-04 | L-28 | Ne pas se fier au code de sortie d'un script shell ad hoc |
| 2026-09-04 | L-29 | Une refactorisation de signature doit balayer la documentation |
| 2026-09-04 | L-30 | Une transition d'état recopiée dans deux branches finit par diverger |
| 2026-09-04 | L-31 | Une annulation doit être une erreur typée, jamais une chaîne |
| 2026-09-04 | L-32 | Une unité mal nommée dans un commentaire est un bug latent |
| 2026-09-04 | L-33 | Une métrique dérivée d'une constante inventée nuit plus qu'une métrique absente |
| 2026-09-04 | L-34 | Un changement de format de fichier se valide sur la sortie décodée, pas sur le fichier |
| 2026-09-04 | L-35 | Un test qui ne peut pas atteindre l'état ne prouve rien sur l'état |
| 2026-09-04 | L-36 | Ne pas figer une empreinte qui dépend d'un outil externe |
| 2026-09-04 | L-37 | Toujours exécuter la contre-épreuve : un test peut passer à vide |
| 2026-09-04 | L-38 | Déduire une propriété d'un nom exige de lister les contre-exemples d'abord |
| 2026-09-04 | L-39 | `runtime.NumCPU()` n'est pas une valeur valide pour un outil externe |
| 2026-09-04 | L-40 | Un test d'annulation doit interrompre un travail réellement commencé |
| 2026-09-04 | L-41 | Observer depuis l'extérieur du processus qu'on va tuer |
| 2026-09-04 | L-42 | Calibrer un paramètre par la mesure, sur le contenu qui le sollicite |
| 2026-09-04 | L-43 | Une même géométrie calculée à deux endroits finit par diverger |
| 2026-09-04 | L-44 | Rendre un bug inexprimable vaut mieux que le corriger |
| 2026-09-04 | L-45 | Extraire l'état, pas les lignes |
| 2026-09-04 | L-46 | Une mesure héritée se revérifie avant d'en tirer une décision |
| 2026-09-04 | L-47 | Une convention non écrite n'est pas une convention |
| 2026-09-04 | L-48 | Une empreinte fige des octets ; elle ne dit rien des propriétés |
| 2026-09-04 | L-49 | Une contre-épreuve peut être fausse par contamination de l'environnement |
| 2026-09-04 | L-50 | Un `t.Skip` est une plateforme entière qui ne teste rien *(exemple Go, principe actif)* |
| 2026-09-04 | L-51 | Un renommage « partout » se vérifie par `grep`, pas par relecture |
| 2026-09-04 | L-52 | Quand l'intention est mesurable, la mesurer bat toute relecture |
| 2026-09-05 | L-53 | Un script enfermé dans un YAML s'extrait pour être essayé |
| 2026-09-05 | L-54 | Une contre-épreuve qui ne change pas le résultat n'est pas une contre-épreuve |
| 2026-09-05 | L-55 | Un paramètre dérivé du ref doit valoir pour tous les refs qui déclenchent le workflow |
| 2026-09-05 | L-56 | Une observation notée en passant doit être reliée à ses conséquences |
| 2026-09-05 | L-57 | Un état transitoire ne s'observe pas après coup |
| 2026-09-05 | L-58 | Un garde-fou qui vérifie le résultat attrape ce que la correction de la cause a manqué |
| 2026-09-06 | L-59 | Une leçon non appliquée jusqu'au bout ne protège de rien |
| 2026-09-06 | L-60 | Un `t.Skip` conditionné à l'environnement doit pouvoir devenir un échec |
| 2026-09-06 | L-61 | Séparer l'appel du processus et le parsing de sa sortie |
| 2026-09-06 | L-62 | Un outil lancé à côté de sa configuration finit par la contredire |
| 2026-09-06 | L-63 | Une observation faite sur une plateforme se rejoue sur l'autre |
| 2026-09-06 | L-64 | Une instruction d'installation se vérifie en installant |
| 2026-09-06 | L-65 | Un artefact qu'il faut tenir à jour avant chaque release finira périmé |
| 2026-09-06 | L-66 | Un principe énoncé dans la doc se vérifie en l'exécutant |
| 2026-09-06 | L-67 | Une catégorisation qui n'attrape pas tout supprime le reste |
| 2026-09-06 | L-68 | `gh` vise le dépôt parent quand on travaille dans un fork |
| 2026-09-06 | L-69 | Mesurer le conteneur ne mesure pas ce qu'il contient |
| 2026-09-06 | L-71 | Une liste de capacités décrit le binaire, pas la machine |
| 2026-09-06 | L-72 | Un numéro de version n'est pas un contrat de compatibilité |
| 2026-09-06 | L-73 | Une sonde mal formée accuse ce qu'elle mesure |
| 2026-09-06 | L-74 | Épingler une dépendance, c'est épingler une propriété, pas un numéro |
| 2026-09-06 | L-75 | Une URL épinglée doit être choisie pour sa durée de vie |
| 2026-09-06 | L-76 | Vérifier l'installation, pas seulement la construction |
| 2026-09-06 | L-77 | Chercher un nom dans un message n'est pas vérifier ce qu'il dit |
| 2026-09-06 | L-78 | Une contre-épreuve appliquée au code partagé se neutralise elle-même |
| 2026-09-06 | L-79 | Un champ rempli avant que sa valeur existe est périmé par construction |
| 2026-09-06 | L-80 | Balayer les symboles ne balaie pas les affirmations |
| 2026-09-07 | L-81 | Un binaire non signé a une étape d'installation que le README ne voit pas |
| 2026-09-07 | L-82 | Une ancre de documentation citée par le code est une API |
| 2026-09-07 | L-83 | Publier en deux langues, c'est doubler la surface d'une affirmation fausse |
| 2026-09-08 | L-84 | Un signal reçu n'est pas une action de l'utilisateur |
| 2026-09-08 | L-85 | Un processus tué de l'extérieur porte un diagnostic, pas seulement un échec |
| 2026-09-08 | L-86 | Une contre-épreuve qui ne compile pas n'a rien éprouvé |
| 2026-09-08 | L-87 | Une préférence ne l'emporte pas sur une capacité |
| 2026-09-08 | L-88 | Un garde-fou se chiffre au banc, et refuse ce qu'il a mesuré |
| 2026-09-08 | L-89 | Deux endroits qui décident de la même chose finissent par diverger |
| 2026-09-08 | L-90 | Un état normal journalisé en avertissement noie le journal |
| 2026-09-08 | L-91 | Un seuil de santé doit se dériver du besoin, pas d'un chiffre rond |
| 2026-09-08 | L-92 | Une API dont la mauvaise utilisation est invisible se remplace, pas se documente |
| 2026-09-09 | L-93 | Une sonde répond à la question qu'elle pose, pas à celle qu'on se pose |
| 2026-09-09 | L-94 | Un garde-fou doit suivre l'objet qu'il garde |
| 2026-09-09 | L-95 | Rendre asynchrone une méthode existante casse ses lecteurs |
| 2026-09-11 | L-96 | Un garde-fou que la CI seule applique n'en est pas un |
| 2026-09-11 | L-97 | Une règle qu'il faut se rappeler au moment de cliquer finit par être mal appliquée |
| 2026-09-11 | L-98 | Un document ne compile pas : ce qu'il compte vieillit sans rien casser |
| 2026-10-14 | L-100 | Un réglage du dépôt ne vaut pas pour ce que fait le jeton d'un workflow |
| 2026-10-14 | L-101 | Une sentinelle créée pour distinguer deux causes doit être relue partout où la première est testée |
| 2026-10-14 | L-102 | Un bouton qui fait la même chose qu'un geste du système doit passer par le même chemin |
| 2026-10-14 | L-103 | Un test « les deux posent la même question » doit comparer la question entière |
| 2026-10-14 | L-104 | Automatiser un geste, c'est aussi perdre ce que ce geste déclenchait |
| 2026-10-14 | L-105 | Déplacer un échec de l'ouverture vers la fin le rend destructeur |
| 2026-10-14 | L-106 | Une assertion « jamais » dans un test périme sans bruit |
| 2026-10-14 | L-107 | `workflow_dispatch` est l'exception à la règle du `GITHUB_TOKEN` |
| 2026-10-14 | L-108 | Une clé de cache immuable appartient au premier job qui finit |
| 2026-10-14 | L-109 | Un crochet de framework ne couvre que les chemins qui le traversent |
| 2026-10-14 | L-110 | Un signal de fin se libère avant tout appel qui peut mener à celui qui l'attend |
| 2026-10-14 | L-111 | Un check requis sauté compte comme réussi |
| 2026-10-14 | L-112 | Le nom d'un test finit dans un chemin, et `cmd.exe` coupe un chemin à la virgule |
| 2026-10-14 | L-113 | Une constante mesurée appartient à la configuration où elle a été mesurée |
| 2026-10-14 | L-114 | Une étape ajoutée à `release.yml` n'est pas vérifiée tant qu'un essai à blanc ne l'a pas fait tourner |
| 2026-10-14 | L-115 | Couper la sortie d'un outil dans un tube (`\| head`, `\| Select-Object -First 1`) le fait échouer |
| 2026-10-17 | L-116 | Deux encodages multi-fil du même clip ne donnent pas les mêmes images |
| 2026-10-21 | L-117 | Un test ne modifie pas l'environnement du processus : on le passe en paramètre |

## Historique (Go)

> Ces leçons parlent de Go, de Fyne ou de leurs outils (DEC-49) : elles sont gardées pour l'histoire et ne
> s'appliquent plus telles quelles ; leur principe a souvent un équivalent Rust (ex. L-06 : voir `upgrade_in_event_loop`, `AGENTS.md`).

| Date | Leçon | Règle |
| --- | --- | --- |
| 2026-09-04 | L-02 | La CI de qualité ne couvre pas le paquet `main` |
| 2026-09-04 | L-06 | Toute mise à jour d'UI depuis une goroutine passe par `fyne.Do` |
| 2026-09-04 | L-08 | `strings.Split("", sep)` retourne `[""]`, pas une tranche vide |
| 2026-09-04 | L-18 | Un canal ne se ferme qu'à un seul endroit |
| 2026-09-04 | L-23 | Un test de concurrence se conçoit pour échouer sous `-race` |
| 2026-09-06 | L-70 | Un test de mise en page s'écrit sur le rendu, pas sur l'arbre de widgets |
| 2026-10-14 | L-99 | Un motif de version n'est pas une version : `1.26.x` veut dire « ce qui est déjà là » |

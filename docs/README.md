# Documentation

Les documents internes sont en français ; ce que lit un visiteur (`hardware-support.md`, `CONTRIBUTING.md`,
les README à la racine) est en anglais (AGENTS.md, *The repository's language split*).

| Document | Pour qui | Contenu |
|---|---|---|
| [architecture.md](architecture.md) | Développeurs, agents IA | La carte : crates, modules, arborescence, flux de données, dépendances |
| [contrats.md](contrats.md) | Développeurs, agents IA | **À lire avant d'éditer un fichier Rust** : invariants du pipeline, faits FFmpeg mesurés |
| [decisions.md](decisions.md) | Développeurs, agents IA | Registre des décisions (`DEC-xx`) : ce qui ressemble à un défaut, sa raison, ce qui le rouvrirait |
| [lecons.md](lecons.md) | Développeurs, agents IA | Leçons numérotées (`L-xxx`) : symptôme, cause, règle, contrôle ; lire le § 2 avant de corriger |
| [pieges-par-domaine.md](pieges-par-domaine.md) | Développeurs, agents IA | La règle courte de chaque leçon, rangée par domaine (FFmpeg, interface, plugin, CI…), avec son numéro `L-xxx` |
| [corrections.md](corrections.md) | Développeurs, agents IA | Journal des corrections appliquées, une entrée par correction |
| [analyse.md](analyse.md) | Développeurs, agents IA | Constats numérotés (`U-03`, `E-04`, `F-12`…) et leur état ; le tableau du § 4 fait foi |
| [plan.md](plan.md) | Développeurs, agents IA | **Le seul plan** : où on en est (le hook de début de session en affiche la tête), ce qui reste à faire, ce qui est écarté, l'historique des lots |
| [tests-manuels.md](tests-manuels.md) | Développeurs | Ce que les tests automatiques ne voient pas : bureau, GPU, sélecteurs, Resolve |
| [environnement.md](environnement.md) | Développeurs | Reconstruire le poste de vérification (rustup sans sudo, ffmpeg, outils de couverture) |
| [hardware-support.md](hardware-support.md) | Utilisateurs, développeurs | Encodeurs matériels : sonde au démarrage, pilotes, repli (anglais) |
| [archives/](archives/) | — | Textes sortis des journaux vivants, inchangés : analyse passes 3 à 13, corrections de l'ère Go, plans terminés du plugin OpenFX, de sa livraison et de l'alignement sur OpenTuneBox |

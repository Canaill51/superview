# Plan de travail

**Le seul plan du projet** : on n'en ouvre jamais un second. Où on en est, puis ce qui reste à faire, ce
qui est écarté, et en bas l'historique, une ligne par lot fait. Le hook de début de session affiche les
15 premières lignes de ce fichier.

**État au 2026-11-12** : **v0.5.0 publiée le 2026-10-28** (installateur Windows, AppImage, archive du plugin
Resolve `superview-ofx-v0.5.0-windows-x86_64.zip`, `checksums.txt`), installée à la main et conforme.
Depuis : DLL GPU du plugin chargées depuis System32 seulement et fichiers de version attestés (#192, DEC-70).
**En cours : l'alignement sur OpenTuneBox**, 7 étapes, une PR chacune (`~/.claude/plans/abstract-squishing-rabbit.md`) ;
étapes 1 (#193), 2 (#194) et 3 (contrôle de la doc) faites, prochaine : l'étape 4. Le plugin attend du matériel pour Studio, Linux
et AMD. Une version suivante seulement sur demande, par `/publier` (DEC-42).

## Comment s'en servir

- **Une session = un lot = une branche = une PR** avec l'étiquette `merge`. Faire `/clear`, puis demander
  « Lot N de `docs/plan.md` ».
- Chaque lot suit **explorer → planifier → implémenter → vérifier → livrer → consigner**. Planifier =
  présenter le découpage et attendre l'accord de l'utilisateur. Consigner = cocher ici, puis
  [corrections.md](corrections.md), [decisions.md](decisions.md), [lecons.md](lecons.md) et
  [pieges-par-domaine.md](pieges-par-domaine.md) selon le cas.
- **Lots manuels (M)** : l'utilisateur agit (Resolve, un vrai Windows) ; Claude prépare les fichiers de test,
  compare les exports par ffmpeg et consigne dans [tests-manuels.md](tests-manuels.md).
- **Vérifier** : `scripts/check.sh` complet (`/verify`), et la contre-preuve de chaque nouveau test.
  Toute modification de `release.yml` : essai à blanc **avant** fusion (L-114, `gh workflow run release.yml
  --ref <branche>`, sans version).
- **Critère d'arrêt** : si l'hypothèse d'un lot tombe, s'arrêter, consigner ce qui a été vu, revoir le plan.
- **Modèle** : Sonnet pour le mécanique et la documentation, Opus pour la conception et le débogage GPU.
- Avant de commencer sur un poste neuf : `git config core.hooksPath .githooks`, Rust par rustup
  ([environnement.md](environnement.md)), puis `scripts/check.sh`. Couverture (50 %), `cargo audit` et
  `cargo deny` : `make coverage-audit`, ou le job de la CI.

## À faire

### Plugin OpenFX — configurations non essayées (M) · en attente de matériel

Elles ne bloquent pas la livraison (DEC-69) ; le README les dit non essayées.

- [ ] **Resolve Studio** (Windows) : chargement, CUDA, vitesse. Demande une licence ou un poste qui l'a.
- [ ] **Resolve sous Linux** (poste KDE, NVIDIA et Intel) : chargement depuis `/usr/OFX/Plugins`, CUDA,
      OpenCL. Puis publier l'archive Linux (DEC-69). À vérifier et documenter : Resolve **gratuit** sous Linux
      ne décoderait pas le H.264/H.265 ni l'AAC (limite de Resolve, pas du plugin).
- [ ] **AMD** : pas de matériel ; reste « non vérifié », comme AMF pour l'application.
- [ ] À surveiller : deux plantages de Resolve en OpenCL au passage à Deliver, dans le pilote OpenGL de
      NVIDIA, non reproduits ni avec ni sans le plugin, non imputés au plugin.

### Application — tests manuels en suspens

Détail dans [tests-manuels.md](tests-manuels.md) : encodeurs AMD (AMF) et Intel QSV (matériel), portail xdg
lent, Linux sans portail ni zenity (champ texte de repli), lien d'installation.

## Écarté

Les arbitrages à ne pas rouvrir sans leur condition sont dans [decisions.md](decisions.md) (table courte dans
AGENTS.md). Écartés en cours de plan :

- **Le plugin dans l'installateur Windows** : téléchargement séparé à la place (DEC-69, 2026-10-28).
- **Une archive Linux du plugin** tant que Resolve n'a pas été essayé sous Linux (DEC-69).
- **Une version du plugin tirée de `SUPERVIEW_VERSION`** : `PluginVersion(1, 0)` reste fixe (DEC-69).

## À savoir

- Un chemin d'entrée non UTF-8 est refusé (`check_video` prend un `&str`, DEC-43).
- L'interface vide l'étiquette « Hardware: … » sur erreur ; un `Prober` doit être durable ; le `temp_dir`
  passé à `check_health` doit être nettoyé.
- winit charge `libxkbcommon-x11` à l'exécution sous X11 (embarquée dans l'AppImage, DEC-47) ; `rfd` a
  besoin de libdbus (portail) ou de zenity.
- Publier une version : demander « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42).

## Historique

Une ligne par lot ou par chantier fait. Le détail est dans les PR, [corrections.md](corrections.md) et les
plans archivés ; ce tableau dit quand et par quoi chaque chantier est prouvé.

| Chantier | Fait le | Sujet et preuve | Détail |
| --- | --- | --- | --- |
| Portage Go → Rust | 2026-10-20 | Rust + Slint, phases 1 à 8 (#99 à #137) ; Go retiré (DEC-43, DEC-49). Dernière version Go : v0.2.9 | DEC-43 ; `git show b6cd5f8:docs/SUITE.md` |
| 14ᵉ passe d'analyse | 2026-10-20 | F-01 à F-30, Q-04 (`-f mov`, DEC-52), Q-05 (durée non finie, DEC-53) ; #141 à #160, v0.3.1 | [analyse.md](analyse.md) § 3terdecies |
| Refonte de l'interface | 2026-10-22 | Maquette cyan, DEC-56 à DEC-64 (#162 à #171), validée à la main sous Windows et Linux ; v0.4.0 | `~/.claude/plans/encapsulated-cooking-badger.md` |
| Défauts du poste Windows | 2026-10-26 | `h264_d3d12va` sans images B (DEC-67, #175), hooks sans `python3` (L-119, #176), ffmpeg cherché comme le code (L-121) | [corrections.md](corrections.md) |
| OFX Lot 1 (M) | 2026-10-26 | Tests manuels sans code : CUDA contre CPU, parité au pixel (47,4 dB), centrage, *Un-squeeze*, défilement (L-122) | [archives/plan-ofx.md](archives/plan-ofx.md) |
| OFX Lot 2 | 2026-10-26 | Journal du plugin (`journal.rs`, `superview-core::logfile`, DEC-65 amendée), vu dans Resolve | idem |
| OFX Lot 3 | 2026-10-26 | Échec du noyau CUDA mémorisé par contexte (`PerContext`, DEC-66 amendée) | idem |
| OFX Lot 4 | 2026-10-27 | Chemin OpenCL à côté de CUDA : sonde, noyau sur tampons, colle, essai dans Resolve (DEC-68) ; 13,3 i/s en 4K | idem |
| OFX Lot 5, en partie (M) | 2026-10-27 | Export long en `FullySafe` (un seul rendu à la fois sur 21 min) ; OpenCL sur l'Intel UHD | idem ; le reste est **À faire** |
| OFX Lot 6 = Livraison | 2026-10-28 | Archive Windows du plugin à chaque version (DEC-69, #188), README (#189), v0.5.0 publiée et installée à la main | [archives/plan-livraison.md](archives/plan-livraison.md) |
| Audit de sécurité | 2026-11-11 | DLL GPU depuis System32, versions immuables et attestées, permissions de Claude Code (DEC-70, #192) | [corrections.md](corrections.md) |
| Alignement OTB, étape 1 | 2026-11-12 | Documents de `docs/` en minuscules, index `docs/README.md` (#193) | [corrections.md](corrections.md) |
| Alignement OTB, étape 2 | 2026-11-12 | Ce plan unique (fusion de `SUITE`, `PLAN-OFX`, `PLAN-LIVRAISON`), pièges rangés par domaine (DEC-71) | [corrections.md](corrections.md) |
| Alignement OTB, étape 3 | 2026-11-12 | Contrôle automatique de la doc (`docs_guards.rs`), compétence `docs-sync`, section Claude Code de `CLAUDE.md` | [corrections.md](corrections.md) |

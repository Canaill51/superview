# Où on en est
- **En cours (2026-10-25) — plugin OpenFX pour DaVinci Resolve** (DEC-65, branche `feat/ofx-plugin`) : la crate
  `crates/superview-ofx` (calcul testé contre les cartes de l'application, colle OpenFX, bundle, artefact CI) est
  écrite ; reste à l'essayer dans Resolve (`docs/TESTS-MANUELS.md`). Pas de livraison dans les versions tant que
  ce n'est pas validé. Plan : `~/.claude/plans/nifty-wondering-snail.md`.
- État (2026-10-22) : **v0.4.0 publiée** (installateur Windows, AppImage, `checksums.txt` ; elle apporte la refonte de l'interface,
  #162 à #171, et l'identifiant de fenêtre `superview`). La v0.3.1 (14ᵉ passe, #141 à #160) la précédait. La v0.3.0 était le premier binaire Rust + Slint. Go est retiré (DEC-49) ; la dernière version Go est la **v0.2.9**. La pré-version
  `RC-0.3.0` l'a précédée. Aucune PR ouverte, aucune décision en suspens. Capture du README remplacée par la fenêtre Slint
  (#140) ; VAAPI, GPU Intel seul et « device frame » passés à la main (#139). **14ᵉ passe d'analyse consignée**
  (`docs/ANALYSE.md` § 3terdecies, F-01 à F-30, Q-04, Q-05). **Plan de la 14ᵉ passe terminé** : ses sessions 1 à 8 (F-01 à F-30) et la session 9 (Q-04 et Q-05 : `-f mov` sur l'entrée, DEC-52 ;
  durée non finie refusée, DEC-53), faites en #143 à #153. Le détail, session par session, est dans `docs/CORRECTIONS.md`
  (entrées du 2026-10-20, les plus récentes en haut). **Refonte de l'interface terminée et validée à la main sous Windows et Linux (2026-10-22)** (`~/.claude/plans/encapsulated-cooking-badger.md`) : #162 à #170 (DEC-56 à DEC-63), et l'identifiant de la fenêtre devenu `superview` (#169, DEC-64), qui règle l'icône et l'épinglage sous KDE avec Gear Lever. Elle est dans la v0.4.0 ; l'artefact de l'essai à blanc a été testé par l'utilisateur avant la publication. Une version suivante seulement sur demande, via `/publier`. Non vérifié sur la release publiée : l'installateur sur un vrai Windows et les
  encodeurs AMD et QSV.
- Tests manuels passés sur Linux (KDE, Wayland), Windows et Intel/NVIDIA (`docs/TESTS-MANUELS.md`) ; en suspend : AMD et QSV
  (matériel), portail lent, champ texte de repli, lien d'installation.
- Avant de commencer : `git config core.hooksPath .githooks` (une fois par clone), Rust par rustup (`docs/ENVIRONNEMENT.md`),
  puis `scripts/check.sh`. Le contrôle de couverture (50 %) et `cargo audit` : `make coverage-audit`, ou le job de la CI.
- Toute modification de `.github/workflows/release.yml` : essai à blanc AVANT fusion (L-114,
  `gh workflow run release.yml --ref <branche>`, sans version).
- À savoir : un chemin d'entrée non UTF-8 est refusé (`check_video` prend un `&str`) ; l'interface vide l'étiquette « Hardware: … »
  sur erreur ; un `Prober` doit être durable ; le `temp_dir` passé à `check_health` doit être nettoyé ; winit charge
  `libxkbcommon-x11` à l'exécution sous X11 (embarquée dans l'AppImage, DEC-47) ; `rfd` a besoin de libdbus (portail) ou de zenity.
- Publier une version : demander « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42), confirme le numéro, lance le workflow Release et vérifie les fichiers.
- Mettre ce fichier à jour à la clôture d'une session (`/cloture`).

## Historique
Le programme a été écrit en Go + Fyne jusqu'à la v0.2.9, puis porté en Rust + Slint (DEC-43, phases 1 à 8 en octobre 2026,
#99 à #137) ; Go a été retiré avec #136 et #137 (DEC-49). Résumé par phase dans DEC-43 (`docs/DECISIONS.md`) ; le journal complet, PR par PR,
est dans `git show b6cd5f8:docs/SUITE.md` (section « Historique du portage Go → Rust »). Pour relire l'ancien code : `git show v0.2.9:<chemin>`.

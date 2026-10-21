# Superview — Journal des corrections appliquées

> **Journal vivant** : une entrée par correction, ajoutée après CHAQUE correction appliquée
> au code (procédure et gabarit : [LECONS.md § 1](LECONS.md)). Il était le § 3 de
> LECONS.md ; il en a été sorti pour que la lecture des règles (§ 2) ne traîne pas
> 2 000 lignes d'historique. Texte inchangé. Les renvois « LECONS § 3 » désignent ce fichier (l'archive pour les entrées d'avant le 2026-10-17).
> **Go et Fyne.** Les noms de ce journal (`common/`, `internal/common/`, `cmd/superview/`, `gui_*.go`, `appState`, `go test`, `-race`…) décrivent le code Go retiré (DEC-49) : ils ne sont pas réécrits (DEC-55). Pour relire un fichier cité : `git show v0.2.9:<chemin>`, avec `common/` = `internal/common/` et `gui_*.go` = `cmd/superview/` (`common.go` y est découpé en huit fichiers, DEC-36) ; les numéros de ligne cités sont ceux du commit analysé, pas de v0.2.9.
> Pour savoir si une correction a déjà été tentée : `grep -n "<mot>" docs/CORRECTIONS.md docs/archives/CORRECTIONS-ere-go.md`.

## 3. Corrections appliquées

### [2026-10-21] Barre de progression rouge, verte et lissée (DEC-58, refonte de l'interface, PR 3)

| | |
| --- | --- |
| **Constat** | une barre Slint d'un seul aspect, qui avançait par à-coups |
| **Cause** | `ProgressIndicator` n'a ni couleur d'état ni animation réglables |
| **Fichiers** | `ui/components.slint` (`BarState`, `ProgressBar`), `ui/main.slint`, `src/state.rs` (`BarState`, transitions), `src/app.rs` (`apply_view`), `src/state/tests.rs`, `src/window_tests.rs`, `src/app_tests.rs`, `docs/DECISIONS.md` (DEC-58) |
| **Règle** | un état d'affichage se fixe là où l'état métier change déjà, pas dans la colle ; un test de couleur compare tous les états entre eux, pas un seul |
| **Vérification** | rouges chacun avec le défaut réintroduit (mesuré) : Done remplacé par Idle (`the_bar_turns_done_on_success`, `a_new_input_resets_the_bar`) ; Stopped retiré de l'échec (test d'état et scénario d'échec) ; arms Idle et Stopped intervertis dans `apply_view` (scénarios d'`app_tests.rs`) ; rôle `progress-indicator` retiré (`the_bar_changes_colour_with_its_state`) ; `scripts/check.sh` |
| **Relecture** | la barre garde son rôle accessible et sa valeur (le `ProgressIndicator` les avait) ; DEC-58 précise que seul un nouveau fichier d'entrée remet la barre à zéro |

### [2026-10-21] Menu Theme et palette cyan (DEC-57, refonte de l'interface, PR 2)

| | |
| --- | --- |
| **Constat** | thème sombre forcé, couleurs en dur (`#5aa9ff`, `#a8a8a8`, `#ff7070`, voile) dans `main.slint`, aucune couleur d'accent |
| **Cause** | `Palette` de Slint est en lecture seule : l'accent et le thème clair n'étaient pas réglables sans global propre |
| **Fichiers** | `ui/theme.slint` et `ui/components.slint` (nouveaux), `ui/main.slint` (menu Theme, `AppButton`, `Card`, couleurs par `Theme`), `src/prefs.rs` (`theme`, `THEMES`), `src/state.rs` (`theme_changed`), `src/app.rs`, `src/window_tests.rs`, `docs/DECISIONS.md` (DEC-57), README.md et README_FR.md |
| **Règle** | un composant maison garde le rôle et le libellé accessibles du widget qu'il remplace, sinon les tests de fenêtre ne le trouvent plus (le texte interne en `accessible-role: none`, sinon le libellé existe deux fois) ; un nouveau modèle de menu sélectionne son premier élément : fixer le choix après |
| **Relecture** | le premier clic souris était perdu : un `FocusScope` pleine taille au-dessus du `TouchArea` prenait l'appui pour le focus (corrigé : largeur 0, comme le `Button` de Slint ; `an_active_button_answers_the_first_mouse_click`, rouge avant) ; accent clair foncé à `#0a7385` (contraste ≥ 4,5:1 avec le blanc) ; `Palette.color-scheme` vérifié (`scheme-dark`) ; Entrée ignorée sur un bouton désactivé |
| **Vérification** | `the_theme_menu_switches_the_colours` rouge sans `Theme.name = root.theme`, `the_theme_defaults_to_dark_and_an_unknown_one_becomes_dark` rouge sans le filtre de `sanitized` (mesuré) ; captures Xvfb en Light et en Dark ; `scripts/check.sh` |

### [2026-10-21] Fenêtre 860×332 : réglages à gauche, panneau de conversion à droite (DEC-56, étude d'interface, PR 1)

| | |
| --- | --- |
| **Constat** | fenêtre de 980×470 aux menus étirés, valeurs loin de leurs libellés, Status, Results et Hardware au-dessus de la barre |
| **Cause** | une seule colonne pour tout, sans largeur propre aux menus |
| **Fichiers** | `superview-app/ui/main.slint` (en-tête sur une ligne, `GridLayout` à gauche avec Hardware dessous, panneau à droite, 860×332), `src/window_tests.rs` (tailles, `the_conversion_panel_sits_beside_the_settings`), `docs/DECISIONS.md` (DEC-56, DEC-54 remplacée), `docs/ARCHITECTURE.md` |
| **Règle** | une disposition se prouve par les positions des éléments, pas par les textes ; l'ajustement des textes à la vraie police se juge sur une capture |
| **Vérification** | le nouveau test rouge sur l'ancienne mise en page (« a menu ends at x=968, past the panel that starts at x=12 »), vert ensuite ; capture sous Xvfb à 860×332 ; `scripts/check.sh` |

### [2026-10-21] Les journaux gardent Go dans leur texte, archivés plutôt que réécrits (DEC-55, plan « traces de Go », PR 5)

| | |
| --- | --- |
| **Constat** | les journaux mêlaient plus de 2 000 lignes d'histoire Go au vivant ; leurs bandeaux disaient que `internal/common/` existait ; 22 liens pointaient vers des fichiers supprimés ; 7 lignes de `PIEGES.md` parlaient d'outils disparus |
| **Cause** | DEC-49 avait laissé les journaux « pas réécrits » sans les situer |
| **Fichiers** | bandeau « Go et Fyne » dans `LECONS`, `ANALYSE`, `CORRECTIONS`, `DECISIONS`, `archives/ANALYSE-passes-3-a-13` (`CONTRATS` en avait déjà un) ; les entrées d'avant le 2026-10-17 déplacées dans `archives/CORRECTIONS-ere-go.md` (titre-renvoi laissé ici, liens relatifs ajustés) ; `PIEGES.md` : section « Historique (Go) » ; liens vers `cmd/`, `internal/` remplacés par du texte ; DEC-55 |
| **Règle** | on archive, on ne réécrit pas : le contenu d'une entrée ne change jamais, seule sa place |
| **Vérification** | script de liens relatifs sur tous les `.md` : 0 cassé (22 avant) ; les entrées archivées ne diffèrent de l'original que par 40 liens relatifs (38 lignes) (`../` ajouté) ; `scripts/check.sh` |

### [2026-10-21] Les noms hérités de Go sont renommés (plan « traces de Go », PR 4)

| | |
| --- | --- |
| **Constat** | `parse_go_float`, une quinzaine de tests (`*_match_the_go_oracle`, `the_constants_are_gos`…), les fichiers `tests/parity_*.rs`, le dossier `fixtures/parity/` et quatre `reason` de `allow(clippy::too_many_arguments)` portaient le nom de Go |
| **Cause** | Go retiré en DEC-49 sans repasser sur les noms |
| **Fichiers** | `parse_go_float` → `parse_ffprobe_float` ; `parity_*.rs` → `recorded_*.rs` (`parity_oracle.rs` → `recorded_pgm_specs.rs`) ; `fixtures/parity/` → `fixtures/recorded/` (`git mv`, contenu gelé inchangé) ; `parity_video` → `recorded_video` ; tests renommés sans « go » ; AGENTS, ARCHITECTURE, CONTRATS (chemins) |
| **Règle** | un nom décrit le comportement ou dit « enregistré » / « v0.2.9 », jamais « Go » ; les `// Go: TestParity_*` et « oracle Go » restent (généalogie des fixtures). Les journaux gardent les anciens noms (PR 5) |
| **Vérification** | la liste des tests (`cargo test -- --list`) avant et après : 646 des deux côtés, seuls des noms changent (21 lignes, fichiers de tests compris) ; `scripts/check.sh` vert ; contre-épreuve : un octet changé dans une fixture fait rougir son test |

### [2026-10-21] Les commentaires du code Rust ne renvoient plus à Go (plan « traces de Go », PR 3)

| | |
| --- | --- |
| **Constat** | ~720 lignes de commentaires (66 fichiers) citaient des fonctions ou des tests Go (« (Go: `fn`) », « Port of … », « Deliberate differences from the Go code ») |
| **Cause** | Go retiré en DEC-49 sans repasser sur les commentaires |
| **Fichiers** | les `.rs` et `main.slint` des trois crates ; commentaires seulement, aucun code, aucune chaîne, aucun identifiant |
| **Règle** | retirer le renvoi, garder le pourquoi ; restent les fixtures enregistrées par l'oracle Go, les écarts voulus avec la v0.2.9 (DEC-43, 45, 52, 53) et les règles de `strconv.ParseFloat` suivies par `parse_go_float` |
| **Reste (PR 4)** | identifiants et chaînes : `parse_go_float`, `*_match_the_go_*`, `the_constants_are_gos`, `validate_bitrate_messages_are_gos`, `reason = "…Go…"` des `allow(clippy::too_many_arguments)`, `"errors.Is looks through %w"`, « LookPath » dans `tools/tests.rs` ; les `// Go: TestParity_*` des tests de parité sont gardés (fixtures enregistrées par l'oracle Go) |
| **Vérification** | `git diff -U0 -- crates/ \| grep '^[-+]' \| grep -vE '^(\+\+\+\|---)' \| grep -vE '^[-+]\s*(//\|#\[doc)'` vide ; `scripts/check.sh` vert |

### [2026-10-21] Les docs vivantes ne parlent plus de Go que par une phrase d'histoire (plan « traces de Go », PR 2)

| | |
| --- | --- |
| **Constat** | AGENTS, ARCHITECTURE, ENVIRONNEMENT, SUITE, RELEASING et des commentaires de `Cargo.toml`, `main.slint`, `build*.rs`, `rust.yml` racontaient encore Go (21 lignes de SUITE pour le portage, « Go carried them… », renvois à `gui_main.go`) |
| **Cause** | Go retiré en DEC-49 sans repasser sur la prose |
| **Fichiers** | les fichiers ci-dessus ; SUITE.md : l'historique du portage condensé en 3 lignes (résumé par phase dans DEC-43 ; journal complet : `git show b6cd5f8:docs/SUITE.md`) |
| **Règle** | une seule phrase d'histoire (« Go + Fyne jusqu'à la v0.2.9 ; retiré en DEC-49 ; `git show v0.2.9:<chemin>` ») ; les textes utiles restent (fixtures gelées = spécification, `powi(2)`, tableau des décisions) reformulés sans « Go » |
| **Vérification** | `scripts/check.sh` ; `git grep -n -E "\bGo\b\|Fyne"` sur ces fichiers ne rend que l'unique phrase d'histoire de chaque fichier touché, les fixtures citées comme telles et les deux commentaires de `build_support.rs` et `main.slint` |

### [2026-10-21] Le workspace Cargo remonte de `rust/` à la racine (plan « traces de Go », PR 1)

| | |
| --- | --- |
| **Constat** | `rust/` n'existait que parce que le code Go occupait la racine ; Go retiré (DEC-49), le dossier ne servait plus qu'à allonger chaque chemin et chaque commande |
| **Cause** | héritage de la cohabitation Go/Rust pendant le port |
| **Fichiers** | `git mv` de `Cargo.toml`, `Cargo.lock`, `crates/`, `deny.toml`, `rust-toolchain.toml` ; chemins relatifs de `build.rs`, `main.rs`, `repo_guards.rs`, `config/tests.rs` (`../../..` → `../..`) ; `rust.yml` et `release.yml` (`working-directory`, `workspaces`, `target/`, `lcov.info`) ; `dependabot.yml` (`directory: "/"`, sinon les mises à jour cargo s'arrêtaient sans erreur) ; `Makefile`, `scripts/checks/rust.sh`, `build_appimage.sh`, `.gitignore`, docs vivantes (les journaux gardent `rust/`, c'est l'histoire) |
| **PR** | Move the Cargo workspace from rust/ to the repository root |
| **Vérification** | `cargo clean` puis `scripts/check.sh` vert (un `target/` déplacé garde les anciens `CARGO_MANIFEST_DIR` : le test de parité ne trouvait plus sa fixture avant le `clean`) ; essai à blanc de `release.yml` à faire AVANT fusion (L-114) |

### [2026-10-21] La fenêtre reprend sa taille fixe (DEC-54, remplace DEC-51)

| | |
| --- | --- |
| **Constat** | essai de l'application par l'utilisateur : la fenêtre redimensionnable de DEC-51 « bouge », il préfère la taille fixe |
| **Cause** | choix de la 14ᵉ passe (F-27) pour les petits écrans, désormais écarté |
| **Fichiers** | `superview-app/ui/main.slint` (`min-` = `max-` à 980×470), `src/repo_guards.rs` (garde `the_window_has_one_size_so_it_cannot_be_resized`), `src/window_tests.rs` (`the_window_fits_its_smallest_size` retiré), `DECISIONS.md` (DEC-54, DEC-51 *Remplacée*, DEC-32), `ARCHITECTURE.md` |
| **PR** | Return the window to its fixed size |
| **Vérification** | contre-preuve : le garde est rouge sur le `main.slint` de `master` (« sets no window max-width »), vert après. Un premier essai par `set_size` dans `window_tests.rs` ne prouvait rien : le backend de test ne clampe pas, le test restait rouge même avec le correctif ; abandonné. **Test retiré par décision** : `the_window_fits_its_smallest_size`, sans objet sans minimum. À la main : fenêtre non redimensionnable à vérifier sous KDE/Wayland (winit applique min = max). **Limite** : écran de 910×512 logiques ou moins, fenêtre coupée (DEC-51 le corrigeait) |

### [2026-10-21] Documentation remise d'aplomb avec le code après les sessions 1 à 9 de la 14ᵉ passe

| | |
| --- | --- |
| **Constat** | audit de tous les `.md` vivants confronté au code, aux workflows et à `git log` : `cargo deny` absent de CONTRATS, ARCHITECTURE et du `Makefile` ; binaire « non strippé » de 24,8 Mo (DEC-50 : 18,4 Mo) ; recherche d'ffmpeg sans `SUPERVIEW_FFMPEG_DIR` ; `XDG_CONFIG_HOME` absent des README ; `0.43 GB` pour 0.42 ; `✅` pour `[OK]` ; « 473 crates » pour 463 ; skill `/publier` qui cherchait des runs `Test` et `Lint` inexistants (`Rust` et `Lint & Code Quality`) ; `SUITE.md` contradictoire (sessions en futur et au passé, ordre 9-8-7-6-1…5, phase 8 « à porter » alors que Go est retiré, « pas de champ texte de secours » alors que `dialogs.rs` en a un) ; DEC-50 absente de l'index, DEC-04 non amendée par DEC-41, DEC-35 citant `test.yml` ; L-115 et la fiche `pieges/` renvoyant à un test Go supprimé ; `AGENTS.md` citant L-114 pour l'environnement injecté ; dependabot « quotidien » et `gomod` dans RELEASING et `.github/release.yml` |
| **Cause** | chaque PR de la 14ᵉ passe mettait à jour le code et son entrée de journal, pas les documents de synthèse qui les citent ; la prose Go du retrait (DEC-49) n'avait pas été balayée dans les pointeurs vers des tests |
| **Fichiers** | `AGENTS.md`, `README.md`/`README_FR.md` (dont la taille de l'installateur), `RELEASING.md`, `THIRD_PARTY_NOTICES.md`, `docs/{ARCHITECTURE,CONTRATS,DECISIONS,ENVIRONNEMENT,LECONS,PIEGES,SUITE,TESTS-MANUELS}.md`, `docs/pieges/2026-10-14-tube-coupe-sortie-outil.md`, `.claude/skills/publier/SKILL.md`, `Makefile`, `scripts/checks/rust.sh`, `.github/release.yml` et un commentaire de `.github/workflows/release.yml` |
| **PR** | Bring the documentation back in line with the code after the fourteenth pass |
| **Vérification** | chaque écart a été constaté dans le code avant d'être corrigé (preuve citée par l'audit) ; 463 recompté par `cargo tree -p superview-app -e normal --target all` ; `scripts/check.sh` vert ; aucun code Rust touché. **Leçon L-117** écrite (environnement injecté), que `AGENTS.md` citait sous un mauvais numéro. **Volontairement laissés** : les liens vers `cmd/` et `internal/` des journaux historiques (`ANALYSE.md`, `archives/`), qui décrivent l'état d'avant DEC-49. **Essai à blanc (L-114)** : `release.yml` lancé sur la branche sans version (run 39573847465, commit `d9790e9`), vert : tests, builds Windows et Linux, `Create GitHub Release` sautée. Le même run a mesuré l'installateur Windows (artefact de 57 Mio) : « environ 95 Mo » devient « environ 60 Mo » dans les deux README |

### [2026-10-20] Entrée lue comme du MP4 et durée non finie refusée (Q-04 et Q-05, session 9)

| | |
| --- | --- |
| **Constat** | ffprobe et ffmpeg devinaient le format de l'entrée d'après son contenu : un « .mp4 » contenant un script `ffconcat` était lu et menait à un autre fichier local. Une durée `inf` ou `nan` passait `validate` (le `<= 0` de Go ne les attrape pas) : `inf` demandait 8 TB d'espace libre, `NaN` annulait le contrôle d'espace |
| **Cause** | pas de `-f` devant `-i` ; `duration_float <= 0.0` seul |
| **Fichiers** | `superview-core/src/video.rs` (`-f mov` dans `check_video_command`, `is_finite` dans `validate`), `encode/args.rs` (`-f mov` avant l'entrée source seule), `tests/parity_encode_args.rs` (`with_forced_demuxer`), `probe/tests.rs` (`-f` propre à la conversion), `video/tests.rs` ; DEC-52, DEC-53, `CONTRATS.md`, `AGENTS.md` |
| **PR** | Read the input as MP4 only and refuse a duration that is not a finite number |
| **Vérification** | contre-preuve rouge, cinq tests avant le correctif : l'argv d'ffprobe, `check_video_refuses_a_concat_script_named_mp4` (le script était accepté), la parité des 237 cas (`-f mov` absent), `validate_duration_limits` et `a_duration_of_inf_or_nan_in_ffprobes_json_is_refused` ; verts après. À la main avec `superview-harness convert` : un vrai MP4 se convertit, le script concat est refusé sans fichier de sortie. **Tests modifiés par décision** : `validate_duration_limits` (« NaN passe » devient « NaN, inf, -inf refusés »), l'argv attendu d'ffprobe, la comparaison sonde/conversion (`-f` propre à la conversion). **Fixture gelée non touchée** : l'écart est dans `with_forced_demuxer`. **Limite** : un MKV renommé en .mp4, converti avant, est maintenant refusé |

### [2026-10-20] Dépôt et CI : délais des jobs, `cargo-deny`, couverture publiée (F-28 à F-30, session 8)

| | |
| --- | --- |
| **Constat** | aucun `timeout-minutes` (défaut GitHub : 360 min) ; les cinq crates non maintenus n'étaient acceptés nulle part ; les licences n'étaient pas vérifiées contre GPL-3.0-or-later ; `THIRD_PARTY_NOTICES.md` annonçait 471 crates ; le seuil de couverture ne laissait aucune trace |
| **Cause** | jamais ajoutés ; `cargo audit` ne fait que des avertissements sur « non maintenu » et le commentaire de `rust.yml` renvoyait à un `audit.toml` absent |
| **Fichiers** | `.github/workflows/rust.yml` (60 min par job, étape `cargo deny`, artefact `lcov` de 30 jours), `lint.yml` (15 min), `rust/deny.toml` (nouveau), `Makefile` (`coverage-audit`), `docs/ENVIRONNEMENT.md`, `AGENTS.md`, `THIRD_PARTY_NOTICES.md` (473 crates). `release.yml` n'est pas touché (essai à blanc obligatoire, L-114) |
| **PR** | Bound the CI jobs in time, check advisories and licences with cargo-deny and keep the coverage report |
| **Vérification** | contre-preuve rouge : sans l'`ignore` de `ttf-parser`, `advisories FAILED` (« `ttf-parser` is unmaintained ») ; sans `Zlib` dans `allow`, `licenses FAILED` ; verts après. **Quatre exceptions seulement** (`ttf-parser`, `zvariant`, `zbus_names`, `zvariant_derive`) : `bincode` n'est dans `Cargo.lock` que par une dépendance optionnelle inactive, `cargo-deny` ne la voit pas. Toutes viennent du backend `winit` de Slint (1.18.1, la dernière) ; à rouvrir à chaque mise à jour de Slint (zbus 6 absorbe zvariant, `skrifa` remplace `ttf-parser`). **Piège** : `cargo-deny` 0.18.4 ne lit pas les scores CVSS 4.0 de la base d'avis (« unsupported CVSS version: 4.0 »), d'où 0.20.2. **Limite** : l'étape `cargo llvm-cov report --lcov` et l'artefact ne tournent qu'en CI ; leur première exécution est celle de cette PR |

### [2026-10-20] Interface de confort : « Cancelling... » stable, question Quit périmée, menu de qualité, Échap et Entrée, fenêtre redimensionnable (F-23 à F-27, session 7)

| | |
| --- | --- |
| **Constat** | une progression déjà en file remettait « Transforming... » sur « Cancelling... » ; la question « Cancel and quit » restait à l'écran quand la conversion finissait et le Done passait derrière ; la liste des profils de qualité existait en double (Slint et `prefs::QUALITY_PROFILES`) ; les dialogues ne répondaient pas au clavier ; la fenêtre était fixe à 980×470 px logiques, plus large qu'un 1366×768 à 150 % (910×512) |
| **Cause** | `WindowSink` ne regardait que `is_encoding()` ; `Popups::close` ne purgeait le `Quit` que derrière un autre popup ; modèle codé en dur dans `main.slint` ; le `FocusScope` de `Modal` n'avait pas de `key-pressed` ; `min-width == max-width` |
| **Fichiers** | `superview-app/src/state.rs` (`cancelling`, `shows_progress`), `app.rs` (`Popups::drop_quit`, appelé par `complete`, `set_quality_profiles`), `ui/main.slint` (propriété `quality-profiles`, callbacks `confirmed`/`dismissed` de `Modal`, `preferred-*` 980×470 et minimum 760×440, plus de maximum ; DEC-51 remplace la taille fixe de DEC-32), le `FocusScope` de `Modal` est le parent de la carte (une touche refusée par un bouton ou un champ remonte), `path-entry` répond à Échap |
| **PR** | Make the window resizable, answer dialogs to Escape and Enter and keep Cancelling and the quit question honest |
| **Vérification** | contre-preuve rouge pour chacun : `a_late_progress_does_not_overwrite_cancelling` (« Transforming... 60% » à la place de « Cancelling... »), `the_quit_question_goes_when_the_conversion_ends` (le Done n'arrive jamais), `the_quality_menu_is_the_rust_list` (propriété vide, et la fenêtre nue perdait « Balanced » : un nouveau modèle sélectionne sa première entrée, d'où l'ordre profils puis choix dans `wire` et `shown()`), `modals_answer_to_escape_and_enter` (handler neutralisé : « Enter did not close the dialog ») ; verts après. **Test modifié, par décision de F-24** : `popups_wait_their_turn` épinglait « le Done attend derrière la question » ; il attend maintenant que le Done prenne sa place. Entrée ne répond jamais à Overwrite ni à Quit (questions destructives : seul Échap, qui vaut « No »). **Limite** : `the_window_fits_its_smallest_size` tourne sur le backend de test, sans police, donc sans vraie mesure des textes renvoyés à la ligne ; la fenêtre à 910×512 n'a pas été vue sous xvfb |

### [2026-10-20] Petites corrections du cœur : limite cgroup, bitrates négatifs, recherche d'outils, permissions du journal (F-19 à F-22, session 6, PR B)

| | |
| --- | --- |
| **Constat** | la limite mémoire d'un cgroup n'était lue qu'à la racine (un `MemoryMax` d'unité ou de slice systemd était ignoré) ; `min_bitrate`/`max_bitrate` négatifs étaient acceptés sans avertissement ; l'override, la copie embarquée et les chemins Windows acceptaient un fichier sans bit exécutable et un `SUPERVIEW_FFMPEG_DIR` relatif ; un journal ou son dossier déjà existants gardaient leur mode large ; un échec de création du fichier de travail était étiqueté `encoding` |
| **Cause** | `available_memory_bytes` ne lisait que `/sys/fs/cgroup/memory.max` ; pas de normalisation des bitrates ; `usable_tool_file` n'exigeait que « pas un dossier » ; `mode()` ne s'applique qu'à la création |
| **Fichiers** | `superview-core/src/memory.rs` (`cgroup_limit_files` lit `/proc/self/cgroup`, `available_memory_bytes_under` prend le minimum de la racine à l'unité), `config.rs` (`normalize_bitrate`), `tools.rs` (`usable_tool_file` = `is_executable_file`, override relatif ignoré avec avertissement), `logfile.rs` (`tighten_permissions`), `pipeline.rs` (étiquette `partial_output`) |
| **PR** | Small core fixes: cgroup limits of parent units, negative bitrates, tool lookup and log permissions |
| **Vérification** | contre-preuve rouge pour chacun : `a_limit_on_a_parent_cgroup_caps_the_available_memory`, `negative_bitrates_are_treated_as_unset`, `a_tool_file_without_the_execute_bit_is_not_used`, `a_relative_override_directory_is_ignored`, `an_existing_loose_log_is_made_private` ; verts après. Les stand-ins Windows de `write_tool` (tests de la recherche winget/scoop, exécutés aussi sous Unix) reçoivent le bit exécutable : l'objet des tests (le raccourci `Links` l'emporte, etc.) est inchangé. **Non fait, par décision** : retirer `#[source]` de `SessionError` et `FailedAfterOutput` — `creation_failure_is_a_session_error_with_its_cause` et `the_stop_messages_match_the_go_text` épinglent `source().is_some()` (contrat `Unwrap` de Go), et le seul parcours de la chaîne du produit (`is_missing_ffmpeg_error`) cherche, il n'affiche pas ; aucun double affichage réel. **Sans test** : l'étiquette `partial_output` (l'échec de `new_partial_output_path` après une validation réussie ne se provoque pas sans root ni injection) |

### [2026-10-20] Sécurité et robustesse : chemin résolu transmis, `/dev` protégé, délai sur ffprobe et `ffmpeg -version`, attente bornée après la fin d'un processus (F-15 à F-18, session 6, PR A)

| | |
| --- | --- |
| **Constat** | ffprobe et ffmpeg recevaient le lien symbolique et non la cible validée ; une sortie `/dev/sda` passait en root ; ffprobe, `ffmpeg -version`, `-encoders` et `-hwaccels` n'avaient aucun délai (un partage réseau figé bloquait le fil) ; `run_with_deadline` attendait sans borne un processus terminé seul dont un petit-fils tenait le tube |
| **Cause** | `is_valid_input_path` ne rendait rien ; `/dev` absent de la liste ; `combined_output` et `split_output` sans délai ; `done_rx.recv()` dans la branche « terminé seul » |
| **Fichiers** | `superview-core/src/security.rs` (rend le `PathBuf` résolu, `/dev`), `pipeline.rs` (le chemin résolu va à `check_video` donc à l'argv), `process.rs` (`combined_output_within`, `split_output_within`, `READER_GRACE` dans les deux branches), `tools.rs` (`TOOL_QUERY_TIMEOUT` 30 s, `ToolResolver::query_timeout`), `video.rs`, `health.rs` (+ `trim_end` : le `\r` de `ffprobe -version`, part de F-22) |
| **PR** | Pass the resolved input path to ffmpeg, protect /dev and time-box ffprobe and ffmpeg queries |
| **Vérification** | contre-preuve rouge pour chacun : `the_tools_get_the_resolved_input_not_the_symlink` (pipeline remis sur `c.input`), `run_with_deadline_does_not_wait_for_a_child_that_keeps_the_pipe` (6 s au lieu de < 4 s), `check_video_gives_up_on_an_ffprobe_that_never_answers`, `a_hung_ffprobe_makes_its_check_fail`, `an_ffmpeg_query_that_never_answers_is_an_error` (30 s au lieu de ~1 s quand le délai est retiré), `dev_is_protected_and_run_media_is_not`, `the_ffprobe_version_line_has_no_carriage_return` ; verts après. Le délai est injecté (`with_query_timeout`, 1 s en test), aucun `sleep` long. **Écart au plan** : `/run` n'est pas protégé, il contient `/run/media/<user>/` (disques amovibles) ; `/proc` et `/sys` l'étaient déjà |

### [2026-10-20] Interface : sonde, Diagnostic et conversion sérialisés ; question Overwrite figée ; hook de panic (F-11 à F-14, session 5)

| | |
| --- | --- |
| **Constat** | la sonde de démarrage pouvait finir pendant un encodage et lancer une vérification GPU à côté (E-14) ; le Diagnostic ne grisait ni Start ni lui-même (deux rapports au double clic, une conversion lancée pendant le rapport) ; un sélecteur de sortie encore ouvert pendant la question « Overwrite » changeait la cible, et « Yes » écrasait un fichier dont on n'avait pas parlé ; un panic non rattrapé disparaissait sans trace dans le journal |
| **Cause** | `set_encoder_probe` et `verify_hardware_in_background` ne regardaient pas `encoding` ; `can_start` ignorait le Diagnostic ; `PopupKind::Overwrite` ne portait pas les fichiers de la question ; aucun `set_hook` |
| **Fichiers** | `superview-app/src/state.rs` (`pending_probe` appliqué par `finish_encoding`, qui garde le résumé du run ; `diagnosing`, `begin_diagnostic`, `finish_diagnostic`), `app.rs` (`Overwrite { input, output }`, `overwrite_yes`), `journal.rs` + `main.rs` (`panic_line`, `install_panic_hook`) |
| **PR** | Keep the start-up probe and the Diagnostic from running beside a conversion, freeze the Overwrite question on its files and log panics |
| **Vérification** | contre-preuve rouge avant chaque correctif : `a_startup_probe_that_lands_during_a_conversion_waits_for_its_end` (2 vérifications pendant l'encodage), `the_diagnostic_and_a_conversion_do_not_overlap` (« Start stayed available » ; « two reports were run » quand on retire la garde `diagnosing` de `begin_diagnostic`), `the_overwrite_question_is_about_the_files_it_was_asked_on` (conversion lancée sur b.mp4), `a_panic_line_names_the_thread_the_place_and_the_message` (rouge par mutation du format) ; verts après. Deux scénarios existants (`a_quit_question_that_waited…`, `a_dialog_queued_behind_the_report…`) lançaient Diagnostic et Start ensemble : ils attendent maintenant le rapport avant Start, leur objet (la file de popups) est inchangé. L'installation du hook, globale au processus, n'est pas testée |

### [2026-10-20] Sonde et cascade : preset de l'aperçu, annulation, cause du repli, échec de lancement (F-07 à F-10, session 4)

| | |
| --- | --- |
| **Constat** | l'aperçu matériel sondait avec un preset vide quand la conversion sonde avec celui du profil : deux lancements d'ffmpeg là où un seul répond ; Annuler n'atteignait pas la vérification à la taille de sortie (jusqu'à deux délais de sonde) ; « falling back to CPU decode » et « retrying with CPU encoder » ne disaient pas pourquoi ; un ffmpeg impossible à lancer était mis en cache comme un refus définitif du pilote |
| **Cause** | `describe_verified_hardware_plan` codait `""` ; `pipeline.rs` passait `None` comme annulation ; les `Err(_)` de la cascade jetaient l'erreur ; `probe_encoder_for_size_cached` ne distinguait pas « refusé » de « jamais lancé » |
| **Fichiers** | `superview-core/src/probe.rs` (paramètre `preset`, `probe_asked` renvoie si ffmpeg a été lancé), `pipeline.rs`, `encode/cascade.rs`, `superview-app/src/state.rs` (`VerificationRequest.quality`, un changement de profil relance la vérification), `engine.rs` (preset via `resolve_start_settings`) |
| **PR** | Verify the hardware preview with the conversion's preset, let Cancel reach the probe and log why a fallback happened |
| **Vérification** | contre-preuve rouge avant le correctif : `the_preview_and_the_conversion_share_one_question_to_the_driver` (2 lancements au lieu de 1), `cancel_reaches_the_verification_of_the_hardware_encoder` (15 s au lieu de < 5 s), `an_ffmpeg_that_cannot_be_started_is_not_cached` (cache non vide), `cascade_logs` (« no cause on … ») ; verts après. Le test d'état `the_verification_is_requested_only_when_it_can_be_answered` couvre le profil. Signature de `describe_verified_hardware_plan` changée : aucune mention en Markdown à mettre à jour |

### [2026-10-20] I/O capturée bornée : stderr de ffmpeg, stdout d'ffprobe à part, lignes de journal entières (F-04, F-05, F-06, session 3)

| | |
| --- | --- |
| **Constat** | le stderr de ffmpeg s'accumulait sans plafond puis partait en entier dans l'erreur, le journal et la popup ; `check_video` analysait stdout et stderr mélangés comme du JSON (un avertissement d'ffprobe cassait l'analyse) ; `writeln!` faisait deux `write`, et deux fils pouvaient coller leurs lignes |
| **Cause** | deux fils lecteurs dupliqués (`run.rs`, `process.rs`) qui poussaient dans un `Vec` ; `combined_output` hérité de `CombinedOutput` en Go ; verrou pris à chaque `write` du `writeln!` |
| **Fichiers** | `superview-core/src/process.rs` (`CappedBuffer`, `spawn_collector`, `split_output`, `CAPTURE_CAP` = 64 Kio à chaque bout, marqueur `[... n bytes dropped ...]`), `encode/run.rs`, `video.rs`, `logfile.rs` (`write_line`), `superview-app/src/journal.rs` |
| **PR** | Bound ffmpeg's captured stderr, parse ffprobe's stdout alone and keep log lines whole |
| **Vérification** | contre-preuve rouge avant le correctif : `cargo test -p superview-core --lib -- a_large_stderr check_video_parses` (2 rouges), `cargo test -p superview-app lines_logged` (« torn line: "" », 3 fois sur 3) ; verts après. **Deux tests existants changent de contrat, volontairement** : `run_with_deadline_reads_more_output_than_a_pipe_holds` exigeait 4 000 000 octets, il exige maintenant les deux bouts et le décompte exact des octets perdus ; `a_large_stderr_does_not_block_ffmpeg` (≥ 4 Mo) devient `…_and_is_bounded` (10 Mo en entrée, < 200 Kio en sortie, début et fin présents). Les deux gardent « ne bloque pas le processus ». `check_video_reports_a_failing_ffprobe_with_its_output` inchangé et vert |

### [2026-10-20] Profil release et cartes PGM non forcées sur le disque (F-02, F-03, session 2)

| | |
| --- | --- |
| **Constat** | pas de `[profile.release]` (binaire de 28,9 Mo) ; `generate_pgm` appelait `sync_all` sur 66 à 113 Mo de fichiers que seul le ffmpeg suivant lit |
| **Cause** | réglages par défaut de cargo jamais revus ; `sync_all` posé pour « voir » l'erreur de fermeture, que `BufWriter::into_inner` (qui vide le tampon) rapporte déjà |
| **Fichiers** | `rust/Cargo.toml`, `superview-core/src/pgm.rs`, `RELEASING.md`, DEC-50 |
| **PR** | Shrink the release binary with LTO and symbol stripping |
| **Vérification** | `cargo build --release` : 18 391 752 octets (28,9 Mo avant), marqueur `SUPERVIEW-BUILD` lisible ; `tests/pgm_golden.rs` inchangé et vert. Pas de test de contre-preuve : l'effet est une taille et une latence, pas un comportement |

### [2026-10-20] Les tests inline des gros modules vivent dans `tests.rs` (F-01, session 1)

| | |
| --- | --- |
| **Constat** | `state.rs` (2 387 lignes), `probe.rs`, `pipeline.rs`, `cascade.rs`… : les blocs `mod tests` faisaient de 60 à 97 % du fichier, lire le code coûtait la lecture des tests |
| **Cause** | tout était né en ligne, sans seuil de taille |
| **Fichiers** | 24 modules de `superview-core/src` et `superview-app/src` : le corps de `mod tests { … }` est devenu `<module>/tests.rs`, le fichier garde `#[cfg(test)] mod tests;` ; aucun code hors tests touché |
| **PR** | Move the inline tests of the large modules into tests.rs files |
| **Vérification** | `cargo test --workspace` : 443 (core) + 137 (app) avant et après, mêmes binaires d'intégration, zéro `ignored` de plus ; texte déplacé identique au blanc et à la virgule finale près (comparaison par empreinte) ; `scripts/check.sh` vert. Pas de contre-preuve rouge : refactor mécanique, le critère est l'égalité du compte |

### [2026-10-20] Retrait de Go (phase 8, DEC-49)

| | |
| --- | --- |
| **Constat** | depuis RC-0.3.0 le produit est le binaire Rust + Slint ; le code Go, ses CI et sa prose ne servaient plus qu'à tromper un lecteur (`go build`, `internal/common/`, Fyne) |
| **Cause** | le portage (DEC-43) devait finir par le retrait de Go, après un dernier GO |
| **Fichiers** | #136 (gardes Go portés en Rust, job *Rust coverage and audit*), #137 (code Go et jobs CI Go supprimés, icône sous `rust/crates/superview-app/ui/`), 8.3 : `AGENTS.md`, `docs/{ARCHITECTURE,CONTRATS,ENVIRONNEMENT,SUITE,PIEGES,DECISIONS}.md`, README, RELEASING, THIRD_PARTY_NOTICES |
| **PR** | #136, #137, docs/phase8-prose |
| **Vérification** | `scripts/check.sh` vert ; balayage `grep -rnE "go (build|test|vet)|gofmt|golangci|[Ff]yne|internal/common|cmd/superview"` : seules des mentions historiques explicites restent ; les journaux (`CORRECTIONS.md`, `LECONS.md`, `ANALYSE.md`, `archives/`) sont inchangés. Dernière version Go : v0.2.9 |

### [2026-10-20] Le Diagnostic ne montre plus les lignes `libva info:` à la place de l'erreur (Rust)

**Constaté** — Essai de `RC-0.3.0` sur un portable Tiger Lake : le refus de `h264_qsv` s'affichait « libva info: VA-API version 1.23.0; libva info: Trying to open …; libva info: Found init function … », la vraie erreur coupée par la limite de trois lignes de `summarize_probe_failure`.
**Correction** — Les lignes qui commencent par `libva info:` sont ignorées avant de garder les trois premières ; `libva error:` reste. Écart voulu avec Go (DEC-48, amendement). Test `libva_information_lines_do_not_crowd_out_the_error` (le refus QSV réel, le marqueur CQP derrière trois lignes de bruit, une ligne `libva error:` conservée, une sortie entièrement faite de bruit) ; contre-preuves : sans le filtre le test est rouge sur le message ci-dessus, un filtre trop large (`libva `) l'est sur la ligne d'erreur conservée.

### [2026-10-19] DEC-48 : un pilote VAAPI qui n'accepte que CQP est utilisé en qualité constante (Rust)

| | |
| --- | --- |
| **Constat** | E-01, option (b) : un Intel VAAPI sans firmware HuC (`supported modes: CQP`) était refusé par la sonde et la conversion retombait sur le processeur, alors que `-rc_mode CQP -qp N` produit un MP4 valide (mesuré le 2026-10-19) |
| **Cause** | la sonde et la conversion ne savaient demander qu'un débit |
| **Fichiers** | `rust/crates/superview-core/src/{encode/args.rs,encode/cascade.rs,probe.rs,pipeline.rs}`, `rust/crates/superview-core/tests/{parity_encode_args.rs,parity_probe_args.rs,cascade_logs.rs,pipeline_constant_quality.rs (neuf)}`, `rust/crates/superview-app/src/report.rs`, DEC-48, `docs/{CONTRATS,hardware-support,ANALYSE,TESTS-MANUELS}.md` |
| **PR** | feat/rust-vaapi-constant-quality |
| **Vérification** | oracles Go `encode_args.json` (237 cas) et `probe_args.json` verts sans régénération ; tests d'argv, de question identique (rejoué en CQP), de détection (VAAPI seulement, raison exacte), de pipeline (CQP puis repli CPU en débit), du Diagnostic ; contre-épreuve de 13 défauts réintroduits |

### [2026-10-18] Phase 6, PR 6.4 : de vrais sélecteurs de fichiers, l'icône de la fenêtre et la checklist manuelle

| | |
| --- | --- |
| **Constat** | les sélecteurs de la fenêtre Rust étaient ceux d'essai (rfd seul) : sans portail ni zenity, sous KDE avec kdialog seulement, le bouton ne faisait rien ; la fenêtre n'avait pas d'icône |
| **Cause** | rfd renvoie `None` pour une annulation comme pour l'absence de tout sélecteur, et ne connaît pas kdialog |
| **Fichiers** | `rust/crates/superview-app/{src/dialogs.rs (neuf),src/app.rs,src/host.rs,src/app_tests.rs,src/window_tests.rs,ui/main.slint,build.rs}`, `docs/TESTS-MANUELS.md` (neuf), DEC-43 |
| **PR** | feat/rust-pickers |
| **Vérification** | tests unitaires de `dialogs.rs` (seuil 299/300 ms, ordre, arguments, code 1 = annulé, recherche dans `PATH`) et 4 scénarios de fenêtre (champ texte entrée, sortie, annulation, file de popups) ; contre-épreuve de 27 défauts réintroduits, tous rougis. Icône vérifiée sous Xvfb (`_NET_WM_ICON` 1024x1024), sans test automatique. Non vérifié : portail réel, kdialog réel, Windows réel (voir la checklist) |

### [2026-10-17] Phase 6, PR 6.2 : la vraie fenêtre Slint, sans câblage

| | |
| --- | --- |
| **Constat** | le port Rust n'avait que la fenêtre d'essai (un bouton, un champ) |
| **Cause** | la fenêtre complète (libellés Go verbatim, barre de 6 boutons centrée, popups) restait à écrire |
| **Fichiers** | `rust/crates/superview-app/{ui/main.slint,src/window_tests.rs,build.rs}`, `main.rs` (test renommé `window_scenarios`) |
| **PR** | feat/rust-ui-window |
| **Vérification** | contre-épreuve de chaque test ; relecture : liaisons `in-out` (case et listes), modale qui verrouille aussi le clavier, une seule popup à la fois (enum `Popup`). Non prouvé : le `TouchArea` de la modale seul, `char-wrap`, le rendu réel (pas de Xvfb) |

### [2026-10-17] Phase 6, PR 6.1 : la logique de la fenêtre portée en Rust, sans fenêtre

| | |
| --- | --- |
| **Constat** | `appState` et les fonctions de `gui_main.go` n'existaient que sous Fyne ; le port Rust n'avait que la fenêtre d'essai |
| **Cause** | la logique (états, textes, préférences) était liée aux widgets, donc intestable sans eux |
| **Fichiers** | `rust/crates/superview-app/src/{state,format,prefs}.rs` (neufs), `main.rs`, `selection.rs` supprimé |
| **PR** | feat/rust-gui-state |
| **Vérification** | 92 tests ; contre-épreuve faite pour chaque groupe (génération non avancée, préférence écrasée par un repli, Diagnostic hors de la liste verrouillée, `done` non libéré sur panique, etc.) · écarts voulus avec Go dans DEC-43 et DEC-46 |

### [2026-10-14 et avant] Corrections de l'ère Go — archivées
Les entrées du 2026-09-04 au 2026-10-14 (U-01 à U-14, M-01 à M-06, E-01 à E-27, P-, N-, B-, R-05, passes 2, 3 et 13, DEC-36, DEC-41…) sont dans [archives/CORRECTIONS-ere-go.md](archives/CORRECTIONS-ere-go.md), texte inchangé (DEC-55).

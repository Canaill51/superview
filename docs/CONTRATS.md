# Contrats techniques du dépôt

> **À lire avant d'ouvrir un fichier Rust de ce dépôt.** Ce fichier dit ce sur
> quoi on peut s'appuyer et ce qu'il ne faut pas casser. Il est maintenu ; les
> deux journaux ([ANALYSE.md](ANALYSE.md), [LECONS.md](LECONS.md)) racontent
> comment on en est arrivé là.
>
> Le code Go a été retiré (DEC-49) ; ses journaux parlent encore de `internal/common/`, de
> `cmd/superview/` et de Fyne : ils décrivent l'état d'avant. Les faits FFmpeg ci-dessous ont
> été mesurés au temps de Go et valent tels quels pour le Rust, qui lance les mêmes binaires.
> Pour installer la chaîne d'outils sur un poste sans `sudo`, voir
> [ENVIRONNEMENT.md](ENVIRONNEMENT.md).

---

## 1. Sources internes au dépôt — par ordre de priorité

| # | Source | Quand la consulter | Fiabilité |
| --- | --- | --- | --- |
| 1 | tests unitaires (`#[cfg(test)]`) à côté du code, dans `crates/*/src/` | **Avant de modifier une fonction** : le test décrit le contrat attendu. | fait foi |
| 1bis | `crates/superview-core/tests/pgm_golden.rs` | **Avant de toucher à `generate_pgm`** : empreintes SHA-256 figeant la sortie exacte de l'algorithme. | fait foi |
| 1ter | `superview-core/tests/real_ffmpeg.rs` et les tests à vrai ffmpeg de `src/pipeline.rs` | Contrat bout-en-bout avec un vrai FFmpeg (profondeur, pistes, date, annulation). | fait foi |
| 1quater | `superview-core/tests/fixtures/ffprobe/` et `fixtures/recorded/` | Sorties ffprobe enregistrées (la forme exacte de la seule entrée que le programme ne produit pas lui-même) et fixtures enregistrées (`decisions.json`, `encode_args.json`, `memory.json`, `pgm.json`, `probe_args.json`, `specs.json`). Enregistrés sur le code Go, **gelés** (DEC-49). | fait foi |
| 2 | `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml` | Versions exactes (Slint et `i-slint-backend-testing` en `=`) avant toute modification d'API. | fait foi |
| 3 | `scripts/checks/rust.sh` | Ce que le pre-commit exige : `cargo fmt --check`, `cargo clippy -D warnings`, tests avec `SUPERVIEW_REQUIRE_FFMPEG=1`. | fait foi |
| 4 | `.github/workflows/*.yml` | Ce qui est réellement vérifié en CI : `rust.yml` (Linux, Windows, et *Rust coverage and audit* = seuil de 50 % de lignes + `cargo audit` + `cargo deny` sur les avis et les licences), `lint.yml` (parité des README). | fait foi |
| 5 | `RELEASING.md` | **La** procédure de release. Il n'y en a plus d'autre, et son § *Bumping the bundled FFmpeg* dit pourquoi le pin est ce qu'il est. | fait foi |
| 5bis | `.github/scripts/nvenc-driver-floor.sh` | Le plancher pilote NVENC que la release accepte de livrer, lu dans le binaire. | fait foi |
| 6 | `superview.yaml` | Options de configuration effectivement livrées. Le fichier livré et le défaut interne (`Config::default`) concordent (`performance_mode: safe_performance`, DEC-33) ; garder les deux alignés. |  |
| 7 | [`ANALYSE.md`](ANALYSE.md) | Constats numérotés B/S/C/X/O/T/N/P/R/D/V/U/M/E et leur état ; le détail des passes 3 à 13 est dans [`archives/`](archives/ANALYSE-passes-3-a-13.md). | journal, voir son en-tête |
| 8 | [`LECONS.md`](LECONS.md), [`CORRECTIONS.md`](CORRECTIONS.md), [`PIEGES.md`](PIEGES.md) | Leçons permanentes ; historique des corrections appliquées ; index des leçons par mot-clé. | journal, voir son en-tête |
| 8bis | [`DECISIONS.md`](DECISIONS.md) | **Avant de « corriger » ce qui ressemble à un défaut** : les choix du dépôt (DEC-01 à DEC-49), chacun avec sa raison et la condition qui le rouvrirait. | fait foi, tant que la décision n'est pas marquée *Remplacée* |
| 9 | `README.md` et `README_FR.md` | Comportement documenté côté utilisateur. Un seul document en deux langues : toute modification visible par l'utilisateur se fait dans les deux, dans la même PR. Le job `readme-parity` de `lint.yml` le vérifie. |  |
| 10 | `Makefile` | Cibles locales : `check`, `build`, `test` (pose `SUPERVIEW_REQUIRE_FFMPEG=1`), `coverage-audit`, `fmt`, `clippy`. |  |
| 11 | [`AGENTS.md`](../AGENTS.md) | Conventions du projet pour les agents, et les pièges qui coûtent le plus cher. `CLAUDE.md` ne fait que l'importer : Claude Code sait lire `AGENTS.md`, mais pas dans toutes ses sessions, et l'import le charge partout. `.claude/` porte les autorisations, les crochets et la compétence `/verify`. |  |

> Les entrées `build.sh`, `tools/` et `coverage.out` ont disparu de cette table avec
> les fichiers eux-mêmes. `.github/copilot-instructions.md` aussi : son contenu vit
> désormais dans `AGENTS.md`, que Copilot lit nativement.

### Fichiers à traiter avec la plus grande prudence

- **`superview-core/src/pgm.rs`** — `generate_pgm` et la géométrie de sortie : mathématiques du remappage,
  ne pas « simplifier » sans vidéo de référence (DEC-13).
- **`superview-core/src/encode/cascade.rs`, `encode/run.rs`** — `encode_video` : cascade de repli à trois
  niveaux ; `run_ffmpeg` : un passage, course fin/annulation/signal. Toute modification touche l'encodage réel.
- **`superview-core/src/pipeline.rs`** — `perform_encoding` : l'ordre des validations, le fichier de travail
  et son renommage (DEC-15).
- **`superview-app/src/app.rs` et `state.rs`** — `app.rs` câble des widgets, ce que seuls les tests de
  fenêtre traversent ; les *décisions* sont dans `AppState` (`state.rs`), sans type Slint, et sont couvertes.
- **`superview-core/src/security.rs`** — modifier une validation, c'est modifier une frontière de sécurité.
  Justifier chaque assouplissement.

---

## 2. Règles de vérification propres à ce dépôt

### Avant de modifier

1. Lire [`LECONS.md`](LECONS.md) — la correction a peut-être déjà été tentée et rejetée.
2. Lire le test correspondant (module `tests` du fichier, ou `superview-core/tests/`).
3. Vérifier que le symbole n'est pas du code mort (constats C-01, C-02, C-03, C-06) :
   ```bash
   grep -rn "nom_du_symbole" --include='*.rs' crates/ | grep -v '/tests/'
   ```
   Deux occurrences seulement (déclaration + commentaire) = jamais appelé (`cargo clippy` signale aussi le code mort).

### Après avoir modifié

```bash
scripts/check.sh                 # tout : fmt, clippy, tests ; --fast s'arrête avant les tests

# ou porte par porte, depuis la racine (c'est la recette de la CI)
cargo fmt --all --check          # doit ne rien afficher — la CI échoue sinon
cargo clippy --workspace --all-targets --locked -- -D warnings
SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked

make coverage-audit              # seuil 50 % de lignes (cargo llvm-cov), cargo audit puis cargo deny
```

C'est ce que la CI exécute, et rien de plus : un outil lancé à côté de la configuration finit par
la contredire (L-62).

`SUPERVIEW_REQUIRE_FFMPEG=1` transforme en échec les tests qui sautent faute d'ffmpeg. Sans elle,
une suite verte peut n'avoir encodé aucune image ; la CI la positionne pour cette raison.

> Les sorties de couverture (`cargo llvm-cov`) vont sous `target/`, ignoré par git : ne pas
> écrire de profil à la racine, un fichier généré à la racine reste du bruit.

### Vérification fonctionnelle réelle

Une compilation verte ne prouve pas que la GUI démarre ni que la conversion fonctionne :

```bash
cargo build -p superview-app --locked
timeout 12 target/debug/superview            # code 124 = toujours vivant, donc OK
# sans session graphique : env -u WAYLAND_DISPLAY xvfb-run -a timeout 12 target/debug/superview
cat ~/.cache/superview/superview.log          # les diagnostics y atterrissent (O-02)

SUPERVIEW_REQUIRE_FFMPEG=1 \
  cargo test -p superview-core --test real_ffmpeg   # conversion réelle 4:3 → 16:9
```

### Validation fonctionnelle sans vidéo réelle

```bash
# Génère un clip 4:3 de 5 s pour un test bout-en-bout
ffmpeg -f lavfi -i testsrc=size=640x480:rate=30 -t 5 -c:v libx264 -b:v 1M /tmp/in43.mp4
ffprobe -v error -select_streams v:0 \
        -show_entries stream=codec_name,width,height,duration,bit_rate \
        -print_format json /tmp/in43.mp4
```

Le second appel reproduit celui de `check_video` : il permet de vérifier qu'un fichier passera
la validation avant de lancer la GUI. Pour éprouver le *parsing* de cette sortie sans produire
de vidéo, les cas sont enregistrés dans `superview-core/tests/fixtures/ffprobe/` et couverts par
`parse_video_specs` — y compris les rejets (durée `N/A`, `bit_rate` absent, JSON tronqué).

### Contrat de configuration

Il n'existe pas de configuration globale. `&Config` se passe explicitement à `check_ffmpeg`,
`EncodingSession::new`, `encode_video`, `perform_encoding` et `check_health` ; `None` (là où la
signature prend `Option<&Config>`) signifie « valeurs par défaut » (`Config::default`).

> N'introduisez pas de nouveau global mutable pour contourner ce passage de paramètre : c'est
> précisément ce que la refactorisation C-05 a supprimé, et la cause de l'écrasement silencieux
> du `video_preset` utilisateur. Il en va de même du résolveur d'outils, du `Prober`, de
> l'horloge, de l'environnement et de la lecture de la mémoire : injectés, jamais lus en global.

Règle de priorité du preset : un `video_preset` renseigné dans la configuration l'emporte sur
celui du profil qualité de la fenêtre. Testée par `a_configured_preset_wins_over_the_quality_profile`
(`superview-app/src/format.rs`).

### Contrat FFmpeg — faits établis par mesure (4ᵉ passe, 2026-09-04)

Vérifiés en exécutant FFmpeg 8.0.1, pas déduits de la documentation. À traiter comme acquis.

| Fait | Conséquence |
| --- | --- |
| `-filter_complex` sans `-map` ne conserve **qu'une piste audio** | Il faut `-map "[v]" -map "0:a?"`. Le `?` est **obligatoire** : sans lui, une source muette fait échouer FFmpeg. |
| Avec trois entrées, les métadonnées globales sont **perdues** | Il faut `-map_metadata 0` pour garder `creation_time`. |
| `format=yuv444p,format=yuv420p` **ramène le 10 bits à 8 bits** | Les variantes `yuv444p10le,yuv420p10le` préservent la profondeur — vérifié : `yuv420p10le` en entrée ressort `yuv420p10le`. |
| Le filtre `remap` accepte le **PGM P5 binaire** 16 bits | Sortie **identique au bit près** au P2 ASCII (même SHA-256 sur les trames décodées), pour 55 % d'empreinte en moins. |
| Le PGM binaire est **gros-boutiste** par spécification | Un écrivain petit-boutiste produirait des cartes fausses **sans erreur**. |
| `-b:v` seul est une cible **moyenne**, sans plafond | Dépassement mesuré à +56 % (`libx264`). `-maxrate` et `-bufsize` sont nécessaires. |
| Une carte de remappage d'une seule trame est réutilisée pour toutes les trames | Pas besoin de boucler l'entrée. |
| **`libx265` refuse `-threads` au-delà de 16** | FFmpeg le fait correspondre à `--frame-threads` de x265. 17 et plus : `Cannot open libx265 encoder`, échec dur. Le nombre de cœurs ne peut donc pas être passé tel quel — voir `clamp_encoder_threads` (`encode/args.rs`, P-11). |
| **`-b:v` seul dépasse jusqu'à +83 %** | Mesuré sur bruit incompressible à 8 Mbps : 14,7 Mbps sans plafond, 10,0 Mbps avec `-maxrate` égal à la consigne et `-bufsize` au double (+24 %), 10,4 Mbps avec la recommandation habituelle de 1,5× (+29 %). Le plafond retenu est **1×** — la valeur « standard » était la moins bonne. |
| Un « .mp4 » qui contient `ffconcat version 1.0` + `file clip.mp4` est **lu comme un script concat** par ffprobe et ffmpeg ; une playlist HLS sous le même nom est refusée par ffmpeg 8 | `-f mov` devant l'entrée source (DEC-52) : le script échoue (`moov atom not found`), un vrai MP4 passe. Mesuré avec ffmpeg 8.0.1 (celui du poste, pas les 8.1.x livrés : le test `check_video_refuses_a_concat_script_named_mp4` tourne sur celui de la machine) ; pas devant les cartes PGM. |
| `ffprobe` donne la cadence en rationnel (`30000/1001`) | Et `0/0` quand il ne sait pas ; `parse_frame_rate` rend 0, que tous les appelants lisent comme « inconnu ». |
| **Le gain de l'accélération matérielle dépend fortement du débit de la source** | Sur une mire à 2 Mbps : décodage +3 %, encodage ×1,9. Sur une source type GoPro à 127 Mbps : décodage **+9,9 %**, encodage **×3,18**. Mesurer le matériel sur une mire mène à la conclusion inverse (N-07, L-46). |
| Rapatrier les trames du GPU vers la RAM annule presque le gain du décodage matériel | En transcodage simple à 127 Mbps : +7 % si les trames restent en VRAM, **+1 %** si elles redescendent. `remap` étant un filtre CPU, Superview est forcément dans le second cas — et y gagne pourtant +9,9 %, vraisemblablement en libérant du CPU pour le filtre. |
| Le filtre `remap` accepte des cartes 16 bits pour une source 10 bits | La profondeur des cartes et celle de la vidéo sont indépendantes. |

Recette de vérification d'un changement de chaîne de filtres — comparer la **sortie décodée**,
jamais le fichier de sortie (les en-têtes de conteneur diffèrent toujours) :

```bash
ffmpeg -v error -i avant.mp4 -f rawvideo -pix_fmt yuv420p - | sha256sum
ffmpeg -v error -i apres.mp4 -f rawvideo -pix_fmt yuv420p - | sha256sum
```

> ⚠️ **Ne jamais figer une de ces empreintes dans un test.** Elle dépend du build de l'encodeur
> et rougirait sur un autre runner pour une raison étrangère au code. Le patron correct est
> `the_binary_maps_remap_like_the_ascii_ones` (`tests/real_ffmpeg.rs`) : comparer **deux exécutions du FFmpeg présent** plutôt
> qu'inscrire le résultat de l'un d'eux. Voir [LECONS.md](LECONS.md) L-36.

### Contrat de résolution des outils (depuis U-04, 2026-09-06)

`ToolResolver::resolve` (`tools.rs`) cherche `ffmpeg` et `ffprobe` dans cet ordre, et **l'ordre est le contrat** :

1. `SUPERVIEW_FFMPEG_DIR`, la sortie de secours explicite de l'utilisateur ;
2. la copie livrée avec la version (installateur Windows, AppImage Linux) ;
3. le `PATH` ;
4. sous Windows, les répertoires d'installation de winget et scoop.

> **La copie empaquetée passe avant le `PATH`, délibérément.** Le FFmpeg installé sur la machine
> est la variable que ce programme ne contrôle pas et ne peut pas inspecter à l'avance. Ne pas
> inverser cet ordre pour « prendre le plus récent » : c'est précisément le plus récent qui a
> retiré NVENC à une RTX A1000 (U-03).

Deux dispositions sur disque, parce que les deux paquets ne sont pas faits pareil :

| Plateforme | Emplacement des outils | Pourquoi |
| --- | --- | --- |
| Windows | à côté de l'exécutable | L'installateur dépose les trois fichiers dans un dossier unique. |
| Linux | `../lib/superview/` relatif à l'exécutable | Dans l'AppImage : `usr/bin/superview` et `usr/lib/superview/ffmpeg`. Un `ffmpeg` déposé à côté du binaire atterrirait, pour toute installation sous `$PREFIX/bin`, dans `/usr/local/bin` et **masquerait celui du système pour toute la machine**. |

Le chemin Linux vaut pour l'AppImage montée comme pour toute installation, puisqu'il est
relatif au binaire.

**Ce qui est épinglé est le plancher pilote, pas la version.** `FFMPEG_DRIVER_FLOOR` vaut `570.0`
dans les deux jobs de release, et `.github/scripts/nvenc-driver-floor.sh` le relit dans le binaire
téléchargé. Monter le pin vers un build compilé contre des en-têtes plus récentes retire
l'encodage matériel à toutes les machines sous le nouveau plancher — voir [RELEASING.md](../RELEASING.md).

### Contrat des capacités matérielles (depuis U-03, 2026-09-06)

**`ffmpeg -encoders` et `ffmpeg -hwaccels` ne décrivent pas la machine.** Ils décrivent le
binaire : ce avec quoi il a été compilé. Aucun des deux ne voit le pilote. Ne jamais rétablir une
décision de capacité fondée sur eux seuls.

La seule source de vérité est la sonde de `superview-core/src/probe.rs` : un encodage de quelques trames (`PROBE_DURATION`, huit depuis DEC-67) par
encodeur, dont le code de retour fait foi. `apply_encoder_probe` retire ensuite du profil ffmpeg
les encodeurs qui ont refusé — **et uniquement ceux-là** : un encodeur non sondé n'est pas un
encodeur refusé.

| Fait | Conséquence |
| --- | --- |
| Sur la machine de développement, six encodeurs annoncés ne peuvent ouvrir aucun périphérique (`h264_qsv`, `hevc_qsv`, `h264_vaapi`, `hevc_vaapi`, `h264_v4l2m2m`, `hevc_v4l2m2m`) | L'écart entre « annoncé » et « utilisable » est la règle, pas l'exception. |
| Un encodeur VAAPI n'accepte que des trames déjà sur le périphérique | Sa sonde exige `-init_hw_device vaapi=…`, `-filter_hw_device` et `-vf format=nv12,hwupload`. Sans cela elle échoue dans le graphe de filtres, avant le pilote, et condamne un encodeur qui fonctionne. |
| FFmpeg imprime la cause en **premier** et les conséquences ensuite | Un résumé qui garde la fin de la sortie garde « Error opening output files » et jette « Unknown encoder 'x' ». |
| Le plancher pilote NVENC est fixé à la compilation par les `nv-codec-headers` | Deux binaires « FFmpeg 8.1.2 » peuvent exiger 570 et 610. Le numéro de version ne dit rien de la compatibilité. Table mesurée : [hardware-support.md](hardware-support.md). |
| Le SDK 13.1 exige un pilote ≥ 610 et retire Maxwell/Pascal/Volta | La branche pilote RTX Enterprise plafonne à 597.06 : un tel build ne peut jamais utiliser NVENC sur une carte professionnelle. |
| La sonde complète coûte ~0,95 s sur 10 encodeurs (243 ms pour un NVENC qui répond, ~35 ms pour un refus) | Elle tourne sur un fil de démarrage ; elle n'a rien à faire sur le fil de l'interface. |
| Un pilote Intel VAAPI sans firmware HuC répond `Driver does not support any RC mode compatible with selected options (supported modes: CQP).` (chaque mode est précédé d'une espace : un pilote à plusieurs modes écrit `supported modes: CQP CBR VBR)`, ce qui ne déclenche pas la seconde question) à tout `-b:v`. Mesuré le 2026-10-19 (FFmpeg embarqué, Tiger Lake, `/dev/dri/renderD128`) : `-rc_mode CQP -qp 23` (`h264_vaapi`) et `-qp 25` (`hevc_vaapi`), avec `-vf format=nv12,hwupload`, produisent un MP4 valide | Le Rust (DEC-48) utilise ce pilote en qualité constante : mode porté par `RateControl`, sonde et conversion identiques. Go (retiré) le refusait. QSV n'est pas concerné : son échec est le firmware. |

Les sondes s'exécutent **en série**. Les encodeurs matériels ont un nombre limité de sessions
simultanées, et deux sondes en parallèle sur le même GPU peuvent faire échouer un encodeur qui
fonctionne.

### Contrat de la chaîne de filtres (depuis la 4ᵉ passe)

`build_encode_base_args` (`encode/args.rs`) doit émettre, dans cet ordre, après les trois `-i` :

```
-filter_complex "[0:v:0][1:v:0][2:v:0]remap,format=<inter>,format=<sortie>[v]"
-map "[v]" -map "0:a?" -map_metadata 0
```

- Le label `[v]` et les trois options qui suivent forment un tout : retirer l'un casse les deux
  autres. Sans eux, une piste audio et la date de prise de vue disparaissent (N-04, N-05).
- Le `?` de `0:a?` rend la sélection audio facultative — sans lui, une source muette échoue.
- `<inter>`/`<sortie>` valent `yuv444p10le`/`yuv420p10le` **uniquement** si la source dépasse
  8 bits *et* l'encodeur est de la famille HEVC (`remap_filter_chain`). `h264_nvenc` ne sait pas
  encoder en 10 bits, et le High 10 de `libx264` se lit mal (N-03).
- Un `pix_fmt` vide ou non reconnu vaut 8 bits : c'est la direction qui préserve l'existant.
- **Encodeurs à trames sur périphérique** (VAAPI, Vulkan, D3D12, depuis U-05) : la chaîne se
  termine par `,format=nv12,hwupload` — `,format=p010,hwupload` en 10 bits, les pools matériels
  étant semi-planaires — et `-init_hw_device <type>=sv -filter_hw_device sv` est émis **avant
  les `-i`**, avec les autres options globales. Après `-i`, ces options ne configurent rien et
  l'upload échoue en cherchant un périphérique jamais créé. `remap` étant un filtre CPU, les
  trames sont en mémoire système à ce point quel que soit l'encodeur : l'upload s'ajoute après,
  donc il ne déplace aucun pixel.
- **Le montage vient de `hw_device_args` et `hw_upload_filters` (`hardware.rs`), que la sonde emploie aussi.** Ne pas
  dupliquer : une sonde qui ouvrirait un périphérique que la conversion n'ouvre pas passerait
  puis échouerait à l'encodage ; l'inverse condamnerait un encodeur qui fonctionne.
  `probe_and_conversion_ask_the_same_question` (`probe.rs`) compare les deux.

### Contrat du pipeline (depuis les correctifs de la 4ᵉ passe)

Invariants à ne pas casser, chacun tenu par un test :

| Invariant | Où | Pourquoi |
| --- | --- | --- |
| L'annulation se transporte par `Error::Cancelled` (et `is_stop_request()` la lit à travers `FailedAfterOutput`) | `encode_video` | Une erreur non typée relançait la cascade de repli : 3 ffmpeg au lieu de 0 après un Cancel |
| L'encodage écrit dans un fichier de travail, renommé en place au succès | `perform_encoding` | Sinon une annulation laisse un `.mp4` tronqué à destination et détruit le fichier précédent |
| L'entrée ne peut pas être la sortie | `same_output_as_input` (`output.rs`) | **Conséquence directe du point précédent** : ffmpeg ne voit plus le conflit, rien n'empêcherait le renommage d'écraser la source |
| Les débits sont en **bits**/seconde | partout | ffprobe `bit_rate` et ffmpeg `-b:v` sont en bits ; le code était juste, les commentaires faux |
| `-threads` n'est pas émis pour un encodeur matériel, et est plafonné à 16 pour `libx265` | `encoder_thread_args` | Au-delà de 16, `libx265` refuse de s'ouvrir (P-11) |
| La géométrie des cartes est calculée par `remap_output_size`, jamais recopiée | `generate_pgm`, `check_temp_space_for_maps` (`disk.rs`) | Deux calculs divergent ; la vérification d'espace réserverait pour une carte qui n'est pas celle qu'on écrit |
| Une cadence d'images inconnue laisse la vitesse d'encodage à zéro | `metrics.rs` | Une métrique dérivée d'une constante inventée trompe plus qu'une métrique absente |

> **Écrire un test d'annulation** : lever le drapeau *avant* l'appel ne teste rien — ffmpeg est
> tué avant d'exécuter sa première ligne, donc avant de créer son fichier. Déclencher
> l'annulation depuis le **rappel de progression** (modèle :
> `a_real_encode_cancelled_from_the_progress_callback_leaves_nothing_behind`, `pipeline.rs`), et
> observer depuis l'extérieur du processus qu'on va tuer. Voir L-40 et L-41.

### Contrat de l'état de la fenêtre (depuis P-10)

Tout l'état de session vit dans `AppState` (`superview-app/src/state.rs`), sans type Slint, et **rien ne
doit revenir en variable capturée dans les callbacks de `app.rs`** : c'est ce qui rendait les décisions
inatteignables par les tests, et c'est là que P-01 et P-02 s'étaient logés.

| Règle | Pourquoi |
| --- | --- |
| `begin_encoding()` **retourne** le drapeau d'annulation et le `Done` ; le fil de conversion ne relit jamais l'état pour les retrouver. Son corps est `run_encoding`, qui libère `done` **avant** de remettre le résultat à `complete` (et par `Drop` en cas de panique) | La relecture était la course P-02. La signature est ce qui l'empêche de revenir (L-44). `done` est ce que `shutdown` attend : après le vidage de la boucle, `complete` s'exécute en ligne sur ce fil et peut mener à `shutdown`, qui attendrait le fil même qui l'appelle (E-03, L-110) |
| `Lifecycle::shutdown` n'attend qu'une fois : une attente expirée rend `false` aussitôt aux appels suivants | Le crochet de fermeture et la ligne après la boucle d'événements l'appellent tous deux ; sans cela l'attente réelle était le double de la limite |
| `shutdown(reason)` est la seule sortie : annule, attend `done` (borné), et chaque chemin y passe — fermeture de la fenêtre, fin de la boucle, SIGINT/SIGTERM | Un signal ne ferme aucune fenêtre tout seul (L-109) ; une pause avant de quitter n'est pas une attente (L-102) |
| `complete_encoding(…)` est la seule branche de fin, et se tait quand `is_shutting_down()` | La complétion peut tourner après la destruction de la fenêtre (E-14, L-109) |
| `Lifecycle::cancel()` est le seul endroit qui lève le drapeau, et le prend (`Option::take`) dans le même geste ; `request_cancel()` (bouton *Cancel*) et `shutdown` y passent tous deux | Le bouton *Cancel* et la fermeture peuvent tous deux tirer ; la seconde demande ne fait rien |
| Un contrôle qui doit être inerte pendant une conversion va dans `locked()`, jamais dans une séquence d'activations écrite à la main. Sans ffmpeg, c'est `requires_ffmpeg()` qui est verrouillé : la même liste **sans** *Diagnostic* ni *Quit* | Deux séquences parallèles divergent toujours — c'est P-01, et le sélecteur de codec oublié dans le chemin « ffmpeg indisponible ». Une seule liste pour deux raisons de verrouiller rendait inaccessible l'outil qui signale l'absence de ffmpeg |
| Le résultat d'un sélecteur de fichier passe par `apply_chosen_input` / `apply_chosen_output`, qui l'ignorent pendant une conversion | Les sélecteurs tournent hors du fil de l'interface et la fenêtre reste active : le résultat peut arriver après *Start* |
| Tout widget touché depuis un autre fil passe par `upgrade_in_event_loop` ; un panic d'un fil de travail devient une erreur affichée, jamais une fenêtre verrouillée (DEC-43, 6.3) | Slint ne se touche que depuis son fil ; un fil mort sans message laisse l'interface inerte |
| Toute nouvelle transition d'état est une **méthode** de `AppState`, avec son test | Ne pas laisser ce filet se percer |

> Pour tester une transition, on construit un `AppState` réel et on lit son `ViewModel`
> (`state.rs`, module `tests`) ; les scénarios de fenêtre avec une conversion simulée sont dans
> `app_tests.rs`. Un état dont les sorties sont fausses ou absentes ne prouve rien : les assertions
> porteraient sur des sorties inexistantes.

### La formule de distorsion — ce qu'on sait d'elle (depuis P-12)

| Fait | Conséquence |
| --- | --- |
| Le décalage **squeeze** vaut **zéro au centre par construction** : ses deux termes se réduisent tous deux à `7/32 × outX × inv` | Toute couture au milieu de l'image est un défaut d'implémentation, pas une intention. Vérifiable en arithmétique exacte, sans mesure. |
| Le décalage **non-squeeze** vaut `tx² × diff/2`, donc 0 en `tx=0` | Le miroir de la moitié gauche y est continu ; ce mode n'a jamais eu de couture. |
| La carte X **non-squeeze** a pour pente `1 − 2·diff/outX` aux bords : elle n'est monotone que si `diff ≤ outX/2`, soit **H/W ≤ 9/8** | Au-delà l'image se replie aux deux bords et sort du cadre source (1080×1920 : échantillon 1164 pour 1080 colonnes), sans erreur de `remap`. `check_geometry` refuse ces sources en mode non-squeeze (E-05) ; le mode squeeze garde la largeur et n'est pas concerné. |
| Les divisions doivent être **flottantes** | `outX/16` et `outX/7` en entiers tronquent et cassent l'annulation des deux termes (P-12). Une largeur multiple de 112 masque le défaut. |
| Le code Go était **identique caractère pour caractère** à `Niek/superview` (le Rust reproduit le Go : mêmes empreintes) | Fidèle ne veut pas dire correct : la couture y était aussi. Vérifier la conformité à la référence ne dispense pas de vérifier la référence (L-48). |
| L'algorithme **n'est pas** celui de GoPro | Le README amont le dit explicitement, et documente l'option squeeze pour des caméras comme la Caddx Tarsier. Ne pas promettre une compatibilité GoPro dans l'interface (P-13). |
| L'implémentation Python de Banelle est **inaccessible** | 403 sur intofpv.com, archive web refusée. Ne pas la citer comme vérifiable. |

> **Le test doré ne suffit pas.** `golden_fingerprints` (`tests/pgm_golden.rs`) fige des octets : il certifie la
> non-régression, jamais la correction — il a figé la couture de P-12 pendant trois passes.
> Toute modification de la formule doit passer *aussi* par
> `squeeze_map_is_continuous`, `squeeze_map_stays_well_formed` et son
> jumeau non-squeeze `widen_map_stays_well_formed` (même fichier), qui testent des propriétés :
> continuité, monotonie, bornes, centre préservé.

### Contrat des cartes de remappage (depuis la 4ᵉ passe)

`generate_pgm` écrit du **PGM P5**, binaire, 16 bits, **gros-boutiste** — l'ordre des octets est
imposé par la spécification PGM, et un écrivain petit-boutiste produirait des cartes fausses
sans aucune erreur. `put_map_sample` (`pgm.rs`) est le **seul** endroit qui encode un échantillon : les deux
cartes doivent y passer, sinon l'invariant est défini deux fois et le test d'ordre devient
inopérant (c'est arrivé, voir L-37).

Trois tests le verrouillent, et ils ne sont pas interchangeables :

| Test | Ce qu'il prouve |
| --- | --- |
| `golden_fingerprints` | Les octets exacts des cartes n'ont pas bougé par accident |
| `maps_are_big_endian_p5` | Le format sur le fil : magie P5, maxval 65535, ordre des octets |
| `the_binary_maps_remap_like_the_ascii_ones` (`tests/real_ffmpeg.rs`) | Ce que FFmpeg **fait** des cartes, seule mesure qui parle du rendu |

### Cas limites à retester systématiquement

| Chemin | Pourquoi |
| --- | --- |
| FFmpeg absent du `PATH` | `check_ffmpeg` rend une erreur et la fenêtre verrouille `requires_ffmpeg()` sans planter (B-05, non-régression) |
| Sélecteur indisponible (ni portail, ni zenity, ni kdialog) | Bascule sur le champ texte de la fenêtre (`dialogs.rs`, DEC-43) |
| **Second encodage dans la même session** | Les widgets doivent tous revenir actifs — `squeezeCheck` ne l'est pas (P-01) |
| **Annulation immédiate, avant le premier octet de progression** | Course sur le drapeau d'annulation (P-02) ; la cascade de repli relance FFmpeg (P-03) ; le fichier partiel reste (P-04) |
| Encodeur matériel refusé par le pilote | Cascade de repli à trois niveaux dans `encode_video` |
| **Source 10 bits, ou à plusieurs pistes audio, ou horodatée** | Trois pertes silencieuses (N-03, N-04, N-05) |
| **Source à 60/120/240 fps** | la vitesse d'encodage supposait 30 fps en dur (P-06) |
| Chemin contenant `..` ou un lien symbolique | `..` rejeté, lien résolu puis validé, par `security.rs` (S-01, S-02, DEC-19) |
| Lancement hors du répertoire du dépôt | Résolution du `superview.yaml` (B-02, non-régression) |

---

## 3. Références externes

### FFmpeg — autorité sur tout le comportement d'encodage

| Sujet | URL | Utile pour |
| --- | --- | --- |
| Filtre `remap` | https://trac.ffmpeg.org/wiki/RemapFilter | Format PGM P2, sémantique des cartes X/Y — référence citée dans `pgm.rs`, en tête de `generate_pgm` |
| Documentation des filtres | https://ffmpeg.org/ffmpeg-filters.html#remap | Contraintes exactes du filtre |
| Options principales | https://ffmpeg.org/ffmpeg.html#Main-options | **Position des options** : avant `-i` = entrée, après = sortie (cf. B-04) |
| `-progress` / `out_time_ms` | https://ffmpeg.org/ffmpeg.html#Advanced-options | ⚠️ `out_time_ms` est en **microsecondes** malgré son nom — c'est ce que suppose le calcul du lecteur de progression de `encode/progress.rs` |
| Accélération matérielle | https://trac.ffmpeg.org/wiki/HWAccelIntro | Jetons `-hwaccel` valides par plateforme |
| NVENC | https://docs.nvidia.com/video-technologies/video-codec-sdk/nvenc-video-encoder-api-prog-guide/ | Presets `h264_nvenc` / `hevc_nvenc` |
| AMD AMF | https://ffmpeg.org/ffmpeg-codecs.html#toc-AMD-AMF-video-encoders | Presets `speed`/`balanced`/`quality` — mappés dans `map_video_preset_for_encoder` |
| Intel QSV | https://trac.ffmpeg.org/wiki/Hardware/QuickSync | Presets QSV |
| Format PGM | https://netpbm.sourceforge.net/doc/pgm.html | En-tête `P2 <w> <h> 65535` |

### Rust et bibliothèques

| Sujet | URL |
| --- | --- |
| `std::process::Command` | https://doc.rust-lang.org/std/process/struct.Command.html |
| Slint — threads et `upgrade_in_event_loop` | https://docs.slint.dev/latest/docs/rust/slint/struct.Weak.html |
| Slint — rendu logiciel, backends | https://docs.slint.dev/latest/docs/rust/slint/ |
| `rfd` | https://docs.rs/rfd |
| `std::path` (`canonicalize`, `is_absolute`) | https://doc.rust-lang.org/std/path/ — liens symboliques (S-01, S-02) |

### Outillage CI

| Sujet | URL |
| --- | --- |
| `cargo llvm-cov` | https://github.com/taiki-e/cargo-llvm-cov |
| `cargo audit` (RustSec) | https://github.com/rustsec/rustsec/tree/main/cargo-audit |
| Épinglage des actions GitHub | https://docs.github.com/actions/security-guides/security-hardening-for-github-actions#using-third-party-actions — cf. X-05 |

### Algorithme d'origine

- Implémentation Python de Banelle : https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview
  — **source de vérité mathématique** pour `generate_pgm`. Toute modification de la formule
  d'offset doit être confrontée à cette référence.
- Dépôt amont : https://github.com/Canaill51/superview

---

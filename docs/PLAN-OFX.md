# Plan — plugin OpenFX : ce qui reste avant de le livrer

> Établi le 2026-10-26 après l'analyse de parité avec l'application. Le plugin ne décode ni n'encode
> (Resolve le fait) : le FFmpeg livré et les encodeurs matériels ne le concernent pas. Ce qui reste :
> les tests manuels ouverts, un chemin GPU hors NVIDIA, un journal, les configurations non essayées,
> la livraison. Cocher ici au fil des lots ; `docs/SUITE.md` renvoie à ce fichier.

## Comment s'en servir

- **Une session = un lot.** Ouvrir une session neuve (`/clear`) et demander : « Lot N de `docs/PLAN-OFX.md` ».
  Un lot de code = une branche = une PR avec le label `merge`.
- **Phases de chaque lot de code** : explorer (lire les fichiers cités, sous-agent `Explore` si la recherche
  est large) → planifier (mode plan, valider le plan avant d'écrire) → implémenter → vérifier (`/verify`,
  plus la contre-preuve de chaque nouveau test) → livrer (PR) → consigner (cocher ici, `SUITE.md`,
  `DECISIONS.md` / `CORRECTIONS.md` / `LECONS.md` selon le cas).
- **Lots manuels (M)** : l'utilisateur agit dans Resolve ; Claude prépare les fichiers de test, compare
  les exports par ffmpeg et consigne dans `docs/TESTS-MANUELS.md`.
- **Critère d'arrêt** : si l'hypothèse d'un lot tombe (un fait mesuré la contredit), on s'arrête, on
  consigne, on revoit le plan. On ne force pas.
- **Modèle** : indiqué par lot. Sonnet pour le mécanique et la documentation, Opus pour la conception et
  le débogage GPU (abonnement Max : garder Opus pour là où il compte).

## Ordre et dépendances

```
Lot 1 (M) ──────────────► doit précéder Lot 4c : une fois OpenCL en place, le réglage OpenCL de Resolve
                          ne fait plus passer le plugin en CPU, et le test CUDA/CPU n'est plus faisable.
Lot 2 (journal) ────────► sert à prouver dans Lot 4 quel chemin a tourné.
Lot 3 (CUDA)       indépendant, petit, peut suivre Lot 2 dans la même PR si l'utilisateur le veut.
Lot 4a → 4b → 4c → 4d     OpenCL, dans cet ordre ; 4a peut tuer le lot.
Lot 5 (M)          après Lot 4 si possible (un passage couvre CUDA et OpenCL).
Lot 6 (livraison)  en dernier, après une décision de l'utilisateur.
```

## Lot 1 (M) — Les tests manuels qui ne demandent aucun code · Sonnet · ~1 h — **fait le 2026-10-26**

Sur le poste actuel (Resolve 21 gratuit, RTX 3050 Ti). Mesures dans `docs/TESTS-MANUELS.md` ; aucune anomalie
du plugin, donc aucun lot de correction à insérer.

- [x] **CUDA contre CPU** : pas identiques, mais un témoin sans l'effet diffère autant (102 dB contre 97) : c'est
      Resolve, qui change d'API avec le mode, pas le plugin (L-122). Exports en DNxHR.
- [x] **Parité au pixel** : 47,4 dB, écart uniforme entre bords et centre, donc la compression de l'export ;
      bords égaux à l'application.
- [x] **Centrage** : Resolve exporte 1921 en 1922 avec une colonne sombre à gauche, témoin compris ; le plugin
      est propre.
- [x] **Un-squeeze** : sur une source étirée tenant lieu de vidéo GoPro, même parité que *Widen*.
- [x] **Autres réglages** : *Stretch* et *crop* donnent bien une image fausse ; noté pour le README (Lot 6).
- [x] **Défilement** rapide sur le clip : ni trame rouge ni erreur.

Pour le Lot 4d : comparer CUDA et OpenCL **à un témoin sans l'effet**, en DNxHR, timeline de largeur paire.

## Lot 2 — Un journal pour le plugin · Sonnet · une PR — **fait le 2026-10-26**

Avant ce lot, aucun logger n'était installé : les `log::error!` n'allaient nulle part (DEC-65), et un utilisateur
sur AMD ou Intel ne pouvait pas savoir que le plugin tournait en CPU. Fait : `journal.rs`, `FileLogger` déplacé dans
`superview-core::logfile`, amendement de DEC-65 ; le fork, qui journalise chaque trame au niveau info, est filtré.
Vu dans Resolve le 2026-10-26, en CUDA et en OpenCL (`docs/TESTS-MANUELS.md`, *Journal*).

- [x] Explorer : `superview-core` (`logfile`, plafond injecté) et l'emplacement du journal de l'application,
      pour réutiliser plutôt que réécrire.
- [x] Un journal fichier plafonné, initialisé une fois (au `Load` ou au premier `Describe`), jamais bloquant :
      un dossier non inscriptible = pas de journal, pas d'échec.
- [x] Journaliser **une fois par instance** : le chemin choisi (CUDA / CPU, bientôt OpenCL) et la raison d'un
      repli. Rien par trame au-dessus du niveau debug (même règle que la progression de l'application).
- [x] Tests : initialisation idempotente, dossier non inscriptible, plafond ; contre-preuve de chacun.
- [x] Amender DEC-65 (« aucun logger ») ; ARCHITECTURE.md si un module apparaît.

## Lot 3 — CUDA : ne pas retenter un noyau qui ne se charge pas · Sonnet · petite PR — **fait le 2026-10-26**

Sur une carte antérieure à Maxwell (`sm_50`), `Warp::new` échoue à **chaque** trame et Resolve la refait en CPU.

- [x] Mémoriser l'échec par contexte CUDA et rendre `FAILED` aussitôt, ou ne pas annoncer CUDA si le
      module ne se charge pas au `Describe` : choisir en mode plan. **Choisi : mémoriser** (`cuda::PerContext`,
      DEC-66 amendée) ; le `Describe` n'a pas de contexte CUDA. Testé sans GPU (chargeur injecté).
- [x] Test sans GPU si la construction du noyau peut être injectée ; sinon le dire dans DEC-66.

## Lot 4 — Un chemin OpenCL à côté du CUDA (AMD, Intel) · Opus · quatre sessions

**OpenCL s'ajoute, CUDA reste.** C'est Resolve qui choisit le mode (*Préférences → Mémoire et GPU*, pour toute
l'application) ; le plugin déclare les deux et suit la mémoire que Resolve lui donne. Sur NVIDIA, Resolve reste
en CUDA par défaut, donc le plugin aussi. La RTX réglée sur OpenCL ne sert qu'à tester ce chemin sur ce poste.

Le fork nomme déjà `OpenCLRenderSupported`, `OpenCLEnabled` et `OpenCLCommandQueue`
(`ofx/src/property.rs`). Le modèle est `cuda.rs` : bibliothèque chargée à l'exécution, noyau compilé par
le pilote, file de l'hôte, repli CPU par `FAILED`.

- [x] **4a — Sonde (jetable, branche à part)** — **faite le 2026-10-26**, le lot continue
      (`docs/TESTS-MANUELS.md`, *Sonde OpenCL*). Resolve gratuit propose OpenCL aux plugins tiers, en
      **tampons** `cl_mem` seulement (pas d'images OpenCL), RGBA flottant, pas des lignes donné par `row_bytes`,
      file de commandes de l'hôte dans le même contexte que les tampons ; tampons d'une réserve, plus grands
      que la trame. Un `FAILED` fait refaire la même trame en CPU, mais la suivante est redemandée en OpenCL.
- [x] **4b — `opencl.rs` et ses tests** — **fait le 2026-10-26** : vert sur la RTX et l'Intel UHD, contre-preuve
      rougie sur les deux ; `PerContext` déplacé dans `gpu.rs`. Non relié à l'hôte (4c). Ce qui était demandé : `OpenCL.dll` / `libOpenCL.so.1` par `libloading`, noyau OpenCL C
      compilé à l'exécution, même `column_map`. Ce que 4a impose : lire le noyau sur des **tampons**
      (`__global float4*`, décalage en octets par `row_bytes`), pas des images ; tout mettre dans la **file de
      l'hôte** et prendre le contexte par `CL_QUEUE_CONTEXT` ; compiler le programme et charger la carte une fois
      par contexte, **et mémoriser un échec** par contexte (comme `cuda::PerContext`), puisque l'hôte redemande
      OpenCL à chaque trame ; ne vérifier la taille d'un tampon que par `≥ lignes × pas`. `tests/opencl.rs` compare à `warp_row` comme `tests/cuda.rs`,
      saute sans OpenCL sauf avec `SUPERVIEW_REQUIRE_OPENCL=1`. À passer sur **deux** appareils : la RTX
      et l'iGPU Intel UHD de l'i7-11800H (un vrai GPU non NVIDIA, sur ce poste). Contre-preuve : un noyau
      qui lit `x` au lieu de `map[x]` doit rougir.
- [x] **4c — Colle et documents** — **fait le 2026-10-27** (DEC-68) : les deux chemins GPU annoncés, l'hôte choisit ; la colle sans test automatique, vérifiée au 4d. Ce qui était demandé : `Describe` annonce OpenCL si l'hôte le propose et que la bibliothèque
      se charge ; branche `Render` ; DEC-68 ; ligne dans la table d'AGENTS.md (test GPU local seulement) ;
      ARCHITECTURE.md. **Le Lot 1 doit être fait avant.**
- [x] **4d (M) — Dans Resolve** — **fait le 2026-10-27** (`docs/TESTS-MANUELS.md`, *OpenCL dans Resolve*) : journal « OpenCL », aucun échec ; export identique à CUDA à l'écart de Resolve près (82 dB contre 84 pour le témoin) ; 13,3 i/s en 4K, autant que le témoin sans l'effet (13). Ce qui était demandé : réglé sur OpenCL, la lecture 4K (i/s), le journal du Lot 2 dit
      « OpenCL », export OpenCL identique à l'export CUDA.

## Lot 5 (M) — Les configurations non essayées · selon le matériel disponible

> **En attente de matériel** (2026-10-28) : Studio, Linux et AMD restent ouverts ; ils ne bloquent pas la livraison
> (DEC-69, le plugin est publié à chaque version). Le README les dit non essayés.

- [ ] **Studio** (Windows) : chargement, CUDA, vitesse. Demande une licence ou un poste qui l'a.
- [ ] **Linux** (poste KDE, NVIDIA et Intel) : chargement depuis `/usr/OFX/Plugins`, CUDA, OpenCL.
      À vérifier et documenter : Resolve **gratuit** sous Linux ne décoderait pas le H.264/H.265 ni l'AAC,
      donc les MP4 usuels n'y entreraient pas (limite de Resolve, pas du plugin).
- [x] **Export long** en `FullySafe` — **fait le 2026-10-27** (`docs/TESTS-MANUELS.md`, *Export long en
      `FullySafe`*) : une sonde jetable a compté les rendus simultanés ; jamais plus d'un, en CUDA (export de
      21 min) comme en OpenCL, et aucune trame perdue. Deux plantages de Resolve en OpenCL au passage à Deliver,
      dans le pilote OpenGL de NVIDIA, non reproduits ni avec ni sans le plugin.
- [x] **Intel UHD dans Resolve** (poste Windows actuel) — **fait le 2026-10-27** (`docs/TESTS-MANUELS.md`, *OpenCL
      sur l'Intel UHD*) : RTX désactivée, Resolve accepte l'UHD et le plugin y rend en OpenCL sans échec ; 4K à
      10-20 i/s avec l'effet, 18-30 sans. Le plan prévoyait : Resolve grise le GPU intégré tant que la RTX est
      visible (« Discrete GPU utilisé »). Fermer Resolve, désactiver la RTX dans le Gestionnaire de périphériques,
      relancer en OpenCL, cocher l'UHD si Resolve l'accepte ; lecture avec l'effet (i/s, témoin sans l'effet), journal
      « OpenCL » ; puis réactiver la RTX. Le plus proche d'un poste AMD ou Intel : un pilote OpenCL non NVIDIA
      derrière la colle. Si Resolve refuse l'UHD même seule, le noter et s'arrêter.
- [ ] **AMD** : pas de matériel. Rester « non vérifié », comme AMF pour l'application.

## Lot 6 — Livraison · décision de l'utilisateur, puis Sonnet

> **Décidé le 2026-10-28 et replanifié dans [PLAN-LIVRAISON.md](PLAN-LIVRAISON.md)** : archive séparée à
> chaque version, Windows seulement tant que Linux n'est pas essayé. Les cases ci-dessous n'y sont plus suivies.

- [x] **Décision à prendre** (tranchée le 2026-10-28 : téléchargement séparé, DEC-69) : un téléchargement séparé, ou un composant facultatif de l'installateur
      Windows (`{commoncf}\OFX\Plugins`) ? Sous Linux : une archive et `sudo cp` (une AppImage ne peut pas
      installer dans `/usr/OFX`).
- [x] `release.yml` et l'essai à blanc **avant** fusion (#188) (L-114) ; vérifier le paquet comme les autres (DEC-41).
- [x] README.md et README_FR.md ensemble (Lot 2 de `PLAN-LIVRAISON.md`) (réglage *Scale entire image to fit*, limites, installation).
- [x] DEC-65 : retirer « prototype » (amendée par DEC-69) ; `SUITE.md`. La publication reste sur demande, par `/publier`.

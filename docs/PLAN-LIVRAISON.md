# Plan — Livrer le plugin Resolve dans chaque version, puis publier

> Écrit le 2026-10-28. Remplace le Lot 6 de [PLAN-OFX.md](PLAN-OFX.md), qui y renvoie.

## Comment s'en servir

Une session par lot : `/clear`, puis « Lot N de docs/PLAN-LIVRAISON.md ». Chaque lot suit
explorer → planifier → implémenter → vérifier → livrer (une PR, label `merge`) → consigner (cocher ici,
`docs/SUITE.md`, `docs/CORRECTIONS.md` si c'est une correction).

## Décisions de l'utilisateur (2026-10-28)

- **Le plugin est publié à chaque version**, en **archive séparée** à côté de l'installateur et de
  l'AppImage, pas comme composant de l'installateur.
- **Windows seulement** : l'archive Linux n'est pas publiée tant que Resolve sous Linux n'a pas été essayé
  (Lot 5 de `PLAN-OFX.md`). Le bundle Linux reste construit par la CI (`rust.yml`, job `ofx-bundle`).
- **Studio et Linux attendent le matériel** ; ils ne bloquent pas la livraison. Le README dit qu'ils n'ont
  pas été essayés, comme AMD pour l'application.
- **La version suivante (v0.4.1 probablement) vient quand tout est à jour**, c'est-à-dire après les Lots 1 et 2,
  et seulement sur demande, par `/publier`.

## Ce qui reste, trié

| Nature | Quoi |
|---|---|
| Code (CI) | `release.yml` : construire, vérifier, publier l'archive du plugin Windows (Lot 1) |
| Documentation | ce qui décrit le paquet, avec lui (Lot 1) : DEC-65 et DEC-69, `RELEASING.md`, `ARCHITECTURE.md`, `THIRD_PARTY_NOTICES.md` (embarqué dans le zip), `/publier` ; ce que lisent les utilisateurs et les agents (Lot 2) : les deux README, `AGENTS.md`, `hardware-support.md`, `ENVIRONNEMENT.md`, `SUITE.md`, `PLAN-OFX.md`, `TESTS-MANUELS.md`, balayage du reste |
| Manuel (l'utilisateur) | installer l'archive de l'essai à blanc dans Resolve ; installer le `setup.exe` de l'essai à blanc sur ce Windows, ce qui lève aussi « installateur jamais vérifié sur un vrai Windows » (Lot 1) |
| Décision | aucune en suspens |
| Hors de portée (matériel) | Resolve Studio, Resolve sous Linux, AMD (plugin et AMF), QSV |

## Ordre et dépendances

```
Lot 1 (release.yml)  ──► Lot 2 (README et suivi) ──► Lot 3 (publication, sur demande)
```
- Lot 2 après Lot 1 : le README décrit un fichier dont le nom et le contenu sont fixés par le Lot 1.
- Lot 3 en dernier : c'est la demande de l'utilisateur (« quand tout sera à jour »).

## Lot 1 — Publier l'archive du plugin Windows avec chaque version · Sonnet (Opus si l'essai à blanc échoue de façon obscure) · une PR

But : chaque version publie `superview-ofx-<tag>-windows-x86_64.zip` (le dossier `Superview.ofx.bundle`
dedans), dans `checksums.txt` et dans la section *Downloads*.

À lire : `.github/workflows/release.yml` (jobs `build-gui-windows`, `create-release`),
`.github/workflows/rust.yml` (job `ofx-bundle` : sa préparation Windows pour bindgen fonctionne déjà),
`packaging/ofx/build_bundle.sh`, `RELEASING.md`, DEC-41, DEC-47 (pas de cache de `target/` dans les jobs de
build), DEC-65, L-114.

- [x] Explorer : le plugin annonce-t-il une version à l'hôte, ou dans son journal ? Si oui, d'où la tient-il
      (`SUPERVIEW_VERSION`, DEC-22) ; sinon, le noter sans l'ajouter dans ce lot.
      → **Non** : `PluginVersion(1, 0)` en dur (`plugin.rs`), journal sans version ; noté dans DEC-69, pas ajouté.
- [x] Un job `build-ofx-windows` (ou des étapes dans `build-gui-windows`) : `sh packaging/ofx/build_bundle.sh`,
      sans cache de `target/`.
- [x] Vérifier le paquet comme les autres (DEC-41) : le `.zip` contient `Contents/Win64/Superview.ofx` et
      `Contents/Info.plist`, et la DLL exporte `OfxGetPlugin` et `OfxGetNumberOfPlugins` (ce que Resolve
      appelle au chargement).
- [x] `create-release` : le télécharge, l'ajoute à `checksums.txt` et à la section *Downloads*.
- [x] **Licences** : le zip embarque `LICENSE` (déjà fait par `build_bundle.sh`) **et** les mentions des
      dépendances du plugin (fork `ofx-rs`, `libloading`, etc.). Lire comment la section *Superview's own
      dependencies (Rust)* de `THIRD_PARTY_NOTICES.md` est tenue, et l'étendre au plugin ; sa phrase d'ouverture
      (« the Windows installer and the Linux AppImage ») nomme aussi le zip. La vérification du paquet exige le fichier.
- [x] Nouvelle décision (DEC-69) : archive séparée, Windows seulement, et ce qui la rouvre (Resolve essayé sous
      Linux → publier l'archive Linux). Amender DEC-65 (« prototype, pas encore livré » ne vaut plus).
- [x] `RELEASING.md` (le fichier publié, ce qui le vérifie) et `ARCHITECTURE.md` (« prototype, pas dans les
      versions »).
- [x] `.claude/skills/publier/SKILL.md` : la vérification des fichiers publiés attend le zip du plugin.
- [x] Consigner : `docs/CORRECTIONS.md` si quelque chose a été corrigé en route ; une leçon dans `LECONS.md`
      (et `PIEGES.md`) si l'essai à blanc en apprend une.

Vérification :
- `scripts/check.sh` vert.
- **Essai à blanc avant fusion (L-114)** : `gh workflow run release.yml --ref <branche>`, sans version ; le zip
  est dans les artefacts.
- Contre-preuve de la vérification du paquet : la faire tourner sur un zip sans `Superview.ofx` (localement
  ou dans une branche jetable), elle doit échouer.
- **Manuel (l'utilisateur)** : sur ce poste, désinstaller le plugin de développement, décompresser le zip de
  l'essai à blanc dans `C:\Program Files\Common Files\OFX\Plugins`, relancer Resolve, appliquer l'effet,
  vérifier le journal (CUDA). Installer aussi le `setup.exe` de l'essai à blanc, lancer une conversion, puis
  désinstaller. Consigner les deux dans `docs/TESTS-MANUELS.md`.

Critère d'arrêt : si le bundle ne se construit pas sur `windows-latest` dans `release.yml` alors qu'il se
construit dans `rust.yml`, comparer les deux environnements avant de modifier `build_bundle.sh`.

## Lot 2 — Le README et les fichiers de suivi · Sonnet · une PR

But : un utilisateur sait installer et régler le plugin ; les fichiers de suivi disent où on en est.

À lire : `README.md`, `README_FR.md` (parité, `readme-parity` dans `lint.yml`), `docs/TESTS-MANUELS.md`
(Lot 1 de `PLAN-OFX.md` : réglages qui donnent une image fausse), `crates/superview-ofx/src/journal.rs`
(emplacement du journal).

- [x] README.md et README_FR.md, même PR, même structure de titres : une section sur le plugin Resolve :
      archive à télécharger, dossier où la copier, redémarrage de Resolve, où trouver l'effet, réglage
      *Scale entire image to fit* (et ceux qui donnent une image fausse, *Stretch* et le recadrage), CUDA et OpenCL
      choisis par Resolve, journal à joindre à un signalement, limites (Windows seulement, Studio pas essayé,
      AMD non vérifié). Libellés de Resolve et de Superview en anglais dans le fichier français.
- [x] `AGENTS.md` : « *still a prototype* » (ligne 8) devient « shipped with each release, Windows only » ; la
      ligne du tableau des décisions pour DEC-69 est déjà faite (Lot 1, comme la ligne « not archives »).
- [x] `docs/hardware-support.md` : le plugin, CUDA (NVIDIA, `sm_50` et plus) et OpenCL, choisis par Resolve ;
      ce qui a été vérifié (RTX 3050 Ti, Intel UHD) et ce qui ne l'a pas été (AMD, Studio, Linux).
- [x] `docs/ENVIRONNEMENT.md`, section *Plugin OpenFX* : distinguer l'installation depuis la version publiée
      (le zip) de celle d'un build local.
- [x] `docs/SUITE.md` : résumer le long paragraphe du plugin ; état depuis la v0.4.0 (#175 en attente de
      version) ; ce qui attend le matériel ; retirer le renvoi « Prochain travail » quand le plan est fini.
- [x] `docs/PLAN-OFX.md` : Lot 5, ce qui reste en attente de matériel ; Lot 6 renvoie ici.
- [x] `docs/TESTS-MANUELS.md` : la liste de ce qui est en suspens, à jour.
- [x] Balayer le reste de la prose, hors `docs/archives/` et journaux datés (qui gardent leur état d'époque) :
      `grep -rniE "prototype|pas (encore )?livr|not (yet )?shipped|hors des versions" --include='*.md' .`
- [x] `docs/ANALYSE.md` et `docs/CONTRATS.md` : rien n'est attendu, mais vérifier au balayage qu'aucun ne dit le
      plugin hors des versions.

Vérification : `scripts/check.sh` (le garde-fou `repo_guards.rs` lit le README), CI `readme-parity` verte.

## Lot 3 — Publier la version · Sonnet · sur demande de l'utilisateur seulement

- [x] L'utilisateur demande « publie la version x.y.z » ; l'agent passe par `/publier` (DEC-42).
      → **v0.5.0** le 2026-10-28 (et non la v0.4.1 : une mineure, le plugin étant un livrable nouveau).
- [ ] Après la publication : le zip est sur la page de la version, `sha256sum -c checksums.txt` passe, et
      l'utilisateur installe le zip **publié** dans Resolve (et le `setup.exe` publié si le Lot 1 ne l'a pas fait).
      → Les quatre fichiers sont publiés et `sha256sum -c` passe ; reste l'installation du zip publié dans Resolve.
- [x] `docs/SUITE.md` : la version publiée et ce qu'elle apporte (D3D12 #175, tokio #173, le plugin).

## En attente de matériel (non planifié)

Resolve Studio, Resolve sous Linux (puis l'archive Linux, DEC-69), AMD pour le plugin et AMF, QSV, portail
xdg lent, Linux sans portail ni zenity : `PLAN-OFX.md` Lot 5 et `docs/TESTS-MANUELS.md`.

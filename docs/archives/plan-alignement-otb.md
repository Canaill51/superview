> **Archive, plan terminé.** Plan d'alignement de Superview sur l'organisation d'OpenTuneBox, approuvé par
> l'utilisateur le 2026-11-12 et mené en sept PR, #193 à #199 (2026-11-12 et 2026-11-13). Il vivait hors du
> dépôt (`~/.claude/plans/abstract-squishing-rabbit.md`) ; texte inchangé. Ce qui a été fait, étape par
> étape : l'historique de [plan.md](../plan.md) et [corrections.md](../corrections.md). Écart avec le texte :
> à l'étape 7, un passage `draft=true` ne pose plus d'étiquette (DEC-73), sans quoi l'essai `RC-…` aurait
> laissé une étiquette que la règle interdit de supprimer.

# Aligner Superview sur l'organisation d'OpenTuneBox

## Contexte

Cédric veut que Superview ait la même structure et la même organisation qu'OpenTuneBox (OTB), sans
régression. L'analyse comparée (structure, nommage, sécurité, workflows) a donné ceci.

**Ce que Superview fait déjà aussi bien ou mieux qu'OTB, et qu'on garde tel quel** :
- les permissions des workflows (lecture seule par défaut, écriture limitée à un seul job) ;
- les attestations et `checksums.txt` des versions, le plancher pilote NVENC et le contrôle `readme-parity` ;
- la liste `deny` de `.claude/settings.json`, `garde-fou.sh` et la recherche de secrets dans `.githooks/pre-commit` ;
- la couverture à 50 %, `cargo audit`, le cache `rust-cache` hors builds de version ;
- la fusion par l'étiquette `merge` (`merge-on-label.yml`).

**Écarts avec OTB que l'on corrige, d'après les choix de Cédric** :
- **docs** : noms en MAJUSCULES au lieu de minuscules, pas d'index `docs/README.md`, trois documents
  de plan (`SUITE`, `PLAN-OFX`, `PLAN-LIVRAISON`) au lieu d'un seul, aucun test de cohérence des
  registres ni compétence `docs-sync` ;
- **contribution** : pas de `CONTRIBUTING.md` ni de modèles d'issue et de PR ;
- **notes de version** : pas d'étiquettes `RN: …`, Superview passe par une rubrique fourre-tout `"*"` ;
- **publication** : Superview passe ses fichiers par des artefacts, alors qu'OTB crée la page Releases
  en brouillon, y téléverse avec `gh release upload --clobber`, puis publie ;
- **hygiène et sécurité** : pas d'actionlint, pas d'audit planifié, Dependabot cargo sans regroupement,
  `deny.toml` sans `yanked`/`wildcards = "deny"`, pas de lint `unsafe_code`, un `SECURITY.md` sans
  portée ni intégrité.

**Ce qu'on n'aligne pas, et pourquoi** :
- **la langue** : on garde la répartition de Superview (docs internes en français, ce que lit un
  visiteur en anglais). Les nouveaux fichiers publics (CONTRIBUTING, modèles, étiquettes) sont donc en
  anglais.
- **l'espace de travail sous `rust/`** : il vient d'en remonter volontairement (CORRECTIONS 2026-10-21).
- **les numéros `DEC-xx`, `L-xxx`, `U-xx`** : ils sont cités dans le code et l'historique.
- **`THIRD_PARTY_NOTICES.md`** : ce nom est livré dans l'installateur, l'AppImage et le zip, et un test
  le lit.
- **les tests en `src/<mod>/tests.rs`** et **`.githooks`, `Makefile`, `scripts/`** : rien à y gagner.

**Méthode commune à chaque étape** : une étape = une session = une branche = une PR. Chaque étape
commence par relire ce plan et la section concernée d'AGENTS.md.

Elle se termine ainsi :
1. `scripts/check.sh` complet, avec `SUPERVIEW_REQUIRE_FFMPEG=1` (via `/verify`) ;
2. un `git grep` des anciens noms, qui doit donner zéro hors citations historiques ;
3. une contre-preuve pour chaque nouveau test (casser, voir rougir, rétablir) ;
4. l'entrée `docs/corrections.md`, plus une `DEC` quand un choix change.

Les actions sur GitHub (étiquettes, règles, lancement de workflow) sont demandées à Cédric au moment
voulu. Fin d'étape : rappeler `/clear`.

---

## Étape 1 — Docs en minuscules et index (mécanique)

Renommer avec `git mv`, puis corriger tous les liens avec un seul `sed` sur les fichiers suivis.

| Avant | Après |
|---|---|
| `ARCHITECTURE.md` | `architecture.md` |
| `DECISIONS.md` | `decisions.md` |
| `LECONS.md` | `lecons.md` |
| `ANALYSE.md` | `analyse.md` |
| `CORRECTIONS.md` | `corrections.md` |
| `CONTRATS.md` | `contrats.md` |
| `ENVIRONNEMENT.md` | `environnement.md` |
| `TESTS-MANUELS.md` | `tests-manuels.md` |
| `archives/ANALYSE-passes-3-a-13.md` | `archives/analyse-passes-3-a-13.md` |
| `archives/CORRECTIONS-ere-go.md` | `archives/corrections-ere-go.md` |

`SUITE`, `PLAN-*`, `PIEGES` et `MERGING` attendent les étapes 2 et 4.

**Pièges silencieux à corriger explicitement** :
- `CLAUDE.md` importe `@docs/ARCHITECTURE.md` ; il faut passer à `@docs/architecture.md`.
- Les commentaires « ANALYSE.md U-03 » se trouvent dans `.github/scripts/nvenc-driver-floor.sh`,
  `release.yml` et `crates/superview-core/src/hardware.rs`.
- Les messages d'erreur de `scripts/checks/rust.sh` citent `ENVIRONNEMENT.md`.
- `README.md` et `README_FR.md` citent `CONTRATS.md`. Les deux fichiers changent ensemble, ce qui
  satisfait `readme-parity`.

**Créer `docs/README.md`**, l'index : un tableau Document | Pour qui | Contenu, sur le modèle de
`OpenTuneBox/docs/README.md`.

**Mettre à jour la section « Arborescence » d'`architecture.md`**, ainsi qu'AGENTS.md (table
« Recording what you learn »).

**Vérifier** :
- `git grep -nE 'docs/(ARCHITECTURE|DECISIONS|LECONS|ANALYSE|CORRECTIONS|CONTRATS|ENVIRONNEMENT|TESTS-MANUELS)'`
  ne donne aucun résultat ;
- dans une nouvelle session, `CLAUDE.md` charge bien l'architecture ;
- `check.sh` complet passe.

## Étape 2 — Un seul plan, pièges par domaine

**Le plan unique** : fusionner `SUITE.md`, `PLAN-OFX.md` et `PLAN-LIVRAISON.md` dans **`docs/plan.md`**,
comme le `plan.md` d'OTB. Il contient :
- en tête, « Où on en est » : les 15 premières lignes, lues par le hook ;
- « À faire » : le Lot 5 du plugin, qui attend le matériel (Studio, Linux, AMD) ;
- « Écarté » ;
- « Historique », une ligne par lot fait : LIVRAISON terminé, OFX lots 1 à 4 et 6.

Le détail utile des deux plans va dans `archives/`, pas à la poubelle. Ajouter dans AGENTS.md la règle
d'OTB : « un seul plan, ne jamais en ouvrir un second ».

**Fichiers à mettre à jour** :
- `.claude/hooks/debut-session.sh`, qui doit lire `docs/plan.md` ;
- `scripts/checks/hooks.sh`, son test ;
- `.claude/skills/publier/SKILL.md` ;
- AGENTS.md ;
- la mémoire `superview-docs-suivi`.

**Les pièges** : `PIEGES.md` devient **`docs/pieges-par-domaine.md`**, avec la règle courte de chaque
piège rangée par domaine : FFmpeg et encodage, interface, plugin OFX, empaquetage, CI, outillage. Le
renvoi vers `L-xxx` est conservé.
- L'unique fiche de `docs/pieges/` passe en leçon dans `lecons.md` si elle n'y est pas déjà, puis le
  dossier et `_modele.md` sont supprimés.
- La référence périmée à `/piege` est retirée.

**Vérifier** : une nouvelle session affiche bien la tête de `plan.md` ; le test des hooks passe ; le
`git grep` de `SUITE.md|PLAN-OFX|PLAN-LIVRAISON|PIEGES.md|docs/pieges/` ne donne rien.

## Étape 3 — Test de cohérence de la doc, compétence `docs-sync`, CLAUDE.md

**Le test** : un nouveau module de test `crates/superview-app/src/docs_guards.rs`, déclaré comme
`repo_guards.rs` (qui en est le modèle). Il reprend les vérifications de `OpenTuneBox/rust/otb-dev/tests/docs.rs`
qui s'appliquent à Superview :
- les liens relatifs des `.md` existent ;
- les chemins de code cités entre backticks existent ;
- `docs/README.md` liste chaque fichier de `docs/` (hors `archives/`) ;
- les numéros `DEC-xx`, `L-xxx` et `U-xx` sont uniques dans leur registre ;
- chaque numéro cité ailleurs (code, docs, AGENTS) existe.

Exclure les archives et les citations historiques volontaires (liste blanche commentée).

**Contre-preuve** pour chaque vérification : un lien cassé, un numéro en double, un fichier absent de
l'index.

**La compétence** : créer `.claude/skills/docs-sync/SKILL.md`, courte (consigne globale), avec un
tableau « tel changement → relire tels documents ». Y mettre les deux README, `architecture.md`,
`contrats.md`, `decisions.md`, `hardware-support.md`, RELEASING et AGENTS.

**CLAUDE.md** : ajouter une section « Spécifique à Claude Code », comme OTB. Y déplacer le paragraphe
`.claude/` d'AGENTS.md, plus les compétences du projet (`docs-sync`, `verify`, `publier`).

## Étape 4 — CONTRIBUTING, modèles, étiquettes `RN:`

**Fichiers du dépôt** :
- **`CONTRIBUTING.md`** (anglais) : signaler un problème, branches `type/short-description`,
  vérification, documentation dans le même commit (`docs-sync`), titre de PR en phrase, une étiquette
  `RN:`, la fusion par `merge` (contenu de `docs/MERGING.md`, ensuite supprimé), un renvoi vers
  RELEASING.
  - Mettre à jour l'URL `RULE_URL` et le commentaire de `merge-on-label.yml`, qui pointent vers
    `docs/MERGING.md`, ainsi qu'AGENTS.md.
- **Modèles GitHub** : `.github/PULL_REQUEST_TEMPLATE.md` (sections Purpose, Registers, Checks) et
  `.github/ISSUE_TEMPLATE/{bug.yml,feature.yml,config.yml}`. Les modèles de bug demandent la version,
  l'OS, le GPU et le journal (comme le README).
- **`.github/release.yml`** : rubriques New features (`RN: major feature`, `RN: feature`),
  Improvements (`RN: improvement`, `RN: security`, `RN: refactor`) et Fixes (`RN: fix`). La rubrique
  Dependencies (étiquette `dependencies`) est conservée, comme aujourd'hui. `RN: ignore` est exclue.
  Le fourre-tout `"*"` est retiré.
- **Workflow `pr-label.yml`**, copie adaptée d'`etiquette-pr.yml` :
  - `permissions: {}`, pas de checkout, titre et étiquettes passés par `env:` ;
  - concurrence par numéro de PR, sans annulation (L-098 d'OTB) ;
  - exactement une étiquette `RN:` ; Dependabot est exempté ; l'étiquette `merge` est ignorée ; refus
    d'un titre qui ressemble à un nom de branche.
- **Test dans `repo_guards.rs`** : la liste d'étiquettes du workflow est identique aux rubriques de
  `.github/release.yml`. Contre-preuve.
- **Registres** : une nouvelle `DEC` remplace la ligne « `"*"` catch-all » (marquée *Remplacée*) ;
  mettre à jour la table d'AGENTS.md et la section « Landing a change ».

**Sur GitHub** (à demander à Cédric, une par une) :
1. créer les 7 étiquettes `RN:` ;
2. **étiqueter les PR fusionnées depuis v0.5.0** (#190, #191 → `RN: ignore` ; #192 → `RN: security`),
   sinon elles disparaissent des notes de la prochaine version ;
3. ajouter le contrôle `pr-label` aux contrôles exigés de la règle de `master`, pour que la fusion
   automatique l'attende.

**Vérifier** :
- une PR de test sans étiquette doit rester rouge, puis passer au vert avec `RN: ignore` (c'est la PR
  elle-même) ;
- `gh api repos/:owner/:repo/releases/generate-notes -f tag_name=v0.5.1-test -f previous_tag_name=v0.5.0`
  montre les trois PR rangées dans leur rubrique (lecture seule).

## Étape 5 — Sécurité (Rust et dépôt)

- **`deny.toml`** : `yanked = "deny"` ; une section `[bans]` avec `wildcards = "deny"` et
  `allow-wildcard-paths = true` (dépendances internes en chemin). Lancer `cargo deny check` localement.
- **`Cargo.toml` `[workspace.lints.rust] unsafe_code = "deny"`** :
  - app, core et harness n'ont aucun `unsafe` réel ;
  - `superview-ofx` (122 occurrences, de la FFI) reçoit `#![allow(unsafe_code)]` à la racine, avec la
    raison et un renvoi à DEC-65 ;
  - contre-preuve : un `unsafe {}` dans core fait échouer clippy.
- **`SECURITY.md`** (anglais) : ajouter la portée (fichiers lus, ffmpeg lancé sans shell, chemins
  validés, plugin et pilotes GPU depuis System32 selon DEC-70) et l'intégrité (versions immuables,
  `checksums.txt`, `gh attestation verify`). Ne **pas** écrire « sans unsafe ».
- **Garde-fous Claude** (repris d'OTB `guard-git.sh`) :
  - `garde-fou.sh` attrape aussi `git -C … push --force` et les formes `sh -c` ;
  - `settings.json` met `gh pr merge*` et `gh workflow run*` en `ask` ;
  - ajouter les cas au test `scripts/checks/hooks.sh` (contre-preuve).

## Étape 6 — Hygiène CI

- **`lint.yml`** : un job `actionlint` (binaire téléchargé, SHA-256 vérifié comme dans OTB
  `tests.yml:61-75`, `persist-credentials: false`, `timeout-minutes: 5`). Le lancer d'abord en local
  sur les quatre workflows et corriger ce qu'il signale.
- **`audit.yml`** (nouveau) : `cargo audit` et `cargo deny` chaque lundi (`schedule`) et à la demande ;
  `contents: read`. Une alerte arrive ainsi sans attendre une PR.
- **`dependabot.yml`**, bloc cargo : jour, heure et fuseau Europe/Paris, `open-pull-requests-limit: 5`,
  regroupement mineures et correctifs, préfixe de commit.
- **`rust.yml`** : `retention-days: 7` sur l'artefact `ofx-bundle` (il est à 90 jours aujourd'hui) ;
  `persist-credentials: false` sur les checkouts qui ne poussent rien.
- **Points d'attention** : ne renommer **aucun** job exigé par la règle (`Rust on ubuntu-latest`, etc.) ni
  les fichiers `rust.yml`/`lint.yml` (codés en dur dans `merge-on-label.yml:252`) ; demander à Cédric
  s'il veut ajouter `actionlint` aux contrôles exigés.

## Étape 7 — Publication sur le modèle `construire.yml` (la plus risquée, en dernier)

Réécrire le circuit de `release.yml` sans toucher à `prepare`, `test`, ni au contenu des builds
(plancher NVENC, marqueur de build, essais d'installation, AppImage).

**Les jobs** :
- **`draft`** (`contents: write`) : sur une exécution avec version, il crée la page en brouillon
  (`gh release create TAG --draft --target $GITHUB_SHA --generate-notes`, avec le corps actuel :
  sommes et `gh attestation verify`), ou réutilise celle qui existe. Il ajoute `--verify-tag` quand
  l'exécution est déclenchée par une étiquette poussée.
- **Les trois builds** :
  - avec version : `permissions: contents: write`, `persist-credentials: false`, et `GH_TOKEN` donné
    **à la seule étape** `gh release upload TAG FICHIER --clobber` ;
  - en essai sans version : `upload-artifact` avec `retention-days: 1`.
- **`publish`** (`contents: write`, `id-token`, `attestations`, `artifact-metadata`) :
  1. `gh release download` du brouillon, puis vérification que la liste contient **exactement** les
     3 fichiers attendus ;
  2. `checksums.txt` téléversé ;
  3. `actions/attest` sur les 3 fichiers ;
  4. l'étiquette annotée, comme aujourd'hui ;
  5. `gh release edit --draft=false --latest`, ou `--prerelease` pour RC, ou on s'arrête au brouillon si
     `draft=true`.
- **`notify`** : inchangé.

**Risque à acter (nouvelle `DEC`, qui amende E-24)** : les builds tiennent désormais un jeton en
écriture, alors qu'aujourd'hui seul `create-release` l'a. La parade est double :
`persist-credentials: false`, et un jeton présent seulement dans l'environnement de l'étape de
téléversement. Ce qui reste : un script de build malveillant pourrait piéger la suite du même job.
**Accepté par Cédric le 2026-11-12**, avec le raisonnement d'OTB (`OpenTuneBox/docs/decisions.md`,
entrée « 2026-11-11 — Sécurité du dépôt et de la chaîne de publication », options écartées) : une
crate capable de voler le jeton pourrait aussi bien piéger l'exécutable, donc la défense est en amont.
Pour Superview, ce sont `cargo deny`, `cargo audit`, `--locked`, les versions immuables, les
attestations et `checksums.txt`. Le montage est celui d'OTB `construire.yml` : lignes 82-100 pour les
droits et le checkout, lignes 200-206 pour l'envoi.

**Fichiers à mettre à jour** :
- `RELEASING.md` ;
- `/publier` (`.claude/skills/publier/SKILL.md`) : la vérification des fichiers publiés ;
- AGENTS.md (la ligne sur `target/` non caché reste vraie) ;
- `corrections.md`.

**Vérifier, dans l'ordre, chaque lancement étant demandé à Cédric** :
1. `actionlint` ;
2. un essai sans version lancé sur la branche (`gh workflow run release.yml --ref <branche>`) : les
   builds passent et rien n'est publié ;
3. un essai `prerelease=true, draft=true` qui crée un brouillon `RC-…` complet (3 fichiers, les sommes,
   la liste vérifiée) ; le contrôler, puis supprimer le brouillon et son étiquette RC ;
4. la vraie validation sera la prochaine version, lancée par `/publier`, qui vérifie les fichiers
   publiés (`sha256sum -c`).

## Après le plan

- Proposer à Cédric d'ajuster sa consigne globale `~/.claude/CLAUDE.md`, qui range encore superview
  parmi les forks « dont on ne touche pas les workflows ».
- Ajouter une ligne d'historique par étape dans `docs/plan.md`.

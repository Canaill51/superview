# Superview — Journal des corrections appliquées

> **Journal vivant** : une entrée par correction, ajoutée après CHAQUE correction appliquée
> au code (procédure et gabarit : [LECONS.md § 1](LECONS.md)). Il était le § 3 de
> LECONS.md ; il en a été sorti pour que la lecture des règles (§ 2) ne traîne pas
> 2 000 lignes d'historique. Texte inchangé. Les renvois « LECONS § 3 » désignent ce fichier.
> Les chemins `common/` y sont ceux d'avant DEC-39 (aujourd'hui `internal/common/`).
> Pour savoir si une correction a déjà été tentée : `grep -n "<mot>" docs/CORRECTIONS.md`.

## 3. Corrections appliquées

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

### [2026-10-14] L'AppImage affiche l'icône générique « W » sous Wayland (KDE)

| | |
| --- | --- |
| **Constat** | l'utilisateur voit un « W » dans la barre de titre au lieu de l'icône « SV », alors que `window.SetIcon` est correct |
| **Cause** | sous Wayland l'application ne peut pas imposer l'icône de sa fenêtre : KWin cherche un fichier `.desktop` dont le **nom** est l'`app_id` (`com.canaill51.superview`, donné à `app.NewWithID`). L'AppImage embarquait `superview.desktop` ; `StartupWMClass` n'y change rien sous Wayland (essayé) |
| **Fichiers** | `packaging/linux/build_appimage.sh`, `cmd/superview/appimage_appid_test.go` |
| **PR** | fix/appimage-desktop-name |
| **Vérification** | mesuré en local : `superview.desktop` + `StartupWMClass` → « W » ; `com.canaill51.superview.desktop` → « SV », même avec une icône sans extension · `TestAppImageDesktopFileIsNamedAfterTheAppID` rouge avec l'ancien nom (contre-épreuve) |

Le fichier `.desktop` d'une AppImage intégrée au bureau par un lanceur tiers est recréé par ce lanceur :
c'est son nom qui compte, d'où le correctif dans le script de construction.

### [2026-10-14] DEC-41 — La release livre un installateur Windows et une AppImage Linux

| | |
| --- | --- |
| **Constat** | demande de l'utilisateur (modèle : OpenTuneBox) ; L-115 trouvée en route |
| **Fichiers** | `packaging/windows/superview.iss`, `packaging/windows/superview.ico`, `packaging/linux/build_appimage.sh`, `.github/workflows/release.yml`, `.github/workflows/test.yml` (L-115) ; README.md, README_FR.md, RELEASING.md, AGENTS.md, docs/DECISIONS.md, docs/ARCHITECTURE.md, docs/CONTRATS.md ; test : `cmd/superview/workflow_pipes_test.go` |
| **PR** | feat/binary-releases |
| **Vérification** | `scripts/check.sh` ✅ · AppImage construite et lancée en local (code 124) · essai à blanc de la release ✅ sur les deux jobs (installation, outils exécutés, désinstallation ; extraction, outils exécutés, lancement sous `xvfb`, code 124) · contre-épreuve de L-115 ✅ |

Le premier essai à blanc a échoué sur Windows (L-115) ; le second est vert. L'AppImage pèse plus que
l'ancien `.tar.xz` (squashfs gzip), l'installateur moins que l'ancien `.zip`.

### [2026-10-14] Q-02, Q-03 — Le garde-fou mémoire suit les fils ; pas de télémétrie GoPro

| | |
| --- | --- |
| **Constat** | Q-02, Q-03 ([ANALYSE.md § 5](ANALYSE.md)) |
| **Fichiers** | `common/memory.go`, `common/encode.go`, `common/pipeline.go` ; tests : `common/memoryguard_test.go`, `common/fallbackmemory_test.go`, `common/integration_test.go` |
| **PR** | #90 |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · `golangci-lint` ✅ · 5 contre-épreuves ✅ (voir ci-dessous) |

**Symptôme** — Aucun observé : une sous-estimation soupçonnée (Q-02), puis mesurée.

**Cause racine** — Les constantes d'U-11 valaient pour 4 fils et s'appliquaient à 16.

**Correctif** — `memoryNeededForFrame` prend le nombre de fils et ajoute 4 % par fil au-delà
de 4 (DEC-37) ; `effectiveEncoderThreads` donne le compte réel aux deux appelants. Q-03 :
rien à coder, DEC-38.

**Contre-épreuves** — (1) pente ignorée : `GrowsWithThreads` et `RefusesWhatOnlyManyThreadsCannotHold`
rougissent ; (2) plafond de 16 retiré : `TestEffectiveEncoderThreads` rougit ; (3a) `PerformEncoding`
repasse la base : `MemoryCheckPricesTheConfiguredThreads` rougit ; (3b) le repli repasse la
base : `FallbackPricesTheThreadsItWillUse/sixteen_threads_do_not_fit` rougit ; (4) économie
créditée sous 4 fils : `NeverBelowTheBaseline` rougit.

**Leçon** — L-113.

### [2026-10-14] E-26, E-27 — Le gate ne bloquait rien, et ce qu'il cachait sous Windows

| | |
| --- | --- |
| **Constat** | E-26, E-27 ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `.github/workflows/test.yml` (gate) ; `common/observability.go` ; `common/observability_default_test.go`, `common/stopcascade_test.go`, `common/housekeeping*_test.go` |
| **PR** | #88 |
| **Vérification** | `gofmt` ✅ · `go vet` ✅ · tests ciblés ✅ sous Linux · Windows : runner CI (seule plateforme qui exécute le `.bat`) · gate prouvé : Windows rouge → gate rouge |

**Symptôme** — #85 à #87 ont fusionné avec le job Windows rouge.

**Cause racine** — le gate requis dépendait des tests par `needs:` ; un `needs` en échec le
fait sauter, et un check requis sauté vaut réussite. Derrière : un plafond de journal qui ne
s'appliquait pas sous Windows (`Truncate` sur une poignée `O_APPEND`), une isolation de cache
Unix seulement, une poignée non fermée, et un `.bat` factice dont `cmd.exe` coupait le chemin
à la virgule du nom du sous-test.

**Correctif** — `if: always()` et contrôle explicite des deux résultats ; réouverture en
`O_TRUNC` ; `LocalAppData` posé avec `XDG_CACHE_HOME` ; `Close` de la quatrième ouverture ;
sous-test renommé et garde dans la fixture.

**Leçon** — L-111, L-112.

### [2026-10-14] E-19 (fin, côté GUI) — Les tests de la fenêtre lisaient la ligne d'attente

| | |
| --- | --- |
| **Constat** | E-19, sous-items du paquet racine ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | tests seulement : `gui_main_test.go`, `gui_handler_test.go` (nouveau) ; aucune ligne de production |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ (seul le test VAAPI saute) · 13 contre-épreuves ✅, toutes compilées |

**Symptôme** — `TestAppState_RefreshHardwareStatus` (« describes the planned path ») et
`TestAppState_SetFFmpegUnblocksTheWindow` laissaient `encoderProbe` à nil : la ligne lue était
« Hardware: checking which encoders… », qui satisfaisait leurs assertions sans que
`DescribeHardwareAccelerationPlan` soit atteint. `TestGUIHandler_GetSqueezeAndBitrate` affirmait
« the GUI never enables squeeze today », faux depuis C-04 (L-106). `GUIHandler.Show*` à 0 %
(`ShowProgressDetail` 40 %), `showPrerequisiteDialog` 13,3 %.

**Cause racine** — L-35 : un état de test qui ne peut pas atteindre la branche testée. L-106 :
une assertion d'absence qui n'a pas suivi l'arrivée de la fonction.

**Correctif** — Les deux tests `appState` posent un rapport de sonde utilisable ; le premier
sélectionne aussi `libx264 encoder` dans la liste (avant la sonde, pour que `encoderChanged` ne
lance pas de vérification en arrière-plan) et exige `libx264`, ni `nvenc`, ni « checking », ni
« not in approved list ». `GetSqueeze` testé dans les deux sens. Nouveau `gui_handler_test.go` :
`ShowProgress(42)` → 0,42 ; `ShowProgressDetail(42, 90 s)` → `formatEncodingStatus(42, 90 s)` ;
`ShowInfo` ouvre un dialogue portant le message ; `ShowError` route « cannot find
ffmpeg/ffprobe » vers le dialogue au lien d'installation et pas une autre erreur ;
`showPrerequisiteDialog` rend vrai (avec et sans *Retry*) pour cette erreur, faux et sans
dialogue pour une autre et pour `nil`. Les dialogues sont observés sur une vraie fenêtre de test
(`test.NewTempWindow`, `test.LaidOutObjects` sur l'overlay).
La garde de génération de `applyVerifiedHardwareLine` était déjà tenue par
`AStaleVerificationDoesNotOverwriteANewerLine` et `RefreshInvalidatesAVerificationInFlight` :
rien ajouté, contre-épreuves refaites.

**Contre-épreuves** — libellé brut passé au planificateur (ligne 593) → rouge (`encoder
libx264 encoder not available`) ; sélection ignorée → rouge (`planned h264_nvenc`) ; appels
`setEncoderProbe` retirés des deux tests → deux rouges (« checking ») ; `GetSqueeze` rendant
`false` puis `true` → rouge chaque fois ; `percent / 1000` → rouge (0,042) ; `ShowProgressDetail`
sans le temps restant → rouge ; `ShowError` sans `showPrerequisiteDialog` → rouge ;
`ShowInfo` sans le message → rouge ; `showPrerequisiteDialog` acceptant tout → trois rouges,
refusant tout → rouges ; garde de génération retirée → deux rouges ; `hardwareStatusGen++`
retiré → rouge.

**Couverture** (`go test . -covermode=atomic`, paquet racine, même machine)

| | avant (`98b8a5d`) | après |
| --- | --- | --- |
| `superview` | 36,3 % | 40,3 % |
| `ShowError` / `ShowInfo` / `ShowProgress` | 0,0 % | 100,0 % |
| `ShowProgressDetail` | 40,0 % | 100,0 % |
| `showPrerequisiteDialog` | 13,3 % | 73,3 % (branche `url.Parse` en échec inatteignable : constante) |
| `refreshHardwareStatus` | 92,3 % | 100,0 % |
| `verifyHardwareInBackground` | 18,2 % | 18,2 % |

**Non fait** — `verifyHardwareInBackground` reste à 18,2 % : sa goroutine lance une vraie sonde
ffmpeg ; sa seule décision, la garde de génération, est testée dans `applyVerifiedHardwareLine`.
Le passage de la case squeeze au `GUIHandler` se fait dans la closure du bouton *Start* de
`main()` (`squeeze := state.squeeze`, puis `squeeze: squeeze`) : aucune méthode ne construit le
handler, donc ce passage n'est pas testable sans extraction, non faite (tests seulement).

**Leçon** — Aucune nouvelle : L-35, L-37, L-86, L-106 s'appliquent.

### [2026-10-14] E-19 (partiel, côté `common`) — Combler les trous de la suite qu'une régression traverserait

| | |
| --- | --- |
| **Constat** | E-19, sous-items du paquet `common` ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `common/pgm.go` (une ligne), `common/pipeline.go` (commentaire) ; tests `common/stopcascade_test.go`, `common/toolresolve_test.go`, `common/earlyfailures_test.go` (nouveau), `common/refusals_test.go`, `common/integration_test.go`, `common/lognoise_test.go` |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ (seul le test VAAPI saute, pilote CQP) · 11 contre-épreuves ✅, toutes compilées |

**Symptôme** — Le chemin audio de la configuration livrée (`safe_performance` : copie puis
AAC) était à 0 % ; `findWindowsToolBinary` à 0 % sur les deux runners ; les sorties précoces de
`PerformEncoding` et leur étiquette `stage` jamais vérifiées ; les branches d'erreur de
`CheckFfmpeg` et le refus de `checkTempSpaceForMaps` jamais atteints ; `integrationUI.GetSqueeze`
rendait `false` en dur, donc squeeze ne traversait jamais le pipeline ; deux tests de
`lognoise_test.go` échouaient au lieu de sauter sans ffmpeg.

**Cause racine** — Des tests écrits sur la configuration par défaut (`nil` → `aac`) et non sur
celle qu'on livre ; un double de test qui ne pouvait pas atteindre l'état testé (L-35) : une valeur de retour codée en dur dans un double de test ; un
lecteur d'espace libre rendu injectable pour un appelant et pas pour l'autre.

**Correctif** — `checkTempSpaceForMaps` lit `readFreeDiskGB` (seule ligne de production ; lève
le « délibérément laissé » de l'entrée précédente). Tests : `SafePerformanceRetriesAudioAsAAC`
(trois cas : relance AAC = 2 lancements et `-c:a aac` au second ; échec après sortie = 1 ;
annulation pendant la copie = 1), nouveau double `fakeAudioCopyRefusingFFmpeg` (+ `.bat`) ;
`FindWindowsToolBinary_FindsAWingetPackagesTree` (arbre `Packages`, paquet hors ffmpeg ignoré,
`Links` prioritaire ; tourne sur tous les OS, variables jamais vides — E-22 non touché) ;
`CheckFfmpeg_ReportsAFailingEncodersQuery` (témoin + `-encoders` + `-hwaccels`, lookup
empaqueté neutralisé par `withFakeExecutableIn`) ; `PerformEncoding_EarlyFailuresAreRecorded`
(six cas, `stage` exact, rien dans le répertoire de sortie) ;
`CheckTempSpaceForMaps_RefusesWhenTmpIsFull` ; `Integration_SqueezeKeepsTheSourceWidth`
(640×480 → 640×480 en squeeze, 852×480 sinon) ; garde `requireRealFFmpegProcess` dans
`lognoise_test.go` (SKIP sans ffmpeg, FAIL sous `SUPERVIEW_REQUIRE_FFMPEG=1`, vérifié).
Le contrat « annulation » sur Windows : les tests bâtis sur `fakeHangingFFmpeg` ne sautent
pas sous Windows (variante `.bat`), rien à écrire.

**Contre-épreuves** — relance AAC retirée → cas 1 rouge (erreur, 1 lancement) ;
`shouldNotRetry` retiré de la relance audio → cas 2 et 3 rouges (2 lancements) ; parcours
`Packages` retiré → rouge (`""`) ; erreur `-encoders` ignorée → rouge ; erreur `-hwaccels`
ignorée → rouge ; étiquettes `encoder_sanitization`/`bitrate_validation` permutées → deux cas
rouges ; `return nil` après l'erreur de sortie enregistrée → rouge ; refus de
`checkTempSpaceForMaps` neutralisé → rouge ; retour à `getFreeDiskGB` → rouge ; `GeneratePGM(video,
false)` dans `PerformEncoding` → squeeze sort en 852×480, rouge ; gardes `lognoise` retirées,
`PATH` sans ffmpeg → deux FAIL.

**Couverture** (`SUPERVIEW_REQUIRE_FFMPEG=1 go test ./... -covermode=atomic`, même machine)

| | avant (`cc2f725`) | après |
| --- | --- | --- |
| Total module | 72,8 % | 75,2 % |
| `common` | 87,1 % | 90,3 % |
| `PerformEncoding` | 74,8 % | 85,9 % |
| `findWindowsToolBinary` | 0,0 % | 89,7 % |
| `CheckFfmpeg` | 92,0 % | 100,0 % |
| `checkTempSpaceForMaps` | 69,2 % | 92,3 % |
| `EncodeVideo` | 93,0 % | 94,5 % |

Seuil CI laissé à 50 % (DEC-27).

**Laissé à la PR GUI** — `appState` avec un rapport de sonde (les deux tests qui passent sur
la ligne d'attente), `GUIHandler.Show*`, `showPrerequisiteDialog`, `verifyHardwareInBackground`,
et l'assertion périmée de `TestGUIHandler_GetSqueezeAndBitrate` (« the GUI never enables
squeeze today », fausse depuis C-04) : tous dans `gui_main_test.go`, qu'une autre PR modifie.
Hors périmètre aussi : l'échec du `Rename` final (couvert par la PR pipeline) et la preuve
Windows « aucun ffmpeg orphelin » (pas de runner pour l'établir).

**Leçon** — Aucune nouvelle : L-35 (le double `integrationUI` ne pouvait pas atteindre
squeeze), L-40 (annulation tirée de la progression), L-41 (lancements comptés côté Go, pas dans
le faux ffmpeg), L-50 (`findWindowsToolBinary` testé hors de Windows plutôt que sauté), L-60
(garde ffmpeg) et L-86 (contre-épreuves toutes compilées) s'appliquent.

### [2026-10-14] E-22, E-23 (SessionError), DEC-33 — Ménage : un interblocage latent, un chemin relatif, le journal, le cgroup, le mode audio

| | |
| --- | --- |
| **Constat** | E-22, E-23 partie `SessionError`, DEC-33 ([ANALYSE.md § 3duodecies](ANALYSE.md), [DECISIONS.md](DECISIONS.md)) |
| **Fichiers** | `common/metrics.go`, `common/tools.go`, `common/observability.go`, `common/health.go`, `common/errors.go`, `common/session.go`, `common/config.go` ; tests `common/housekeeping_test.go`, `common/housekeeping_unix_test.go`, `common/config_test.go`, `common/observability_default_test.go` ; `README.md`, `README_FR.md`, `docs/CONTRATS.md`, `docs/DECISIONS.md` |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `GOOS=windows go vet ./common` ✅ · `golangci-lint` ✅ 0 alerte · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · 7 contre-épreuves ✅ |

**Symptôme** — Aucun observé : `Summary` et `LogMetrics` reprenaient `RLock` via `ElapsedTime` ;
une variable Windows vide donnait un candidat relatif résolu contre le répertoire courant ; le
plafond du journal ne jouait qu'à l'ouverture et le fichier était lisible de tous ; sous cgroup,
`MemAvailable` décrivait l'hôte ; `SessionError` n'était jamais rendu par la session ; un
utilisateur sans YAML (archives) réencodait l'audio.

**Correctif** — `elapsedLocked()` ; candidats Windows construits seulement depuis des variables
non vides ; `cappedLogWriter` (recoupe avant l'écriture qui dépasserait le plafond, `OpenLogFile`
rend désormais un `io.WriteCloser`, l'appelant de `gui_main.go` n'a pas changé) ; `memory.max`
et `memory.current` en variables de paquet ; `SessionError.Err` + `Unwrap` ; défaut
`safe_performance` dans le code, le tag et `normalizePerformanceMode("")`, les README, DEC-33
acceptée. Une valeur invalide retombe toujours sur `safe` (inchangé).

**Au passage** : trois tests (`fallbackmemory_test.go` ×2, `integration_test.go`) comptaient les
lancements d'un ffmpeg qui refuse tout et dépendaient en silence du défaut `safe` ; en
`safe_performance` un refus est rejoué une fois avec l'audio AAC, ce qui double les lancements.
Ils épinglent désormais `safe` (`reAudioConfig`), le sujet étant l'échelle de repli vidéo.

**Contre-épreuves** — `ElapsedTime()` rappelé sous `RLock` → `NoRecursiveRLock` rouge par
expiration de 5 s, 3 fois sur 3 ; `tools.go` d'origine → `Microsoft/WinGet/Links/ffmpeg.exe`
résolu, rouge ; plafond neutralisé → rouge à la douzième écriture ; modes 0644/0755 → rouge ;
cgroup ignoré → 8 GiB au lieu de 424 MiB, rouge ; `session.go` d'origine → rouge ; `""` → `safe`
→ rouge. Le test des valeurs par défaut de `LoadConfig` a été retourné avec le défaut.

**Leçon** — Aucune nouvelle : L-05 et DEC-28 s'appliquent.

### [2026-10-14] E-03, E-14, E-15, E-20, E-23 (partiel) — Quit, la croix et SIGTERM attendent tous que l'encodage se défasse

**PR** — *à ouvrir*

| | |
| --- | --- |
| **Constat** | E-03, E-14, E-15, E-20, E-23 (closures de `main()`, `fyneDo`) ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `gui_main.go`, `gui_main_test.go`, `common/gui_helpers.go`, `common/gui_helpers_test.go`, `FyneApp.toml`, `docs/DECISIONS.md` (DEC-34), `docs/LECONS.md`, `docs/ANALYSE.md` |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race` ✅ sauf **1 rouge attendu** (E-01, VAAPI/CQP, dépend du GPU) · GUI lancée 8 s (`timeout 8`, code 124 : vivante), aucune panique, aucun avertissement `fyne.Do` ; `kill -TERM` après 3 s : sortie code 0 en 0,1 s, journal sans panique · huit contre-épreuves sur les six tests, chacune rougie puis retirée : 255 relu comme annulation ; *Diagnostic* retiré de `locked()` ; le `case ErrCancelled` supprimé ; `shutdown` qui n'attend pas `done` ; `shutdown` qui n'annule pas (délai de 20 s atteint) ; la garde `shuttingDown` retirée ; `restoringSelection` ignoré ; `GetEncoder` qui ne rend plus le nom capturé |

**Symptôme** — *Quit* pendant un encodage : la fenêtre disparaît, ffmpeg continue jusqu'au
bout dans le fichier de travail, le répertoire de session reste. La croix demandait
confirmation puis dormait une seconde avant `app.Quit()`. SIGTERM : Fyne démontait sa boucle
pendant que `PerformEncoding` retirait encore le fichier partiel. *Cancel* se soldait par
*Status: Failed* et un dialogue **Error**. *Diagnostic* restait cliquable pendant un
encodage matériel, et son retour écrasait le statut par *Ready*. Le nom d'encodeur était lu
dans le widget depuis la goroutine, et un `SetSelected` programmé remplaçait pour de bon la
préférence sauvegardée. zenity en erreur (255) passait pour une annulation. Les sélecteurs
natifs gelaient la fenêtre.

**Cause racine** — Trois sorties, trois chemins, aucun ne connaissant la goroutine
d'encodage : rien ne disait quand elle avait fini. Les branches de fin étaient deux closures
dans `main()`, sans test ; `ErrCancelled` y tombait dans `default`. La liste `state.locked`
était écrite dans `main()`, hors d'atteinte des tests, et le bouton ajouté en 8ᵉ passe n'y
a jamais été mis.

**Correctif** — `beginEncoding()` rend aussi `done`, que la goroutine ferme par `defer` en
première ligne. `appState.shutdown(reason)` : annule, attend `done` jusqu'à 20 s (la sonde
pré-encodage, qui n'écoute pas l'annulation, a 15 s), journalise. Deux crochets y mènent :
`window.SetOnClosed` (synchrone dans `window.Close()`, donc dans `app.Quit()`) et la ligne
après `ShowAndRun()` — parce que, **vérifié dans fyne 2.8.2**, `catchTerm` appelle
`driver.Quit`, pas `window.Close` : `SetOnClosed` ne voit jamais SIGTERM. *Quit* et la croix
passent par la même confirmation. `completeEncoding(err, output)` est la seule branche de fin,
à quatre issues (succès, annulation sans dialogue, signal système, échec), gardée par
`shuttingDown` parce que Fyne exécute en ligne les `fyne.Do` en attente une fois sa boucle
arrêtée. `windowControls.locked()` calcule la liste verrouillée, *Diagnostic* compris, et le
test appelle cette fonction-là. `GUIHandler.encoderName` capturé sur le fil UI ;
`restoringSelection` autour des `SetSelected` programmés ; `refreshEncoderOptions` et
`encoderChanged` sont des méthodes. 255 n'est plus une annulation (DEC-34 amendée).
Sélecteurs natifs dans une goroutine, résultat livré par `fyne.Do`, bouton tenu enfoncé
pendant ce temps. `selectedQualityProfile` et `squeezeSource` repliés dans `appState`.
`[Migrations] fyneDo = true`. **Laissé** : le reste d'E-23 (`SessionError`, cache de sonde
annulée), PR 7.

**Relecture** — sept constats, chacun avec un test rougi par la remise du défaut puis
retiré (sauf le 5) :
(1) sans ffmpeg, le démarrage verrouillait `state.locked`, *Diagnostic* compris — l'outil
qui signale l'absence de ffmpeg devenait inaccessible. Deux listes :
`windowControls.requiresFFmpeg()` (sans *Diagnostic*) et `locked()`, appliquées par
`applyFFmpegAvailability`. Le retour de *Diagnostic* restaure désormais le statut d'avant
au lieu d'écrire *Ready* par-dessus *ffmpeg unavailable*. Contre-épreuve :
`requiresFFmpeg` qui rend `locked()` → rouge.
(2) Un sélecteur natif validé après *Start* remplaçait `state.video` en plein encodage.
`applyChosenInput` / `applyChosenOutput` (repli Fyne compris) l'ignorent et le
journalisent. Contre-épreuve : garde `isEncoding` neutralisée → rouge sur les deux.
(3) Après le vidage, `fyne.Do` s'exécute en ligne : un `ErrStoppedBySignal` menait sur la
goroutine d'encodage à `quit` → `Close` → `shutdown`, qui attendait le `done` que seul le
`defer` de cette même goroutine fermerait — 20 s d'auto-attente. Le corps de la goroutine
est devenu `runEncoding`, qui ferme `done` **avant** de poster la fin (et par `defer` pour
une panique). Contre-épreuves : fermeture par `defer` seul → rouge à 3,0 s ; `defer` retiré
→ le test de panique rougit. Correction au passage : les closures *en file* au vidage sont
jetées par `runGL`, pas exécutées ; seuls les `fyne.Do` postérieurs s'exécutent en ligne —
la phrase du correctif ci-dessus le disait mal.
(4) Une attente expirée dans le crochet de fermeture était refaite après `ShowAndRun()` :
40 s. `shutdownGaveUp` rend `false` aussitôt. Contre-épreuve : garde retirée → second appel
à 300 ms → rouge.
(5) Le label du rapport *Diagnostic* était construit hors du fil UI ; déplacé dans le
`fyne.Do`. Pas de test unitaire : vérifié par `go test -race -tags migrated_fynedo .`.
(6) `refreshEncoderOptions` préférait la sélection courante — un repli — à la préférence
sauvegardée : après une relance réussie, *hevc_nvenc* n'était pas restauré. Ordre :
préférence, puis sélection courante, puis première option. Contre-épreuve : ancien ordre →
rouge.
(7) Un reste < 0,5 s affichait « about 0s left » : la comparaison porte désormais sur la
valeur arrondie. Contre-épreuve : comparaison sur la valeur brute → rouge.

**Leçon** — L-109 ; L-102 corrigée ; L-110 (relecture).

---

### [2026-10-14] E-05, E-16, E-17, E-21, E-12 — Refuser ce que le pipeline ne sait pas faire, et dire combien de temps il reste

| | |
| --- | --- |
| **Constat** | E-05 (arbitrage : **refuser**, pas pivoter), E-16, E-17, E-21, E-12 ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `common/pgm.go`, `common/pipeline.go`, `common/metrics.go`, `common/health.go`, `common/security.go`, `common/health_disk_windows.go`, `common/config.go`, `gui_main.go`, `gui_native_dialog_windows.go` ; tests `common/geometry_test.go`, `common/refusals_test.go`, `common/security_unix_test.go`, `gui_config_test.go`, `gui_native_dialog_windows_test.go` ; `README.md`, `README_FR.md`, `docs/CONTRATS.md` |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `GOOS=windows go vet ./common` ✅ · `golangci-lint` ✅ 0 alerte · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · 22 contre-épreuves ✅. Le fichier Windows de la GUI : `GOOS=windows go vet` sur un module jetable ne contenant que lui et `common` ✅, test exécuté sous Linux sur une copie sans étiquette de build ; **jamais exécuté sous Windows** |

**Symptôme** — Une source portrait sortait pliée sans erreur ; un disque de destination plein
était découvert vingt minutes après le début ; l'estimation affichait des heures au premier
pourcent ; `<tmp>/out → /usr` n'était arrêté que par le test d'écriture (pas sous root) ;
Windows lisait le total libre et non l'espace de l'utilisateur ; une clé YAML mal orthographiée
était ignorée, `min > max` accepté, `"264, 265"` perdait HEVC ; le sélecteur Windows rendait
`C:\Users\Cédric` en U+FFFD.

**Cause racine** — Aucune de ces limites n'était vérifiée là où elle se décide : la carte X
non-squeeze n'est monotone que pour H/W ≤ 9/8 et le commentaire de `putMapSample` affirmait le
contraire ; l'origine de l'estimation était posée avant validation, cartes et première trame ;
le contrôle système lisait le chemin lexical ; `yaml.Unmarshal` ignore les clés inconnues ;
`[Console]::Out` suit la page de code OEM.

**Correctif** — `checkGeometry` (sur le `outX` réellement calculé, `outX ≤ 2W`), appelé dans
`PerformEncoding` (étape `geometry_check`) et en tête de `GeneratePGM` ; `checkOutputSpace`
(débit × durée / 8 × 1,15 contre `readFreeDiskGB`, lecture en échec = pas de refus), étape
`output_space_check` ; estimation datée de la première progression non nulle, rien sous 2 %,
origine reprise quand la progression recule (repli d'encodeur) ; parent résolu par
`EvalSymlinks` puis contrôle système relancé ; `freeBytesAvailable` ; `KnownFields(true)`
(document vide toléré), `min > max` refusé après l'environnement, codecs épurés avec repli sur
les défauts ; écriture UTF-8 explicite dans les deux scripts PowerShell. **Au passage** : la
GUI rechargeait `LoadConfig("")` en jetant l'erreur — devenue possible (`min > max` par
l'environnement), elle aurait rendu `nil` ; `loadStartupConfig` retombe sur
`common.DefaultConfig()`. **Délibérément laissé** : une configuration refusée n'est signalée
que dans le journal, pas par un dialogue ; `checkTempSpaceForMaps` lit toujours
`getFreeDiskGB` directement.

**Contre-épreuves** — `checkGeometry` neutralisé → `CheckGeometry` rouge ; appel de
`GeneratePGM` retiré → rouge (session ouverte, la carte s'écrit) ; appel de `PerformEncoding`
retiré → `PortraitSourceIsRefusedUpFront` rouge (le contrôle mémoire tourne) ; 1080×1920
laissé passer → `WidenMapStaysWellFormed` rouge (x=2497 échantillonne 1080) ; signe du décalage
inversé → rouge ; carte Y décalée d'un → `YMapIsIdentity` rouge ; `checkOutputSpace` sans refus,
refus sur lecture en échec, appel retiré → trois rouges ; estimation depuis `StartTime` → 24 h
au lieu de < 1 min ; plancher de 2 % retiré → rouge ; origine non reprise → rouge ; parent non
résolu → rouge sur « permission denied » ; préfixe sans séparateur → `/etcetera` rouge ; contrôle
jamais vrai → rouge ; `KnownFields(false)`, `min > max` accepté, codecs non épurés, liste vide
gardée, document vide refusé → cinq rouges ; repli `DefaultConfig` retiré → `nil` ; script
PowerShell d'origine → test Windows rouge (exécuté sous Linux sur copie).

**Leçon** — Aucune nouvelle : L-05/L-09 (le commentaire de `putMapSample` était une affirmation
jamais testée) et DEC-28 s'appliquent.

### [2026-10-14] E-06, E-11, E-18, E-24 — La CI ne tournait plus sur `master`, jamais sous `-race`, jamais contre le FFmpeg livré, et deux fois pour rien

| | |
| --- | --- |
| **Constat** | E-06, E-11, E-18, E-24 ([ANALYSE.md § 3duodecies](ANALYSE.md)) ; accélération des workflows demandée en cours de route |
| **Fichiers** | `.github/workflows/test.yml`, `lint.yml`, `release.yml`, `merge-on-label.yml`, `AGENTS.md` (une ligne du tableau), `RELEASING.md` (§ *Manual pins*), `docs/MERGING.md`, `docs/DECISIONS.md` (DEC-35, DEC-04, DEC-24), `docs/ANALYSE.md`, `docs/LECONS.md` |
| **PR** | *à ouvrir* |
| **Vérification** | workflows et documentation seuls, aucun `.go` touché · `python3 -c 'yaml.safe_load'` sur les quatre fichiers ✅ · `actionlint` v1.7.12 ✅ 0 alerte, contre-épreuve : un `steps.landedd` inventé le fait rougir ✅, un nom d'entrée `cache-dependency-pth` **ne le fait pas** (les actions sont épinglées par SHA, actionlint ne connaît pas leurs entrées) — entrées et sorties de `setup-go` lues dans son `action.yml` au SHA épinglé (`cache`, `cache-dependency-path`, sortie `go-version`), `use_oidc` dans le README de `codecov-action` v7 · chaque `uses:` : SHA résolu vers son tag par `gh api .../git/ref/tags/…` (tags annotés déréférencés), commentaires portés en `vX.Y.Z` · contre-épreuve du gate : `needs: [test, test-windowz]` fait rougir actionlint ✅ · noms des workflows `test.yml`/`lint.yml` confirmés par `gh api .../actions/workflows` · contextes requis du ruleset relus, aucun renommé · archive gyan.dev 8.1.1 téléchargée : SHA-256 identique au pin, un seul `bin/` avec `ffmpeg.exe` et `ffprobe.exe` ✅ · `gofmt` ✅ · `go build ./...` ✅ · `TestInstallURLFragmentPointsAtARealHeading` ✅ · **non vérifié ici** : l'étape PowerShell (pas de `pwsh`), le dispatch réel (ne joue qu'à la fusion suivante), l'OIDC côté Codecov |

**Symptôme** — Cinq fusions (#73 à #77) et aucun run sur `master`. Quatre `go test ./...` sans
`-race` alors que le `Makefile` et `AGENTS.md` l'exigent. La release testait contre le
`ffmpeg` d'apt (6.1.1) et de choco (9.0.2), jamais contre les 8.1.1/n8.1.2 qu'elle livre, et
`choco install ffmpeg` sans version ni relance avait déjà tué un run sur un 504. Codecov
échouait à chaque run (`Token required - not valid tokenless upload`) sous un `success`.
`contents: write` sur tous les jobs de la release, un double cache, une suite jouée deux fois
pour le même profil, un commentaire de couverture qui aurait fait échouer le contrôle
obligatoire sur une PR de fork, deux commentaires faux.

**Cause racine** — E-06 : la moitié de L-100 jamais appliquée (L-104), et personne n'avait lu
la subordonnée de la phrase de GitHub qui donne la solution (L-107). E-11 : la CI a été écrite
avant que `-race` ne devienne la règle, et `SUPERVIEW_FFMPEG_DIR` a été ajouté pour les
utilisateurs sans que la release s'en serve. E-18 : deux amortisseurs (`fail_ci_if_error:
false`, `continue-on-error`) posés pour qu'un service tiers ne bloque pas, qui ont caché qu'il
ne marchait jamais. E-24 : accrétion. Et en mesurant les durées pour l'accélération, un défaut
que personne n'avait vu : le job `fmt` sauvait un cache vide sous la clé commune (L-108).

**Correctif** —
- **E-06** : `on: workflow_dispatch` sur `test.yml` et `lint.yml` ; dans `merge-on-label.yml`
  l'attente de la fusion devient une étape à part (`id: landed`), dont dépendent la
  suppression de branche (inchangée, toujours ni fork ni pile) et une nouvelle étape qui
  dispatche les deux workflows sur la branche par défaut, en meilleur effort ; `actions: write`
  sur le job. Ni ruleset strict ni jeton d'application : DEC-35 dit pourquoi.
- **E-11** : `-race -count=1` sur toutes les invocations Linux (`-count=1` parce que le cache
  de build, désormais réellement restauré, laisserait `go test` répondre `(cached)`). Windows
  reste sans `-race` : le détecteur y doublerait le job le plus long du workflow (7m38 de
  `go test`) pour des courses qui sont dans du Go portable. `release.yml`, job Linux : la suite
  tourne contre le binaire téléchargé et vérifié, `SUPERVIEW_FFMPEG_DIR=$RUNNER_TEMP/ffbin`.
  `test.yml`, Windows : `choco` remplacé par l'archive gyan.dev 8.1.1 de `release.yml` (même
  URL, même SHA-256, `Get-FileHash`, `7z x`, `bin/` ajouté à `GITHUB_PATH`), et
  `ffmpeg -version` doit contenir `8.1.1` pour qu'un ffmpeg de l'image ne réponde pas à sa
  place.
- **E-18** : l'envoi Codecov est **retiré**. Premier essai par OIDC (`use_oidc`, `id-token:
  write`, `fail_ci_if_error: true`) sur cette PR même : le jeton est bien émis (1 922
  caractères), et Codecov répond `Repository not found`. Le dépôt n'a jamais été activé sur
  codecov.io : aucun rapport n'est jamais arrivé, ni avant ni après. Le gate de couverture
  interne décide seul, et le commentaire de PR affiche le chiffre. À réactiver seulement après
  activation du dépôt sur codecov.io, geste de compte que le dépôt ne peut pas faire. Aucun
  badge à retirer. Le même run a vérifié ce qui ne pouvait pas l'être en local : `-race` sous
  Linux, et l'archive gyan.dev 8.1.1 sous Windows, SHA-256 compris.
- **E-24** : étape `actions/cache` supprimée, `cache-dependency-path: go.sum` partout ;
  `contents: read` en tête de `release.yml`, `write` sur `create-release` seul ; commentaire de
  couverture conditionné au même dépôt et `continue-on-error` ; `coverage.out` publié en
  artefact et téléchargé par le gate ; une seule exécution dans le job `test` de la release ;
  `notify` passe tout par `env:` ; les deux commentaires corrigés ; § *Manual pins* dans
  `RELEASING.md`.
- **Accélération**, sans retirer une vérification ni renommer un contrôle requis :
  `concurrency` sur `test.yml` et `lint.yml` (annulation sur PR seulement) ; `test` scindé en
  deux jobs aux noms inchangés, le gate `needs: [test, test-windows]` — aucun des deux n'est un
  contrôle requis, c'est le gate qui porte leur verdict, et un premier jet qui ne lui faisait
  attendre que Linux laissait un Windows rouge fusionner (vu à la relecture) ; `native-build-matrix`
  sans `needs: test` (les deux sont des contrôles requis, et le gate bloque déjà sur un test
  rouge) ; `coverage-report` sans en-têtes GUI, sans ffmpeg, sans cache ; `fmt` avec
  `cache: false` (L-108) ; binaires `golangci-lint` et `govulncheck` en cache, clé = version de
  l'outil + version de Go ; `--no-install-recommends` ; `go mod download` retiré (deux secondes
  que `go test` refait de toute façon). `lint` et `govulncheck` restent deux jobs : ils
  tournent en parallèle, un seul job ferait leur somme.

Durées mesurées sur le dernier run vert de `master` (`801660d`, runs 37425624470 et
37425624499, `gh run view --json jobs`) et gain attendu ; **à remesurer sur le premier run de
cette PR**, `-race` ralentissant le job Linux d'un facteur que seul le run dira :

| Job | Avant (801660d) | Après, attendu |
| --- | --- | --- |
| Test ubuntu | 3m49 (apt 32 s, `go test` 3m00 à cache vide) | `-race` alourdit, le cache de build restauré allège ; ordre de grandeur égal |
| Test windows | 8m49 (choco 31 s, `go test` 7m38) | ≈ égal (le téléchargement gyan.dev vaut choco) ; reste le chemin critique |
| Coverage Report & Gate | 2m52, démarré à +8m53, après Windows | ≈ 20 s, toujours après Windows : il ne rejoue plus la suite |
| Native Build ubuntu / windows | 3m24 / 1m23, démarrés à +8m53 | inchangés, démarrés à +0 |
| **test.yml bout en bout** | **12m17** | **≈ 9m10**, borné par le job Windows puis le gate |
| golangci-lint | 3m35 (apt 32 s, `go install` + run 2m54) | la compilation du linter disparaît sur cache chaud |
| Vulnerability Scan | 2m24 | idem |
| Code Format Check / README parity | 7 s / 4 s | inchangés, et `fmt` ne sauve plus de cache vide |
| **lint.yml bout en bout** | **3m37** | borné par golangci-lint, en baisse de la compilation de l'outil |

**Ce qui est délibérément laissé** — Le job Windows de la release ne lance pas la suite
contre son FFmpeg : `test.yml` le fait déjà avec la même archive. Le commentaire de couverture
poste encore un commentaire par push. **La première fusion après celle-ci ne dispatchera
rien** — `pull_request_target` exécute le `merge-on-label.yml` de `master`, l'ancien :
`gh workflow run test.yml --ref master` et `lint.yml` à la main, une fois.

**Leçon** — L-107, L-108.

### [2026-10-14] E-01, E-07, E-08, E-09, E-23 (cache) — La sonde pose la question de l'encodage ; presets par encodeur ; `hvc1`

| | |
| --- | --- |
| **Constat** | E-01 (arbitrage **a**), E-07, E-08, E-09, E-23 partie cache ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `common/encode.go`, `common/probe.go`, `common/hardware.go`, `common/pipeline.go` ; tests `common/probequestion_test.go`, `common/probe_test.go`, `common/sizeprobe_test.go` ; `README.md`, `README_FR.md`, `docs/hardware-support.md` |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ **plus aucun rouge** : le test VAAPI saute avec la raison du pilote · 6 contre-épreuves ✅ |

**Symptôme** — `h264_vaapi` passait la sonde et refusait chaque conversion (CQP seul) ; un
`video_preset` x264 tuait NVENC (code 234) et basculait sur le CPU ; « Fast » valait `p1`
sur NVENC ; `-hwaccel qsv` livrait des trames GPU à `remap` et `-hwaccel drm` échouait,
un ffmpeg gaspillé par conversion ; les sorties HEVC étaient taguées `hev1`.

**Cause racine** — La sonde et la conversion construisaient leurs options chacune de leur
côté, et le test « même question » ne comparait que deux d'entre elles (L-103) ; la table
de presets ne connaissait qu'AMF et QSV ; deux jetons de décodage n'étaient pas des
`-hwaccel` ; le tag HEVC par défaut de ffmpeg est celui qu'Apple refuse.

**Correctif** — `encoderFacingArgs` (codec, débit, preset, tag) et `pixelFormatChain`
partagés par `buildEncodeBaseArgs` et `probeArgs` ; `probeSpec` porte encodeur, taille,
profondeur source, débit et preset, et sert de clé de cache ; `verifyEncoderForOutput`
reçoit le preset de la configuration ; table NVENC `p1`–`p7` (`fast → p3`), `""` pour les
familles à trames sur périphérique ; `qsv` retiré des candidats de décodage, `drm` retiré ;
`-tag:v hvc1` pour tout encodeur HEVC ; une sonde annulée par son appelant n'est plus mise
en cache. `TestProbeAndConversionAskTheSameQuestion` compare désormais **l'argv entier** et
liste les différences permises. **Délibérément laissé** : la ligne *Hardware* de la fenêtre
sonde sans preset (le preset n'est connu qu'au lancement) ; l'option (b) « s'adapter au
CQP » n'est pas écrite.

**Contre-épreuves** — sonde sans options d'encodeur → `AskForRateControl` et
`AskTheSameQuestion` rouges ; sonde en 8 bits → `AskAtTheSourceDepth` rouge ; table NVENC
retirée → rouge ; tag retiré → test unitaire **et** intégration (ffprobe lit `hev1`) rouges ;
`qsv` remis → rouge ; cache sans garde → rouge.

**Leçon** — L-103, écrite par la 13ᵉ passe, appliquée ici.

### [2026-10-14] E-02, E-13, E-04, E-10 — L'arrêt est honoré, l'échec tardif n'est plus rejoué, le fichier terminé n'est plus effacé

| | |
| --- | --- |
| **Constat** | E-02, E-13, E-04, E-10 ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `common/errors.go`, `common/encode.go`, `common/pipeline.go`, `common/security.go` ; tests `common/stopcascade_test.go`, `common/partialoutput_unix_test.go`, `common/integration_test.go` |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ (seul rouge : E-01, inchangé) · 5 contre-épreuves ✅ |

**Symptôme** — Un SIGTERM lançait six ffmpeg et pouvait finir en « Transform complete » ;
un échec après des minutes d'encodage rejouait jusqu'à cinq encodages complets ; un
renommage final en échec effaçait la vidéo terminée ; le fichier produit était en 0600.

**Cause racine** — Les trois portes de la cascade testaient `ErrCancelled` seul (la
sentinelle d'U-09 n'y avait pas été reportée, L-101) ; la cascade ne distinguait pas un
échec à l'ouverture d'un échec après production (L-105) ; le `defer` de nettoyage ne savait
pas que l'encodage avait réussi ; `os.CreateTemp` crée en 0600.

**Correctif** — `isStopRequest` et `shouldNotRetry` (`errors.go`), employés aux trois portes ;
`errFailedAfterOutput` posé par `run()` dès qu'une progression strictement positive a été
vue, enveloppé avec `%w` ; à l'échec du renommage, `keepPartial = true`, journal en `Error`
et message qui nomme le fichier de travail ; `isValidOutputPath` refuse une destination
qui est un répertoire ; `newPartialOutputPath` crée en `O_EXCL` 0666 sous umask avec un nom
aléatoire. **Délibérément laissé** : la cascade reste intacte pour les échecs d'ouverture
(`TestEncodeVideo_FallbackProceedsWhenMemoryAllows` compte toujours 3).

**Contre-épreuves** — porte réduite à `ErrCancelled` → `SystemSignalStopsTheFallbackCascade`
rouge (6 lancements) ; marqueur retiré → `FailureAfterOutputIsNotRetried` rouge ;
`keepPartial` laissé faux → `RenameFailureKeepsTheFinishedFile` rouge ; mode 0600 →
`HonoursUmask` rouge ; contrôle de répertoire retiré → `RejectsADirectory` rouge.

**Leçon** — L-101 et L-105, écrites par la 13ᵉ passe, appliquées ici.

### [2026-10-14] DEC-36 — `common.go` découpé en huit fichiers, un par responsabilité

| | |
| --- | --- |
| **Constat** | aucun — décision de structure, [DEC-36](DECISIONS.md), demandée par l'utilisateur |
| **Fichiers** | `common/common.go` → `errors.go`, `tools.go`, `session.go`, `video.go`, `pgm.go`, `memory.go`, `encode.go`, `pipeline.go` ; `README.md`, `README_FR.md`, `AGENTS.md`, `docs/CONTRATS.md`, `readme_link_test.go` (références) |
| **PR** | *à ouvrir* |
| **Vérification** | `gofmt` ✅ · `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ (seul rouge : E-01, inchangé) · test doré PGM ✅ |

**Symptôme** — Un fichier de 2 023 lignes et huit responsabilités ; deux commentaires de doc
détachés de leur déclaration, preuve qu'on y insérait sans relire.

**Cause racine** — Le fichier d'origine du projet a reçu chaque ajout faute d'un endroit
nommé pour le recevoir.

**Correctif** — Découpage par déplacements purs, à partir d'une carte des déclarations et de
leurs blocs de commentaires (script jetable, chaque ligne non vide de l'original placée
exactement une fois, vérifié). `goimports` a élagué les imports. Les deux commentaires
détachés sont rattachés. Les références `common.go:NNN` de la prose remplacées par
fichier + symbole. **Délibérément laissé** : `hardware.go` (607 lignes) et `main()`.

**Leçon** — aucune nouvelle ; L-98 (une référence de ligne dans un document périme) s'applique.

### [2026-10-14] 13ᵉ passe — état des lieux complet, aucune correction de code

| | |
| --- | --- |
| **Constat** | E-01 à E-25 ([ANALYSE.md § 3duodecies](ANALYSE.md)) |
| **Fichiers** | `docs/ANALYSE.md`, `docs/DECISIONS.md` (nouveau), `docs/LECONS.md`, `docs/CONTRATS.md`, `AGENTS.md`, `CLAUDE.md`, `.claude/` (nouveau) |
| **PR** | *à ouvrir* |
| **Vérification** | `go build` ✅ · `go vet` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race` ❌ **1 rouge attendu** (E-01, VAAPI/CQP, dépend du GPU) · couverture 71,1 % |

**Symptôme** — Douze passes, 107 constats clos, et un seul test rouge connu. La demande était
un état des lieux complet confronté aux documentations officielles, avec un registre des
décisions et un plan d'actions.

**Cause racine** — Sans objet : passe d'analyse. Ce qu'elle a trouvé relève de quatre
familles : des correctifs appliqués à moitié (E-02 pour U-09, E-06 pour L-100, E-14 pour
U-09), des défauts introduits par un correctif (E-04 et E-10 par P-04), des faits FFmpeg
jamais mesurés (E-07, E-08, E-09), et des tests verts à vide (E-19).

**Correctif** — Aucun dans le code. `DECISIONS.md` créé (34 décisions, dont une à
réexaminer), `.claude/` créé (autorisations, crochet `gofmt`, compétence `/verify`), six
leçons. Le plan d'actions, en sept PR, est en fin de § 3duodecies. Trois contre-épreuves
jetables ont été écrites puis retirées (E-02, E-17, et la géométrie de E-05).

**Leçon** — L-101 à L-106.

### [2026-10-14] M-06 — Les branches fusionnées par le workflow n'étaient pas supprimées

**PR** — #77

| | |
| --- | --- |
| **Constat** | M-06 ([ANALYSE.md § 3undecies](ANALYSE.md)) |
| **Fichiers** | `.github/workflows/merge-on-label.yml` (pas *Delete the branch once merged*), `docs/MERGING.md`, `docs/ANALYSE.md`, `docs/LECONS.md` |
| **Vérification** | workflow et documentation seuls · YAML valide · requêtes du script rejouées sur l'API réelle : `autoMergeRequest` vaut `true` pour #73 armée et `false` pour #76 fusionnée, `DELETE` d'une référence absente renvoie `Reference does not exist` (422) sur stderr · contre-épreuve en conditions réelles : la branche de cette PR doit disparaître après sa fusion par le workflow, comme celle de #76 ne l'a pas fait |

**Symptôme** — Après la fusion de #76 par le workflow, `origin/ci/go-check-latest` existait
encore, malgré `delete_branch_on_merge: true`.

**Cause racine** — La fusion est faite avec le `GITHUB_TOKEN` du workflow, directement ou en
armant l'auto-fusion, que GitHub termine au nom de qui l'a armée. Aucun automatisme de GitHub
ne réagit à ce jeton. #74 et #75, fusionnées de la même façon, ont perdu leur branche parce
que Dependabot les a supprimées lui-même.

**Correctif** — Second pas dans le workflow : attente de la fusion (une heure au plus),
puis suppression par l'API. Abstention si une PR ouverte est basée sur la branche (la
suppression par l'API la fermerait, quand le réglage l'aurait reciblée), si la branche vit
dans un fork, si la fusion a été désarmée ou la PR fermée. Branche déjà supprimée = succès.

**Leçon** — L-100.

---

### [2026-10-14] M-05 — La CI prenait le Go en cache du runner, pas le dernier correctif

**PR** — #76

| | |
| --- | --- |
| **Constat** | M-05 ([ANALYSE.md § 3undecies](ANALYSE.md)) |
| **Fichiers** | `.github/workflows/lint.yml`, `.github/workflows/test.yml`, `.github/workflows/release.yml` (`check-latest: true` sur les neuf étapes `setup-go`), `docs/ENVIRONNEMENT.md` (Go 1.26.9), `docs/ANALYSE.md`, `docs/LECONS.md` |
| **Vérification** | workflows et documentation seuls, aucun fichier Go modifié · Go 1.26.9 local · `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint run ./...` 0 alerte ✅ · `govulncheck ./...` 0 vulnérabilité ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ sauf le test VAAPI, rouge pour une raison matérielle consignée à part · contre-épreuve : le job `Vulnerability Scan` de cette PR doit installer 1.26.9 et passer, celui des PR Dependabot non rebasées reste rouge |

**Symptôme** — #73, #74 et #75, trois montées de version Dependabot, bloquées sur la
vérification obligatoire `Vulnerability Scan` : neuf vulnérabilités de la bibliothèque
standard, GO-2026-6603 à GO-2026-6617, toutes `Fixed in: …@go1.26.9`. `master` était vert au
dernier passage, le 2026-10-06.

**Cause racine** — Le journal du job : `Setup go version spec 1.26.x` → `Found in cache @
…/go/1.26.8`. Le motif `1.26.x` est satisfait par la version en cache, et `setup-go` ne
cherche pas plus loin sans `check-latest`. Les vulnérabilités ont été publiées entre le
dernier vert de `master` et l'ouverture des PR.

**Correctif** — `check-latest: true` sur les neuf étapes `setup-go`, y compris celles de
`release.yml`, qui aurait sinon compilé les binaires publiés avec 1.26.8. Mesuré avant de
choisir l'ampleur du correctif : avec Go 1.26.9, `govulncheck ./...` rend 0 vulnérabilité
**sans** toucher `golang.org/x/net` v0.55.0, que cinq des identifiants citent aussi — la
montée de x/net envisagée d'abord n'est pas nécessaire et n'est pas faite.

**Découvert en chemin** — L'échec `Test Go 1.26.x on windows-latest` de #74 est un 504 de
`community.chocolatey.org` pendant `choco install ffmpeg`, sans rapport avec la PR. Et sur la
machine de vérification, le pilote VAAPI n'accepte que le mode de débit CQP : la sonde passe,
la conversion retombe sur le CPU (ANALYSE.md § 3undecies, à traiter dans sa propre PR).

**Leçon** — L-99.

---

### [2026-09-11] M-04 — Les chiffres et la numérotation qui servaient à naviguer avaient dérivé

**PR** — #70

| | |
| --- | --- |
| **Constat** | M-04 ([ANALYSE.md § 3decies](ANALYSE.md)) |
| **Fichiers** | `AGENTS.md` (tableau *decisions that look like defects*, § *Landing a change*), `docs/ANALYSE.md` (en-tête, § 4, renumérotation § 5/§ 6), `docs/LECONS.md` (en-tête : comptes et légende des préfixes, plus le renvoi) |
| **Vérification** | documentation seule, aucun fichier Go modifié · `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · chiffres remesurés à `996bed5` (2023 lignes, `main()` 585, `common.go` 82,7 %) ✅ · total des constats recompté depuis le § 4, 105 ✅ · plus aucun renvoi pendant à `§ 5bis` ✅ |

**Symptôme** — `AGENTS.md` donnait `common/common.go` à « ~1600 lines » (2023 en réalité) et
`main()` à « ~540 lines » (585). L'en-tête d'`ANALYSE.md`, celui qui sert à naviguer le
document, annonçait « cinq passes et les 67 constats » et « Dernière passe : 9ᵉ », et son
ordre de lecture s'arrêtait à § 3octies. `## 5bis` était placé **avant** `## 5`, alors que
*bis* veut dire « après ». Le § 4 portait deux lignes « 11ᵉ passe » distinctes, séquelle de
deux éditions successives dans la même passe.

**Cause racine** — Rien ne compile un document. Ces phrases étaient vraies à l'écriture, et
une passe met à jour le corps sans relire l'en-tête qu'elle vient de périmer. Le balayage de
L-80 ne les attrape pas : il porte sur ce que la documentation *promet* (installer,
vérifier, recevoir), pas sur ce qu'elle *compte*.

**Correctif** — Chiffres remesurés et **arrondis à la centaine**, pour qu'ils vieillissent
plus lentement qu'une valeur précise et fausse. En-tête refait, ordre de lecture complété, et
une phrase ajoutée qui dit qu'il périme : ajouter une passe oblige à le rouvrir.
« Questions ouvertes » devient § 5 et « Journal des révisions » § 6 — la renumérotation plutôt
que le déplacement, pour que le journal reste la dernière section comme dans ce fichier-ci.
Les trois renvois à `§ 5bis` suivent ; l'entrée historique du 2026-09-04 garde sa formulation
d'époque avec la mention du renumérotage, plutôt que d'être réécrite. Les deux lignes du § 4
sont fusionnées en une, à quatre constats.

**Aussi dans cette PR, hors constat** — la façon de poser le label `merge` depuis un terminal
est écrite dans `AGENTS.md` : `gh pr edit --add-label` échoue sur ce dépôt (il lit
`projectCards`, retiré avec les Projects classiques) et **échoue en silence** si stderr est
redirigé, ce qui a fait croire une fois que `merge-on-label.yml` ne se déclenchait pas. Le
contournement est l'endpoint REST.

**Délibérément laissé** — le point 3 de la revue d'entretien, `EventRecorder.RegisterHandler`
sans appelant en production, en attente d'arbitrage utilisateur : c'est une suppression d'API
exportée, pas une correction de documentation.

**Balayage de L-98, exécuté** — ce fichier-ci portait la même dérive, trouvée en la
cherchant : « 80 règles » pour 98, « les 67 constats sont clos » pour 105, et une légende
des préfixes qui s'arrêtait à `R-xx`, sans `D/V`, `U` ni `M`. `CONTRATS.md`,
`ENVIRONNEMENT.md`, `hardware-support.md` et les deux README ne portent aucun compte :
rien à y dériver.

**Leçon** — L-98.

---

### [2026-09-11] M-03 — La règle de fusion était fausse, et personne n'a plus à se la rappeler

**PR** — #67

| | |
| --- | --- |
| **Constat** | M-03 ([ANALYSE.md § 3decies](ANALYSE.md)) |
| **Fichiers** | `AGENTS.md` (§ *Landing a change*), `.github/workflows/merge-on-label.yml` (nouveau) ; hors dépôt : réglages de fusion et ruleset `master` |
| **Vérification** | `actionlint` 0 alerte sur le nouveau workflow **et sur les six autres** ✅ · YAML et corps shell reparsés ✅ · logique de décision rejouée contre l'API sur #66 et #67 ✅ · contre-épreuve du ruleset : push direct sur `master` refusé, *« Changes must be made through a pull request. 7 of 7 required status checks are expected »* ✅ · `can_bypass` relu = `pull_requests_only` ✅ |

**Symptôme** — `AGENTS.md` concluait sa règle de fusion par « The `base` field says which
case you are in: if it is not `master`, you are stacked. » C'est faux pour la PR du bas
d'une pile, qui a `master` pour base et dont le squash réécrit le SHA sur lequel la
suivante est posée. #59, #61 et #62 ont toutes dû être fusionnées par commit de fusion
contre ce que le fichier prescrivait.

**Cause racine** — La règle est décidable par une requête (« une PR ouverte est-elle basée
sur ma branche ? ») mais était formulée par un raccourci lisible qui se trouve être faux.
Et même corrigée, elle restait une chose à se rappeler **devant le bouton**, c'est-à-dire au
seul moment où personne ne relit le fichier.

**Correctif** — La phrase est remplacée par le vrai critère et sa commande. Surtout,
`merge-on-label.yml` l'applique : label `merge` → le workflow interroge l'API, choisit
squash ou commit de fusion, arme la fusion pour quand les contrôles requis passent, et
commente la PR avec sa décision et sa raison. Il traite les états inconfortables au lieu
de les supposer absents — brouillon refusé, conflit signalé, `mergeStateStatus` à `UNKNOWN`
lu comme « armer et attendre » et non « fusionner maintenant ». Découvert en le mesurant :
`UNKNOWN` n'est pas transitoire, GitHub l'a renvoyé sur #66 pendant plusieurs minutes alors
que #67 répondait `CLEAN`.

Trois réglages et un ruleset tiennent le reste, et retirent les sous-questions plutôt que
de les documenter : auto-merge activé ; **« Rebase and merge » retiré du menu** (il n'écrit
ni `Title (#NN)` ni commit de fusion, et n'a jamais servi ici) ; **titre de squash épinglé
au titre de la PR** — il valait `COMMIT_OR_PR_TITLE`, donc sur une PR à un seul commit il
prenait le sujet du **commit**, contredisant « Commit messages do not appear there » ;
**ruleset sur `master`** qui refuse le push direct et impose sept contrôles.

**Sur le choix des sept** — `Coverage Report & Gate` et les deux `Native Build` déclarent
`needs: test`, donc exiger ces trois-là exige **transitivement** les deux jobs de test sans
nommer la version de Go. Exiger `Test Go 1.26.x on ubuntu-latest` aurait fait qu'une montée
de version renomme un contrôle requis et bloque toute fusion, en silence. L'administrateur
garde un contournement **limité aux PR** : il ne peut pas pousser sur `master`, mais peut
forcer une fusion si un contrôle se coince — pas de verrouillage possible.

**Délibérément laissé** — la fusion entièrement automatique, sans label. Arbitrage
utilisateur : il veut décider **quand**, plus jamais **comment**.

**Leçon** — L-97.

---

### [2026-09-11] M-01 + M-02 — Le `Makefile` lance maintenant les tests comme la CI les lance

**PR** — #66

| | |
| --- | --- |
| **Constats** | M-01 et M-02 ([ANALYSE.md § 3decies](ANALYSE.md)) |
| **Fichiers** | `Makefile` (cibles `test`, `coverage`, `clean`, texte d'aide), `README.md`, `README_FR.md`, `docs/CONTRATS.md` (commande d'intégration sans garde-fou, trouvée par le balayage de L-96 ; ligne 10 du tableau des contrats) |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · `make check` de bout en bout ✅, couverture module 71,1 % · job `readme-parity` rejoué à la main ✅ · 2 contre-épreuves ✅ |

**Symptôme** — `make test` rendait 0 sur une machine sans ffmpeg, en ayant sauté quinze
tests dont les sept `TestIntegration_*` et `TestGeneratePGM_RemapOutputIsStable` : tout ce
qui vérifie une conversion réelle de bout en bout. `make clean`, lui, supprimait `dist/` —
produit par rien dans ce dépôt — et laissait les six résidus que `fyne package` écrit dans
la racine.

**Cause racine** — Le garde-fou existait et n'était appliqué que par la CI. `test.yml` pose
`SUPERVIEW_REQUIRE_FFMPEG=1` depuis le chantier D, `AGENTS.md` en fait un paragraphe du §
*Verifying a change*, mais les cibles `test` et `coverage` ne l'ont jamais posé. Pour
`clean`, la liste des résidus d'empaquetage s'est construite dans `.gitignore`, une
publication à la fois, au fil de R-06 ; personne n'a répercuté dans la cible censée les
enlever. Un fichier ignoré est caché, pas absent.

**Correctif** — `SUPERVIEW_REQUIRE_FFMPEG := 1` exporté sur `test` et `coverage`,
`TEST_FLAGS := -race -count=1` sur les deux, pour que la cible *soit* la recette
d'`AGENTS.md` et non son approximation. `clean` perd `dist/` et gagne
`fyne_metadata_init.go`, `*.syso`, `superview`, `superview.exe`, `tmp-pkg/` et les deux
archives, avec le commentaire qui rattache la liste à `.gitignore`. Les deux README
décrivent ces cibles : mis à jour ensemble, `make clean` ajouté et l'exigence ffmpeg dite.

**Délibérément laissé** — deux tests de `common/lognoise_test.go` appellent le vrai
`EncodeVideo`, donc le vrai ffmpeg, sans passer par `skipWithoutFFmpeg` : sans ffmpeg elles
échouent au lieu de se sauter. Même famille, mais c'est une modification de tests, hors du
périmètre arbitré ici. Consigné en M-01.

**Leçon** — L-96, dont le balayage a été exécuté : treize invocations de `go test` dans le dépôt, une seule sans garde-fou, corrigée.

---

### [2026-09-09] U-13 + U-14 — L'encodeur promis, chiffré et exécuté doivent être le même

**PR** — #64

| | |
| --- | --- |
| **Constats** | U-13 et U-14 ([ANALYSE.md § 3nonies](ANALYSE.md)) |
| **Fichiers** | `common/probe.go` (taille en paramètre), `common/hardware.go` (`outputFrameSize`, cache par taille, `verifyEncoderForOutput`, `DescribeVerifiedHardwarePlan`), `common/common.go` (appel dans `PerformEncoding`, `sessionOutputSize`, chiffrage du repli), `gui_main.go` (`verifyHardwareInBackground`, `applyVerifiedHardwareLine`, état `squeeze`), `common/sizeprobe_test.go` et `common/fallbackmemory_test.go` (nouveaux), `common/integration_test.go`, `gui_main_test.go`, `common/probe_test.go`, `README.md`, `README_FR.md` |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · `golangci-lint run ./...` 0 alerte ✅ · couverture module 71,1 % ✅ · GUI démarrée et vivante ✅ · 10 contre-épreuves, chacune vérifiée compilable ✅ |

**Symptôme** — Sur le portable du signalement, la fenêtre annonçait `h264_vaapi` et la
conversion tournait entièrement sur le processeur. Le pilote refuse les 5120 px de large,
ce qu'aucune vérification ne demandait avant de promettre.

**Cause racine** — Deux fois la même : quelque chose décidait à partir d'un encodeur qui
n'était pas celui qui allait tourner. La sonde répondait pour une image de 256×256 ; le
garde-fou mémoire chiffrait l'encodeur choisi au départ et pas celui qui prenait le relais.

**Correctif** — Sonde paramétrée en taille, posée à la géométrie réelle dans
`PerformEncoding` avant tout le reste, avec recul vers l'encodeur logiciel de la famille
source. La ligne « Hardware » de la fenêtre est recalculée hors du fil d'interface, avec
cache par (encodeur, taille) et compteur de génération. Le repli interne d'`EncodeVideo`
chiffre l'encodeur qui prend le relais, en lisant la géométrie dans l'en-tête des cartes
de la session.

**Découvert en chemin** — Deux défauts introduits par le correctif lui-même et attrapés
avant la fusion : le découpage de l'estimation mémoire avait fait passer un contrôle de
nullité **après** `remapOutputSize`, qui déréférence `Streams[0]` sans garde (test existant
rougi, L-05) ; et la ligne « Hardware » rendue asynchrone a produit une course sous `-race`
(L-95).

**Leçon** — L-93, L-94, L-95.

### [2026-09-08] U-12 — Bruit et seuils faux dans le journal

**PR** — #63

| | |
| --- | --- |
| **Constat** | U-12 ([ANALYSE.md § 3nonies](ANALYSE.md)) |
| **Fichiers** | `common/common.go` (lecture de progression, commentaire orphelin), `common/health.go` (`minimumTempFreeGB`), `common/observability.go` (`SetHandler`, `SetObservabilityHandler`, retrait de `RegisterObservabilityHandler`), `gui_main.go` (installation du gestionnaire, texte d'attente), `common/lognoise_test.go` (nouveau), `common/observability_test.go` |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · `golangci-lint run ./...` 0 alerte ✅ · couverture module 70,6 % ✅ · GUI démarrée et vivante ✅ · balayage du symbole retiré dans la prose (L-29) ✅ · 4 contre-épreuves, chacune vérifiée compilable ✅ |

**Symptôme** — Quatre défauts lisibles dans un seul journal utilisateur : 45 `WARN` sur
une valeur normale, un contrôle disque en échec permanent, chaque événement écrit deux
fois, et un commentaire godoc attaché à la mauvaise fonction.

**Cause racine** — Un état normal traité comme une anomalie ; un seuil rond que rien ne
justifiait ; une API d'enregistrement qui ajoute là où l'appelant voulait remplacer ; un
bloc de commentaire déplacé lors d'une réorganisation.

**Correctif** — `N/A` reconnu et journalisé en `Debug`, le reste toujours en `WARN` ;
seuil disque ramené à 1 Go avec un message qui dit à quoi sert le répertoire ;
`SetObservabilityHandler` remplace au lieu d'ajouter et `RegisterObservabilityHandler`
est retirée ; commentaire remis en place. La fenêtre annonce l'attente de la première
image au lieu d'afficher une barre à zéro.

**Laissé délibérément** — Un compteur de temps écoulé pendant cette attente : le texte
suffit à dire ce qui se passe, un compteur vivant demanderait un ticker et son
annulation pour une phase de quelques secondes.

**Leçon** — L-90, L-91, L-92.

### [2026-09-08] U-11 — Rien ne vérifiait la mémoire disponible avant un encodage

**PR** — #62

| | |
| --- | --- |
| **Constat** | U-11 ([ANALYSE.md § 3nonies](ANALYSE.md)) |
| **Fichiers** | `common/common.go` (`encodesInTenBits`, constantes mesurées, `memoryNeededForEncode`, `checkMemoryForEncode`, appel dans `PerformEncoding`), `common/memoryguard_test.go` (nouveau), `common/integration_test.go`, `README.md`, `README_FR.md` |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · `golangci-lint run ./...` 0 alerte ✅ · couverture module 70,3 % ✅ · GUI démarrée et vivante ✅ · 5 contre-épreuves, chacune vérifiée compilable ✅ |

**Symptôme** — Une conversion qui ne peut pas tenir en mémoire démarrait quand même,
jusqu'à ce que le noyau tue ffmpeg et que systemd emporte la fenêtre avec.

**Cause racine** — Le contrôle disque existait pour 57 Mo de cartes, mais rien ne
regardait les gigaoctets de l'encodage lui-même. `checkMemoryHealth` teste un seuil fixe
de 1 Gio sans rapport avec la résolution, et n'est câblé que sur le bouton *Diagnostic*.

**Correctif** — `checkMemoryForEncode`, posé juste après le contrôle disque et après le
choix de l'encodeur, dont le coût dépend. Estimation mesurée au banc : 290 octets par
pixel de sortie en 8 bits, 455 en 10 bits. Refus en dessous du besoin, avertissement
jusqu'à un quart au-dessus, abstention pour les encodeurs matériels (non mesurés) et
quand la lecture mémoire est impossible. Un test d'intégration prouve le câblage et la
position : rien n'est produit, et ffmpeg n'a pas démarré.

**Leçon** — L-88, L-89.

### [2026-09-08] U-10 — Le repli CPU ignorait un encodeur matériel disponible dans l'autre famille

**PR** — #61

| | |
| --- | --- |
| **Constat** | U-10 ([ANALYSE.md § 3nonies](ANALYSE.md)) |
| **Fichiers** | `common/hardware.go` (familles de codecs, `crossFamilyHardwareEncoder`, `describeCodecFamilySwitch`, `softwareEncoderForFallback`, `HardwarePlan.CodecSwitch`), `common/common.go` (`FindEncoder`, cascade de repli, lignes de résumé), `common/crossfamily_test.go` (nouveau), `README.md`, `README_FR.md` |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · `golangci-lint run ./...` 0 alerte ✅ · couverture module 69,8 % ✅ · GUI démarrée et vivante ✅ · 6 contre-épreuves, chacune vérifiée compilable ✅ |

**Symptôme** — Une source HEVC sur une machine dont seul `h264_vaapi` fonctionne partait
sur `libx265`, à quatre threads, pour 6 Go de mémoire — et le journal n'en disait rien.

**Cause racine** — `FindEncoder` ne parcourait que `candidateEncodersForCodec` du codec
**de la source**. Aucune ligne de code ne regardait l'autre famille, et le choix
implicite — conserver le codec plutôt qu'utiliser le matériel — n'était écrit nulle part.

**Correctif** — Bascule vers un encodeur matériel de l'autre famille quand la famille de
la source n'en a aucun d'utilisable, annoncée dans la ligne « Hardware: » et dans le
journal, coût compris (une source 10 bits devient 8 bits en H.264). Le repli logiciel
revient désormais à la famille de la source, ce qui rend la bascule sans regret. Un
encodeur choisi explicitement dans la liste n'est jamais remplacé.

**Leçon** — L-87.

### [2026-09-08] U-09 — Un arrêt décidé par le système rapporté comme une annulation de l'utilisateur

**PR** — #59

| | |
| --- | --- |
| **Constat** | U-09 ([ANALYSE.md § 3nonies](ANALYSE.md)) |
| **Fichiers** | `common/common.go` (sentinelle, goroutine de signal, `classifyFfmpegFailure`), `common/health.go` (`availableMemoryBytes`, `parseMemAvailable`, `formatAvailableMemory`), `common/exitsignal_unix.go` et `common/exitsignal_windows.go` (nouveaux), `gui_main.go` (sortie sur signal), `common/stopreason_test.go` et `common/stopreason_unix_test.go` (nouveaux), `common/common_test.go` |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./...` ✅ · `golangci-lint run ./...` 0 alerte ✅ · couverture module 69,0 % (seuil 50 %) ✅ · GUI démarrée et vivante (code 124) ✅ · 4 contre-épreuves, dont une réécrite parce qu'elle ne compilait pas (L-86) ✅ |

**Symptôme** — Sur un portable de 8 Gio, le noyau tue ffmpeg par manque de mémoire ; le
journal annonce « encoding interrupted by user » et la fenêtre disparaît sans un mot.
L'utilisateur n'avait touché à rien.

**Cause racine** — Trois confusions empilées. `ErrCancelled` couvrait à la fois
l'annulation par l'utilisateur et un signal du système, les deux passant par un canal
d'entiers vides qui ne portait aucune raison. Un `SIGKILL` reçu par ffmpeg ressortait en
« ffmpeg failed: signal: killed », sans que rien ne regarde la mémoire. Et le
gestionnaire de signaux intercepte `SIGTERM` sans quitter, alors que systemd, qui
applique son `OOMPolicy` à l'unité entière, vient précisément de demander l'arrêt : le
`SIGKILL` qui suit ne laisse plus rien nettoyer.

**Correctif** — Sentinelle `ErrStoppedBySignal` distincte de `ErrCancelled`, portant le
nom du signal dans son message. Le canal d'arrêt transporte désormais la raison au lieu
d'un `struct{}`. `classifyFfmpegFailure` diagnostique un processus tué de l'extérieur
comme un manque de mémoire probable et joint le relevé `MemAvailable`, que
`checkMemoryHealth` partage maintenant au lieu de le reparser pour lui seul. La GUI
quitte sur `ErrStoppedBySignal`, après le déroulé des nettoyages de `PerformEncoding`.
**Laissé délibérément** : un signal reçu hors encodage tue toujours le processus sans
trace — rien n'est alors en vol, et un gestionnaire permanent ferait courir la fermeture
contre les nettoyages.

**Leçon** — L-84, L-85, L-86.

### [2026-09-07] U-08 — La documentation n'existait qu'en anglais

**PR** — #58

| | |
| --- | --- |
| **Constat** | U-08 ([ANALYSE.md § 3octies](ANALYSE.md)) |
| **Fichiers** | `README_FR.md` (nouveau), `README.md` (ligne de langue), `.github/workflows/lint.yml` (job `readme-parity`), `.all-contributorsrc`, `AGENTS.md`, `docs/CONTRATS.md` § 1 |
| **Vérification** | parité de forme des deux fichiers ✅ · liens et cibles relatives des deux ✅ · ancres accentuées conservées ✅ · UTF-8 sans BOM, fins de ligne LF ✅ · logique du job éprouvée sur cinq cas, dont `docs/README.md` qui ne doit pas compter ✅ · `lint.yml` reparsé ✅ · suite complète rejouée ✅ |

**Symptôme** — le dépôt s'adresse à un public qui n'est pas nécessairement anglophone, et
la personne qui n'a pas su installer l'application est francophone.

**Cause racine** — la convention de langue du dépôt (journaux d'audit en français, tout
ce qui est lu de l'extérieur en anglais) n'était **écrite nulle part** : cas de figure de
L-47. Rien n'avait donc jamais tranché la question de la langue du README.

**Correctif** — `README_FR.md` à la racine, miroir complet, ligne de sélection de langue
en tête des deux. Les libellés de l'interface restent en anglais dans le fichier
français, l'interface n'étant pas traduite ; les chaînes de Windows, elles, sont celles
d'un Windows français. La convention et la règle de co-modification sont écrites dans
`AGENTS.md`, et `docs/CONTRATS.md` § 1 nomme désormais les deux fichiers.

**Ce qui garde le miroir** — le job `readme-parity` échoue quand une PR modifie un README
sans l'autre, et quand leurs structures de titres divergent. `.all-contributorsrc` liste
les deux fichiers, sinon la PR du bot en toucherait un seul et échouerait à chaque
nouveau contributeur ; son `projectOwner` était par ailleurs resté sur `Niek` alors que
la table générée pointe sur `Canaill51`, ce qui aurait produit des liens faux sur deux
fichiers au premier passage du bot.

**Ce qui a été délibérément laissé** — l'interface graphique reste en anglais, et il n'y
a pas d'i18n dans le code. Traduire l'interface est un autre chantier ; le faire à moitié
aurait désaccordé le README français des boutons qu'il décrit.

**Leçon** — L-83.

---

### [2026-09-07] U-07 — Le README n'était pas suivable par son destinataire

**PR** — #57

| | |
| --- | --- |
| **Constat** | U-07 ([ANALYSE.md § 3octies](ANALYSE.md)) |
| **Fichiers** | `README.md` (restructuré), `gui_main.go` (`installURL`), `common/common.go` (texte d'erreur), `readme_link_test.go` (nouveau), `docs/ENVIRONNEMENT.md` (renvoi de section) |
| **Vérification** | `gofmt -l .` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · contre-épreuve du test d'ancre dans les deux sens ✅ · archive Windows v0.2.6 téléchargée, `unzip -l` ✅ · archive Linux v0.2.6 : `sha256sum -c` ✅, `tar -xJf` et structure ✅ · liens et ancres du README ✅ · **séquence Windows non vérifiée, voir ci-dessous** |

**Symptôme** — un utilisateur lambda, à qui il était demandé d'installer la dernière
version sous Windows, n'y est pas arrivé.

**Cause racine** — le README décrivait le trajet de l'auteur, pas celui du lecteur. Il
ouvrait sur treize lignes de FFmpeg/NVENC qui ne concernent que les compilations depuis
les sources ; il donnait ses instructions en PowerShell à quelqu'un qui double-clique ;
et il ne mentionnait nulle part que, les binaires n'étant pas signés, Windows interpose
un écran qui ne propose que *Ne pas exécuter*. Voir L-81.

**Correctif** — `## Download and install` devient la première section H2, en six étapes
numérotées sans terminal, l'écran SmartScreen décrit avec les libellés exacts de ses
boutons et un `#### Why Windows warns about Superview` qui dit pourquoi et ce qu'on peut
vérifier à la place. `## Requirements` disparaît, réparti entre l'installation, un
nouveau `### If something goes wrong` et `## Development`. Plus aucun numéro de version
n'est écrit : « the file ending in `-windows-x86_64.zip` » ne périme pas.

**Ce qui a été déplacé, pas supprimé** — les avertissements mesurés (winget `8.1.1`,
plancher pilote 610.00) descendent en `## Development`, seul endroit où quelqu'un tape
`winget` ; l'argument long contre `ffmpeg -encoders` renvoie à
`docs/hardware-support.md`, qui le porte déjà. Aucune phrase retirée du README sans
avoir été retrouvée ailleurs.

**Défaut trouvé en chemin** — `gui_main.go` et `common/common.go` codaient en dur
`?tab=readme-ov-file#requirements`, l'URL montrée à l'utilisateur privé de FFmpeg.
Supprimer la section l'aurait déposé en haut du README, sans erreur. Constante renommée
`installURL`, et `readme_link_test.go` interdit à ce fragment de ne désigner aucun titre.
Voir L-82.

**Ce qui n'a pas été vérifié, et pourquoi** — toute la séquence Windows : les libellés
SmartScreen, le chemin *Extraire tout…*, l'emboîtement des dossiers, le comportement des
antivirus. Le poste de vérification est sous Linux, et **un exécuteur `windows-latest` ne
peut pas servir d'oracle** : il télécharge par API, donc sans Mark-of-the-Web, SmartScreen
y est désactivé et la boîte est interactive. Ce qui a pu être prouvé l'a été sur les
archives réelles de la v0.2.6. L'oracle restant est l'utilisateur qui a échoué : lui
faire suivre la nouvelle section mot à mot.

**Leçon** — L-81, L-82.

---

### [2026-09-06] U-06 — Le README prescrivait encore le build qui casse l'encodage matériel

| | |
| --- | --- |
| **Constat** | U-06 ([ANALYSE.md § 3septies](ANALYSE.md)) |
| **Fichiers** | `README.md` (Requirements, Overview, Installation, Usage, Configuration, Architecture, Development), `docs/ANALYSE.md` (§ 1), `docs/CONTRATS.md` (§ 1), `docs/ENVIRONNEMENT.md`, `superview.yaml` |
| **PR** | #56 |
| **Vérification** | documentation seule ; aucun fichier Go modifié · `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `superview.yaml` reparsé et tests de config rejoués (fichier livré) ✅ · liens internes du README vérifiés ✅ |

**Symptôme** — relevé par l'utilisateur après la fusion de #54. Le § *Requirements* prescrivait
`winget install -e --id Gyan.FFmpeg`, c'est-à-dire le build 8.1.2 à plancher 610 qui est à
l'origine du signalement, et donnait `ffmpeg -encoders | grep nvenc` comme moyen de vérifier son
GPU — la vérification dont U-03 a établi qu'elle ne prouve rien.

**Cause racine** — les trois PR du chantier avaient toutes modifié `README.md`, chacune une
section. Le balayage prescrit par AGENTS.md porte sur les **symboles** ; aucun n'avait changé. Ce
qui était devenu faux, ce sont des affirmations, qu'aucun `grep` sur un identifiant ne relie au
correctif.

**Correctif** — § *Requirements* réécrite autour de « une archive n'exige rien à installer »,
avertissement `winget` avec la version qui fonctionne, et renvoi à *Diagnostic* pour vérifier le
GPU. Contenu réel des archives, disposition d'installation, `SUPERVIEW_FFMPEG_DIR` documenté là où
on le cherche, cartographie du projet à jour. Hors README : compteurs et couverture de
`ANALYSE.md` § 1 rafraîchis à `ec7d753`, table des sources de `CONTRATS.md` complétée,
`ENVIRONNEMENT.md` précise qu'un build source n'embarque pas de FFmpeg — donc que les sondes
mesurent celui du système et non celui que les utilisateurs recevront — et `superview.yaml` dit
pourquoi le choix du binaire n'est pas une clé de configuration.

**Trouvé en chemin, antérieur au chantier** — les étapes de la GUI nommaient des boutons
inexistants (« 1) Choose input file », « 3) Start Superview transform ») et
`.\superview-gui.exe` ne correspondait au nom d'aucun exécutable livré. Corrigés ici : ils sont
dans les sections relues.

**Leçon** — L-80.

---


### [2026-09-06] Hygiène du journal — le numéro de PR remplace le sha de fusion

| | |
| --- | --- |
| **Constat** | aucun ; hygiène de ce document, relevée par l'utilisateur |
| **Fichiers** | `docs/LECONS.md` (gabarit § 1 et les 18 entrées du § 3), `docs/ANALYSE.md` (journal des révisions) |
| **PR** | #55 |
| **Vérification** | documentation seule ; aucun fichier Go modifié, `go build ./...` rejoué pour le confirmer ✅ · les 18 numéros établis par `git log -S` sur le code, pas par déduction ✅ |

**Symptôme** — le gabarit demandait le sha de fusion. Dix entrées sur treize affichaient encore
« non commité », dont quatre écrites au cours de cette série en sachant que c'était déjà faux.

**Cause racine** — l'entrée s'écrit avec le correctif, donc **avant** la fusion : la valeur
demandée n'existe pas encore. Le champ ne pouvait qu'être faux à l'écriture, et sa correction
supposait de revenir sur le fichier une fois la PR fusionnée — une étape que rien ne déclenche.

**Correctif** — le champ prend le **numéro de PR**, connu dès l'ouverture. Le sha reste
accessible par `git log --grep '(#NN)'`, donc rien n'est perdu.

Les dix-huit entrées ont été renseignées à partir de l'historique. `git log -S` sur
`docs/LECONS.md` ne sert à rien ici : il attribue **toutes** les anciennes entrées à `3c20954`,
la réécriture intégrale du fichier. C'est `git log -S` sur le **code** que chaque entrée décrit
qui répond, en nommant le commit d'écrasement — lequel porte le numéro dans son titre. Lire le
corps des PR n'aurait pas suffi : plusieurs mentionnent un même constat, l'une le corrigeant et
les suivantes y renvoyant, si bien que `R-05` correspondait à #40 et #43, et `N-07` à quatre PR.

**Ce qui a été laissé** — les cinq entrées groupées n'ont pas de tableau propre ; elles portent
une ligne `**PR** — #NN` sous leur titre, et le gabarit documente cette forme.

**Leçon** — L-79.

---


### [2026-09-06] U-05 — Deux chemins matériels sans plancher pilote, et VAAPI réparé au passage

| | |
| --- | --- |
| **Constat** | U-05 ([ANALYSE.md § 3septies](ANALYSE.md)) |
| **Fichiers** | `common/hardware.go`, `common/common.go`, `common/probe.go` ; tests : `common/hardware_test.go`, `common/common_test.go`, `common/probe_test.go`, `common/integration_test.go` ; docs : `README.md`, `docs/CONTRATS.md`, `docs/hardware-support.md` |
| **PR** | #54 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · conversion réelle 640×480 → 853×480 par `h264_vulkan` ✅ · 5 contre-épreuves ✅ |

**Symptôme** — FFmpeg 8.1 expose `h264_vulkan`, `hevc_vulkan` et, sous Windows,
`h264_d3d12va` / `hevc_d3d12va`. Ces encodeurs passent par le pilote d'affichage, pas par l'API
NVENC : la négociation de version qui a coûté l'accélération matérielle à une RTX A1000 (U-03) ne
les concerne pas. Ils étaient déjà listés par `ffmpeg -encoders` chez nous, et
`candidateEncodersForCodec` ne les proposait jamais.

**Cause racine, découverte en les ajoutant** — ils n'acceptent que des trames déjà sur le
périphérique. `remap` étant un filtre CPU, les trames sont en mémoire système à la sortie de la
chaîne. Le pipeline n'émettait ni `-init_hw_device` ni `hwupload` : **`h264_vaapi`, candidat de
longue date, n'avait donc jamais pu fonctionner** — sur une machine où VAAPI marche, l'encodage
échouait et repliait silencieusement sur le CPU.

**Correctif** — `hwDeviceArgs` et `hwUploadFilters`, employées **à la fois** par la conversion et
par la sonde. Le périphérique est créé avant les `-i` (après, l'option ne configure rien) et
l'upload s'ajoute à la fin de la chaîne, donc après le `remap` : la géométrie est inchangée. En
10 bits l'upload passe par `p010`, les pools matériels étant semi-planaires.

**Deux contre-épreuves fausses, corrigées** — casser le code partagé neutralisait la
contre-épreuve, le test se sautant au lieu d'échouer (L-78) ; et l'assertion du test cherchait le
nom de l'encodeur dans un résumé que le message de repli contient aussi (L-77).

**Ce qui n'a pas pu être essayé** — `*_d3d12va`, spécifique à Windows. La sonde rend l'ajout sans
risque : un chemin qui ne répond pas n'est jamais retenu.

**Leçon** — L-77, L-78.

---


### [2026-09-06] U-04 — Les archives embarquent leur FFmpeg, épinglé sur son plancher pilote

| | |
| --- | --- |
| **Constat** | U-04 ([ANALYSE.md § 3septies](ANALYSE.md)) |
| **Fichiers** | `.github/workflows/release.yml`, `.github/scripts/nvenc-driver-floor.sh` (nouveau), `common/common.go`, `THIRD_PARTY_NOTICES.md` (nouveau) ; tests : `common/toolresolve_test.go` (nouveau) ; docs : `README.md`, `AGENTS.md`, `RELEASING.md`, `docs/hardware-support.md`, `docs/CONTRATS.md` |
| **PR** | #53 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · scripts du YAML extraits et exécutés en vrai ✅ · paquet Linux construit, empaqueté, installé et **lancé installé** ✅ |

**Symptôme** — décidé avec l'utilisateur après U-03 : le comportement de l'application dépendait
du FFmpeg présent sur la machine, c'est-à-dire de la variable la plus difficile à diagnostiquer à
distance et la seule capable de retirer silencieusement l'accélération matérielle.

**Correctif** — les deux archives de release embarquent `ffmpeg` et `ffprobe`, et
`resolveToolBinary` les préfère au `PATH`. Ce qui est épinglé est le **plancher pilote NVENC**,
570.0, relu dans le binaire téléchargé par un script de CI qui fait échouer la release s'il a
changé. `SUPERVIEW_FFMPEG_DIR` reste la sortie de secours.

Sous Linux les outils vont dans `../lib/superview/` et non à côté de l'application : celle-ci
s'installe dans `$PREFIX/bin`, et un `ffmpeg` déposé là masquerait celui du système pour toute la
machine. Le `Makefile` généré par fyne est patché en conséquence, puis le paquet est installé dans
un répertoire jetable pour vérifier que tout atterrit au bon endroit.

**Ce qui n'a pas été retenu** — les variantes `lgpl` (sans libx264/libx265, donc sans repli CPU),
gyan `full_build-shared` (92 Mo zippés contre 71), et le build statique BtbN du fork (106 Mo).
Détail des mesures : ANALYSE.md U-04.

**Défaut trouvé en chemin** — `make install` d'un paquet fyne 1.7.2 échoue sur sa ligne d'icône
(`$(Icon)` sans `.png`), après avoir installé le binaire. Aucune archive Linux publiée par ce
projet n'était installable jusqu'au bout. Corrigé dans le même patch, à partir du nom du fichier
réellement livré plutôt qu'en ajoutant `.png`, pour que la correction devienne inopérante — et non
fausse — si fyne répare la sienne.

**Coût assumé** — +71 Mo sur l'archive Windows, +80 Mo sur l'archive Linux. Arbitrage de
l'utilisateur, explicite : « Même si le livrable voit sa taille augmentée, on s'assure ainsi que
l'application ne passera plus en fallback. »

**Leçon** — L-74, L-75, L-76.

---


### [2026-09-06] U-03 — L'accélération matérielle était promise sans avoir jamais été vérifiée

| | |
| --- | --- |
| **Constat** | U-03 ([ANALYSE.md § 3septies](ANALYSE.md)) |
| **Fichiers** | `common/probe.go` (nouveau), `common/common.go` (`newFFmpegCommandContext`), `common/health.go`, `gui_main.go` ; tests : `common/probe_test.go` (nouveau), `common/health_report_test.go`, `gui_main_test.go` |
| **PR** | #52 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · GUI lancée, capturée, journal relu ✅ · 8 contre-épreuves ✅ |

**Symptôme** — signalé par l'utilisateur : sur une NVIDIA RTX A1000 sous Windows, avec le FFmpeg
8.1.2 installé par winget (gyan.dev), l'accélération matérielle ne fonctionne plus. Superview
annonçait pourtant `Hardware: planned h264_nvenc encode + CUDA decode` et encodait sur le CPU.

**Cause racine** — deux causes superposées, et une seule nous appartient.

*Chez FFmpeg* : le plancher pilote NVENC est fixé à la compilation par les `nv-codec-headers`.
gyan.dev est passé du SDK 13.0 au SDK 13.1 entre 8.1.1 et 8.1.2, portant l'exigence de 570 à
610, alors que la branche pilote RTX Enterprise plafonne à 597.06. Mesures et sources :
ANALYSE.md § 3septies.

*Chez nous* : `CheckFfmpeg` déduisait les capacités de `ffmpeg -encoders` et `ffmpeg -hwaccels`,
qui décrivent le binaire et pas la machine. Rien dans le programme n'était en mesure de
distinguer « compilé avec » de « fonctionne ici ».

**Correctif** — une sonde à l'exécution. Chaque encodeur annoncé encode une trame ; le code de
retour décide. Les refusés disparaissent du profil ffmpeg, donc du menu déroulant, du plan
annoncé et de la sélection. La ligne « Hardware » dit `checking which encoders this machine
accepts...` tant que la sonde n'est pas revenue, plutôt que de nommer un encodeur invérifié. Le
rapport Diagnostic gagne une section *Encoders* qui porte, pour chaque refus, les mots de ffmpeg
— y compris `The minimum required Nvidia driver for nvenc is 610.00 or newer`, la ligne qui
aurait épargné l'enquête entière.

**Ce qui a été laissé** — l'empaquetage d'un FFmpeg épinglé (plancher 570, Windows et Linux,
binaire embarqué prioritaire), décidé avec l'utilisateur et traité dans sa propre PR ; et les
encodeurs `*_vulkan` / `*_d3d12va`, qui contournent la négociation NVENC et deviennent sûrs à
ajouter maintenant que la sonde filtre ce qui ne marche pas.

**Deux défauts trouvés par leur propre contre-épreuve** — le résumé d'erreur gardait les
**dernières** lignes de ffmpeg, c'est-à-dire les conséquences, et jetait la cause : corrigé en
gardant les premières (L-73 en est le voisin). Et le test de délai passait encore après
suppression du délai, parce que celui de l'appelant le couvrait : il a été scindé en deux gardes
dont chacune rougit pour sa propre raison (L-37, L-54).

**Leçon** — L-71, L-72, L-73.

---


### [2026-09-06] U-01, U-02 — Libellés rognés et rangée de boutons collée à gauche

| | |
| --- | --- |
| **Constat** | U-01, U-02 ([ANALYSE.md § 3sexies](ANALYSE.md)) |
| **Fichiers** | `gui_main.go:34-76` (géométrie et helpers), `gui_main.go:1040` (rangée), `gui_main.go:1085-1088` (taille de fenêtre) ; `gui_main_test.go:142-241` |
| **PR** | #51 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1` ✅ · `golangci-lint run ./...` 0 alerte ✅ · GUI lancée et capturée ✅ · contre-épreuves ✅ |

**Symptôme** — signalé par l'utilisateur, capture à l'appui : les six boutons d'action ne sont
pas centrés dans la fenêtre, et leur texte déborde de leur cadre.

**Cause racine** — deux défauts indépendants dans la même rangée. (1) Chaque bouton était posé
dans un `GridWrap` de 150 × 34 px, taille imposée et identique pour tous, alors que les boutons
portent une icône : `Start transformation` réclame 186 × 36, `Choose output file` 168 × 36,
`Choose input file` 158 × 36. Un `GridWrap` ne s'agrandit pas pour son contenu. (2) La rangée
était un `HBox` nu, qui empile depuis la gauche.

**Pourquoi le garde-fou n'a rien vu** — `TestToolbarFitsWindow` mesurait la somme des cellules,
que le `GridWrap` rapporte égale à ce qu'on lui a donné, sur des boutons construits sans leurs
icônes. 920 ≤ 980 : vert. C'est la deuxième fois que ce test passe à vide (V-04 était le premier
resserrage).

**Correctif** — `actionButtonCell` dérive la cellule du minimum du bouton, élargi à un plancher
de 140 px pour que `Quit` ne se réduise pas à une pastille ; `newActionToolbar` centre la rangée.
`window.Resize` prend le maximum entre la géométrie voulue et `content.MinSize()`, de sorte qu'un
libellé plus long élargisse la fenêtre au lieu d'en sortir sans le dire. Le test compare
maintenant chaque cellule au bouton qu'elle porte, et `TestToolbarIsCentred` mesure la position
absolue de la rangée dans un canevas.

**Ce qui n'a pas changé** — la fenêtre reste 980 × 470 : la rangée corrigée mesure 952 px, donc
le `Max` ne se déclenche pas aujourd'hui. Aucun libellé n'a été raccourci, ce qui aurait été
l'autre façon de faire tenir la rangée.

**Contre-épreuves** — cellule remise à 150 × 34 : `TestToolbarFitsWindow` nomme les six boutons
et leur déficit. `Center` retiré : `TestToolbarIsCentred` rougit sur « la rangée commence à x=4
dans une fenêtre de 980 px ». La première version de cette seconde contre-épreuve rougissait
sur la forme de l'arbre de conteneurs et a été refaite (L-70).

**Leçon** — L-69, L-70 ; L-20 mise à jour.

---


### [2026-09-05] R-05 — Le premier `workflow_dispatch` de Release échouait sur la version

| | |
| --- | --- |
| **Constat** | R-05 (palier 12) |
| **Fichiers** | `.github/workflows/release.yml` |
| **PR** | #40 |
| **Vérification** | YAML relu par `yaml.safe_load` ✅ · dérivation extraite du YAML et essayée sur cinq refs (`master`, `v0.2.3`, `RC-0.3.0`, `v0.2.3-rc1`, `feature/foo`) ✅ |

**Symptôme** — run 33929547442, lancé à la main sur `master` : `Build GUI (Windows)` et
`Build GUI (Linux)` rouges en moins d'une minute, sur
`invalid --app-version parameter, integer and '.' characters only up to x.y.z`. `Notify` a
correctement signalé l'échec, ce qui est le comportement que R-01/#32 avait installé.

**Cause racine** — R-04 passe `VERSION="${GITHUB_REF_NAME#v}"` à `fyne package`. Sur un
`workflow_dispatch`, le ref est la branche : la version demandée valait `master`. Le retrait du
`v` initial ne dérive une version que pour la forme `v*` ; les deux autres refs que le bloc `on:`
accepte — une branche et un tag `RC-*` — donnent une chaîne que `fyne` refuse.

**Correctif** — extraire le premier `x.y.z` que le ref contient, et retomber sur `0.0.0` quand il
n'en contient aucun. Un dispatch est un essai à blanc : il ne publie rien (`create-release` reste
conditionné au ref-tag) et ne doit donc revendiquer aucun numéro. Un tag `RC-0.3.0` produit
maintenant `0.3.0`, ce qui répare au passage un échec qui attendait la première release candidate.

**Ce qui n'a pas changé** — le nom des archives continue d'utiliser `github.ref_name` tel quel,
donc un dispatch produit `superview-gui-master-linux-x86_64.tar.xz`. C'est juste : l'archive dit
d'où elle vient, et rien de ce qui sort d'un dispatch n'est publié.

**Leçon** — L-55.

---


### [2026-09-04] P-12, P-13 — Couture au centre en mode squeeze, et libellé trompeur

| | |
| --- | --- |
| **Constats** | P-12, P-13 ([§ 3ter](ANALYSE.md)) |
| **Fichiers** | `common/common.go` · `common/pgm_golden_test.go` · `gui_main.go` · `gui_main_test.go` · `README.md` |
| **PR** | #33 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · GUI démarrée ✅ · contre-épreuve ✅ |

**Contexte** — l'utilisateur n'a pas de fichier GoPro en mode squeeze sous la main. Plutôt que
d'attendre, j'ai cherché ce qui restait vérifiable sans : la santé géométrique de la carte de
remappage, que le test doré ne couvre pas.

**P-12, symptôme** — une couture verticale de 1 à 2,6 px au milieu de l'image, en mode squeeze
uniquement, d'amplitude erratique selon la résolution.

**P-12, cause racine** — les deux termes du décalage squeeze valent tous deux `7/32 × outX × inv`
au centre : la courbe est **conçue** pour y passer par zéro. Mais `outX/16` et `outX/7` étaient
des divisions entières, dont la troncature laissait un résidu que le miroir de la moitié gauche
transformait en saut. Preuve directe : sur une largeur multiple de 112 (16×7), aucune troncature
n'a lieu et la couture disparaît entièrement.

**P-12, correctif** — diviser en flottant, en conservant la forme de l'expression pour rester
lisible face à l'implémentation de référence (contrainte inscrite dans `.golangci.yml`).
Amplitude maximale inchangée à 0,1 % près : raffinement sous-pixel, pas changement d'intention.

**Une fausse piste, corrigée en chemin** — j'avais d'abord annoncé une compression de 2,09× au
centre. C'était une erreur de mesure : ma différence centrée enjambait l'axe de symétrie et
intégrait la discontinuité. La pente réelle y est de 1,43. L'erreur a néanmoins mis le défaut au
jour.

**P-13** — la case s'intitulait « Source already stretched (GoPro SuperView) », alors que le
README amont documente l'option pour des caméras comme la Caddx Tarsier et déclare l'algorithme
« not a 1-1 copy of the GoPro algorithm ». Renommée « Source already stretched to 16:9
(un-squeeze) », README aligné sur le périmètre réel.

**Ce que je n'ai pas pu trancher** — l'implémentation Python de Banelle est inaccessible (403,
archive web refusée), donc j'ignore si la couture y existait déjà ou si elle est née au portage
Python → Go. Sans importance pour la décision : l'intention est démontrable depuis la formule.

**Leçon** — L-48.

---


### [2026-09-04] Cohérence du document — quatre titres contredisaient le tableau d'avancement

| | |
| --- | --- |
| **Constat** | aucun ; dérive de documentation relevée en clôture |
| **Fichiers** | `docs/ANALYSE.md` · `docs/LECONS.md` — **aucun code** |
| **PR** | #30 |
| **Vérification** | audit systématique des 22 titres N-xx/P-xx contre le § 4bis, plus les cases de la file d'attente |

**Symptôme** — `### N-03 🟠 — Les sources 10 bits sont ramenées à 8 bits`, alors que le § 4bis
compte N-03 parmi les corrigés et que le corps du constat porte « ✅ correctif vérifié ». Idem
N-04, N-05 et P-11. Un lecteur qui parcourt les titres conclut que trois pertes de contenu sont
toujours actives.

**Cause racine** — le document porte **deux conventions de marquage**, aucune écrite. Le § 3
conserve les sévérités d'origine parce que c'est une analyse datée et non un tableau de bord ;
les § 3bis et 3ter barrent le titre et le passent en ✅ parce qu'on les ouvre pour connaître
l'état courant. Les deux se défendent, mais rien ne les énonçait, donc rien ne rappelait de
restyler en corrigeant.

**Correctif** — les quatre titres sont restylés, et **les deux conventions sont écrites en tête
du § 3**, avec la mention explicite que le § 4bis fait foi. C'est la seconde partie qui compte :
corriger les quatre titres sans écrire la règle laissait la dérive se reproduire au constat
suivant.

Deux formulations périmées corrigées au passage : l'intro du § 3ter décrivait encore N-07 comme
en attente avec l'ancien chiffre de 5 %, et un titre de palier annonçait des constats « encore
ouverts » alors que toutes ses cases sont cochées. Les entrées du **journal des révisions** sont
laissées telles quelles : elles disent ce qui était vrai à leur date, c'est leur rôle.

**Leçon** — L-47.

---


### [2026-09-04] N-07 — Mesure révisée, arbitrage inversé (aucun changement de code)

| | |
| --- | --- |
| **Constat** | N-07 ([§ 3bis](ANALYSE.md)) |
| **Fichiers** | `docs/ANALYSE.md` · `docs/CONTRATS.md` — **aucun fichier de code** |
| **PR** | #29 |
| **Vérification** | mesures refaites sur RTX 4070, code actuel, deux exécutions concordantes par cas |

**Symptôme** — le constat annonçait « le décodage matériel n'apporte que ~5 %, pour un risque
réel », et j'ai relayé cette conclusion à l'utilisateur comme un fait. Il a demandé une
explication, ce qui m'a conduit à refaire la mesure.

**Cause racine** — la mesure d'origine portait sur une mire `testsrc` à ~2 Mbps. Un tel flux se
décode quasi gratuitement : il n'y a presque rien à déporter sur le GPU, donc presque rien à
gagner. Une GoPro 5,3K produit du 100-120 Mbps de contenu détaillé.

**Mesures** — même matériel, code actuel, meilleur de deux exécutions :

| Source | Décodage matériel | Encodage matériel |
| --- | --- | --- |
| Mire `testsrc`, 2 Mbps *(échantillon d'origine)* | +3,1 % | ×1,92 |
| Type GoPro, 2880×2160 à 127 Mbps | **+9,9 %** | **×3,18** |

Le mécanisme a aussi été isolé, en transcodage sans `remap` sur la source de 127 Mbps :
trames gardées en VRAM 2,34 s (+7 %), trames rapatriées en RAM — ce que fait Superview —
2,47 s (+1 %), décodage CPU 2,51 s. Le rapatriement annule donc le gain *en transcodage simple*,
alors qu'avec `remap` le même décodage matériel rapporte +9,9 %. L'explication cohérente avec ces
deux mesures est que le gain vient du **CPU libéré pour le filtre**, pas d'un décodage plus
rapide — inférence, non vérifiée directement.

**Conclusion** — l'arbitrage s'inverse : à ~10 % pour un risque essentiellement limité à
l'initialisation, conserver le décodage matériel est le meilleur choix. **Aucune modification de
code.** Décision finale à l'utilisateur.

**Leçon** — L-46.

---


### [2026-09-04] P-10 — L'état de `main()` extrait dans `appState`

| | |
| --- | --- |
| **Constat** | P-10 ([§ 3ter](ANALYSE.md)) |
| **Fichiers** | `gui_main.go` · `gui_main_test.go` |
| **PR** | #28 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · GUI démarrée ✅ · contre-épreuve sur les trois propriétés clés ✅ |

**Symptôme** — aucun test ne pouvait atteindre l'état de la GUI. Les 14 tests du paquet `main`
ne touchaient que des fonctions pures, et c'est exactement pour cela que P-01 et P-02 avaient
survécu à T-01 : ils vivaient dans les ~600 lignes de `main()` où `video`, `outputPath`,
`encodingInProgress` et `cancelEncoding` étaient des variables capturées par closure.

**Cause racine** — l'état et les décisions qui le lisent étaient au même endroit que la
construction des widgets, dans une seule fonction. Rien n'était nommé, donc rien n'était
appelable.

**Correctif** — un type `appState` porte les six éléments d'état et les widgets qu'ils pilotent,
avec onze méthodes : `canStart`, `refreshStart`, `setInput`, `setOutput`, `setFFmpeg`,
`refreshHardwareStatus`, `setEncoding`, `beginEncoding`, `finishEncoding`, `requestCancel`,
`isEncoding`. Toutes à **100 % de couverture**.

Deux décisions vont plus loin qu'un simple déplacement :

- `beginEncoding()` **retourne** le canal d'annulation. La goroutine ne peut plus le relire dans
  la structure : P-02 devient inexprimable plutôt que corrigé (L-44).
- `requestCancel()` est le seul endroit qui ferme le canal et le met à `nil` dans le même geste ;
  l'appeler deux fois est inoffensif au lieu de paniquer. Testé explicitement.

Trouvé en chemin : le chemin « ffmpeg indisponible » désactivait quatre contrôles à la main et
**oubliait le sélecteur de codec**, créé plus loin dans `main()` — il restait actif parmi quatre
contrôles grisés. C'était la dernière séquence d'`Enable`/`Disable` écrite à la main du fichier ;
elle passe maintenant par la même liste que la transition d'encodage.

**Laissé de côté délibérément** — `main()` reste longue (524 lignes contre 600). Le défaut
n'était pas la longueur mais l'inaccessibilité des décisions ; la construction de widgets et la
mise en page n'ont rien à gagner à être découpées (L-45).

| | Avant | Après |
| --- | --- | --- |
| État applicatif | 6 variables en closure | 1 type, 11 méthodes |
| Tests atteignant l'état | 0 | 18 sous-tests |
| Couverture du paquet `main` | 12,6 % | **21,8 %** |
| Couverture du module | 56,6 % | **59,3 %** |

**Leçons** — L-44, L-45.

---


### [2026-09-04] Second lot — P-01 à P-07, P-09, N-06, N-08, N-09, N-10

| | |
| --- | --- |
| **Constats** | P-01 à P-07, P-09 ([§ 3ter](ANALYSE.md)) · N-06, N-08, N-09, N-10 ([§ 3bis](ANALYSE.md)) |
| **Fichiers** | `common/common.go` · `common/metrics.go` · `common/observability.go` · `common/config.go` · `gui_main.go` · `superview.yaml` · `.github/copilot-instructions.md` · 5 fichiers de test |
| **PR** | #28 |
| **Vérification** | `gofmt` ✅ · `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · GUI démarrée ✅ · contre-épreuve sur chaque correctif ✅ |

**P-01 — la case squeeze restait grisée.** Les deux branches de fin d'encodage réactivaient les
widgets une par une et avaient divergé. Remplacées par un type `encodingControls` porteur d'un
`setEnabled(bool)` : une seule liste, parcourue dans les deux sens. C'est aussi ce qui rend la
transition testable, ce qu'elle n'était pas — les 14 tests du paquet `main` ne touchaient que des
fonctions pures, et c'est exactement pour cela que P-01 avait survécu à T-01 (L-35).

**P-02 — course sur le canal d'annulation.** Le canal est capturé dans une locale avant le
lancement de la goroutine, au lieu d'être relu depuis la variable partagée que le fil UI met à
`nil`.

**P-03 — l'annulation relançait la cascade.** `ErrCancelled` remplace `errors.New("encoding
interrupted by user")`, et les trois points de repli d'`EncodeVideo` la testent avec
`errors.Is`. Mesuré par `TestEncodeVideo_CancellationStopsTheFallbackCascade` : 1 lancement de
ffmpeg au lieu de 3.

**P-04 — fichier de sortie partiel.** L'encodage écrit dans un fichier de travail créé par
`os.CreateTemp` dans le répertoire de destination, renommé en place seulement en cas de succès.
Le renommage est atomique (même système de fichiers) et un échec ne détruit plus le fichier
existant. **N-09 devenait obligatoire** dans le même mouvement : ffmpeg ne voyant plus de
conflit entrée/sortie, plus rien n'empêchait le renommage final d'écraser la source.

**P-05 — unités.** « bytes/second » → « bits/second » dans le code, le YAML, les tests, la
documentation d'agent et les messages d'erreur montrés à l'utilisateur. Aucun changement de
comportement : les valeurs étaient déjà des bits.

**P-06 — vitesse d'encodage inventée.** `r_frame_rate` est lu auprès de ffprobe et parsé
(`parseFrameRate`, qui gère les cadences NTSC rationnelles). Quand la cadence est inconnue, la
métrique reste à zéro plutôt que d'être calculée sur un 30 fps supposé (L-33).

**P-07 + N-10 — retour utilisateur.** `ProgressDetailHandler`, interface optionnelle détectée
par assertion de type, transporte le temps restant que `metrics.go` calculait déjà sans que
personne ne le lise. Le statut affiche « Transforming... 42% - about 1m20s left ». Le chemin du
fichier de journal apparaît dans le dialogue Diagnostic.

**P-09 — partiellement.** `RecordEvent` prend un `RLock` ; `-threads` n'est plus émis pour les
encodeurs matériels. Le court-circuit d'allocation sur les événements de progression a été
**délibérément écarté** : le coût est négligeable devant un encodage vidéo et l'optimisation
risquait de faire disparaître des événements du journal.

**N-06 — débit non borné.** `-maxrate` et `-bufsize` ajoutés, calibrés par la mesure et non par
la recommandation d'usage (L-42).

**N-08 — espace disque.** `checkTempSpaceForMaps` s'exécute avant `InitEncodingSession` avec le
besoin **réel** (`remapMapBytes`, + 20 % de marge) et non un seuil fixe de 10 Go. Le message
mentionne que le répertoire temporaire est souvent en RAM et suggère `TMPDIR`. Une sonde qui
échoue ne bloque pas l'encodage.

**Leçons** — L-40 à L-43.

---


### [2026-09-04] N-03 + N-04 + N-05 — Chaîne de filtres : profondeur, pistes audio, métadonnées

| | |
| --- | --- |
| **Constat** | N-03, N-04, N-05 ([ANALYSE.md § 3bis](ANALYSE.md)) |
| **Fichiers** | `common/common.go` (`VideoStream`, `CheckVideo`, `buildEncodeBaseArgs`, + 4 helpers) · `common/common_test.go` · `common/integration_test.go` |
| **PR** | #28 |
| **Vérification** | `go build ./...` ✅ · `go vet ./...` ✅ · `golangci-lint` ✅ 0 alerte · `go test -race ./...` ✅ · contre-épreuve ✅ |

**Symptôme** — trois pertes silencieuses sur un encodage réussi, toutes mesurées : une source
`yuv420p10le` ressortait en `yuv420p` ; deux pistes audio en entrée donnaient une seule piste en
sortie ; `creation_time` disparaissait du fichier produit.

**Cause racine** — une seule ligne. `-filter_complex` figeait le format de sortie à
`format=yuv444p,format=yuv420p`, et aucun `-map` n'accompagnait les **trois** entrées du graphe
(vidéo + carte X + carte Y). Sans `-map`, FFmpeg applique sa sélection automatique et ne retient
qu'un flux par type ; et avec trois entrées il ne sait de laquelle hériter les métadonnées
globales, donc il n'en hérite d'aucune.

**Correctif** — la chaîne devient
`remap,format=yuv444p10le,format=yuv420p10le[v]` suivie de `-map "[v]" -map "0:a?"
-map_metadata 0`. Le `?` est ce qui empêche une source muette d'échouer.

Les 10 bits ne sont conservés que si **la source les porte et l'encodeur est de la famille
HEVC** : `h264_nvenc` ne sait pas encoder en 10 bits du tout, et le profil High 10 de `libx264`
se lit mal sur du matériel grand public. Les modes 10 bits des GoPro enregistrent en HEVC, donc
le cas qui compte est couvert sans risquer de transformer une conversion qui marchait en échec.
`pix_fmt` est désormais demandé à ffprobe ; absent, il est lu comme du 8 bits — le comportement
antérieur.

Laissé de côté délibérément : les sources au-delà de 10 bits sont ramenées à 10, rien dans ce
projet ne vise Main12.

**Leçon** — L-38.

---

### [2026-09-04] P-08 — Cartes de remappage en PGM P5 binaire

| | |
| --- | --- |
| **Constat** | P-08 ([ANALYSE.md § 3ter](ANALYSE.md)) |
| **Fichiers** | `common/common.go` (`GeneratePGM`, `putMapSample`) · `common/pgm_golden_test.go` |
| **PR** | #28 |
| **Vérification** | `go test -race ./...` ✅ · équivalence prouvée sur la sortie décodée ✅ · contre-épreuve boutisme ✅ |

**Symptôme** — les deux cartes de remappage pèsent jusqu'à **146,6 Mo** pour une source GoPro
4064×3048. `/tmp` étant un tmpfs sur la plupart des distributions récentes, ce sont des
mégaoctets de RAM.

**Cause racine** — le format PGM P2 écrit chaque coordonnée en décimal ASCII suivi d'un
séparateur, soit ~4,4 octets par échantillon là où la valeur en fait 2.

**Correctif** — passage au PGM P5, la variante binaire, en 16 bits gros-boutistes (l'ordre est
imposé par la spécification). **146,6 Mo → 66,1 Mo**, et la génération ne formate plus 33
millions d'entiers. `putMapSample` est l'unique endroit qui décide de l'ordre des octets ; la
carte Y y passe aussi, précisément parce que la première version l'inlinait et rendait le test
d'ordre inopérant (L-37). Le plafonnement dans `putMapSample` est un garde-fou : la
distorsion atterrit toujours dans `[0, largeur-1]`, vérifié sur sept géométries.

**Preuve d'équivalence** — les empreintes SHA-256 des trames décodées par le vrai filtre
`remap` sont **identiques** avant et après, sur quatre cas (squeeze et non-squeeze, deux
géométries). Les empreintes figées de `TestGeneratePGM_Golden` ne pouvaient pas certifier ce
changement, qui réécrit tous les octets par construction : c'est
`TestGeneratePGM_RemapOutputIsStable` qui le fait, et il est auto-référentiel pour ne pas
dépendre du build de FFmpeg (L-36).

**Leçon** — L-36, L-37.

---

### [2026-09-04] P-11 — `libx265` échouait au-delà de 16 cœurs

| | |
| --- | --- |
| **Constat** | P-11 ([ANALYSE.md § 3ter](ANALYSE.md)) — découvert en écrivant le test de N-03 |
| **Fichiers** | `common/common.go` (`clampEncoderThreads`, `x265MaxFrameThreads`) · `common/common_test.go` |
| **PR** | #28 |
| **Vérification** | reproduit à 17/24/32 threads ✅ · `go test -race ./...` ✅ |

**Symptôme** — sur cette machine (24 cœurs logiques), **aucun encodage H.265 ne pouvait
aboutir**. `x265 [error]: frameNumThreads (--frame-threads) must be
[0 .. X265_MAX_FRAME_THREADS)`, puis `Cannot open libx265 encoder`.

**Cause racine** — `encoderThreads` vaut `runtime.NumCPU()` par défaut ; FFmpeg fait
correspondre `-threads` à `--frame-threads` de x265, plafonné à 16. Mesuré : 4, 8, 16 passent,
17 et au-delà échouent. Aggravant : `libx265` étant un encodeur CPU, `isHardwareEncoder` est
faux et la cascade de repli d'`EncodeVideo` ne se déclenche pas — l'échec est terminal.

**Correctif** — `clampEncoderThreads` ramène `-threads` à 16 pour `libx265` uniquement. Les
autres encodeurs se limitent eux-mêmes plutôt que d'échouer, on ne les touche pas.

**Pourquoi personne ne l'avait vu** — il faut plus de 16 cœurs logiques pour le déclencher. Les
runners GitHub en ont 4, et les trois passes d'analyse précédentes n'avaient jamais exécuté
`libx265` sur cette machine.

**Leçon** — L-39.

---


<!-- Les entrées les plus récentes vont en tête de cette section, juste sous ce commentaire. -->

### [2026-09-04] Correction de N-02 et N-01

**PR** — #27.

Deux constats de la 3ᵉ passe traités, sur décision de l'utilisateur. Le reste (N-03 à N-10)
demeure documenté et non traité.

#### N-02 — Processus zombie à chaque annulation

| | |
| --- | --- |
| **Fichiers** | `common/common.go`, `common/cancel_leak_test.go` (nouveau) |
| **Vérification** | garde de régression **prouvée dans les deux sens** ✅ |

La branche `<-cancel` tuait le processus puis retournait sans appeler `cmd.Wait()`. Le processus
n'était donc jamais moissonné, et la goroutine interne que `os/exec` crée pour vider stderr
jamais libérée. Mesuré avant correction : **3 annulations → 3 zombies**, persistant jusqu'à la
fermeture de l'application.

`TestEncodeVideo_CancelReapsProcess` compte les enfants à l'état `Z` en lisant directement
`/proc` — pas de dépendance à `ps`. Correctif retiré : le test échoue en signalant 3 zombies.
Correctif remis : il passe.

#### N-01 — Les sélecteurs proposaient dix formats pour un produit MP4 uniquement

Les trois sélecteurs (Fyne, zenity/kdialog, PowerShell) sont restreints au `.mp4`, ce qui aligne
l'entrée sur la sortie — déjà contrainte par `ensureMP4Extension`. Cela supprime le problème
plutôt que de le contourner : les 5 formats que `CheckVideo` rejetait ne sont plus proposés.

`supportedInputExtensions` centralise la liste, et deux tests la verrouillent, dont un qui
vérifie la **cohérence des deux bouts** : toute extension acceptée en entrée doit être laissée
intacte par `ensureMP4Extension`.

#### Dérive de documentation corrigée au passage

Le bloc « API Documentation » du README montrait encore `common.CheckFfmpeg()` et
`common.PerformEncoding("input.mp4", ...)` — les signatures d'avant la refactorisation C-05.
**J'avais introduit cette dérive dans la 2ᵉ passe sans la corriger.** Voir [[L-29]].


### [2026-09-04] 3ᵉ passe — analyse empirique, aucune correction encore appliquée

**PR** — #27.

Dix constats N-01 à N-10 consignés dans [ANALYSE.md § 3bis](ANALYSE.md).
**Aucun code modifié** : cette passe est un diagnostic, les corrections restent à arbitrer.

Méthode : corpus de fichiers difficiles fabriqué avec FFmpeg, mesures de mémoire, de débit et de
durée, comptage de processus zombies, et validation du chemin matériel sur une **RTX 4070 avec
NVENC fonctionnel** — jamais testé jusqu'ici.

Quatre hypothèses de départ invalidées par la mesure (voir [[L-27]]), deux conclusions corrigées
en cours de route ([[L-25]], [[L-26]]).

Constats les plus solides, tous mesurés :
- **N-02** — 5 annulations produisent 5 zombies ; `cmd.Wait()` les ramène à 0 (prouvé dans les
  deux sens).
- **N-01** — 5 des 10 formats proposés par la GUI sont rejetés.
- **N-07** — l'encodage matériel vaut ×1,7 ; le décodage matériel seulement ~5 %, alors qu'il est
  tenté en premier et qu'un échec tardif coûte un encodage entier.
- **N-03/N-04/N-05** — 10 bits ramenés à 8, pistes audio surnuméraires supprimées, date de prise
  de vue perdue.


### [2026-09-04] 2ᵉ passe — C-05, C-06, et deux constats découverts en chemin

**PR** — #25.

Vérifié en exécution : suite CI complète, `go test ./... -race`, démarrage de la GUI avec
inspection du journal, conversion réelle de bout en bout.

#### C-05 — La configuration ne transite plus par un global mutable

| | |
| --- | --- |
| **Fichiers** | `common/config.go`, `common/common.go`, `common/health.go`, `gui_main.go` |

**Cause racine** — `var currentConfig` avec `GetConfig()`/`SetConfig()`. La GUI y écrivait avant
chaque encodage, ce qui produisait trois défauts distincts : (1) le `video_preset` de
l'utilisateur était **systématiquement écrasé** par le profil qualité, sans que rien ne le
signale ; (2) le global n'était jamais restauré, donc l'état divergeait après le premier
encodage ; (3) il était lu depuis la goroutine d'encodage pendant que le fil UI pouvait encore
y écrire.

**Correctif** — `Config` passée en paramètre à `CheckFfmpeg`, `InitEncodingSession`,
`EncodeVideo`, `PerformEncoding` et `CheckHealth`. `configOrDefault` évite aux appelants de
tester `nil`. Les trois symboles globaux ont disparu.

**Règle de résolution du preset, désormais explicite et testée** : un `video_preset` renseigné
dans la configuration l'emporte sur celui du profil qualité ; le profil ne s'applique que si
l'utilisateur n'a rien précisé.

#### C-06 — Élaguer, mais aussi brancher

Neuf symboles exportés sans appelant. La bonne réponse n'était pas « tout supprimer » : trois
d'entre eux comblaient un vrai manque une fois branchés.

| Symbole | Décision |
| --- | --- |
| `GetHeader` | supprimé — vestige de la CLI retirée |
| `ValidateVideoFile` | supprimé — redondant avec ce que fait déjà `PerformEncoding` |
| `EncodingMetrics.ToJSON` | supprimé — `LogMetrics` écrit déjà dans le journal |
| `CreateDefaultConfig` | supprimé — `LoadConfig("")` fournit déjà les défauts |
| `GetEventHistory`, `ClearHistory` | supprimés avec l'historique lui-même (voir O-06) |
| `LogHealth` | **branché** — le bouton Diagnostic écrit aussi le rapport dans le journal |
| `ResetToolResolutionCache` | **branché** — bouton « Retry » sur le dialogue d'absence de ffmpeg |
| `RecordEncodingEvent` | **branché** — émet enfin l'événement « start », absent du cycle de vie |

Le cas `ResetToolResolutionCache` mérite d'être noté : je l'avais **moi-même introduite sans
appelant** lors de la 1ʳᵉ passe (O-04), en prévision d'un bouton « Réessayer ». C'était de
l'API spéculative. Sans re-sondage, cesser de mettre en cache les échecs ne servait à rien : le
correctif O-04 était incomplet. Le dialogue propose désormais « Retry », qui vide le cache,
re-sonde ffmpeg et réactive l'interface si l'outil vient d'être installé.

**Leçon** — [[L-22]].

#### O-05 — Dispatch des événements en goroutines *(constat découvert en traitant C-06)*

Les quatre méthodes de `EventRecorder` faisaient `go handler.OnX(...)`. Comme
`RecordEncodingProgress` est appelé plusieurs fois par seconde, cela créait une goroutine par
événement et **ne garantissait aucun ordre** ; les derniers événements pouvaient être perdus si
le processus se terminait avant leur exécution. Inoffensif tant que les journaux allaient dans
`io.Discard`, gênant depuis O-02.

Dispatch rendu synchrone. `TestEventRecorder_DispatchIsSynchronousAndOrdered` enregistre les
appels **sans verrou** : un retour à l'asynchrone déclencherait le détecteur de courses.

#### O-06 — Historique d'événements en écriture seule

`EventRecorder` retenait 1000 événements que rien ne lisait. Le fichier de journal contient la
même information, persistée. Supprimé.


### [2026-09-04] Arbitrages utilisateur — S-02, C-01, C-03, C-04

**PR** — #24.

Quatre décisions de périmètre soumises à l'utilisateur, qui a tranché. Toutes vérifiées en
exécution (suite CI complète + démarrage GUI + conversion réelle).

#### S-02 — Liens symboliques : résolus puis validés *(décision : résoudre la cible)*

`isValidInputPath` appelle désormais `filepath.EvalSymlinks` et valide la **destination**.
Ajout au passage d'un contrôle `Mode().IsRegular()` : un périphérique caractère comme
`/dev/zero` ferait lire FFmpeg indéfiniment.

Le test `TestSymlinkRejection`, qui encodait l'ancienne politique, a été **remplacé** par quatre
tests décrivant la nouvelle : lien valide accepté, lien cassé rejeté, lien vers un répertoire
rejeté, fichier non régulier rejeté. Un test qui contredit une décision produit doit être
réécrit, jamais contourné.

#### C-01 — `health.go` branché *(décision : bouton « Diagnostic »)*

Bouton ajouté à la barre d'outils : il exécute `CheckHealth()` **hors du fil UI** (les contrôles
lancent `ffmpeg`), puis affiche `GetHealthReport()` dans une boîte défilante monospace. Les
277 lignes du module deviennent utiles et un rapport d'incident devient exploitable.

**Effet de bord attrapé** — passer de 5 à 6 boutons portait la barre à ~1220 px dans une fenêtre
fixée à 900 px : les boutons auraient été silencieusement rognés (`SetFixedSize(true)` n'ajuste
rien). Boutons ramenés à 150 px, fenêtre à 980×470, et `TestToolbarFitsWindow` ajouté pour que
le prochain bouton ne repasse pas inaperçu. Mesure actuelle : 920 px pour 980 disponibles.

**Leçon** — [[L-20]] : dans une fenêtre à taille fixe, ajouter un widget dans une rangée est un
changement de mise en page à vérifier, pas une simple addition.

#### C-03 — Options mortes retirées *(décision : les retirer)*

`min_video_width`, `min_video_height` et `quality_preset` supprimées de `Config`, de
`superview.yaml`, du `README` et des tests. La GUI conserve ses profils « Fast »/« Balanced ».
Plus aucune promesse non tenue dans la configuration.

#### C-04 — Squeeze exposé *(décision : case à cocher)*

Case « Source already stretched (GoPro SuperView) » ajoutée au formulaire d'options, préférence
persistée (`ui.squeeze_source`), désactivée pendant l'encodage comme les autres contrôles.
`GUIHandler.GetSqueeze()` retourne enfin une vraie valeur.

**Vérifié fonctionnellement** : conversion réelle avec squeeze activé — une entrée 640×480
ressort en 640×480 (largeur conservée, centre dé-étiré), ce qui est le comportement attendu de
cette branche, par opposition au 852×480 du mode normal.


### [2026-09-04] Passe d'application par priorité — 28 constats traités

**PR** — #24.

Environnement de vérification établi au préalable (L-13) : Go 1.26.8 dans `~/.local/go`,
sysroot GUI local, FFmpeg 8.0.1 système. **Tous les correctifs ci-dessous ont été vérifiés en
exécution** : `gofmt`, `go build ./...`, `go vet ./...`, `staticcheck ./...`,
`golangci-lint v2 ./...`, `govulncheck ./...`, `go test ./... -race`, plus un lancement réel de
la GUI et une conversion 4:3 → 16:9 de bout en bout.

---

#### B-01 — Boucle infinie sur erreur de lecture non-EOF

| | |
| --- | --- |
| **Fichiers** | `common/common.go:770-791` |
| **Vérification** | build ✅ · vet ✅ · test ✅ · **garde de régression prouvée** ✅ |

**Symptôme** — toute erreur autre que `io.EOF` sur le pipe de progression faisait tourner la
goroutine de lecture indéfiniment ; `readDone` n'était jamais fermé, le `select` en aval
bloquait pour toujours, l'application se figeait en consommant 100 % d'un cœur.

**Cause racine** — `if err == io.EOF { break }` ne traitait qu'un seul cas d'erreur et ne
prévoyait aucune sortie pour les autres.

**Correctif** — sortie sur **toute** erreur non nulle, avec `errors.Is(err, io.EOF)` pour
distinguer la fin normale (journal `Debug`) d'un échec (journal `Warn`).

**Preuve** — `TestEncodeVideo_ProgressReaderNonEOFErrorDoesNotHang` injecte un lecteur en
erreur permanente. Correctif retiré : le test se fige puis échoue à 20 s. Correctif remis : il
passe en 0,06 s. La garde a donc été validée dans les deux sens.

**Leçon** — [[L-15]] : dans une boucle de lecture, traiter le cas d'erreur générique avant le
cas particulier. Un `break` sur une seule valeur d'erreur est un blocage en puissance.

---

#### B-02 — Configuration introuvable en installation réelle

| | |
| --- | --- |
| **Fichiers** | `common/config.go` (+`ResolveConfigPath`), `gui_main.go` |
| **Vérification** | build ✅ · test ✅ · **test fonctionnel depuis `/`** ✅ |

**Symptôme** — `LoadConfig("superview.yaml")` résolvait un chemin relatif contre le répertoire
de travail. Lancée depuis un lanceur `.desktop` ou le menu Démarrer, l'application ne lisait
jamais sa configuration.

**Correctif** — `ResolveConfigPath()` sonde dans l'ordre : `$SUPERVIEW_CONFIG`, le répertoire
de l'exécutable, `os.UserConfigDir()/superview/`, puis le répertoire courant. Le chemin retenu
est journalisé.

**Preuve** — binaire et `superview.yaml` copiés dans `/tmp/installdir`, lancement avec
`cd / && /tmp/installdir/superview-gui`. Journal :
`Configuration loaded from file path=/tmp/installdir/superview.yaml`. Avant le correctif, rien
n'était trouvé.

**Leçon** — [[L-16]] : un chemin relatif dans une application graphique est un bug latent ; le
CWD d'une application lancée par l'environnement de bureau est arbitraire.

---

#### O-01 — Encodage bridé au temps réel par défaut

| | |
| --- | --- |
| **Fichiers** | `common/common.go` (`buildEncodeBaseArgs`), `common/config.go`, `superview.yaml`, `README.md` |
| **Vérification** | test ✅ · **conversion réelle** ✅ |

**Symptôme** — le mode par défaut `safe` ajoutait `-re`, plafonnant tout encodage à la durée
du clip. Combiné à B-02, cela s'appliquait à la quasi-totalité des utilisateurs.

**Correctif** — `-re` supprimé des deux modes, avec un commentaire expliquant pourquoi.
`performance_mode` ne pilote plus que la stratégie audio (AAC vs copie), documentation mise à
jour partout. Le paramètre `safePerformanceMode`, devenu inutile dans `buildEncodeBaseArgs`, a
été retiré de la signature plutôt que neutralisé par un `_ =`.

**Leçon** — [[L-14]].

---

#### O-03 — `GeneratePGM` 4× plus rapide, sortie identique au bit près

| | |
| --- | --- |
| **Fichiers** | `common/common.go`, `common/pgm_golden_test.go` (nouveau) |
| **Vérification** | **SHA-256 identiques sur 4 jeux de dimensions** ✅ · benchmark ✅ |

**Cause racine** — deux redondances : (1) la carte X est **invariante par ligne** — `sx`, `tx`
et `offset` ne dépendent que de `x` — mais était recalculée 1080 fois à l'identique ;
(2) chaque ligne de la carte Y appelait `strconv.AppendInt` une fois par pixel pour formater
toujours le même nombre. Les écritures étaient de plus non tamponnées.

**Correctif** — ligne X construite une seule fois puis écrite `outY` fois ; nombre de la ligne
Y formaté une fois par ligne ; `bufio.Writer` de 1 MiB sur les deux fichiers, avec `Flush`
vérifié.

**Mesure** — 1440×1080 : **67,2 ms → 16,9 ms**.

**Preuve** — empreintes SHA-256 et tailles capturées **avant** modification sur 4 cas
(1440×1080 et 641×481, avec et sans squeeze), comparées après : strictement identiques. Elles
sont figées dans `TestGeneratePGM_Golden`, accompagné de
`TestGeneratePGM_XMapIsRowInvariant` qui documente la propriété exploitée.

**Leçon** — [[L-12]].

---

#### B-07 — Parsing de la sortie FFmpeg résistant aux formats inattendus

| | |
| --- | --- |
| **Fichiers** | `common/common.go` (`parseFFmpegVersion`, `parseFFmpegEncoders`) |
| **Vérification** | **test différentiel contre FFmpeg 8.0.1 réel** ✅ |

**Symptôme** — `strings.Split(version, " ")[2]` et `enc[2]` paniquaient sur une sortie FFmpeg
non standard ; le saut d'en-tête était figé à 10 lignes en dur.

**Correctif** — deux fonctions pures et testables : version repliée sur `"unknown"` si moins de
3 champs ; liste d'encodeurs scannée à partir du séparateur `------`, avec repli sur l'ensemble
des lignes bien formées s'il est absent.

**Preuve** — test différentiel exécutant l'ancien et le nouvel algorithme sur la sortie réelle
de FFmpeg 8.0.1 : **13 encodeurs identiques dans le même ordre**, version identique. Puis
tests unitaires permanents sur les entrées malformées (chaîne vide, `" V"` seul, CRLF, absence
de séparateur).

**Leçon** — [[L-17]] : quand on remplace un parsing, prouver l'équivalence sur une entrée
réelle avant de raisonner sur les cas limites.

---

#### B-04 — `-threads` configurait le décodeur, pas l'encodeur

| | |
| --- | --- |
| **Fichiers** | `common/common.go` |
| **Vérification** | test ✅ · **mesure avant/après** ✅ |

**Correctif** — `-threads` déplacé après `-c:v <encoder>`.

**Mesure honnête** — aucun écart significatif sur le clip de test (785/782 ms avant,
767/778/774 ms après) : 20 s en 640×480 est trop court et trop petit pour être limité par les
threads. **C'est une correction de justesse, pas de performance** — le code configure
désormais ce qu'il annonce configurer. Un gain reste plausible sur des sources 4K longues,
non mesuré ici.

**Leçon** — [[L-03]], et : rapporter une mesure nulle telle quelle plutôt que de supposer un
gain.

---

#### B-05, B-06, B-08 — Bugs GUI et flux

| Constat | Correctif | Fichier |
| --- | --- | --- |
| **B-05** | Entrées vides filtrées dans la liste d'encodeurs — `strings.Split("", ",")` retourne `[""]`, d'où une option fantôme `" encoder"` quand FFmpeg est absent | `gui_main.go` |
| **B-06** | Chemin d'annulation unique et idempotent, partagé par le bouton *Cancel* et l'intercepteur de fermeture ; un seul `close(cancelEncoding)` subsiste, sous garde. Élimine la `panic: close of closed channel` | `gui_main.go` |
| **B-08** | Erreurs de `Close()` des fichiers PGM remontées via un retour nommé — un fichier tronqué au flush produisait une erreur FFmpeg incompréhensible bien plus tard | `common/common.go` |

**Leçon (B-06)** — [[L-18]] : un canal ne doit être fermé qu'à un seul endroit ; si deux
chemins peuvent annuler, ils appellent la même fonction idempotente.

---

#### S-01, S-03, S-04 — Validation d'entrées

| Constat | Correctif |
| --- | --- |
| **S-01** | `hasTraversalComponent` teste les **composants** du chemin, plus la sous-chaîne. `vacances..2024.mp4` et `GoPro..raw/` étaient rejetés à tort ; `../../etc/passwd` l'est toujours |
| **S-03** | `os.CreateTemp` remplace le fichier sonde à nom fixe `.superview_write_test` — plus de collision entre exécutions concurrentes ni de résidu prévisible |
| **S-04** | Répertoires système Windows lus depuis `%SystemRoot%` / `%ProgramFiles%`, littéraux `C:\...` conservés en repli ; comparaison insensible à la casse sous Windows |

Tests ajoutés pour chacun, dont la non-régression du rejet de traversée réelle.

**Leçon** — [[L-19]] : une validation de sécurité qui rejette des entrées légitimes se paie en
utilisabilité sans rien acheter. Valider la structure (composants), pas la forme textuelle.

---

#### X-01, X-04 — La CI voit enfin le paquet GUI

| | |
| --- | --- |
| **Fichiers** | `.github/workflows/lint.yml`, `test.yml`, `release.yml`, `.golangci.yml` |

**Correctif** — portée passée de `./common` à `./...` pour `golangci-lint`, `go vet`,
`staticcheck`, `govulncheck` et `go test`, avec installation de `libgl1-mesa-dev xorg-dev`
(et `ffmpeg`) sur les jobs concernés. `.golangci.yml` migré en schéma v2, golangci-lint
épinglé à **v2.13.2** via le chemin de module `/v2`.

**staticcheck cadré volontairement** sur `SA*` + `S1*`, équivalent fidèle de l'ancienne paire
`staticcheck` + `gosimple`. La famille `QF*` est exclue : ce sont des suggestions de
refactorisation d'éditeur, et l'une d'elles réécrirait les `math.Pow` de `GeneratePGM`, dont la
forme reflète l'algorithme de référence ([[L-09]]).

**Ce que l'extension a immédiatement révélé** — 4 défauts jamais vus par la CI :
`updateHardwareStatus` assigné sans effet, `dialog.ProgressDialog` déprécié **et toujours nil**
(champ mort supprimé), `theme.DarkTheme()` déprécié (remplacé par un thème personnalisé qui
force la variante sombre — même rendu, testé), et `profileBitrate` initialisé puis écrasé dans
chaque branche.

**Leçon** — [[L-02]], [[L-11]], [[L-13]].

---

#### O-02 — Les diagnostics deviennent exploitables

| | |
| --- | --- |
| **Fichiers** | `common/observability.go` (`OpenLogFile`, `ParseLogLevel`), `gui_main.go` |
| **Vérification** | **lancement réel de la GUI, journal inspecté** ✅ |

**Symptôme** — le logger écrivait dans `io.Discard`. Les 650 lignes de `observability.go` +
`metrics.go` alimentaient un puits sans fond : aucun diagnostic récupérable après un échec.

**Correctif** — journal dans `os.UserCacheDir()/superview/superview.log`, plafonné à 5 MiB
(remise à zéro au-delà), niveau piloté par `log_level` une fois la configuration chargée.
Repli silencieux sur `io.Discard` si le fichier ne peut pas s'ouvrir : la journalisation ne
doit jamais empêcher le démarrage.

**Preuve** — GUI lancée, contenu de `~/.cache/superview/superview.log` vérifié.

---

#### O-04 — Plus de cache sur les échecs de résolution

**Symptôme** — `resolveToolBinary` mémorisait l'échec pour toute la durée du processus. La GUI
affiche justement un lien d'installation quand FFmpeg manque ; l'utilisateur installait puis
devait redémarrer sans qu'on le lui dise.

**Correctif** — les échecs ne sont plus mis en cache (les succès le restent).
`ResetToolResolutionCache()` exposée pour un futur bouton « Réessayer ». Deux tests couvrent
les deux comportements.

---

#### C-02, C-07, C-08 — Nettoyage

- **C-02** : `EncodingOptions` supprimée (aucune référence dans tout le dépôt, tests compris).
- **C-07** : messages d'erreur conformes aux conventions Go — `"Error running ffmpeg, output is:\n%s"`
  devient `"ffmpeg failed: %w"`, avec `%w` au lieu de `%s` pour préserver la chaîne d'erreurs.
- **C-08** : constante `exitCodeUnavailable` à la place des 9 occurrences de `-1`.

---

#### T-01, T-02, T-03 — Filet de sécurité

- **T-01** — premiers tests du paquet `main` : `ensureMP4Extension` et `isMissingFFmpegError`
  extraits pour être testables, plus `containsString`, `formatResultsPanel` (dont la division
  par zéro), le thème sombre forcé et les accesseurs de `GUIHandler`. **9 tests** (dont la garde de mise en page de la barre d'outils).
- **T-03** — deux tests d'intégration autonomes : ils génèrent leur propre clip 4:3 via
  `ffmpeg -f lavfi -i testsrc`, exécutent tout le pipeline et **vérifient que la sortie est bien
  en 16:9** (640×480 → 852×480), que la progression remonte et que les métriques sont
  enregistrées ; le second vérifie que l'annulation interrompt réellement FFmpeg.
  Ils s'ignorent proprement si FFmpeg est absent.
- **T-02** — couverture réelle **54,6 %**, seuil CI relevé de **30 % → 50 %** et mesuré sur
  `./...` au lieu de `./common`.

---

#### X-02, X-03, X-05 à X-08 — Outillage et documentation

| Constat | Correctif |
| --- | --- |
| **X-02** | `Makefile` compile le paquet (`.`) au lieu du fichier inexistant `superview-gui.go` |
| **X-03** | `.github/copilot-instructions.md` corrigé : `gui_main.go`, Go 1.26, bitrate max 200 M, règle `fyne.Do` ajoutée |
| **X-05** | `softprops/action-gh-release` épinglée par SHA (`efb3536…`, v3.0.3) — c'était la seule action non épinglée, et celle qui détient `contents: write` |
| **X-06** | `coverage.out` retiré du suivi git ; `coverage.out` et `coverage.html` ajoutés au `.gitignore` |
| **X-07** | Lignes `echo >> /tmp/release.md` mortes retirées du job `notify` (elles écrivaient sur une autre machine) |
| **X-08** | `build.sh` marqué obsolète et refuse de s'exécuter sans `SUPERVIEW_ALLOW_LEGACY_BUILD=1` |

Le `README.md` a également été aligné : plus aucune référence à `superview-gui.go` dans le
dépôt, et la procédure de résolution du fichier de configuration y est documentée.

---

#### B-03 — Constat invalidé, aucune correction nécessaire

Voir [ANALYSE.md](ANALYSE.md) B-03 et [[L-10]]. Le remplacement par
`file.URI().Path()` a été conservé comme simplification, pas comme correctif.

---


# Working on Superview

Superview converts 4:3 video to 16:9 by dynamic distortion: the edges are
stretched, the centre keeps its ratio. It generates two PGM remap maps and hands
them to FFmpeg's `remap` filter. Go, Fyne, **GUI-only**, Windows and Linux.

**MP4 in, MP4 out.** The file pickers offer nothing else and the output
extension is enforced. This is a product constraint, not an oversight.

Read [`docs/CONTRATS.md`](docs/CONTRATS.md) before editing a `.go` file: it holds
the pipeline invariants and the FFmpeg facts established by measurement. This
file is the short version — what bites first.

## Verifying a change

```bash
. /tmp/guienv.sh                        # Linux without sudo: see docs/ENVIRONNEMENT.md
gofmt -l .                              # must print nothing
go build ./... && go vet ./...
SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1
golangci-lint run ./... --timeout=5m
```

Three things about that:

- **The root package needs GUI headers to compile at all.** On a machine without
  `sudo`, `docs/ENVIRONNEMENT.md` builds a sysroot in `/tmp` that does not
  survive a reboot. Without it `./common` compiles and `main` does not — do not
  report a GUI change as verified in that state.
- **`SUPERVIEW_REQUIRE_FFMPEG=1` is not optional.** Without it, every test that
  needs ffmpeg skips silently: the four integration tests and the remap
  equivalence test, which together are the whole of what checks a real
  conversion. A green suite can mean nothing was encoded. CI sets it.
- **`./common` alone is not the suite.** It skips the root package's GUI tests
  and is not what the 50% coverage gate measures.

**Every test is proved by counter-proof**: reintroduce the defect it guards and
confirm the test reddens. Tests here have passed vacuously before. And check the
counter-proof itself is valid — an environment can make it mute.

## Decisions that look like defects

Do not "fix" these. Each was measured or argued, and reverting one costs more
than it looks.

| What you'll see | Why it is that way |
| --- | --- |
| `.golangci.yml` restricts staticcheck to `SA*`+`S1*` | `QF*` would rewrite the `math.Pow` calls in `GeneratePGM`, which mirror a published reference algorithm and must stay readable against it. `ST*` is off deliberately too. Never add a second linter job beside golangci-lint: it would enforce what the config disables. |
| `FyneApp.toml` has no `Version` | The version comes from the tag, via `fyne package --app-version`, so a published binary cannot claim a number a committed file drifted away from. A plain `go build` reports `dev`. **Do not add one back.** |
| `main()` is ~540 lines | Deliberate. The defect was that its *state* was unreachable, fixed by the `appState` type and its methods. Widget construction has nothing to gain from being split. |
| `.github/release.yml` has a `"*"` catch-all | Load-bearing. GitHub drops any pull request matching no category, and this repository labels none of its own — removing it empties every release. |
| `common/common.go` is ~1600 lines | Known. Splitting `pgm.go` and `tools.go` out is identified and not urgent: it is 81% covered and `pgm_golden_test.go` pins the geometry byte for byte. |
| The bundled FFmpeg is pinned to **8.1.1**, not the newest | What is pinned is its NVENC driver floor, 570.0, not its version. gyan.dev's 8.1.2 is compiled against newer NVIDIA headers and demands driver 610.00, which a professional card cannot reach — its driver branch stops at 597.06. Bumping the pin to "the current release" silently takes hardware encoding away from those machines. `.github/scripts/nvenc-driver-floor.sh` fails the release if the floor moves; read [`RELEASING.md`](RELEASING.md) before touching it. |
| The release workflow patches fyne's generated `Makefile` | It installs three named files, so the bundled `ffmpeg`/`ffprobe` have to be added to it, and its icon line is broken upstream (`$(Icon)` without the `.png`, so `make install` fails on its last line). The patch is keyed on the exact lines, and the job then runs `make install` into a throwaway directory: if fyne changes shape, the release fails there instead of shipping a package that installs nothing. |
| The French README is `README_FR.md`, but the English one is not `README_EN.md` | GitHub renders a repository's landing page from `README.md` and no other name. Renaming the English half to `README_EN.md` for symmetry leaves the repository front page with no README at all — a visitor sees the file tree and nothing else. The asymmetry is the cost of the landing page. |
| Progress events log at debug level | They fire several times a second. Raising them drowns the log the README asks users to attach to bug reports. |

## Contracts you must not break

- **Configuration is passed explicitly.** `CheckFfmpeg(cfg)`,
  `InitEncodingSession(cfg)`, `PerformEncoding(cfg, …)`. There is no config
  global any more; do not reintroduce one.
- **Session lifecycle**: `InitEncodingSession(cfg)` then `defer common.CleanUp()`.
  Temporary files live in the session's isolated directory — never the working
  directory, never a hardcoded path.
- **Any widget update from a goroutine goes through `fyne.Do(...)`.** The encode
  runs in a goroutine; the UI must stay responsive.
- **Bitrates are bits per second**, everywhere, including logs and comments.
  They were documented as bytes for a long time, off by a factor of eight.
- **`exec.Command` with arguments, never a shell string**, for ffmpeg and
  ffprobe. Paths from file pickers are untrusted: validate before use.
- **Cancellation must leave nothing behind** — no orphan ffmpeg process, no
  partial output file, no temp directory. Tests pin all three.
- **Check every error return.** Domain errors are typed: `InvalidVideoError`,
  `EncoderError`, `SessionError`.

## Changing the code

- **Build the package, never a file**: `go build .`. The native dialog files are
  build-tagged and `go build gui_main.go` fails.
- Keep user-facing strings stable unless the task is about them.
- **`README.md` and `README_FR.md` are one document in two languages.** Any
  user-visible change goes into both, in the same pull request. CI enforces it
  (`readme-parity` in `lint.yml`): it fails when one moves without the other,
  and when their heading structures diverge. In the French file, **GUI labels
  stay in English** and in their exact case -- translating *Choose input file*
  sends the reader looking for a button that does not exist -- while Windows'
  own strings are the French ones a French Windows shows (*Informations
  complémentaires*, *Exécuter quand même*).
- **The repository's language split**, until now unwritten: internal audit
  journals (`docs/LECONS.md`, `docs/ANALYSE.md`, `docs/CONTRATS.md`,
  `docs/ENVIRONNEMENT.md`) are in French; everything an outsider reads -- both
  READMEs' English half, `RELEASING.md`, `AGENTS.md`, `SECURITY.md`,
  `docs/hardware-support.md`, the GUI, error strings, commit and pull request
  titles -- is in English.
- After changing an exported signature, sweep the prose:
  `grep -rn "FunctionName" --include='*.md' .` — Markdown is not compiled, and a
  stale example in the README survived five audit passes here.

## Landing a change

**Every change goes through a pull request, however small.** Release notes are
generated from merged pull requests, so a direct push to `master` ships the
change and loses the record of it. One commit did exactly that and appears in no
release. A repository ruleset now refuses the direct push rather than trusting
this paragraph, and holds the required checks in front of every merge.

**The pull request title is what users read in the release.** Write it as a
sentence about what changed — *Stop release binaries from announcing themselves
as modified*, not *fix bug*. Commit messages do not appear there.

**Put the `merge` label on it, and stop there.**
[`merge-on-label.yml`](.github/workflows/merge-on-label.yml) works out which
method applies, merges once the required checks pass, and comments on the pull
request saying which method it took and why. Nothing else starts it, so an
unlabelled pull request stays where it is: you still decide *when* something
lands, you no longer decide *how*. If the label produced no comment, remove it
and put it back — a label already present fires nothing.

The rule it applies, which is also the one to follow when merging by hand:

**Squash and merge**, which is what the whole history uses: one commit per pull
request, titled `Title (#NN)`. Use *Create a merge commit* only when another
open pull request is based on this one — squash rewrites the SHA, and the one
stacked above then loses its base and conflicts.

**The question is what sits on top of you, not what you sit on**, and one
command answers it:

```bash
gh pr list --base "$(gh pr view <NN> --json headRefName -q .headRefName)" --state open
```

Nothing listed → *Squash and merge*. Anything listed → *Create a merge commit*,
and merge the stack from the bottom up. **A `base` of `master` settles nothing**:
the bottom pull request of a stack has exactly that base and still must not be
squashed. That is how #59, #61 and #62 came to need merge commits while their
base read `master` — the earlier wording here said the opposite, and following
it would have orphaned the pull request above each of them.

Three repository settings hold the rest of it up, and they are why you no
longer have to think about the dialog either: *Rebase and merge* is switched
off, because it writes neither `Title (#NN)` nor a merge commit and nothing
here has ever used it; the squash title is pinned to the pull request title,
which used to fall back to the commit subject on a single-commit pull request
and quietly contradict the paragraph above; and the branch is deleted on merge.

Releases are one button, documented in [`RELEASING.md`](RELEASING.md). Nothing to
prepare, no file to write, no version to bump. Never tag by hand unless the
intent is explicitly to release.

## Recording what you learn

Two journals, and they are how this repository keeps from repeating itself:

- [`docs/LECONS.md`](docs/LECONS.md) — a numbered lesson per generalisable rule,
  plus the fix log. **Read § 2 before correcting anything**: the correction may
  have been tried and rejected.
- [`docs/ANALYSE.md`](docs/ANALYSE.md) — the numbered findings and their state.
  Its § 4 status table is the one that counts. When a finding turns out to be
  wrong, mark it invalid and say why rather than deleting it.

After a correction, add the entry. A lesson whose remedy is a command ends by
running the command, not by fixing the one occurrence that inspired it.

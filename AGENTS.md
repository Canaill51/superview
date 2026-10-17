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

`scripts/check.sh` runs the recipe below in one command (`--fast` stops before
the tests). It is what the git pre-commit hook, the Claude hooks in `.claude/`
and `make check` call. Enable the git hook once per clone:
`git config core.hooksPath .githooks`. The gate-by-gate form below is what
`/verify` reports.

```bash
. /tmp/guienv.sh                        # Linux without sudo: see docs/ENVIRONNEMENT.md
gofmt -l .                              # must print nothing
go build ./... && go vet ./...
SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1
golangci-lint run ./... --timeout=5m
```

Three things about that:

- **The `cmd/superview` package needs GUI headers to compile at all.** On a machine without
  `sudo`, `docs/ENVIRONNEMENT.md` builds a sysroot in `/tmp` that does not
  survive a reboot. Without it `./internal/common` compiles and `cmd/superview` does not — do not
  report a GUI change as verified in that state.
- **`SUPERVIEW_REQUIRE_FFMPEG=1` is not optional.** Without it, every test that
  needs ffmpeg skips silently: the integration tests and the remap
  equivalence test, which together are the whole of what checks a real
  conversion. A green suite can mean nothing was encoded. CI sets it.
- **`./internal/common` alone is not the suite.** It skips the `cmd/superview` GUI tests
  and is not what the 50% coverage gate measures.

**Every test is proved by counter-proof**: reintroduce the defect it guards and
confirm the test reddens. Tests here have passed vacuously before. And check the
counter-proof itself is valid — an environment can make it mute.

### Rust (port in progress, DEC-43)

`rust/` is a Cargo workspace (`superview-core`, `superview-harness`, `superview-app`) that
grows beside the Go code; Go stays the reference and the shipped product until parity.
`scripts/checks/rust.sh` is the Rust half of `scripts/check.sh`, so the pre-commit hook now needs cargo for every commit, Go-only ones included. Install with rustup into
`~/.cargo` (no sudo, `docs/ENVIRONNEMENT.md`). The gate-by-gate form, from `rust/`:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked
```

- **No test modifies the process environment.** Tests run in parallel in one process and
  `std::env::set_var` is `unsafe` in edition 2024; tool lookup takes the environment as a
  parameter (L-114).
- **`powi(2)`, never `powf(2.0)`**, where `pgm.go` has `math.Pow(x, 2)`: the latter depends
  on the platform's libm and would break the golden fingerprints.
- `superview-harness` is a test bench, not a CLI, and is never shipped (DEC-44).

## Decisions that look like defects

Do not "fix" these. Each was measured or argued, and reverting one costs more
than it looks.

| What you'll see | Why it is that way |
| --- | --- |
| `.golangci.yml` restricts staticcheck to `SA*`+`S1*` | `QF*` would rewrite the `math.Pow` calls in `GeneratePGM`, which mirror a published reference algorithm and must stay readable against it. `ST*` is off deliberately too. Never add a second linter job beside golangci-lint: it would enforce what the config disables. |
| `FyneApp.toml` has no `Version` | The version comes from the tag, via `fyne package --app-version`, so a published binary cannot claim a number a committed file drifted away from. A plain `go build` reports `dev`. **Do not add one back.** |
| `main()` is ~600 lines | Deliberate. The defect was that its *state* was unreachable, fixed by the `appState` type and its methods. Widget construction has nothing to gain from being split. |
| `.github/release.yml` has a `"*"` catch-all | Load-bearing. GitHub drops any pull request matching no category, and this repository labels none of its own — removing it empties every release. |
| `internal/common/` is eight files where there used to be one `common.go` | Split by responsibility on 2026-10-14, as pure moves: `pipeline.go`, `encode.go`, `pgm.go`, `session.go`, `video.go`, `tools.go`, `memory.go`, `errors.go`. `pgm_golden_test.go` pinned the geometry byte for byte across the move. Put new code in the file whose header names its job; do not grow a new catch-all. |
| The bundled FFmpeg is **8.1.1 on Windows** (gyan.dev) and **n8.1.2 on Linux** (BtbN autobuild 2026-08-31), neither the newest | What is pinned is the NVENC driver floor, 570.0 on both, not a version. gyan.dev's 8.1.2 is compiled against newer NVIDIA headers and demands driver 610.00, which a professional card cannot reach — its driver branch stops at 597.06. Bumping a pin to "the current release" silently takes hardware encoding away from those machines. `.github/scripts/nvenc-driver-floor.sh` fails the release if the floor moves; read [`RELEASING.md`](RELEASING.md) before touching either. |
| The release ships `…-setup.exe` and `….AppImage`, not archives | One ready-to-run file per platform. `fyne package` still compiles and stamps the version, but only its binary is kept. Each package is installed or launched in CI (installer silently, AppImage under `xvfb`), because a package that builds is not one that starts. The installer's `AppId` in `packaging/windows/superview.iss` must never change. See DEC-41. |
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
  partial output file, no temp directory. Tests pin all three: the Go suite checks the
  first two, `superview-harness`'s signal tests (Rust, Unix only) check the temp directory as well.
- **Check every error return.** Domain errors are typed: `InvalidVideoError`,
  `EncoderError`, `SessionError`.

## Changing the code

- **Build the package, never a file**: `go build ./cmd/superview`. The native dialog files are
  build-tagged and `go build cmd/superview/gui_main.go` fails.
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

- **Every change goes through a pull request, however small.** Release notes
  are generated from merged pull requests; a ruleset refuses direct pushes to
  `master`.
- **The pull request title is what users read in the release**: a sentence
  about what changed, not *fix bug*.
- **Put the `merge` label on it, and stop there**: the workflow picks squash or
  merge commit and merges once checks pass. From a terminal, label with
  `gh api -X POST "repos/$REPO/issues/<NN>/labels" -f "labels[]=merge"` —
  `gh pr edit` fails on this repository, silently with stderr redirected.
- Releases are one button ([`RELEASING.md`](RELEASING.md)). Never tag by hand
  unless the intent is explicitly to release. **An agent publishes only when the
  user asks for a release in the conversation, and only through `/publier`**: it
  checks the preconditions, proposes the version, waits for the user's yes to that
  exact number, dispatches the Release workflow and verifies the published files.
  A merged pull request or a green CI is not a request.

Merging by hand, stacked pull requests, the repository settings behind all
this: [`docs/MERGING.md`](docs/MERGING.md).

## Recording what you learn

Two journals, and they are how this repository keeps from repeating itself:

- [`docs/LECONS.md`](docs/LECONS.md) — a numbered lesson per generalisable rule.
  **Read § 2 before correcting anything**: the correction may have been tried and
  rejected. [`docs/PIEGES.md`](docs/PIEGES.md) indexes it by keyword; the fix log
  is [`docs/CORRECTIONS.md`](docs/CORRECTIONS.md), where new entries go.
- [`docs/ANALYSE.md`](docs/ANALYSE.md) — the numbered findings and their state.
  Its § 4 status table is the one that counts. When a finding turns out to be
  wrong, mark it invalid and say why rather than deleting it. Passes 3 to 13
  live in `docs/archives/` (sections left as pointers); find a finding with
  `grep -rn "U-03" docs/`.
- [`docs/SUITE.md`](docs/SUITE.md) — where things stand; the session-start hook
  prints its first lines. [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) is the map.

And one register, [`docs/DECISIONS.md`](docs/DECISIONS.md): every choice that
looks like a defect, with its reason and the condition that would reopen it.
The table above is its short version. A new decision gets an entry; a reversed
one is marked *Remplacée*, never deleted.

After a correction, add the entry. A lesson whose remedy is a command ends by
running the command, not by fixing the one occurrence that inspired it.

`.claude/` holds the Claude Code side of this (and `/publier`, see above): the permission allowlist for the
verification recipe, a `gofmt` hook on every Go edit, and `/verify`, which runs
the recipe above and reports each gate instead of summarising. Other agent tools
ignore the directory; nothing in it is an instruction.

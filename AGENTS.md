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
- After changing an exported signature, sweep the prose:
  `grep -rn "FunctionName" --include='*.md' .` — Markdown is not compiled, and a
  stale example in the README survived five audit passes here.

## Landing a change

**Every change goes through a pull request, however small.** Release notes are
generated from merged pull requests, so a direct push to `master` ships the
change and loses the record of it. One commit did exactly that and appears in no
release.

**The pull request title is what users read in the release.** Write it as a
sentence about what changed — *Stop release binaries from announcing themselves
as modified*, not *fix bug*. Commit messages do not appear there.

**Squash and merge**, which is what the whole history uses: one commit per pull
request, titled `Title (#NN)`. Use *Create a merge commit* only when the pull
request is based on another that is still open — squash rewrites the SHA and the
next one in the stack then loses its base and conflicts. The `base` field says
which case you are in: if it is not `master`, you are stacked.

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

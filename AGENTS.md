# Working on Superview

Superview converts 4:3 video to 16:9 by dynamic distortion: the edges are
stretched, the centre keeps its ratio. It generates two PGM remap maps and hands
them to FFmpeg's `remap` filter. Rust, Slint, **GUI-only**, Windows and Linux. (It was
Go and Fyne until v0.2.9, the last Go release; Go was removed in DEC-49.)

**MP4 in, MP4 out.** The file pickers offer nothing else and the output
extension is enforced. This is a product constraint, not an oversight.

Read [`docs/CONTRATS.md`](docs/CONTRATS.md) before editing a Rust file: it holds
the pipeline invariants and the FFmpeg facts established by measurement. This
file is the short version — what bites first.

## Verifying a change

`scripts/check.sh` runs the recipe below in one command (`--fast` stops before
the tests). It is what the git pre-commit hook, the Claude hooks in `.claude/`
and `make check` call. Enable the git hook once per clone:
`git config core.hooksPath .githooks`. The gate-by-gate form below is what
`/verify` reports.

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked
```

Two things about that:

- **`SUPERVIEW_REQUIRE_FFMPEG=1` is not optional.** Without it, every test that
  needs ffmpeg skips silently: the integration tests and the remap
  equivalence test, which together are the whole of what checks a real
  conversion. A green suite can mean nothing was encoded. CI sets it.
- **The workspace is the suite.** `cargo test -p superview-core` alone skips the
  window tests of `superview-app`.

**Every test is proved by counter-proof**: reintroduce the defect it guards and
confirm the test reddens. Tests here have passed vacuously before. And check the
counter-proof itself is valid — an environment can make it mute.

### The Rust workspace

The root is a Cargo workspace (`superview-core`, `superview-harness`, `superview-app`) and the
whole product. `scripts/checks/rust.sh` is the Rust half of `scripts/check.sh`, so the
pre-commit hook needs cargo for every commit. Install with rustup into `~/.cargo` (no sudo,
`docs/ENVIRONNEMENT.md`); the toolchain is pinned by `rust-toolchain.toml`.
`make coverage-audit` runs what the CI job *Rust coverage and audit* runs (a 50 % line floor
with `cargo llvm-cov`, then `cargo audit` and `cargo deny`); it needs the three tools installed.

- **No test modifies the process environment.** Tests run in parallel in one process and
  `std::env::set_var` is `unsafe` in edition 2024; tool lookup takes the environment as a
  parameter (L-117).
- **`powi(2)`, never `powf(2.0)`**, where Go's `pgm.go` had `math.Pow(x, 2)`: the latter depends
  on the platform's libm and would break the golden fingerprints
  (`superview-core/tests/pgm_golden.rs`).
- The parity fixtures under `superview-core/tests/fixtures/` (`parity/*.json`, `ffprobe/`) were
  recorded from the Go code and are now frozen: they are the specification, not output to
  regenerate. A failing parity test means the Rust changed, not that the fixture is stale.
- `superview-harness` is a test bench, not a CLI, and is never shipped (DEC-44). It no longer
  serves a Go cross-check; it stays for the tests that run a real conversion in a
  separate process and send it SIGINT/SIGTERM (`superview-harness/tests/harness.rs`).

## Decisions that look like defects

Do not "fix" these. Each was measured or argued, and reverting one costs more
than it looks.

| What you'll see | Why it is that way |
| --- | --- |
| No real version in any committed file (`0.0.0` placeholders in `Cargo.toml` and `packaging/windows/superview.iss` only); `build.rs` reads `SUPERVIEW_VERSION` and a plain `cargo build` reports `dev` | The version comes from the tag (the release workflow sets the variable), so a published binary cannot claim a number a committed file drifted away from. **Do not add a version constant back** (DEC-22, DEC-47). |
| `.github/release.yml` has a `"*"` catch-all | Load-bearing. GitHub drops any pull request matching no category, and this repository labels none of its own — removing it empties every release. |
| The bundled FFmpeg is **8.1.1 on Windows** (gyan.dev) and **n8.1.2 on Linux** (BtbN autobuild 2026-08-31), neither the newest | What is pinned is the NVENC driver floor, 570.0 on both, not a version. gyan.dev's 8.1.2 is compiled against newer NVIDIA headers and demands driver 610.00, which a professional card cannot reach — its driver branch stops at 597.06. Bumping a pin to "the current release" silently takes hardware encoding away from those machines. `.github/scripts/nvenc-driver-floor.sh` fails the release if the floor moves; read [`RELEASING.md`](RELEASING.md) before touching either. |
| The release ships `…-setup.exe` and `….AppImage`, not archives | One ready-to-run file per platform (`cargo build --release`, version from `SUPERVIEW_VERSION`). Each package is installed or launched in CI (installer silently, AppImage under `xvfb`), because a package that builds is not one that starts. The installer's `AppId` in `packaging/windows/superview.iss` must never change. The release *build* jobs do not cache `target/`: the build marker must describe that build. See DEC-41, DEC-47. |
| The AppImage carries `libxkbcommon`, `libxkbcommon-x11` and `libxcb-xkb`, found through a `DT_RPATH` set in `build.rs`, with no `LD_LIBRARY_PATH` in `AppRun` | An `LD_LIBRARY_PATH` would be inherited by ffmpeg, zenity and kdialog; a `RUNPATH` (the default) does not reach the dependencies of a `dlopen`ed library; two of the three libraries without the third crash at start-up on a newer host. CI cannot see that last failure: test by hand (DEC-47). |
| The French README is `README_FR.md`, but the English one is not `README_EN.md` | GitHub renders a repository's landing page from `README.md` and no other name. Renaming the English half to `README_EN.md` for symmetry leaves the repository front page with no README at all — a visitor sees the file tree and nothing else. The asymmetry is the cost of the landing page. |
| Progress events log at debug level | They fire several times a second. Raising them drowns the log the README asks users to attach to bug reports. |
| `superview-harness` has `convert` and `probe` subcommands | A test bench, never packaged or mentioned in a README. It is not the command line DEC-02 forbids; referencing it from a README or a package would make it one (DEC-44). |
| A VAAPI driver that only offers CQP is driven with `-rc_mode CQP -qp N` and no `-b:v`; the bitrate field then has no effect on it | Measured on a Tiger Lake: such a driver rejects every bitrate. It is detected by the probe's own error text (`supported modes: CQP)`), only for `h264_vaapi` and `hevc_vaapi`. QSV is not concerned (its failure is missing HuC firmware). Do not "fix" it back to a refusal (DEC-48). |
| The probe's failure summary skips `libva info:` lines | libva prints three information lines before any VAAPI or QSV session; kept, they pushed the real error out of the Diagnostic. A `libva error:` line stays (DEC-48, amended 2026-10-20). |
| On `PATH` under Windows, only `.com` and `.exe` count as `ffmpeg`/`ffprobe`; `.bat` and `.cmd` are refused | A batch file runs through `cmd.exe`, and cancelling kills `cmd.exe` but leaves ffmpeg running. Only the test doubles are batch files (`ToolResolver::with_batch_stand_ins`); a user with a wrapper points `SUPERVIEW_FFMPEG_DIR` at a real `ffmpeg.exe` (DEC-45). |
| On Linux, the file pickers treat a `None` returned in under 300 ms as "no picker could open" and fall back to zenity, kdialog, then a text field; on Windows there is no fallback, rfd's native box alone, and `None` is a cancel | rfd cannot tell a cancellation from a missing portal, and a human does not cancel in 300 ms. The clock is injected; never add a `sleep` to a test (DEC-43, phase 6.4). |
| The Diagnostic shows `[OK]` / `[FAIL]` in its text form, and an input or output path that is not UTF-8 is refused | Slint's software renderer draws no colour emoji; `check_video` takes a `&str`. Both are accepted costs, not oversights (DEC-43). |
| The source is read with `-f mov` by ffprobe and ffmpeg, so `encode_args.json` (frozen) is compared with `-f mov` added before its first `-i`, not as recorded; a non-MP4 file is refused whatever its extension | A text file named `.mp4` was read by ffmpeg as a concat script and led to other local files. The gap with the Go oracle is written in `with_forced_demuxer`, not hidden in a regenerated fixture (DEC-52). |
| A duration of `inf` or `nan` is refused although Go's `<= 0` let it through | `inf` demanded 8 TB of free space and `NaN` zeroed the disk check (DEC-53). |
| Preferences live in a new `preferences.json`; nothing is read from the old Fyne preferences | Their format was never a contract, and Fyne is gone (DEC-46). |
| The YAML is read with `serde-saphyr`, not `serde_yaml_ng`; removed keys are not migrated and `SUPERVIEW_*` variables other than `SUPERVIEW_FFMPEG_DIR` are not read | `serde_yaml_ng` rests on `unsafe-libyaml` (RUSTSEC-2024-0449). The migration and the variables belonged to the Go code (DEC-43). |
| The coverage floor is 50 % of lines although the measured figure is about 95 % | It is a floor against collapse, raised when someone decides to, not when a number allows it (DEC-47). |

## Contracts you must not break

- **Configuration is passed explicitly.** `check_ffmpeg(cfg, resolver)`,
  `EncodingSession::new(cfg)`, `perform_encoding(cfg, …)`. There is no config
  global; do not reintroduce one. The same goes for the tool resolver, the
  prober and the injected clocks, environments and memory readers: a global
  makes a test depend on the machine.
- **Session lifecycle**: `EncodingSession` (`superview-core/src/session.rs`) owns
  an isolated temporary directory and removes it on `Drop` (`close()` reports a
  failure). Temporary files live there — never the working directory, never a
  hardcoded path.
- **Any widget update from another thread goes through
  `upgrade_in_event_loop`** (Slint). The conversion runs on its own thread; the
  UI must stay responsive, and a panic on a worker thread must become a displayed
  error, never a locked window.
- **Bitrates are bits per second**, everywhere, including logs and comments.
  They were documented as bytes for a long time, off by a factor of eight.
- **`std::process::Command` with arguments, never a shell string**, for ffmpeg
  and ffprobe. Paths from file pickers are untrusted: validate before use
  (`security.rs`).
- **Cancellation must leave nothing behind** — no orphan ffmpeg process, no
  partial output file, no temp directory. Tests pin all three: the pipeline tests
  in `superview-core` check the files and the directory, and `superview-harness`'s
  signal tests (Unix only) check them on a real signal.
- **Check every error return.** Domain errors are typed in
  `superview-core/src/error.rs`: `InvalidVideoError`, `EncoderError`,
  `SessionError`, and the `Error` variants `Cancelled`, `StoppedBySignal` and
  `FailedAfterOutput` that the fallback cascade reads. A cancellation is never a
  string.

## Changing the code

- **Build the workspace through cargo**: `cargo build -p superview-app --locked`
  (`make build` for the release binary). The window needs no system GUI headers beyond
  `libfontconfig1-dev` (`docs/ENVIRONNEMENT.md`).
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
- After changing a public signature, sweep the prose:
  `grep -rn "function_name" --include='*.md' .` — Markdown is not compiled, and a
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
verification recipe, a hook that runs `scripts/check.sh --fast` (format and clippy) after every
code edit, one that runs the full check whenever Claude stops with non-Markdown changes pending, a guard against dangerous
commands, a session-start hook that prints the head of `docs/SUITE.md`, and `/verify`, which runs
the recipe above and reports each gate instead of summarising. Other agent tools
ignore the directory; nothing in it is an instruction.

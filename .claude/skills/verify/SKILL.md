---
name: verify
description: Run the AGENTS.md verification recipe for Superview, gate by gate, and report each result instead of a summary. Use before claiming any change is verified.
---

`scripts/check.sh` runs gates 1-5 in one command (it is also what the git
pre-commit hook and the Claude hooks run), but stops at the first failure and
says nothing about skips: this skill stays the gate-by-gate report.

Run the gates below **in order**, from the repository root, with
`export PATH=$HOME/.local/go/bin:$HOME/go/bin:$PATH` if `go` is not found. Stop
at the first gate that cannot run and say so: a gate that did not run is not a
gate that passed.

1. **GUI headers.** `go build ./cmd/superview 2>&1 | head -3`. If it fails on a GL, X11 or
   Wayland header, the `cmd/superview` package cannot be verified on this machine
   (`docs/ENVIRONNEMENT.md`). Report that and continue with `./internal/common` only,
   saying explicitly that the GUI package was **not** verified.
2. **Formatting.** `gofmt -l .` must print nothing.
3. **Build and vet.** `go build ./... && go vet ./...`.
4. **Tests, with ffmpeg required.**
   `SUPERVIEW_REQUIRE_FFMPEG=1 go test -race ./... -count=1 2>&1 | tee /tmp/sv-test.log | tail -40`.
   Then `grep -c -- '--- SKIP' /tmp/sv-test.log` and list every skipped test by
   name: a skip in `internal/common/integration_test.go` or in
   `TestGeneratePGM_RemapOutputIsStable` means no real conversion was checked.
   One known red on machines with an Intel iGPU:
   `TestIntegration_DeviceFrameEncoderRunsTheWholePipeline` (E-01, VAAPI/CQP).
   Report it as the known defect, not as noise, and not as a pass.
5. **Lint.** `golangci-lint run ./... --timeout=5m` must report 0 issues.
   Never run staticcheck or another linter beside it (DEC-21).
6. **Counter-proof.** For every test added or changed in this change:
   reintroduce the defect it guards, run that test alone, confirm it reddens,
   revert. Name each test and whether its counter-proof reddened. A test whose
   counter-proof stayed green is reported as **unproven**, not as passing.

Report the six gates as a table: gate, command, result (pass / fail / did not
run / unproven), and the one line of output that matters. Never fold a skipped
or unrun gate into "all green".

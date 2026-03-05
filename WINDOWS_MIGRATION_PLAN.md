# Windows-Only Migration Plan (Safe, Step-by-Step)

Goal: simplify the project to focus on Windows while avoiding regressions.

## Baseline (done)
- Branch: `windows-focus`
- Safety tag: `v-next-before-windows-focus`
- Baseline commit: `de443c7`

## Guardrails (must stay green in every step)
- `go build superview-cli.go`
- `go test ./common`
- `go build -ldflags="-H=windowsgui" -o superview-gui.exe superview-gui.go`
- Manual GUI smoke test on Windows (open input, choose output, start encode)

## PR1 - Documentation and Build Policy (No Behavior Change)
Scope:
- Update README to state Windows as the official target.
- Keep Linux/macOS notes only as legacy information (non-supported).
- Document one official local workflow for Windows.

Acceptance:
- No code behavior changes.
- Guardrails pass.

## PR2 - GUI OS Simplification (Windows path only)
Scope:
- Remove Linux dialog branches from GUI file dialog selection.
- Keep one Windows-native flow only.
- Keep fallback to Fyne dialog only if native dialog fails.

Acceptance:
- No black PowerShell popup.
- Input/output selection works.
- Guardrails pass.

## PR3 - Platform Layer Cleanup
Scope:
- Move GUI native dialog helpers to a Windows-focused helper section/package.
- Reduce runtime OS branching in GUI entrypoint.
- Keep `common/` encoding pipeline unchanged.

Acceptance:
- Same end-user behavior on Windows.
- Guardrails pass.

## PR4 - Repository Cleanup (final)
Scope:
- Remove unused non-Windows scripts/docs where appropriate.
- Simplify release/build docs to Windows-first process.
- Keep rollback path via safety tag.

Acceptance:
- Project structure is easier to maintain.
- Guardrails pass.

## Rollback
At any point:
- `git checkout master`
- `git reset --hard v-next-before-windows-focus` (only if explicitly desired)

## Notes
- Do not change core encoding logic in `common/` during PR1/PR2.
- Prefer small commits with one intent each.

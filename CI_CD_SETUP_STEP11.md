# CI/CD Setup Step 11 (GUI-Only)

## Objective
Establish a Windows-first CI/CD pipeline for the GUI application only.

## Implemented Scope
- Run tests on `common` package with coverage.
- Enforce coverage gate (30%).
- Build Windows GUI artifact on native runner.
- Verify GUI artifact output exists.

## Current Workflow Mapping
- Test workflow: `.github/workflows/test.yml`
- Release workflow: `.github/workflows/release.yml`

## Local Validation Commands
```powershell
go test ./common
go build -ldflags="-H=windowsgui" -o superview-gui.exe superview-gui.go
```

## Notes
- CI pipeline is now GUI-only.
- Cross-compilation and distribution for removed command-line components are no longer part of Step 11.
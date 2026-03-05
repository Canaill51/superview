# Distribution Step 12 (GUI-Only)

## Objective
Publish Windows GUI release artifacts with reproducible CI flow.

## Final Distribution Model
- Build target: `superview-gui.go`
- Platform target: Windows native runner
- Release assets: GUI ZIP archives + checksums

## CI Release Jobs
- `test`: run `go test ./common` and coverage gate.
- `build-gui-windows`: build and archive `superview-gui`.
- `create-release`: aggregate GUI artifacts and publish draft release.

## Local Verification
```powershell
go test ./common
go build -ldflags="-H=windowsgui" -o superview-gui.exe superview-gui.go
```

## Result
Step 12 is now fully coherent with the GUI-only product scope.
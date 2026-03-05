# Project Guidelines

## Code Style
- Language: Go (`go.mod` uses module `superview`, Go 1.22).
- Keep changes minimal and consistent with existing straightforward style in `superview-gui.go` and `common/common.go`.
- Prefer explicit error returns and proper error handling: check ALL error returns.
- Use custom error types (`InvalidVideoError`, `EncoderError`, `SessionError`) for domain-specific errors.
- Preserve explicit error returns and `dialog.ShowError` patterns used by GUI entrypoints.
- Preserve current package split: entrypoints in root, shared encoding logic in `common/`.
- Keep user-facing strings stable unless the task explicitly requests UX changes.

## Architecture
- One binary:
  - `superview-gui.go`: desktop UI built with Fyne.
- Shared types in `common/common.go`:
  - `VideoSpecs`: contains video metadata with validation method.
  - `VideoStream`: named type for individual stream data (replaces anonymous struct).
  - Error types: `InvalidVideoError`, `EncoderError`, `SessionError` for better error handling.
- Shared video pipeline lives in `common/common.go`:
  - `EncodingSession`: manages secure temporary files in isolated directory per session (not in working dir).
  - `InitEncodingSession()` / `CloseEncodingSession()`: lifecycle management for temp files.
  - `CheckFfmpeg` discovers ffmpeg version/encoders/accels.
  - `CheckVideo` reads stream metadata via `ffprobe` with full error handling.
  - `GeneratePGM` creates remap maps in session's temp directory.
  - `EncodeVideo` runs ffmpeg with remap filtering and reports progress.
  - `ValidateBitrate()`: validates bitrate is in acceptable range (100k-50M bytes/sec).
  - `FindEncoder()`: selects encoder with error validation.
  - `CleanUp` removes session's entire temp directory.
- OS-specific process behavior is isolated in:
  - `common/command-other.go`
  - `common/command-windows.go`

## Build and Test
- Build GUI: `go build superview-gui.go`
- Run tests (if present): `go test ./common`
- Preferred verification order for small changes: build touched binary first, then `go test ./common`.
- Repository has one GUI entrypoint at root (`superview-gui.go`).
- Keep using `go test ./common` for routine validation.
- CI coverage gate: minimum 30% in `.github/workflows/test.yml` and `.github/workflows/release.yml`.
- GUI builds are most reliable on native OS runners; cross-compiling Fyne GUI binaries (especially for macOS) may fail locally.
- Cross-build/release script: `./build.sh <version>`
  - Requires `fyne-cross`.
  - Creates git tags and pushes tags at the end; do not run automatically unless release intent is explicit.
  - May trigger signing/release tooling (`codesign`, `hub`) when available.

## Project Conventions
- FFmpeg/FFprobe are required runtime dependencies; failures should keep current user-facing error style.
- Preserve encoder selection behavior:
  - Check validated encoders via `FindEncoder()` (now returns error).
  - Default to input codec unless user selects/sets a supported encoder.
- Validation patterns required for all major operations:
  - `VideoSpecs.Validate()`: checks stream data completeness before encoding.
  - `ValidateBitrate()`: ensures bitrate is in acceptable range (min 100k, max 50M bytes/sec).
  - Always check error returns from `FindEncoder()`.
- Temporary remap files lifecycle: initialize session → generate files → encode → cleanup in isolated temp directory.
  - Always call `InitEncodingSession()` before encoding and `defer common.CleanUp()` for guaranteed cleanup.
  - Never hardcode temp file paths; use session management functions.
- GUI behavior should stay responsive: long encode work runs in goroutine (see `superview-gui.go`).
- Keep encode progress callback behavior intact (`EncodeVideo` callback drives GUI progress bar).

## Integration Points
- External tools: `ffmpeg`, `ffprobe` executed through `os/exec`.
- GUI toolkit: `fyne.io/fyne`.
- Cross-platform GUI packaging via `fyne-cross` in `build.sh`.
- Platform-specific process setup: `prepareBackgroundCommand` in `common/command-*.go`.

## Security
- Treat file paths from GUI file pickers as untrusted input; validate before processing.
- Validate all user input before processing:
  - Video metadata via `VideoSpecs.Validate()`.
  - Bitrate ranges via `ValidateBitrate()` (100k-50M bytes/sec).
  - Encoder selection via `FindEncoder()` which checks availability.
- Temporary files are managed in isolated directories via `EncodingSession` (not in working directory).
  - Use `InitEncodingSession()` / `CloseEncodingSession()` for safe session lifecycle.
  - Never create temp files directly in working directory or hardcode paths.
- Avoid introducing shell interpolation for ffmpeg calls; keep `exec.Command` argument-based invocation.
- Do not hardcode secrets/cert identities in new code; `build.sh` contains release-signing-specific behavior that should remain opt-in.
- Preserve Ctrl+C termination behavior in encoding (`common.EncodeVideo` signal handling) when touching process logic.
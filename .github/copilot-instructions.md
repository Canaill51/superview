# Project Guidelines

## Code Style
- Language: Go (`go.mod` uses module `superview`, Go 1.26.0).
- Keep changes minimal and consistent with existing straightforward style in `gui_main.go` and `common/common.go`.
- Prefer explicit error returns and proper error handling: check ALL error returns.
- Use custom error types (`InvalidVideoError`, `EncoderError`, `SessionError`) for domain-specific errors.
- Preserve explicit error returns and `dialog.ShowError` patterns used by GUI entrypoints.
- Preserve current package split: entrypoints in root, shared encoding logic in `common/`.
- Keep user-facing strings stable unless the task explicitly requests UX changes.

## Architecture
- One binary:
  - `gui_main.go`: desktop UI built with Fyne.
  - `gui_native_dialog_linux.go` / `gui_native_dialog_windows.go`: native file dialogs.
- Shared logic in `common/` is split by responsibility:
  - `common/common.go`: encoding workflow, session lifecycle, shared exported types.
  - `common/config.go`: configuration loading and defaults.
  - `common/gui_helpers.go`: GUI helper functions.
  - `common/hardware.go`: encoder capability profiling.
  - `common/health.go`: system health checks.
  - `common/metrics.go`: encoding metrics.
  - `common/observability.go`: event recording and logging hooks.
  - `common/security.go`: path and input validation helpers.
  - `common/command-*.go`: OS-specific process setup.
- Shared types are primarily defined in `common/common.go`:
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
  - `ValidateBitrate()`: validates bitrate against `Config.MinBitrate`/`MaxBitrate` (defaults 100k-200M bytes/sec).
  - `FindEncoder()`: selects encoder with error validation.
  - `CleanUp` removes session's entire temp directory.
- OS-specific process behavior is isolated in `common/command-other.go` and `common/command-windows.go`.

## Build and Test
- Build GUI: `go build .` (build the package, never a single file: the native dialog files are build-tagged).
- Run tests (if present): `go test ./common`
- Preferred verification order for small changes: build touched binary first, then `go test ./common`.
- Repository has one GUI entrypoint at root (`gui_main.go`).
- Keep using `go test ./common` for routine validation.
- CI coverage gate: minimum 30% in `.github/workflows/test.yml` and `.github/workflows/release.yml`.
- GUI builds are most reliable on native OS runners; cross-compiling Fyne GUI binaries (especially for macOS) may fail locally.
- Cross-build/release script: `./build.sh <version>` — DEPRECATED, superseded by `release.yml`.
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
  - `ValidateBitrate()`: ensures bitrate is within the configured range (defaults: min 100k, max 200M bytes/sec).
  - Always check error returns from `FindEncoder()`.
- Temporary remap files lifecycle: initialize session → generate files → encode → cleanup in isolated temp directory.
  - Always call `InitEncodingSession()` before encoding and `defer common.CleanUp()` for guaranteed cleanup.
  - Never hardcode temp file paths; use session management functions.
- GUI behavior should stay responsive: long encode work runs in a goroutine (see `gui_main.go`).
- Any widget update from a goroutine MUST be wrapped in `fyne.Do(...)`.
- Keep encode progress callback behavior intact (`EncodeVideo` callback drives GUI progress bar).

## Integration Points
- External tools: `ffmpeg`, `ffprobe` executed through `os/exec`.
- GUI toolkit: `fyne.io/fyne`.
- Release packaging is done by `.github/workflows/release.yml` (native runners). The legacy `build.sh` is deprecated.
- Platform-specific process setup: `prepareBackgroundCommand` in `common/command-*.go`.

## Security
- Treat file paths from GUI file pickers as untrusted input; validate before processing.
- Validate all user input before processing:
  - Video metadata via `VideoSpecs.Validate()`.
  - Bitrate ranges via `ValidateBitrate()` (configured range, defaults 100k-200M bytes/sec).
  - Encoder selection via `FindEncoder()` which checks availability.
- Temporary files are managed in isolated directories via `EncodingSession` (not in working directory).
  - Use `InitEncodingSession()` / `CloseEncodingSession()` for safe session lifecycle.
  - Never create temp files directly in working directory or hardcode paths.
- Avoid introducing shell interpolation for ffmpeg calls; keep `exec.Command` argument-based invocation.
- Do not hardcode secrets/cert identities in new code.
- Preserve Ctrl+C termination behavior in encoding (`common.EncodeVideo` signal handling) when touching process logic.
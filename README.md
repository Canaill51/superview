# Superview
<!-- ALL-CONTRIBUTORS-BADGE:START - Do not remove or modify this section -->
[![All Contributors](https://img.shields.io/badge/all_contributors-4-orange.svg?style=flat-square)](#contributors-)
<!-- ALL-CONTRIBUTORS-BADGE:END -->

Transform 4:3 aspect ratio videos to 16:9 using intelligent dynamic scaling, inspired by the GoPro SuperView method. This Go program smoothly stretches outer areas while preserving the center, creating a natural-looking widescreen conversion.

> Official target platform: **Windows**.
> Superview is now distributed and maintained as a **GUI-only** application.

## Quick Links

- [Overview](#overview)
- [Requirements](#requirements)
- [Installation](#installation)
- [Usage (GUI)](#usage)
- [Configuration](#configuration)
- [Architecture](#architecture)
- [API Documentation](#api-documentation)
- [Development](#development)

## Overview

This program applies sophisticated distortion to convert 4:3 video to 16:9 widescreen:

- **Dynamic Scaling**: Outer areas stretched more aggressively, center maintains aspect ratio
- **Hardware Acceleration**: Supports available H.264/H.265 encoders and GPU acceleration
- **Flexible Configuration**: Customizable bitrate constraints and encoder selection
- **Simplified GUI Flow**: 3-step guided workflow with native file dialogs on Windows

The algorithm is based on [Banelle's original Python implementation](https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview), adapted for Go and FFmpeg.

Here is a quick animation showing the scaling, note how the text in the center stays the same:

![Sample of scaling result](.github/sample.gif)

## Requirements

### Official (Windows)

Use the commands below.

```powershell
winget install -e --id Gyan.FFmpeg --accept-package-agreements --accept-source-agreements
winget install -e --id GoLang.Go --accept-package-agreements --accept-source-agreements
winget install -e --id BrechtSanders.WinLibs.POSIX.UCRT --accept-package-agreements --accept-source-agreements

ffmpeg -version
ffprobe -version
go version
gcc --version
```

If a command is not found after install, close and reopen your terminal so `PATH` is refreshed.

## Installation

### Option 1: Use prebuilt binaries (recommended for final users)

1. Download the Windows archive from [Releases](https://github.com/Canaill51/superview/releases).
2. Extract it.
3. Run `superview-gui.exe`.

Windows (PowerShell):
```powershell
.\superview-gui.exe
```

### Option 2: Build from source

Official local build flow (Windows GUI):

```powershell
go build -ldflags="-H=windowsgui" -o superview-gui.exe superview-gui.go
```

Then launch:

```powershell
.\superview-gui.exe
```

## Usage

### Quick Run

Windows (PowerShell):
```powershell
.\superview-gui.exe
```

GUI workflow:
1. Click **1) Choose input file**
2. (Optional) Select **Output codec**
3. Click **2) Choose output file**
4. Click **3) Start Superview transform**
5. Wait for encoding completion

Notes:
- GUI bitrate is fixed from configuration (`max_bitrate`), there is no manual bitrate field.

![GUI Screenshot](.github/sample-gui.png)

If you get `Cannot find ffmpeg/ffprobe`, fix your `PATH` and retry.

### Configuration

Edit `superview.yaml` to customize:

```yaml
min_bitrate: 102400       # ~0.1 Mbps minimum
max_bitrate: 52428800     # ~50 Mbps maximum
temp_dir_prefix: "superview-*"
encoder_codecs: ["264", "265", "hevc"]
log_level: info
```

Override with environment variables:

```bash
export SUPERVIEW_MIN_BITRATE=262144
export SUPERVIEW_MAX_BITRATE=20971520
export SUPERVIEW_LOG_LEVEL=debug
./superview-gui
```

## Architecture

### Project Structure

```
superview/
├── common/
│   ├── common.go          # Core encoding pipeline
│   ├── common_test.go     # Unit tests
│   ├── config.go          # Configuration management
│   ├── config_test.go     # Config tests
│   ├── hardware.go         # Hardware capability profiling
│   ├── observability.go    # Observability hooks
│   ├── metrics.go          # Runtime metrics
│   ├── health.go           # Health checks
│   ├── security.go         # Security helpers
│   └── command-*.go       # OS-specific process setup
├── superview-gui.go       # GUI entry point (Fyne)
└── superview.yaml         # Default configuration
```

### Encoding Pipeline

```
Input → CheckFfmpeg → CheckVideo → PerformEncoding → CleanUp → Output
                                         ↓
                               GetBitrate + ValidateBitrate
                               GetEncoder + FindEncoder
                               InitEncodingSession
                               GeneratePGM (create remap filters)
                               EncodeVideo (ffmpeg with progress)
```

## API Documentation

Public API in `common` package:

```go
// Configuration
GetConfig() *Config
SetConfig(cfg *Config)
LoadConfig(filepath string) (*Config, error)
CreateDefaultConfig(filepath string) error

// Logging
SetLogger(l *slog.Logger)
GetLogger() *slog.Logger

// Encoding Workflow
CheckFfmpeg() (map[string]string, error)
CheckVideo(file string) (*VideoSpecs, error)
PerformEncoding(inputFile, outputFile string, ui UIHandler, 
                ffmpeg map[string]string) error
```

Implement the `UIHandler` interface for custom UIs:

```go
type UIHandler interface {
    ShowError(error)
    ShowInfo(msg string)
    ShowProgress(percent float64)
    GetBitrate() (int, error)
    GetEncoder() string
    GetSqueeze() bool
}
```

### Example: Custom Handler

```go
type MyHandler struct{}

func (h *MyHandler) ShowError(err error) { log.Printf("ERROR: %v\n", err) }
func (h *MyHandler) ShowInfo(msg string) { fmt.Println("INFO:", msg) }
func (h *MyHandler) ShowProgress(percent float64) { fmt.Printf("%.1f%%\r", percent) }
func (h *MyHandler) GetBitrate() (int, error) { return 5242880, nil }
func (h *MyHandler) GetEncoder() string { return "libx265" }
func (h *MyHandler) GetSqueeze() bool { return false }

// Use it
ffmpeg, _ := common.CheckFfmpeg()
common.PerformEncoding("input.mp4", "output.mp4", &MyHandler{}, ffmpeg)
```

## Development

### Build & Test

```powershell
# Run tests with coverage
go test ./common -cover

# Run package tests
go test ./common

# Build GUI binary
go build -ldflags="-H=windowsgui" -o superview-gui.exe superview-gui.go
```

### Recent Improvements

- **Étape 1**: Go 1.22+, dependency updates
- **Étape 2**: Secure temp file handling
- **Étape 3**: Custom error types, validation
- **Étape 4**: UIHandler interface, reduced duplication
- **Étape 5**: 32 comprehensive unit tests
- **Étape 6**: Structured logging with slog
- **Étape 7**: External configuration (YAML + env vars)
- **Étape 8**: Full documentation (Godoc + this README)

## Contributors ✨

Thanks goes to these wonderful people ([emoji key](https://allcontributors.org/docs/en/emoji-key)):

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tr>
    <td align="center"><a href="https://github.com/naorunaoru"><img src="https://avatars0.githubusercontent.com/u/3761149?v=4" width="100px;" alt=""/><br /><sub><b>Roman Kuraev</b></sub></a><br /><a href="#ideas-naorunaoru" title="Ideas, Planning, & Feedback">🤔</a> <a href="https://github.com/Niek/superview/commits?author=naorunaoru" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/dangr0"><img src="https://avatars1.githubusercontent.com/u/61669715?v=4" width="100px;" alt=""/><br /><sub><b>dangr0</b></sub></a><br /><a href="https://github.com/Niek/superview/issues?q=author%3Adangr0" title="Bug reports">🐛</a></td>
    <td align="center"><a href="https://github.com/dga711"><img src="https://avatars1.githubusercontent.com/u/2995606?v=4" width="100px;" alt=""/><br /><sub><b>DG</b></sub></a><br /><a href="#ideas-dga711" title="Ideas, Planning, & Feedback">🤔</a> <a href="https://github.com/Niek/superview/commits?author=dga711" title="Tests">⚠️</a></td>
    <td align="center"><a href="https://github.com/tommaier123"><img src="https://avatars2.githubusercontent.com/u/40432491?v=4" width="100px;" alt=""/><br /><sub><b>Nova_Max</b></sub></a><br /><a href="https://github.com/Niek/superview/commits?author=tommaier123" title="Documentation">📖</a></td>
  </tr>
</table>

<!-- markdownlint-enable -->
<!-- prettier-ignore-end -->
<!-- ALL-CONTRIBUTORS-LIST:END -->

This project follows the [all-contributors](https://github.com/all-contributors/all-contributors) specification. Contributions of any kind welcome!
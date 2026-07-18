# Superview
<!-- ALL-CONTRIBUTORS-BADGE:START - Do not remove or modify this section -->
[![All Contributors](https://img.shields.io/badge/all_contributors-4-orange.svg?style=flat-square)](#contributors-)
<!-- ALL-CONTRIBUTORS-BADGE:END -->

Transform 4:3 aspect ratio videos to 16:9 using intelligent dynamic scaling, inspired by the GoPro SuperView method. This Go program smoothly stretches outer areas while preserving the center, creating a natural-looking widescreen conversion.

> Official target platform: **Windows**.
> Superview is distributed and maintained as a **GUI-only** application.
> The current codebase targets **Go 1.25+**.

## Quick Links

- [Overview](#overview)
- [Requirements](#requirements)
- [Hardware Compatibility](#hardware-compatibility)
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

Superview now shows a hardware diagnostic in the GUI:

- the planned path before launch, for example `h264_nvenc + D3D11VA`
- the actual path used after the run, including CPU fallback when FFmpeg or the driver rejects a hardware path

The algorithm is based on [Banelle's original Python implementation](https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview), adapted for Go and FFmpeg.

Here is a quick animation showing the scaling, note how the text in the center stays the same:

![Sample of scaling result](.github/sample.gif)

## Requirements

### Official (Windows)

Use the commands below.

```powershell
winget install -e --id Gyan.FFmpeg --accept-package-agreements --accept-source-agreements

ffmpeg -version
ffprobe -version

```

If a command is not found after install, close and reopen your terminal so `PATH` is refreshed.

## Hardware Compatibility

Superview does not hardcode a fixed GPU whitelist at runtime. It relies on the hardware capabilities actually exposed by your installed FFmpeg build and graphics driver.

For H.264 and H.265 hardware acceleration, the practical prerequisites are:

- the GPU must provide hardware encode and decode support for the codec you want to use
- the installed driver must expose that support correctly
- the installed FFmpeg build must include the relevant hardware encoder (`nvenc`, `amf`, or `qsv`)

On Windows, Superview can now combine these hardware encoders with `D3D11VA` or `DXVA2` decode when the vendor-specific `hwaccel` token is not exposed by FFmpeg.

### Nvidia

Recommended minimum for reliable H.264 + H.265 encode/decode: `Pascal` and newer.

Commonly compatible families:

- GeForce GTX `10xx`
- GeForce RTX `20xx`, `30xx`, `40xx`, `50xx`
- Quadro `Pxxxx`
- RTX professional `Axxxx`
- Tesla / data center `P4`, `P40`, `T4`, `A2`, `A10`, `A16`, `L4`, `L40`

Notes:

- Some low-end exceptions exist even inside newer generations, so FFmpeg detection remains authoritative.
- Older `Maxwell 2` cards may support parts of HEVC, but are not the baseline recommended here.

### AMD

Recommended minimum for reliable H.264 + H.265 encode/decode: `VCN 2.0` and newer.

Commonly compatible families:

- Radeon RX `5300`, `5500`, `5600`, `5700`
- Radeon RX `6600`, `6700`, `6800`, `6900`
- Radeon RX `7600`, `7700`, `7800`, `7900`
- Radeon RX `9060`, `9070`
- Radeon Pro `W5xxx`, `W6xxx`, `W7xxx` families with matching VCN support
- Ryzen APUs with supported media engines, typically Ryzen `5000` and newer integrated graphics platforms

Important exclusions:

- some `Navi24` products are decode-only for this use case, including RX `6300`, `6400`, `6500`
- some Radeon Pro variants built on the same media block are also decode-only

### Intel

Minimum practical baseline for H.264 + H.265 hardware encode/decode: `Skylake`.

Recommended baseline to reduce edge cases: `Kaby Lake` and newer.

Commonly compatible families:

- Intel Core `6th gen` and newer with active Quick Sync support
- Intel Core `7th`, `8th`, `9th`, `10th`, `11th`, `12th`, `13th`, `14th gen`
- Intel `Core Ultra`
- Intel `Arc`

Important limitations:

- CPUs without an active iGPU or without Quick Sync available are not compatible
- some `F`, `KF`, Xeon, and workstation variants may not expose usable Quick Sync
- BIOS settings can disable the iGPU and therefore remove hardware video support entirely

### What Superview Actually Supports

Superview currently targets the following FFmpeg hardware encoders for H.264 and H.265:

- Nvidia: `h264_nvenc`, `hevc_nvenc`
- AMD: `h264_amf`, `hevc_amf`
- Intel: `h264_qsv`, `hevc_qsv`
- CPU fallback: `libx264`, `libx265`

If your GPU is theoretically compatible but the encoder does not appear in the GUI, the issue is usually one of these:

- FFmpeg was installed without the relevant hardware encoder enabled
- the graphics driver is missing, outdated, or vendor-generic in a way that hides the video engine
- the machine exposes decode support but not encode support for that specific card
- the codec is supported on paper by the family, but not by that exact SKU

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
2. Select **Quality** (**Fast** or **Balanced**)
3. (Optional) Select **Video codec**
4. Click **2) Choose output file**
5. Click **3) Start Superview transform**
6. Wait for encoding completion

Notes:
- GUI quality is profile-driven (GPU-friendly bitrate + preset strategy).
- `Fast`: faster encode, smaller output.
- `Balanced`: best visual quality.
- The app asks for confirmation before overwriting an existing output file.
- The GUI shows the planned hardware path before launch and the actual path used after encoding completes.

![GUI Screenshot](.github/sample-gui.png)

If you get `Cannot find ffmpeg/ffprobe`, fix your `PATH` and retry.

### Configuration

Edit `superview.yaml` to customize:

```yaml
min_bitrate: 102400       # ~0.1 Mbps minimum
max_bitrate: 209715200    # ~200 Mbps maximum
quality_preset: balanced  # balanced | fast
temp_dir_prefix: "superview-*"
encoder_codecs: ["264", "265", "hevc"]
log_level: info
performance_mode: safe_performance    # safe | safe_performance
video_preset: ""         # optional: ultrafast..veryslow (empty = ffmpeg default)
filter_threads: 0         # 0 = auto/default
encoder_threads: 0        # 0 = auto/default
```

Override with environment variables:

```bash
export SUPERVIEW_MIN_BITRATE=262144
export SUPERVIEW_MAX_BITRATE=209715200
export SUPERVIEW_QUALITY_PRESET=balanced
export SUPERVIEW_LOG_LEVEL=debug
export SUPERVIEW_PERFORMANCE_MODE=safe_performance
export SUPERVIEW_VIDEO_PRESET=fast
export SUPERVIEW_FILTER_THREADS=4
export SUPERVIEW_ENCODER_THREADS=8
./superview-gui
```

## Architecture

### Project Structure

```
superview/
├── common/
│   ├── common.go           # Encoding pipeline, session lifecycle, exported workflow
│   ├── config.go           # Configuration loading and defaults
│   ├── gui_helpers.go      # GUI-specific helpers shared with tests
│   ├── hardware.go         # Hardware capability profiling
│   ├── health.go           # System health checks
│   ├── metrics.go          # Encoding metrics collection
│   ├── observability.go    # Event recording and logging hooks
│   ├── security.go         # Path and input validation helpers
│   ├── command-*.go        # OS-specific process setup
│   └── *_test.go           # Unit tests for the common package
├── superview-gui.go        # GUI entry point (Fyne)
├── superview.yaml          # Default configuration
├── build.sh                # Release / cross-build helper
└── FyneApp.toml            # Fyne packaging metadata
```

### Encoding Pipeline

```
Input → CheckFfmpeg → CheckVideo → InitEncodingSession → GeneratePGM → EncodeVideo → CleanUp → Output
                                              ↓
                               ValidateBitrate + FindEncoder
                               VideoSpecs.Validate()
                               EncodingMetrics / Observability hooks
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
                ffmpeg map[string]string, cancel <-chan struct{}) error
InitEncodingSession() error
CleanUp() error
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
cancel := make(chan struct{})
common.PerformEncoding("input.mp4", "output.mp4", &MyHandler{}, ffmpeg, cancel)
```

## Development

Use the commands below.

```powershell
winget install -e --id Gyan.FFmpeg --accept-package-agreements --accept-source-agreements
winget install -e --id GoLang.Go --accept-package-agreements --accept-source-agreements
winget install -e --id BrechtSanders.WinLibs.POSIX.UCRT --accept-package-agreements --accept-source-agreements
```

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

- **Étape 1**: Go 1.25+ and dependency refresh
- **Étape 2**: Secure temp file handling
- **Étape 3**: Custom error types and validation
- **Étape 4**: UIHandler interface and shared GUI helpers
- **Étape 5**: Expanded unit test coverage
- **Étape 6**: Structured logging with `slog`
- **Étape 7**: External configuration (YAML + env vars)
- **Étape 8**: Updated documentation for the current project layout

## Contributors ✨

Thanks goes to these wonderful people ([emoji key](https://allcontributors.org/docs/en/emoji-key)):

<!-- ALL-CONTRIBUTORS-LIST:START - Do not remove or modify this section -->
<!-- prettier-ignore-start -->
<!-- markdownlint-disable -->
<table>
  <tr>
    <td align="center"><a href="https://github.com/naorunaoru"><img src="https://avatars0.githubusercontent.com/u/3761149?v=4" width="100px;" alt=""/><br /><sub><b>Roman Kuraev</b></sub></a><br /><a href="#ideas-naorunaoru" title="Ideas, Planning, & Feedback">🤔</a> <a href="https://github.com/Canaill51/superview/commits?author=naorunaoru" title="Code">💻</a></td>
    <td align="center"><a href="https://github.com/dangr0"><img src="https://avatars1.githubusercontent.com/u/61669715?v=4" width="100px;" alt=""/><br /><sub><b>dangr0</b></sub></a><br /><a href="https://github.com/Canaill51/superview/issues?q=author%3Adangr0" title="Bug reports">🐛</a></td>
    <td align="center"><a href="https://github.com/dga711"><img src="https://avatars1.githubusercontent.com/u/2995606?v=4" width="100px;" alt=""/><br /><sub><b>DG</b></sub></a><br /><a href="#ideas-dga711" title="Ideas, Planning, & Feedback">🤔</a> <a href="https://github.com/Canaill51/superview/commits?author=dga711" title="Tests">⚠️</a></td>
    <td align="center"><a href="https://github.com/tommaier123"><img src="https://avatars2.githubusercontent.com/u/40432491?v=4" width="100px;" alt=""/><br /><sub><b>Nova_Max</b></sub></a><br /><a href="https://github.com/Canaill51/superview/commits?author=tommaier123" title="Documentation">📖</a></td>
  </tr>
</table>

<!-- markdownlint-enable -->
<!-- prettier-ignore-end -->
<!-- ALL-CONTRIBUTORS-LIST:END -->

This project follows the [all-contributors](https://github.com/all-contributors/all-contributors) specification. Contributions of any kind welcome!
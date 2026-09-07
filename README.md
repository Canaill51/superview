# Superview

**English** · [Français](README_FR.md)

<!-- ALL-CONTRIBUTORS-BADGE:START - Do not remove or modify this section -->
[![All Contributors](https://img.shields.io/badge/all_contributors-4-orange.svg?style=flat-square)](#contributors-)
<!-- ALL-CONTRIBUTORS-BADGE:END -->

Turn 4:3 footage into 16:9 without black bars and without cropping the picture.
Superview stretches the edges progressively and leaves the centre alone, the way
GoPro's SuperView does.

![Sample of scaling result](.github/sample.gif)

Notice how the text in the centre keeps its shape while the sides widen.

> Officially supported platforms: **Windows** and **Linux** (Ubuntu 24.04 LTS+).
> Superview is distributed and maintained as a **GUI-only** application.

## Download and install

There is nothing to install alongside it. Each archive carries its own `ffmpeg`
and `ffprobe`, and Superview uses those in preference to anything on the
machine — deliberately, because which FFmpeg build is installed decides whether
hardware encoding works at all.

### Windows

1. Open the **[latest release](https://github.com/Canaill51/superview/releases/latest)**.
   Under **Assets**, click the file whose name ends in **`-windows-x86_64.zip`**.
   It is around 95 MB — most of that is FFmpeg — and it lands in your
   `Downloads` folder.

2. Your browser may say the file is not commonly downloaded and offer to discard
   it. Choose **Keep**.
   [Why Windows warns about Superview](#why-windows-warns-about-superview)
   explains what that warning is about.

3. **Extract the zip before doing anything else.** Right-click it →
   **Extract All…** → **Extract**. Double-clicking a zip only shows you what is
   inside; Superview cannot start from that preview, because it needs
   `ffmpeg.exe` and `ffprobe.exe` as real files next to it.

4. Open the folder you just extracted, and keep opening folders until you see
   these four files — Windows puts a folder inside a folder of almost the same
   name here:

   ```
   superview-gui-windows-amd64.exe
   ffmpeg.exe
   ffprobe.exe
   THIRD_PARTY_NOTICES.md
   ```

   If your Windows hides file extensions, the names show up without `.exe`. The
   one you want is `superview-gui-windows-amd64`, the one carrying the Superview
   icon. `ffmpeg` and `ffprobe` are the tools it drives: double-clicking those
   flashes a black window and does nothing.

5. **Double-click `superview-gui-windows-amd64.exe`.**

6. Windows shows a blue window titled **"Windows protected your PC"**. It offers
   only **Don't run**, which is not what you want. Click the small **More info**
   link, then the **Run anyway** button that appears underneath. Superview
   opens. Windows asks this once, not on every launch.

**Keep the four files together.** Superview looks for `ffmpeg.exe` beside
itself. Dragging just the `.exe` onto your desktop leaves it falling back to
whatever FFmpeg the machine has, or reporting `cannot find ffmpeg/ffprobe on
your system`. To put Superview somewhere else, move the whole folder; to launch
it from the desktop, right-click the `.exe` → **Show more options** →
**Send to** → **Desktop (create shortcut)**, which leaves the file where it is.

#### Why Windows warns about Superview

The releases are not signed with a code-signing certificate. Those are rented
yearly from a certificate authority, and this project has none — so Windows has
no publisher name to show you, and SmartScreen has no download history for a
file it is seeing for the first time. The warning says Windows does not know who
made this, not that the file is harmful. Some antivirus products go further and
quarantine the file without asking; the cause is the same missing signature.

What you can check instead: every release ships a `checksums.txt`, the source of
what you are running is this repository, and the archive is assembled by
[`.github/workflows/release.yml`](.github/workflows/release.yml) on GitHub's own
runners — not on anybody's laptop.

### Linux

Download the file whose name ends in `-linux-x86_64.tar.xz` from the
[latest release](https://github.com/Canaill51/superview/releases/latest). It
carries the binary, its FFmpeg, a `.desktop` entry, an icon and a `Makefile`, so
you can either run it in place or install it:

```bash
tar -xJf superview-gui-*-linux-x86_64.tar.xz

./superview/usr/local/bin/superview      # run it where it is
sudo make -C superview install           # or install it into /usr/local
```

Installing puts the application in `/usr/local/bin` and its FFmpeg in
`/usr/local/lib/superview` — not beside the application, where it would shadow
the system's `ffmpeg` for every program on the machine.

### Verify your download

Optional, and worth doing if you want more than the archive's word for itself.
Put `checksums.txt`, published beside the archives, in the same folder:

```bash
sha256sum -c checksums.txt          # Linux
```

On Windows, print the two values and compare them by eye:

```powershell
(Get-FileHash .\superview-gui-*-windows-x86_64.zip -Algorithm SHA256).Hash
Select-String -Path .\checksums.txt -Pattern windows
```

They must show the same 64 characters, ignoring case.

## Using Superview

1. Click **Choose input file** and pick a 4:3 MP4.
2. Select a **Quality profile** — **Fast** or **Balanced**.
3. Optionally select a **Video codec**.
4. Click **Choose output file**.
5. Click **Start transformation**.
6. Wait for the encode to finish.

![GUI Screenshot](.github/sample-gui.png)

**MP4 in, MP4 out.** The file pickers offer MP4 only, and the output extension
is enforced.

**If your camera already stretched the picture**, tick *Source already stretched
to 16:9 (un-squeeze)* — GoPro's SuperView recording modes, the Caddx Tarsier and
similar store a 4:3 capture stretched to 16:9. Superview then un-stretches the
centre instead of widening the frame. The curve is an approximation of the
inverse stretch, not a reproduction of any camera's own algorithm.

Notes:

- Both quality profiles request the same bitrate: 4/3 of the source's, which is
  exactly how much the pixel count grows when a 4:3 frame is widened to 16:9, so
  the output holds the bits per pixel of the source. They differ by encoder
  preset alone. `Fast` is a quicker encode; `Balanced` is a slower preset with
  slightly better detail at the same size.
- The app asks for confirmation before overwriting an existing output file.
- The GUI shows the planned hardware path before launch, for example
  `h264_nvenc + D3D11VA`, and the path actually used once the run finishes.

### If something goes wrong

**Press *Diagnostic*.** It reports ffmpeg/ffprobe availability, free disk space,
memory and CPU, and **which encoders this machine actually accepts**, with
FFmpeg's own words for each refusal. Attach its output to any bug report.

That button is also the answer to "will my GPU be used?" — not
`ffmpeg -encoders | grep nvenc`, which lists what the binary was compiled with
and knows nothing about your driver. See
[docs/hardware-support.md](docs/hardware-support.md) for the GPU families that
generally work and what to check when a card that should be supported does not
appear.

`cannot find ffmpeg/ffprobe on your system`, from a release archive, means its
`ffmpeg` and `ffprobe` are no longer beside the application: extract the archive
again rather than moving the executable out of its folder. From a source build,
put FFmpeg on `PATH`. To make Superview use a particular FFmpeg, point
`SUPERVIEW_FFMPEG_DIR` at the directory holding the two — it wins over the
bundled copy and over `PATH`.

The binary reports its own identity — release number, the commit it was built
from, and whether the tree was modified — in the window title, the first line of
the Diagnostic report and the log at startup. Quote it in any bug report.

## What Superview does

- **Dynamic scaling**: outer areas stretched more aggressively, centre keeps its
  aspect ratio
- **Hardware acceleration**: uses the H.264/H.265 encoders this machine actually
  accepts, and falls back to the CPU otherwise
- **Faithful to the source**: 10-bit footage stays 10-bit when encoding to H.265
  (HERO 10 and later record 10-bit), every audio track is carried over, and the
  recording date is preserved
- **Flexible configuration**: customisable bitrate constraints and encoder
  selection
- **Guided workflow**: three steps, with native file dialogs

The algorithm is based on
[Banelle's original Python implementation](https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview),
adapted for Go and FFmpeg.

> It is an *approximation* of GoPro's SuperView, not a reproduction of it: the
> distortion curve comes from
> [Banelle's original implementation](https://intofpv.com/t-using-free-command-line-sorcery-to-fake-superview)
> and aims for a comparable result, not identical output.

## Hardware acceleration

At startup Superview asks each encoder to encode one frame, and keeps only the
ones that answer. It falls back to `libx264`/`libx265` on the CPU whenever no
hardware path is usable. Asking rather than reading `ffmpeg -encoders` is the
point: that list says what the binary was compiled with and cannot see your
driver.

**The release archives ship their own FFmpeg**, and Superview prefers it over
whatever is installed on the machine. The NVENC driver requirement is fixed when
FFmpeg is compiled, so two builds both calling themselves "8.1.2" can demand
different NVIDIA drivers, and the wrong one costs you hardware encoding with no
symptom but a slow conversion. `SUPERVIEW_FFMPEG_DIR` is the way out — see
[Configuration](#configuration).

Which encoders are targeted, which GPU families work, and why the driver floor
belongs to the FFmpeg build rather than to FFmpeg:
**[docs/hardware-support.md](docs/hardware-support.md)**.

## Configuration

Superview looks for `superview.yaml` in this order, and uses the first file it finds:

1. `$SUPERVIEW_CONFIG` (explicit path, wins over everything)
2. next to the executable
3. `~/.config/superview/superview.yaml` (Linux) or `%AppData%\superview\superview.yaml` (Windows)
4. the current working directory

If none exists, built-in defaults apply. This is the `superview.yaml` shipped with
the project:

```yaml
min_bitrate: 102400       # ~0.1 Mbps minimum
max_bitrate: 209715200    # ~200 Mbps maximum
temp_dir_prefix: "superview-*"
encoder_codecs: ["264", "265", "hevc"]
log_level: info
performance_mode: safe_performance    # safe = re-encode audio to AAC | safe_performance = copy audio
video_preset: ""         # optional: ultrafast..veryslow (empty = ffmpeg default)
filter_threads: 0         # 0 = auto/default
encoder_threads: 0        # 0 = auto/default
```

> One value differs between this file and the built-in defaults: the shipped file
> sets `performance_mode: safe_performance` (copy the audio stream untouched),
> whereas the built-in default, used when no config file is found at all, is
> `safe` (re-encode audio to AAC). Deleting your `superview.yaml` therefore
> changes how audio is handled.

Override with environment variables:

```bash
export SUPERVIEW_MIN_BITRATE=262144
export SUPERVIEW_MAX_BITRATE=209715200
export SUPERVIEW_LOG_LEVEL=debug
export SUPERVIEW_PERFORMANCE_MODE=safe_performance
export SUPERVIEW_VIDEO_PRESET=fast
export SUPERVIEW_FILTER_THREADS=4
export SUPERVIEW_ENCODER_THREADS=8
./superview-gui
```

`SUPERVIEW_FFMPEG_DIR` is the one that has no counterpart in the file: it names the
directory holding `ffmpeg` and `ffprobe`, and takes precedence over both the bundled
copy and `PATH`. It exists because the bundled build is one decision applied to every
machine, and a machine it suits badly needs a way out that does not involve waiting for
a release.

```bash
SUPERVIEW_FFMPEG_DIR=/usr/bin ./superview-gui
```

## Architecture

### Project Structure

```
superview/
├── common/                     # Encoding logic, shared by any front end
│   ├── common.go               # Pipeline, session lifecycle, ffprobe/ffmpeg calls
│   ├── config.go               # Configuration loading and defaults
│   ├── hardware.go             # Encoder classification, hardware device setup
│   ├── probe.go                # Asks each encoder to encode a frame; the verdict rules
│   ├── health.go               # System health checks (the Diagnostic button)
│   ├── metrics.go              # Encoding metrics
│   ├── observability.go        # Event recording and logging
│   ├── security.go             # Path and input validation
│   ├── command-*.go            # OS-specific process setup
│   ├── health_disk_*.go        # Free-disk-space probe per platform
│   ├── *_test.go               # Unit, golden and integration tests
│   └── testdata/ffprobe/       # Recorded ffprobe output the parser is tested against
├── docs/                       # Technical contracts, audit journal, hardware support
├── gui_main.go                 # GUI entry point (Fyne)
├── gui_native_dialog_*.go      # Native file dialogs (zenity/kdialog, PowerShell)
├── superview.yaml              # Default configuration
├── FyneApp.toml                # Fyne packaging metadata
├── Makefile                    # Local build and quality targets
├── THIRD_PARTY_NOTICES.md      # The FFmpeg shipped in the archives, and its licence
├── .github/scripts/            # Release-time checks (the NVENC driver floor guard)
└── RELEASING.md                # How a release is made
```

### Encoding Pipeline

```
Startup → CheckFfmpeg → ProbeHardwareSupport → ApplyEncoderProbe
Input → CheckVideo → InitEncodingSession → GeneratePGM → EncodeVideo → CleanUp → Output
                               ↓
                ValidateBitrate + FindEncoder
                VideoSpecs.Validate()
                EncodingMetrics / Observability hooks
```

## Development

> The current codebase targets **Go 1.26+**.

A source build ships no FFmpeg, so a development machine needs one on `PATH`.

### Linux build dependencies

```bash
sudo apt update
sudo apt install -y ffmpeg libgl1-mesa-dev xorg-dev libwayland-dev libxkbcommon-dev
```

> `libwayland-dev` and `libxkbcommon-dev` are needed since Fyne 2.8, which moved
> to GLFW 3.4 and its Wayland backend. They are build-time requirements only.

Optional, for native file dialogs (falls back to the Fyne dialog otherwise):

```bash
sudo apt install -y zenity
```

### Windows toolchain

```powershell
winget install -e --id Gyan.FFmpeg --version 8.1.1 --accept-package-agreements --accept-source-agreements
winget install -e --id GoLang.Go --accept-package-agreements --accept-source-agreements
winget install -e --id BrechtSanders.WinLibs.POSIX.UCRT --accept-package-agreements --accept-source-agreements
```

> ⚠️ **The `--version 8.1.1` is load-bearing; do not drop it.** Plain
> `winget install Gyan.FFmpeg` currently installs a build compiled against NVIDIA
> headers that demand driver **610.00** — a version the RTX Enterprise branch,
> which drives professional cards, does not reach: its driver branch stops at
> 597.06. On such a machine NVENC can never start, whatever the driver. `8.1.1`
> demands 570.0 and works, so testing the hardware paths against 8.1.2 would
> measure the wrong thing.
> [docs/hardware-support.md](docs/hardware-support.md) has the measured table,
> and [RELEASING.md](RELEASING.md#bumping-the-bundled-ffmpeg) covers the pin.

### Build from source

A source build produces no bundle, so it uses whatever FFmpeg is on `PATH`.

Windows GUI:

```powershell
go build -ldflags="-H=windowsgui" -o superview-gui.exe .
.\superview-gui.exe
```

Linux:

```bash
go build -o superview-gui .
./superview-gui
```

### Build & Test

```bash
make test        # go test ./... -- the whole module, as CI does
make coverage    # coverage over ./..., which the 50% CI gate measures
make check       # fmt, vet, lint, coverage and govulncheck
make build       # GUI binary for the current platform
```

`make build-gui-windows` is Windows-native: Fyne draws through cgo, so setting
`GOOS=windows` from Linux gives no C toolchain and the link step fails. The release
workflow builds each platform on its own runner for the same reason.

Set `SUPERVIEW_REQUIRE_FFMPEG=1` to turn the ffmpeg-dependent skips into failures --
this is what CI does, so that a green suite cannot mean "encoded nothing".

Releases are made from the Actions tab and are documented in
[RELEASING.md](RELEASING.md). There is no local release script.

For contributors: [RELEASING.md](RELEASING.md) covers how a release is made,
[docs/CONTRATS.md](docs/CONTRATS.md) what the code guarantees, and
[docs/hardware-support.md](docs/hardware-support.md) which GPUs generally work.

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
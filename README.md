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

There is nothing to install alongside it. Each download carries its own `ffmpeg`
and `ffprobe`, and Superview uses those in preference to anything on the
machine — deliberately, because which FFmpeg build is installed decides whether
hardware encoding works at all.

### Windows

1. Open the **[latest release](https://github.com/Canaill51/superview/releases/latest)**.
   Under **Assets**, click the file whose name ends in **`-windows-x86_64-setup.exe`**.
   It is around 60 MB — most of that is FFmpeg — and it lands in your
   `Downloads` folder.

2. Your browser may say the file is not commonly downloaded and offer to discard
   it. Choose **Keep**.
   [Why Windows warns about Superview](#why-windows-warns-about-superview)
   explains what that warning is about.

3. **Double-click the file you downloaded.** Windows shows a blue window titled
   **"Windows protected your PC"**. It offers only **Don't run**, which is not
   what you want. Click the small **More info** link, then the **Run anyway**
   button that appears underneath. Windows asks this once per downloaded file.

4. The installer needs no administrator rights: it installs Superview for your
   user only, together with `ffmpeg.exe` and `ffprobe.exe`, in
   `%LOCALAPPDATA%\Programs\Superview`. Click **Next** until **Install**, then
   **Finish**. Tick *Create a desktop shortcut* on the way if you want one.

Superview is in the Start menu from then on. To remove it, use **Settings** →
**Apps** → **Installed apps** → **Superview** → **Uninstall**. Installing a newer
version over an older one updates it in place.

#### Why Windows warns about Superview

The releases are not signed with a code-signing certificate. Those are rented
yearly from a certificate authority, and this project has none — so Windows has
no publisher name to show you, and SmartScreen has no download history for a
file it is seeing for the first time. The warning says Windows does not know who
made this, not that the file is harmful. Some antivirus products go further and
quarantine the file without asking; the cause is the same missing signature.

What you can check instead: every release ships a `checksums.txt`, the source of
what you are running is this repository, and the installer is assembled by
[`.github/workflows/release.yml`](.github/workflows/release.yml) on GitHub's own
runners — not on anybody's laptop.

### Linux

Download the file whose name ends in `-linux-x86_64.AppImage` from the
[latest release](https://github.com/Canaill51/superview/releases/latest). It is a
single file that carries the application and its FFmpeg: nothing to extract and
nothing to install. Make it executable, then run it:

```bash
chmod +x superview-*-linux-x86_64.AppImage
./superview-*-linux-x86_64.AppImage
```

In a file manager, the equivalent is right-click → **Properties** → allow
executing the file as a program, then double-click it.

An AppImage needs FUSE 2 to mount itself. Ubuntu 24.04 does not ship it; install
it with `sudo apt install libfuse2t64`, or skip FUSE altogether and run
`./superview-*-linux-x86_64.AppImage --appimage-extract-and-run`.

### Verify your download

Optional, and worth doing if you want more than the download's word for itself.
Put `checksums.txt`, published beside the downloads, in the same folder:

```bash
sha256sum -c checksums.txt          # Linux
```

On Windows, print the two values and compare them by eye:

```powershell
(Get-FileHash .\superview-*-windows-x86_64-setup.exe -Algorithm SHA256).Hash
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
- A video taller than 9:8 — a portrait phone clip, for instance — is refused:
  widening it to 16:9 would fold the picture back on itself at both edges. The
  un-squeeze mode keeps the frame's width and is not affected.

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

`cannot find ffmpeg/ffprobe on your system`, from an installed release, means its
`ffmpeg` and `ffprobe` are no longer beside the application: reinstall it
rather than moving or deleting those files. From a source build,
put FFmpeg on `PATH`. To make Superview use a particular FFmpeg, point
`SUPERVIEW_FFMPEG_DIR` at the directory holding the two — it wins over the
bundled copy and over `PATH`.

**`not enough memory for this conversion`** is Superview declining to start
something the machine cannot hold. Encoding on the CPU needs roughly 0.27 GB of
memory per megapixel of the *output* frame, and 0.42 GB when the source is
10-bit — a 4:3 4K clip widens to 5120×2880, which is about 6 GB. The message
quotes both figures. Close other applications, convert a video with fewer
pixels, or pick a hardware encoder in the codec dropdown if *Diagnostic* says
this machine accepts one; a GPU encode is not subject to this check.

**If a conversion stops on its own and the window closes**, memory ran out
anyway — the estimate above is an average, not a guarantee. The operating system
kills the encoder, and on Linux it then stops the whole application with it. The
log names that too, with the memory free at that moment: look for `ffmpeg was
killed by the system` or `The system signalled the application to stop`.

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
adapted for Rust and FFmpeg.

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

When no hardware encoder works for the source's own codec but one works for the
other codec family, Superview uses it rather than falling back to the CPU — an
Intel HD 620 refuses every H.265 encoder and accepts H.264, and encoding H.264
on the GPU beats encoding H.265 on four cores by a wide margin. The window and
the log say so, and say what it costs: a 10-bit source is stored as 8-bit when
the conversion moves to H.264. Picking an encoder yourself in the codec dropdown
always wins over this.

A hardware encoder is also asked about the frame it will actually be given, not
just about a small one. Encoders have upper bounds — the same Intel HD 620 tops
out at 4096 pixels a side, and a 4:3 4K clip widens to 5120 — so the question is
put again at the real size once a file is chosen, and Superview steps back to the
CPU before starting if the answer is no. The window says which encoder will
really run, and why, before you press *Start transformation*.

**The releases ship their own FFmpeg**, and Superview prefers it over
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

1. next to the executable
2. `$XDG_CONFIG_HOME/superview/superview.yaml`, or `~/.config/superview/superview.yaml` when that variable is not set (Linux), or `%AppData%\superview\superview.yaml` (Windows)
3. the current working directory

If none exists, built-in defaults apply. This is the `superview.yaml` shipped with
the project:

```yaml
min_bitrate: 102400       # ~0.1 Mbps minimum
max_bitrate: 209715200    # ~200 Mbps maximum
temp_dir_prefix: "superview-*"
encoder_codecs: ["264", "265", "hevc"]
log_level: info
performance_mode: safe_performance    # safe = re-encode audio to AAC | safe_performance = copy audio
video_preset: ""         # optional: ultrafast..veryslow, translated per encoder (NVENC p1-p7, AMF, QSV)
filter_threads: 0         # 0 = auto/default
encoder_threads: 0        # 0 = auto/default
```

> The shipped file and the built-in defaults agree: `performance_mode` defaults
> to `safe_performance` (copy the audio stream untouched, falling back to AAC if
> the copy is refused), with or without a `superview.yaml`. Set `safe` to always
> re-encode the audio to AAC.

A key Superview does not know, or a `min_bitrate` above `max_bitrate`, makes the
whole file ignored rather than half-applied: the log names the key or the two
values, and the built-in defaults apply instead.

The configuration comes from the file and the built-in defaults only: there is no
`SUPERVIEW_*` override of a setting.

The one environment variable Superview reads is `SUPERVIEW_FFMPEG_DIR`: it names the
directory holding `ffmpeg` and `ffprobe`, and takes precedence over both the bundled
copy and `PATH`. It exists because the bundled build is one decision applied to every
machine, and a machine it suits badly needs a way out that does not involve waiting for
a release.

```bash
SUPERVIEW_FFMPEG_DIR=/usr/bin ./superview
```

## Architecture

### Project Structure

```
superview/
├── Cargo.toml, Cargo.lock      # The Cargo workspace (the shipped product); rust-toolchain.toml pins the toolchain
├── crates/superview-core/      # Encoding logic, with no window code
│   └── src/                    # pgm (distortion maps), encode/ (ffmpeg arguments, run, fallback
│                               #   cascade), pipeline, video (ffprobe), tools (finding ffmpeg),
│                               #   hardware + probe (encoder choice), memory, disk, session,
│                               #   health (the Diagnostic), config, security, metrics, logfile
├── crates/superview-app/       # The window (Slint): state, dialogs, preferences, journal
├── crates/superview-harness/   # A test bench, never shipped
├── packaging/                  # The Windows installer script and the Linux AppImage script
├── scripts/                    # check.sh, the single entry point of verification
├── docs/                       # Technical contracts, audit journal, hardware support
├── superview.yaml              # Default configuration
├── Makefile                    # Shortcuts for the checks and the release build
├── THIRD_PARTY_NOTICES.md      # The FFmpeg shipped with the releases, and its licence
├── .github/scripts/            # Release-time checks (the NVENC driver floor guard)
└── RELEASING.md                # How a release is made
```

### Encoding Pipeline

```
Startup → find ffmpeg/ffprobe → probe each encoder on a real frame
Input → check_video (ffprobe) → session (temporary directory) → PGM maps → encode_video → clean-up → Output
                               ↓
                bitrate validation + encoder choice
                video checks, metrics, log
```

## Development

> Superview is written in **Rust**; the compiler version is pinned in
> `rust-toolchain.toml`, and [rustup](https://rustup.rs) provides it. The window is drawn by Slint's software renderer.

A source build ships no FFmpeg, so a development machine needs one on `PATH`.

### Linux build dependencies

```bash
sudo apt update
sudo apt install -y ffmpeg libfontconfig1-dev
```

`libfontconfig1-dev` is what Slint's text layout is built against; the other
graphics libraries are loaded at run time. This is the list the CI installs.

Optional, for native file dialogs: the file pickers go through the desktop portal
first, then `zenity` or `kdialog`. With none of them the window asks for the path
in a text field instead.

```bash
sudo apt install -y zenity
```

### Windows toolchain

```powershell
winget install -e --id Rustlang.Rustup --accept-package-agreements --accept-source-agreements
winget install -e --id Gyan.FFmpeg --version 8.1.1 --accept-package-agreements --accept-source-agreements
```

rustup's default Windows toolchain is the MSVC one, which needs the Visual Studio
Build Tools: if they are missing, install them with the "Desktop development with
C++" workload.

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

```bash
cargo build --release --locked -p superview-app
./target/release/superview        # superview.exe on Windows
```

The binary reports its version from `SUPERVIEW_VERSION` (`x.y.z`), read at build
time; without it a build calls itself `dev`. `make build` does the same build from
the repository root.

### Build & Test

```bash
make check             # every gate: format, clippy, tests (scripts/check.sh)
make test              # the whole test suite
make coverage-audit    # the 50 % line-coverage floor and cargo audit, as CI does
make fmt               # cargo fmt --check
make clippy            # clippy, warnings are errors
make clean             # cargo clean
```

`make test` needs **ffmpeg on your PATH**. `make coverage-audit` also needs
`cargo-llvm-cov` and `cargo-audit` (`cargo install --locked …`; the versions CI
uses are in `.github/workflows/rust.yml`).

Without `SUPERVIEW_REQUIRE_FFMPEG=1`, every test that needs ffmpeg skips silently
-- the integration tests and the remap equivalence test, which are the whole of
what checks a real conversion end to end. `make test` and CI set it, so that a
green suite cannot mean "encoded nothing". Running `cargo test` by hand, set it
yourself.

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
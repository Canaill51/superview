# Third-party notices

Superview's release archives ship a copy of **FFmpeg** (`ffmpeg` and `ffprobe`)
alongside the application. This file records what that copy is and what your
rights to it are.

## Why a copy is shipped

FFmpeg decides at compile time which NVIDIA NVENC API version it will ask the
driver for, and refuses to encode on a driver older than that. The version
number of an FFmpeg build says nothing about which one it chose: two builds both
calling themselves "8.1.2" can demand NVIDIA driver 570 and 610 respectively.
A machine whose driver cannot reach the higher of those loses hardware encoding
entirely, with no symptom beyond a conversion that runs several times slower.

Superview therefore ships a build whose requirement it has measured, and prefers
it over whatever FFmpeg is installed on the machine. See
[`docs/hardware-support.md`](docs/hardware-support.md) for the measurements.

If you would rather Superview used your own FFmpeg, set `SUPERVIEW_FFMPEG_DIR`
to the directory containing `ffmpeg` and `ffprobe`. That takes precedence over
the bundled copy.

## FFmpeg

- **Upstream project**: <https://ffmpeg.org/>
- **Source code**: <https://github.com/FFmpeg/FFmpeg>
- **Licence**: GNU General Public License version 3 or later (these are GPL
  builds; they include libx264 and libx265, which are GPL).

The binaries are redistributed unmodified from the builds below. Neither build
is produced by this project.

| Platform | Build | Provenance |
| --- | --- | --- |
| Windows | `ffmpeg-8.1.1-essentials_build` | [GyanD/codexffmpeg release `8.1.1`](https://github.com/GyanD/codexffmpeg/releases/tag/8.1.1) |
| Linux | `ffmpeg-n8.1.2-…-linux64-gpl-8.1` | [BtbN/FFmpeg-Builds release `autobuild-2026-08-31-13-27`](https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2026-08-31-13-27) |

The exact archive URLs and their SHA-256 checksums are pinned in
[`.github/workflows/release.yml`](.github/workflows/release.yml), which verifies
both before packaging. The build recipes are published by each project at the
links above.

### Written offer for source code

Superview itself is distributed under the GPLv3 (see [`LICENSE`](LICENSE)), and
so are the FFmpeg binaries shipped with it. The complete corresponding source
for those binaries is the FFmpeg source at the revision each build names, which
is publicly available at <https://github.com/FFmpeg/FFmpeg>, together with the
build recipes linked above. If you would prefer to receive it another way, open
an issue at <https://github.com/Canaill51/superview/issues> and we will arrange
it.

## Superview's own dependencies

The Go modules Superview links against are listed in [`go.mod`](go.mod), with
their exact versions and checksums in [`go.sum`](go.sum). Their licences travel
with them in the Go module cache.

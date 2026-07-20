# Third-Party Notices

## FFmpeg

Superview's Windows release archives bundle a prebuilt FFmpeg (`ffmpeg.exe`,
`ffprobe.exe`) so that hardware-acceleration behavior (`nvenc`/`amf`/`qsv` encoders
combined with `D3D11VA`/`DXVA2` decode) is reproducible across machines and does not
depend on whichever FFmpeg version a user happens to have installed.

- **Build**: [BtbN/FFmpeg-Builds](https://github.com/BtbN/FFmpeg-Builds), release
  [`autobuild-2026-07-20-14-10`](https://github.com/BtbN/FFmpeg-Builds/releases/tag/autobuild-2026-07-20-14-10),
  asset `ffmpeg-n8.1.2-29-g703dcc25b9-win64-gpl-8.1.zip` (static, win64, GPL
  configuration).
- **Upstream source**: [FFmpeg](https://github.com/FFmpeg/FFmpeg), commit
  `703dcc25b9` (29 commits after tag `n8.1.2`).
- **License**: [GPL-3.0](https://www.gnu.org/licenses/gpl-3.0.html) (this build enables
  `libx264`/`libx265`, which are GPL-licensed). Superview itself is distributed under
  GPLv3 (see [LICENSE](LICENSE)), so bundling this build introduces no additional
  license obligations beyond this notice.
- The exact BtbN build configuration/scripts used to produce this binary are published
  at https://github.com/BtbN/FFmpeg-Builds.

FFmpeg is not modified from the upstream build; Superview invokes `ffmpeg.exe`/
`ffprobe.exe` as separate subprocesses (no static or dynamic linking against
Superview's own Go code).

When updating the bundled version, update the pinned `FFMPEG_RELEASE_TAG`,
`FFMPEG_ASSET_NAME`, and `FFMPEG_ASSET_SHA256` values in
[`.github/workflows/release.yml`](.github/workflows/release.yml) and this file
together.

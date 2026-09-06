# Hardware support

Superview does not carry a GPU whitelist, and it no longer trusts the list
`ffmpeg -encoders` prints either. At startup it **asks each encoder to encode a
frame** and keeps only the ones that answer. What the GUI offers, what the
hardware line announces and what the conversion runs all come from that
measurement.

The distinction matters more than it sounds. `ffmpeg -encoders` answers a
compile-time question -- what the binary was built with -- and knows nothing
about your driver. On the machine this was written on it advertises six
encoders (`h264_qsv`, `hevc_qsv`, `h264_vaapi`, `hevc_vaapi`, `h264_v4l2m2m`,
`hevc_v4l2m2m`) that cannot open a device at all.

The lists below are a guide to what usually works, not a matrix the program
consults.

For H.264 and H.265 hardware acceleration, the practical prerequisites are:

- the GPU must provide hardware encode support for the codec you want
- the installed driver must expose that support **at the API version your
  FFmpeg build demands** -- see the section below, this is the one that bites
- the installed FFmpeg build must include the relevant hardware encoder
  (`nvenc`, `amf`, or `qsv`)

On Windows, Superview can combine these hardware encoders with `D3D11VA` or
`DXVA2` decode when the vendor-specific `hwaccel` token is not exposed by
FFmpeg.

## Superview ships its own FFmpeg

The Windows and Linux release archives carry `ffmpeg` and `ffprobe`, and
Superview uses those in preference to any FFmpeg on the machine. The reason is
the section below: the NVENC driver requirement is a property of how FFmpeg was
compiled, not of its version number, so leaving that choice to whatever the user
happens to have installed means the program behaves differently on two machines
that look identical.

The bundled build is pinned to an NVENC driver floor of **570.0**, and the
release refuses to publish a build with a different one.

To use your own instead, set `SUPERVIEW_FFMPEG_DIR` to a directory containing
both `ffmpeg` and `ffprobe`:

```bash
SUPERVIEW_FFMPEG_DIR=/usr/bin superview        # Linux
set SUPERVIEW_FFMPEG_DIR=C:\ffmpeg\bin         # Windows
```

That override wins over the bundled copy; if it names a directory with no
`ffmpeg` in it, Superview logs a warning and carries on with the bundled one.
Building from source produces no bundle, so a source build uses your PATH.

## The NVIDIA driver floor belongs to your FFmpeg build, not to FFmpeg

NVENC negotiates an API version. If the version FFmpeg was **compiled** against
is newer than the one your driver exposes, the encoder refuses the work:

```
Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0
The minimum required Nvidia driver for nvenc is 610.00 or newer
```

That minimum is fixed at build time by the `nv-codec-headers` the binary was
compiled against, so **two builds that both call themselves "FFmpeg 8.1.2" can
demand different drivers**. Measured by reading the compiled literal out of each
binary, on 2026-09-06:

| Build | FFmpeg | Minimum driver |
| --- | --- | --- |
| Ubuntu `libavcodec62` 7:8.0.1-3ubuntu2 | 8.0.1 | 530.41.03 |
| gyan.dev `ffmpeg-8.1.1-full_build` (winget `Gyan.FFmpeg`) | 8.1.1 | 570.0 |
| gyan.dev `ffmpeg-8.1.2-full_build` (winget `Gyan.FFmpeg`) | 8.1.2 | **610.00** |
| gyan.dev `ffmpeg-9.0.1-full_build` | 9.0.1 | 610.00 |
| BtbN `win64-gpl-8.1` | 8.1.2 | 570.0 |
| BtbN `win64-gpl-9.0` | 9.0.1 | 610.00 |

The upstream table those floors come from, published with each header release:

| `nv-codec-headers` | Video Codec SDK | Minimum driver |
| --- | --- | --- |
| `n12.1.14.0` | 12.1 | 530.41.03 (Linux) / 531.61 (Windows) |
| `n12.2.72.0` | 12.2 | 550.54.14 / 551.76 |
| `n13.0.19.0` | 13.0 | 570.0 |
| `n13.1.15.0` | 13.1 | 610.0 |

Two consequences worth knowing:

- **A newer FFmpeg can lose hardware acceleration that an older one had.** An
  NVIDIA RTX A1000 on Windows is driven by the RTX Enterprise branch, which
  tops out at 597.06 — so a build demanding 610 can never use NVENC on it, no
  matter how current the driver is. Dropping back to a build with a 570 floor
  is the fix.
- **Video Codec SDK 13.1 dropped Maxwell, Pascal and Volta** from its supported
  architectures; 13.0 still listed them. A build compiled against older headers
  keeps those GPUs working.

You can read the floor out of any build yourself:

```bash
strings ffmpeg.exe | grep -E "^(610\.00|570\.0|550\.54\.14|530\.41\.03)$"
```

## Nvidia

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

## AMD

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

## Intel

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

## What Superview actually targets

Superview currently targets the following FFmpeg hardware encoders for H.264 and H.265,
in this order:

- Nvidia: `h264_nvenc`, `hevc_nvenc`
- AMD: `h264_amf`, `hevc_amf`
- Intel: `h264_qsv`, `hevc_qsv`
- VAAPI: `h264_vaapi`, `hevc_vaapi`
- **Vendor-neutral**: `h264_d3d12va`, `hevc_d3d12va` (Windows) and `h264_vulkan`,
  `hevc_vulkan`
- V4L2: `h264_v4l2m2m`, `hevc_v4l2m2m`
- CPU fallback: `libx264`, `libx265`

The vendor encoders come first because they expose the rate control and presets Superview
sets. The vendor-neutral pair matters for the failure this page describes: **D3D12 Video
Encode and Vulkan video encode are driven by the display driver, not by NVENC's own API**,
so they have no driver floor to miss. On a machine whose FFmpeg demands an NVIDIA driver it
cannot install, they are the hardware path that remains — and the probe is what finds that
out, without anyone having to know in advance which one works.

## If your GPU does not appear

**Press Diagnostic first.** Its *Encoders* section lists every encoder that was
probed, and for each refusal it carries FFmpeg's own words. That line usually
names the cause outright, and it is the single most useful thing to attach to a
bug report.

The common causes, in the order they occur:

- the driver is older than the API version this FFmpeg build was compiled
  against — the report says so explicitly, with the driver version it wants
- FFmpeg was installed without the relevant hardware encoder enabled
- the graphics driver is missing, or vendor-generic in a way that hides the
  video engine
- the machine exposes decode support but not encode support for that card
- the codec is supported on paper by the family, but not by that exact SKU

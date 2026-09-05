# Hardware support

Superview does not carry a GPU whitelist. It asks the FFmpeg build you have
installed what it can do, and that answer is the one that counts -- the lists
below are a guide to what usually works, not a compatibility matrix the program
consults.

If a card here does not show its encoder in the GUI, the list is not what needs
fixing; see [What to check when a supported GPU does not appear](#if-your-gpu-does-not-appear).

Superview does not hardcode a fixed GPU whitelist at runtime. It relies on the hardware capabilities actually exposed by your installed FFmpeg build and graphics driver.

For H.264 and H.265 hardware acceleration, the practical prerequisites are:

- the GPU must provide hardware encode and decode support for the codec you want to use
- the installed driver must expose that support correctly
- the installed FFmpeg build must include the relevant hardware encoder (`nvenc`, `amf`, or `qsv`)

On Windows, Superview can now combine these hardware encoders with `D3D11VA` or `DXVA2` decode when the vendor-specific `hwaccel` token is not exposed by FFmpeg.

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

Superview currently targets the following FFmpeg hardware encoders for H.264 and H.265:

- Nvidia: `h264_nvenc`, `hevc_nvenc`
- AMD: `h264_amf`, `hevc_amf`
- Intel: `h264_qsv`, `hevc_qsv`
- CPU fallback: `libx264`, `libx265`

## If your GPU does not appear

If your GPU is theoretically compatible but the encoder does not appear in the GUI, the issue is usually one of these:

- FFmpeg was installed without the relevant hardware encoder enabled
- the graphics driver is missing, outdated, or vendor-generic in a way that hides the video engine
- the machine exposes decode support but not encode support for that specific card
- the codec is supported on paper by the family, but not by that exact SKU

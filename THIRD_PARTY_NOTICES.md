# Third-party notices

Superview's releases (the Windows installer and the Linux AppImage) ship a copy of **FFmpeg** (`ffmpeg` and `ffprobe`)
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

## Superview's own dependencies (Rust)

The application shipped in the releases is the Rust program under
[`rust/`](rust/). It links 471 crates (build tools excluded), whose exact
versions and checksums are in [`rust/Cargo.lock`](rust/Cargo.lock). List them
with their licences with:

```bash
cd rust && cargo tree -p superview-app -e normal --target all --prefix none --format "{p} {l}"
```

The main ones:

| Crate | Role | Licence |
| --- | --- | --- |
| `slint` 1.18.1 (and its `i-slint-*` crates) | the window | GPL-3.0-only (one of the three licences Slint offers; Superview is GPLv3 and uses it under that one) |
| `winit` 0.30 | window system access (X11, Wayland, Windows) | Apache-2.0 |
| `softbuffer` 0.4 | software rendering to the window | MIT OR Apache-2.0 |
| `rfd` 0.17 (with `zbus` 5) | file pickers (desktop portal) | MIT |
| `serde`, `serde_json`, `serde-saphyr` | settings and ffprobe output | MIT OR Apache-2.0 |
| `fs4` | free disk space | MIT OR Apache-2.0 |
| `signal-hook` (Unix), `ctrlc` (Windows) | SIGINT/SIGTERM and Ctrl+C | MIT OR Apache-2.0 |
| `log` | logging | MIT OR Apache-2.0 |
| `tiny-skia`, `skrifa`, `ab_glyph`, `fontdb` | drawing and text | BSD-3-Clause, MIT OR Apache-2.0, Apache-2.0, MIT |

Across all 471 crates the licences are: MIT or Apache-2.0 (or both, as an
option) for the large majority, plus Unicode-3.0, Zlib, BSD-2/3-Clause, ISC,
BSL-1.0, 0BSD, CC0-1.0 and the Slint licence above. Where a crate offers a
choice, Superview uses the permissive one (or GPL-3.0 for Slint). Each crate's
licence text travels with its source on crates.io.

## Libraries embedded in the Linux AppImage

The window toolkit loads two X11 libraries at run time. A minimal system may
not have them, so the AppImage carries a copy of each in `usr/lib/`, taken
unmodified from the Ubuntu packages on the build machine (Ubuntu 24.04 in the
release workflow), together with each package's copyright file under
`usr/share/doc/<package>/copyright`:

| Library | Ubuntu package | Upstream | Licence |
| --- | --- | --- | --- |
| `libxkbcommon-x11.so.0` | `libxkbcommon-x11-0` | <https://xkbcommon.org/> | MIT/X11 |
| `libxcb-xkb.so.1` | `libxcb-xkb1` | <https://xcb.freedesktop.org/> | MIT/X11 |

They are not linked into Superview: the application finds them through a
relative library path (`$ORIGIN/../lib`) when it starts. Their copyright
notices, as the packages provide them:

### libxkbcommon-x11-0

```text
Source: http://xkbcommon.org/

Copyright 1985, 1987, 1988, 1990, 1998  The Open Group
Copyright 2008, 2009  Dan Nicholson

Permission is hereby granted, free of charge, to any person obtaining a
copy of this software and associated documentation files (the "Software"),
to deal in the Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute, sublicense,
and/or sell copies of the Software, and to permit persons to whom the
Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN
ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN
CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.

Except as contained in this notice, the names of the authors or their
institutions shall not be used in advertising or otherwise to promote the
sale, use or other dealings in this Software without prior written
authorization from the authors.

Copyright (c) 1993, 1994, 1995, 1996 by Silicon Graphics Computer Systems, Inc.

Permission to use, copy, modify, and distribute this
software and its documentation for any purpose and without
fee is hereby granted, provided that the above copyright
notice appear in all copies and that both that copyright
notice and this permission notice appear in supporting
documentation, and that the name of Silicon Graphics not be
used in advertising or publicity pertaining to distribution
of the software without specific prior written permission.
Silicon Graphics makes no representation about the suitability
of this software for any purpose. It is provided "as is"
without any express or implied warranty.

SILICON GRAPHICS DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS
SOFTWARE, INCLUDING ALL IMPLIED WARRANTIES OF MERCHANTABILITY
AND FITNESS FOR A PARTICULAR PURPOSE. IN NO EVENT SHALL SILICON
GRAPHICS BE LIABLE FOR ANY SPECIAL, INDIRECT OR CONSEQUENTIAL
DAMAGES OR ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE,
DATA OR PROFITS, WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE
OR OTHER TORTIOUS ACTION, ARISING OUT OF OR IN CONNECTION  WITH
THE USE OR PERFORMANCE OF THIS SOFTWARE.

Copyright 1987, 1988 by Digital Equipment Corporation, Maynard, Massachusetts.

                        All Rights Reserved

Permission to use, copy, modify, and distribute this software and its
documentation for any purpose and without fee is hereby granted,
provided that the above copyright notice appear in all copies and that
both that copyright notice and this permission notice appear in
supporting documentation, and that the name of Digital not be
used in advertising or publicity pertaining to distribution of the
software without specific, written prior permission.

DIGITAL DISCLAIMS ALL WARRANTIES WITH REGARD TO THIS SOFTWARE, INCLUDING
ALL IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS, IN NO EVENT SHALL
DIGITAL BE LIABLE FOR ANY SPECIAL, INDIRECT OR CONSEQUENTIAL DAMAGES OR
ANY DAMAGES WHATSOEVER RESULTING FROM LOSS OF USE, DATA OR PROFITS,
WHETHER IN AN ACTION OF CONTRACT, NEGLIGENCE OR OTHER TORTIOUS ACTION,
ARISING OUT OF OR IN CONNECTION WITH THE USE OR PERFORMANCE OF THIS
SOFTWARE.
```

### libxcb-xkb1

```text
This package was debianized by Jamey Sharp <sharpone@debian.org> on
Thu, 18 Mar 2004 00:48:42 -0800, and later updated by Josh Triplett
<josh@freedesktop.org>.  The package is co-maintained by the XCB developers
via the XCB mailing list <xcb@lists.freedesktop.org>.

It was downloaded from https://xcb.freedesktop.org/dist

Upstream Authors: Jamey Sharp <sharpone@debian.org>
                  Josh Triplett <josh@freedesktop.org>

Copyright:

Copyright (C) 2001-2006 Bart Massey, Jamey Sharp, and Josh Triplett.
All Rights Reserved.

Permission is hereby granted, free of charge, to any person
obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the
Software without restriction, including without limitation
the rights to use, copy, modify, merge, publish, distribute,
sublicense, and/or sell copies of the Software, and to
permit persons to whom the Software is furnished to do so,
subject to the following conditions:

The above copyright notice and this permission notice shall
be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY
KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR
PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS
BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
OTHER DEALINGS IN THE SOFTWARE.

Except as contained in this notice, the names of the authors
or their institutions shall not be used in advertising or
otherwise to promote the sale, use or other dealings in this
Software without prior written authorization from the
authors.
```

## Go modules (not shipped any more)

The Go program is still in the repository as the reference implementation, but
no release contains it any more. Its modules are listed in [`go.mod`](go.mod),
with their exact versions and checksums in [`go.sum`](go.sum).

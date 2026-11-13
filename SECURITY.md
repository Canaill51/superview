# Security Policy

## Supported Versions

Security fixes are provided for the current `master` branch and the latest stable release line.

| Version / Branch | Supported          |
| ---------------- | ------------------ |
| master           | :white_check_mark: |
| latest release   | :white_check_mark: |
| older releases   | :x:                |

## Reporting a Vulnerability

Please report vulnerabilities through GitHub Security Advisories so the issue can be reviewed privately.
Include the affected version, the platform you used, the exact input that triggered the issue, and any relevant logs or screenshots.

We will acknowledge valid reports as soon as practical and provide a status update after triage.

## Scope

What Superview touches, and therefore what a report can be about:

- **The files it reads.** The input video, `superview.yaml` and `preferences.json`. The video is
  read by ffprobe and ffmpeg with the MP4 demuxer forced (`-f mov`), so a file renamed `.mp4`
  is refused rather than interpreted as something else, such as a concat script.
- **The paths it is given.** Paths from the file pickers are treated as untrusted: they are
  canonicalised and checked before use (`crates/superview-core/src/security.rs`), and an
  output in a system directory, or over the input, is refused. Temporary files live in a private
  directory removed at the end of each conversion.
- **The programs it starts.** ffmpeg and ffprobe, bundled with the installer and the AppImage or
  found on `PATH`, are started with an argument list, never through a shell. On Windows, a
  `.bat` or `.cmd` named `ffmpeg` is refused.
- **The Resolve plugin.** `superview-ofx` runs inside DaVinci Resolve and calls C interfaces
  (OpenFX, the CUDA driver, the OpenCL loader), so it contains `unsafe` code; the rest of the
  workspace denies it. On Windows, the GPU driver libraries are loaded from System32 only, so a
  copy planted next to Resolve is never picked up.
- **Out of scope:** vulnerabilities in FFmpeg itself (report them to FFmpeg), and anything
  that needs an attacker who can already write to your installation directory.

## Integrity of the downloads

- **Releases are immutable**: once published, a release's files and its tag cannot change. A
  wrong file is fixed by a new version, never replaced.
- **`checksums.txt`** is published beside the downloads; the README shows how to check them.
- **Provenance attestations** (releases after v0.5.0): each installer, AppImage and plugin archive is attested for the
  workflow run that built it, which tells a file built here from one swapped in afterwards:

  ```bash
  gh attestation verify <downloaded file> --repo Canaill51/superview
  ```

- **Dependencies** are checked on every pull request (`cargo audit`, `cargo deny`: known
  advisories, yanked crates, licences, sources limited to crates.io and one pinned git fork),
  and every build uses the committed `Cargo.lock` (`--locked`).

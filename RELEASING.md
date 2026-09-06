# Releasing

A release is one click: **Actions → Release → Run workflow**, type the version,
run it. The workflow tests, builds Windows and Linux, tags the commit and
publishes. There is nothing to prepare beforehand.

This file explains the parts that are not obvious from the button.

## Every change goes through a pull request

This is the one rule the release flow depends on, so it comes first.

GitHub builds the release notes from **pull requests merged since the previous
release**. A commit pushed straight to `master` is not one, so it appears
nowhere in the release — not in the list, not in the contributor line. The
change ships; the record of it does not. This has already happened here:
`8d02159 Updated RElease YAML File` went in directly and would be absent from
any release covering it.

So: branch, open a pull request, merge it. Even for a typo. It costs a minute
and it is the only thing keeping the release notes honest.

**The pull request title is what users read.** It is the line that lands in the
release, so write it as a sentence about what changed — *Stop release binaries
from announcing themselves as modified*, not *fix bug*. Commit messages do not
appear; only the title does.

## The notes write themselves

The body of a published release is:

1. the download list and the `sha256sum -c checksums.txt` line, written by the run;
2. **Changes** — one entry per pull request merged since the previous release;
3. **Dependencies** — the Dependabot bumps, kept separate so a week of daily
   `gomod` updates cannot bury the rest;
4. a **Full Changelog** compare link.

Sections 2 and 3 come from `.github/release.yml`. That file's `"*"` catch-all is
load-bearing: GitHub drops any pull request matching no category, and this
repository does not label its own, so removing it would empty every release.

**To see the notes before releasing**, ask GitHub for them. This creates and
publishes nothing:

```bash
gh api -X POST /repos/Canaill51/superview/releases/generate-notes \
  -f tag_name=v0.2.4 -f target_commitish=master --jq '.body'
```

> Until v0.2.3 the notes lived in `RELEASE_NOTES.md`, which had to be rewritten
> before every release and copied into the tag by the run. It was easy to
> forget — so the run grew a check that its first line named the version — and
> between releases it sat in `master` holding the *previous* release's text,
> which reads exactly like something stale. The generated list says what
> changed without any of it.

> **`gh` points at the wrong repository by default.** This is a fork of
> `Niek/superview`, so `gh pr create` and friends resolve to the parent unless
> told otherwise — a pull request meant for your own release would be offered
> to the upstream author instead. Fix it once, per clone:
>
> ```bash
> gh repo set-default Canaill51/superview
> ```
>
> `git push` is unaffected: `origin` already points to the right place.

**To write the notes yourself**, push an annotated tag by hand instead of using
the button. Its message leads the release body, ahead of the generated list:

```bash
git tag -a v0.2.4          # your editor opens; first line is the heading
git push origin v0.2.4
```

That path stops at a draft, so you can read it before it is public.

## The version number lives in the tag

`fyne package --app-version` stamps it into the binary, which then reports it
in its window title, its log and its Diagnostic report. **Nothing reads the
version from the source tree**, and `FyneApp.toml` deliberately carries no
`Version` line, so there is nothing to bump and nothing that can drift.

A binary built any other way — a plain `go build` — has no version to claim and
says `dev`, followed by the commit it came from:

```
dev (7a74276)                 built locally, clean tree
dev (7a74276, modified)       built locally, uncommitted changes
0.2.4 (5d3b6f9)               a release
```

## The three ways to run it

| | What it does |
| --- | --- |
| **Run workflow, with a version** | The normal path. Tests, builds, tags, publishes. |
| **Run workflow, version left empty** | A dry run: builds both platforms, tags nothing, publishes nothing. Use it to try a change to the workflow without spending a version number. |
| **`git push` an annotated tag** | The escape hatch, and the way to write your own notes. The release stops at a draft. |

Two checkboxes sit next to the version field: **draft** stops before
publishing, and **prerelease** tags `RC-x.y.z` instead of `vx.y.z` and marks
the release as a pre-release.

## What the run guarantees, and what it does not

The tag is created only after both platforms have built, so a failed Windows
build cannot burn a version number. Before anything is built, the run refuses a
version that is not `x.y.z` and a tag that already exists. Checksums are
generated from the uploaded artifacts with bare filenames, so the
`sha256sum -c checksums.txt` the notes advertise works on the files as
downloaded. Each packaged binary is unpacked and rejected if it is stamped
`vcs.modified=true`, which is what stops a release from announcing itself as a
modified build.

Both archives carry a pinned FFmpeg, and the run refuses to ship one whose
NVENC driver floor is not the 570.0 it was pinned for — the check reads that
floor out of the downloaded binary. See **Bumping the bundled FFmpeg** below
before changing the pin. The Linux package is also installed into a throwaway
directory and rejected if `make install` does not put `ffmpeg`, `ffprobe`, the
application and its icon where they belong.

Not checked automatically: that the published binary reports the version it
claims. Downloading the Linux archive and running it is still worth doing on a
release that matters — the log line is enough:

```bash
tar -xJf superview-gui-v0.2.4-linux-x86_64.tar.xz
DISPLAY= timeout 10 ./superview/usr/local/bin/superview
tail -1 ~/.cache/superview/superview.log     # should read 0.2.4 (<commit>)
```

## Bumping the bundled FFmpeg

Each build job pins `FFMPEG_URL`, `FFMPEG_SHA256` and `FFMPEG_DRIVER_FLOOR` in
[`.github/workflows/release.yml`](.github/workflows/release.yml).

**The property being pinned is the driver floor, not the version.** FFmpeg fixes
at compile time the NVENC API version it will demand, so two builds both called
"8.1.2" can require NVIDIA driver 570 and 610. A machine that cannot reach the
floor loses hardware encoding entirely, and the only symptom is a conversion
several times slower than it should be. Superview ships 8.1.1 on Windows for
exactly this reason: gyan.dev's 8.1.2 demands 610.00, and the NVIDIA RTX
Enterprise driver branch tops out at 597.06.

To move the pin, read the floor out of the candidate build before anything else:

```bash
. .github/scripts/nvenc-driver-floor.sh
driver_floor_of path/to/ffmpeg          # prints e.g. 570.0
```

If it is higher than the current pin, raising it takes hardware encoding away
from every machine below the new number. That is a decision, not an upgrade.

Pick a source whose URL will still resolve. gyan.dev publishes versioned GitHub
releases, which are permanent. BtbN prunes its mid-month autobuilds — the pin in
the fork this idea came from is already a 404 — but keeps the last build of each
month, and those have held since October 2024.

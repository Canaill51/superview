# Releasing

A release is one click: **Actions → Release → Run workflow**, type the version,
run it. The workflow tests, builds Windows and Linux, tags the commit and
publishes. This file explains the two things that are not obvious from the
button.

## RELEASE_NOTES.md is the release notes, in full

The whole file becomes the message of the annotated tag, and the tag message
becomes the body of the published release. So:

- **No header, no comment, no "notes for the next release" preamble.** Anything
  in the file is text users read.
- The first line is the title. It must mention the version being released --
  the run refuses to start otherwise, which is what stops v0.2.4 from shipping
  with v0.2.3's text.
- Indent the body by two spaces, as the existing notes do. Six spaces or more
  is a code block in Markdown, which is how the measurement table in v0.2.2's
  notes kept its alignment.

Write it in a pull request, like any other file. That review is the only human
checkpoint in the flow, so it is where the text has to be right.

## The version number lives in the tag

`fyne package --app-version` stamps it into the binary, which then reports it
in its window title, its log and its Diagnostic report. Nothing reads the
version from the source tree.

`FyneApp.toml` still holds a `Version`, but only as a fallback for a build made
outside the workflow. Bump it in the same pull request as the notes, so a plain
`go build` from the tag does not announce the previous release.

## The three ways to run it

| | What it does |
| --- | --- |
| **Run workflow, with a version** | The normal path. Tests, builds, tags, publishes. |
| **Run workflow, version left empty** | A dry run: builds both platforms, tags nothing, publishes nothing. Use it to try a change to the workflow without spending a version number. |
| **`git push` an annotated tag** | The escape hatch. The tag's own message is the notes, and the release stops at a draft. |

Two checkboxes sit next to the version field: **draft** stops before
publishing, and **prerelease** tags `RC-x.y.z` instead of `vx.y.z` and marks
the release as a pre-release.

## What the run guarantees, and what it does not

The tag is created only after both platforms have built, so a failed Windows
build cannot burn a version number. Checksums are generated from the uploaded
artifacts with bare filenames, so the `sha256sum -c checksums.txt` the notes
advertise works on the files as downloaded.

Not checked automatically: that the published binary reports the version it
claims. Downloading the Linux archive and running it is still worth doing on a
release that matters.

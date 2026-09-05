Superview v0.2.3

The video this release produces is identical to v0.2.2's. What changes is that
the application now tells you which build made it.

What changed for users

  The build identity appears in three places: the window title, a line in the
  log at startup, and the first line of the Diagnostic report -- before
  everything else, because it is the one line that says which binary the rest
  describes.

  It reads the release number followed by the seven characters of the commit
  it was built from. A binary built from a modified checkout says so, with
  ", modified". One built outside the release workflow reads "dev" instead of
  inventing a number.

  This matters because the three releases before this one do not behave alike.
  v0.2.0 removed a seam at the centre of the image; v0.2.2 lowered the
  Balanced bitrate by about 20%. Until now neither the log nor the Diagnostic
  report -- the two things the README asks users to attach to a bug report --
  said which of them had produced the file in question.

  The number comes from the release tag, never from a file in the source tree,
  so a published binary cannot claim a version that a committed file drifted
  away from.

Also

  Releasing is now one button. Running the Release workflow with a version
  builds both platforms, tags the commit with these notes and publishes. The
  dry run that made this possible found its own first bug: the packaging tool
  rejects a version string like "master", and so would have the first RC-*
  tag, in the middle of a real release.

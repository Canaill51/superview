# Landing a change

The full procedure behind the short version in [`AGENTS.md`](../AGENTS.md).
[`merge-on-label.yml`](../.github/workflows/merge-on-label.yml) links here from
the comment it leaves on every pull request it merges.

**Every change goes through a pull request, however small.** Release notes are
generated from merged pull requests, so a direct push to `master` ships the
change and loses the record of it. One commit did exactly that and appears in no
release. A repository ruleset now refuses the direct push rather than trusting
this paragraph, and holds the required checks in front of every merge.

**The pull request title is what users read in the release.** Write it as a
sentence about what changed — *Stop release binaries from announcing themselves
as modified*, not *fix bug*. Commit messages do not appear there.

**Put the `merge` label on it, and stop there.**
[`merge-on-label.yml`](../.github/workflows/merge-on-label.yml) works out which
method applies, merges once the required checks pass, and comments on the pull
request saying which method it took and why. Nothing else starts it, so an
unlabelled pull request stays where it is: you still decide *when* something
lands, you no longer decide *how*. If the label produced no comment, remove it
and put it back — a label already present fires nothing.

From a terminal, label through the REST endpoint, not `gh pr edit`:

```bash
gh api -X POST "repos/$REPO/issues/<NN>/labels" -f "labels[]=merge"
```

`gh pr edit --add-label` fails on this repository — it reads `projectCards`,
which GitHub has retired with Projects (classic) — and it fails *silently* if
stderr is redirected, which once looked exactly like the workflow not firing.
`gh pr edit --title` goes the same way; `gh api -X PATCH repos/$REPO/pulls/<NN>
-f title=…` does not. From the web interface the label behaves normally.

The rule it applies, which is also the one to follow when merging by hand:

**Squash and merge**, which is what the whole history uses: one commit per pull
request, titled `Title (#NN)`. Use *Create a merge commit* only when another
open pull request is based on this one — squash rewrites the SHA, and the one
stacked above then loses its base and conflicts.

**The question is what sits on top of you, not what you sit on**, and one
command answers it:

```bash
gh pr list --base "$(gh pr view <NN> --json headRefName -q .headRefName)" --state open
```

Nothing listed → *Squash and merge*. Anything listed → *Create a merge commit*,
and merge the stack from the bottom up. **A `base` of `master` settles nothing**:
the bottom pull request of a stack has exactly that base and still must not be
squashed. That is how #59, #61 and #62 came to need merge commits while their
base read `master` — the earlier wording here said the opposite, and following
it would have orphaned the pull request above each of them.

Three repository settings hold the rest of it up, and they are why you no
longer have to think about the dialog either: *Rebase and merge* is switched
off, because it writes neither `Title (#NN)` nor a merge commit and nothing
here has ever used it; the squash title is pinned to the pull request title,
which used to fall back to the commit subject on a single-commit pull request
and quietly contradict the paragraph above; and the branch is deleted on merge.

Releases are one button, documented in [`RELEASING.md`](../RELEASING.md). Nothing to
prepare, no file to write, no version to bump. Never tag by hand unless the
intent is explicitly to release.

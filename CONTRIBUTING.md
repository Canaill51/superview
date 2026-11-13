# Contributing to Superview

Superview is released under the [GNU GPL v3](LICENSE); a change you propose is
published under the same licence. The technical rules -- how the code is
organised, its contracts, the traps already met, the commands -- are in
[AGENTS.md](AGENTS.md). They apply to people and to AI agents alike.

## Reporting a problem

Open an issue with the **Bug report** template. It asks for what a diagnosis
needs first:

- the **version**, as the window title and the first line of the *Diagnostic*
  report show it (for the Resolve plugin, the name of the zip you downloaded);
- the **operating system**, the **GPU and its driver**;
- the output of **Diagnostic**, which says which encoders this machine accepts;
- the **log**: `%LOCALAPPDATA%\superview\superview.log` on Windows,
  `~/.cache/superview/superview.log` on Linux, and
  `%LOCALAPPDATA%\superview-ofx\superview-ofx.log` for the plugin.

A security issue does not go in a public issue: see [SECURITY.md](SECURITY.md).

## Proposing a change

1. **Branch** from `master`, named `type/short-description`: lower case,
   words joined by hyphens, in English. Types: `feat` (new feature), `fix`,
   `docs`, `refactor`, `test`, `chore` (maintenance), `ci`. One branch per
   subject.

   ```sh
   git switch -c fix/windows-close-crash
   ```

2. **Verify** with `scripts/check.sh` (or `make check`): format, clippy, then
   the whole workspace suite with `SUPERVIEW_REQUIRE_FFMPEG=1`, so ffmpeg must
   be on `PATH`. [rustup](https://rustup.rs) provides the pinned compiler;
   [docs/environnement.md](docs/environnement.md) rebuilds the rest. A new test
   is proved by counter-proof: put the defect back and watch it fail
   (*Verifying a change* in AGENTS.md).

3. **Update the documentation in the same commit.** The table of
   [`.claude/skills/docs-sync/SKILL.md`](.claude/skills/docs-sync/SKILL.md)
   says which documents to reread for each kind of change, and
   `crates/superview-app/src/docs_guards.rs`, part of the suite, checks links,
   cited paths, the index of `docs/` and the register numbers. `README.md` and
   `README_FR.md` move together (CI checks it). Where it applies, complete the
   numbered registers: a choice that looks like a defect →
   [docs/decisions.md](docs/decisions.md) (`DEC-xx`); a trap that cost time →
   [docs/lecons.md](docs/lecons.md) (`L-xxx`); every fix →
   [docs/corrections.md](docs/corrections.md).

4. **Open a pull request** to `master`. **Its title is a sentence about what
   changed**, for someone who uses Superview -- *Stop release binaries from
   announcing themselves as modified*, not *fix bug*: it becomes, as written,
   the line of the release notes. Commit messages appear nowhere there. A title
   that looks like a branch name is refused. The description follows the pull
   request template.

5. **Put exactly one `RN: …` label on it.** It picks the section of the release
   notes (DEC-72):

   | Label | Section of the notes | For |
   |---|---|---|
   | `RN: major feature`, `RN: feature` | New features | something Superview or the plugin could not do |
   | `RN: improvement`, `RN: security`, `RN: refactor` | Improvements | what existed works better, faster or more safely |
   | `RN: fix` | Fixes | a defect corrected |
   | `RN: ignore` | left out | CI, tooling, plans, registers, internal documents |

   ```sh
   gh pr create --base master --title "Stop the window from freezing on a cancelled probe" --label "RN: fix"
   ```

   The [`pr-label.yml`](.github/workflows/pr-label.yml) check stays red until
   the pull request has exactly one `RN: …` label and a sentence for a title;
   it runs again when either changes. A pull request with no `RN:` label would
   simply be missing from the notes: [`.github/release.yml`](.github/release.yml)
   has no catch-all section, on purpose.

   Dependabot's pull requests are the exception: they carry the `dependencies`
   label, no `RN:` label, and are listed under *Dependencies*.

6. **Label it `merge`**, as the next section says.

## Merging

**Every change goes through a pull request, however small.** Release notes are
generated from merged pull requests, so a direct push to `master` ships the
change and loses the record of it. One commit did exactly that and appears in no
release. A repository ruleset now refuses the direct push rather than trusting
this paragraph, and holds the required checks in front of every merge: *README
parity (EN/FR)*, *Rust on ubuntu-latest*, *Rust on windows-latest*, *Rust
coverage and audit*, *pr-label* and *Workflows pass actionlint*.

**Put the `merge` label on it, and stop there.**
[`merge-on-label.yml`](.github/workflows/merge-on-label.yml) works out which
method applies, merges once the required checks pass, and comments on the pull
request saying which method it took and why. Nothing else starts it, so an
unlabelled pull request stays where it is: you still decide *when* something
lands, you no longer decide *how*. If the label produced no comment, remove it
and put it back -- a label already present fires nothing.

From a terminal, label through the REST endpoint, not `gh pr edit`:

```bash
gh api -X POST "repos/$REPO/issues/<NN>/labels" -f "labels[]=merge"
gh api -X POST "repos/$REPO/issues/<NN>/labels" -f "labels[]=RN: fix"
```

`gh pr edit --add-label` fails on this repository -- it reads `projectCards`,
which GitHub has retired with Projects (classic) -- and it fails *silently* if
stderr is redirected, which once looked exactly like the workflow not firing.
`gh pr edit --title` goes the same way; `gh api -X PATCH repos/$REPO/pulls/<NN>
-f title=…` does not. From the web interface the labels behave normally.

The rule it applies, which is also the one to follow when merging by hand:

**Squash and merge**, which is what the whole history uses: one commit per pull
request, titled `Title (#NN)`. Use *Create a merge commit* only when another
open pull request is based on this one -- squash rewrites the SHA, and the one
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
base read `master` -- an earlier wording of this rule said the opposite, and
following it would have orphaned the pull request above each of them.

Three repository settings hold the rest of it up, and they are why you no
longer have to think about the dialog either: *Rebase and merge* is switched
off, because it writes neither `Title (#NN)` nor a merge commit and nothing
here has ever used it; the squash title is pinned to the pull request title,
which used to fall back to the commit subject on a single-commit pull request
and quietly contradict the paragraph above; and the branch is deleted on merge.

That last setting does nothing for a pull request the workflow merges: GitHub
runs none of its automations on what the workflow's own token does. So the
workflow waits for the merge and deletes the branch itself -- except when
another open pull request is based on it, because deleting a base branch through
the API closes the pull request above instead of retargeting it. It comments on
the pull request when it keeps a branch for that reason.

Nor does the merge commit start `rust.yml` or `lint.yml` on `master`: a push
made with the workflow's token triggers no workflow, `workflow_dispatch` being
the documented exception. So, once the merge has landed on the default branch,
the workflow dispatches both on it, and `master` is tested as a push would have
tested it. If a run is missing -- the dispatch is best effort and only warns --
`gh workflow run rust.yml --ref master` and the same for `lint.yml` do by hand
what it does. The ruleset stays non-strict on purpose (DEC-35): requiring
branches to be up to date would make every stacked pull request unmergeable
until rebased by hand.

## Releases

Releases are one button, documented in [RELEASING.md](RELEASING.md). Nothing to
prepare, no file to write, no version to bump. Never tag by hand unless the
intent is explicitly to release. Before one, read the titles and `RN:` labels
of the pull requests merged since the last release: they are the notes.

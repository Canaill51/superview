---
name: publier
description: Publish a new Superview release (the Release workflow, with a version) when the user asks for one. Checks the preconditions, runs the workflow, watches it and verifies the published files. Never use it unprompted.
---

Publishing is public and spends a version number for good: **run this only when
the user has asked for a release in this conversation.** A merged pull request, a
green CI or a finished feature is not a request. If the user has not asked, say
what is ready and stop.

The release is the **Release** workflow dispatched with a version
([`RELEASING.md`](../../../RELEASING.md)). It tests, builds Windows and Linux, and
only then tags and publishes. Never tag by hand, never `gh release create`
(both are denied in `.claude/settings.json` on purpose).

Use `--repo Canaill51/superview` on every `gh` command: this is a fork, and `gh`
resolves to the parent without it.

## 1. Preconditions — stop at the first one that fails, and say which

1. **`master` is current and clean.**
   `git fetch origin && git status -sb` — on `master`, no local changes, equal to
   `origin/master`. The workflow builds `origin/master`, whatever the local tree is.
2. **Nothing the user expects is still waiting.**
   `gh pr list --repo Canaill51/superview --state open` — list any open pull
   request, especially one labelled `merge` whose checks are still running. Release
   notes list the pull requests merged **since the previous release**: one that
   lands after the release is not in it. Ask whether to wait.
3. **`master` is green.**
   `gh run list --repo Canaill51/superview --branch master --limit 5 --json workflowName,conclusion,headSha`
   — the latest `Test` / `Lint` runs on the current `origin/master` commit must be
   successes.
4. **The release workflow has been tried since its last change (L-114).**
   `git log -1 --format=%H -- .github/workflows/release.yml packaging` gives the
   last commit that touched the release; `gh run list --repo Canaill51/superview --workflow=release.yml --json conclusion,headSha,event,createdAt --limit 10`
   must hold a successful run whose `headSha` is that commit **or a later one**.
   If not, run a dry run first (`gh workflow run release.yml --repo Canaill51/superview --ref master`
   with no version), watch it, and only continue if it is green.

## 2. The version

1. `gh release list --repo Canaill51/superview --limit 3` gives the latest.
2. If the user named a version, use it. Otherwise **propose** the next patch
   (`0.2.8` → `0.2.9`) and say why (new minor or major only if the user says so).
   Never choose it silently.
3. It must be `x.y.z` with nothing else, and the tag must not exist already
   (`gh release list` and `gh api repos/Canaill51/superview/git/matching-refs/tags/v<version>`).
4. Show the notes it would get, which creates and publishes nothing:
   `gh api -X POST repos/Canaill51/superview/releases/generate-notes -f tag_name=v<version> -f target_commitish=master --jq '.body'`

## 3. Confirm, then run

State in one short block: the version, the commit (`git rev-parse --short origin/master`),
the notes' pull requests, and whether it is a draft. Then **wait for the user's
yes to that exact version.** A request to "publish" without a number is answered
with the proposal, not with a run.

Default is to publish. If the user prefers to read it first, add `-f draft=true`
(the run stops at a draft they publish by hand); a release candidate is
`-f prerelease=true` (tags `RC-x.y.z`).

```bash
gh workflow run release.yml --repo Canaill51/superview --ref master -f version=<x.y.z>
```

## 4. Watch and verify

1. `gh run list --repo Canaill51/superview --workflow=release.yml --limit 1` for the run id,
   then `gh run watch <id> --repo Canaill51/superview --exit-status` (about 15 minutes; run it
   in the background rather than polling).
2. If a job fails, report **which job and step** from `gh run view <id> --log-failed | tail -40`
   and stop. The tag is only created after both platforms build, so a failed run
   has not spent the version number. Do not rerun blindly.
3. On success, `gh release view v<version> --repo Canaill51/superview --json assets,isDraft,url`
   must list **exactly** `superview-v<version>-windows-x86_64-setup.exe`,
   `superview-v<version>-linux-x86_64.AppImage` and `checksums.txt`, and
   `isDraft` must match what was asked.
4. Report the release URL and the three files. Say plainly what was **not** checked:
   the published binary on a real Windows machine, and any GPU encoder
   (`RELEASING.md`, *What the run guarantees, and what it does not*).

## 5. Afterwards

Update `docs/SUITE.md` (latest release, nothing left open) and, if the run
taught something, add the lesson with `/piege`.

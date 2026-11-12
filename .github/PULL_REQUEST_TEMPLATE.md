<!-- Title: a sentence saying what changed, for someone who uses Superview
     (e.g. "Stop release binaries from announcing themselves as modified").
     It becomes the line of the release notes. One "RN: ..." label (table in CONTRIBUTING.md). -->

## Purpose

<!-- What this pull request changes, and why. -->

## Registers

<!-- Numbers created or changed: DEC-xx (docs/decisions.md), L-xxx (docs/lecons.md),
     findings (docs/analyse.md). "None" otherwise. -->

## Checks

- [ ] `scripts/check.sh` passes, with ffmpeg on `PATH` (it sets `SUPERVIEW_REQUIRE_FFMPEG=1`)
- [ ] Every new test reddens when the defect it guards is put back
- [ ] Documentation reread as `.claude/skills/docs-sync/SKILL.md` says; `README.md` and `README_FR.md` moved together
- [ ] Title is a sentence; exactly one `RN: …` label (`RN: ignore` if nothing changes for a user)
- [ ] `docs/corrections.md` and the registers completed if needed
- [ ] Screenshots before / after if the window changes

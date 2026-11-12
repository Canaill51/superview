---
name: docs-sync
description: Bring Superview's documentation up to date after any change (code, window, encoding, plugin, packaging, CI, procedure). Load it before each commit, and whenever a change touches what a user sees or what a .md file describes. Says which documents to reread for each kind of change, and how to check them.
---

**The documentation follows each change, in the same pull request.** Nothing is done while
the documents the change concerns have not been reread and corrected.

## 1. The automatic check

```sh
cargo test -p superview-app --locked -- docs_guards
```

`crates/superview-app/src/docs_guards.rs` checks relative links and their headings, the
paths cited between backticks (and `path.rs:item`), that `docs/README.md` lists every
document, that each `DEC-xx`, `L-xxx` and finding number is given once, that the
`decisions.md` index lists every decision, that every lesson has its rule in
`pieges-par-domaine.md`, and that every number cited anywhere exists. It runs with the
workspace suite (hooks, CI). A failure means a document to fix, **not** a check to weaken.
A new kind of inconsistency gets a new check there, with its counter-proof test.

## 2. What to reread, by change

The test does not judge whether a text is still true. Reread **whole** the sections below
and compare them with the code:

| Change | Reread |
|---|---|
| Anything a user sees: window, label, message, Diagnostic, log | `README.md` **and** `README_FR.md` (*Using Superview*, *If something goes wrong*) |
| Encoder, hardware path, probe, bundled FFmpeg, driver floor | `docs/hardware-support.md`; *Hardware acceleration* in both READMEs; `RELEASING.md` (*Bumping the bundled FFmpeg*, *Manual pins*); `docs/contrats.md` (FFmpeg and hardware contracts) |
| Pipeline, ffmpeg arguments, cascade, session, tool lookup | the matching *Contrat* section of `docs/contrats.md`; `docs/architecture.md` (module tables, data flow) |
| `superview.yaml` key or default | *Configuration* in both READMEs; `docs/contrats.md` (*Contrat de configuration*) |
| Resolve plugin | *DaVinci Resolve plugin* in both READMEs; `docs/hardware-support.md` (*The DaVinci Resolve plugin*); `docs/architecture.md` (`superview-ofx`); `docs/tests-manuels.md` |
| New module, crate or moved file | `docs/architecture.md`; *Project Structure* in both READMEs; `AGENTS.md` where it names it |
| Packaging, release workflow, CI | `RELEASING.md`; *Download and install* in both READMEs; the release rows of `AGENTS.md`; `.claude/skills/publier/SKILL.md` |
| Verification recipe, script, hook, environment variable | `AGENTS.md` (*Verifying a change*); `docs/environnement.md`; `.claude/skills/verify/SKILL.md`; *Build & Test* in both READMEs; `CLAUDE.md` for `.claude/` |
| A choice that looks like a defect | `docs/decisions.md` (new entry and index row, or *Remplacée* / *amendée* on the old one); the table of `AGENTS.md` if it is structural |
| A trap that cost time | `docs/lecons.md` (entry) and `docs/pieges-par-domaine.md` (short rule, in its domain) |
| A fix; a finding opened or closed | `docs/corrections.md`; `docs/analyse.md` (§ 4 table: mark invalid, never delete) |
| A batch of work finished or set aside | `docs/plan.md` (state at the top, one history line) |
| A new document under `docs/` | `docs/README.md` (one line) |
| A public signature | `grep -rn "name" --include='*.md' .` |
| Any change | the pull request title, a sentence: it becomes the release note line |

## 3. Writing rules

- Internal journals in French; everything an outsider reads in English (`AGENTS.md`,
  *The repository's language split*).
- Both READMEs move together, headings included (`readme-parity` in CI). In the French one,
  GUI labels stay in English, in their exact case.
- Registers are never rewritten: an entry is marked replaced or invalid, with the reason.

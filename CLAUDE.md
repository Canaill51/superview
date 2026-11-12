@AGENTS.md
@docs/architecture.md

## Specific to Claude Code

`AGENTS.md` holds the instructions every agent follows; this section only describes `.claude/`,
which other agent tools ignore. Nothing in `.claude/` is an instruction of its own.

- **Skills** (`.claude/skills/`), loaded from their description:
  - `docs-sync` — which documents to reread for each kind of change, and the automatic check
    (`crates/superview-app/src/docs_guards.rs`). Load it before each commit.
  - `verify` — runs the recipe of *Verifying a change* gate by gate and reports each gate
    instead of summarising.
  - `publier` — publishes a release, only when the user asks for one (*Landing a change*).
- **Hooks** (`.claude/hooks/`, tested by `scripts/checks/hooks.sh`): `scripts/check.sh --fast`
  (format and clippy) after every code edit; the full check whenever Claude stops with
  non-Markdown changes pending; a guard against dangerous commands (`garde-fou.sh`); and at
  session start, the head of `docs/plan.md`.
- **Permissions** (`.claude/settings.json`): the verification recipe is allowed; tagging,
  creating a release and force-pushing are denied.

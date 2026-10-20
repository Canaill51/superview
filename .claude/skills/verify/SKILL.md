---
name: verify
description: Run the AGENTS.md verification recipe for Superview, gate by gate, and report each result instead of a summary. Use before claiming any change is verified.
---

`scripts/check.sh` runs gates 1-3 in one command (it is also what the git
pre-commit hook and the Claude hooks run), but stops at the first failure and
says nothing about skips: this skill stays the gate-by-gate report.

Run the gates below **in order**, from `rust/`, with
`export PATH=$HOME/.cargo/bin:$PATH` if `cargo` is not found. Stop at the first
gate that cannot run and say so: a gate that did not run is not a gate that
passed.

1. **Formatting.** `cargo fmt --all --check` must print nothing.
2. **Clippy.** `cargo clippy --workspace --all-targets --locked -- -D warnings`
   must report nothing.
3. **Tests, with ffmpeg required.**
   `SUPERVIEW_REQUIRE_FFMPEG=1 cargo test --workspace --locked 2>&1 | tee /tmp/sv-test.log | tail -40`.
   Then `grep -c "ignored\|skipping" /tmp/sv-test.log` and list every skipped or
   ignored test by name: a skip in the integration tests or in the remap
   equivalence test means no real conversion was checked.
4. **Counter-proof.** For every test added or changed in this change:
   reintroduce the defect it guards, run that test alone, confirm it reddens,
   revert. Name each test and whether its counter-proof reddened. A test whose
   counter-proof stayed green is reported as **unproven**, not as passing.

Report the four gates as a table: gate, command, result (pass / fail / did not
run / unproven), and the one line of output that matters. Never fold a skipped
or unrun gate into "all green".

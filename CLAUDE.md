@AGENTS.md

<!--
Claude Code can read AGENTS.md on its own, but not in every session; the import
above loads it everywhere. Every other agent tool reads AGENTS.md directly.

Put the content in AGENTS.md. Anything added below would be Claude-specific and
invisible to the other tools -- which is how a repository ends up with two sets
of instructions that disagree. This one had three sources of truth about
releasing, and the whole point of collapsing them was to stop that.

The only Claude-specific material lives in .claude/ (permissions, a gofmt hook,
the /verify skill), and none of it is an instruction.
-->

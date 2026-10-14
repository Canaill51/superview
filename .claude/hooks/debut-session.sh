#!/usr/bin/env bash
# Hook SessionStart : injecte le court état de reprise (docs/SUITE.md) dans le contexte.
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
[ -f docs/SUITE.md ] && { echo "=== Reprise (docs/SUITE.md) ==="; head -n 15 docs/SUITE.md; }
exit 0

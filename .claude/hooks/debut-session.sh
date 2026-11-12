#!/usr/bin/env bash
# Hook SessionStart : injecte le court état de reprise (tête de docs/plan.md) dans le contexte.
cd "${CLAUDE_PROJECT_DIR:-.}" || exit 0
[ -f docs/plan.md ] && { echo "=== Reprise (docs/plan.md) ==="; head -n 15 docs/plan.md; }
exit 0

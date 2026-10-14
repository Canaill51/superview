#!/bin/sh
# PostToolUse hook: gofmt the Go file Claude just edited.
#
# "gofmt -l . must print nothing" is a CI gate (lint.yml, job fmt). Running it at
# edit time is cheap and idempotent, so the gate never fires on formatting alone.
# Exit 2 is the only status Claude Code shows back to the model; anything else
# non-zero is a silent "non-blocking error", so a file gofmt cannot parse must
# exit 2 to be noticed. A missing gofmt is not that case: the hook then does
# nothing, and the CI gate still stands.
set -u

command -v jq >/dev/null 2>&1 || exit 0
file=$(jq -r '.tool_input.file_path // empty' 2>/dev/null) || exit 0
case "$file" in
  *.go) ;;
  *) exit 0 ;;
esac
[ -f "$file" ] || exit 0

# Hooks run in a non-interactive shell, where a Go toolchain installed under
# $HOME (docs/ENVIRONNEMENT.md puts it in ~/.local/go) is not on PATH.
gofmt=$(command -v gofmt 2>/dev/null || true)
if [ -z "$gofmt" ]; then
  for dir in "$HOME/.local/go/bin" "$HOME/go/bin" /usr/local/go/bin /usr/lib/go/bin; do
    if [ -x "$dir/gofmt" ]; then gofmt="$dir/gofmt"; break; fi
  done
fi
[ -n "$gofmt" ] || exit 0

if out=$("$gofmt" -l -w "$file" 2>&1); then
  exit 0
fi
printf 'gofmt could not format %s:\n%s\n' "$file" "$out" >&2
exit 2

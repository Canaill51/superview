package main

import (
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"
)

// cutPipe matches a tool's output being cut short by the next stage of a pipe:
// "ffmpeg -version | head -n 1" in bash, "... | Select-Object -First 1" in
// PowerShell. The tool is then written to a closed pipe and exits non-zero
// (141 under bash, a failing $LASTEXITCODE in PowerShell), so a step that
// checks its status fails although the tool works (L-115).
var cutPipe = regexp.MustCompile(`-version\s*\|\s*(head\b|Select-Object\s+-First\b)`)

// TestWorkflows_DoNotCutAToolsOutputShort guards L-115: the release run's own
// checks must capture a tool's whole output before keeping its first line.
func TestWorkflows_DoNotCutAToolsOutputShort(t *testing.T) {
	files, err := filepath.Glob("../../.github/workflows/*.yml")
	if err != nil || len(files) == 0 {
		t.Fatalf("no workflow files found (err=%v)", err)
	}
	for _, f := range files {
		raw, err := os.ReadFile(f)
		if err != nil {
			t.Fatalf("cannot read %s: %v", f, err)
		}
		for i, line := range strings.Split(string(raw), "\n") {
			if strings.HasPrefix(strings.TrimSpace(line), "#") {
				continue
			}
			if cutPipe.MatchString(line) {
				t.Errorf("%s:%d cuts a tool's output short; capture it whole first (L-115):\n  %s",
					f, i+1, strings.TrimSpace(line))
			}
		}
	}
}

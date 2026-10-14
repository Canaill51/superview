//go:build !windows

package common

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestIsSensitiveSystemPath_UnixLiterals spells out the cases the audit named,
// on top of the table over sensitiveSystemDirectories.
func TestIsSensitiveSystemPath_UnixLiterals(t *testing.T) {
	for path, want := range map[string]bool{
		"/etc/x.mp4":      true,
		"/etc":            true,
		"/etcetera/x.mp4": false,
		"/home/x.mp4":     false,
	} {
		if got := isSensitiveSystemPath(path); got != want {
			t.Errorf("isSensitiveSystemPath(%q) = %v, want %v", path, got, want)
		}
	}
}

// TestIsValidOutputPath_SymlinkedParentIntoSystemDirectory pins E-17: the
// system check read the path as written, so <tmp>/out -> /usr passed it and only
// the write probe refused -- with "permission denied", which root does not get.
// The assertion is on the reason, so the test means the same thing as root.
func TestIsValidOutputPath_SymlinkedParentIntoSystemDirectory(t *testing.T) {
	link := filepath.Join(t.TempDir(), "out")
	if err := os.Symlink("/usr", link); err != nil {
		t.Skipf("cannot create a symlink here: %v", err)
	}

	err := isValidOutputPath(filepath.Join(link, "clip.mp4"))
	if err == nil {
		t.Fatal("a destination under /usr, reached through a symlink, was accepted")
	}
	if !strings.Contains(err.Error(), "protected system directory") {
		t.Errorf("refused for the wrong reason: %v", err)
	}
}

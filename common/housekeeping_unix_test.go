//go:build !windows

package common

import (
	"os"
	"path/filepath"
	"testing"
)

// TestOpenLogFile_Permissions: the log carries home-directory paths.
func TestOpenLogFile_Permissions(t *testing.T) {
	isolateUserCacheDir(t)

	w, path, err := OpenLogFile("superview-test", 1<<20)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = w.Close() }()

	info, err := os.Stat(path)
	if err != nil {
		t.Fatal(err)
	}
	if perm := info.Mode().Perm(); perm != 0o600 {
		t.Errorf("log file mode = %o, want 600", perm)
	}
	dirInfo, err := os.Stat(filepath.Dir(path))
	if err != nil {
		t.Fatal(err)
	}
	if perm := dirInfo.Mode().Perm(); perm != 0o700 {
		t.Errorf("log directory mode = %o, want 700", perm)
	}
}

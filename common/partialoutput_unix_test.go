//go:build !windows

package common

import (
	"os"
	"path/filepath"
	"syscall"
	"testing"
)

// TestNewPartialOutputPath_HonoursUmask pins E-10.
//
// os.CreateTemp creates with mode 0600; ffmpeg truncates that inode and the
// final rename keeps the mode, so every finished video arrived unreadable by
// anyone but its owner. The working file has to be created the way ffmpeg
// itself would have created the destination: 0666 before umask.
//
// Counter-proof: go back to os.CreateTemp and the mode reads 0600.
func TestNewPartialOutputPath_HonoursUmask(t *testing.T) {
	previous := syscall.Umask(0o022)
	defer syscall.Umask(previous)

	dir := t.TempDir()
	partial, err := newPartialOutputPath(filepath.Join(dir, "out.mp4"))
	if err != nil {
		t.Fatalf("newPartialOutputPath: %v", err)
	}
	defer func() { _ = os.Remove(partial) }()

	info, err := os.Stat(partial)
	if err != nil {
		t.Fatal(err)
	}
	if got := info.Mode().Perm(); got != 0o644 {
		t.Fatalf("working file mode = %04o, want 0644 (0666 under umask 022)", got)
	}
	if filepath.Dir(partial) != dir || filepath.Ext(partial) != ".mp4" {
		t.Fatalf("working file %q must sit next to the destination with the .mp4 extension", partial)
	}

	// Two calls must not collide.
	second, err := newPartialOutputPath(filepath.Join(dir, "out.mp4"))
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = os.Remove(second) }()
	if second == partial {
		t.Fatal("two working files for the same destination got the same name")
	}
}

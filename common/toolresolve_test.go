package common

import (
	"os"
	"path/filepath"
	"runtime"
	"testing"
)

// withFakeExecutableIn makes resolveToolBinary believe the program is running
// from dir, and clears the resolution cache around the test.
func withFakeExecutableIn(t *testing.T, dir string) {
	t.Helper()

	previous := osExecutable
	osExecutable = func() (string, error) {
		return filepath.Join(dir, "superview-gui"), nil
	}
	ResetToolResolutionCache()
	t.Cleanup(func() {
		osExecutable = previous
		ResetToolResolutionCache()
	})
}

// writeFakeTool drops a file where a tool binary would be and returns its path.
func writeFakeTool(t *testing.T, dir, tool string) string {
	t.Helper()

	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("failed to create %s: %v", dir, err)
	}
	path := filepath.Join(dir, toolFileName(tool))
	if err := os.WriteFile(path, []byte("stand-in"), 0o755); err != nil {
		t.Fatalf("failed to write the stand-in tool: %v", err)
	}
	return path
}

// TestResolveToolBinary_BundledIsPreferredOverPath is the decision this whole
// bundling exists to make.
//
// Whatever FFmpeg the machine has installed is the variable Superview cannot
// control: two builds both called "8.1.2" can demand different NVIDIA drivers,
// and picking up the wrong one costs the user hardware encoding with no
// symptom but a slow conversion. The shipped copy wins.
func TestResolveToolBinary_BundledIsPreferredOverPath(t *testing.T) {
	appDir := t.TempDir()
	pathDir := t.TempDir()

	bundled := writeFakeTool(t, appDir, "ffmpeg")
	writeFakeTool(t, pathDir, "ffmpeg")

	withFakeExecutableIn(t, appDir)
	t.Setenv("PATH", pathDir)

	if got := resolveToolBinary("ffmpeg"); got != bundled {
		t.Errorf("resolveToolBinary = %q, want the bundled copy %q", got, bundled)
	}
}

// TestResolveToolBinary_FindsTheLinuxInstallLayout covers the layout the Linux
// package installs.
//
// The tools cannot sit beside the app there: that directory is /usr/local/bin,
// and an "ffmpeg" dropped into it would shadow the system's for every program
// on the machine. They go under ../lib/superview instead, and this is what
// makes the app able to find them again.
func TestResolveToolBinary_FindsTheLinuxInstallLayout(t *testing.T) {
	prefix := t.TempDir()
	appDir := filepath.Join(prefix, "bin")
	if err := os.MkdirAll(appDir, 0o755); err != nil {
		t.Fatalf("failed to create %s: %v", appDir, err)
	}
	bundled := writeFakeTool(t, filepath.Join(prefix, "lib", "superview"), "ffprobe")

	withFakeExecutableIn(t, appDir)
	t.Setenv("PATH", t.TempDir())

	got := resolveToolBinary("ffprobe")
	if filepath.Clean(got) != filepath.Clean(bundled) {
		t.Errorf("resolveToolBinary = %q, want %q", got, bundled)
	}
}

// TestResolveToolBinary_OverrideWinsOverTheBundledCopy pins the escape hatch.
//
// A pinned build is the point of bundling, but it is also a single decision
// applied to every machine. If it is wrong for one of them, the user needs a
// way out that does not involve waiting for a release.
func TestResolveToolBinary_OverrideWinsOverTheBundledCopy(t *testing.T) {
	appDir := t.TempDir()
	overrideDir := t.TempDir()

	writeFakeTool(t, appDir, "ffmpeg")
	chosen := writeFakeTool(t, overrideDir, "ffmpeg")

	withFakeExecutableIn(t, appDir)
	t.Setenv(toolDirEnv, overrideDir)

	if got := resolveToolBinary("ffmpeg"); got != chosen {
		t.Errorf("resolveToolBinary = %q, want the overridden copy %q", got, chosen)
	}
}

// TestResolveToolBinary_BogusOverrideFallsThrough keeps a typo in an
// environment variable from making the program unusable.
func TestResolveToolBinary_BogusOverrideFallsThrough(t *testing.T) {
	appDir := t.TempDir()
	bundled := writeFakeTool(t, appDir, "ffmpeg")

	withFakeExecutableIn(t, appDir)
	t.Setenv(toolDirEnv, filepath.Join(t.TempDir(), "nothing-here"))

	if got := resolveToolBinary("ffmpeg"); got != bundled {
		t.Errorf("an override pointing at nothing should fall through to the bundled copy; got %q", got)
	}
}

// TestResolveToolBinary_FallsBackToPathWithNothingBundled is the case of a user
// who built from source: there is no release archive, so PATH is all there is.
func TestResolveToolBinary_FallsBackToPathWithNothingBundled(t *testing.T) {
	if runtime.GOOS == "windows" {
		// findWindowsToolBinary looks in winget and scoop directories that PATH
		// does not govern, so a real ffmpeg there would answer instead.
		t.Skip("PATH does not govern tool discovery on Windows")
	}

	appDir := t.TempDir()
	pathDir := t.TempDir()
	onPath := writeFakeTool(t, pathDir, "ffmpeg")

	withFakeExecutableIn(t, appDir)
	t.Setenv("PATH", pathDir)

	if got := resolveToolBinary("ffmpeg"); got != onPath {
		t.Errorf("resolveToolBinary = %q, want the copy on PATH %q", got, onPath)
	}
}

// TestResolveToolBinary_IgnoresADirectoryNamedLikeTheTool guards the check that
// keeps a plausible-looking path from being handed to exec.Command.
//
// Without it the failure surfaces at encoding time as a permission error, long
// after the fallback that would have worked was skipped.
func TestResolveToolBinary_IgnoresADirectoryNamedLikeTheTool(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("PATH does not govern tool discovery on Windows")
	}

	appDir := t.TempDir()
	if err := os.MkdirAll(filepath.Join(appDir, toolFileName("ffmpeg")), 0o755); err != nil {
		t.Fatalf("failed to create the decoy directory: %v", err)
	}
	pathDir := t.TempDir()
	onPath := writeFakeTool(t, pathDir, "ffmpeg")

	withFakeExecutableIn(t, appDir)
	t.Setenv("PATH", pathDir)

	if got := resolveToolBinary("ffmpeg"); got != onPath {
		t.Errorf("a directory was taken for the tool: resolveToolBinary = %q, want %q", got, onPath)
	}
}

// TestFindBundledToolBinary_SurvivesAnUnknownExecutablePath covers the
// degraded case: os.Executable can fail, and that must mean "nothing bundled"
// rather than a panic on an empty path.
func TestFindBundledToolBinary_SurvivesAnUnknownExecutablePath(t *testing.T) {
	previous := osExecutable
	osExecutable = func() (string, error) { return "", os.ErrNotExist }
	t.Cleanup(func() { osExecutable = previous })

	if got := findBundledToolBinary("ffmpeg"); got != "" {
		t.Errorf("findBundledToolBinary = %q, want empty when the executable path is unknown", got)
	}
}

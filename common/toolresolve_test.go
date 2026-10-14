package common

import (
	"os"
	"path/filepath"
	"runtime"
	"strings"
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

// wingetLayout points the three variables findWindowsToolBinary reads at fresh
// temporary directories and returns the LOCALAPPDATA one.
//
// ProgramFiles and USERPROFILE get real, empty directories rather than "":
// filepath.Join("", ...) yields a relative path that os.Stat resolves against
// the working directory (E-22), and a test must not depend on that.
func wingetLayout(t *testing.T) string {
	t.Helper()
	local := t.TempDir()
	t.Setenv("LOCALAPPDATA", local)
	t.Setenv("ProgramFiles", t.TempDir())
	t.Setenv("USERPROFILE", t.TempDir())
	return local
}

// writeWindowsTool drops a stand-in "<tool>.exe" under root/parts... and
// returns its path.
func writeWindowsTool(t *testing.T, root, tool string, parts ...string) string {
	t.Helper()
	dir := filepath.Join(append([]string{root}, parts...)...)
	if err := os.MkdirAll(dir, 0o755); err != nil {
		t.Fatalf("failed to create %s: %v", dir, err)
	}
	path := filepath.Join(dir, tool+".exe")
	if err := os.WriteFile(path, []byte("stand-in"), 0o755); err != nil {
		t.Fatalf("failed to write the stand-in tool: %v", err)
	}
	return path
}

// TestFindWindowsToolBinary_FindsAWingetPackagesTree covers the last resort of
// tool discovery on Windows, which no test reached on either runner (E-19):
// choco installs into none of the candidates, and the resolution tests skip
// under Windows. The function reads environment variables and has no GOOS gate
// of its own, so it runs here on every OS.
//
// winget's "Gyan.FFmpeg" package unpacks into a versioned directory under
// Packages and only sometimes creates the Links shim; the walk is what finds
// ffmpeg for a user who installed it the way the README says.
//
// Counter-proof: delete the Packages walk (return "" after the candidates loop)
// and the first case reports "".
func TestFindWindowsToolBinary_FindsAWingetPackagesTree(t *testing.T) {
	packages := []string{"Microsoft", "WinGet", "Packages"}

	t.Run("package tree", func(t *testing.T) {
		local := wingetLayout(t)
		want := writeWindowsTool(t, local, "ffmpeg", append(packages,
			"Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe", "ffmpeg-8.1.1-full_build", "bin")...)

		if got := findWindowsToolBinary("ffmpeg"); got != want {
			t.Errorf("findWindowsToolBinary = %q, want the winget package copy %q", got, want)
		}
	})

	t.Run("package not named after ffmpeg", func(t *testing.T) {
		local := wingetLayout(t)
		writeWindowsTool(t, local, "ffmpeg", append(packages, "SomeVendor.MediaKit_x", "bin")...)

		if got := findWindowsToolBinary("ffmpeg"); got != "" {
			t.Errorf("findWindowsToolBinary = %q, want \"\": the walk must stay inside ffmpeg packages", got)
		}
	})

	t.Run("Links shim wins over the package tree", func(t *testing.T) {
		local := wingetLayout(t)
		writeWindowsTool(t, local, "ffmpeg", append(packages, "Gyan.FFmpeg_x", "bin")...)
		link := writeWindowsTool(t, local, "ffmpeg", "Microsoft", "WinGet", "Links")

		if got := findWindowsToolBinary("ffmpeg"); got != link {
			t.Errorf("findWindowsToolBinary = %q, want the Links shim %q", got, link)
		}
	})
}

// fakeCheckFFmpeg puts on PATH an ffmpeg that answers -version and -hwaccels
// like a real one, lists libx264 for -encoders, and exits 1 on the one query
// named by failing.
//
// The bundled-tool lookup is pointed at an empty directory as well. It could
// not find a real ffmpeg anyway -- os.Executable is the test binary in the
// go-build cache -- but saying so is cheaper than relying on it.
func fakeCheckFFmpeg(t *testing.T, failing string) {
	t.Helper()
	dir := t.TempDir()
	withFakeExecutableIn(t, t.TempDir())
	t.Setenv(toolDirEnv, "")

	encoders := []string{"Encoders:", " ------", " V....D libx264              H.264"}
	name, mode := "ffmpeg", os.FileMode(0o755)
	sh := func(query string, lines ...string) string {
		if query == failing {
			return "echo 'something went wrong' 1>&2; exit 1"
		}
		out := ""
		for _, line := range lines {
			out += "echo '" + line + "'; "
		}
		return out + "exit 0"
	}
	script := "#!/bin/sh\n" +
		"case \"$1\" in\n" +
		"  -version) " + sh("-version", "ffmpeg version 9.9 Copyright") + " ;;\n" +
		"  -hwaccels) " + sh("-hwaccels", "Hardware acceleration methods:", "cuda") + " ;;\n" +
		"  -encoders) " + sh("-encoders", encoders...) + " ;;\n" +
		"esac\n" +
		"exit 1\n"
	if runtime.GOOS == "windows" {
		name, mode = "ffmpeg.bat", os.FileMode(0o644)
		bat := func(query string, lines ...string) string {
			out := ":" + strings.TrimPrefix(query, "-") + "\r\n"
			if query == failing {
				return out + "echo something went wrong 1>&2\r\nexit /b 1\r\n"
			}
			for _, line := range lines {
				out += "echo " + line + "\r\n"
			}
			return out + "exit /b 0\r\n"
		}
		script = "@echo off\r\n" +
			"if \"%1\"==\"-version\" goto version\r\n" +
			"if \"%1\"==\"-hwaccels\" goto hwaccels\r\n" +
			"if \"%1\"==\"-encoders\" goto encoders\r\n" +
			"exit /b 1\r\n" +
			bat("-version", "ffmpeg version 9.9 Copyright") +
			bat("-hwaccels", "Hardware acceleration methods:", "cuda") +
			bat("-encoders", encoders...)
	}
	if err := os.WriteFile(filepath.Join(dir, name), []byte(script), mode); err != nil {
		t.Fatalf("failed to create the stand-in ffmpeg: %v", err)
	}
	t.Setenv("PATH", dir+string(os.PathListSeparator)+os.Getenv("PATH"))
	toolResolveCache.Delete("ffmpeg")
	t.Cleanup(func() { toolResolveCache.Delete("ffmpeg") })
}

// TestCheckFfmpeg_ReportsAFailingEncodersQuery pins CheckFfmpeg's two error
// branches after -version, which no test reached (E-19). An ffmpeg that runs
// but cannot list its accelerators or encoders must be reported, not turned
// into a map with an empty encoder list -- which the window would present as
// "no encoder available" with nothing to say why.
//
// The first case is the control: the same stand-in, failing nothing, is read
// correctly, so the error cases cannot pass because the fake is broken.
//
// Counter-proof: drop the error check after either query and the matching case
// reports a nil error.
func TestCheckFfmpeg_ReportsAFailingEncodersQuery(t *testing.T) {
	t.Run("control: nothing fails", func(t *testing.T) {
		fakeCheckFFmpeg(t, "")
		got, err := CheckFfmpeg(nil)
		if err != nil {
			t.Fatalf("CheckFfmpeg: %v", err)
		}
		if got["version"] != "9.9" || got["accels"] != "cuda" || got["encoders"] != "libx264" {
			t.Fatalf("the stand-in was misread: %v", got)
		}
	})

	for _, tc := range []struct{ failing, want string }{
		{"-encoders", "encoders"},
		{"-hwaccels", "hardware accelerators"},
	} {
		t.Run(tc.failing+" fails", func(t *testing.T) {
			fakeCheckFFmpeg(t, tc.failing)
			got, err := CheckFfmpeg(nil)
			if err == nil {
				t.Fatalf("CheckFfmpeg returned no error although %s failed; map: %v", tc.failing, got)
			}
			if !strings.Contains(err.Error(), tc.want) {
				t.Errorf("error %q should mention %q", err, tc.want)
			}
			if got != nil {
				t.Errorf("a half-filled map came back with the error: %v", got)
			}
		})
	}
}

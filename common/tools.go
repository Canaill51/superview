// Finding and invoking ffmpeg/ffprobe, and reading what the binary says about itself.

package common

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"sync"
)

// toolResolveCache stores resolved binary paths (ffmpeg/ffprobe) per process.
var toolResolveCache sync.Map

func newFFmpegCommand(args ...string) *exec.Cmd {
	return exec.Command(resolveToolBinary("ffmpeg"), args...)
}

// newFFmpegCommandContext is newFFmpegCommand with a deadline. The encoder
// probes use it: a driver that is present but wedged answers nothing at all,
// and a probe that never returns would hang the window it is meant to inform.
func newFFmpegCommandContext(ctx context.Context, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, resolveToolBinary("ffmpeg"), args...)
}

func newFFprobeCommand(args ...string) *exec.Cmd {
	return exec.Command(resolveToolBinary("ffprobe"), args...)
}

// toolDirEnv names a directory holding ffmpeg and ffprobe, and it wins over
// everything else.
//
// The escape hatch for the bundled build. A release ships a pinned FFmpeg whose
// NVENC driver floor we have measured, which is the point -- but if that build
// turns out to be wrong for a particular machine, the user needs a way out that
// does not involve waiting for a release.
const toolDirEnv = "SUPERVIEW_FFMPEG_DIR"

// osExecutable is a wrapper around os.Executable for testability.
var osExecutable = os.Executable

// resolveToolBinary finds ffmpeg or ffprobe, in this order:
//
//  1. SUPERVIEW_FFMPEG_DIR, if set -- the user's explicit choice;
//  2. the copy shipped in the release archive, next to the running executable;
//  3. PATH;
//  4. on Windows, the directories winget and scoop install into.
//
// The bundled copy comes before PATH deliberately. What FFmpeg a machine has
// installed is the variable this program cannot control and cannot inspect
// ahead of time: two builds calling themselves "8.1.2" can demand different
// NVIDIA drivers, and one of them silently costs the user hardware encoding.
// Shipping a known build and preferring it makes the behaviour the same
// everywhere, which is worth more than picking up whatever is newer.
func resolveToolBinary(tool string) string {
	if cached, ok := toolResolveCache.Load(tool); ok {
		return cached.(string)
	}

	if dir := strings.TrimSpace(os.Getenv(toolDirEnv)); dir != "" {
		if path := usableToolFile(filepath.Join(dir, toolFileName(tool))); path != "" {
			toolResolveCache.Store(tool, path)
			return path
		}
		// Not cached and not fatal: an override pointing at nothing is a
		// mistake worth reporting, not a reason to refuse to work.
		GetLogger().Warn("ignoring "+toolDirEnv+": it does not contain the tool",
			slog.String("dir", dir),
			slog.String("tool", tool),
		)
	}

	if path := findBundledToolBinary(tool); path != "" {
		toolResolveCache.Store(tool, path)
		return path
	}

	if path, err := exec.LookPath(tool); err == nil {
		toolResolveCache.Store(tool, path)
		return path
	}

	if runtime.GOOS == "windows" {
		if path := findWindowsToolBinary(tool); path != "" {
			toolResolveCache.Store(tool, path)
			return path
		}
	}

	// Deliberately not cached: the GUI shows an install link when ffmpeg is
	// missing, so the user very often installs it while the app is running.
	// Caching the failure would force a restart with no hint that one is needed.
	return tool
}

// ResetToolResolutionCache forgets previously resolved ffmpeg/ffprobe paths so
// the next call re-runs discovery. Call it when the user asks to retry after
// installing the tools.
func ResetToolResolutionCache() {
	toolResolveCache.Clear()
}

// toolFileName is the on-disk name of a tool for this platform.
func toolFileName(tool string) string {
	if runtime.GOOS == "windows" {
		return tool + ".exe"
	}
	return tool
}

// findBundledToolBinary looks for the FFmpeg shipped in the release archive.
//
// Two layouts, because the two archives are shaped differently:
//
//   - next to the executable, which is how the Windows zip is laid out and how
//     a Linux tarball unpacked by hand looks;
//   - ../lib/superview/, which is where the Linux package installs them.
//     They cannot go in the same directory as the app there: that is
//     /usr/local/bin, and an "ffmpeg" dropped in it would shadow the system's
//     for every program on the machine, not just this one.
func findBundledToolBinary(tool string) string {
	exePath, err := osExecutable()
	if err != nil {
		return ""
	}

	name := toolFileName(tool)
	dir := filepath.Dir(exePath)

	for _, candidate := range []string{
		filepath.Join(dir, name),
		filepath.Join(dir, "..", "lib", "superview", name),
	} {
		if path := usableToolFile(candidate); path != "" {
			return path
		}
	}
	return ""
}

// usableToolFile returns the path if it names a regular file, and "" otherwise.
//
// The regular-file check is not ceremony: a directory called "ffmpeg" next to
// the executable would otherwise be handed to exec.Command, which fails at
// encoding time with a permission error rather than here with a fallback.
func usableToolFile(path string) string {
	info, err := os.Stat(path)
	if err != nil || info.IsDir() {
		return ""
	}
	return path
}

func findWindowsToolBinary(tool string) string {
	exe := tool + ".exe"
	// Common install paths used by winget/scoop/manual installs.
	candidates := []string{
		filepath.Join(os.Getenv("LOCALAPPDATA"), "Microsoft", "WinGet", "Links", exe),
		filepath.Join(os.Getenv("ProgramFiles"), "ffmpeg", "bin", exe),
		filepath.Join(os.Getenv("ProgramFiles"), "FFmpeg", "bin", exe),
		filepath.Join(os.Getenv("USERPROFILE"), "scoop", "apps", "ffmpeg", "current", "bin", exe),
	}

	for _, path := range candidates {
		if path != "" {
			if info, err := os.Stat(path); err == nil && !info.IsDir() {
				return path
			}
		}
	}

	// Winget often extracts under LOCALAPPDATA/Microsoft/WinGet/Packages.
	packageRoot := filepath.Join(os.Getenv("LOCALAPPDATA"), "Microsoft", "WinGet", "Packages")
	dirs, err := os.ReadDir(packageRoot)
	if err != nil {
		return ""
	}

	lowerExe := strings.ToLower(exe)
	for _, dir := range dirs {
		if !dir.IsDir() {
			continue
		}
		name := strings.ToLower(dir.Name())
		if !strings.Contains(name, "ffmpeg") {
			continue
		}
		root := filepath.Join(packageRoot, dir.Name())
		var found string
		_ = filepath.WalkDir(root, func(path string, d os.DirEntry, walkErr error) error {
			if walkErr != nil || d == nil || d.IsDir() {
				return nil
			}
			if strings.EqualFold(d.Name(), lowerExe) {
				found = path
				return errors.New("tool found")
			}
			return nil
		})
		if found != "" {
			return found
		}
	}

	return ""
}

// CheckFfmpeg discovers the installed ffmpeg version, hardware accelerators, and available H.264/H.265 encoders.
// This function must be called before encoding to verify ffmpeg is installed and identify encoder options.
// Returns a map with keys: "version", "accels" (comma-separated), and "encoders" (comma-separated).
// cfg supplies EncoderCodecs; nil means the built-in defaults.
func CheckFfmpeg(cfg *Config) (map[string]string, error) {
	cfg = configOrDefault(cfg)
	ret := make(map[string]string)

	cmd := newFFmpegCommand("-version")
	prepareBackgroundCommand(cmd)
	version, err := cmd.CombinedOutput()

	if err != nil {
		return nil, errors.New("cannot find ffmpeg/ffprobe on your system\nmake sure to install it first: https://github.com/Canaill51/superview?tab=readme-ov-file#download-and-install")
	}

	ret["version"] = parseFFmpegVersion(string(version))

	// split on newline, skip first line
	cmd = newFFmpegCommand("-hwaccels", "-hide_banner")
	prepareBackgroundCommand(cmd)
	accels, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("failed to query ffmpeg hardware accelerators: %w", err)
	}
	accelsArr := strings.Split(strings.ReplaceAll(string(accels), "\r\n", "\n"), "\n")
	for i := 1; i < len(accelsArr); i++ {
		if len(accelsArr[i]) != 0 {
			ret["accels"] += accelsArr[i] + ","
		}
	}

	// split on newline, skip first 10 lines
	cmd = newFFmpegCommand("-encoders", "-hide_banner")
	prepareBackgroundCommand(cmd)
	encoders, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("failed to query ffmpeg encoders: %w", err)
	}
	ret["encoders"] = strings.Join(parseFFmpegEncoders(string(encoders), cfg.EncoderCodecs), ",")

	ret["accels"] = strings.Trim(ret["accels"], ",")

	return ret, nil
}

// parseFFmpegVersion extracts the version token from "ffmpeg version N.N.N ...".
//
// It never panics on an unexpected layout: a custom, patched or localised build
// must degrade to "unknown" rather than take the whole app down at startup.
func parseFFmpegVersion(output string) string {
	line, _, _ := strings.Cut(output, "\n")
	fields := strings.Fields(line)
	if len(fields) < 3 {
		return "unknown"
	}
	return fields[2]
}

// parseFFmpegEncoders extracts video encoder names matching any of the given
// codec fragments from the output of "ffmpeg -encoders".
//
// The listing is "<header>\n ------\n <flags> <name> <description>". Rather
// than skipping a hard-coded number of header lines, this scans for the "------"
// separator and falls back to accepting every well-formed entry when it is
// absent, so a build with a differently sized banner still works.
func parseFFmpegEncoders(output string, codecs []string) []string {
	lines := strings.Split(strings.ReplaceAll(output, "\r\n", "\n"), "\n")

	start := 0
	for i, line := range lines {
		if strings.Contains(line, "------") {
			start = i + 1
			break
		}
	}

	var found []string
	for _, line := range lines[start:] {
		// A video encoder row starts with a space then the "V" capability flag.
		if !strings.HasPrefix(line, " V") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		name := fields[1]
		for _, codec := range codecs {
			if strings.Contains(name, codec) {
				found = append(found, name)
				break
			}
		}
	}
	return found
}

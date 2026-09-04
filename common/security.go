package common

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"
	"strings"
)

// sensitiveSystemDirectories returns directories that must never be used
// as an output destination, regardless of whether the current process
// happens to have write access to them (e.g. running as root in CI/containers).
func sensitiveSystemDirectories() []string {
	if runtime.GOOS == "windows" {
		// Resolve from the environment: the system drive is not always C:.
		// The literals are kept as a fallback for a stripped environment.
		var dirs []string
		for _, key := range []string{"SystemRoot", "ProgramFiles", "ProgramFiles(x86)", "ProgramData"} {
			if v := strings.TrimSpace(os.Getenv(key)); v != "" {
				dirs = append(dirs, filepath.Clean(v))
			}
		}
		for _, fallback := range []string{
			`C:\Windows`,
			`C:\Program Files`,
			`C:\Program Files (x86)`,
			`C:\ProgramData`,
		} {
			if !containsPath(dirs, fallback) {
				dirs = append(dirs, fallback)
			}
		}
		return dirs
	}
	return []string{
		"/etc", "/bin", "/sbin", "/usr", "/boot",
		"/root", "/sys", "/proc", "/lib", "/lib64", "/var/lib",
	}
}

func containsPath(paths []string, target string) bool {
	for _, p := range paths {
		if pathEqual(p, target) {
			return true
		}
	}
	return false
}

// pathEqual compares two paths, case-insensitively on Windows.
func pathEqual(a, b string) bool {
	if runtime.GOOS == "windows" {
		return strings.EqualFold(a, b)
	}
	return a == b
}

// hasTraversalComponent reports whether any path component is exactly "..".
//
// Testing the raw string with strings.Contains rejected perfectly ordinary
// names such as "vacances..2024.mp4" or a directory called "GoPro..raw",
// while adding nothing: the path is required to be absolute and is passed
// through filepath.Clean right after, which already resolves traversal.
func hasTraversalComponent(filePath string) bool {
	normalized := strings.ReplaceAll(filePath, "\\", "/")
	for _, component := range strings.Split(normalized, "/") {
		if component == ".." {
			return true
		}
	}
	return false
}

func isSensitiveSystemPath(cleanPath string) bool {
	for _, dir := range sensitiveSystemDirectories() {
		if pathEqual(cleanPath, dir) {
			return true
		}
		prefix := dir + string(os.PathSeparator)
		if runtime.GOOS == "windows" {
			if len(cleanPath) >= len(prefix) && strings.EqualFold(cleanPath[:len(prefix)], prefix) {
				return true
			}
			continue
		}
		if strings.HasPrefix(cleanPath, prefix) {
			return true
		}
	}
	return false
}

// isValidInputPath validates that a file path is safe for input operations.
//
// Security checks:
//   - No ".." path components (directory traversal prevention)
//   - Must be an absolute path
//   - Symlinks are resolved, and it is the *target* that must satisfy the rest
//   - The resolved target must exist and be a regular file
//
// Symlinks used to be rejected outright. That blocked ordinary setups -- a
// ~/Videos pointing at a mounted drive, a symlinked /media entry -- while
// buying nothing: the user picks the file themselves through a native dialog,
// so there is no untrusted party to defend against. Resolving and then
// validating the destination keeps the guarantee that matters (we know exactly
// which file will be read) without the false positives.
func isValidInputPath(filePath string) error {
	if filePath == "" {
		return fmt.Errorf("file path cannot be empty")
	}

	// Reject ".." path components (not the substring: a file may legitimately
	// be named "clip..final.mp4").
	if hasTraversalComponent(filePath) {
		return fmt.Errorf("path traversal detected: %s", filePath)
	}

	// Normalize and validate
	cleanPath := filepath.Clean(filePath)

	// Require absolute path
	if !filepath.IsAbs(cleanPath) {
		return fmt.Errorf("path must be absolute: %s", filePath)
	}

	// Resolve symlinks and validate what they actually point at.
	resolvedPath, err := filepath.EvalSymlinks(cleanPath)
	if err != nil {
		return fmt.Errorf("cannot access file: %w", err)
	}

	// A symlink must not be a way to smuggle traversal back in.
	if !filepath.IsAbs(resolvedPath) {
		return fmt.Errorf("resolved path is not absolute: %s", resolvedPath)
	}

	stat, err := os.Stat(resolvedPath)
	if err != nil {
		return fmt.Errorf("cannot access file: %w", err)
	}

	if stat.IsDir() {
		return fmt.Errorf("path is a directory, expected file: %s", filePath)
	}

	// Reject anything that is not a regular file: a device, socket or FIFO
	// would make ffmpeg block or read something unintended.
	if !stat.Mode().IsRegular() {
		return fmt.Errorf("not a regular file: %s", filePath)
	}

	return nil
}

// isValidOutputPath validates that an output file path is safe for writing.
// It prevents directory traversal and ensures the output directory is writable.
// Security checks:
// - No ".." components (directory traversal prevention)
// - Must be an absolute path
// - Parent directory must exist and be writable
// - Does not check if file exists (OK to overwrite for output)
func isValidOutputPath(filePath string) error {
	if filePath == "" {
		return fmt.Errorf("output path cannot be empty")
	}

	// Reject ".." path components (not the substring).
	if hasTraversalComponent(filePath) {
		return fmt.Errorf("path traversal detected in output path: %s", filePath)
	}

	// Normalize and validate
	cleanPath := filepath.Clean(filePath)

	// Require absolute path
	if !filepath.IsAbs(cleanPath) {
		return fmt.Errorf("output path must be absolute: %s", filePath)
	}

	// Reject known system directories outright, independent of actual
	// write permissions (a process running with elevated rights must
	// still not be allowed to target these paths).
	if isSensitiveSystemPath(cleanPath) {
		return fmt.Errorf("output path targets a protected system directory: %s", filePath)
	}

	// Check that parent directory exists and is writable
	dir := filepath.Dir(cleanPath)
	dirStat, err := os.Stat(dir)
	if err != nil {
		return fmt.Errorf("cannot access output directory %s: %w", dir, err)
	}

	if !dirStat.IsDir() {
		return fmt.Errorf("output parent is not a directory: %s", dir)
	}

	// Probe writability by creating a uniquely named file rather than a fixed
	// one: two concurrent runs cannot clobber each other, and a crash between
	// creation and removal cannot leave a predictable leftover behind.
	probe, err := os.CreateTemp(dir, ".superview-write-*")
	if err != nil {
		return fmt.Errorf("output directory not writable: %w", err)
	}
	probeName := probe.Name()
	_ = probe.Close()
	_ = os.Remove(probeName)

	return nil
}

// SanitizeEncoderInput validates encoder selection against a whitelist.
// This prevents injection of arbitrary ffmpeg parameters.
// Returns sanitized encoder name or error if encoder is not in approved list.
func SanitizeEncoderInput(encoder string, availableEncoders string) (string, error) {
	if encoder == "" {
		return "", nil // Empty string is valid (use input codec)
	}

	// Whitelist check: encoder must be in available encoders list
	approvedEncoders := strings.Split(availableEncoders, ",")
	for _, approved := range approvedEncoders {
		approved = strings.TrimSpace(approved)
		if encoder == approved {
			return encoder, nil
		}
	}

	return "", fmt.Errorf("encoder %q not in approved list", encoder)
}

// ValidateVideoFile performs comprehensive security validation on input video file.
// It combines path validation with ffprobe metadata validation.
func ValidateVideoFile(filePath string) error {
	// First validate the path itself
	if err := isValidInputPath(filePath); err != nil {
		return err
	}

	// Then validate it's actually a video file by checking with ffprobe
	// This is done implicitly by CheckVideo() which will fail if not a valid video
	specs, err := CheckVideo(filePath)
	if err != nil {
		return fmt.Errorf("invalid video file: %w", err)
	}

	// Perform security validation of video metadata
	if err := specs.Validate(); err != nil {
		return fmt.Errorf("video validation failed: %w", err)
	}

	return nil
}

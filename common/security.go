package common

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
)

// isValidInputPath validates that a file path is safe for input operations.
// It prevents directory traversal attacks and ensures paths are absolute.
// Returns true only if the path is safe to use with file operations.
// Security checks:
// - No ".." components (directory traversal prevention)
// - Must be an absolute path
// - Must exist and be a regular file
// - Must not be a symlink to prevent symlink attacks
func isValidInputPath(filePath string) error {
	if filePath == "" {
		return fmt.Errorf("file path cannot be empty")
	}

	// Check for ".." before normalization to catch path traversal attempts
	if strings.Contains(filePath, "..") {
		return fmt.Errorf("path traversal detected: %s", filePath)
	}

	// Normalize and validate
	cleanPath := filepath.Clean(filePath)

	// Require absolute path
	if !filepath.IsAbs(cleanPath) {
		return fmt.Errorf("path must be absolute: %s", filePath)
	}

	// Check file exists
	stat, err := os.Stat(cleanPath)
	if err != nil {
		return fmt.Errorf("cannot access file: %w", err)
	}

	// Reject directories
	if stat.IsDir() {
		return fmt.Errorf("path is a directory, expected file: %s", filePath)
	}

	// Check if it's a symlink (potential symlink attack)
	// Note: lstat returns info about the symlink itself, not the target
	lstat, err := os.Lstat(cleanPath)
	if err != nil {
		return fmt.Errorf("cannot stat file: %w", err)
	}

	// Reject symlinks (they could point outside intended boundaries)
	if (lstat.Mode() & os.ModeSymlink) != 0 {
		return fmt.Errorf("symlinks not allowed for security: %s", filePath)
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

	// Check for ".." before normalization to catch path traversal attempts
	if strings.Contains(filePath, "..") {
		return fmt.Errorf("path traversal detected in output path: %s", filePath)
	}

	// Normalize and validate
	cleanPath := filepath.Clean(filePath)

	// Require absolute path
	if !filepath.IsAbs(cleanPath) {
		return fmt.Errorf("output path must be absolute: %s", filePath)
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

	// Check directory is writable by attempting to create a temp file
	// This is more reliable than checking mode bits
	testFile := filepath.Join(dir, ".superview_write_test")
	if err := os.WriteFile(testFile, []byte{}, 0600); err != nil {
		return fmt.Errorf("output directory not writable: %w", err)
	}
	_ = os.Remove(testFile) // Clean up test file

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

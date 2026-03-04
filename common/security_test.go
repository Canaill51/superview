package common

import (
	"os"
	"path/filepath"
	"testing"
)

// TestIsValidInputPath tests path validation for input files
func TestIsValidInputPath(t *testing.T) {
	tests := []struct {
		name      string
		path      string
		shouldErr bool
		desc      string
	}{
		{
			name:      "empty path",
			path:      "",
			shouldErr: true,
			desc:      "empty paths not allowed",
		},
		{
			name:      "relative path",
			path:      "video.mp4",
			shouldErr: true,
			desc:      "only absolute paths allowed",
		},
		{
			name:      "path traversal with ..",
			path:      "/home/user/../../../etc/passwd",
			shouldErr: true,
			desc:      "path traversal detected",
		},
		{
			name:      "directory instead of file",
			path:      "/tmp",
			shouldErr: true,
			desc:      "directories not allowed",
		},
		{
			name:      "nonexistent file",
			path:      "/nonexistent/file/path.mp4",
			shouldErr: true,
			desc:      "file must exist",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := isValidInputPath(tt.path)
			if (err != nil) != tt.shouldErr {
				t.Errorf("isValidInputPath(%s) error = %v, shouldErr %v (%s)",
					tt.path, err, tt.shouldErr, tt.desc)
			}
		})
	}
}

// TestIsValidOutputPath tests path validation for output files
func TestIsValidOutputPath(t *testing.T) {
	// Create temporary directory for testing
	tmpDir := t.TempDir()

	tests := []struct {
		name      string
		path      string
		shouldErr bool
		desc      string
	}{
		{
			name:      "empty path",
			path:      "",
			shouldErr: true,
			desc:      "empty paths not allowed",
		},
		{
			name:      "relative path",
			path:      "output.mp4",
			shouldErr: true,
			desc:      "only absolute paths allowed",
		},
		{
			name:      "path traversal",
			path:      filepath.Join(tmpDir, "..", "../../etc/output.mp4"),
			shouldErr: true,
			desc:      "path traversal detected",
		},
		{
			name:      "nonexistent parent directory",
			path:      "/nonexistent/directory/output.mp4",
			shouldErr: true,
			desc:      "parent directory must exist",
		},
		{
			name:      "valid output path",
			path:      filepath.Join(tmpDir, "output.mp4"),
			shouldErr: false,
			desc:      "valid absolute path in writable directory",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := isValidOutputPath(tt.path)
			if (err != nil) != tt.shouldErr {
				t.Errorf("isValidOutputPath(%s) error = %v, shouldErr %v (%s)",
					tt.path, err, tt.shouldErr, tt.desc)
			}
		})
	}
}

// TestSanitizeEncoderInput tests encoder input validation
func TestSanitizeEncoderInput(t *testing.T) {
	availableEncoders := "libx264,libx265,h264_nvenc,hevc_nvenc"

	tests := []struct {
		name      string
		encoder   string
		shouldErr bool
		expected  string
		desc      string
	}{
		{
			name:      "empty encoder (use input codec)",
			encoder:   "",
			shouldErr: false,
			expected:  "",
			desc:      "empty string is valid",
		},
		{
			name:      "valid encoder libx264",
			encoder:   "libx264",
			shouldErr: false,
			expected:  "libx264",
			desc:      "approved encoder",
		},
		{
			name:      "valid encoder libx265",
			encoder:   "libx265",
			shouldErr: false,
			expected:  "libx265",
			desc:      "approved encoder",
		},
		{
			name:      "invalid encoder injection attempt",
			encoder:   "libx264 -ssof /etc/passwd",
			shouldErr: true,
			expected:  "",
			desc:      "injection attempt rejected",
		},
		{
			name:      "encoder not in whitelist",
			encoder:   "mpeg4",
			shouldErr: true,
			expected:  "",
			desc:      "not approved",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := SanitizeEncoderInput(tt.encoder, availableEncoders)
			if (err != nil) != tt.shouldErr {
				t.Errorf("SanitizeEncoderInput(%s) error = %v, shouldErr %v (%s)",
					tt.encoder, err, tt.shouldErr, tt.desc)
			}
			if !tt.shouldErr && result != tt.expected {
				t.Errorf("SanitizeEncoderInput(%s) = %q, want %q",
					tt.encoder, result, tt.expected)
			}
		})
	}
}

// TestPathTraversalPrevention tests various path traversal techniques
func TestPathTraversalPrevention(t *testing.T) {
	traversalAttempts := []string{
		"/home/user/../../../etc/passwd",
		"/home/user/./../../etc/passwd",
		"/tmp/video/../../sensitive/file.txt",
		"/var/www/uploads/../../config.php",
		"/home/user/video/../../../etc/shadow",
	}

	for _, attempt := range traversalAttempts {
		t.Run("traversal_"+attempt, func(t *testing.T) {
			err := isValidInputPath(attempt)
			if err == nil {
				t.Errorf("Path traversal not detected: %s", attempt)
			}
			// Verify it's a traversal error or file not found (due to normalization)
			if err != nil {
				errMsg := err.Error()
				if !(contains(errMsg, "traversal") || contains(errMsg, "cannot access")) {
					t.Errorf("Expected path traversal or access error, got: %v", err)
				}
			}
		})
	}
}

// TestSymlinkRejection tests that symlinks are properly rejected
func TestSymlinkRejection(t *testing.T) {
	// Create a temporary file
	tmpFile := filepath.Join(t.TempDir(), "targetfile.txt")
	if err := os.WriteFile(tmpFile, []byte("test"), 0600); err != nil {
		t.Fatalf("Failed to create test file: %v", err)
	}

	// Create a symlink to the file
	tmpDir := filepath.Dir(tmpFile)
	symlinkPath := filepath.Join(tmpDir, "symlink.txt")
	if err := os.Symlink(tmpFile, symlinkPath); err != nil {
		t.Skipf("Cannot create symlinks on this system: %v", err)
	}

	// Verify the symlink is rejected
	err := isValidInputPath(symlinkPath)
	if err == nil {
		t.Fatal("Symlink was not rejected - security issue!")
	}
}

// Helper function to check if error message contains substring
func contains(s, substr string) bool {
	for i := 0; i <= len(s)-len(substr); i++ {
		if s[i:i+len(substr)] == substr {
			return true
		}
	}
	return false
}

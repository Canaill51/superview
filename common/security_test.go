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
func TestSymlinkResolution_AcceptedWhenTargetIsValid(t *testing.T) {
	// Policy decision (2026-09-04): symlinks are resolved and their target is
	// validated, instead of being rejected outright. A ~/Videos pointing at a
	// mounted drive is an ordinary setup, and the user picks the file through a
	// native dialog, so there is no untrusted party to defend against.
	dir := t.TempDir()
	target := filepath.Join(dir, "targetfile.mp4")
	if err := os.WriteFile(target, []byte("test"), 0600); err != nil {
		t.Fatalf("Failed to create test file: %v", err)
	}

	link := filepath.Join(dir, "symlink.mp4")
	if err := os.Symlink(target, link); err != nil {
		t.Skipf("Cannot create symlinks on this system: %v", err)
	}

	if err := isValidInputPath(link); err != nil {
		t.Errorf("a symlink to a valid regular file must be accepted, got: %v", err)
	}
}

func TestSymlinkResolution_RejectedWhenTargetIsMissing(t *testing.T) {
	dir := t.TempDir()
	link := filepath.Join(dir, "dangling.mp4")
	if err := os.Symlink(filepath.Join(dir, "does-not-exist.mp4"), link); err != nil {
		t.Skipf("Cannot create symlinks on this system: %v", err)
	}

	if err := isValidInputPath(link); err == nil {
		t.Error("a dangling symlink must be rejected")
	}
}

func TestSymlinkResolution_RejectedWhenTargetIsDirectory(t *testing.T) {
	dir := t.TempDir()
	targetDir := filepath.Join(dir, "somedir")
	if err := os.Mkdir(targetDir, 0755); err != nil {
		t.Fatalf("Failed to create dir: %v", err)
	}
	link := filepath.Join(dir, "dirlink")
	if err := os.Symlink(targetDir, link); err != nil {
		t.Skipf("Cannot create symlinks on this system: %v", err)
	}

	if err := isValidInputPath(link); err == nil {
		t.Error("a symlink to a directory must be rejected")
	}
}

func TestIsValidInputPath_RejectsNonRegularFile(t *testing.T) {
	// /dev/zero would make ffmpeg read forever.
	if _, err := os.Stat("/dev/zero"); err != nil {
		t.Skip("/dev/zero unavailable")
	}
	if err := isValidInputPath("/dev/zero"); err == nil {
		t.Error("a character device must be rejected")
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

func TestHasTraversalComponent(t *testing.T) {
	cases := []struct {
		path string
		want bool
	}{
		// Real traversal.
		{"/home/user/../../etc/passwd", true},
		{"/home/user/..", true},
		{"../relative.mp4", true},
		{`C:\Users\..\Windows\x.mp4`, true},
		// Legitimate names that the old substring check rejected.
		{"/home/user/Videos/vacances..2024.mp4", false},
		{"/home/user/GoPro..raw/clip.mp4", false},
		{"/home/user/a..b..c.mp4", false},
		{"/home/user/...hidden.mp4", false},
		{"/home/user/normal.mp4", false},
	}
	for _, c := range cases {
		if got := hasTraversalComponent(c.path); got != c.want {
			t.Errorf("hasTraversalComponent(%q) = %v, want %v", c.path, got, c.want)
		}
	}
}

func TestIsValidInputPath_AcceptsDoubleDotInFilename(t *testing.T) {
	dir := t.TempDir()
	file := filepath.Join(dir, "vacances..2024.mp4")
	if err := os.WriteFile(file, []byte("x"), 0600); err != nil {
		t.Fatalf("setup failed: %v", err)
	}
	if err := isValidInputPath(file); err != nil {
		t.Errorf("a filename containing '..' must be accepted, got: %v", err)
	}
}

func TestIsValidInputPath_StillRejectsTraversal(t *testing.T) {
	if err := isValidInputPath("/home/user/../../etc/passwd"); err == nil {
		t.Error("expected traversal to be rejected")
	}
}

func TestIsValidOutputPath_LeavesNoProbeFileBehind(t *testing.T) {
	dir := t.TempDir()
	if err := isValidOutputPath(filepath.Join(dir, "out.mp4")); err != nil {
		t.Fatalf("expected a writable temp dir to pass, got: %v", err)
	}
	entries, err := os.ReadDir(dir)
	if err != nil {
		t.Fatalf("failed to read dir: %v", err)
	}
	for _, e := range entries {
		t.Errorf("probe file left behind: %s", e.Name())
	}
}

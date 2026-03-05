package common

import (
	"fmt"
	"os/exec"
	"runtime"
	"testing"
)

func commandExitWithCode(code int) *exec.Cmd {
	if runtime.GOOS == "windows" {
		return exec.Command("cmd", "/c", "exit", fmt.Sprintf("%d", code))
	}
	return exec.Command("sh", "-c", fmt.Sprintf("exit %d", code))
}

// ============================================================================
// Tests for NormalizeNativeDialogResult
// ============================================================================

func TestNormalizeNativeDialogResult_Success(t *testing.T) {
	path, err := NormalizeNativeDialogResult("/tmp/video.mp4", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if path != "/tmp/video.mp4" {
		t.Errorf("expected /tmp/video.mp4, got %q", path)
	}
}

func TestNormalizeNativeDialogResult_TrimsWhitespace(t *testing.T) {
	path, err := NormalizeNativeDialogResult("  /tmp/video.mp4  \n", nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if path != "/tmp/video.mp4" {
		t.Errorf("expected trimmed path, got %q", path)
	}
}

func TestNormalizeNativeDialogResult_CancellationExitCode1(t *testing.T) {
	// Simulate a cancelled dialog: exit code 1 means user cancelled
	cmd := commandExitWithCode(1)
	err := cmd.Run()

	path, normErr := NormalizeNativeDialogResult("", err)
	if normErr != nil {
		t.Errorf("exit code 1 should be treated as cancellation, not error: %v", normErr)
	}
	if path != "" {
		t.Errorf("expected empty path on cancellation, got %q", path)
	}
}

func TestNormalizeNativeDialogResult_UnexpectedError(t *testing.T) {
	// Simulate an unexpected non-zero exit code (not 1 or 255)
	cmd := commandExitWithCode(2)
	err := cmd.Run()

	_, normErr := NormalizeNativeDialogResult("", err)
	if normErr == nil {
		t.Error("expected error for unexpected exit code 2, got nil")
	}
}

func TestNormalizeNativeDialogResult_NonExitError(t *testing.T) {
	genericErr := fmt.Errorf("some generic error")
	_, normErr := NormalizeNativeDialogResult("", genericErr)
	if normErr == nil {
		t.Error("expected error for non-exit error, got nil")
	}
}

// ============================================================================
// Tests for ParseEncoderSelection
// ============================================================================

func TestParseEncoderSelection_Empty(t *testing.T) {
	got := ParseEncoderSelection("")
	if got != "" {
		t.Errorf("expected empty string, got %q", got)
	}
}

func TestParseEncoderSelection_UseInputCodec(t *testing.T) {
	got := ParseEncoderSelection("Use same video codec as input file")
	if got != "" {
		t.Errorf("expected empty string for 'use input codec', got %q", got)
	}
}

func TestParseEncoderSelection_UseInputCodecWithPadding(t *testing.T) {
	// The GUI centerLabel helper wraps option text with spaces for centering
	got := ParseEncoderSelection("   Use same video codec as input file   ")
	if got != "" {
		t.Errorf("expected empty string for padded 'use input codec', got %q", got)
	}
}

func TestParseEncoderSelection_EncoderWithSuffix(t *testing.T) {
	// GUI appends " encoder" to each option
	got := ParseEncoderSelection("libx264 encoder")
	if got != "libx264" {
		t.Errorf("expected libx264, got %q", got)
	}
}

func TestParseEncoderSelection_EncoderWithPaddingAndSuffix(t *testing.T) {
	got := ParseEncoderSelection("   libx265 encoder   ")
	if got != "libx265" {
		t.Errorf("expected libx265, got %q", got)
	}
}

func TestParseEncoderSelection_HardwareEncoder(t *testing.T) {
	got := ParseEncoderSelection("h264_nvenc encoder")
	if got != "h264_nvenc" {
		t.Errorf("expected h264_nvenc, got %q", got)
	}
}

func TestParseEncoderSelection_WhitespaceOnly(t *testing.T) {
	got := ParseEncoderSelection("   ")
	if got != "" {
		t.Errorf("expected empty string for whitespace-only input, got %q", got)
	}
}

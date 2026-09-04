package common

import (
	"strings"
	"testing"
	"time"
)

func TestNewEncodingMetrics(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	if m == nil {
		t.Fatal("expected metrics instance")
	}
	if m.InputFile != "in.mp4" || m.OutputFile != "out.mp4" {
		t.Fatalf("unexpected files: %s -> %s", m.InputFile, m.OutputFile)
	}
	if m.StartTime.IsZero() {
		t.Fatal("expected non-zero StartTime")
	}
}

func TestEncodingMetrics_RecordAndCompute(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	video := &VideoSpecs{
		Streams: []VideoStream{{
			Codec:         "h264",
			Width:         1920,
			Height:        1080,
			DurationFloat: 10,
			BitrateInt:    1000000,
			FrameRate:     60,
		}},
	}

	m.RecordInputMetadata(video, 2000000)
	m.RecordOutputMetadata(500000, "libx264")
	m.RecordProgress(50)
	time.Sleep(5 * time.Millisecond)
	m.RecordCompletion(1000000)

	if !m.Success {
		t.Fatal("expected success=true")
	}
	if m.OutputFileSize != 1000000 {
		t.Fatalf("unexpected output size: %d", m.OutputFileSize)
	}
	if m.CompressionRatio <= 0 {
		t.Fatalf("expected compression ratio > 0, got %f", m.CompressionRatio)
	}
	if m.BitrateReduction <= 0 {
		t.Fatalf("expected bitrate reduction > 0, got %f", m.BitrateReduction)
	}
	if m.EncodingSpeed <= 0 {
		t.Fatalf("expected encoding speed > 0, got %f", m.EncodingSpeed)
	}
	// 10 s at 60 fps is 600 frames; the whole run took milliseconds, so the
	// figure must be far above the 300 the old hard-coded 30 fps would give.
	if m.EncodingSpeed < 600 {
		t.Errorf("encoding speed = %f, too low for 600 frames in a few ms: the frame rate is being guessed", m.EncodingSpeed)
	}
}

// TestEncodingMetrics_UnknownFrameRateLeavesSpeedUnpublished pins P-06.
//
// The frame count used to be InputDuration * 30 with a comment admitting the
// assumption. GoPros record at 60, 120 and 240, so the published speed could be
// wrong by a factor of 8 while looking exactly as trustworthy as the measured
// values beside it. When ffprobe cannot report a rate, no number is better than
// an invented one.
func TestEncodingMetrics_UnknownFrameRateLeavesSpeedUnpublished(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	video := &VideoSpecs{Streams: []VideoStream{{
		Codec: "h264", Width: 1920, Height: 1080,
		DurationFloat: 10, BitrateInt: 1000000,
		// FrameRate deliberately left at zero: ffprobe reported nothing usable.
	}}}

	m.RecordInputMetadata(video, 2000000)
	m.RecordOutputMetadata(500000, "libx264")
	time.Sleep(5 * time.Millisecond)
	m.RecordCompletion(1000000)

	if m.EncodingSpeed != 0 {
		t.Errorf("EncodingSpeed = %f, want 0 when the frame rate is unknown", m.EncodingSpeed)
	}
	// The metrics that do not depend on the frame rate must still be there.
	if m.CompressionRatio <= 0 {
		t.Errorf("CompressionRatio = %f, want > 0", m.CompressionRatio)
	}
}

func TestEncodingMetrics_RecordProgressClamp(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.RecordProgress(150)
	if m.LastProgress != 100 {
		t.Fatalf("expected clamped progress 100, got %f", m.LastProgress)
	}
	m.RecordProgress(-10)
	if m.LastProgress != 0 {
		t.Fatalf("expected clamped progress 0, got %f", m.LastProgress)
	}
}

func TestEncodingMetrics_RecordErrorAndSummary(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.RecordError(234, "ffmpeg failed")
	if m.Success {
		t.Fatal("expected success=false")
	}
	if m.FfmpegExitCode != 234 {
		t.Fatalf("unexpected exit code: %d", m.FfmpegExitCode)
	}
	summary := m.Summary()
	if !strings.Contains(summary, "FAILED") || !strings.Contains(summary, "ffmpeg failed") {
		t.Fatalf("summary missing error info: %s", summary)
	}
}

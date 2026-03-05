package common

import (
	"encoding/json"
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

func TestEncodingMetrics_ToJSON(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	video := &VideoSpecs{Streams: []VideoStream{{Codec: "h264", DurationFloat: 1, BitrateInt: 800000}}}
	m.RecordInputMetadata(video, 1000)
	m.RecordOutputMetadata(400000, "libx264")
	m.RecordCompletion(500)

	payload := m.ToJSON()
	var parsed map[string]interface{}
	if err := json.Unmarshal([]byte(payload), &parsed); err != nil {
		t.Fatalf("invalid json: %v", err)
	}

	success, ok := parsed["success"].(bool)
	if !ok || !success {
		t.Fatalf("expected success=true in json, got %v", parsed["success"])
	}
}

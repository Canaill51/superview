package common

import (
	"bytes"
	"log/slog"
	"strings"
	"testing"
)

// withoutFFmpegOnPath empties PATH and clears the resolution cache, so the
// health checks have to conclude that ffmpeg is missing.
//
// It returns false when the tools are still reachable anyway -- on Windows
// resolveToolBinary falls back to the winget and scoop install directories,
// which PATH does not govern. That is a real feature, not a test problem, so
// the caller skips rather than asserting something untrue.
func withoutFFmpegOnPath(t *testing.T) bool {
	t.Helper()

	t.Setenv("PATH", t.TempDir())
	ResetToolResolutionCache()
	// Later tests resolve ffmpeg for real; leaving a negative cached would make
	// them fail for a reason that has nothing to do with them.
	t.Cleanup(ResetToolResolutionCache)

	_, err := CheckFfmpeg(nil)
	return err != nil
}

// TestCheckHealth_ReportsEveryCheck runs the Diagnostic button's whole path.
//
// The README asks users to attach this report to a bug report, which makes it
// the first thing read on any defect -- and until now nothing ran CheckHealth
// end to end. Only the individual CPU, memory and disk probes had tests, and
// GetHealthReport was only ever fed structs built by hand in the test itself.
func TestCheckHealth_ReportsEveryCheck(t *testing.T) {
	health := CheckHealth(nil)
	if health == nil {
		t.Fatal("CheckHealth returned nil")
	}

	// Every check the struct names must also appear in AllChecks: the report
	// and LogHealth both iterate that slice, so a check missing from it is a
	// check the user never sees.
	want := []string{"ffmpeg", "ffprobe", "disk_space", "memory", "cpu"}
	if len(health.AllChecks) != len(want) {
		t.Fatalf("AllChecks has %d entries, want %d: %+v",
			len(health.AllChecks), len(want), health.AllChecks)
	}
	for i, name := range want {
		if health.AllChecks[i].Name != name {
			t.Errorf("AllChecks[%d].Name = %q, want %q", i, health.AllChecks[i].Name, name)
		}
		if health.AllChecks[i].Timestamp == 0 {
			t.Errorf("AllChecks[%d] (%s) has no timestamp", i, name)
		}
	}

	// Overall is defined as the three critical checks, and deliberately does
	// not include memory or CPU -- a busy machine is slow, not broken.
	wantOverall := health.FFmpeg.Healthy && health.FFprobe.Healthy && health.Disk.Healthy
	if health.Overall != wantOverall {
		t.Errorf("Overall = %v, want %v", health.Overall, wantOverall)
	}
}

// TestCheckHealth_WithFFmpegInstalled pins what a healthy machine looks like.
func TestCheckHealth_WithFFmpegInstalled(t *testing.T) {
	if _, err := CheckFfmpeg(nil); err != nil {
		skipWithoutFFmpeg(t, "CheckFfmpeg failed: %v", err)
	}

	health := CheckHealth(nil)

	if !health.FFmpeg.Healthy {
		t.Errorf("FFmpeg check unhealthy with ffmpeg installed: %s", health.FFmpeg.Message)
	}
	if health.FFmpeg.Value == "" {
		t.Error("FFmpeg check reports no version; the report would say which build only by omission")
	}
	if !health.FFprobe.Healthy {
		t.Errorf("FFprobe check unhealthy with ffprobe installed: %s", health.FFprobe.Message)
	}
	if health.FFprobe.Value == "" {
		t.Error("FFprobe check reports no version line")
	}

	report := GetHealthReport(health)
	for _, want := range []string{"FFMPEG", "FFPROBE", "DISK_SPACE", "MEMORY", "CPU"} {
		if !strings.Contains(report, want) {
			t.Errorf("report omits %s:\n%s", want, report)
		}
	}
	if !strings.Contains(report, health.FFmpeg.Value) {
		t.Errorf("report omits the ffmpeg version %q:\n%s", health.FFmpeg.Value, report)
	}
}

// TestCheckHealth_WithoutFFmpeg is the case the report exists for.
//
// A user filing a bug because nothing encodes needs the report to say ffmpeg
// is missing. This path had never run.
func TestCheckHealth_WithoutFFmpeg(t *testing.T) {
	if !withoutFFmpegOnPath(t) {
		t.Skip("ffmpeg is still resolvable with an empty PATH (platform install directories)")
	}

	health := CheckHealth(nil)

	if health.FFmpeg.Healthy {
		t.Error("FFmpeg check healthy with an empty PATH")
	}
	if health.FFmpeg.Message == "" {
		t.Error("FFmpeg check failed without saying why; the report would show an empty line")
	}
	if health.FFprobe.Healthy {
		t.Error("FFprobe check healthy with an empty PATH")
	}
	if health.Overall {
		t.Error("Overall healthy while ffmpeg is missing")
	}

	report := GetHealthReport(health)
	if !strings.Contains(report, "UNHEALTHY") {
		t.Errorf("report does not say UNHEALTHY:\n%s", report)
	}
	if !strings.Contains(report, health.FFmpeg.Message) {
		t.Errorf("report omits the ffmpeg failure message %q:\n%s", health.FFmpeg.Message, report)
	}
}

// TestLogHealth_WritesFailuresAtWarn checks the other half of the diagnostic:
// the log line, which the README also asks for on a bug report.
func TestLogHealth_WritesFailuresAtWarn(t *testing.T) {
	health := &SystemHealth{
		Overall: false,
		FFmpeg:  HealthCheckResult{Name: "ffmpeg", Healthy: false, Message: "cannot find ffmpeg", Value: ""},
		FFprobe: HealthCheckResult{Name: "ffprobe", Healthy: true},
		Disk:    HealthCheckResult{Name: "disk_space", Healthy: false, Message: "2GB free", Value: "2"},
		Memory:  HealthCheckResult{Name: "memory", Healthy: true},
		CPU:     HealthCheckResult{Name: "cpu", Healthy: true},
	}
	health.AllChecks = []HealthCheckResult{health.FFmpeg, health.FFprobe, health.Disk, health.Memory, health.CPU}

	var buf bytes.Buffer
	LogHealth(slog.New(slog.NewTextHandler(&buf, &slog.HandlerOptions{Level: slog.LevelDebug})), health)
	got := buf.String()

	if !strings.Contains(got, "UNHEALTHY") {
		t.Errorf("log does not report the overall status:\n%s", got)
	}
	// The failing checks, and only those, get their own warning line.
	if !strings.Contains(got, "cannot find ffmpeg") {
		t.Errorf("log omits the ffmpeg failure:\n%s", got)
	}
	if !strings.Contains(got, "2GB free") {
		t.Errorf("log omits the disk failure:\n%s", got)
	}
	if strings.Contains(got, "ffprobe check failed") {
		t.Errorf("log warns about a check that passed:\n%s", got)
	}

	// A nil health is what the GUI holds before the first Diagnostic run.
	LogHealth(slog.New(slog.NewTextHandler(&buf, nil)), nil)
}

package common

import (
	"log/slog"
	"strings"
	"testing"
	"time"
)

func TestGetHealthReportNil(t *testing.T) {
	got := GetHealthReport(nil)
	if got != "No health data available" {
		t.Fatalf("unexpected report: %s", got)
	}
}

func TestGetHealthReportFormatting(t *testing.T) {
	health := &SystemHealth{
		Overall: false,
		AllChecks: []HealthCheckResult{
			{Name: "ffmpeg", Healthy: true, Message: "ok", Value: "6.1.1"},
			{Name: "disk_space", Healthy: false, Message: "low disk", Value: "1.0 GB free"},
		},
	}

	report := GetHealthReport(health)
	if !strings.Contains(report, "System Health Check") {
		t.Fatalf("report header missing: %s", report)
	}
	if !strings.Contains(report, "FFMPEG") || !strings.Contains(report, "DISK_SPACE") {
		t.Fatalf("expected check names in report: %s", report)
	}
	if !strings.Contains(report, "❌") {
		t.Fatalf("expected unhealthy marker in report: %s", report)
	}
}

func TestCheckCPUHealth(t *testing.T) {
	ts := time.Now().Unix()
	result := checkCPUHealth(ts)
	if result.Name != "cpu" {
		t.Fatalf("unexpected name: %s", result.Name)
	}
	if result.Timestamp != ts {
		t.Fatalf("unexpected timestamp: %d", result.Timestamp)
	}
	if result.Value == "" || result.Message == "" {
		t.Fatal("expected value and message")
	}
}

func TestCheckMemoryHealth(t *testing.T) {
	ts := time.Now().Unix()
	result := checkMemoryHealth(ts)
	if result.Name != "memory" {
		t.Fatalf("unexpected name: %s", result.Name)
	}
	if result.Timestamp != ts {
		t.Fatalf("unexpected timestamp: %d", result.Timestamp)
	}
	if result.Value == "" || result.Message == "" {
		t.Fatal("expected value and message")
	}
}

func TestCheckDiskSpaceHealth(t *testing.T) {
	ts := time.Now().Unix()
	result := checkDiskSpaceHealth(ts)
	if result.Name != "disk_space" {
		t.Fatalf("unexpected name: %s", result.Name)
	}
	if result.Timestamp != ts {
		t.Fatalf("unexpected timestamp: %d", result.Timestamp)
	}
	if result.Message == "" {
		t.Fatal("expected non-empty message")
	}
}

func TestLogHealthWithNil(t *testing.T) {
	logger := slog.Default()
	LogHealth(logger, nil)
}

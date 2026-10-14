package main

import (
	"io"
	"log/slog"
	"os"
	"path/filepath"
	"testing"
)

// TestLoadStartupConfig_NeverNil pins the start-up fallback once LoadConfig can
// refuse (E-21): with SUPERVIEW_MIN_BITRATE above SUPERVIEW_MAX_BITRATE even
// LoadConfig("") fails, and the window used to dereference the nil it returned.
func TestLoadStartupConfig_NeverNil(t *testing.T) {
	quiet := slog.New(slog.NewTextHandler(io.Discard, nil))

	t.Run("file refused, environment used", func(t *testing.T) {
		path := filepath.Join(t.TempDir(), "superview.yaml")
		if err := os.WriteFile(path, []byte("video-preset: slow\n"), 0o600); err != nil {
			t.Fatal(err)
		}
		t.Setenv("SUPERVIEW_LOG_LEVEL", "debug")
		cfg := loadStartupConfig(path, quiet)
		if cfg == nil || cfg.LogLevel != "debug" {
			t.Fatalf("a refused file did not fall back to the environment: %+v", cfg)
		}
	})

	t.Run("environment refused, defaults used", func(t *testing.T) {
		t.Setenv("SUPERVIEW_MIN_BITRATE", "500")
		t.Setenv("SUPERVIEW_MAX_BITRATE", "100")
		cfg := loadStartupConfig("", quiet)
		if cfg == nil {
			t.Fatal("loadStartupConfig returned nil")
		}
		if cfg.MinBitrate > cfg.MaxBitrate {
			t.Errorf("the inconsistent bounds survived: min %d, max %d", cfg.MinBitrate, cfg.MaxBitrate)
		}
	})
}

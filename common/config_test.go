package common

import (
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// ============================================================================
// Tests for Configuration Loading
// ============================================================================

func TestLoadConfig_DefaultValues(t *testing.T) {
	// When no file is provided, should use defaults
	cfg, err := LoadConfig("")
	if err != nil {
		t.Errorf("LoadConfig(\"\") failed: %v", err)
	}

	if cfg.MinBitrate != 102400 {
		t.Errorf("Expected MinBitrate=102400, got %d", cfg.MinBitrate)
	}

	if cfg.MaxBitrate != 52428800 {
		t.Errorf("Expected MaxBitrate=52428800, got %d", cfg.MaxBitrate)
	}

	if cfg.LogLevel != "info" {
		t.Errorf("Expected LogLevel=info, got %s", cfg.LogLevel)
	}
}

func TestLoadConfig_NonexistentFile(t *testing.T) {
	// When file doesn't exist, should use defaults without error
	cfg, err := LoadConfig("/nonexistent/path/config.yaml")
	if err != nil {
		t.Errorf("LoadConfig with nonexistent file should not error: %v", err)
	}

	if cfg.MinBitrate != 102400 {
		t.Errorf("Expected MinBitrate defaults when file not found")
	}
}

func TestLoadConfig_FromYAML(t *testing.T) {
	// Create a temporary config file
	tmpFile, err := os.CreateTemp(t.TempDir(), "config-*.yaml")
	if err != nil {
		t.Fatalf("Failed to create temp file: %v", err)
	}
	defer tmpFile.Close()

	// Write test config
	configContent := `
min_bitrate: 256000
max_bitrate: 20000000
log_level: debug
temp_dir_prefix: "custom-*"
encoder_codecs:
  - "264"
`

	if _, err := tmpFile.WriteString(configContent); err != nil {
		t.Fatalf("Failed to write config: %v", err)
	}

	cfg, err := LoadConfig(tmpFile.Name())
	if err != nil {
		t.Errorf("LoadConfig from file failed: %v", err)
	}

	if cfg.MinBitrate != 256000 {
		t.Errorf("Expected MinBitrate=256000, got %d", cfg.MinBitrate)
	}

	if cfg.MaxBitrate != 20000000 {
		t.Errorf("Expected MaxBitrate=20000000, got %d", cfg.MaxBitrate)
	}

	if cfg.LogLevel != "debug" {
		t.Errorf("Expected LogLevel=debug, got %s", cfg.LogLevel)
	}

	if cfg.TempDirPrefix != "custom-*" {
		t.Errorf("Expected TempDirPrefix=custom-*, got %s", cfg.TempDirPrefix)
	}
}

func TestLoadConfig_EnvironmentOverrides(t *testing.T) {
	// Set environment variables
	t.Setenv("SUPERVIEW_MIN_BITRATE", "131072")
	t.Setenv("SUPERVIEW_MAX_BITRATE", "26214400")
	t.Setenv("SUPERVIEW_LOG_LEVEL", "warn")

	cfg, err := LoadConfig("")
	if err != nil {
		t.Errorf("LoadConfig with env vars failed: %v", err)
	}

	if cfg.MinBitrate != 131072 {
		t.Errorf("Expected MinBitrate=131072 from env, got %d", cfg.MinBitrate)
	}

	if cfg.MaxBitrate != 26214400 {
		t.Errorf("Expected MaxBitrate=26214400 from env, got %d", cfg.MaxBitrate)
	}

	if cfg.LogLevel != "warn" {
		t.Errorf("Expected LogLevel=warn from env, got %s", cfg.LogLevel)
	}
}

func TestLoadConfig_EnvironmentOverridesYAML(t *testing.T) {
	// Create a temporary config file
	tmpFile, err := os.CreateTemp(t.TempDir(), "config-*.yaml")
	if err != nil {
		t.Fatalf("Failed to create temp file: %v", err)
	}
	defer tmpFile.Close()

	configContent := `
min_bitrate: 102400
max_bitrate: 52428800
log_level: info
`

	if _, err := tmpFile.WriteString(configContent); err != nil {
		t.Fatalf("Failed to write config: %v", err)
	}

	// Override with env vars
	t.Setenv("SUPERVIEW_MIN_BITRATE", "204800")
	t.Setenv("SUPERVIEW_LOG_LEVEL", "error")

	cfg, err := LoadConfig(tmpFile.Name())
	if err != nil {
		t.Errorf("LoadConfig with YAML + env override failed: %v", err)
	}

	// Env var should override YAML file
	if cfg.MinBitrate != 204800 {
		t.Errorf("Expected MinBitrate=204800 (env override), got %d", cfg.MinBitrate)
	}

	// This value should stay from YAML since no env override
	if cfg.MaxBitrate != 52428800 {
		t.Errorf("Expected MaxBitrate=52428800 (from file), got %d", cfg.MaxBitrate)
	}

	// Env var should override
	if cfg.LogLevel != "error" {
		t.Errorf("Expected LogLevel=error (env override), got %s", cfg.LogLevel)
	}
}

func TestCreateDefaultConfig(t *testing.T) {
	tmpDir := t.TempDir()
	configPath := filepath.Join(tmpDir, "superview.yaml")

	err := CreateDefaultConfig(configPath)
	if err != nil {
		t.Errorf("CreateDefaultConfig failed: %v", err)
	}

	// Verify file was created
	if _, err := os.Stat(configPath); err != nil {
		t.Errorf("Config file not created: %v", err)
	}

	// Load the created file
	cfg, err := LoadConfig(configPath)
	if err != nil {
		t.Errorf("Failed to load created config: %v", err)
	}

	if cfg.MinBitrate != 102400 {
		t.Errorf("Created config has wrong MinBitrate: %d", cfg.MinBitrate)
	}
}

func TestGetConfig_SetConfig(t *testing.T) {
	// Create custom config
	customCfg := &Config{
		MinBitrate: 512000,
		MaxBitrate: 10000000,
		LogLevel:   "debug",
	}

	SetConfig(customCfg)
	retrieved := GetConfig()

	if retrieved.MinBitrate != customCfg.MinBitrate {
		t.Errorf("GetConfig returned different MinBitrate")
	}

	if retrieved.LogLevel != customCfg.LogLevel {
		t.Errorf("GetConfig returned different LogLevel")
	}

	// Reset to default
	SetConfig(nil) // Setting nil should not change config
	if GetConfig().MinBitrate != 512000 {
		t.Errorf("SetConfig(nil) should not change config")
	}
}

func TestConfig_String(t *testing.T) {
	cfg := &Config{
		MinBitrate: 102400,
		MaxBitrate: 52428800,
		LogLevel:   "info",
	}

	str := cfg.String()
	if str == "" {
		t.Errorf("Config.String() returned empty string")
	}

	// Check that key info is in the string
	if !strings.Contains(str, "Min Bitrate") {
		t.Errorf("Config.String() missing 'Min Bitrate'")
	}

	if !strings.Contains(str, "Log Level") {
		t.Errorf("Config.String() missing 'Log Level'")
	}
}

func TestValidateBitrate_WithConfig(t *testing.T) {
	// Set custom config
	customCfg := &Config{
		MinBitrate: 256000,
		MaxBitrate: 10000000,
	}
	SetConfig(customCfg)

	// Test with config values
	tests := []struct {
		name    string
		bitrate int
		wantErr bool
	}{
		{"valid bitrate", 5000000, false},
		{"below custom min", 100000, true},   // Below 256000
		{"above custom max", 50000000, true}, // Above 10000000
		{"at custom min", 256000, false},
		{"at custom max", 10000000, false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateBitrate(tt.bitrate, customCfg.MinBitrate, customCfg.MaxBitrate)
			if (err != nil) != tt.wantErr {
				t.Errorf("ValidateBitrate with custom config failed")
			}
		})
	}

	// Reset to default
	SetConfig(defaultConfig)
}

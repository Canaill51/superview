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

	if cfg.MaxBitrate != 209715200 {
		t.Errorf("Expected MaxBitrate=209715200 (200M bytes/sec), got %d", cfg.MaxBitrate)
	}

	if cfg.LogLevel != "info" {
		t.Errorf("Expected LogLevel=info, got %s", cfg.LogLevel)
	}

	if cfg.PerformanceMode != "safe" {
		t.Errorf("Expected PerformanceMode=safe, got %s", cfg.PerformanceMode)
	}

	if cfg.VideoPreset != "" {
		t.Errorf("Expected VideoPreset empty, got %s", cfg.VideoPreset)
	}

	if cfg.FilterThreads != 0 {
		t.Errorf("Expected FilterThreads=0, got %d", cfg.FilterThreads)
	}

	if cfg.EncoderThreads != 0 {
		t.Errorf("Expected EncoderThreads=0, got %d", cfg.EncoderThreads)
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
	defer func() { _ = tmpFile.Close() }()

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
	t.Setenv("SUPERVIEW_PERFORMANCE_MODE", "safe_performance")
	t.Setenv("SUPERVIEW_VIDEO_PRESET", "fast")
	t.Setenv("SUPERVIEW_FILTER_THREADS", "4")
	t.Setenv("SUPERVIEW_ENCODER_THREADS", "8")

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

	if cfg.PerformanceMode != "safe_performance" {
		t.Errorf("Expected PerformanceMode=safe_performance from env, got %s", cfg.PerformanceMode)
	}

	if cfg.VideoPreset != "fast" {
		t.Errorf("Expected VideoPreset=fast from env, got %s", cfg.VideoPreset)
	}

	if cfg.FilterThreads != 4 {
		t.Errorf("Expected FilterThreads=4 from env, got %d", cfg.FilterThreads)
	}

	if cfg.EncoderThreads != 8 {
		t.Errorf("Expected EncoderThreads=8 from env, got %d", cfg.EncoderThreads)
	}
}

func TestLoadConfig_InvalidPerformanceModeFallsBackToSafe(t *testing.T) {
	t.Setenv("SUPERVIEW_PERFORMANCE_MODE", "turbo")

	cfg, err := LoadConfig("")
	if err != nil {
		t.Errorf("LoadConfig with invalid performance mode failed: %v", err)
	}

	if cfg.PerformanceMode != "safe" {
		t.Errorf("Expected PerformanceMode=safe fallback, got %s", cfg.PerformanceMode)
	}
}

func TestLoadConfig_InvalidFfmpegTuningValuesFallbackToDefaults(t *testing.T) {
	t.Setenv("SUPERVIEW_VIDEO_PRESET", "turbo")
	t.Setenv("SUPERVIEW_FILTER_THREADS", "-1")
	t.Setenv("SUPERVIEW_ENCODER_THREADS", "-2")

	cfg, err := LoadConfig("")
	if err != nil {
		t.Errorf("LoadConfig with invalid tuning values failed: %v", err)
	}

	if cfg.VideoPreset != "" {
		t.Errorf("Expected VideoPreset empty fallback, got %s", cfg.VideoPreset)
	}

	if cfg.FilterThreads != 0 {
		t.Errorf("Expected FilterThreads=0 fallback, got %d", cfg.FilterThreads)
	}

	if cfg.EncoderThreads != 0 {
		t.Errorf("Expected EncoderThreads=0 fallback, got %d", cfg.EncoderThreads)
	}
}

func TestLoadConfig_EnvironmentOverridesYAML(t *testing.T) {
	// Create a temporary config file
	tmpFile, err := os.CreateTemp(t.TempDir(), "config-*.yaml")
	if err != nil {
		t.Fatalf("Failed to create temp file: %v", err)
	}
	defer func() { _ = tmpFile.Close() }()

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

func TestConfigOrDefault(t *testing.T) {
	// The mutable package-level config was removed: callers now pass a *Config
	// explicitly, and nil means "use the built-in defaults".
	custom := &Config{MinBitrate: 512000, MaxBitrate: 10000000, LogLevel: "debug"}
	if got := configOrDefault(custom); got != custom {
		t.Error("a non-nil config must be returned unchanged")
	}

	fallback := configOrDefault(nil)
	if fallback == nil {
		t.Fatal("configOrDefault(nil) must not return nil")
	}
	if fallback.MinBitrate != defaultConfig.MinBitrate ||
		fallback.TempDirPrefix != defaultConfig.TempDirPrefix {
		t.Errorf("configOrDefault(nil) should yield the defaults, got %+v", fallback)
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
	customCfg := &Config{
		MinBitrate: 256000,
		MaxBitrate: 10000000,
	}

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
}

func TestConfigCandidatePaths_OrderAndOverride(t *testing.T) {
	t.Setenv("SUPERVIEW_CONFIG", "/explicit/override.yaml")

	candidates := configCandidatePaths()
	if len(candidates) < 2 {
		t.Fatalf("expected at least 2 candidates, got %d: %v", len(candidates), candidates)
	}
	if candidates[0] != "/explicit/override.yaml" {
		t.Errorf("SUPERVIEW_CONFIG must win, got %q", candidates[0])
	}
	if candidates[len(candidates)-1] != ConfigFileName {
		t.Errorf("cwd fallback must come last, got %q", candidates[len(candidates)-1])
	}
	for _, c := range candidates[1 : len(candidates)-1] {
		if !filepath.IsAbs(c) {
			t.Errorf("intermediate candidate %q should be absolute", c)
		}
	}
}

func TestResolveConfigPath_FindsExplicitOverride(t *testing.T) {
	dir := t.TempDir()
	cfgPath := filepath.Join(dir, "custom.yaml")
	if err := os.WriteFile(cfgPath, []byte("log_level: debug\n"), 0600); err != nil {
		t.Fatalf("failed to write config: %v", err)
	}
	t.Setenv("SUPERVIEW_CONFIG", cfgPath)

	if got := ResolveConfigPath(); got != cfgPath {
		t.Errorf("expected %q, got %q", cfgPath, got)
	}
}

func TestResolveConfigPath_IgnoresDirectoryAndMissingFile(t *testing.T) {
	// A directory named superview.yaml must not be mistaken for the config file.
	t.Setenv("SUPERVIEW_CONFIG", t.TempDir())

	got := ResolveConfigPath()
	if got != "" && filepath.Base(got) != ConfigFileName {
		t.Errorf("unexpected resolution %q", got)
	}
	if got == os.Getenv("SUPERVIEW_CONFIG") {
		t.Error("a directory must not be resolved as the config file")
	}
}

func TestLoadConfig_EmptyPathYieldsDefaults(t *testing.T) {
	cfg, err := LoadConfig("")
	if err != nil {
		t.Fatalf("LoadConfig(\"\") returned error: %v", err)
	}
	if cfg.MinBitrate != defaultConfig.MinBitrate || cfg.MaxBitrate != defaultConfig.MaxBitrate {
		t.Errorf("expected defaults, got min=%d max=%d", cfg.MinBitrate, cfg.MaxBitrate)
	}
}

// TestPipelineHonoursExplicitConfig checks the configuration actually reaches
// the pipeline. It used to be read from a mutable package-level global that the
// GUI overwrote before each run, so the effective settings depended on call
// order and could be written from the UI thread while the encoding goroutine
// read them.
func TestPipelineHonoursExplicitConfig(t *testing.T) {
	cfg := &Config{
		MinBitrate:    1000,
		MaxBitrate:    9_000_000,
		TempDirPrefix: "superview-explicit-*",
		EncoderCodecs: []string{"264"},
		LogLevel:      "info",
	}

	if err := InitEncodingSession(cfg); err != nil {
		t.Fatalf("InitEncodingSession: %v", err)
	}
	defer func() {
		if err := CleanUp(); err != nil {
			t.Errorf("CleanUp: %v", err)
		}
	}()

	xPath, _, err := getSessionPaths()
	if err != nil {
		t.Fatalf("getSessionPaths: %v", err)
	}
	// The temp directory must come from the config we passed, not from a global.
	if !strings.Contains(xPath, "superview-explicit-") {
		t.Errorf("session path %q does not use the supplied TempDirPrefix", xPath)
	}
}

func TestInitEncodingSession_NilConfigUsesDefaults(t *testing.T) {
	if err := InitEncodingSession(nil); err != nil {
		t.Fatalf("InitEncodingSession(nil): %v", err)
	}
	defer func() { _ = CleanUp() }()

	xPath, _, err := getSessionPaths()
	if err != nil {
		t.Fatalf("getSessionPaths: %v", err)
	}
	if !strings.Contains(xPath, "superview-") {
		t.Errorf("session path %q does not use the default TempDirPrefix", xPath)
	}
}

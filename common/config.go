package common

import (
	"bytes"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strconv"
	"strings"

	"gopkg.in/yaml.v3"
)

// Config contains all configuration parameters for the video encoding pipeline.
// Values can be loaded from YAML files and overridden via environment variables.
// See superview.yaml for an example configuration file with documentation.
type Config struct {
	// Bitrate constraints in bytes/second
	// MinBitrate: minimum acceptable output bitrate (prevents lossy compression)
	// MaxBitrate: maximum acceptable output bitrate (controls file size)
	MinBitrate int `yaml:"min_bitrate" default:"102400"`    // 100k bytes/sec (~0.1 Mbps)
	MaxBitrate int `yaml:"max_bitrate" default:"209715200"` // 200M bytes/sec (~200 Mbps)

	// TempDirPrefix is the template for temporary directory creation
	TempDirPrefix string `yaml:"temp_dir_prefix" default:"superview-*"`

	// EncoderCodecs is a list of H.264/H.265 encoder codec identifiers to recognize
	EncoderCodecs []string `yaml:"encoder_codecs" default:"264,265,hevc"`

	// LogLevel controls the verbosity of logging (debug, info, warn, error)
	LogLevel string `yaml:"log_level" default:"info"`

	// PerformanceMode controls the audio handling tradeoff.
	// Supported values:
	// - "safe": always re-encode audio to AAC (maximum container compatibility).
	// - "safe_performance": try to copy the audio stream untouched, falling back
	//   to AAC when the copy is rejected. Faster and lossless for audio.
	//
	// Neither mode throttles the encode any more: the "-re" realtime input flag
	// was removed because it only applies to streaming, not file output.
	PerformanceMode string `yaml:"performance_mode" default:"safe"`

	// VideoPreset controls ffmpeg encoder preset (optional).
	// Empty value keeps ffmpeg defaults for maximum compatibility.
	VideoPreset string `yaml:"video_preset" default:""`

	// FilterThreads controls ffmpeg filter graph thread count.
	// 0 means ffmpeg auto/implicit behavior.
	FilterThreads int `yaml:"filter_threads" default:"0"`

	// EncoderThreads controls ffmpeg encoder thread count.
	// 0 means auto (or runtime defaults where explicitly configured).
	EncoderThreads int `yaml:"encoder_threads" default:"0"`
}

var defaultConfig = &Config{
	MinBitrate:      102400,    // 100k bytes/sec
	MaxBitrate:      209715200, // 200M bytes/sec
	TempDirPrefix:   "superview-*",
	EncoderCodecs:   []string{"264", "265", "hevc"},
	LogLevel:        "info",
	PerformanceMode: "safe",
	VideoPreset:     "",
	FilterThreads:   0,
	EncoderThreads:  0,
}

// configOrDefault returns cfg, or the built-in defaults when cfg is nil.
//
// The pipeline used to read a mutable package-level global instead of taking
// the configuration as an argument. The GUI wrote to it before every run, so
// the effective settings depended on call order, the user's video_preset was
// silently overwritten, and the value was read from the encoding goroutine
// while the UI thread could still write it. Configuration is now passed
// explicitly; this helper only keeps callers from having to nil-check.
//
// The returned value must be treated as read-only when cfg is nil: it is the
// shared defaults instance, not a copy.
func configOrDefault(cfg *Config) *Config {
	if cfg == nil {
		return defaultConfig
	}
	return cfg
}

// ConfigFileName is the name of the YAML configuration file looked up by ResolveConfigPath.
const ConfigFileName = "superview.yaml"

// configCandidatePaths returns the config file locations to probe, in priority order.
// It is split out from ResolveConfigPath so the ordering can be unit tested.
func configCandidatePaths() []string {
	var candidates []string

	// 1. Explicit override always wins.
	if explicit := strings.TrimSpace(os.Getenv("SUPERVIEW_CONFIG")); explicit != "" {
		candidates = append(candidates, explicit)
	}

	// 2. Next to the executable: this is where a packaged install keeps it.
	if exe, err := os.Executable(); err == nil {
		if resolved, linkErr := filepath.EvalSymlinks(exe); linkErr == nil {
			exe = resolved
		}
		candidates = append(candidates, filepath.Join(filepath.Dir(exe), ConfigFileName))
	}

	// 3. Per-user config directory (~/.config/superview on Linux,
	//    %AppData%\superview on Windows).
	if userCfg, err := os.UserConfigDir(); err == nil {
		candidates = append(candidates, filepath.Join(userCfg, "superview", ConfigFileName))
	}

	// 4. Current working directory: developer runs from the repository root.
	candidates = append(candidates, ConfigFileName)

	return candidates
}

// ResolveConfigPath returns the first configuration file that actually exists.
//
// A bare relative name is resolved against the process working directory, which
// is arbitrary when the GUI is started from a desktop launcher or the Start
// menu. Probing the executable directory and the per-user config directory first
// is what makes the file reachable in a real installation.
//
// Returns "" when no candidate exists; LoadConfig("") yields the defaults.
func ResolveConfigPath() string {
	for _, candidate := range configCandidatePaths() {
		if candidate == "" {
			continue
		}
		if info, err := os.Stat(candidate); err == nil && !info.IsDir() {
			return candidate
		}
	}
	return ""
}

// LoadConfig loads configuration from a YAML file and applies environment variable overrides.
// If filepath is empty, returns default configuration.
// Environment variables (SUPERVIEW_*) override values from the YAML file.
// Returns an error only if the file cannot be read (not if file doesn't exist).
func LoadConfig(filepath string) (*Config, error) {
	config := &Config{}

	// Start with defaults
	*config = *defaultConfig

	// If filepath is provided, try to load from file
	if filepath != "" {
		data, err := os.ReadFile(filepath)
		if err != nil {
			if os.IsNotExist(err) {
				logger.Info("Config file not found, using defaults",
					slog.String("path", filepath),
				)
			} else {
				return nil, fmt.Errorf("failed to read config file: %w", err)
			}
		} else {
			if err := yaml.Unmarshal(data, config); err != nil {
				return nil, fmt.Errorf("failed to parse config file: %w", err)
			}
			logger.Info("Configuration loaded from file",
				slog.String("path", filepath),
			)
		}
	}

	// Apply environment variable overrides
	if minBitrate := os.Getenv("SUPERVIEW_MIN_BITRATE"); minBitrate != "" {
		val, err := strconv.Atoi(minBitrate)
		if err != nil {
			logger.Warn("Invalid SUPERVIEW_MIN_BITRATE, using config value",
				slog.String("value", minBitrate),
			)
		} else {
			config.MinBitrate = val
		}
	}

	if maxBitrate := os.Getenv("SUPERVIEW_MAX_BITRATE"); maxBitrate != "" {
		val, err := strconv.Atoi(maxBitrate)
		if err != nil {
			logger.Warn("Invalid SUPERVIEW_MAX_BITRATE, using config value",
				slog.String("value", maxBitrate),
			)
		} else {
			config.MaxBitrate = val
		}
	}

	if logLevel := os.Getenv("SUPERVIEW_LOG_LEVEL"); logLevel != "" {
		config.LogLevel = logLevel
	}

	if logLevel := os.Getenv("SUPERVIEW_TEMP_DIR_PREFIX"); logLevel != "" {
		config.TempDirPrefix = logLevel
	}

	if encoders := os.Getenv("SUPERVIEW_ENCODER_CODECS"); encoders != "" {
		config.EncoderCodecs = strings.Split(encoders, ",")
	}

	if mode := os.Getenv("SUPERVIEW_PERFORMANCE_MODE"); mode != "" {
		config.PerformanceMode = mode
	}

	if preset := os.Getenv("SUPERVIEW_VIDEO_PRESET"); preset != "" {
		config.VideoPreset = preset
	}

	if filterThreads := os.Getenv("SUPERVIEW_FILTER_THREADS"); filterThreads != "" {
		val, err := strconv.Atoi(filterThreads)
		if err != nil {
			logger.Warn("Invalid SUPERVIEW_FILTER_THREADS, using config value",
				slog.String("value", filterThreads),
			)
		} else {
			config.FilterThreads = val
		}
	}

	if encoderThreads := os.Getenv("SUPERVIEW_ENCODER_THREADS"); encoderThreads != "" {
		val, err := strconv.Atoi(encoderThreads)
		if err != nil {
			logger.Warn("Invalid SUPERVIEW_ENCODER_THREADS, using config value",
				slog.String("value", encoderThreads),
			)
		} else {
			config.EncoderThreads = val
		}
	}

	config.PerformanceMode = normalizePerformanceMode(config.PerformanceMode)
	config.VideoPreset = normalizeVideoPreset(config.VideoPreset)
	config.FilterThreads = normalizeThreadCount(config.FilterThreads, "filter_threads")
	config.EncoderThreads = normalizeThreadCount(config.EncoderThreads, "encoder_threads")

	return config, nil
}

func normalizePerformanceMode(mode string) string {
	switch strings.ToLower(strings.TrimSpace(mode)) {
	case "", "safe":
		return "safe"
	case "safe_performance", "performance":
		return "safe_performance"
	default:
		logger.Warn("Invalid performance_mode, falling back to safe",
			slog.String("value", mode),
		)
		return "safe"
	}
}

func normalizeVideoPreset(preset string) string {
	switch strings.ToLower(strings.TrimSpace(preset)) {
	case "", "ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow", "placebo":
		return strings.ToLower(strings.TrimSpace(preset))
	default:
		logger.Warn("Invalid video_preset, falling back to ffmpeg default",
			slog.String("value", preset),
		)
		return ""
	}
}

func normalizeThreadCount(value int, key string) int {
	if value < 0 {
		logger.Warn("Invalid thread count, falling back to auto",
			slog.String("key", key),
			slog.Int("value", value),
		)
		return 0
	}
	return value
}

// IsSafePerformanceMode returns true when optional performance optimizations are enabled.
func (c *Config) IsSafePerformanceMode() bool {
	if c == nil {
		return false
	}
	return normalizePerformanceMode(c.PerformanceMode) == "safe_performance"
}

// String returns a formatted representation of the config
func (c *Config) String() string {
	var buf bytes.Buffer
	buf.WriteString("Configuration:\n")
	buf.WriteString(fmt.Sprintf("  Min Bitrate: %d bytes/sec (%.2f Mbps)\n",
		c.MinBitrate, float64(c.MinBitrate)/1000000))
	buf.WriteString(fmt.Sprintf("  Max Bitrate: %d bytes/sec (%.2f Mbps)\n",
		c.MaxBitrate, float64(c.MaxBitrate)/1000000))
	buf.WriteString(fmt.Sprintf("  Temp Dir Prefix: %s\n", c.TempDirPrefix))
	buf.WriteString(fmt.Sprintf("  Encoder Codecs: %s\n", strings.Join(c.EncoderCodecs, ",")))
	buf.WriteString(fmt.Sprintf("  Log Level: %s\n", c.LogLevel))
	buf.WriteString(fmt.Sprintf("  Performance Mode: %s\n", normalizePerformanceMode(c.PerformanceMode)))
	buf.WriteString(fmt.Sprintf("  Video Preset: %s\n", normalizeVideoPreset(c.VideoPreset)))
	buf.WriteString(fmt.Sprintf("  Filter Threads: %d\n", normalizeThreadCount(c.FilterThreads, "filter_threads")))
	buf.WriteString(fmt.Sprintf("  Encoder Threads: %d\n", normalizeThreadCount(c.EncoderThreads, "encoder_threads")))
	return buf.String()
}

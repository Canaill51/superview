package common

import (
	"bytes"
	"errors"
	"fmt"
	"io"
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
	// Bitrate constraints in bits/second -- the unit ffprobe reports for
	// bit_rate and the one ffmpeg's -b:v expects. These fields were documented
	// as bytes/second for a long time while holding bits, so anyone who sized
	// them from the comment was off by a factor of 8.
	// MinBitrate: minimum acceptable output bitrate (prevents lossy compression)
	// MaxBitrate: maximum acceptable output bitrate (controls file size)
	MinBitrate int `yaml:"min_bitrate" default:"102400"`    // ~0.1 Mbps
	MaxBitrate int `yaml:"max_bitrate" default:"209715200"` // ~200 Mbps

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
	MinBitrate:      102400,    // bits/sec, ~0.1 Mbps
	MaxBitrate:      209715200, // bits/sec, ~200 Mbps
	TempDirPrefix:   "superview-*",
	EncoderCodecs:   []string{"264", "265", "hevc"},
	LogLevel:        "info",
	PerformanceMode: "safe",
	VideoPreset:     "",
	FilterThreads:   0,
	EncoderThreads:  0,
}

// DefaultConfig returns a copy of the built-in defaults, ignoring both the
// configuration file and the environment.
//
// It is the last resort when LoadConfig refuses: an inconsistent environment
// (SUPERVIEW_MIN_BITRATE above SUPERVIEW_MAX_BITRATE) makes even LoadConfig("")
// fail, and the application still has to start.
func DefaultConfig() *Config {
	cfg := *defaultConfig
	cfg.EncoderCodecs = append([]string(nil), defaultConfig.EncoderCodecs...)
	return &cfg
}

// retiredConfigKeys are keys earlier releases shipped in superview.yaml and that
// no longer mean anything (C-03 removed them in the first pass; v0.1.6 still
// carried them). A user who copied that file and tuned it must not lose every
// other setting to the strict decoder because of a key Superview itself once
// wrote there: these are dropped with a warning, while any other unknown key
// stays an error that names it.
var retiredConfigKeys = map[string]bool{
	"quality_preset":   true,
	"min_video_width":  true,
	"min_video_height": true,
}

// withoutRetiredKeys returns the document with the retired top-level keys
// removed, or the input unchanged when it cannot be read as a mapping -- the
// strict decode that follows then reports the real problem.
func withoutRetiredKeys(data []byte, path string) []byte {
	var doc yaml.Node
	if err := yaml.Unmarshal(data, &doc); err != nil || len(doc.Content) == 0 {
		return data
	}
	root := doc.Content[0]
	if root.Kind != yaml.MappingNode {
		return data
	}

	kept := make([]*yaml.Node, 0, len(root.Content))
	dropped := false
	for i := 0; i+1 < len(root.Content); i += 2 {
		key := root.Content[i]
		if retiredConfigKeys[key.Value] {
			logger.Warn("Ignoring a configuration key that no longer has any effect; it can be deleted",
				slog.String("path", path),
				slog.String("key", key.Value),
			)
			dropped = true
			continue
		}
		kept = append(kept, key, root.Content[i+1])
	}
	if !dropped {
		return data
	}
	root.Content = kept
	out, err := yaml.Marshal(&doc)
	if err != nil {
		return data
	}
	return out
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
// Returns an error if the file cannot be read (not if it does not exist), if it
// holds a key Config does not know, or if min_bitrate ends up above max_bitrate.
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
			// KnownFields: an unknown key is an error that names it. yaml.Unmarshal
			// ignored it, so a misspelt "video-preset:" was dropped without a word
			// -- the way a user's preset was already lost once (C-05).
			dec := yaml.NewDecoder(bytes.NewReader(withoutRetiredKeys(data, filepath)))
			dec.KnownFields(true)
			// io.EOF is an empty document: a file holding only comments is valid
			// and changes nothing, as it did with yaml.Unmarshal.
			if err := dec.Decode(config); err != nil && !errors.Is(err, io.EOF) {
				return nil, fmt.Errorf("failed to parse config file: %w", err)
			}
			config.EncoderCodecs = normalizeEncoderCodecs(config.EncoderCodecs, "encoder_codecs")
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
		config.EncoderCodecs = normalizeEncoderCodecs(strings.Split(encoders, ","), "SUPERVIEW_ENCODER_CODECS")
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

	// After the environment, which can set either bound: the GUI clamps its
	// bitrate into [min, max], and with min above max every value it can
	// produce is one ValidateBitrate then refuses, run after run.
	if config.MinBitrate > 0 && config.MaxBitrate > 0 && config.MinBitrate > config.MaxBitrate {
		return nil, fmt.Errorf("invalid configuration: min_bitrate (%d) is above max_bitrate (%d)",
			config.MinBitrate, config.MaxBitrate)
	}

	config.PerformanceMode = normalizePerformanceMode(config.PerformanceMode)
	config.VideoPreset = normalizeVideoPreset(config.VideoPreset)
	config.FilterThreads = normalizeThreadCount(config.FilterThreads, "filter_threads")
	config.EncoderThreads = normalizeThreadCount(config.EncoderThreads, "encoder_threads")

	return config, nil
}

// normalizeEncoderCodecs trims every codec identifier and drops the empty ones.
//
// "264, 265" used to yield " 265", which matches no encoder name, and every
// HEVC encoder vanished from the list. An empty result -- "encoder_codecs: []",
// or a variable holding only commas -- would empty the encoder list the same
// way, so it falls back to the defaults with a warning instead.
func normalizeEncoderCodecs(codecs []string, source string) []string {
	cleaned := make([]string, 0, len(codecs))
	for _, codec := range codecs {
		if trimmed := strings.TrimSpace(codec); trimmed != "" {
			cleaned = append(cleaned, trimmed)
		}
	}
	if len(cleaned) == 0 {
		logger.Warn("No encoder codec configured, using the defaults",
			slog.String("source", source),
			slog.String("defaults", strings.Join(defaultConfig.EncoderCodecs, ",")),
		)
		return append([]string(nil), defaultConfig.EncoderCodecs...)
	}
	return cleaned
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
	buf.WriteString(fmt.Sprintf("  Min Bitrate: %d bits/sec (%.2f Mbps)\n",
		c.MinBitrate, float64(c.MinBitrate)/1000000))
	buf.WriteString(fmt.Sprintf("  Max Bitrate: %d bits/sec (%.2f Mbps)\n",
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

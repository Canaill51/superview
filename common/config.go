package common

import (
	"bytes"
	"fmt"
	"log/slog"
	"os"
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
	MinBitrate int `yaml:"min_bitrate" default:"102400"`      // 100k bytes/sec (~0.1 Mbps)
	MaxBitrate int `yaml:"max_bitrate" default:"52428800"`    // 50M bytes/sec (~50 Mbps)

	// TempDirPrefix is the template for temporary directory creation
	TempDirPrefix string `yaml:"temp_dir_prefix" default:"superview-*"`

	// EncoderCodecs is a list of H.264/H.265 encoder codec identifiers to recognize
	EncoderCodecs []string `yaml:"encoder_codecs" default:"264,265,hevc"`

	// LogLevel controls the verbosity of logging (debug, info, warn, error)
	LogLevel string `yaml:"log_level" default:"info"`

	// MinVideoWidth and MinVideoHeight enforce minimum input video dimensions
	MinVideoWidth  int `yaml:"min_video_width" default:"320"`
	MinVideoHeight int `yaml:"min_video_height" default:"240"`
}

var defaultConfig = &Config{
	MinBitrate:     102400,      // 100k bytes/sec
	MaxBitrate:     52428800,    // 50M bytes/sec
	TempDirPrefix:  "superview-*",
	EncoderCodecs:  []string{"264", "265", "hevc"},
	LogLevel:       "info",
	MinVideoWidth:  320,
	MinVideoHeight: 240,
}

var currentConfig = defaultConfig

// GetConfig returns the current global configuration used by the encoding pipeline.
func GetConfig() *Config {
	return currentConfig
}

// SetConfig sets the global configuration.
// If nil is passed, the configuration is unchanged.
func SetConfig(cfg *Config) {
	if cfg != nil {
		currentConfig = cfg
		logger.Debug("Configuration updated",
			slog.Int("min_bitrate", cfg.MinBitrate),
			slog.Int("max_bitrate", cfg.MaxBitrate),
			slog.String("log_level", cfg.LogLevel),
		)
	}
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

	return config, nil
}

// CreateDefaultConfig creates a default configuration file at the specified path.
// The file includes commented documentation for all configuration options.
// Useful for generating initial configuration templates for users.
func CreateDefaultConfig(filepath string) error {
	data, err := yaml.Marshal(defaultConfig)
	if err != nil {
		return fmt.Errorf("failed to marshal default config: %w", err)
	}

	// Add comments
	commentedData := []byte(`# Superview Configuration File
# All values can be overridden with environment variables prefixed with SUPERVIEW_

`)
	commentedData = append(commentedData, data...)

	if err := os.WriteFile(filepath, commentedData, 0644); err != nil {
		return fmt.Errorf("failed to write config file: %w", err)
	}

	logger.Info("Default config file created",
		slog.String("path", filepath),
	)
	return nil
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
	return buf.String()
}

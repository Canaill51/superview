package common

import (
	"encoding/json"
	"fmt"
	"log/slog"
	"math"
	"sync"
	"time"
)

// EncodingMetrics tracks performance metrics throughout the encoding lifecycle.
// All timestamps are recorded in UTC. All sizes are in bytes. All bitrates in bytes/sec.
// This structure is thread-safe via internal mutex.
type EncodingMetrics struct {
	mu sync.RWMutex

	// Timestamps marking key events
	StartTime       time.Time // Encoding start (after validation)
	EndTime         time.Time // Encoding completion
	ProgressUpdates int       // Number of progress updates

	// Input file information
	InputFile     string  // Source file path
	InputFileSize int64   // File size in bytes
	InputDuration float64 // Duration in seconds
	InputBitrate  int     // bytes/second (from video metadata)
	InputCodec    string  // Video codec name
	InputWidth    int     // Video width in pixels
	InputHeight   int     // Video height in pixels

	// Output file information
	OutputFile     string // Destination file path
	OutputFileSize int64  // File size in bytes (0 until encoding complete)
	OutputBitrate  int    // bytes/second (configured output bitrate)
	OutputCodec    string // Output encoder name

	// Encoding progress
	LastProgress  float64   // Last reported progress percentage (0-100)
	ProgressTime  time.Time // Time of last progress update
	LastFrameTime float64   // Approx frame timing from ffmpeg output

	// Processing metrics
	EncodingSpeed      float64       // Computed frames per second
	CompressionRatio   float64       // Output size / Input size (computed)
	BitrateReduction   float64       // (Input - Output) / Input ratio (computed)
	EstimatedRemaining time.Duration // Computed time remaining
	VideoCheckDuration time.Duration // Time spent in CheckVideo
	PGMGenerationTime  time.Duration // Time spent generating remap PGM files
	EncodeDuration     time.Duration // Time spent in ffmpeg encoding
	CleanupDuration    time.Duration // Time spent cleaning temporary session data

	// Error tracking
	FfmpegExitCode int    // ffmpeg process exit code (0 = success)
	ErrorMessage   string // Error message if encoding failed
	Success        bool   // True if encoding completed successfully
}

// RecordStageDurations stores per-stage durations for later UI/reporting consumption.
func (m *EncodingMetrics) RecordStageDurations(videoCheck, pgmGeneration, encode, cleanup time.Duration) {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.VideoCheckDuration = videoCheck
	m.PGMGenerationTime = pgmGeneration
	m.EncodeDuration = encode
	m.CleanupDuration = cleanup
}

// NewEncodingMetrics creates a new metrics tracker with initialized timestamp.
// inputFile and outputFile should be the source and destination paths.
// ffmpegInfo is the result from CheckFfmpeg() to track version and encoders.
func NewEncodingMetrics(inputFile, outputFile string) *EncodingMetrics {
	return &EncodingMetrics{
		StartTime:  time.Now().UTC(),
		InputFile:  inputFile,
		OutputFile: outputFile,
	}
}

// RecordInputMetadata captures video specifications from CheckVideo result.
// Should be called after video validation and before encoding starts.
func (m *EncodingMetrics) RecordInputMetadata(video *VideoSpecs, fileSize int64) {
	m.mu.Lock()
	defer m.mu.Unlock()

	if video == nil || len(video.Streams) == 0 {
		return
	}

	stream := video.Streams[0]
	m.InputFileSize = fileSize
	m.InputDuration = stream.DurationFloat
	m.InputBitrate = stream.BitrateInt
	m.InputCodec = stream.Codec
	m.InputWidth = stream.Width
	m.InputHeight = stream.Height
}

// RecordOutputMetadata captures configured output parameters.
// Should be called before encoding starts.
func (m *EncodingMetrics) RecordOutputMetadata(bitrate int, encoder string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.OutputBitrate = bitrate
	m.OutputCodec = encoder
}

// RecordProgress updates progress tracking during encoding.
// percent should be 0-100. Called from the progress callback.
func (m *EncodingMetrics) RecordProgress(percent float64) {
	m.mu.Lock()
	defer m.mu.Unlock()

	now := time.Now().UTC()
	m.LastProgress = math.Min(math.Max(percent, 0), 100)
	m.ProgressUpdates++
	m.ProgressTime = now

	// Estimate remaining time based on progress
	if percent > 0 && percent < 100 {
		elapsedSeconds := now.Sub(m.StartTime).Seconds()
		estimatedTotal := (elapsedSeconds / percent) * 100
		remaining := estimatedTotal - elapsedSeconds
		m.EstimatedRemaining = time.Duration(remaining) * time.Second
	}
}

// RecordCompletion marks encoding as successfully completed.
// outputFileSize should be the final output file size in bytes.
// Call this after encoding finishes normally.
func (m *EncodingMetrics) RecordCompletion(outputFileSize int64) {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.EndTime = time.Now().UTC()
	m.OutputFileSize = outputFileSize
	m.Success = true
	m.FfmpegExitCode = 0
	m.LastProgress = 100

	// Compute derived metrics
	m.computeMetrics()
}

// RecordError marks encoding as failed with error details.
// exitCode is the process exit code. message is the error description.
// Call this when encoding fails.
func (m *EncodingMetrics) RecordError(exitCode int, message string) {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.EndTime = time.Now().UTC()
	m.Success = false
	m.FfmpegExitCode = exitCode
	m.ErrorMessage = message
}

// EllapsedTime returns total encoding duration.
// Returns 0 if encoding hasn't completed.
func (m *EncodingMetrics) ElapsedTime() time.Duration {
	m.mu.RLock()
	defer m.mu.RUnlock()

	if m.EndTime.IsZero() {
		return time.Since(m.StartTime)
	}
	return m.EndTime.Sub(m.StartTime)
}

// Summary returns a human-readable summary of encoding metrics.
func (m *EncodingMetrics) Summary() string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	elapsed := m.ElapsedTime()
	if m.EndTime.IsZero() {
		elapsed = time.Since(m.StartTime)
	}

	status := "FAILED"
	if m.Success {
		status = "SUCCESS"
	}

	summary := fmt.Sprintf(`=== Encoding Summary ===
Status           : %s
Input File       : %s (%.1f MB)
Output File      : %s (%.1f MB)
Input Codec      : %s (%dx%d @ %.0fs)
Output Encoder   : %s
Total Time       : %s

=== Performance ===
Encoding Speed   : %.1f fps
Compression      : %.1f%%
Bitrate Reduction: %.1f%%

=== Bitrate ===
Input            : %.1f Mb/s
Output           : %.1f Mb/s
`,
		status,
		m.InputFile, float64(m.InputFileSize)/1024/1024,
		m.OutputFile, float64(m.OutputFileSize)/1024/1024,
		m.InputCodec, m.InputWidth, m.InputHeight, m.InputDuration,
		m.OutputCodec,
		elapsed,
		m.EncodingSpeed,
		m.CompressionRatio*100,
		m.BitrateReduction*100,
		float64(m.InputBitrate)/1024/1024,
		float64(m.OutputBitrate)/1024/1024,
	)

	if !m.Success {
		summary += fmt.Sprintf("\nError            : %s (exit code %d)", m.ErrorMessage, m.FfmpegExitCode)
	}

	return summary
}

// ToJSON returns metrics as JSON for structured logging and export.
func (m *EncodingMetrics) ToJSON() string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	elapsed := m.ElapsedTime()
	if m.EndTime.IsZero() {
		elapsed = time.Since(m.StartTime)
	}

	data := map[string]interface{}{
		"success":            m.Success,
		"status":             map[string]interface{}{"input_codec": m.InputCodec, "output_codec": m.OutputCodec},
		"files":              map[string]interface{}{"input_size": m.InputFileSize, "output_size": m.OutputFileSize},
		"timing":             map[string]interface{}{"elapsed_seconds": elapsed.Seconds()},
		"encoding_speed_fps": m.EncodingSpeed,
		"compression_ratio":  m.CompressionRatio,
		"progress_updates":   m.ProgressUpdates,
		"bitrates":           map[string]float64{"input": float64(m.InputBitrate) / 1024 / 1024, "output": float64(m.OutputBitrate) / 1024 / 1024},
	}

	if !m.Success {
		data["error"] = map[string]interface{}{"message": m.ErrorMessage, "exit_code": m.FfmpegExitCode}
	}

	jsonBytes, _ := json.MarshalIndent(data, "", "  ")
	return string(jsonBytes)
}

// LogMetrics logs the metrics using the global logger at appropriate levels.
func (m *EncodingMetrics) LogMetrics(log *slog.Logger) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	elapsed := m.ElapsedTime()
	if m.EndTime.IsZero() {
		elapsed = time.Since(m.StartTime)
	}

	if m.Success {
		log.Info("Encoding completed successfully",
			slog.String("output_file", m.OutputFile),
			slog.Int64("output_size_bytes", m.OutputFileSize),
			slog.String("duration", elapsed.String()),
			slog.Float64("encoding_speed_fps", m.EncodingSpeed),
			slog.Float64("compression_ratio", m.CompressionRatio),
		)
	} else {
		log.Error("Encoding failed",
			slog.String("error", m.ErrorMessage),
			slog.Int("exit_code", m.FfmpegExitCode),
			slog.String("duration", elapsed.String()),
		)
	}
}

// computeMetrics calculates derived metrics from basic observations.
// Should only be called when holding the write lock.
func (m *EncodingMetrics) computeMetrics() {
	if m.EndTime.IsZero() || m.StartTime.IsZero() {
		return
	}

	elapsedSeconds := m.EndTime.Sub(m.StartTime).Seconds()

	// Encoding speed: total frame count / elapsed time
	if elapsedSeconds > 0 && m.InputDuration > 0 {
		totalFrames := m.InputDuration * 30 // Assuming ~30fps average
		m.EncodingSpeed = totalFrames / elapsedSeconds
	}

	// Compression ratio: output size / input size
	if m.InputFileSize > 0 {
		m.CompressionRatio = float64(m.OutputFileSize) / float64(m.InputFileSize)
	}

	// Bitrate reduction: (input - output) / input
	if m.InputBitrate > 0 && m.OutputBitrate > 0 {
		m.BitrateReduction = float64(m.InputBitrate-m.OutputBitrate) / float64(m.InputBitrate)
	}
}

// PerformEncoding: validation, the working file, and the metrics around one conversion.

package common

import (
	"context"
	"crypto/rand"
	"encoding/hex"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// UIHandler abstracts user interface interactions between GUI components and the core pipeline.
// It allows the core encoding pipeline to be UI-agnostic and testable.
type UIHandler interface {
	// ShowError displays an error message to the user.
	ShowError(error)
	// ShowInfo displays an information or success message to the user.
	ShowInfo(msg string)
	// ShowProgress updates the progress indicator (0-100 percent).
	ShowProgress(percent float64)
	// GetBitrate returns the desired output bitrate in bits/second.
	// Returns 0 to use the input video's bitrate.
	GetBitrate() (int, error)
	// GetEncoder returns the encoder selection (e.g., "libx265").
	// Returns empty string to use the input video's codec.
	GetEncoder() string
	// GetSqueeze returns true to apply squeeze filter for 4:3 to 16:9 scaling.
	GetSqueeze() bool
}

// ProgressDetailHandler is an optional companion to UIHandler.
//
// A UIHandler that also implements it receives the estimated time remaining
// alongside the raw percentage. EncodingMetrics has been computing that
// estimate on every progress update since it was written and nothing ever read
// it: the GUI showed a bare percentage for an operation that runs for minutes
// on 4K footage.
//
// It is deliberately a separate interface rather than another method on
// UIHandler: UIHandler is what the pipeline requires, this is what it uses if
// the caller offers it.
type ProgressDetailHandler interface {
	// ShowProgressDetail reports progress together with the estimated time
	// left. remaining is zero when there is not enough data to estimate yet,
	// which callers should render as "no estimate", not as "no time left".
	ShowProgressDetail(percent float64, remaining time.Duration)
}

// canonicalPath resolves a path as far as the filesystem allows, so two
// spellings of the same location compare equal. Falls back to an absolute,
// cleaned form when the file does not exist yet, which is the normal case for a
// destination.
func canonicalPath(path string) string {
	if resolved, err := filepath.EvalSymlinks(path); err == nil {
		return filepath.Clean(resolved)
	}
	if abs, err := filepath.Abs(path); err == nil {
		return filepath.Clean(abs)
	}
	return filepath.Clean(path)
}

// sameOutputAsInput reports whether the destination is the source file.
//
// FFmpeg used to catch this on its own: it refuses to read and write the same
// file and exits 234, which left the source intact but told the user nothing
// useful. Now that the encode goes to a temporary file first, FFmpeg no longer
// sees a conflict at all -- and the final rename would happily destroy the
// source. The guard is what keeps that from happening, and it can say something
// comprehensible while it is at it.
func sameOutputAsInput(inputFile, outputFile string) bool {
	inInfo, inErr := os.Stat(inputFile)
	outInfo, outErr := os.Stat(outputFile)
	if inErr == nil && outErr == nil {
		return os.SameFile(inInfo, outInfo)
	}
	return pathEqual(canonicalPath(inputFile), canonicalPath(outputFile))
}

// newPartialOutputPath creates the file ffmpeg writes into while it works.
//
// It keeps the .mp4 extension, because ffmpeg picks its muxer from it, and it
// sits in the destination directory so the final rename stays on one filesystem
// and is therefore atomic. The name is random and the open is exclusive: two
// conversions targeting the same folder must not collide.
//
// Not os.CreateTemp, which creates with mode 0600. ffmpeg then truncates that
// inode and the rename keeps its mode, so the finished video arrived unreadable
// by anyone but its owner -- in a shared folder, a NAS export or a media
// server's library, that is the one place the user wanted it readable. 0666
// before umask is what ffmpeg itself would have created, and is what the user
// got before the working file existed.
func newPartialOutputPath(outputFile string) (string, error) {
	dir := filepath.Dir(outputFile)
	for attempt := 0; attempt < 100; attempt++ {
		var suffix [6]byte
		if _, err := rand.Read(suffix[:]); err != nil {
			return "", fmt.Errorf("cannot name a working file next to %s: %w", outputFile, err)
		}
		name := filepath.Join(dir, ".superview-partial-"+hex.EncodeToString(suffix[:])+".mp4")

		file, err := os.OpenFile(name, os.O_WRONLY|os.O_CREATE|os.O_EXCL, 0o666)
		if errors.Is(err, os.ErrExist) {
			continue
		}
		if err != nil {
			return "", fmt.Errorf("cannot create a working file next to %s: %w", outputFile, err)
		}
		if closeErr := file.Close(); closeErr != nil {
			_ = os.Remove(name)
			return "", fmt.Errorf("cannot create a working file next to %s: %w", outputFile, closeErr)
		}
		return name, nil
	}
	return "", fmt.Errorf("cannot create a working file next to %s: every name tried already exists", outputFile)
}

// PerformEncoding orchestrates the complete encoding workflow from input file to output file.
// It coordinates all steps: validation, metadata extraction, option gathering, encoding, and cleanup.
// The ui parameter handles user interaction (showing errors, progress, getting options).
// Returns nil on success, or an error if any step fails.
// Call this from GUI entry points only; the logic is pipeline-agnostic.
// Security: Validates input/output paths and encoder selection for defensive programming.
// Observability: Records metrics and events throughout the pipeline.
func PerformEncoding(cfg *Config, inputFile string, outputFile string, ui UIHandler, ffmpeg map[string]string, cancel <-chan struct{}) error {
	cfg = configOrDefault(cfg)
	// ==== OBSERVABILITY: Initialize metrics collection ====
	metrics := NewEncodingMetrics(inputFile, outputFile)
	stageDurations := make(map[string]time.Duration)
	defer func() {
		// Always record completion or error
		if metrics.Success {
			RecordEncodingCompletion(metrics)
		}
	}()
	defer func() {
		metrics.RecordStageDurations(
			stageDurations["video_check"],
			stageDurations["pgm_generation"],
			stageDurations["encoding"],
			stageDurations["cleanup"],
		)
		logger.Info("Encoding stage timings",
			slog.Int64("video_check_ms", stageDurations["video_check"].Milliseconds()),
			slog.Int64("pgm_generation_ms", stageDurations["pgm_generation"].Milliseconds()),
			slog.Int64("encoding_ms", stageDurations["encoding"].Milliseconds()),
			slog.Int64("cleanup_ms", stageDurations["cleanup"].Milliseconds()),
		)
	}()

	// ==== SECURITY VALIDATION ====
	// Validate input file path (prevents directory traversal, symlink attacks, etc.)
	if err := isValidInputPath(inputFile); err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("invalid input file: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "input_validation"})
		return fmt.Errorf("invalid input file: %w", err)
	}

	// Validate output file path (prevents directory traversal, checks parent writable)
	if err := isValidOutputPath(outputFile); err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("invalid output file: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "output_validation"})
		return fmt.Errorf("invalid output file: %w", err)
	}

	if sameOutputAsInput(inputFile, outputFile) {
		err := errors.New("the output file is the input file; choose a different destination")
		metrics.RecordError(exitCodeUnavailable, err.Error())
		RecordEncodingError(err, map[string]interface{}{"stage": "output_validation"})
		return err
	}

	// The EncodingEvent lifecycle documents a "start" event, but nothing ever
	// emitted one: the log jumped straight to progress, with no record of what
	// run those percentages belonged to.
	RecordEncodingEvent(&EncodingEvent{
		EventType:  "start",
		Message:    "encoding started",
		InputFile:  inputFile,
		OutputFile: outputFile,
	})

	// Load and validate video metadata (includes security checks)
	videoCheckStart := time.Now()
	video, err := CheckVideo(inputFile)
	stageDurations["video_check"] = time.Since(videoCheckStart)
	if err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("video validation failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{
			"stage":             "video_check",
			"stage_duration_ms": stageDurations["video_check"].Milliseconds(),
		})
		return fmt.Errorf("video validation failed: %w", err)
	}

	// ==== OBSERVABILITY: Record input metadata ====
	inputFileInfo, _ := os.Stat(inputFile)
	inputFileSize := int64(0)
	if inputFileInfo != nil {
		inputFileSize = inputFileInfo.Size()
	}
	metrics.RecordInputMetadata(video, inputFileSize)

	// Get and sanitize encoder selection from UI (whitelist validation)
	encoderInput := ui.GetEncoder()
	encoderSanitized, err := SanitizeEncoderInput(encoderInput, ffmpeg["encoders"])
	if err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("invalid encoder selection: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "encoder_sanitization"})
		return fmt.Errorf("invalid encoder selection: %w", err)
	}

	// Get encoding quality options from UI (bitrate)
	bitrate := 0

	// Bitrate mode: get bitrate from UI
	bitrateFromUI, err := ui.GetBitrate()
	if err == nil && bitrateFromUI > 0 {
		bitrate = bitrateFromUI
	}
	if bitrate == 0 {
		bitrate = video.Streams[0].BitrateInt
	}

	// Validate bitrate using configured constraints
	if err := ValidateBitrate(bitrate, cfg.MinBitrate, cfg.MaxBitrate); err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("bitrate validation failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "bitrate_validation"})
		return err
	}

	// Get encoder with full validation (uses sanitized input)
	encoder, err := FindEncoder(encoderSanitized, ffmpeg, video)
	if err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("encoder selection failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "encoder_selection"})
		return err
	}

	// Ask the UI once and reuse the answer: the size check just below, the space
	// check and the map generation further down must all agree on which geometry
	// is being produced.
	squeeze := ui.GetSqueeze()

	// Ask a hardware encoder whether it will take the frame it is about to be
	// given, and step back to the CPU here if it will not.
	//
	// Here rather than inside EncodeVideo, because everything downstream is
	// entitled to know which encoder will really run: the memory estimate
	// declines to price a hardware encoder, and the metrics and the log would
	// otherwise record one that never encoded a frame.
	encoder, _ = verifyEncoderForOutput(context.Background(), encoder, video, squeeze, ffmpeg, cfg.VideoPreset)

	profile := AnalyzeMachineProfile(ffmpeg)
	logger.Info("Machine profile analyzed",
		slog.Int("cpu_cores", profile.CPUCores),
		slog.String("hw_accels", strings.Join(profile.HardwareAccels, ",")),
		slog.String("selected_encoder", encoder),
		slog.Bool("hardware_encoder", isHardwareEncoder(encoder)),
	)

	// ==== OBSERVABILITY: Record output metadata ====
	metrics.RecordOutputMetadata(bitrate, encoder)

	if err := checkTempSpaceForMaps(video, squeeze); err != nil {
		metrics.RecordError(exitCodeUnavailable, err.Error())
		RecordEncodingError(err, map[string]interface{}{"stage": "space_check"})
		return err
	}

	// After the encoder is known, because what a conversion costs in memory
	// depends on it: a hardware encoder is not measured here, and a 10-bit
	// source only stays 10-bit on an HEVC one.
	if err := checkMemoryForEncode(video, squeeze, encoder); err != nil {
		metrics.RecordError(exitCodeUnavailable, err.Error())
		RecordEncodingError(err, map[string]interface{}{"stage": "memory_check"})
		return err
	}

	// Initialize encoding session
	if err := InitEncodingSession(cfg); err != nil {
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("session initialization failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "session_init"})
		return err
	}
	defer func() {
		cleanupStart := time.Now()
		if cleanupErr := CleanUp(); cleanupErr != nil {
			logger.Warn("Failed to cleanup encoding session", slog.String("error", cleanupErr.Error()))
		}
		stageDurations["cleanup"] = time.Since(cleanupStart)
	}()

	// Generate remap filters
	pgmStart := time.Now()
	if err := GeneratePGM(video, squeeze); err != nil {
		stageDurations["pgm_generation"] = time.Since(pgmStart)
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("filter generation failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{
			"stage":             "pgm_generation",
			"stage_duration_ms": stageDurations["pgm_generation"].Milliseconds(),
		})
		return err
	}
	stageDurations["pgm_generation"] = time.Since(pgmStart)

	// Perform encoding with progress callback + metrics recording
	detailUI, wantsDetail := ui.(ProgressDetailHandler)
	progressFunc := func(percent float64) {
		// Record before reporting: RecordProgress is what refreshes the
		// remaining-time estimate, and the UI should see the estimate that goes
		// with the percentage it is being handed.
		metrics.RecordProgress(percent)
		ui.ShowProgress(percent)
		if wantsDetail {
			detailUI.ShowProgressDetail(percent, metrics.Remaining())
		}
		RecordEncodingProgress(percent, fmt.Sprintf("Encoding: %.1f%%", percent))
	}

	// Encode into a working file and only move it into place once ffmpeg has
	// finished. ffmpeg runs with -y and used to write straight to the
	// destination, so a cancellation or a failure left a truncated .mp4 sitting
	// exactly where the user expected a finished one -- and, when overwriting,
	// destroyed the file that was already there before producing nothing.
	partialOutput, err := newPartialOutputPath(outputFile)
	if err != nil {
		metrics.RecordError(exitCodeUnavailable, err.Error())
		RecordEncodingError(err, map[string]interface{}{"stage": "encoding"})
		return err
	}
	keepPartial := false
	defer func() {
		if keepPartial {
			return
		}
		if rmErr := os.Remove(partialOutput); rmErr != nil && !os.IsNotExist(rmErr) {
			logger.Warn("Failed to remove the partial output file",
				slog.String("path", partialOutput),
				slog.String("error", rmErr.Error()),
			)
		}
	}()

	encodeStart := time.Now()
	if err := EncodeVideo(cfg, video, encoder, bitrate, partialOutput, ffmpeg, progressFunc, cancel); err != nil {
		stageDurations["encoding"] = time.Since(encodeStart)
		metrics.RecordError(exitCodeUnavailable, fmt.Sprintf("encoding failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{
			"stage":             "encoding",
			"stage_duration_ms": stageDurations["encoding"].Milliseconds(),
		})
		return err
	}
	stageDurations["encoding"] = time.Since(encodeStart)

	// Same directory, so this is atomic: the destination is either the previous
	// file or the finished one, never a half-written mixture.
	if err := os.Rename(partialOutput, outputFile); err != nil {
		// The encode is finished and correct; only its name is wrong. Deleting
		// it here -- which the deferred cleanup would do -- would turn a rename
		// error into the loss of the whole conversion, so the working file is
		// kept and the error says where it is.
		keepPartial = true
		logger.Error("The finished video could not be moved into place; it is kept under its working name",
			slog.String("partial", partialOutput),
			slog.String("destination", outputFile),
			slog.String("error", err.Error()),
		)
		err = fmt.Errorf("failed to move the finished file to %s: %w. The finished video is kept at %s; move or rename it yourself",
			outputFile, err, partialOutput)
		metrics.RecordError(exitCodeUnavailable, err.Error())
		RecordEncodingError(err, map[string]interface{}{"stage": "finalize"})
		return err
	}
	keepPartial = true

	// ==== OBSERVABILITY: Record successful completion ====
	outputFileInfo, _ := os.Stat(outputFile)
	outputFileSize := int64(0)
	if outputFileInfo != nil {
		outputFileSize = outputFileInfo.Size()
	}
	metrics.RecordCompletion(outputFileSize)
	metrics.LogMetrics(logger)
	SetLastEncodingMetrics(metrics) // Make metrics available to GUI reporting components

	logger.Info("Encoding completed successfully",
		slog.String("output_file", filepath.Base(outputFile)),
		slog.String("encoder", encoder),
		slog.Int("bitrate_bits_sec", bitrate),
		slog.String("elapsed_time", metrics.ElapsedTime().String()),
	)

	return nil
}

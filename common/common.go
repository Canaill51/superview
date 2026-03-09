package common

import (
	"bufio"
	"bytes"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"math"
	"os"
	"os/exec"
	"os/signal"
	"path/filepath"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"syscall"
	"time"
)

// Global logger instance
var logger = slog.Default()

// toolResolveCache stores resolved binary paths (ffmpeg/ffprobe) per process.
var toolResolveCache sync.Map

// Wrappers around os/signal for testability.
var signalNotify = signal.Notify
var signalStop = signal.Stop

// Wrapper around Cmd.StdoutPipe for testability of error paths.
var commandStdoutPipe = func(cmd *exec.Cmd) (io.ReadCloser, error) {
	return cmd.StdoutPipe()
}

func newFFmpegCommand(args ...string) *exec.Cmd {
	return exec.Command(resolveToolBinary("ffmpeg"), args...)
}

func newFFprobeCommand(args ...string) *exec.Cmd {
	return exec.Command(resolveToolBinary("ffprobe"), args...)
}

func resolveToolBinary(tool string) string {
	if cached, ok := toolResolveCache.Load(tool); ok {
		return cached.(string)
	}

	if path, err := exec.LookPath(tool); err == nil {
		toolResolveCache.Store(tool, path)
		return path
	}

	if runtime.GOOS == "windows" {
		if path := findWindowsToolBinary(tool); path != "" {
			toolResolveCache.Store(tool, path)
			return path
		}
	}

	toolResolveCache.Store(tool, tool)
	return tool
}

func findWindowsToolBinary(tool string) string {
	exe := tool + ".exe"
	// Common install paths used by winget/scoop/manual installs.
	candidates := []string{
		filepath.Join(os.Getenv("LOCALAPPDATA"), "Microsoft", "WinGet", "Links", exe),
		filepath.Join(os.Getenv("ProgramFiles"), "ffmpeg", "bin", exe),
		filepath.Join(os.Getenv("ProgramFiles"), "FFmpeg", "bin", exe),
		filepath.Join(os.Getenv("USERPROFILE"), "scoop", "apps", "ffmpeg", "current", "bin", exe),
	}

	for _, path := range candidates {
		if path != "" {
			if info, err := os.Stat(path); err == nil && !info.IsDir() {
				return path
			}
		}
	}

	// Winget often extracts under LOCALAPPDATA/Microsoft/WinGet/Packages.
	packageRoot := filepath.Join(os.Getenv("LOCALAPPDATA"), "Microsoft", "WinGet", "Packages")
	dirs, err := os.ReadDir(packageRoot)
	if err != nil {
		return ""
	}

	lowerExe := strings.ToLower(exe)
	for _, dir := range dirs {
		if !dir.IsDir() {
			continue
		}
		name := strings.ToLower(dir.Name())
		if !strings.Contains(name, "ffmpeg") {
			continue
		}
		root := filepath.Join(packageRoot, dir.Name())
		var found string
		_ = filepath.WalkDir(root, func(path string, d os.DirEntry, walkErr error) error {
			if walkErr != nil || d == nil || d.IsDir() {
				return nil
			}
			if strings.EqualFold(d.Name(), lowerExe) {
				found = path
				return errors.New("tool found")
			}
			return nil
		})
		if found != "" {
			return found
		}
	}

	return ""
}

// InvalidVideoError is returned when video metadata validation fails.
// It indicates issues with video dimensions, codec, duration, or bitrate information.
type InvalidVideoError struct {
	Reason string
}

func (e *InvalidVideoError) Error() string {
	return fmt.Sprintf("invalid video: %s", e.Reason)
}

// EncoderError is returned when encoder selection or validation fails.
// It indicates that the requested encoder is not available or cannot be used.
type EncoderError struct {
	Msg string
}

func (e *EncoderError) Error() string {
	return fmt.Sprintf("encoder error: %s", e.Msg)
}

// SessionError is returned when encoding session initialization or cleanup fails.
// It indicates problems with temporary directory management.
type SessionError struct {
	Msg string
}

func (e *SessionError) Error() string {
	return fmt.Sprintf("session error: %s", e.Msg)
}

// SetLogger sets the global logger instance used throughout the encoding pipeline.
// If nil is passed, the current logger is unchanged.
// This allows customization of log output format, level, and destination.
func SetLogger(l *slog.Logger) {
	if l != nil {
		logger = l
	}
}

// GetLogger returns the current global logger instance.
// Use this in handlers and UI code to log encoding progress and diagnostic information.
func GetLogger() *slog.Logger {
	return logger
}

// EncodingOptions contains all parameters for a video encoding job.
// InputFile and OutputFile are the source and destination video paths.
// Encoder selects the output video codec (empty string uses input codec).
// Bitrate is in bytes/second; 0 means use input video's bitrate.
// Squeeze applies special scaling for 4:3 video stretched to 16:9 aspect ratio.
// FfmpegInfo contains version, accelerators, and available encoders from ffmpeg.
// ProgressFunc is called with progress percentage (0-100) during encoding.
type EncodingOptions struct {
	InputFile    string
	OutputFile   string
	Encoder      string // empty string means use input codec
	Bitrate      int    // bytes/second, 0 means use input bitrate
	Squeeze      bool
	FfmpegInfo   map[string]string
	ProgressFunc func(float64)
}

// UIHandler abstracts user interface interactions between GUI components and the core pipeline.
// It allows the core encoding pipeline to be UI-agnostic and testable.
type UIHandler interface {
	// ShowError displays an error message to the user.
	ShowError(error)
	// ShowInfo displays an information or success message to the user.
	ShowInfo(msg string)
	// ShowProgress updates the progress indicator (0-100 percent).
	ShowProgress(percent float64)
	// GetBitrate returns the desired output bitrate in bytes/second.
	// Returns 0 to use the input video's bitrate.
	GetBitrate() (int, error)
	// GetEncoder returns the encoder selection (e.g., "libx265").
	// Returns empty string to use the input video's codec.
	GetEncoder() string
	// GetSqueeze returns true to apply squeeze filter for 4:3 to 16:9 scaling.
	GetSqueeze() bool
}

// EncodingSession manages temporary files for a single encoding job.
// It ensures all PGM filter maps are created in a secure, isolated directory.
type EncodingSession struct {
	tempDir  string // Path to temporary directory created with os.MkdirTemp
	pgmXPath string // Path to X-coordinate remap filter (PGM format)
	pgmYPath string // Path to Y-coordinate remap filter (PGM format)
}

var (
	currentSession *EncodingSession
	sessionMutex   sync.Mutex
)

// VideoSpecs contains metadata about a video file extracted by ffprobe.
type VideoSpecs struct {
	File    string        // Absolute path to the video file
	Streams []VideoStream // Video stream information (typically just the first stream)
}

// VideoStream contains metadata about a single video stream.
// The JSON tags correspond to ffprobe's output format.
type VideoStream struct {
	Codec         string  `json:"codec_name"`
	Width         int     `json:"width"`
	Height        int     `json:"height"`
	Duration      string  `json:"duration"`
	DurationFloat float64 `json:"-"`
	Bitrate       string  `json:"bit_rate"`
	BitrateInt    int     `json:"-"`
}

// Validate checks if video specs contain all required and valid information.
// Returns InvalidVideoError if metadata is incomplete or invalid.
func (v *VideoSpecs) Validate() error {
	if len(v.Streams) == 0 {
		return &InvalidVideoError{Reason: "no video streams found"}
	}

	stream := &v.Streams[0]

	if stream.Width <= 0 || stream.Height <= 0 {
		return &InvalidVideoError{Reason: fmt.Sprintf("invalid dimensions: %dx%d", stream.Width, stream.Height)}
	}

	if stream.DurationFloat <= 0 {
		return &InvalidVideoError{Reason: "invalid or missing duration"}
	}

	if stream.BitrateInt <= 0 {
		return &InvalidVideoError{Reason: "invalid or missing bitrate"}
	}

	if stream.Codec == "" {
		return &InvalidVideoError{Reason: "no codec information"}
	}

	return nil
}

// CheckFfmpeg discovers the installed ffmpeg version, hardware accelerators, and available H.264/H.265 encoders.
// This function must be called before encoding to verify ffmpeg is installed and identify encoder options.
// Returns a map with keys: "version", "accels" (comma-separated), and "encoders" (comma-separated).
func CheckFfmpeg() (map[string]string, error) {
	ret := make(map[string]string)

	cmd := newFFmpegCommand("-version")
	prepareBackgroundCommand(cmd)
	version, err := cmd.CombinedOutput()

	if err != nil {
		return nil, errors.New("cannot find ffmpeg/ffprobe on your system\nmake sure to install it first: https://github.com/Canaill51/superview?tab=readme-ov-file#requirements")
	}

	ret["version"] = strings.Split(string(version), " ")[2]

	// split on newline, skip first line
	cmd = newFFmpegCommand("-hwaccels", "-hide_banner")
	prepareBackgroundCommand(cmd)
	accels, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("failed to query ffmpeg hardware accelerators: %w", err)
	}
	accelsArr := strings.Split(strings.ReplaceAll(string(accels), "\r\n", "\n"), "\n")
	for i := 1; i < len(accelsArr); i++ {
		if len(accelsArr[i]) != 0 {
			ret["accels"] += accelsArr[i] + ","
		}
	}

	// split on newline, skip first 10 lines
	cmd = newFFmpegCommand("-encoders", "-hide_banner")
	prepareBackgroundCommand(cmd)
	encoders, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("failed to query ffmpeg encoders: %w", err)
	}
	encodersArr := strings.Split(strings.ReplaceAll(string(encoders), "\r\n", "\n"), "\n")
	for i := 10; i < len(encodersArr); i++ {
		if strings.Index(encodersArr[i], " V") == 0 {
			enc := strings.Split(encodersArr[i], " ")
			// Filter encoders based on configured codec preferences
			for _, codec := range GetConfig().EncoderCodecs {
				if strings.Contains(enc[2], codec) {
					ret["encoders"] += enc[2] + ","
					break
				}
			}
		}
	}

	ret["accels"] = strings.Trim(ret["accels"], ",")
	ret["encoders"] = strings.Trim(ret["encoders"], ",")

	return ret, nil
}

// GetHeader returns a formatted string with ffmpeg information for display to the user.
func GetHeader(ffmpeg map[string]string) string {
	return fmt.Sprintf("- ffmpeg version: %s\n- Hardware accelerators: %s\n- H.264/H.265 encoders: %s\n\n", ffmpeg["version"], ffmpeg["accels"], ffmpeg["encoders"])
}

// InitEncodingSession creates a new secure temporary directory for this encoding job.
// Call this before GeneratePGM and EncodeVideo.
// Always use defer common.CleanUp() to guarantee cleanup even on error.
func InitEncodingSession() error {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession != nil {
		return errors.New("encoding session already active")
	}

	tempDir, err := os.MkdirTemp("", GetConfig().TempDirPrefix)
	if err != nil {
		return fmt.Errorf("failed to create temp directory: %w", err)
	}

	session := &EncodingSession{
		tempDir:  tempDir,
		pgmXPath: filepath.Join(tempDir, "x.pgm"),
		pgmYPath: filepath.Join(tempDir, "y.pgm"),
	}

	currentSession = session
	return nil
}

// CloseEncodingSession closes the current encoding session and removes its temporary directory.
// This function is idempotent and safe to call multiple times.
func CloseEncodingSession() error {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession == nil {
		return nil
	}

	defer func() {
		currentSession = nil
	}()

	return os.RemoveAll(currentSession.tempDir)
}

func getSessionPaths() (xPath, yPath string, err error) {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession == nil {
		return "", "", errors.New("no encoding session active")
	}

	return currentSession.pgmXPath, currentSession.pgmYPath, nil
}

// CheckVideo loads and validates video metadata using ffprobe.
// It extracts codec, dimensions, duration, and bitrate from the first video stream.
// Returns InvalidVideoError if required metadata is missing or invalid.
func CheckVideo(file string) (*VideoSpecs, error) {
	// Check specs of the input video (codec, dimensions, duration, bitrate)
	cmd := newFFprobeCommand("-i", file, "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,width,height,duration,bit_rate", "-print_format", "json")
	prepareBackgroundCommand(cmd)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("Error running ffprobe, output is:\n%s", out)
	}

	// Parse ffprobe output
	var response struct {
		Streams []VideoStream `json:"streams"`
	}
	if err := json.Unmarshal(out, &response); err != nil {
		return nil, fmt.Errorf("failed to parse video metadata: %w", err)
	}

	if len(response.Streams) == 0 {
		return nil, &InvalidVideoError{Reason: "no video streams in file"}
	}

	specs := &VideoSpecs{
		File:    file,
		Streams: response.Streams,
	}

	// Parse duration from first stream
	durationFloat, err := strconv.ParseFloat(specs.Streams[0].Duration, 64)
	if err != nil {
		return nil, fmt.Errorf("invalid duration value '%s': %w", specs.Streams[0].Duration, err)
	}
	specs.Streams[0].DurationFloat = durationFloat

	// Parse bitrate from first stream
	if specs.Streams[0].Bitrate == "" {
		return nil, &InvalidVideoError{Reason: "bitrate information not available"}
	}
	bitrateInt, err := strconv.Atoi(specs.Streams[0].Bitrate)
	if err != nil {
		return nil, fmt.Errorf("invalid bitrate value '%s': %w", specs.Streams[0].Bitrate, err)
	}
	specs.Streams[0].BitrateInt = bitrateInt

	// Validate all required data is present
	if err := specs.Validate(); err != nil {
		return nil, err
	}

	return specs, nil
}

// GeneratePGM creates the remap filter maps for ffmpeg that apply the superview distortion.
// The maps are saved as PGM (Portable Graymap) files in the current encoding session's temp directory.
// If squeeze is true, applies asymmetric scaling for 4:3 video stretched to 16:9.
// If squeeze is false, applies symmetric barrel-distortion correction.
func GeneratePGM(video *VideoSpecs, squeeze bool) error {
	// Validate video before processing
	if err := video.Validate(); err != nil {
		return err
	}

	var outX int

	if squeeze {
		outX = video.Streams[0].Width
	} else {
		outX = int(float64(video.Streams[0].Height)*(16.0/9.0)) / 2 * 2 // multiplier of 2
	}
	outY := video.Streams[0].Height

	logger.Info("Scaling video",
		slog.String("file", video.File),
		slog.String("codec", video.Streams[0].Codec),
		slog.Int("duration_secs", int(video.Streams[0].DurationFloat)),
		slog.Int("input_width", video.Streams[0].Width),
		slog.Int("input_height", video.Streams[0].Height),
		slog.Int("output_width", outX),
		slog.Int("output_height", outY),
		slog.Bool("squeeze", squeeze),
	)

	// Generate PGM P2 files for remap filter, see https://trac.ffmpeg.org/wiki/RemapFilter
	xPath, yPath, err := getSessionPaths()
	if err != nil {
		return err
	}

	fX, err := os.Create(xPath)
	if err != nil {
		return fmt.Errorf("failed to create temp file x.pgm: %w", err)
	}
	fY, err := os.Create(yPath)
	if err != nil {
		fX.Close()
		return fmt.Errorf("failed to create temp file y.pgm: %w", err)
	}
	defer fX.Close()
	defer fY.Close()

	// Pre-allocate buffers for efficient line generation (optimization for Étape 9)
	// Estimate: each number ~5 chars + space = 6 bytes per pixel, plus newline
	bufXCapacity := outX * 8
	bufYCapacity := outX * 8

	var bufX, bufY []byte

	// Write PGM headers
	headerX := fmt.Sprintf("P2 %d %d 65535\n", outX, outY)
	headerY := fmt.Sprintf("P2 %d %d 65535\n", outX, outY)

	if _, err := fX.WriteString(headerX); err != nil {
		return fmt.Errorf("failed to write header to x.pgm: %w", err)
	}
	if _, err := fY.WriteString(headerY); err != nil {
		return fmt.Errorf("failed to write header to y.pgm: %w", err)
	}

	for y := 0; y < outY; y++ {
		// Reset buffers for this line (optimization: reuse allocation)
		bufX = bufX[:0]
		bufY = bufY[:0]

		// Ensure buffer capacity before appending
		if cap(bufX) < bufXCapacity {
			bufX = make([]byte, 0, bufXCapacity)
		}
		if cap(bufY) < bufYCapacity {
			bufY = make([]byte, 0, bufYCapacity)
		}

		for x := 0; x < outX; x++ {
			sx := float64(x) - float64(outX-video.Streams[0].Width)/2.0 // x - width diff/2
			tx := (float64(x)/float64(outX) - 0.5) * 2.0                // (x/width - 0.5) * 2

			var offset float64

			if squeeze {
				inv := 1 - math.Abs(tx)

				offset = inv*(float64((outX/16)*7)/2.0) - math.Pow((inv/16)*7, 2)*(float64((outX/7)*16)/2.0)

				if tx < 0 {
					offset *= -1
				}

				bufX = strconv.AppendInt(bufX, int64(int(sx+offset)), 10)
			} else {
				offset = math.Pow(tx, 2) * (float64(outX-video.Streams[0].Width) / 2.0) // tx^2 * width diff/2

				if tx < 0 {
					offset *= -1
				}

				bufX = strconv.AppendInt(bufX, int64(int(sx-offset)), 10)
			}

			bufX = append(bufX, ' ')

			bufY = strconv.AppendInt(bufY, int64(y), 10)
			bufY = append(bufY, ' ')
		}
		bufX = append(bufX, '\n')
		bufY = append(bufY, '\n')

		if _, err := fX.Write(bufX); err != nil {
			return fmt.Errorf("failed to write x.pgm: %w", err)
		}
		if _, err := fY.Write(bufY); err != nil {
			return fmt.Errorf("failed to write y.pgm: %w", err)
		}
	}

	logger.Info("Filter files generated successfully")

	return nil
}

// ValidateBitrate checks if the given bitrate is within acceptable constraints.
// minBitrate and maxBitrate define the valid range in bytes/second.
// If either constraint is 0, that constraint is not applied.
// Returns an error describing the validation failure.
func ValidateBitrate(bitrate int, minBitrate int, maxBitrate int) error {
	if bitrate <= 0 {
		return fmt.Errorf("bitrate must be positive, got %d", bitrate)
	}
	if minBitrate > 0 && bitrate < minBitrate {
		return fmt.Errorf("bitrate %d is below minimum %d bytes/second", bitrate, minBitrate)
	}
	if maxBitrate > 0 && bitrate > maxBitrate {
		return fmt.Errorf("bitrate %d exceeds maximum %d bytes/second", bitrate, maxBitrate)
	}
	return nil
}

// FindEncoder selects the best available video encoder for the job.
// If codec is empty, selects the best encoder based on machine profile (GPU first, CPU fallback).
// Otherwise, searches the ffmpeg encoders list for the requested codec.
// Returns EncoderError if the requested encoder is not available.
func FindEncoder(codec string, ffmpeg map[string]string, video *VideoSpecs) (string, error) {
	if len(video.Streams) == 0 {
		return "", &InvalidVideoError{Reason: "no video streams"}
	}

	profile := AnalyzeMachineProfile(ffmpeg)
	encoder := ""

	if codec != "" {
		if !canUseEncoderWithProfile(codec, profile) {
			return "", &EncoderError{Msg: fmt.Sprintf("encoder %s not available. Available encoders: %s", codec, ffmpeg["encoders"])}
		}
		encoder = codec
	} else {
		for _, candidate := range candidateEncodersForCodec(video.Streams[0].Codec) {
			if canUseEncoderWithProfile(candidate, profile) {
				encoder = candidate
				break
			}
		}

		if encoder == "" {
			for _, enc := range profile.AvailableEncoders {
				if enc != "" {
					encoder = enc
					break
				}
			}
		}
	}

	if encoder == "" {
		return "", &EncoderError{Msg: "no encoder found"}
	}

	return encoder, nil
}

// EncodeVideo runs ffmpeg with the remap filter to apply the superview distortion.
// It reads PGM filter maps from the current session and encodes using the specified encoder and bitrate.
// The callback function is called with progress percentage (0-100) for UI updates.
// Returns nil on successful completion, or an error if ffmpeg fails.
func EncodeVideo(video *VideoSpecs, encoder string, bitrate int, output string, ffmpeg map[string]string, callback func(float64)) error {
	// Get the session paths for PGM files
	xPath, yPath, err := getSessionPaths()
	if err != nil {
		return err
	}

	safePerformanceMode := GetConfig().IsSafePerformanceMode()
	cfg := GetConfig()
	encoderThreads := runtime.NumCPU()
	if cfg != nil && cfg.EncoderThreads > 0 {
		encoderThreads = cfg.EncoderThreads
	}
	filterThreads := 0
	if cfg != nil && cfg.FilterThreads > 0 {
		filterThreads = cfg.FilterThreads
	}
	videoPreset := ""
	if cfg != nil {
		videoPreset = cfg.VideoPreset
	}

	buildBaseArgs := func(audioCodec string) []string {
		baseArgs := []string{
			"-hide_banner", "-progress", "pipe:1", "-loglevel", "error", "-y",
		}
		if !safePerformanceMode {
			baseArgs = append(baseArgs, "-re")
		}

		baseArgs = append(baseArgs,
			"-threads", strconv.Itoa(encoderThreads),
			"-i", video.File, "-i", xPath, "-i", yPath,
			"-filter_complex", "[0:v:0][1:v:0][2:v:0]remap,format=yuv444p,format=yuv420p",
			"-c:v", encoder, "-b:v", strconv.Itoa(bitrate), "-c:a", audioCodec,
		)

		if filterThreads > 0 {
			baseArgs = append(baseArgs, "-filter_threads", strconv.Itoa(filterThreads))
		}

		if videoPreset != "" {
			baseArgs = append(baseArgs, "-preset", videoPreset)
		}

		if encoder == "libx265" {
			baseArgs = append(baseArgs, "-x265-params", "log-level=error")
		}

		return baseArgs
	}

	run := func(hwaccel string, audioCodec string) error {
		baseArgs := buildBaseArgs(audioCodec)
		args := make([]string, 0, len(baseArgs)+4)
		if hwaccel != "" {
			args = append(args, "-hwaccel", hwaccel)
		}
		args = append(args, baseArgs...)
		args = append(args, output)

		cmd := newFFmpegCommand(args...)
		prepareBackgroundCommand(cmd)
		stdout, err := commandStdoutPipe(cmd)
		stderrBytes := new(bytes.Buffer)
		cmd.Stderr = stderrBytes

		if err != nil {
			return err
		}
		rd := bufio.NewReader(stdout)

		err = cmd.Start()
		if err != nil {
			return fmt.Errorf("Error starting ffmpeg, output is:\n%s", err)
		}

		// Stop ffmpeg on Ctrl+C and return a clean interruption error.
		sigC := make(chan os.Signal, 1)
		done := make(chan struct{})
		interrupted := make(chan struct{}, 1)
		signalNotify(sigC, os.Interrupt, syscall.SIGTERM)
		defer signalStop(sigC)
		go func() {
			select {
			case <-sigC:
				if cmd.Process != nil {
					_ = cmd.Process.Kill()
				}
				select {
				case interrupted <- struct{}{}:
				default:
				}
			case <-done:
			}
		}()
		defer close(done)

		for {
			line, _, err := rd.ReadLine()

			if err == io.EOF {
				logger.Debug("Encoding complete")
				break
			}

			if bytes.Contains(line, []byte("out_time_ms=")) {
				time := bytes.Replace(line, []byte("out_time_ms="), nil, 1)
				timeF, err := strconv.ParseFloat(string(time), 64)
				if err != nil {
					logger.Warn("Failed to parse progress value",
						slog.String("raw_value", string(time)),
						slog.String("error", err.Error()),
					)
					continue
				}
				callback(math.Min(timeF/(video.Streams[0].DurationFloat*10000), 100))
			}
		}

		if err := cmd.Wait(); err != nil {
			select {
			case <-interrupted:
				return errors.New("encoding interrupted by user")
			default:
			}
			if stderrBytes.Len() > 0 {
				return fmt.Errorf("Error running ffmpeg, output is:\n%s\nffmpeg stderr:\n%s", err, stderrBytes.String())
			}
			return fmt.Errorf("Error running ffmpeg, output is:\n%s", err)
		}

		return nil
	}

	runWithAudioFallback := func(hwaccel string) error {
		preferredAudioCodec := "aac"
		if safePerformanceMode {
			preferredAudioCodec = "copy"
		}

		err := run(hwaccel, preferredAudioCodec)
		if err == nil {
			return nil
		}

		if safePerformanceMode && preferredAudioCodec == "copy" {
			logger.Warn("Audio stream copy failed, retrying with AAC",
				slog.String("error", err.Error()),
			)
			return run(hwaccel, "aac")
		}

		return err
	}

	profile := AnalyzeMachineProfile(ffmpeg)
	hwaccel := accelForEncoder(encoder)
	if hwaccel != "" {
		if toSet(profile.HardwareAccels)[hwaccel] {
			logger.Info("Trying hardware decode+encode path",
				slog.String("encoder", encoder),
				slog.String("hwaccel", hwaccel),
			)
			if err := runWithAudioFallback(hwaccel); err == nil {
				return nil
			}
			logger.Warn("Hardware decode path failed, falling back to CPU decode",
				slog.String("encoder", encoder),
				slog.String("hwaccel", hwaccel),
			)
		}
	}

	logger.Info("Using CPU decode path",
		slog.String("encoder", encoder),
		slog.Int("threads", encoderThreads),
		slog.Int("filter_threads", filterThreads),
		slog.String("video_preset", videoPreset),
	)
	err = runWithAudioFallback("")
	if err == nil {
		return nil
	}

	if isHardwareEncoder(encoder) {
		fallbackEncoder := ""
		if strings.Contains(encoder, "h264") {
			fallbackEncoder = "libx264"
		} else if strings.Contains(encoder, "hevc") || strings.Contains(encoder, "265") {
			fallbackEncoder = "libx265"
		}

		if fallbackEncoder != "" && fallbackEncoder != encoder && canUseEncoderWithProfile(fallbackEncoder, profile) {
			logger.Warn("Hardware encoder failed, retrying with CPU encoder",
				slog.String("failed_encoder", encoder),
				slog.String("fallback_encoder", fallbackEncoder),
			)
			return EncodeVideo(video, fallbackEncoder, bitrate, output, ffmpeg, callback)
		}
	}

	return err
}

func CleanUp() error {
	return CloseEncodingSession()
}

// PerformEncoding orchestrates the complete encoding workflow from input file to output file.
// It coordinates all steps: validation, metadata extraction, option gathering, encoding, and cleanup.
// The ui parameter handles user interaction (showing errors, progress, getting options).
// Returns nil on success, or an error if any step fails.
// Call this from GUI entry points only; the logic is pipeline-agnostic.
// Security: Validates input/output paths and encoder selection for defensive programming.
// Observability: Records metrics and events throughout the pipeline.
func PerformEncoding(inputFile string, outputFile string, ui UIHandler, ffmpeg map[string]string) error {
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
		metrics.RecordError(-1, fmt.Sprintf("invalid input file: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "input_validation"})
		return fmt.Errorf("invalid input file: %w", err)
	}

	// Validate output file path (prevents directory traversal, checks parent writable)
	if err := isValidOutputPath(outputFile); err != nil {
		metrics.RecordError(-1, fmt.Sprintf("invalid output file: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "output_validation"})
		return fmt.Errorf("invalid output file: %w", err)
	}

	// Load and validate video metadata (includes security checks)
	videoCheckStart := time.Now()
	video, err := CheckVideo(inputFile)
	stageDurations["video_check"] = time.Since(videoCheckStart)
	if err != nil {
		metrics.RecordError(-1, fmt.Sprintf("video validation failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{
			"stage":            "video_check",
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
		metrics.RecordError(-1, fmt.Sprintf("invalid encoder selection: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "encoder_sanitization"})
		return fmt.Errorf("invalid encoder selection: %w", err)
	}

	// Get encoding options from UI
	bitrate := 0
	bitrateFromUI, err := ui.GetBitrate()
	if err == nil && bitrateFromUI > 0 {
		bitrate = bitrateFromUI
	}
	if bitrate == 0 {
		bitrate = video.Streams[0].BitrateInt
	}

	// Validate bitrate using configured constraints
	cfg := GetConfig()
	if err := ValidateBitrate(bitrate, cfg.MinBitrate, cfg.MaxBitrate); err != nil {
		metrics.RecordError(-1, fmt.Sprintf("bitrate validation failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "bitrate_validation"})
		return err
	}

	// Get encoder with full validation (uses sanitized input)
	encoder, err := FindEncoder(encoderSanitized, ffmpeg, video)
	if err != nil {
		metrics.RecordError(-1, fmt.Sprintf("encoder selection failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{"stage": "encoder_selection"})
		return err
	}

	profile := AnalyzeMachineProfile(ffmpeg)
	logger.Info("Machine profile analyzed",
		slog.Int("cpu_cores", profile.CPUCores),
		slog.String("hw_accels", strings.Join(profile.HardwareAccels, ",")),
		slog.String("selected_encoder", encoder),
		slog.Bool("hardware_encoder", isHardwareEncoder(encoder)),
	)

	// ==== OBSERVABILITY: Record output metadata ====
	metrics.RecordOutputMetadata(bitrate, encoder)

	// Initialize encoding session
	if err := InitEncodingSession(); err != nil {
		metrics.RecordError(-1, fmt.Sprintf("session initialization failed: %v", err))
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
	if err := GeneratePGM(video, ui.GetSqueeze()); err != nil {
		stageDurations["pgm_generation"] = time.Since(pgmStart)
		metrics.RecordError(-1, fmt.Sprintf("filter generation failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{
			"stage":            "pgm_generation",
			"stage_duration_ms": stageDurations["pgm_generation"].Milliseconds(),
		})
		return err
	}
	stageDurations["pgm_generation"] = time.Since(pgmStart)

	// Perform encoding with progress callback + metrics recording
	progressFunc := func(percent float64) {
		ui.ShowProgress(percent)
		metrics.RecordProgress(percent)
		RecordEncodingProgress(percent, fmt.Sprintf("Encoding: %.1f%%", percent))
	}

	encodeStart := time.Now()
	if err := EncodeVideo(video, encoder, bitrate, outputFile, ffmpeg, progressFunc); err != nil {
		stageDurations["encoding"] = time.Since(encodeStart)
		metrics.RecordError(-1, fmt.Sprintf("encoding failed: %v", err))
		RecordEncodingError(err, map[string]interface{}{
			"stage":            "encoding",
			"stage_duration_ms": stageDurations["encoding"].Milliseconds(),
		})
		return err
	}
	stageDurations["encoding"] = time.Since(encodeStart)

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
		slog.Int("bitrate_bytes_sec", bitrate),
		slog.String("elapsed_time", metrics.ElapsedTime().String()),
	)

	return nil
}

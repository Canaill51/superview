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
	"strconv"
	"strings"
	"sync"
	"syscall"
)

// Global logger instance
var logger = slog.Default()

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

// UIHandler abstracts user interface interactions between CLI and GUI implementations.
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
	File    string         // Absolute path to the video file
	Streams []VideoStream  // Video stream information (typically just the first stream)
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

	cmd := exec.Command("ffmpeg", "-version")
	prepareBackgroundCommand(cmd)
	version, err := cmd.CombinedOutput()

	if err != nil {
		return nil, errors.New("Cannot find ffmpeg/ffprobe on your system.\nMake sure to install it first: https://github.com/Niek/superview/#requirements")
	}

	ret["version"] = strings.Split(string(version), " ")[2]

	// split on newline, skip first line
	cmd = exec.Command("ffmpeg", "-hwaccels", "-hide_banner")
	prepareBackgroundCommand(cmd)
	accels, err := cmd.CombinedOutput()
	accelsArr := strings.Split(strings.ReplaceAll(string(accels), "\r\n", "\n"), "\n")
	for i := 1; i < len(accelsArr); i++ {
		if len(accelsArr[i]) != 0 {
			ret["accels"] += accelsArr[i] + ","
		}
	}

	// split on newline, skip first 10 lines
	cmd = exec.Command("ffmpeg", "-encoders", "-hide_banner")
	prepareBackgroundCommand(cmd)
	encoders, err := cmd.CombinedOutput()
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

func isValidPath(path string) bool {
	// Ensure path doesn't escape the intended directory via .. or other tricks
	clean := filepath.Clean(path)
	return clean == path && filepath.IsAbs(clean)
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
		tempDir: tempDir,
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
	cmd := exec.Command("ffprobe", "-i", file, "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,width,height,duration,bit_rate", "-print_format", "json")
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

	wX := bufio.NewWriter(fX)
	wY := bufio.NewWriter(fY)

	wX.WriteString(fmt.Sprintf("P2 %d %d 65535\n", outX, outY))
	wY.WriteString(fmt.Sprintf("P2 %d %d 65535\n", outX, outY))

	for y := 0; y < outY; y++ {
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

				wX.WriteString(strconv.Itoa(int(sx + offset)))
			} else {
				offset = math.Pow(tx, 2) * (float64(outX-video.Streams[0].Width) / 2.0) // tx^2 * width diff/2

				if tx < 0 {
					offset *= -1
				}

				wX.WriteString(strconv.Itoa(int(sx - offset)))
			}

			wX.WriteString(" ")

			wY.WriteString(strconv.Itoa(y))
			wY.WriteString(" ")
		}
		wX.WriteString("\n")
		wY.WriteString("\n")
	}

	wX.Flush()
	wY.Flush()

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
// If codec is empty, returns the input video's original codec.
// Otherwise, searches the ffmpeg encoders list for the requested codec.
// Returns EncoderError if the requested encoder is not available.
func FindEncoder(codec string, ffmpeg map[string]string, video *VideoSpecs) (string, error) {
	if len(video.Streams) == 0 {
		return "", &InvalidVideoError{Reason: "no video streams"}
	}

	encoder := video.Streams[0].Codec

	if codec != "" {
		found := false
		for _, enc := range strings.Split(ffmpeg["encoders"], ",") {
			if enc == codec {
				encoder = enc
				found = true
				break
			}
		}
		if !found {
			return "", &EncoderError{Msg: fmt.Sprintf("encoder %s not available. Available encoders: %s", codec, ffmpeg["encoders"])}
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
func EncodeVideo(video *VideoSpecs, encoder string, bitrate int, output string, callback func(float64)) error {
	// Get the session paths for PGM files
	xPath, yPath, err := getSessionPaths()
	if err != nil {
		return err
	}

	// Starting encoder, write progress to stdout pipe
	cmd := exec.Command("ffmpeg", "-hide_banner", "-progress", "pipe:1", "-loglevel", "panic", "-y", "-re", "-i", video.File, "-i", xPath, "-i", yPath, "-filter_complex", "remap,format=yuv444p,format=yuv420p", "-c:v", encoder, "-b:v", strconv.Itoa(bitrate), "-c:a", "aac", "-x265-params", "log-level=error", output)
	prepareBackgroundCommand(cmd)
	stdout, err := cmd.StdoutPipe()
	rd := bufio.NewReader(stdout)

	if err != nil {
		return err
	}

	err = cmd.Start()
	if err != nil {
		return fmt.Errorf("Error starting ffmpeg, output is:\n%s", err)
	}

	// Kill encoder process on Ctrl+C
	sigC := make(chan os.Signal, 1)
	signal.Notify(sigC, os.Interrupt, syscall.SIGTERM)
	go func() {
		<-sigC
		cmd.Process.Kill()
		os.Exit(1)
	}()

	// Read and parse progress
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
				// Log warning but continue, don't fail the entire encode
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
		return fmt.Errorf("Error running ffmpeg, output is:\n%s", err)
	}

	return nil
}

func CleanUp() error {
	return CloseEncodingSession()
}

// PerformEncoding orchestrates the complete encoding workflow from input file to output file.
// It coordinates all steps: validation, metadata extraction, option gathering, encoding, and cleanup.
// The ui parameter handles user interaction (showing errors, progress, getting options).
// Returns nil on success, or an error if any step fails.
// Call this from entry points (CLI/GUI) only; the logic is pipeline-agnostic.
func PerformEncoding(inputFile string, outputFile string, ui UIHandler, ffmpeg map[string]string) error {
	// Check input file exists
	_, err := os.Stat(inputFile)
	if err != nil {
		return fmt.Errorf("input file not found: %s", inputFile)
	}

	// Load video metadata
	video, err := CheckVideo(inputFile)
	if err != nil {
		return err
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
		return err
	}

	// Get encoder selection from UI
	encoder, err := FindEncoder(ui.GetEncoder(), ffmpeg, video)
	if err != nil {
		return err
	}

	// Initialize encoding session
	if err := InitEncodingSession(); err != nil {
		return err
	}
	defer CleanUp()

	// Generate remap filters
	if err := GeneratePGM(video, ui.GetSqueeze()); err != nil {
		return err
	}

	// Perform encoding with progress callback
	progressFunc := func(percent float64) {
		ui.ShowProgress(percent)
	}

	if err := EncodeVideo(video, encoder, bitrate, outputFile, progressFunc); err != nil {
		return err
	}

	return nil
}


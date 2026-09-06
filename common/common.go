package common

import (
	"bufio"
	"bytes"
	"context"
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

// newFFmpegCommandContext is newFFmpegCommand with a deadline. The encoder
// probes use it: a driver that is present but wedged answers nothing at all,
// and a probe that never returns would hang the window it is meant to inform.
func newFFmpegCommandContext(ctx context.Context, args ...string) *exec.Cmd {
	return exec.CommandContext(ctx, resolveToolBinary("ffmpeg"), args...)
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

	// Deliberately not cached: the GUI shows an install link when ffmpeg is
	// missing, so the user very often installs it while the app is running.
	// Caching the failure would force a restart with no hint that one is needed.
	return tool
}

// ResetToolResolutionCache forgets previously resolved ffmpeg/ffprobe paths so
// the next call re-runs discovery. Call it when the user asks to retry after
// installing the tools.
func ResetToolResolutionCache() {
	toolResolveCache.Clear()
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

// ErrCancelled is returned when the user stops a conversion, either through the
// Cancel button or with Ctrl+C.
//
// It has to be a sentinel rather than a bare errors.New string. EncodeVideo
// retries a failed encode on progressively safer paths -- CPU decode, then the
// equivalent CPU encoder -- and it decides by looking at the error. With an
// untyped error a cancellation was indistinguishable from an encoder failure,
// so asking to stop started up to three more ffmpeg processes instead of none.
// Callers should test it with errors.Is, never by matching the message.
var ErrCancelled = errors.New("encoding interrupted by user")

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

// exitCodeUnavailable is recorded when a stage fails before ffmpeg ever ran,
// so there is no process exit code to report.
const exitCodeUnavailable = -1

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
	// PixFmt is ffprobe's pixel format name, e.g. "yuv420p" or "yuv420p10le".
	// It is what tells the filter chain whether the source carries more than 8
	// bits per sample. Absent on very old ffprobe builds, which is handled as
	// 8-bit rather than guessed at.
	PixFmt string `json:"pix_fmt"`
	// RFrameRate is ffprobe's frame rate as a rational string, e.g. "30/1" or
	// "30000/1001". FrameRate holds it parsed, or 0 when it is missing or
	// unusable -- the encoding speed metric is then simply not published rather
	// than computed from a guess.
	RFrameRate string  `json:"r_frame_rate"`
	FrameRate  float64 `json:"-"`
}

// parseFrameRate turns ffprobe's rational frame rate into frames per second.
//
// Returns 0 for anything it cannot make sense of, including the "0/0" ffprobe
// reports for streams whose rate it could not determine. 0 means "unknown" to
// every caller; none of them substitutes a default.
func parseFrameRate(rational string) float64 {
	numText, denText, found := strings.Cut(strings.TrimSpace(rational), "/")
	if !found {
		value, err := strconv.ParseFloat(strings.TrimSpace(rational), 64)
		if err != nil || value <= 0 {
			return 0
		}
		return value
	}

	num, numErr := strconv.ParseFloat(strings.TrimSpace(numText), 64)
	den, denErr := strconv.ParseFloat(strings.TrimSpace(denText), 64)
	if numErr != nil || denErr != nil || den == 0 || num <= 0 {
		return 0
	}
	return num / den
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
// cfg supplies EncoderCodecs; nil means the built-in defaults.
func CheckFfmpeg(cfg *Config) (map[string]string, error) {
	cfg = configOrDefault(cfg)
	ret := make(map[string]string)

	cmd := newFFmpegCommand("-version")
	prepareBackgroundCommand(cmd)
	version, err := cmd.CombinedOutput()

	if err != nil {
		return nil, errors.New("cannot find ffmpeg/ffprobe on your system\nmake sure to install it first: https://github.com/Canaill51/superview?tab=readme-ov-file#requirements")
	}

	ret["version"] = parseFFmpegVersion(string(version))

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
	ret["encoders"] = strings.Join(parseFFmpegEncoders(string(encoders), cfg.EncoderCodecs), ",")

	ret["accels"] = strings.Trim(ret["accels"], ",")

	return ret, nil
}

// parseFFmpegVersion extracts the version token from "ffmpeg version N.N.N ...".
//
// It never panics on an unexpected layout: a custom, patched or localised build
// must degrade to "unknown" rather than take the whole app down at startup.
func parseFFmpegVersion(output string) string {
	line, _, _ := strings.Cut(output, "\n")
	fields := strings.Fields(line)
	if len(fields) < 3 {
		return "unknown"
	}
	return fields[2]
}

// parseFFmpegEncoders extracts video encoder names matching any of the given
// codec fragments from the output of "ffmpeg -encoders".
//
// The listing is "<header>\n ------\n <flags> <name> <description>". Rather
// than skipping a hard-coded number of header lines, this scans for the "------"
// separator and falls back to accepting every well-formed entry when it is
// absent, so a build with a differently sized banner still works.
func parseFFmpegEncoders(output string, codecs []string) []string {
	lines := strings.Split(strings.ReplaceAll(output, "\r\n", "\n"), "\n")

	start := 0
	for i, line := range lines {
		if strings.Contains(line, "------") {
			start = i + 1
			break
		}
	}

	var found []string
	for _, line := range lines[start:] {
		// A video encoder row starts with a space then the "V" capability flag.
		if !strings.HasPrefix(line, " V") {
			continue
		}
		fields := strings.Fields(line)
		if len(fields) < 2 {
			continue
		}
		name := fields[1]
		for _, codec := range codecs {
			if strings.Contains(name, codec) {
				found = append(found, name)
				break
			}
		}
	}
	return found
}

// InitEncodingSession creates a new secure temporary directory for this encoding job.
// Call this before GeneratePGM and EncodeVideo.
// Always use defer common.CleanUp() to guarantee cleanup even on error.
// cfg supplies TempDirPrefix; nil means the built-in defaults.
func InitEncodingSession(cfg *Config) error {
	sessionMutex.Lock()
	defer sessionMutex.Unlock()

	if currentSession != nil {
		return errors.New("encoding session already active")
	}

	tempDir, err := os.MkdirTemp("", configOrDefault(cfg).TempDirPrefix)
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
	cmd := newFFprobeCommand("-i", file, "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=codec_name,width,height,duration,bit_rate,pix_fmt,r_frame_rate", "-print_format", "json")
	prepareBackgroundCommand(cmd)
	out, err := cmd.CombinedOutput()
	if err != nil {
		return nil, fmt.Errorf("ffprobe failed, output:\n%s", out)
	}

	return parseVideoSpecs(file, out)
}

// parseVideoSpecs turns ffprobe's JSON into a validated VideoSpecs.
//
// Split out from CheckVideo so it can be tested against recorded ffprobe
// output. This is the only place the program reads something it did not
// produce itself, and every one of its rejection paths -- malformed JSON, no
// stream, an unparsable duration, a missing or non-numeric bitrate -- used to
// be reachable only by finding a video file that provoked it, which is to say
// not at all. See common/testdata/ffprobe.
func parseVideoSpecs(file string, out []byte) (*VideoSpecs, error) {
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

	// Optional: a missing or unusable frame rate is not a reason to reject a
	// file, it only means the encoding-speed metric stays unpublished.
	specs.Streams[0].FrameRate = parseFrameRate(specs.Streams[0].RFrameRate)

	// Validate all required data is present
	if err := specs.Validate(); err != nil {
		return nil, err
	}

	return specs, nil
}

// remapOutputSize returns the dimensions of the remapped frame, which are also
// the dimensions of each remap map.
//
// Shared by GeneratePGM and by the space check that runs before it: computing
// the geometry twice is exactly how the two would drift apart, and the check
// would then be reserving space for a map of a different size than the one
// about to be written.
func remapOutputSize(video *VideoSpecs, squeeze bool) (outX, outY int) {
	if squeeze {
		outX = video.Streams[0].Width
	} else {
		outX = int(float64(video.Streams[0].Height)*(16.0/9.0)) / 2 * 2 // multiplier of 2
	}
	return outX, video.Streams[0].Height
}

// remapMapBytes returns the number of bytes the two remap maps will take.
//
// Two files, one 16-bit sample per pixel, plus a short ASCII header each. Exact
// rather than approximate: TestRemapMapBytes_MatchesWhatGeneratePGMWrites
// compares it against the files actually produced.
func remapMapBytes(video *VideoSpecs, squeeze bool) int64 {
	if video == nil || len(video.Streams) == 0 {
		return 0
	}
	outX, outY := remapOutputSize(video, squeeze)
	header := int64(len(fmt.Sprintf("P5 %d %d 65535\n", outX, outY)))
	perMap := header + int64(outX)*int64(outY)*2
	return perMap * 2
}

// checkTempSpaceForMaps refuses to start when the temporary filesystem cannot
// hold the remap maps.
//
// A 12 Mpx GoPro frame needs 66 MB of maps, and on most current distributions
// the temporary directory is a tmpfs -- which is to say RAM. Without this the
// shortfall surfaced halfway through as an opaque write error from a stage the
// user has no reason to know exists.
//
// A failed probe is not a refusal: if the free space cannot be read, the encode
// goes ahead and fails later at worst, exactly as it did before.
func checkTempSpaceForMaps(video *VideoSpecs, squeeze bool) error {
	required := remapMapBytes(video, squeeze)
	if required <= 0 {
		return nil
	}

	tempDir := os.TempDir()
	freeGB, err := getFreeDiskGB(tempDir)
	if err != nil {
		logger.Warn("Could not check free space before encoding",
			slog.String("path", tempDir),
			slog.String("error", err.Error()),
		)
		return nil
	}

	free := int64(freeGB * 1024 * 1024 * 1024)
	// A fifth of headroom: the maps are not the only thing written there while
	// ffmpeg runs, and landing exactly at the limit is its own kind of failure.
	needed := required + required/5
	if free >= needed {
		return nil
	}

	return &SessionError{Msg: fmt.Sprintf(
		"not enough space in %s: the remap maps need %d MB (%d MB with headroom) but only %d MB is free. On many systems this directory lives in RAM; set TMPDIR to a location with more room",
		tempDir, required/(1024*1024), needed/(1024*1024), free/(1024*1024))}
}

// putMapSample writes one remap coordinate into dst as the big-endian 16-bit
// sample the PGM P5 format mandates. Little-endian would produce maps that are
// silently wrong rather than rejected, so the order is not an implementation
// detail.
//
// The clamp is a guard, not a behaviour: for every input size the distortion
// math lands in [0, inputWidth-1], well inside the range. It exists so a future
// change to the offset formula degrades into a fill pixel instead of a wrapped
// coordinate pointing somewhere arbitrary in the frame.
func putMapSample(dst []byte, value int) {
	if value < 0 {
		value = 0
	} else if value > 65535 {
		value = 65535
	}
	dst[0] = byte(value >> 8)
	dst[1] = byte(value)
}

// GeneratePGM creates the remap filter maps for ffmpeg that apply the superview distortion.
// The maps are saved as binary PGM (P5) files in the current encoding session's temp directory.
// If squeeze is true, applies asymmetric scaling for 4:3 video stretched to 16:9.
// If squeeze is false, applies symmetric barrel-distortion correction.
func GeneratePGM(video *VideoSpecs, squeeze bool) (err error) {
	// Validate video before processing
	if err := video.Validate(); err != nil {
		return err
	}

	outX, outY := remapOutputSize(video, squeeze)

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

	// Generate PGM P5 files for remap filter, see https://trac.ffmpeg.org/wiki/RemapFilter
	//
	// P5 is the binary flavour of PGM: two bytes per sample instead of a decimal
	// number and a separator. FFmpeg's remap filter reads it exactly like the P2
	// ASCII form these maps used to use -- verified byte for byte on the decoded
	// output -- but a 12 Mpx GoPro frame drops from 146 MB of maps to 66 MB, and
	// generation no longer has to format 33 million integers. That matters more
	// than it looks: /tmp is a tmpfs on most current distributions, so those
	// megabytes are RAM.
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
		_ = fX.Close()
		return fmt.Errorf("failed to create temp file y.pgm: %w", err)
	}
	// Close errors matter here: a remap map truncated at flush time makes ffmpeg
	// fail much later with an opaque message. Surface it at the source instead.
	defer func() {
		if closeErr := fX.Close(); closeErr != nil && err == nil {
			err = fmt.Errorf("failed to close x.pgm: %w", closeErr)
		}
		if closeErr := fY.Close(); closeErr != nil && err == nil {
			err = fmt.Errorf("failed to close y.pgm: %w", closeErr)
		}
	}()

	// Buffer both files: the maps run to tens of megabytes and were previously
	// written one line at a time straight to the file descriptor.
	wX := bufio.NewWriterSize(fX, 1<<20)
	wY := bufio.NewWriterSize(fY, 1<<20)

	// Write PGM headers. Everything goes through the buffered writers, headers
	// included, so there is no ordering to reason about between the two.
	header := fmt.Sprintf("P5 %d %d 65535\n", outX, outY)

	if _, err := wX.WriteString(header); err != nil {
		return fmt.Errorf("failed to write header to x.pgm: %w", err)
	}
	if _, err := wY.WriteString(header); err != nil {
		return fmt.Errorf("failed to write header to y.pgm: %w", err)
	}

	rowBytes := outX * 2

	// The X map is row-invariant: sx, tx and offset are functions of x alone,
	// so every row of x.pgm is byte-for-byte identical. Build it once instead
	// of recomputing the same math outY times.
	bufX := make([]byte, rowBytes)
	for x := 0; x < outX; x++ {
		sx := float64(x) - float64(outX-video.Streams[0].Width)/2.0 // x - width diff/2
		tx := (float64(x)/float64(outX) - 0.5) * 2.0                // (x/width - 0.5) * 2

		var offset float64

		if squeeze {
			inv := 1 - math.Abs(tx)

			// The two terms are algebraically identical at the centre --
			// both reduce to 7/32 * outX * inv -- so the curve is meant to
			// pass through zero there, exactly as the non-squeeze branch does.
			//
			// They only failed to cancel because outX/16 and outX/7 were
			// integer divisions: the truncation left a residue, mirrored into
			// a jump of 0.9 to 2.6 px straight down the middle of the frame,
			// varying erratically with the resolution. A width divisible by
			// 112 (16*7) produced no seam at all, which is what pointed at the
			// truncation. Dividing in floating point is the whole fix; the
			// shape of the expression is kept recognisable against the
			// reference implementation, which spells it this way.
			offset = inv*(float64(outX)/16.0*7.0/2.0) - math.Pow((inv/16)*7, 2)*(float64(outX)/7.0*16.0/2.0)

			if tx < 0 {
				offset *= -1
			}

			putMapSample(bufX[x*2:], int(sx+offset))
		} else {
			offset = math.Pow(tx, 2) * (float64(outX-video.Streams[0].Width) / 2.0) // tx^2 * width diff/2

			if tx < 0 {
				offset *= -1
			}

			putMapSample(bufX[x*2:], int(sx-offset))
		}
	}

	bufY := make([]byte, rowBytes)
	var sample [2]byte

	for y := 0; y < outY; y++ {
		if _, err := wX.Write(bufX); err != nil {
			return fmt.Errorf("failed to write x.pgm: %w", err)
		}

		// A row of the Y map is the row index repeated outX times; encode the
		// sample once rather than once per pixel. It goes through putMapSample
		// like the X map so the byte order is defined in exactly one place --
		// inlining it here would let the two maps drift apart silently.
		putMapSample(sample[:], y)
		for i := 0; i < rowBytes; i += 2 {
			bufY[i], bufY[i+1] = sample[0], sample[1]
		}

		if _, err := wY.Write(bufY); err != nil {
			return fmt.Errorf("failed to write y.pgm: %w", err)
		}
	}

	if err := wX.Flush(); err != nil {
		return fmt.Errorf("failed to flush x.pgm: %w", err)
	}
	if err := wY.Flush(); err != nil {
		return fmt.Errorf("failed to flush y.pgm: %w", err)
	}

	logger.Info("Filter files generated successfully")

	return nil
}

// ValidateBitrate checks if the given bitrate is within acceptable constraints.
// minBitrate and maxBitrate define the valid range in bits/second.
// If either constraint is 0, that constraint is not applied.
// Returns an error describing the validation failure.
func ValidateBitrate(bitrate int, minBitrate int, maxBitrate int) error {
	if bitrate <= 0 {
		return fmt.Errorf("bitrate must be positive, got %d", bitrate)
	}
	if minBitrate > 0 && bitrate < minBitrate {
		return fmt.Errorf("bitrate %d is below minimum %d bits/second", bitrate, minBitrate)
	}
	if maxBitrate > 0 && bitrate > maxBitrate {
		return fmt.Errorf("bitrate %d exceeds maximum %d bits/second", bitrate, maxBitrate)
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
// It reads PGM filter maps from the current session and encodes using the specified encoder and quality settings.
// The callback function is called with progress percentage (0-100) for UI updates.
// Returns nil on successful completion, or an error if ffmpeg fails.
// sourcePixelFormat returns the pixel format ffprobe reported for the input, or
// "" when it is unknown. "" is read as 8-bit everywhere, which is the previous
// behaviour and the safe answer: no encoder can reject a format we did not ask
// for.
func sourcePixelFormat(video *VideoSpecs) string {
	if video == nil || len(video.Streams) == 0 {
		return ""
	}
	return video.Streams[0].PixFmt
}

// isHighBitDepth reports whether an ffprobe pixel format name carries more than
// 8 bits per sample.
//
// Two conditions, and both are needed. A sample wider than a byte has a byte
// order, so ffmpeg always suffixes those names with "le" or "be" -- no 8-bit
// format has one. And the digits just before that suffix are the depth, which
// must land in 9..16.
//
// Each condition alone gets it wrong. Looking only for a depth substring reads
// "nv12" and "yuv410p" -- both plainly 8-bit -- as high depth. Looking only for
// the endianness suffix accepts packed formats like "rgb565le", where the digits
// are a channel layout rather than a depth. Anything unrecognised is reported as
// 8-bit, which is the direction that preserves the existing behaviour.
func isHighBitDepth(pixFmt string) bool {
	name := strings.ToLower(strings.TrimSpace(pixFmt))

	trimmed := strings.TrimSuffix(name, "le")
	if trimmed == name {
		trimmed = strings.TrimSuffix(name, "be")
	}
	if trimmed == name {
		return false // no byte order, so a single-byte sample
	}

	digits := 0
	for digits < len(trimmed) && trimmed[len(trimmed)-1-digits] >= '0' && trimmed[len(trimmed)-1-digits] <= '9' {
		digits++
	}
	if digits == 0 {
		return false
	}
	depth, err := strconv.Atoi(trimmed[len(trimmed)-digits:])
	if err != nil {
		return false
	}
	return depth > 8 && depth <= 16
}

// isHEVCEncoder reports whether the encoder belongs to the H.265/HEVC family.
func isHEVCEncoder(encoder string) bool {
	name := strings.ToLower(encoder)
	return strings.Contains(name, "hevc") || strings.Contains(name, "265")
}

// remapFilterChain builds the filter graph, choosing the working and output
// pixel formats from the source depth and the target encoder.
//
// The remap filter needs a planar 4:4:4 intermediate, so a conversion is
// unavoidable; the question is only what it converts *to*. The chain used to be
// pinned to 8-bit, which silently flattened every 10-bit source -- and HERO 10
// and later GoPros, the cameras this tool exists for, record 10-bit.
//
// Ten bits are kept only for HEVC encoders. HEVC Main10 is supported by libx265
// and by every consumer hardware HEVC encoder, whereas h264_nvenc cannot encode
// 10-bit at all and libx264's High 10 profile plays back poorly. GoPro's 10-bit
// modes record HEVC, so the case that matters is covered without risking a
// failed encode on the H.264 path. Sources deeper than 10 bits are brought to
// 10, not to their native depth: nothing in this pipeline targets Main12.
func remapFilterChain(pixFmt, encoder string) string {
	intermediate, output := "yuv444p", "yuv420p"
	if isHighBitDepth(pixFmt) && isHEVCEncoder(encoder) {
		intermediate, output = "yuv444p10le", "yuv420p10le"
	}
	return fmt.Sprintf("[0:v:0][1:v:0][2:v:0]remap,format=%s,format=%s[v]", intermediate, output)
}

// x265MaxFrameThreads is the largest -threads value libx265 accepts.
//
// ffmpeg maps -threads onto x265's --frame-threads, which rejects anything
// higher with "frameNumThreads (--frame-threads) must be
// [0 .. X265_MAX_FRAME_THREADS)" and then fails to open the encoder at all.
// Since the default thread count is runtime.NumCPU(), every H.265 encode failed
// outright on a machine with more than 16 logical cores -- and libx265 is a CPU
// encoder, so the hardware-to-CPU fallback in EncodeVideo never fires for it:
// the user just got a wall of x265 errors. Verified against ffmpeg 8.0.1: 16
// works, 17 does not.
const x265MaxFrameThreads = 16

// clampEncoderThreads lowers the requested thread count to what the encoder can
// actually accept. Only libx265 has a ceiling low enough to matter; x264 clamps
// internally instead of failing.
func clampEncoderThreads(encoder string, threads int) int {
	if encoder == "libx265" && threads > x265MaxFrameThreads {
		logger.Debug("Capping encoder threads for libx265",
			slog.Int("requested", threads),
			slog.Int("capped", x265MaxFrameThreads),
		)
		return x265MaxFrameThreads
	}
	return threads
}

// encoderThreadArgs returns the -threads option for the chosen encoder, or
// nothing at all.
//
// A hardware encoder does its work on the GPU's dedicated block; the CPU thread
// count says nothing about it, and passing the host core count was only ever
// noise in the command line. Software encoders get the value, clamped to what
// they accept.
func encoderThreadArgs(encoder string, threads int) []string {
	if isHardwareEncoder(encoder) {
		return nil
	}
	return []string{"-threads", strconv.Itoa(clampEncoderThreads(encoder, threads))}
}

func buildEncodeBaseArgs(video *VideoSpecs, xPath, yPath, encoder string, bitrate int, audioCodec string, encoderThreads int, filterThreads int, videoPreset string) []string {
	baseArgs := []string{
		"-hide_banner", "-progress", "pipe:1", "-loglevel", "error", "-y",
	}
	// No "-re" here on purpose. It throttles reading the input to its native
	// frame rate, which only makes sense when streaming to a realtime sink. For
	// a file-to-file transcode it just caps the run at the clip's duration, so a
	// 10-minute video could never convert in less than 10 minutes regardless of
	// the hardware. It offered no safety, only a slowdown.
	// PerformanceMode now only drives the audio codec strategy, in EncodeVideo.

	baseArgs = append(baseArgs,
		"-i", video.File, "-i", xPath, "-i", yPath,
		"-filter_complex", remapFilterChain(sourcePixelFormat(video), encoder),
		// Explicit maps. Left to itself with three inputs, ffmpeg picks one
		// stream per type and keeps a single audio track -- a camera or an
		// edit carrying two of them silently lost one. "0:a?" is optional on
		// purpose: the "?" is what keeps a clip with no audio from failing.
		"-map", "[v]", "-map", "0:a?",
		// With three inputs ffmpeg cannot tell which one holds the global
		// metadata, so it kept none and the recording date was dropped. Point
		// it at the source explicitly.
		"-map_metadata", "0",
		"-c:v", encoder, "-b:v", strconv.Itoa(bitrate),
		// -b:v on its own is an *average* target with no ceiling, so a demanding
		// scene overshoots freely: measured at +83% on incompressible content
		// (8 Mbps requested, 14.7 Mbps produced). Constraining the VBV brings that
		// to +24% for the same request. maxrate equal to the target with a buffer
		// of twice it measured better than the usual 1.5x headroom (+24% against
		// +29%) and keeps the resulting file size predictable, which is the point:
		// the quality profile already builds its own headroom into the request.
		"-maxrate", strconv.Itoa(bitrate),
		"-bufsize", strconv.Itoa(bitrate*2),
	)

	// After -c:v so it applies to the encoder. Placed before -i it would have
	// configured the input decoder instead, which is not the intent.
	baseArgs = append(baseArgs, encoderThreadArgs(encoder, encoderThreads)...)

	baseArgs = append(baseArgs, "-c:a", audioCodec)

	if filterThreads > 0 {
		baseArgs = append(baseArgs, "-filter_threads", strconv.Itoa(filterThreads))
	}

	if mappedPreset := mapVideoPresetForEncoder(encoder, videoPreset); mappedPreset != "" {
		baseArgs = append(baseArgs, "-preset", mappedPreset)
	}

	if encoder == "libx265" {
		baseArgs = append(baseArgs, "-x265-params", "log-level=error")
	}

	return baseArgs
}

func mapVideoPresetForEncoder(encoder string, videoPreset string) string {
	preset := strings.TrimSpace(strings.ToLower(videoPreset))
	if preset == "" {
		return ""
	}

	if strings.Contains(encoder, "_amf") {
		switch preset {
		case "ultrafast", "superfast", "veryfast", "faster", "fast":
			return "speed"
		case "medium":
			return "balanced"
		case "slow", "slower", "veryslow", "placebo":
			return "quality"
		default:
			return ""
		}
	}

	if strings.Contains(encoder, "_qsv") {
		switch preset {
		case "ultrafast", "superfast":
			return "veryfast"
		case "placebo":
			return "veryslow"
		case "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow":
			return preset
		default:
			return ""
		}
	}

	return preset
}

func EncodeVideo(cfg *Config, video *VideoSpecs, encoder string, bitrate int, output string, ffmpeg map[string]string, callback func(float64), cancel <-chan struct{}) error {
	SetLastHardwareAccelerationSummary("")

	// Get the session paths for PGM files
	xPath, yPath, err := getSessionPaths()
	if err != nil {
		return err
	}

	cfg = configOrDefault(cfg)
	safePerformanceMode := cfg.IsSafePerformanceMode()
	encoderThreads := runtime.NumCPU()
	if cfg.EncoderThreads > 0 {
		encoderThreads = cfg.EncoderThreads
	}
	filterThreads := cfg.FilterThreads
	videoPreset := cfg.VideoPreset

	run := func(hwaccel string, audioCodec string) error {
		baseArgs := buildEncodeBaseArgs(video, xPath, yPath, encoder, bitrate, audioCodec, encoderThreads, filterThreads, videoPreset)
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
			return fmt.Errorf("failed to start ffmpeg: %w", err)
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
			case <-cancel:
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

		readDone := make(chan struct{})
		go func() {
			defer close(readDone)
			for {
				line, _, err := rd.ReadLine()

				if err != nil {
					// Any error terminates the reader. Looping on a non-EOF error
					// would spin forever and never close readDone, freezing the
					// encode with no diagnostic (the pipe is already broken).
					if errors.Is(err, io.EOF) {
						logger.Debug("Encoding complete")
					} else {
						logger.Warn("Progress reader stopped on error",
							slog.String("error", err.Error()),
						)
					}
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
		}()

		select {
		case <-cancel:
			if cmd.Process != nil {
				_ = cmd.Process.Kill()
			}
			<-readDone
			// Reap the killed process. Without Wait it stays a zombie for the
			// lifetime of the application, and the goroutine os/exec spawns to
			// drain stderr is never released either. A user cancelling a few
			// times would accumulate both. The error is discarded on purpose:
			// we killed it, so a non-zero status is expected.
			_ = cmd.Wait()
			return ErrCancelled
		case <-readDone:
		}

		if err := cmd.Wait(); err != nil {
			select {
			case <-interrupted:
				return ErrCancelled
			default:
			}
			if stderrBytes.Len() > 0 {
				return fmt.Errorf("ffmpeg failed: %w\nffmpeg stderr:\n%s", err, stderrBytes.String())
			}
			return fmt.Errorf("ffmpeg failed: %w", err)
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
		if errors.Is(err, ErrCancelled) {
			return err
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
	hwaccel := selectHardwareDecodeAccel(encoder, profile)
	if hwaccel != "" {
		logger.Info("Trying hardware decode+encode path",
			slog.String("encoder", encoder),
			slog.String("hwaccel", hwaccel),
		)
		err := runWithAudioFallback(hwaccel)
		if err == nil {
			SetLastHardwareAccelerationSummary(fmt.Sprintf("Hardware: used %s encode + %s decode", encoder, strings.ToUpper(hwaccel)))
			return nil
		}
		// A cancellation is not a reason to try another path: the user asked for
		// the work to stop, not for it to be attempted differently.
		if errors.Is(err, ErrCancelled) {
			return err
		}
		logger.Warn("Hardware decode path failed, falling back to CPU decode",
			slog.String("encoder", encoder),
			slog.String("hwaccel", hwaccel),
		)
	}

	logger.Info("Using CPU decode path",
		slog.String("encoder", encoder),
		slog.Int("threads", encoderThreads),
		slog.Int("filter_threads", filterThreads),
		slog.String("video_preset", videoPreset),
	)
	err = runWithAudioFallback("")
	if errors.Is(err, ErrCancelled) {
		return err
	}
	if err == nil {
		if isHardwareEncoder(encoder) {
			SetLastHardwareAccelerationSummary(fmt.Sprintf("Hardware: used %s encode + CPU decode fallback", encoder))
		} else {
			SetLastHardwareAccelerationSummary(fmt.Sprintf("Hardware: used CPU encode (%s) + CPU decode", encoder))
		}
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
			fallbackErr := EncodeVideo(cfg, video, fallbackEncoder, bitrate, output, ffmpeg, callback, cancel)
			if fallbackErr == nil {
				SetLastHardwareAccelerationSummary(fmt.Sprintf("Hardware: %s failed; used CPU encode (%s) + CPU decode", encoder, fallbackEncoder))
			}
			return fallbackErr
		}
	}

	return err
}

func CleanUp() error {
	return CloseEncodingSession()
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
// and is therefore atomic. The name comes from os.CreateTemp: two conversions
// targeting the same folder must not collide.
func newPartialOutputPath(outputFile string) (string, error) {
	file, err := os.CreateTemp(filepath.Dir(outputFile), ".superview-partial-*.mp4")
	if err != nil {
		return "", fmt.Errorf("cannot create a working file next to %s: %w", outputFile, err)
	}
	name := file.Name()
	if closeErr := file.Close(); closeErr != nil {
		_ = os.Remove(name)
		return "", fmt.Errorf("cannot create a working file next to %s: %w", outputFile, closeErr)
	}
	return name, nil
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

	profile := AnalyzeMachineProfile(ffmpeg)
	logger.Info("Machine profile analyzed",
		slog.Int("cpu_cores", profile.CPUCores),
		slog.String("hw_accels", strings.Join(profile.HardwareAccels, ",")),
		slog.String("selected_encoder", encoder),
		slog.Bool("hardware_encoder", isHardwareEncoder(encoder)),
	)

	// ==== OBSERVABILITY: Record output metadata ====
	metrics.RecordOutputMetadata(bitrate, encoder)

	// Ask the UI once and reuse the answer: the space check below and the map
	// generation further down must agree on which geometry is being produced.
	squeeze := ui.GetSqueeze()

	if err := checkTempSpaceForMaps(video, squeeze); err != nil {
		metrics.RecordError(exitCodeUnavailable, err.Error())
		RecordEncodingError(err, map[string]interface{}{"stage": "space_check"})
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
		err = fmt.Errorf("failed to move the finished file to %s: %w", outputFile, err)
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

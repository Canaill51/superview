// One ffmpeg run: encoder selection, the argument list, and the fallback cascade.

package common

import (
	"bufio"
	"bytes"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"math"
	"os"
	"os/exec"
	"os/signal"
	"runtime"
	"strconv"
	"strings"
	"sync/atomic"
	"syscall"
)

// Wrappers around os/signal for testability.
var signalNotify = signal.Notify
var signalStop = signal.Stop

// Wrapper around Cmd.StdoutPipe for testability of error paths.
var commandStdoutPipe = func(cmd *exec.Cmd) (io.ReadCloser, error) {
	return cmd.StdoutPipe()
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

		// The list above is ordered hardware first, so landing on a CPU encoder
		// means this machine has no usable hardware encoder *for the source's
		// codec*. That is not the same as having none at all, and the difference
		// was measured: an Intel HD 620 refused every hevc_* encoder and
		// accepted h264_vaapi, and because the clip was HEVC the H.264 list was
		// never consulted. The conversion ran on libx265 -- a quarter of an
		// hour, and 6 GB of memory on a machine with 8, which the kernel ended
		// by killing ffmpeg.
		//
		// Keeping the source's codec is a preference. Using the machine's
		// encoder is worth more, and what it costs is said out loud rather than
		// discovered in the output: see describeCodecFamilySwitch.
		if encoder != "" && !isHardwareEncoder(encoder) {
			if crossFamily := crossFamilyHardwareEncoder(video.Streams[0].Codec, profile); crossFamily != "" {
				logger.Info("No hardware encoder for the source codec; using the other codec family",
					slog.String("source_codec", video.Streams[0].Codec),
					slog.String("instead_of", encoder),
					slog.String("encoder", crossFamily),
					slog.String("cost", describeCodecFamilySwitch(video, crossFamily)),
				)
				encoder = crossFamily
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

// encodesInTenBits reports whether a conversion keeps its source's ten bits.
//
// One function rather than the same condition written twice: the filter chain
// decides the working depth here, and the memory estimate has to reach the same
// answer -- ten bits costs about half as much memory again, so a copy of this
// test that drifted would silently make the estimate wrong by that much.
func encodesInTenBits(pixFmt, encoder string) bool {
	return isHighBitDepth(pixFmt) && isHEVCEncoder(encoder)
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
	return "[0:v:0][1:v:0][2:v:0]remap," + pixelFormatChain(pixFmt, encoder) + "[v]"
}

// pixelFormatChain is everything after remap: the working and output pixel
// formats, then the upload for the encoders that need their frames on the
// device. The probe runs exactly this chain on a synthetic source, so a frame
// the conversion would hand the encoder is a frame the probe has already
// handed it -- depth included (E-01).
func pixelFormatChain(pixFmt, encoder string) string {
	intermediate, output := "yuv444p", "yuv420p"
	if encodesInTenBits(pixFmt, encoder) {
		intermediate, output = "yuv444p10le", "yuv420p10le"
	}
	// remap is a CPU filter, so the frames are in system memory at this point
	// whatever the encoder. The ones that cannot take them from there get the
	// upload appended.
	return fmt.Sprintf("format=%s,format=%s%s", intermediate, output, hwUploadFilters(encoder, output))
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

// requestedEncoderThreads is the thread count asked for, before any encoder
// clamps it: the configured value, else one per core.
func requestedEncoderThreads(cfg *Config) int {
	if cfg != nil && cfg.EncoderThreads > 0 {
		return cfg.EncoderThreads
	}
	return runtime.NumCPU()
}

// effectiveEncoderThreads is the -threads value an encode with this encoder
// really passes to ffmpeg. The memory guard prices this count, not the host's
// core count, so the two cannot disagree about libx265's 16-thread ceiling.
func effectiveEncoderThreads(cfg *Config, encoder string) int {
	return clampEncoderThreads(encoder, requestedEncoderThreads(cfg))
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

	// Before -i, with the other global options: this creates the device the
	// filter graph uploads to, and is nothing at all for the encoders that take
	// frames from system memory.
	baseArgs = append(baseArgs, hwDeviceArgs(encoder)...)

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
	)

	// Everything the encoder itself is asked for comes from one function the
	// probe shares, so the two cannot drift apart again (E-01).
	baseArgs = append(baseArgs, encoderFacingArgs(encoder, bitrate, videoPreset)...)

	// After -c:v so it applies to the encoder. Placed before -i it would have
	// configured the input decoder instead, which is not the intent.
	baseArgs = append(baseArgs, encoderThreadArgs(encoder, encoderThreads)...)

	baseArgs = append(baseArgs, "-c:a", audioCodec)

	if filterThreads > 0 {
		baseArgs = append(baseArgs, "-filter_threads", strconv.Itoa(filterThreads))
	}

	if encoder == "libx265" {
		baseArgs = append(baseArgs, "-x265-params", "log-level=error")
	}

	return baseArgs
}

// encoderFacingArgs is what the encoder itself is asked for: which codec, what
// rate control, which preset, which tag. One function, used by the conversion
// and by the probe, so the probe cannot pass an encoder that the conversion
// then fails on. It used to: the probe passed none of this, and an Intel driver
// that only knows the CQP rate-control mode accepted the probe and refused
// every conversion, silently, onto the CPU (E-01).
func encoderFacingArgs(encoder string, bitrate int, videoPreset string) []string {
	args := []string{"-c:v", encoder}
	args = append(args, encoderRateArgs(bitrate)...)
	if mappedPreset := mapVideoPresetForEncoder(encoder, videoPreset); mappedPreset != "" {
		args = append(args, "-preset", mappedPreset)
	}
	if isHEVCEncoder(encoder) {
		// MP4 can tag HEVC as "hev1" or "hvc1"; ffmpeg's default is hev1, which
		// QuickTime, iOS Photos and Final Cut refuse to open. hvc1 is what every
		// camera writes. Measured: without this the output reads
		// codec_tag_string=hev1 (E-09).
		args = append(args, "-tag:v", "hvc1")
	}
	return args
}

// encoderRateArgs is the rate control every encode and every probe asks for.
//
// -b:v on its own is an *average* target with no ceiling, so a demanding scene
// overshoots freely: measured at +83% on incompressible content (8 Mbps
// requested, 14.7 Mbps produced). Constraining the VBV brings that to +24% for
// the same request. maxrate equal to the target with a buffer of twice it
// measured better than the usual 1.5x headroom (+24% against +29%) and keeps
// the resulting file size predictable, which is the point: the quality profile
// already builds its own headroom into the request.
func encoderRateArgs(bitrate int) []string {
	return []string{
		"-b:v", strconv.Itoa(bitrate),
		"-maxrate", strconv.Itoa(bitrate),
		"-bufsize", strconv.Itoa(bitrate * 2),
	}
}

// mapVideoPresetForEncoder translates an x264-style preset name into what the
// target encoder accepts, or "" when the encoder takes none.
//
// The names in the configuration are x264's, and only libx264 and libx265 take
// them verbatim. NVENC's option parser rejects most of them outright -- measured
// on ffmpeg 8.0.1: "ultrafast", "superfast", "veryfast", "faster", "slower",
// "veryslow" and "placebo" all fail with "Undefined constant or missing '(' in
// 'veryslow'", exit 234, before the driver is consulted -- and the cascade then
// landed on libx264 with nothing saying why (E-08). NVENC's own scale is p1
// (fastest) to p7 (slowest); its legacy "fast" is an alias of p1, the coarsest
// of all, which is not what "Fast" promises. The encoders that take frames on
// the device have no preset to speak of: an unknown -preset is only a warning
// there, but a warning on every run is noise.
func mapVideoPresetForEncoder(encoder string, videoPreset string) string {
	preset := strings.TrimSpace(strings.ToLower(videoPreset))
	if preset == "" {
		return ""
	}

	if strings.Contains(encoder, "_nvenc") {
		switch preset {
		case "ultrafast", "superfast", "veryfast":
			return "p1"
		case "faster":
			return "p2"
		case "fast":
			return "p3"
		case "medium":
			return "p4"
		case "slow":
			return "p5"
		case "slower":
			return "p6"
		case "veryslow", "placebo":
			return "p7"
		default:
			return ""
		}
	}

	if needsDeviceFrames(encoder) || strings.Contains(encoder, "_v4l2m2m") {
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

// classifyFfmpegFailure turns a failed ffmpeg run into an error that says what
// happened, rather than one that only says that it happened.
//
// The case worth naming is a kill signal we did not send. Everything this
// program sends ffmpeg is accounted for before the call gets here -- a
// cancellation and a system signal both return their own reason -- so a process
// killed from outside is the machine intervening, and on Linux that is almost
// always the out-of-memory killer. What the user saw before this existed was
// "ffmpeg failed: signal: killed", with no mention of memory anywhere, for an
// encode that had asked for 5.2 GB on a machine holding 8.
//
// The stderr of an ordinary failure is still attached, unchanged: a kill leaves
// none, and every other failure needs it.
func classifyFfmpegFailure(err error, stderr string) error {
	sig, killed := killedBySignal(err)
	if !killed {
		if stderr != "" {
			return fmt.Errorf("ffmpeg failed: %w\nffmpeg stderr:\n%s", err, stderr)
		}
		return fmt.Errorf("ffmpeg failed: %w", err)
	}

	available := formatAvailableMemory()
	logger.Error("ffmpeg was killed by the system",
		slog.String("signal", sig),
		slog.String("memory_available", available),
	)

	memory := ""
	if available != unknownMemory {
		memory = fmt.Sprintf(" Memory available when it stopped: %s.", available)
	}

	return &EncoderError{Msg: fmt.Sprintf(
		"ffmpeg was killed by the system (signal: %s). Nothing in Superview sent that signal, "+
			"which on this platform almost always means the machine ran out of memory.%s "+
			"Close other applications, choose a hardware encoder if this machine offers one, "+
			"or convert a video with fewer pixels.",
		sig, memory)}
}

// EncodeVideo runs ffmpeg with the remap filter to apply the superview distortion.
// It reads PGM filter maps from the current session and encodes using the specified encoder and quality settings.
// The callback function is called with progress percentage (0-100) for UI updates.
// Returns nil on successful completion, or an error if ffmpeg fails.
func EncodeVideo(cfg *Config, video *VideoSpecs, encoder string, bitrate int, output string, ffmpeg map[string]string, callback func(float64), cancel <-chan struct{}) error {
	SetLastHardwareAccelerationSummary("")

	// Get the session paths for PGM files
	xPath, yPath, err := getSessionPaths()
	if err != nil {
		return err
	}

	cfg = configOrDefault(cfg)
	safePerformanceMode := cfg.IsSafePerformanceMode()
	encoderThreads := requestedEncoderThreads(cfg)
	filterThreads := cfg.FilterThreads
	videoPreset := cfg.VideoPreset

	run := func(hwaccel string, audioCodec string) error {
		baseArgs := buildEncodeBaseArgs(video, xPath, yPath, encoder, bitrate, audioCodec, encoderThreads, filterThreads, videoPreset)
		// No capacity arithmetic: CodeQL flags len(baseArgs)+4 as a possible
		// allocation-size overflow (go/allocation-size-overflow, alert #20).
		// It cannot overflow in practice, but the preallocation saved nothing
		// worth an alert on every scan.
		var args []string
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

		// Stop ffmpeg when the user cancels or the system signals us, and record
		// which of the two it was.
		//
		// The two used to share one empty-struct channel, so both came back as
		// ErrCancelled -- "encoding interrupted by user" for a stop the user had
		// nothing to do with. The channel now carries the reason.
		sigC := make(chan os.Signal, 1)
		done := make(chan struct{})
		stopped := make(chan error, 1)
		signalNotify(sigC, os.Interrupt, syscall.SIGTERM)
		defer signalStop(sigC)

		go func() {
			var reason error

			select {
			case sig := <-sigC:
				reason = fmt.Errorf("%w (signal: %v)", ErrStoppedBySignal, sig)
				// Error, not Warn: this is the line that has to survive in a log
				// the user sends with a bug report, and the memory reading beside
				// it is what says whether the machine was out of room.
				logger.Error("The system signalled the application to stop",
					slog.String("signal", sig.String()),
					slog.String("memory_available", formatAvailableMemory()),
				)
			case <-cancel:
				reason = ErrCancelled
			case <-done:
				return
			}

			if cmd.Process != nil {
				_ = cmd.Process.Kill()
			}
			select {
			case stopped <- reason:
			default:
			}
		}()
		defer close(done)

		// Set once ffmpeg has reported a frame leaving the filter graph. Read
		// after readDone, which the reader closes before anything reads it.
		var producedOutput atomic.Bool

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
					raw := bytes.TrimSpace(bytes.Replace(line, []byte("out_time_ms="), nil, 1))

					// ffmpeg answers N/A until the first frame reaches the far
					// end of the filter graph, and on a large frame that is the
					// normal state for the first seconds: one user's log carried
					// 45 of these in 81 seconds, at WARN, for a conversion that
					// was working. It is the log the README asks people to
					// attach to a bug report, so what is normal does not go in
					// it at warning level.
					if bytes.EqualFold(raw, []byte("N/A")) {
						logger.Debug("ffmpeg has not produced a frame yet")
						continue
					}

					timeF, err := strconv.ParseFloat(string(raw), 64)
					if err != nil {
						logger.Warn("Failed to parse progress value",
							slog.String("raw_value", string(raw)),
							slog.String("error", err.Error()),
						)
						continue
					}
					if timeF > 0 {
						producedOutput.Store(true)
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
			case reason := <-stopped:
				return reason
			default:
			}
			failure := classifyFfmpegFailure(err, stderrBytes.String())
			if producedOutput.Load() {
				return fmt.Errorf("%w: %w", errFailedAfterOutput, failure)
			}
			return failure
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
		if shouldNotRetry(err) {
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
			SetLastHardwareAccelerationSummary(withCodecSwitchNote(video, encoder,
				fmt.Sprintf("Hardware: used %s encode + %s decode", encoder, strings.ToUpper(hwaccel))))
			return nil
		}
		// A stop request is not a reason to try another path: the user, or the
		// system, asked for the work to stop, not for it to be attempted
		// differently. Neither is a failure that came after frames were written:
		// the next path would fail the same way, one full encode later.
		if shouldNotRetry(err) {
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
	if shouldNotRetry(err) {
		return err
	}
	if err == nil {
		if isHardwareEncoder(encoder) {
			SetLastHardwareAccelerationSummary(withCodecSwitchNote(video, encoder,
				fmt.Sprintf("Hardware: used %s encode + CPU decode fallback", encoder)))
		} else {
			SetLastHardwareAccelerationSummary(withCodecSwitchNote(video, encoder,
				fmt.Sprintf("Hardware: used CPU encode (%s) + CPU decode", encoder)))
		}
		return nil
	}

	if isHardwareEncoder(encoder) {
		fallbackEncoder := softwareEncoderForFallback(video, encoder, profile)

		if fallbackEncoder != "" {
			// Price the encoder that is about to take over, not the one that
			// was priced before the conversion started.
			//
			// PerformEncoding's guard was shown a hardware encoder and declined
			// to estimate it, which is correct -- none has been measured. But
			// the encoder arriving here is a software one, and on the machine
			// that produced this finding it is the expensive one: 6.2 GB of
			// libx265 on a laptop with 8, which is how the kernel came to kill
			// ffmpeg in the first place. Falling into that silently would have
			// made the guard a half-truth.
			if outX, outY, known := sessionOutputSize(); known {
				if memErr := checkMemoryForFrame(outX, outY, video, fallbackEncoder, effectiveEncoderThreads(cfg, fallbackEncoder)); memErr != nil {
					logger.Error("The CPU encoder that would take over does not fit in memory",
						slog.String("failed_encoder", encoder),
						slog.String("fallback_encoder", fallbackEncoder),
						slog.String("error", memErr.Error()),
					)
					return memErr
				}
			}

			logger.Warn("Hardware encoder failed, retrying with CPU encoder",
				slog.String("failed_encoder", encoder),
				slog.String("fallback_encoder", fallbackEncoder),
			)
			fallbackErr := EncodeVideo(cfg, video, fallbackEncoder, bitrate, output, ffmpeg, callback, cancel)
			if fallbackErr == nil {
				SetLastHardwareAccelerationSummary(withCodecSwitchNote(video, fallbackEncoder,
					fmt.Sprintf("Hardware: %s failed; used CPU encode (%s) + CPU decode", encoder, fallbackEncoder)))
			}
			return fallbackErr
		}
	}

	return err
}

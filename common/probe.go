package common

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"time"
)

// Runtime probing of the encoders ffmpeg claims to have.
//
// `ffmpeg -encoders` and `ffmpeg -hwaccels` answer a compile-time question:
// what the binary was built with. They say nothing about whether the driver on
// this machine will accept the work. The gap is not theoretical -- it is how a
// user of an NVIDIA RTX A1000 came to see every conversion run on the CPU:
//
//	Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0
//	The minimum required Nvidia driver for nvenc is 610.00 or newer
//
// That minimum is chosen when ffmpeg is compiled, by the nv-codec-headers it is
// built against, so two binaries both calling themselves "ffmpeg 8.1.2" can
// demand different drivers. No amount of reading ffmpeg's own lists can predict
// it. The only honest question is the one this file asks: encode one frame and
// see what happens.
//
// See docs/hardware-support.md for the measured version-to-driver table.

// probeTimeout caps a single probe. A driver that is installed but wedged can
// leave ffmpeg waiting with no output at all, and the probe runs while the user
// is looking at the window: it has to come back either way.
//
// A var, not a const, so a test can shorten it. The first version of that test
// passed a 300ms context and was satisfied -- but the caller's deadline covers
// the probe whether or not the probe has one of its own, so removing this
// timeout entirely left the test green. It guarded nothing.
var probeTimeout = 15 * time.Second

// probeFrameSize is the frame every probe encodes. Small enough to cost
// nothing, and above the minimum dimensions the hardware encoders enforce.
//
// A probe at this size cannot prove that a 4K frame will also encode: some
// encoders have an upper bound too, and a few refuse specific pixel formats.
// It proves the part that fails in practice -- that the driver answers at all.
const probeFrameSize = "256x256"

// probeReasonLines caps how much of ffmpeg's complaint is kept.
//
// The first lines, not the last. Under -loglevel error everything printed is a
// real error, but ffmpeg reports the specific cause first and the consequences
// after it. An unknown encoder prints four lines and only the first names it:
//
//	Unknown encoder 'superview_no_such_encoder'
//	Error selecting an encoder
//	Error opening output file -.
//	Error opening output files: Encoder not found
//
// Keeping the tail returned three restatements of "something went wrong" and
// dropped the one line worth reading. NVENC is the same shape: the version
// mismatch and the driver it wants come first, the plumbing errors after.
const probeReasonLines = 3

// probeReasonBudget caps the reason in characters, so a talkative failure
// cannot turn the GUI's one-line hardware status into a paragraph.
const probeReasonBudget = 300

// EncoderProbe is what one encoder answered when it was actually asked to
// encode something, rather than asked whether it exists.
type EncoderProbe struct {
	Encoder string
	Usable  bool
	Reason  string // ffmpeg's own words; empty when the probe succeeded
	Took    time.Duration
}

// EncoderProbeReport is the verdict for every encoder that was probed.
//
// An encoder absent from Results was never asked, which is not the same as
// unusable: ApplyEncoderProbe leaves those alone rather than assuming.
type EncoderProbeReport struct {
	Results []EncoderProbe
	Took    time.Duration
}

// Find returns the verdict recorded for one encoder.
func (r *EncoderProbeReport) Find(encoder string) (EncoderProbe, bool) {
	if r == nil {
		return EncoderProbe{}, false
	}
	for _, probe := range r.Results {
		if probe.Encoder == encoder {
			return probe, true
		}
	}
	return EncoderProbe{}, false
}

// Unusable returns the encoders that were probed and refused the work.
func (r *EncoderProbeReport) Unusable() []EncoderProbe {
	if r == nil {
		return nil
	}
	refused := make([]EncoderProbe, 0, len(r.Results))
	for _, probe := range r.Results {
		if !probe.Usable {
			refused = append(refused, probe)
		}
	}
	return refused
}

// probeArgs builds the argv for one probe.
//
// VAAPI is the exception that shapes this function. Its encoder only accepts
// frames that already live on the device, so the same argv that works for every
// other family fails on a pixel-format conversion that never reaches the
// driver -- condemning a working encoder on the strength of a malformed
// question. It needs a device and an explicit upload.
func probeArgs(encoder string) []string {
	onDevice := strings.Contains(encoder, "_vaapi")

	args := []string{"-hide_banner", "-loglevel", "error"}
	if onDevice {
		args = append(args, "-init_hw_device", "vaapi=probe", "-filter_hw_device", "probe")
	}
	args = append(args, "-f", "lavfi", "-i", "nullsrc=s="+probeFrameSize+":r=25:d=0.04")
	if onDevice {
		args = append(args, "-vf", "format=nv12,hwupload")
	}

	return append(args, "-c:v", encoder, "-f", "null", "-")
}

// ProbeEncoder encodes one frame with the given encoder and reports what
// happened. It never returns an error: a refusal is the answer, not a failure.
func ProbeEncoder(ctx context.Context, encoder string) EncoderProbe {
	started := time.Now()

	ctx, cancel := context.WithTimeout(ctx, probeTimeout)
	defer cancel()

	cmd := newFFmpegCommandContext(ctx, probeArgs(encoder)...)
	prepareBackgroundCommand(cmd)
	output, err := cmd.CombinedOutput()

	probe := EncoderProbe{
		Encoder: encoder,
		Usable:  err == nil,
		Took:    time.Since(started),
	}
	if err != nil {
		probe.Reason = summarizeProbeFailure(string(output), err, ctx.Err(), probe.Took)
	}
	return probe
}

// summarizeProbeFailure turns ffmpeg's output into one line a user can act on.
func summarizeProbeFailure(output string, runErr, ctxErr error, took time.Duration) string {
	// The deadline can be the probe's own or a shorter one the caller set, so
	// the message reports what was actually waited rather than the constant.
	if errors.Is(ctxErr, context.DeadlineExceeded) {
		return fmt.Sprintf("gave no answer in %s", took.Round(time.Millisecond))
	}

	lines := make([]string, 0, probeReasonLines)
	for _, line := range strings.Split(strings.ReplaceAll(output, "\r\n", "\n"), "\n") {
		if line = strings.TrimSpace(line); line != "" {
			lines = append(lines, line)
		}
	}

	if len(lines) == 0 {
		// Nothing on stderr: all we have is the exit status, which at least
		// distinguishes "refused" from "never ran".
		return runErr.Error()
	}
	if len(lines) > probeReasonLines {
		lines = lines[:probeReasonLines]
	}

	reason := strings.Join(lines, "; ")
	if len(reason) > probeReasonBudget {
		reason = reason[:probeReasonBudget] + "..."
	}
	return reason
}

// encodersToProbe picks the encoders worth asking about: every hardware one,
// plus the CPU encoders the fallback depends on.
//
// Probing the CPU pair is not ceremony. An ffmpeg built without libx264 leaves
// Superview with no fallback at all, and today that only surfaces when a
// conversion has already failed twice.
func encodersToProbe(ffmpeg map[string]string) []string {
	available := splitCSV(ffmpeg["encoders"])
	wanted := make([]string, 0, len(available))

	for _, encoder := range available {
		if isHardwareEncoder(encoder) || encoder == "libx264" || encoder == "libx265" {
			wanted = append(wanted, encoder)
		}
	}
	return wanted
}

// ProbeEncoders asks each encoder in turn to encode a frame.
//
// Sequentially, deliberately. Hardware encoders have a limited number of
// concurrent sessions, and several probes at once on the same GPU can make a
// perfectly good encoder report failure because the previous probe still holds
// a session. The whole sweep costs well under a second either way.
func ProbeEncoders(ctx context.Context, encoders []string) *EncoderProbeReport {
	started := time.Now()
	report := &EncoderProbeReport{Results: make([]EncoderProbe, 0, len(encoders))}

	for _, encoder := range encoders {
		if ctx.Err() != nil {
			break
		}
		report.Results = append(report.Results, ProbeEncoder(ctx, encoder))
	}

	report.Took = time.Since(started)
	return report
}

// ProbeHardwareSupport probes what the ffmpeg capability map advertises, and
// logs the verdict.
func ProbeHardwareSupport(ctx context.Context, ffmpeg map[string]string) *EncoderProbeReport {
	report := ProbeEncoders(ctx, encodersToProbe(ffmpeg))

	for _, probe := range report.Results {
		if probe.Usable {
			GetLogger().Debug("Encoder probe succeeded",
				slog.String("encoder", probe.Encoder),
				slog.Duration("took", probe.Took),
			)
			continue
		}
		// Warn, not Debug: this is the line that explains an encode running
		// three times slower than the user expects.
		GetLogger().Warn("Encoder is advertised but refused the work",
			slog.String("encoder", probe.Encoder),
			slog.String("reason", probe.Reason),
		)
	}

	return report
}

// ApplyEncoderProbe returns a copy of the capability map with the encoders that
// failed their probe removed.
//
// Only those. An encoder that was never probed stays in the list: the report
// says what was asked, and silence is not a refusal. The map is copied rather
// than edited so a caller holding the pre-probe capabilities keeps them.
func ApplyEncoderProbe(ffmpeg map[string]string, report *EncoderProbeReport) map[string]string {
	updated := make(map[string]string, len(ffmpeg))
	for key, value := range ffmpeg {
		updated[key] = value
	}
	if report == nil {
		return updated
	}

	refused := make(map[string]bool, len(report.Results))
	for _, probe := range report.Results {
		if !probe.Usable {
			refused[probe.Encoder] = true
		}
	}
	if len(refused) == 0 {
		return updated
	}

	kept := make([]string, 0, len(ffmpeg["encoders"]))
	for _, encoder := range splitCSV(ffmpeg["encoders"]) {
		if !refused[encoder] {
			kept = append(kept, encoder)
		}
	}
	updated["encoders"] = strings.Join(kept, ",")
	return updated
}

// DescribeEncoderProbe renders the probe verdicts for the diagnostic report.
func DescribeEncoderProbe(report *EncoderProbeReport) string {
	if report == nil {
		return "Encoders: not probed\n"
	}
	if len(report.Results) == 0 {
		return "Encoders: none to probe\n"
	}

	var b strings.Builder
	fmt.Fprintf(&b, "=== Encoders (probed in %s) ===\n", report.Took.Round(time.Millisecond))
	for _, probe := range report.Results {
		if probe.Usable {
			fmt.Fprintf(&b, "✅ %s: works (%s)\n", probe.Encoder, probe.Took.Round(time.Millisecond))
			continue
		}
		fmt.Fprintf(&b, "❌ %s: %s\n", probe.Encoder, probe.Reason)
	}
	return b.String()
}

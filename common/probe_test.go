package common

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"
)

// TestProbeArgs_DeviceFrameEncodersGetADeviceAndAnUpload pins the asymmetry in
// the probe argv, and it is not cosmetic.
//
// VAAPI, Vulkan and D3D12 only accept frames that already live on the device.
// Feeding them software frames fails in the filter graph, before the driver is
// ever consulted:
//
//	Impossible to convert between the formats supported by the filter
//	'Parsed_null_0' and the filter 'auto_scale_0'
//
// A probe shaped that way reports "unusable" for an encoder that works
// perfectly, which is worse than not probing at all: it would take hardware
// away from the machines that have it.
func TestProbeArgs_DeviceFrameEncodersGetADeviceAndAnUpload(t *testing.T) {
	for encoder, deviceType := range map[string]string{
		"h264_vaapi":   "vaapi",
		"hevc_vulkan":  "vulkan",
		"h264_d3d12va": "d3d12va",
	} {
		joined := strings.Join(probeArgs(encoder), " ")

		for _, needed := range []string{
			"-init_hw_device " + deviceType + "=" + hwDeviceAlias,
			"-filter_hw_device " + hwDeviceAlias,
			"format=nv12,hwupload",
		} {
			if !strings.Contains(joined, needed) {
				t.Errorf("the %s probe is missing %q; it would fail in the filter graph and blame the encoder\ngot: %s",
					encoder, needed, joined)
			}
		}
	}

	// NVENC, AMF and QSV upload for themselves. Asking for a device they do not
	// need would fail on machines that cannot create one.
	for _, encoder := range []string{"h264_nvenc", "hevc_amf", "h264_qsv", "libx264"} {
		joined := strings.Join(probeArgs(encoder), " ")
		for _, unwanted := range []string{"-init_hw_device", "hwupload"} {
			if strings.Contains(joined, unwanted) {
				t.Errorf("the %s probe should not carry %q: %s", encoder, unwanted, joined)
			}
		}
	}
}

// TestProbeAndConversionAskTheSameQuestion is the guard that makes the probe
// worth trusting.
//
// A probe that sets up a device the conversion does not would pass and then
// fail at encoding time -- the user watching a run fall back to the CPU after
// being told the hardware path was ready. A probe that omits a device the
// conversion does set up would condemn a working encoder. Both mistakes are
// invisible in either function read on its own, so the two are checked against
// each other here.
func TestProbeAndConversionAskTheSameQuestion(t *testing.T) {
	for _, encoder := range []string{
		"h264_nvenc", "hevc_amf", "h264_qsv",
		"h264_vaapi", "hevc_vulkan", "h264_d3d12va",
		"h264_v4l2m2m", "libx264", "libx265",
	} {
		probe := strings.Join(probeArgs(encoder), " ")

		// The conversion's device setup, verbatim from what it emits.
		conversionDevice := strings.Join(hwDeviceArgs(encoder), " ")
		if conversionDevice == "" {
			if strings.Contains(probe, "-init_hw_device") {
				t.Errorf("%s: the probe creates a device the conversion does not", encoder)
			}
		} else if !strings.Contains(probe, conversionDevice) {
			t.Errorf("%s: the conversion sets up %q, the probe does not:\n  %s", encoder, conversionDevice, probe)
		}

		// And the upload steps, which the conversion appends to its remap chain.
		chain := remapFilterChain("yuv420p", encoder)
		uploads := strings.Contains(chain, "hwupload")
		if uploads != strings.Contains(probe, "hwupload") {
			t.Errorf("%s: conversion uploads=%v but probe uploads=%v\n  chain: %s\n  probe: %s",
				encoder, uploads, !uploads, chain, probe)
		}
	}
}

// TestProbeArgs_NamesTheEncoderAndDiscardsOutput checks the probe is a probe:
// it encodes with the requested encoder and writes nothing anywhere.
func TestProbeArgs_NamesTheEncoderAndDiscardsOutput(t *testing.T) {
	args := probeArgs("hevc_nvenc")
	joined := strings.Join(args, " ")

	if !strings.Contains(joined, "-c:v hevc_nvenc") {
		t.Errorf("the probe does not select the encoder it is probing: %s", joined)
	}
	if !strings.HasSuffix(joined, "-f null -") {
		t.Errorf("a probe must discard its output, got: %s", joined)
	}
	if !strings.Contains(joined, "-loglevel error") {
		t.Errorf("the probe needs the error lines and nothing else: %s", joined)
	}
}

func TestSummarizeProbeFailure(t *testing.T) {
	longLine := strings.Repeat("x", probeReasonBudget+50)

	cases := []struct {
		name     string
		output   string
		runErr   error
		ctxErr   error
		took     time.Duration
		want     string
		contains []string
	}{
		{
			// The case the whole feature exists for. NVENC prints the mismatch
			// and then the driver it wants; both lines have to survive, because
			// together they tell the user what to do.
			name: "nvenc driver refusal keeps the actionable lines",
			output: "[h264_nvenc @ 0x55] Driver does not support the required nvenc API version. Required: 13.1 Found: 13.0\n" +
				"[h264_nvenc @ 0x55] The minimum required Nvidia driver for nvenc is 610.00 or newer\n",
			runErr: errors.New("exit status 1"),
			contains: []string{
				"Required: 13.1 Found: 13.0",
				"610.00 or newer",
			},
		},
		{
			name:   "a silent failure still says something",
			output: "   \n\n",
			runErr: errors.New("exit status 218"),
			want:   "exit status 218",
		},
		{
			name:   "a deadline reports what was actually waited",
			ctxErr: context.DeadlineExceeded,
			runErr: errors.New("signal: killed"),
			took:   302 * time.Millisecond,
			want:   "gave no answer in 302ms",
		},
		{
			// The cause first, the consequences after: ffmpeg reports in that
			// order, so keeping the tail keeps the restatements and throws
			// away the only line that identifies the problem.
			name:   "the first lines are kept, not the last",
			output: "Unknown encoder 'nope'\nError selecting an encoder\nError opening output file -.\nError opening output files: Encoder not found\n",
			runErr: errors.New("exit status 1"),
			want:   "Unknown encoder 'nope'; Error selecting an encoder; Error opening output file -.",
		},
		{
			name:     "a talkative failure is capped",
			output:   longLine,
			runErr:   errors.New("exit status 1"),
			contains: []string{"..."},
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := summarizeProbeFailure(tc.output, tc.runErr, tc.ctxErr, tc.took)

			if tc.want != "" && got != tc.want {
				t.Errorf("summarizeProbeFailure = %q, want %q", got, tc.want)
			}
			for _, needle := range tc.contains {
				if !strings.Contains(got, needle) {
					t.Errorf("summarizeProbeFailure = %q, want it to contain %q", got, needle)
				}
			}
			if len(got) > probeReasonBudget+3 {
				t.Errorf("reason is %d characters, over the %d budget: %q", len(got), probeReasonBudget, got)
			}
		})
	}
}

func TestEncodersToProbe(t *testing.T) {
	ffmpeg := map[string]string{
		"encoders": "libx264,libx264rgb,h264_nvenc,h264_qsv,h264_vaapi,h264_vulkan,libx265,hevc_nvenc",
	}

	got := strings.Join(encodersToProbe(ffmpeg), ",")
	// h264_vulkan is in there: it is a hardware encoder like the rest, and the
	// reason it exists in the candidate list is that it survives an NVENC the
	// driver refuses. Probing it is how that survival gets established.
	want := "libx264,h264_nvenc,h264_qsv,h264_vaapi,h264_vulkan,libx265,hevc_nvenc"

	if got != want {
		t.Errorf("encodersToProbe = %q, want %q", got, want)
	}
}

func TestEncodersToProbe_EmptyCapabilities(t *testing.T) {
	if got := encodersToProbe(map[string]string{}); len(got) != 0 {
		t.Errorf("nothing advertised, so nothing to probe; got %v", got)
	}
}

// TestApplyEncoderProbe_RemovesOnlyWhatWasRefused is the contract that keeps a
// partial probe from disabling working encoders.
//
// The probe covers the hardware encoders and the CPU pair. Everything else in
// the list -- h264_vulkan here -- was never asked, and silence must not be read
// as a refusal.
func TestApplyEncoderProbe_RemovesOnlyWhatWasRefused(t *testing.T) {
	ffmpeg := map[string]string{
		"version":  "8.1.2",
		"encoders": "libx264,h264_nvenc,h264_vulkan,hevc_nvenc",
	}
	report := &EncoderProbeReport{Results: []EncoderProbe{
		{Encoder: "libx264", Usable: true},
		{Encoder: "h264_nvenc", Usable: false, Reason: "driver too old"},
		{Encoder: "hevc_nvenc", Usable: false, Reason: "driver too old"},
	}}

	updated := ApplyEncoderProbe(ffmpeg, report)

	if got, want := updated["encoders"], "libx264,h264_vulkan"; got != want {
		t.Errorf("encoders = %q, want %q (an unprobed encoder must survive)", got, want)
	}
	if updated["version"] != "8.1.2" {
		t.Errorf("the rest of the capability map must come through untouched, got %v", updated)
	}
}

// TestApplyEncoderProbe_DoesNotMutateItsInput matters because the caller keeps
// the pre-probe map: the GUI holds it while the probe runs in the background.
func TestApplyEncoderProbe_DoesNotMutateItsInput(t *testing.T) {
	ffmpeg := map[string]string{"encoders": "libx264,h264_nvenc"}
	report := &EncoderProbeReport{Results: []EncoderProbe{
		{Encoder: "h264_nvenc", Usable: false, Reason: "nope"},
	}}

	_ = ApplyEncoderProbe(ffmpeg, report)

	if ffmpeg["encoders"] != "libx264,h264_nvenc" {
		t.Errorf("ApplyEncoderProbe edited the map it was given: %q", ffmpeg["encoders"])
	}
}

func TestApplyEncoderProbe_NilReportChangesNothing(t *testing.T) {
	ffmpeg := map[string]string{"encoders": "libx264,h264_nvenc"}

	if got := ApplyEncoderProbe(ffmpeg, nil)["encoders"]; got != "libx264,h264_nvenc" {
		t.Errorf("with no report there is nothing to remove, got %q", got)
	}
}

func TestEncoderProbeReport_FindAndUnusable(t *testing.T) {
	report := &EncoderProbeReport{Results: []EncoderProbe{
		{Encoder: "libx264", Usable: true},
		{Encoder: "h264_nvenc", Usable: false, Reason: "driver too old"},
	}}

	if probe, ok := report.Find("h264_nvenc"); !ok || probe.Reason != "driver too old" {
		t.Errorf("Find lost the verdict: %+v (found=%v)", probe, ok)
	}
	if _, ok := report.Find("hevc_qsv"); ok {
		t.Error("Find claims a verdict for an encoder that was never probed")
	}
	if got := report.Unusable(); len(got) != 1 || got[0].Encoder != "h264_nvenc" {
		t.Errorf("Unusable = %+v, want only h264_nvenc", got)
	}

	// A nil report is what every caller holds before the probe finishes.
	var missing *EncoderProbeReport
	if _, ok := missing.Find("libx264"); ok {
		t.Error("a nil report cannot have found anything")
	}
	if got := missing.Unusable(); len(got) != 0 {
		t.Errorf("a nil report has no refusals, got %+v", got)
	}
}

// TestDescribeEncoderProbe_CarriesTheReason guards what a bug report will
// contain. A diagnostic that says "h264_nvenc: unusable" and stops has thrown
// away the only line that identifies the cause.
func TestDescribeEncoderProbe_CarriesTheReason(t *testing.T) {
	report := &EncoderProbeReport{
		Took: 812 * time.Millisecond,
		Results: []EncoderProbe{
			{Encoder: "libx264", Usable: true, Took: 40 * time.Millisecond},
			{
				Encoder: "h264_nvenc",
				Reason:  "The minimum required Nvidia driver for nvenc is 610.00 or newer",
			},
		},
	}

	got := DescribeEncoderProbe(report)

	for _, needle := range []string{
		"libx264",
		"h264_nvenc",
		"610.00 or newer",
	} {
		if !strings.Contains(got, needle) {
			t.Errorf("the diagnostic section drops %q:\n%s", needle, got)
		}
	}
}

func TestDescribeEncoderProbe_DegradedCases(t *testing.T) {
	if got := DescribeEncoderProbe(nil); !strings.Contains(got, "not probed") {
		t.Errorf("a missing report must say so, got %q", got)
	}
	if got := DescribeEncoderProbe(&EncoderProbeReport{}); !strings.Contains(got, "none to probe") {
		t.Errorf("an empty report must say so, got %q", got)
	}
}

// TestProbeEncoder_SoftwareEncoderWorks is the probe run for real.
//
// libx264 is the right subject: it is the fallback the whole pipeline leans on,
// it needs no hardware, and if this reddens on a machine that has ffmpeg then
// the probe itself is malformed.
func TestProbeEncoder_SoftwareEncoderWorks(t *testing.T) {
	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		skipWithoutFFmpeg(t, "ffmpeg is unavailable: %v", err)
		return
	}
	if !strings.Contains(ffmpeg["encoders"], "libx264") {
		skipWithoutFFmpeg(t, "this ffmpeg has no libx264, so the CPU fallback cannot be probed")
		return
	}

	probe := ProbeEncoder(context.Background(), "libx264")

	if !probe.Usable {
		t.Fatalf("libx264 refused a 256x256 frame, so the probe is asking a malformed question: %s", probe.Reason)
	}
	if probe.Took <= 0 {
		t.Error("the probe reported no elapsed time")
	}
}

// TestProbeEncoder_UnknownEncoderIsRefusedWithFFmpegsWords checks the failure
// path against a real ffmpeg, and that the reason survives to the caller.
func TestProbeEncoder_UnknownEncoderIsRefusedWithFFmpegsWords(t *testing.T) {
	if _, err := CheckFfmpeg(nil); err != nil {
		skipWithoutFFmpeg(t, "ffmpeg is unavailable: %v", err)
		return
	}

	probe := ProbeEncoder(context.Background(), "superview_no_such_encoder")

	if probe.Usable {
		t.Fatal("ffmpeg accepted an encoder that does not exist")
	}
	if probe.Reason == "" {
		t.Fatal("the probe refused the encoder without saying why, which is the whole point of it")
	}
	if !strings.Contains(probe.Reason, "superview_no_such_encoder") {
		t.Errorf("the reason does not name the encoder ffmpeg rejected: %q", probe.Reason)
	}
}

// TestProbeEncoder_BoundsItselfWithoutACallerDeadline is the one that guards
// probeTimeout.
//
// The GUI passes context.Background(): there is no outer deadline to fall back
// on, so if the probe carries none of its own, a wedged driver holds the
// goroutine -- and the hardware line with it -- for the life of the session.
func TestProbeEncoder_BoundsItselfWithoutACallerDeadline(t *testing.T) {
	fakeHangingFFmpeg(t, t.TempDir())

	previous := probeTimeout
	probeTimeout = 300 * time.Millisecond
	t.Cleanup(func() { probeTimeout = previous })

	started := time.Now()
	probe := ProbeEncoder(context.Background(), "libx264")
	elapsed := time.Since(started)

	if probe.Usable {
		t.Fatal("an ffmpeg that never exits cannot have succeeded")
	}
	if elapsed > 5*time.Second {
		t.Fatalf("the probe ran for %s against an unbounded context: it has no timeout of its own", elapsed)
	}
}

// TestProbeEncoder_HonoursTheCallerDeadline covers the other direction: a
// caller that wants to give up sooner than probeTimeout must be obeyed.
func TestProbeEncoder_HonoursTheCallerDeadline(t *testing.T) {
	fakeHangingFFmpeg(t, t.TempDir())

	ctx, cancel := context.WithTimeout(context.Background(), 300*time.Millisecond)
	defer cancel()

	started := time.Now()
	probe := ProbeEncoder(ctx, "libx264")
	elapsed := time.Since(started)

	if probe.Usable {
		t.Fatal("an ffmpeg that never exits cannot have succeeded")
	}
	if elapsed > 5*time.Second {
		t.Fatalf("the probe took %s to give up: the deadline is not being applied", elapsed)
	}
	if !strings.Contains(probe.Reason, "gave no answer") {
		t.Errorf("a timeout must be reported as one, got %q", probe.Reason)
	}
}

// TestProbeEncoders_StopsOnCancellation keeps a cancelled probe sweep from
// running every remaining encoder to completion.
func TestProbeEncoders_StopsOnCancellation(t *testing.T) {
	ctx, cancel := context.WithCancel(context.Background())
	cancel()

	report := ProbeEncoders(ctx, []string{"libx264", "libx265", "h264_nvenc"})

	if len(report.Results) != 0 {
		t.Errorf("a cancelled sweep probed %d encoders anyway: %+v", len(report.Results), report.Results)
	}
}

// TestProbeHardwareSupport_AgreesWithWhatWasAdvertised runs the sweep the GUI
// runs at startup, against the real ffmpeg on this machine.
func TestProbeHardwareSupport_AgreesWithWhatWasAdvertised(t *testing.T) {
	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		skipWithoutFFmpeg(t, "ffmpeg is unavailable: %v", err)
		return
	}

	report := ProbeHardwareSupport(context.Background(), ffmpeg)

	wanted := encodersToProbe(ffmpeg)
	if len(report.Results) != len(wanted) {
		t.Fatalf("probed %d encoders, expected %d (%v)", len(report.Results), len(wanted), wanted)
	}

	// Every advertised encoder that fails is a real finding on this machine, so
	// log rather than fail: a CI runner without a GPU is expected to refuse
	// most of them. What must hold is that the verdict carries a reason.
	for _, probe := range report.Results {
		if probe.Usable {
			t.Logf("  %-14s works in %s", probe.Encoder, probe.Took.Round(time.Millisecond))
			continue
		}
		if probe.Reason == "" {
			t.Errorf("%s was refused with no reason recorded", probe.Encoder)
		}
		t.Logf("  %-14s refused: %s", probe.Encoder, probe.Reason)
	}

	// And the filtered map must never advertise what was just refused.
	filtered := splitCSV(ApplyEncoderProbe(ffmpeg, report)["encoders"])
	for _, probe := range report.Unusable() {
		for _, kept := range filtered {
			if kept == probe.Encoder {
				t.Errorf("%s refused the work but is still advertised", probe.Encoder)
			}
		}
	}
}

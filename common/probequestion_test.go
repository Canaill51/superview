package common

import (
	"context"
	"os"
	"path/filepath"
	"strings"
	"testing"
)

// TestProbeArgs_AskForRateControl pins the half of E-01 that was measured: a
// probe without a bitrate let an Intel driver that only knows CQP pass, and
// every conversion -- which asks for one -- then fell back to the CPU.
func TestProbeArgs_AskForRateControl(t *testing.T) {
	joined := strings.Join(probeArgs(sweepSpec("h264_vaapi")), " ")
	for _, needed := range []string{"-b:v ", "-maxrate ", "-bufsize "} {
		if !strings.Contains(joined, needed) {
			t.Errorf("the probe does not ask for %q, so it cannot catch a driver that refuses rate control: %s", needed, joined)
		}
	}
}

// TestProbeArgs_AskAtTheSourceDepth: a GPU that encodes 8-bit HEVC but not
// Main10 must be refused by the probe, not by the conversion.
func TestProbeArgs_AskAtTheSourceDepth(t *testing.T) {
	ten := strings.Join(probeArgs(probeSpec{Encoder: "hevc_vaapi", Size: "1920x1080", PixFmt: "yuv420p10le", Bitrate: probeBitrate}), " ")
	if !strings.Contains(ten, "format=yuv444p10le,format=yuv420p10le,format=p010,hwupload") {
		t.Errorf("a 10-bit source must be probed in 10 bits: %s", ten)
	}
	eight := strings.Join(probeArgs(probeSpec{Encoder: "hevc_vaapi", Size: "1920x1080", PixFmt: "yuv420p", Bitrate: probeBitrate}), " ")
	if strings.Contains(eight, "10le") {
		t.Errorf("an 8-bit source must not be probed in 10 bits: %s", eight)
	}
}

// TestMapVideoPreset_NVENCTakesItsOwnScale pins E-08: x264 names handed to
// NVENC fail its option parser (exit 234) and the cascade then landed on the
// CPU; and legacy "fast" is p1, the coarsest, which is not what "Fast" means.
func TestMapVideoPreset_NVENCTakesItsOwnScale(t *testing.T) {
	cases := map[string]string{
		"ultrafast": "p1", "superfast": "p1", "veryfast": "p1",
		"faster": "p2", "fast": "p3", "medium": "p4", "slow": "p5",
		"slower": "p6", "veryslow": "p7", "placebo": "p7",
		"": "", "bogus": "",
	}
	for in, want := range cases {
		for _, encoder := range []string{"h264_nvenc", "hevc_nvenc"} {
			if got := mapVideoPresetForEncoder(encoder, in); got != want {
				t.Errorf("mapVideoPresetForEncoder(%s, %q) = %q, want %q", encoder, in, got, want)
			}
		}
	}
}

// TestMapVideoPreset_DeviceFrameEncodersTakeNone: VAAPI, Vulkan, D3D12 and
// v4l2m2m have no x264-style preset; an unknown -preset is a warning on every
// run there, and a warning on every run is noise.
func TestMapVideoPreset_DeviceFrameEncodersTakeNone(t *testing.T) {
	for _, encoder := range []string{"h264_vaapi", "hevc_vulkan", "h264_d3d12va", "h264_v4l2m2m"} {
		if got := mapVideoPresetForEncoder(encoder, "medium"); got != "" {
			t.Errorf("%s: got preset %q, want none", encoder, got)
		}
	}
	// The CPU encoders still take the name verbatim.
	if got := mapVideoPresetForEncoder("libx264", "veryslow"); got != "veryslow" {
		t.Errorf("libx264 must keep the x264 name, got %q", got)
	}
}

// TestEncoderFacingArgs_TagsHEVCForApplePlayers pins E-09.
func TestEncoderFacingArgs_TagsHEVCForApplePlayers(t *testing.T) {
	for _, encoder := range []string{"libx265", "hevc_nvenc", "hevc_vaapi"} {
		joined := strings.Join(encoderFacingArgs(encoder, 1_000_000, ""), " ")
		if !strings.Contains(joined, "-tag:v hvc1") {
			t.Errorf("%s output must be tagged hvc1: %s", encoder, joined)
		}
	}
	for _, encoder := range []string{"libx264", "h264_nvenc"} {
		joined := strings.Join(encoderFacingArgs(encoder, 1_000_000, ""), " ")
		if strings.Contains(joined, "-tag:v") {
			t.Errorf("%s must not carry an HEVC tag: %s", encoder, joined)
		}
	}
}

// TestDecodeAccelCandidates_AreRealHwaccels pins E-07: "qsv" keeps the decoded
// frames on the device, which the CPU remap filter cannot take, and "drm" is a
// device type that -hwaccel cannot create. Both failed hard on every run.
func TestDecodeAccelCandidates_AreRealHwaccels(t *testing.T) {
	for _, encoder := range []string{
		"h264_nvenc", "hevc_amf", "h264_qsv", "hevc_qsv", "h264_vaapi",
		"hevc_vulkan", "h264_d3d12va", "h264_v4l2m2m", "libx264",
	} {
		for _, token := range decodeAccelCandidatesForEncoder(encoder) {
			if token == "qsv" || token == "drm" {
				t.Errorf("%s: %q is not a usable -hwaccel for a CPU filter graph", encoder, token)
			}
		}
	}
	if got := accelForEncoder("h264_v4l2m2m"); got != "" {
		t.Errorf("v4l2m2m must not require an accel, got %q", got)
	}
	if !canUseEncoderWithProfile("h264_v4l2m2m", &MachineProfile{AvailableEncoders: []string{"h264_v4l2m2m"}}) {
		t.Error("v4l2m2m must stay usable without any hwaccel")
	}
}

// TestSizeProbeCache_DoesNotRememberACancelledProbe pins the cache half of
// E-23: a probe cut short by its caller says nothing about the driver.
func TestSizeProbeCache_DoesNotRememberACancelledProbe(t *testing.T) {
	freshSizeProbeCache(t)
	dir := t.TempDir()
	fakeRefusingFFmpeg(t, dir, filepath.Join(dir, "calls.txt"))

	ctx, cancel := context.WithCancel(context.Background())
	cancel()
	spec := probeSpec{Encoder: "h264_vaapi", Size: "640x480", Bitrate: probeBitrate}
	probe := probeEncoderForSizeCached(ctx, spec)
	if probe.Usable {
		t.Fatal("a cancelled probe cannot report the encoder usable")
	}

	sizeProbeMu.Lock()
	_, cached := sizeProbeCache[spec.cacheKey()]
	sizeProbeMu.Unlock()
	if cached {
		t.Fatal("a cancelled probe was cached as the driver's answer")
	}
}

// TestIntegration_HEVCOutputIsTaggedHvc1 reads the tag back from a real
// encode, which is the only place E-09 can be seen.
func TestIntegration_HEVCOutputIsTaggedHvc1(t *testing.T) {
	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		skipWithoutFFmpeg(t, "CheckFfmpeg failed: %v", err)
	}
	if !strings.Contains(ffmpeg["encoders"], "libx265") {
		skipWithoutFFmpeg(t, "this ffmpeg has no libx265")
	}

	input := makeTestClip(t, 320, 240, 1)
	output := filepath.Join(t.TempDir(), "out.mp4")
	ui := &integrationUI{bitrate: 1_000_000, encoder: "libx265"}
	if err := PerformEncoding(nil, input, output, ui, ffmpeg, make(chan struct{})); err != nil {
		t.Fatalf("PerformEncoding: %v", err)
	}

	cmd := newFFprobeCommand("-v", "error", "-select_streams", "v:0",
		"-show_entries", "stream=codec_tag_string", "-of", "csv=p=0", output)
	out, err := cmd.Output()
	if err != nil {
		t.Fatalf("ffprobe: %v", err)
	}
	if tag := strings.TrimSpace(string(out)); tag != "hvc1" {
		t.Fatalf("codec_tag_string = %q, want hvc1 (Apple players refuse hev1)", tag)
	}
	_ = os.Remove(output)
}

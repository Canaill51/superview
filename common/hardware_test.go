package common

import (
	"strings"
	"testing"
)

func testVideoSpecs(codec string) *VideoSpecs {
	return &VideoSpecs{
		File: "test.mp4",
		Streams: []VideoStream{{
			Codec:         codec,
			Width:         1920,
			Height:        1080,
			BitrateInt:    5000000,
			DurationFloat: 60,
		}},
	}
}

func TestAnalyzeMachineProfile(t *testing.T) {
	ffmpeg := map[string]string{
		"accels":   "cuda,vaapi",
		"encoders": "h264_nvenc,libx264",
	}
	profile := AnalyzeMachineProfile(ffmpeg)
	if profile.CPUCores <= 0 {
		t.Fatalf("expected CPU cores > 0, got %d", profile.CPUCores)
	}
	if len(profile.HardwareAccels) != 2 {
		t.Fatalf("unexpected hwaccels: %+v", profile.HardwareAccels)
	}
	if len(profile.AvailableEncoders) != 2 {
		t.Fatalf("unexpected encoders: %+v", profile.AvailableEncoders)
	}
}

func TestCanUseEncoderWithProfile(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels:    []string{"cuda"},
		AvailableEncoders: []string{"h264_nvenc", "libx264"},
	}
	if !canUseEncoderWithProfile("h264_nvenc", profile) {
		t.Fatal("expected h264_nvenc to be usable")
	}
	if !canUseEncoderWithProfile("libx264", profile) {
		t.Fatal("expected libx264 to be usable")
	}
	if canUseEncoderWithProfile("hevc_qsv", profile) {
		t.Fatal("expected hevc_qsv to be unusable")
	}
}

func TestCanUseEncoderWithProfile_AllowsNVENCWithoutCUDAAccel(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels:    []string{"d3d11va", "dxva2"},
		AvailableEncoders: []string{"h264_nvenc", "libx264"},
	}

	if !canUseEncoderWithProfile("h264_nvenc", profile) {
		t.Fatal("expected h264_nvenc to remain usable when CUDA hwaccel is unavailable")
	}
}

func TestCanUseEncoderWithProfile_AllowsAMFWithoutDedicatedAccel(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels:    []string{"d3d11va", "dxva2"},
		AvailableEncoders: []string{"h264_amf", "libx264"},
	}

	if !canUseEncoderWithProfile("h264_amf", profile) {
		t.Fatal("expected h264_amf to remain usable when AMF decode hwaccel is unavailable")
	}
}

func TestCanUseEncoderWithProfile_AllowsQSVWithoutDedicatedAccel(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels:    []string{"d3d11va", "dxva2"},
		AvailableEncoders: []string{"h264_qsv", "libx264"},
	}

	if !canUseEncoderWithProfile("h264_qsv", profile) {
		t.Fatal("expected h264_qsv to remain usable when qsv hwaccel is unavailable")
	}
}

func TestSelectHardwareDecodeAccel_NVENCPrefersWindowsAccelFallbacks(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels: []string{"d3d11va", "dxva2"},
	}

	accel := selectHardwareDecodeAccel("h264_nvenc", profile)
	if accel != "d3d11va" {
		t.Fatalf("expected d3d11va, got %q", accel)
	}
}

func TestSelectHardwareDecodeAccel_AMFPrefersWindowsAccelFallbacks(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels: []string{"d3d11va", "dxva2"},
	}

	accel := selectHardwareDecodeAccel("h264_amf", profile)
	if accel != "d3d11va" {
		t.Fatalf("expected d3d11va, got %q", accel)
	}
}

func TestSelectHardwareDecodeAccel_QSVFallsBackToWindowsAccels(t *testing.T) {
	profile := &MachineProfile{
		HardwareAccels: []string{"d3d11va", "dxva2"},
	}

	accel := selectHardwareDecodeAccel("h264_qsv", profile)
	if accel != "d3d11va" {
		t.Fatalf("expected d3d11va, got %q", accel)
	}
}

func TestDescribeHardwareAccelerationPlan_HardwareFallback(t *testing.T) {
	ffmpeg := map[string]string{
		"accels":   "d3d11va,dxva2",
		"encoders": "h264_nvenc,libx264",
	}

	summary := DescribeHardwareAccelerationPlan(ffmpeg, testVideoSpecs("h264"), "")
	if summary != "Hardware: planned h264_nvenc encode + D3D11VA decode" {
		t.Fatalf("unexpected summary: %q", summary)
	}
}

func TestDescribeHardwareAccelerationPlan_CPUFallback(t *testing.T) {
	ffmpeg := map[string]string{
		"accels":   "",
		"encoders": "libx264,libx265",
	}

	summary := DescribeHardwareAccelerationPlan(ffmpeg, testVideoSpecs("h264"), "")
	if summary != "Hardware: planned CPU encode (libx264) + CPU decode" {
		t.Fatalf("unexpected summary: %q", summary)
	}
}

// TestIsHardwareEncoder_CoversTheVendorNeutralPair keeps the two newest paths
// classified as hardware.
//
// The classification is load-bearing in four places: whether the encoder gets
// probed, whether the plan says "hardware encode", whether -threads is passed,
// and whether a failure falls back to the CPU. Adding an encoder to the
// candidate list without adding it here would leave it selected, unprobed and
// handed a host thread count.
func TestIsHardwareEncoder_CoversTheVendorNeutralPair(t *testing.T) {
	for _, encoder := range []string{
		"h264_nvenc", "hevc_amf", "h264_qsv", "hevc_vaapi",
		"h264_vulkan", "hevc_vulkan", "h264_d3d12va", "hevc_d3d12va",
		"h264_v4l2m2m",
	} {
		if !isHardwareEncoder(encoder) {
			t.Errorf("isHardwareEncoder(%q) = false, want true", encoder)
		}
	}
	for _, encoder := range []string{"libx264", "libx264rgb", "libx265", ""} {
		if isHardwareEncoder(encoder) {
			t.Errorf("isHardwareEncoder(%q) = true, want false", encoder)
		}
	}
}

// TestDeviceTypeForEncoder separates the encoders that upload for themselves
// from the ones that must be handed frames already on the device.
func TestDeviceTypeForEncoder(t *testing.T) {
	cases := map[string]string{
		"h264_vaapi":   "vaapi",
		"hevc_vaapi":   "vaapi",
		"h264_vulkan":  "vulkan",
		"hevc_d3d12va": "d3d12va",
		// These three create their own context from software frames.
		"h264_nvenc": "",
		"hevc_amf":   "",
		"h264_qsv":   "",
		"libx264":    "",
	}

	for encoder, want := range cases {
		if got := deviceTypeForEncoder(encoder); got != want {
			t.Errorf("deviceTypeForEncoder(%q) = %q, want %q", encoder, got, want)
		}
		if got, want := needsDeviceFrames(encoder), want != ""; got != want {
			t.Errorf("needsDeviceFrames(%q) = %v, want %v", encoder, got, want)
		}
	}
}

// TestCandidateEncoders_VendorFirstThenTheNeutralPair pins the order.
//
// The vendor encoders expose the rate control and presets this pipeline sets,
// so they come first. D3D12 and Vulkan sit between them and v4l2m2m because
// they are driven by the display driver rather than by NVENC's own API: they
// are what remains when an FFmpeg build demands a newer NVIDIA driver than the
// machine can install. The CPU encoders stay last.
func TestCandidateEncoders_VendorFirstThenTheNeutralPair(t *testing.T) {
	cases := map[string][]string{
		"h264": {"h264_nvenc", "h264_amf", "h264_qsv", "h264_vaapi", "h264_d3d12va", "h264_vulkan", "h264_v4l2m2m", "libx264", "libx264rgb"},
		"hevc": {"hevc_nvenc", "hevc_amf", "hevc_qsv", "hevc_vaapi", "hevc_d3d12va", "hevc_vulkan", "hevc_v4l2m2m", "libx265"},
	}

	for codec, want := range cases {
		got := candidateEncodersForCodec(codec)
		if strings.Join(got, ",") != strings.Join(want, ",") {
			t.Errorf("candidateEncodersForCodec(%q) =\n  %v\nwant\n  %v", codec, got, want)
		}
		// Whatever the order, no hardware encoder may follow a CPU one: that
		// would make the CPU the default on a machine that has hardware.
		seenCPU := false
		for _, encoder := range got {
			if !isHardwareEncoder(encoder) {
				seenCPU = true
				continue
			}
			if seenCPU {
				t.Errorf("%s: hardware encoder %q comes after a CPU encoder", codec, encoder)
			}
		}
	}
}

// TestDecodeAccelCandidates_D3D12FallsBackThroughTheOlderWindowsPaths keeps a
// D3D12 encode usable on a machine whose ffmpeg exposes only the older decode
// accelerators.
func TestDecodeAccelCandidates_D3D12FallsBackThroughTheOlderWindowsPaths(t *testing.T) {
	got := decodeAccelCandidatesForEncoder("h264_d3d12va")
	want := []string{"d3d12va", "d3d11va", "dxva2"}

	if strings.Join(got, ",") != strings.Join(want, ",") {
		t.Errorf("decodeAccelCandidatesForEncoder(h264_d3d12va) = %v, want %v", got, want)
	}

	// Vulkan has no such family: it decodes through Vulkan or not at all.
	if got := decodeAccelCandidatesForEncoder("h264_vulkan"); strings.Join(got, ",") != "vulkan" {
		t.Errorf("decodeAccelCandidatesForEncoder(h264_vulkan) = %v, want [vulkan]", got)
	}
}

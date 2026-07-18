package common

import "testing"

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

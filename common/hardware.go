package common

import (
	"fmt"
	"runtime"
	"strings"
)

// MachineProfile describes runtime hardware/software capabilities detected from ffmpeg and host CPU.
type MachineProfile struct {
	CPUCores          int
	HardwareAccels    []string
	AvailableEncoders []string
}

// HardwarePlan describes the encoder and decode path Superview expects to use.
type HardwarePlan struct {
	Encoder        string
	DecodeAccel    string
	HardwareEncode bool
}

// AnalyzeMachineProfile analyzes host and ffmpeg capabilities for encoder selection.
func AnalyzeMachineProfile(ffmpeg map[string]string) *MachineProfile {
	profile := &MachineProfile{
		CPUCores: runtime.NumCPU(),
	}

	if ffmpeg == nil {
		return profile
	}

	profile.HardwareAccels = splitCSV(ffmpeg["accels"])
	profile.AvailableEncoders = splitCSV(ffmpeg["encoders"])
	return profile
}

func splitCSV(value string) []string {
	if strings.TrimSpace(value) == "" {
		return []string{}
	}
	parts := strings.Split(value, ",")
	out := make([]string, 0, len(parts))
	for _, p := range parts {
		p = strings.TrimSpace(p)
		if p != "" {
			out = append(out, p)
		}
	}
	return out
}

func toSet(items []string) map[string]bool {
	set := make(map[string]bool, len(items))
	for _, item := range items {
		set[item] = true
	}
	return set
}

func isHardwareEncoder(encoder string) bool {
	for _, suffix := range []string{"_nvenc", "_amf", "_qsv", "_vaapi", "_vulkan", "_d3d12va", "_v4l2m2m"} {
		if strings.Contains(encoder, suffix) {
			return true
		}
	}
	return false
}

// hwDeviceAlias names the hardware device the filter graph uploads to. Any
// name would do; it only has to be the same in -init_hw_device and
// -filter_hw_device.
const hwDeviceAlias = "sv"

// deviceTypeForEncoder returns the ffmpeg hardware device an encoder needs its
// frames to live on, or "" for the encoders that take frames from system
// memory.
//
// NVENC, AMF and QSV upload for themselves; VAAPI, Vulkan and D3D12 do not, and
// handing them software frames fails in the filter graph -- before the driver
// is ever consulted -- with a message about pixel formats that names neither
// the encoder nor the device.
func deviceTypeForEncoder(encoder string) string {
	switch {
	case strings.Contains(encoder, "_vaapi"):
		return "vaapi"
	case strings.Contains(encoder, "_vulkan"):
		return "vulkan"
	case strings.Contains(encoder, "_d3d12va"):
		return "d3d12va"
	default:
		return ""
	}
}

// needsDeviceFrames reports whether an encoder has to be fed uploaded frames.
func needsDeviceFrames(encoder string) bool {
	return deviceTypeForEncoder(encoder) != ""
}

// hwDeviceArgs returns the global options that create the device and point the
// filter graph at it. They belong before -i, with the other global options.
func hwDeviceArgs(encoder string) []string {
	deviceType := deviceTypeForEncoder(encoder)
	if deviceType == "" {
		return nil
	}
	return []string{
		"-init_hw_device", deviceType + "=" + hwDeviceAlias,
		"-filter_hw_device", hwDeviceAlias,
	}
}

// hwUploadFilters returns the filter steps that move frames onto the device,
// for an output pixel format the filter chain has already settled on.
//
// The upload format is not the chain's output format: hardware frame pools are
// semi-planar, so 8-bit goes up as nv12 and 10-bit as p010.
func hwUploadFilters(encoder, outputFormat string) string {
	if !needsDeviceFrames(encoder) {
		return ""
	}
	uploadFormat := "nv12"
	if strings.HasSuffix(outputFormat, "10le") {
		uploadFormat = "p010"
	}
	return ",format=" + uploadFormat + ",hwupload"
}

func accelForEncoder(encoder string) string {
	switch {
	case strings.Contains(encoder, "_nvenc"):
		return "cuda"
	case strings.Contains(encoder, "_qsv"):
		return "qsv"
	case strings.Contains(encoder, "_vaapi"):
		return "vaapi"
	case strings.Contains(encoder, "_vulkan"):
		return "vulkan"
	case strings.Contains(encoder, "_d3d12va"):
		return "d3d12va"
	case strings.Contains(encoder, "_v4l2m2m"):
		return "drm"
	default:
		return ""
	}
}

func decodeAccelCandidatesForEncoder(encoder string) []string {
	switch {
	case strings.Contains(encoder, "_nvenc"):
		return []string{"cuda", "d3d11va", "dxva2"}
	case strings.Contains(encoder, "_amf"):
		return []string{"d3d11va", "dxva2"}
	case strings.Contains(encoder, "_qsv"):
		return []string{"qsv", "d3d11va", "dxva2"}
	case strings.Contains(encoder, "_d3d12va"):
		return []string{"d3d12va", "d3d11va", "dxva2"}
	default:
		requiredAccel := accelForEncoder(encoder)
		if requiredAccel == "" {
			return []string{}
		}
		return []string{requiredAccel}
	}
}

func canUseDedicatedHardwareEncoderWithoutMatchingAccel(encoder string) bool {
	return strings.Contains(encoder, "_nvenc") || strings.Contains(encoder, "_amf") || strings.Contains(encoder, "_qsv")
}

func selectHardwareDecodeAccel(encoder string, profile *MachineProfile) string {
	if profile == nil {
		return ""
	}

	accelSet := toSet(profile.HardwareAccels)
	for _, accel := range decodeAccelCandidatesForEncoder(encoder) {
		if accelSet[accel] {
			return accel
		}
	}

	return ""
}

func describePlannedHardwarePath(plan HardwarePlan) string {
	if plan.Encoder == "" {
		return "Hardware: no compatible encoder selected"
	}

	if !plan.HardwareEncode {
		return fmt.Sprintf("Hardware: planned CPU encode (%s) + CPU decode", plan.Encoder)
	}

	if plan.DecodeAccel != "" {
		return fmt.Sprintf("Hardware: planned %s encode + %s decode", plan.Encoder, strings.ToUpper(plan.DecodeAccel))
	}

	return fmt.Sprintf("Hardware: planned %s encode + CPU decode fallback", plan.Encoder)
}

// BuildHardwarePlan determines the most likely encoder and decode path for the current input.
func BuildHardwarePlan(ffmpeg map[string]string, video *VideoSpecs, requestedEncoder string) (HardwarePlan, error) {
	if video == nil || len(video.Streams) == 0 {
		return HardwarePlan{}, &InvalidVideoError{Reason: "no video streams"}
	}

	encoder, err := FindEncoder(requestedEncoder, ffmpeg, video)
	if err != nil {
		return HardwarePlan{}, err
	}

	profile := AnalyzeMachineProfile(ffmpeg)
	return HardwarePlan{
		Encoder:        encoder,
		DecodeAccel:    selectHardwareDecodeAccel(encoder, profile),
		HardwareEncode: isHardwareEncoder(encoder),
	}, nil
}

// DescribeHardwareAccelerationPlan returns a user-facing summary of the planned hardware path.
func DescribeHardwareAccelerationPlan(ffmpeg map[string]string, video *VideoSpecs, requestedEncoder string) string {
	if video == nil {
		return "Hardware: waiting for input video"
	}

	plan, err := BuildHardwarePlan(ffmpeg, video, requestedEncoder)
	if err != nil {
		return fmt.Sprintf("Hardware: %s", err.Error())
	}

	return describePlannedHardwarePath(plan)
}

func candidateEncodersForCodec(codec string) []string {
	switch strings.ToLower(codec) {
	// The vendor encoders first: they expose the rate control and presets this
	// pipeline actually sets. Then the vendor-neutral pair, which is the point
	// of having them -- D3D12 and Vulkan video encode are driven by the display
	// driver rather than by NVENC's own API, so they are still there when an
	// FFmpeg build demands a newer NVIDIA driver than the machine can install
	// (ANALYSE.md U-03). v4l2m2m last, then the CPU.
	case "h264", "avc":
		return []string{"h264_nvenc", "h264_amf", "h264_qsv", "h264_vaapi", "h264_d3d12va", "h264_vulkan", "h264_v4l2m2m", "libx264", "libx264rgb"}
	case "h265", "hevc":
		return []string{"hevc_nvenc", "hevc_amf", "hevc_qsv", "hevc_vaapi", "hevc_d3d12va", "hevc_vulkan", "hevc_v4l2m2m", "libx265"}
	default:
		return []string{"libx264", "libx265"}
	}
}

func canUseEncoderWithProfile(encoder string, profile *MachineProfile) bool {
	if profile == nil {
		return false
	}

	encSet := toSet(profile.AvailableEncoders)
	if !encSet[encoder] {
		return false
	}

	if !isHardwareEncoder(encoder) {
		return true
	}

	if canUseDedicatedHardwareEncoderWithoutMatchingAccel(encoder) {
		return true
	}

	requiredAccel := accelForEncoder(encoder)
	if requiredAccel == "" {
		return true
	}
	accelSet := toSet(profile.HardwareAccels)
	return accelSet[requiredAccel]
}

package common

import (
	"context"
	"fmt"
	"log/slog"
	"runtime"
	"strings"
	"sync"
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
	// CodecSwitch is the cost of encoding into a family the source is not in,
	// empty when the plan stays in the source's own family. It is part of the
	// plan rather than derived by the caller because the plan is what the window
	// shows before anything runs, and this is the half of it a user can act on.
	CodecSwitch string
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

	line := ""
	switch {
	case !plan.HardwareEncode:
		line = fmt.Sprintf("Hardware: planned CPU encode (%s) + CPU decode", plan.Encoder)
	case plan.DecodeAccel != "":
		line = fmt.Sprintf("Hardware: planned %s encode + %s decode", plan.Encoder, strings.ToUpper(plan.DecodeAccel))
	default:
		line = fmt.Sprintf("Hardware: planned %s encode + CPU decode fallback", plan.Encoder)
	}

	if plan.CodecSwitch != "" {
		line += " -- " + plan.CodecSwitch
	}
	return line
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
		CodecSwitch:    describeCodecFamilySwitch(video, encoder),
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

// codecFamilies is the pair of families this pipeline knows how to encode into.
// Everything here is written against exactly these two, and a source outside
// them takes no part in the cross-family logic below.
const (
	familyH264 = "h264"
	familyHEVC = "hevc"
)

// normalizeCodecFamily maps a codec name as ffprobe reports it onto one of the
// two families, or "" for anything else.
func normalizeCodecFamily(codec string) string {
	switch strings.ToLower(strings.TrimSpace(codec)) {
	case "h264", "avc":
		return familyH264
	case "h265", "hevc":
		return familyHEVC
	default:
		return ""
	}
}

// encoderCodecFamily maps an encoder name onto the family it produces.
//
// isHEVCEncoder is asked first because it matches on "265" as well as "hevc",
// which is what puts libx265 in the right family; only then does the H.264 test
// run, so libx264rgb and h264_vaapi land where they belong.
func encoderCodecFamily(encoder string) string {
	name := strings.ToLower(encoder)
	switch {
	case name == "":
		return ""
	case isHEVCEncoder(name):
		return familyHEVC
	case strings.Contains(name, "h264") || strings.Contains(name, "x264"):
		return familyH264
	default:
		return ""
	}
}

// sourceCodecFamily is the family of the video being converted, or "".
func sourceCodecFamily(video *VideoSpecs) string {
	if video == nil || len(video.Streams) == 0 {
		return ""
	}
	return normalizeCodecFamily(video.Streams[0].Codec)
}

// otherCodecFamily names the family a source is not in.
func otherCodecFamily(codec string) string {
	switch normalizeCodecFamily(codec) {
	case familyH264:
		return familyHEVC
	case familyHEVC:
		return familyH264
	default:
		return ""
	}
}

// softwareEncoderForFamily is the CPU encoder that produces a given family.
func softwareEncoderForFamily(family string) string {
	switch family {
	case familyH264:
		return "libx264"
	case familyHEVC:
		return "libx265"
	default:
		return ""
	}
}

// displayCodecFamily is how a family is named to a user, who has never heard of
// "hevc" but knows what H.265 is.
func displayCodecFamily(family string) string {
	switch family {
	case familyH264:
		return "H.264"
	case familyHEVC:
		return "H.265"
	default:
		return family
	}
}

// crossFamilyHardwareEncoder returns a usable hardware encoder from the codec
// family the source is *not* in, or "" when there is none.
//
// This exists because of a measured case. On an Intel HD 620 laptop, every
// hevc_* encoder was refused by the driver -- "No usable encoding entrypoint
// found for profile VAProfileHEVCMain" -- while h264_vaapi passed its probe.
// The source was HEVC, so the search never looked at the H.264 list, and a
// machine with a working hardware encoder spent a quarter of an hour on libx265
// and 6 GB of memory it did not have.
//
// A source outside the two known families yields nothing, because
// candidateEncodersForCodec answers such a source with CPU encoders only.
func crossFamilyHardwareEncoder(codec string, profile *MachineProfile) string {
	for _, candidate := range candidateEncodersForCodec(otherCodecFamily(codec)) {
		if isHardwareEncoder(candidate) && canUseEncoderWithProfile(candidate, profile) {
			return candidate
		}
	}
	return ""
}

// describeCodecFamilySwitch states what a cross-family choice costs, in the one
// clause the GUI has room for. It returns "" for the ordinary case, where the
// encoder is in the source's own family.
//
// The clause is not decoration. Moving a conversion to H.264 to reach the
// hardware also drops a 10-bit source to 8 bits, because remapFilterChain keeps
// ten bits only for HEVC encoders -- and a quality decision the user is not told
// about is one they will discover in the output file.
func describeCodecFamilySwitch(video *VideoSpecs, encoder string) string {
	source := sourceCodecFamily(video)
	target := encoderCodecFamily(encoder)
	if source == "" || target == "" || source == target {
		return ""
	}

	note := fmt.Sprintf("%s has no usable hardware encoder here, so the output is %s",
		displayCodecFamily(source), displayCodecFamily(target))
	if target == familyH264 && isHighBitDepth(sourcePixelFormat(video)) {
		note += "; the 10-bit source is stored as 8-bit"
	}
	return note
}

// softwareEncoderForFallback picks the CPU encoder to retry with after a
// hardware encoder has failed outright.
//
// The source's own family is tried first, and that is a change of rule: it used
// to follow the family of the *failed encoder*. Since Superview now moves a
// conversion to the other family when that is where the usable hardware is, the
// old rule turned "your GPU cannot do H.265, so H.264 on the GPU" into "... so
// H.264 on the CPU" -- a codec nobody had a reason to want once the GPU was out
// of the picture, and one that costs a 10-bit source its extra bits. Falling
// back inside the source's family lands where the machine would have gone had
// the cross-family move never been attempted.
//
// The failed encoder's own family is still the second choice: it covers a
// source whose codec is neither H.264 nor H.265, and the case where the
// source's CPU encoder is missing from this FFmpeg build.
func softwareEncoderForFallback(video *VideoSpecs, failed string, profile *MachineProfile) string {
	for _, family := range []string{sourceCodecFamily(video), encoderCodecFamily(failed)} {
		candidate := softwareEncoderForFamily(family)
		if candidate == "" || candidate == failed {
			continue
		}
		if canUseEncoderWithProfile(candidate, profile) {
			return candidate
		}
	}
	return ""
}

// withCodecSwitchNote appends the cost of a cross-family choice to a hardware
// summary line, and returns the line unchanged when there was no such choice.
//
// The summary is the one place in the window where a user learns which encoder
// ran, so it is where the trade has to be stated. The log carries the same
// sentence at selection time, for the reader who has only the log file.
func withCodecSwitchNote(video *VideoSpecs, encoder string, summary string) string {
	note := describeCodecFamilySwitch(video, encoder)
	if note == "" {
		return summary
	}
	return summary + " -- " + note
}

// outputFrameSize renders the frame the conversion will produce, in the "WxH"
// form ffmpeg's lavfi sources take.
func outputFrameSize(video *VideoSpecs, squeeze bool) string {
	// Same reason as memoryNeededForEncode: remapOutputSize dereferences
	// Streams[0] without checking, so the empty cases stop here.
	if video == nil || len(video.Streams) == 0 {
		return ""
	}
	outX, outY := remapOutputSize(video, squeeze)
	if outX <= 0 || outY <= 0 {
		return ""
	}
	return fmt.Sprintf("%dx%d", outX, outY)
}

// sizeProbeCache remembers what each encoder answered for a given frame size.
//
// A session-lifetime cache, and the same assumption the startup sweep already
// makes: the driver's answer for one encoder at one size does not change while
// the application is running. It matters because the window asks this question
// again on every change of file, of the squeeze checkbox and of the codec
// selection, and a probe costs about 0.30 s.
var (
	sizeProbeMu    sync.Mutex
	sizeProbeCache = map[string]EncoderProbe{}
)

// resetSizeProbeCache empties the cache. Tests only: every one of them would
// otherwise inherit the verdicts of the last.
func resetSizeProbeCache() {
	sizeProbeMu.Lock()
	defer sizeProbeMu.Unlock()
	sizeProbeCache = map[string]EncoderProbe{}
}

// probeEncoderForSizeCached is probeEncoderAtSize behind the cache.
func probeEncoderForSizeCached(ctx context.Context, encoder, size string) EncoderProbe {
	key := encoder + "@" + size

	sizeProbeMu.Lock()
	cached, ok := sizeProbeCache[key]
	sizeProbeMu.Unlock()
	if ok {
		return cached
	}

	probe := probeEncoderAtSize(ctx, encoder, size)

	sizeProbeMu.Lock()
	sizeProbeCache[key] = probe
	sizeProbeMu.Unlock()

	return probe
}

// verifyEncoderForOutput asks a hardware encoder whether it will take the frame
// this conversion is about to produce, and steps back to the CPU when it will
// not. It returns the encoder to use and, when that is not the one it was
// given, ffmpeg's own words for why.
//
// The startup sweep encodes 256x256, which is the only size available before a
// file is chosen. It proves the driver answers; it cannot prove the driver will
// take a 15-megapixel frame. On an Intel HD 620 the gap is exactly one message:
//
//	Hardware does not support encoding at size 5120x2880
//	(constraints: width 32-4096 height 32-4096)
//
// Without this, that machine selected h264_vaapi, announced H.264 to the user,
// failed twice inside EncodeVideo, and landed on libx265 -- whose memory
// requirement the guard had already declined to estimate, because the encoder
// it was shown was a hardware one. Asking here makes the announcement, the
// memory estimate and the encoder that runs the same three things.
//
// Software encoders are returned untouched: they have no device to consult, and
// a probe would only cost the conversion a process start.
func verifyEncoderForOutput(ctx context.Context, encoder string, video *VideoSpecs, squeeze bool, ffmpeg map[string]string) (string, string) {
	if encoder == "" || !isHardwareEncoder(encoder) {
		return encoder, ""
	}

	size := outputFrameSize(video, squeeze)
	if size == "" {
		return encoder, ""
	}

	probe := probeEncoderForSizeCached(ctx, encoder, size)
	if probe.Usable {
		return encoder, ""
	}

	reason := fmt.Sprintf("%s cannot encode %s on this machine: %s", encoder, size, probe.Reason)

	fallback := softwareEncoderForFallback(video, encoder, AnalyzeMachineProfile(ffmpeg))
	if fallback == "" {
		// Nothing to step back to. Say so and leave the encoder alone: the
		// cascade in EncodeVideo is then the only thing left, exactly as before.
		logger.Warn("The hardware encoder refuses this frame and there is no CPU encoder to fall back to",
			slog.String("encoder", encoder),
			slog.String("output_size", size),
			slog.String("reason", probe.Reason),
		)
		return encoder, reason
	}

	logger.Warn("The hardware encoder refuses this frame; using the CPU encoder instead",
		slog.String("encoder", encoder),
		slog.String("output_size", size),
		slog.String("reason", probe.Reason),
		slog.String("using", fallback),
	)

	return fallback, reason
}

// DescribeVerifiedHardwarePlan is DescribeHardwareAccelerationPlan with the
// hardware encoder actually asked about the frame it would be given.
//
// It takes a context and can spend a probe, so it belongs off the UI thread --
// the window already runs the startup sweep that way. The cache makes every
// repeat of the same question free.
func DescribeVerifiedHardwarePlan(ctx context.Context, ffmpeg map[string]string, video *VideoSpecs, squeeze bool, requestedEncoder string) string {
	if video == nil {
		return "Hardware: waiting for input video"
	}

	plan, err := BuildHardwarePlan(ffmpeg, video, requestedEncoder)
	if err != nil {
		return fmt.Sprintf("Hardware: %s", err.Error())
	}

	verified, refusal := verifyEncoderForOutput(ctx, plan.Encoder, video, squeeze, ffmpeg)
	if refusal == "" {
		return describePlannedHardwarePath(plan)
	}

	// Rebuild the plan around the encoder that will really run, so the decode
	// path and the codec-switch note describe it and not the one that refused.
	plan.Encoder = verified
	plan.HardwareEncode = isHardwareEncoder(verified)
	plan.DecodeAccel = selectHardwareDecodeAccel(verified, AnalyzeMachineProfile(ffmpeg))
	plan.CodecSwitch = describeCodecFamilySwitch(video, verified)

	return describePlannedHardwarePath(plan) + " -- " + refusal
}

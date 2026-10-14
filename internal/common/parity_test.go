package common

import (
	"bytes"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"flag"
	"fmt"
	"os"
	"path/filepath"
	"reflect"
	"sort"
	"testing"
)

// Parity oracle for the Rust port (DEC-43).
//
// Each test computes the outputs of pure functions over a fixed matrix of
// inputs and compares them with testdata/parity/<domain>.json. The Rust port
// reads the same files and must reproduce every field, so these files, not the
// Go code, are the contract. Changing one is a decision, not a detail:
// `go test ./internal/common -run Parity -update-parity` rewrites them, and the
// git diff that results is what must be reviewed. -update-parity refuses to
// write after a failure or from a filtered run, so a partial result can never
// replace a complete file.
//
// The JSON is deterministic: sorted keys (encoding/json sorts maps), two-space
// indentation, final newline. No output depends on the machine: FindEncoder
// reads runtime.NumCPU() through AnalyzeMachineProfile, but CPUCores influences
// neither the encoder choice nor the error (only the pipeline log reads it), so
// nothing recorded here depends on it.
var updateParity = flag.Bool("update-parity", false, "rewrite testdata/parity/*.json instead of comparing")

// parityCases is the content of a file: stable case name -> result.
type parityCases map[string]any

// checkParity compares a parity file, or rewrites it with -update-parity.
func checkParity(t *testing.T, domain string, got parityCases) {
	t.Helper()

	var buf bytes.Buffer
	enc := json.NewEncoder(&buf)
	enc.SetEscapeHTML(false)
	enc.SetIndent("", "  ")
	if err := enc.Encode(got); err != nil {
		t.Fatalf("serialising %s: %v", domain, err)
	}
	path := filepath.Join("testdata", "parity", domain+".json")

	if *updateParity {
		if t.Failed() {
			t.Fatalf("%s: refusing to rewrite %s after a failure", domain, path)
		}
		if err := os.MkdirAll(filepath.Dir(path), 0o755); err != nil {
			t.Fatal(err)
		}
		if err := os.WriteFile(path, buf.Bytes(), 0o644); err != nil {
			t.Fatal(err)
		}
		t.Logf("%s rewritten (%d cases)", path, len(got))
		return
	}

	wantBytes, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("reading %s: %v (run with -update-parity to create it)", path, err)
	}
	if bytes.Equal(buf.Bytes(), wantBytes) {
		return
	}

	// The bytes differ: name the cases rather than printing two files.
	var want map[string]any
	if err := json.Unmarshal(wantBytes, &want); err != nil {
		t.Fatalf("%s is not valid JSON: %v", path, err)
	}
	var gotAny map[string]any
	if err := json.Unmarshal(buf.Bytes(), &gotAny); err != nil {
		t.Fatal(err)
	}
	names := map[string]bool{}
	for k := range want {
		names[k] = true
	}
	for k := range gotAny {
		names[k] = true
	}
	sorted := make([]string, 0, len(names))
	for k := range names {
		sorted = append(sorted, k)
	}
	sort.Strings(sorted)

	shown, differing := 0, 0
	for _, name := range sorted {
		w, inWant := want[name]
		g, inGot := gotAny[name]
		if inWant && inGot && reflect.DeepEqual(w, g) {
			continue
		}
		differing++
		if shown >= 10 {
			continue
		}
		shown++
		wj, _ := json.Marshal(w)
		gj, _ := json.Marshal(g)
		switch {
		case !inWant:
			t.Errorf("%s: case %q is missing from the file\n  got  %s", domain, name, gj)
		case !inGot:
			t.Errorf("%s: case %q is no longer produced\n  want %s", domain, name, wj)
		default:
			t.Errorf("%s: case %q changed\n  got  %s\n  want %s", domain, name, gj, wj)
		}
	}
	if differing > shown {
		t.Errorf("%s: %d more cases differ", domain, differing-shown)
	}
	if differing == 0 {
		t.Errorf("%s: same content, different formatting (rewrite with -update-parity)", domain)
	}
}

// parityError describes an error by its type and message, which is what the
// port must reproduce. Untyped errors (the ones fmt.Errorf wraps) are "plain":
// their message follows the Go standard library's text (encoding/json,
// strconv), which the port need not imitate word for word.
func parityError(err error) map[string]any {
	kind := "plain"
	var invalid *InvalidVideoError
	var encoder *EncoderError
	var session *SessionError
	switch {
	case errors.As(err, &invalid):
		kind = "InvalidVideoError"
	case errors.As(err, &encoder):
		kind = "EncoderError"
	case errors.As(err, &session):
		kind = "SessionError"
	}
	return map[string]any{"type": kind, "message": err.Error()}
}

// parityVideo builds a minimal source for the functions that read only the
// codec and the pixel format.
func parityVideo(codec, pixFmt string) *VideoSpecs {
	return &VideoSpecs{File: "input.mp4", Streams: []VideoStream{{
		Codec: codec, Width: 1440, Height: 1080,
		Duration: "60", DurationFloat: 60,
		Bitrate: "5000000", BitrateInt: 5000000,
		PixFmt: pixFmt,
	}}}
}

// parityCapabilities is what CheckFfmpeg would record under the "accels" and
// "encoders" keys, under a stable name.
var parityCapabilities = []struct {
	name   string
	ffmpeg map[string]string
}{
	{"nil_map", nil},
	{"empty", map[string]string{}},
	{"cpu_only", map[string]string{"encoders": "libx264,libx265"}},
	{"cpu_x264_only", map[string]string{"encoders": "libx264"}},
	{"cpu_x265_only", map[string]string{"encoders": "libx265"}},
	{"nvidia", map[string]string{"accels": "cuda", "encoders": "h264_nvenc,hevc_nvenc,libx264,libx265"}},
	{"nvidia_h264_only", map[string]string{"accels": "cuda", "encoders": "h264_nvenc,libx264,libx265"}},
	{"vaapi_h264_only", map[string]string{"accels": "vaapi", "encoders": "h264_vaapi,libx264,libx265"}},
	{"vaapi_encoder_without_accel", map[string]string{"encoders": "h264_vaapi,hevc_vaapi,libx264,libx265"}},
	{"qsv_without_accel", map[string]string{"encoders": "h264_qsv,hevc_qsv,libx264,libx265"}},
	{"amf_windows", map[string]string{"accels": "d3d11va,dxva2", "encoders": "h264_amf,hevc_amf,libx264,libx265"}},
	{"vulkan_hevc_only", map[string]string{"accels": "vulkan", "encoders": "hevc_vulkan,libx264,libx265"}},
	{"d3d12va_with_accel", map[string]string{"accels": "d3d12va", "encoders": "h264_d3d12va,hevc_d3d12va,libx264,libx265"}},
	{"d3d12va_without_accel", map[string]string{"encoders": "h264_d3d12va,hevc_d3d12va,libx264,libx265"}},
	{"vulkan_hevc_without_accel", map[string]string{"encoders": "hevc_vulkan,libx264,libx265"}},
	{"only_libx264rgb", map[string]string{"encoders": "libx264rgb"}},
	{"v4l2m2m_only", map[string]string{"encoders": "h264_v4l2m2m,libx264"}},
	{"hardware_only_no_cpu", map[string]string{"accels": "cuda", "encoders": "h264_nvenc"}},
	{"unknown_encoder_only", map[string]string{"encoders": "foo_enc,libaom-av1"}},
	{"csv_with_spaces", map[string]string{"encoders": " libx265 , ,libx264 "}},
}

// TestParity_PGM records the output size and the fingerprint of both maps over
// a matrix of geometries, with and without squeeze. Same method as
// TestGeneratePGM_Golden, whose four fingerprints are re-checked here.
func TestParity_PGM(t *testing.T) {
	geometries := []struct{ width, height int }{
		{640, 480}, {1440, 1080}, {641, 481}, {1920, 1440}, {5120, 3840},
		{480, 640}, {1080, 1440}, // portraits: refused without squeeze
	}

	got := parityCases{}
	for _, g := range geometries {
		for _, squeeze := range []bool{false, true} {
			name := fmt.Sprintf("%dx%d_squeeze=%t", g.width, g.height, squeeze)
			video := &VideoSpecs{File: "parity.mp4", Streams: []VideoStream{{
				Codec: "h264", Width: g.width, Height: g.height,
				Duration: "60", DurationFloat: 60,
				Bitrate: "5000000", BitrateInt: 5000000,
			}}}

			outX, outY := remapOutputSize(video, squeeze)
			entry := map[string]any{"out_width": outX, "out_height": outY}

			// One subtest per case: the session is global state and each map
			// weighs up to tens of MB.
			t.Run(name, func(t *testing.T) {
				if err := InitEncodingSession(nil); err != nil {
					t.Fatalf("InitEncodingSession: %v", err)
				}
				defer func() {
					if err := CleanUp(); err != nil {
						t.Errorf("CleanUp: %v", err)
					}
				}()

				if err := GeneratePGM(video, squeeze); err != nil {
					entry["error"] = parityError(err)
					return
				}
				xPath, yPath, err := getSessionPaths()
				if err != nil {
					t.Fatalf("getSessionPaths: %v", err)
				}
				for _, f := range []struct{ key, path string }{{"x", xPath}, {"y", yPath}} {
					data, err := os.ReadFile(f.path)
					if err != nil {
						t.Fatalf("read %s: %v", f.key, err)
					}
					sum := sha256.Sum256(data)
					entry[f.key+"_bytes"] = len(data)
					entry[f.key+"_sha256"] = hex.EncodeToString(sum[:])
				}
			})
			got[name] = entry
		}
	}

	// A -run filter on a subtest leaves entries without a fingerprint; neither
	// the comparison nor a rewrite may go on with them.
	for name, e := range got {
		entry := e.(map[string]any)
		_, hasHash := entry["x_sha256"]
		_, hasError := entry["error"]
		if !hasHash && !hasError {
			t.Fatalf("subtest filter left %s without a result: run the whole TestParity_PGM (nothing was compared or written)", name)
		}
	}

	// The fingerprints in pgm_golden_test.go are the historical reference: if
	// the oracle departs from them, the oracle is wrong, not the golden test.
	golden := map[string][2]string{
		"1440x1080_squeeze=false": {"2227c17119b499db7ce11bc27376e423ecccbfe24bf4a9f6c49363312261202f", "87348bfb3fbb4a513dfc5f02293445c09e940ab663f1befb033c8dc383b9e6fa"},
		"1440x1080_squeeze=true":  {"f9c97781445026e2d306e05b521df416e77bca843e66b04ea3c60a44e31c6421", "38cb195cc83f764cab65cd5c3db87de8a4a66b16bdaf2e68b55b994ef807a33e"},
		"641x481_squeeze=false":   {"e500fe0ca537005f4d9a9c78b2a25492cc7e5bb0592dd0d7f4df856166897a8e", "aeddcf2ebad943c66d267d387f77eb63ab49491a7945d473a6e0e1f881656cfa"},
		"641x481_squeeze=true":    {"a10aa7c6ead73b685b651cb4e04d338ae3ebda4e2e6144b533705667de5f1e46", "aace303c7eb5336eba6bdec932e263f3610a377731e01cdbdffde248380abb72"},
	}
	for name, want := range golden {
		entry := got[name].(map[string]any)
		if entry["x_sha256"] != want[0] || entry["y_sha256"] != want[1] {
			t.Errorf("%s: fingerprints differ from pgm_golden_test.go: %v", name, entry)
		}
	}

	checkParity(t, "pgm", got)
}

// TestParity_EncodeArgs records the full argv of buildEncodeBaseArgs.
func TestParity_EncodeArgs(t *testing.T) {
	encoders := []string{
		"libx264", "libx264rgb", "libx265",
		"h264_nvenc", "hevc_nvenc", "h264_amf", "hevc_amf", "h264_qsv", "hevc_qsv",
		"h264_vaapi", "hevc_vaapi", "h264_vulkan", "hevc_vulkan", "h264_d3d12va", "hevc_d3d12va",
		"h264_v4l2m2m", "hevc_v4l2m2m",
	}
	pixFmts := []string{"", "yuv420p", "yuv420p10le"}
	// The x264 presets plus two edge cases: case/whitespace, and an unknown name.
	presets := []string{"", "ultrafast", "superfast", "veryfast", "faster", "fast", "medium", "slow", "slower", "veryslow", "placebo", " FAST ", "bogus"}
	presetEncoders := []string{"libx264", "libx265", "h264_nvenc", "h264_amf", "h264_qsv", "h264_vaapi", "h264_v4l2m2m"}

	got := parityCases{}
	add := func(name, pixFmt, encoder string, bitrate int, audio string, threads, filterThreads int, preset string) {
		if _, dup := got[name]; dup {
			t.Fatalf("duplicate case name: %s", name)
		}
		got[name] = buildEncodeBaseArgs(parityVideo("h264", pixFmt), "x.pgm", "y.pgm", encoder, bitrate, audio, threads, filterThreads, preset)
	}

	// Every encoder x pixel format x (no preset, medium).
	for _, enc := range encoders {
		for _, pf := range pixFmts {
			for _, preset := range []string{"", "medium"} {
				add(fmt.Sprintf("enc=%s/pix=%s/preset=%s", enc, pf, preset), pf, enc, 8_000_000, "copy", 8, 0, preset)
			}
		}
	}
	// Every preset on the families that translate them.
	for _, enc := range presetEncoders {
		for _, preset := range presets {
			add(fmt.Sprintf("preset-sweep/enc=%s/preset=%q", enc, preset), "yuv420p", enc, 8_000_000, "copy", 8, 0, preset)
		}
	}
	// Threads: libx265's ceiling (16), hardware encoders (no -threads).
	for _, enc := range []string{"libx264", "libx265", "h264_nvenc", "h264_vaapi"} {
		for _, threads := range []int{1, 4, 16, 17, 64} {
			add(fmt.Sprintf("threads/enc=%s/n=%d", enc, threads), "yuv420p", enc, 8_000_000, "copy", threads, 0, "medium")
		}
	}
	// Bitrates in bits per second, and -bufsize = 2 x bitrate.
	for _, bitrate := range []int{1, 2_000_000, 8_000_000, 100_000_000} {
		add(fmt.Sprintf("bitrate/n=%d", bitrate), "yuv420p", "libx264", bitrate, "copy", 8, 0, "medium")
	}
	// Pixel format names that isHighBitDepth must tell apart, on a software and a
	// device-frame HEVC encoder (the latter changes the upload format).
	for _, enc := range []string{"libx265", "hevc_vaapi"} {
		for _, pf := range []string{"yuv420p12le", "p010le", "yuv420p10be", "YUV420P10LE", "rgb565le", "nv12", "yuv410p", "gray16le"} {
			add(fmt.Sprintf("pixfmt/enc=%s/pix=%s", enc, pf), pf, enc, 8_000_000, "copy", 8, 0, "medium")
		}
	}
	// Audio and filter threads.
	for _, audio := range []string{"copy", "aac"} {
		for _, ft := range []int{0, 2} {
			add(fmt.Sprintf("audio=%s/filter_threads=%d", audio, ft), "yuv420p", "libx265", 8_000_000, audio, 8, ft, "medium")
		}
	}

	checkParity(t, "encode_args", got)
}

// TestParity_ProbeArgs records the argv of an encoder probe.
func TestParity_ProbeArgs(t *testing.T) {
	got := parityCases{}
	add := func(name string, spec probeSpec) {
		if _, dup := got[name]; dup {
			t.Fatalf("duplicate case name: %s", name)
		}
		got[name] = probeArgs(spec)
	}

	for _, enc := range []string{
		"libx264", "libx265", "h264_nvenc", "hevc_nvenc", "h264_amf", "hevc_amf", "h264_qsv", "hevc_qsv",
		"h264_vaapi", "hevc_vaapi", "h264_vulkan", "hevc_vulkan", "h264_d3d12va", "h264_v4l2m2m",
	} {
		add("sweep/"+enc, sweepSpec(enc))
		for _, pf := range []string{"yuv420p", "yuv420p10le"} {
			add(fmt.Sprintf("direct/enc=%s/pix=%s/size=5120x2880/preset=slow", enc, pf),
				probeSpec{Encoder: enc, Size: "5120x2880", PixFmt: pf, Bitrate: 8_000_000, Preset: "slow"})
		}
	}
	add("direct/empty_pixfmt_no_preset", probeSpec{Encoder: "libx265", Size: "256x256", Bitrate: probeBitrate})
	add("direct/preset_unknown", probeSpec{Encoder: "h264_nvenc", Size: "1920x1080", PixFmt: "yuv420p", Bitrate: 1, Preset: "bogus"})
	add("direct/empty_encoder", probeSpec{Size: "256x256", Bitrate: probeBitrate})

	checkParity(t, "probe_args", got)
}

// TestParity_Decisions records the encoder and hardware-decode choices.
//
// FindEncoder goes through AnalyzeMachineProfile, which reads runtime.NumCPU().
// Nothing in the selection reads that field (see the file header): the result
// is the same on every machine.
func TestParity_Decisions(t *testing.T) {
	sources := []struct {
		name  string
		video *VideoSpecs
	}{
		{"src=h264", parityVideo("h264", "yuv420p")},
		{"src=avc", parityVideo("avc", "yuv420p")},
		{"src=hevc_padded", parityVideo(" hevc ", "yuv420p")},
		{"src=hevc_10bit", parityVideo("hevc", "yuv420p10le")},
		{"src=h265", parityVideo("h265", "yuv420p")},
		{"src=HEVC_upper", parityVideo("HEVC", "yuv420p")},
		{"src=av1", parityVideo("av1", "yuv420p")},
		{"src=no_codec_name", parityVideo("", "")},
		{"src=no_streams", &VideoSpecs{File: "input.mp4"}},
	}
	result := func(encoder string, err error) map[string]any {
		if err != nil {
			return map[string]any{"error": parityError(err)}
		}
		return map[string]any{"encoder": encoder}
	}

	got := parityCases{}
	for _, caps := range parityCapabilities {
		for _, src := range sources {
			for _, requested := range []string{"", "libx264", "hevc_nvenc", "h264_vaapi", "nonexistent"} {
				// A requested encoder ignores the source's codec: two sources
				// are enough to show it.
				if requested != "" && src.name != "src=h264" && src.name != "src=no_streams" {
					continue
				}
				enc, err := FindEncoder(requested, caps.ffmpeg, src.video)
				got[fmt.Sprintf("FindEncoder/%s/%s/requested=%s", caps.name, src.name, requested)] = result(enc, err)
			}
		}
	}

	profiles := []struct {
		name    string
		profile *MachineProfile
	}{
		{"nil", nil},
		{"empty", &MachineProfile{}},
		{"cpu_both", &MachineProfile{AvailableEncoders: []string{"libx264", "libx265"}}},
		{"cpu_x264", &MachineProfile{AvailableEncoders: []string{"libx264"}}},
		{"cpu_x265", &MachineProfile{AvailableEncoders: []string{"libx265"}}},
		{"hardware_accels_only", &MachineProfile{HardwareAccels: []string{"cuda", "vaapi"}}},
		{"nvidia_full", &MachineProfile{HardwareAccels: []string{"cuda"}, AvailableEncoders: []string{"h264_nvenc", "libx264", "libx265"}}},
	}
	failedEncoders := []string{"h264_nvenc", "hevc_vaapi", "libx264", "libx265", "hevc_amf", "foo_bar", ""}
	for _, p := range profiles {
		for _, src := range sources {
			for _, failed := range failedEncoders {
				got[fmt.Sprintf("softwareEncoderForFallback/%s/%s/failed=%s", p.name, src.name, failed)] =
					map[string]any{"encoder": softwareEncoderForFallback(src.video, failed, p.profile)}
			}
		}
	}
	// A nil source, which the code accepts explicitly.
	for _, p := range profiles {
		got[fmt.Sprintf("softwareEncoderForFallback/%s/src=nil_video/failed=h264_nvenc", p.name)] =
			map[string]any{"encoder": softwareEncoderForFallback(nil, "h264_nvenc", p.profile)}
	}

	accelProfiles := []struct {
		name    string
		profile *MachineProfile
	}{
		{"nil", nil},
		{"empty", &MachineProfile{}},
		{"cuda", &MachineProfile{HardwareAccels: []string{"cuda"}}},
		{"vaapi", &MachineProfile{HardwareAccels: []string{"vaapi"}}},
		{"qsv", &MachineProfile{HardwareAccels: []string{"qsv"}}},
		{"vulkan", &MachineProfile{HardwareAccels: []string{"vulkan"}}},
		{"d3d11va_dxva2", &MachineProfile{HardwareAccels: []string{"dxva2", "d3d11va"}}},
		{"dxva2_only", &MachineProfile{HardwareAccels: []string{"dxva2"}}},
		{"d3d12va_d3d11va", &MachineProfile{HardwareAccels: []string{"d3d11va", "d3d12va"}}},
		{"cuda_and_d3d11va", &MachineProfile{HardwareAccels: []string{"d3d11va", "cuda"}}},
		{"everything", &MachineProfile{HardwareAccels: []string{"cuda", "vaapi", "qsv", "vulkan", "d3d11va", "dxva2", "d3d12va"}}},
	}
	for _, p := range accelProfiles {
		for _, enc := range []string{
			"h264_nvenc", "hevc_nvenc", "h264_amf", "h264_qsv", "h264_vaapi", "hevc_vulkan",
			"h264_d3d12va", "h264_v4l2m2m", "libx264", "libx265", "",
		} {
			got[fmt.Sprintf("selectHardwareDecodeAccel/%s/enc=%s", p.name, enc)] =
				map[string]any{"accel": selectHardwareDecodeAccel(enc, p.profile)}
		}
	}

	checkParity(t, "decisions", got)
}

// TestParity_Memory records the memory estimate of a software encode.
func TestParity_Memory(t *testing.T) {
	geometries := []struct{ width, height int }{
		{640, 480}, {1920, 1080}, {2560, 1440}, {5120, 2880}, {7680, 4320},
	}
	encoders := []string{"libx264", "libx265", "h264_nvenc", "hevc_vaapi", "h264_v4l2m2m", ""}

	got := parityCases{}
	for _, g := range geometries {
		for _, enc := range encoders {
			for _, pf := range []string{"yuv420p", "yuv420p10le"} {
				for _, threads := range []int{0, 1, 4, 5, 8, 16, 32} {
					name := fmt.Sprintf("%dx%d/enc=%s/pix=%s/threads=%d", g.width, g.height, enc, pf, threads)
					got[name] = memoryNeededForFrame(g.width, g.height, parityVideo("hevc", pf), enc, threads)
				}
			}
		}
	}
	// Degenerate cases: zero or negative size, missing source or no stream.
	for _, size := range [][2]int{{0, 1080}, {1920, 0}, {-1, 1080}} {
		got[fmt.Sprintf("degenerate/%dx%d", size[0], size[1])] = memoryNeededForFrame(size[0], size[1], parityVideo("h264", ""), "libx264", 8)
	}
	got["degenerate/nil_video"] = memoryNeededForFrame(1920, 1080, nil, "libx264", 8)
	got["degenerate/no_streams"] = memoryNeededForFrame(1920, 1080, &VideoSpecs{}, "libx264", 8)

	// The same estimate from the source: goes through remapOutputSize.
	for _, squeeze := range []bool{false, true} {
		for _, enc := range []string{"libx264", "libx265"} {
			name := fmt.Sprintf("from_source/1440x1080/squeeze=%t/enc=%s/threads=8", squeeze, enc)
			got[name] = memoryNeededForEncode(parityVideo("h264", "yuv420p"), squeeze, enc, 8)
		}
	}

	got["from_source/nil"] = memoryNeededForEncode(nil, false, "libx264", 8)
	got["from_source/no_streams"] = memoryNeededForEncode(&VideoSpecs{}, false, "libx264", 8)

	checkParity(t, "memory", got)
}

// TestParity_Specs runs parseVideoSpecs on every recorded ffprobe response.
func TestParity_Specs(t *testing.T) {
	entries, err := os.ReadDir(filepath.Join("testdata", "ffprobe"))
	if err != nil {
		t.Fatal(err)
	}

	got := parityCases{}
	for _, e := range entries {
		if e.IsDir() {
			continue
		}
		specs, err := parseVideoSpecs("input.mp4", readFixture(t, e.Name()))
		if err != nil {
			got[e.Name()] = map[string]any{"error": parityError(err)}
			continue
		}
		streams := make([]any, 0, len(specs.Streams))
		for _, s := range specs.Streams {
			streams = append(streams, map[string]any{
				"codec": s.Codec, "width": s.Width, "height": s.Height,
				"duration": s.Duration, "duration_float": s.DurationFloat,
				"bitrate": s.Bitrate, "bitrate_int": s.BitrateInt,
				"pix_fmt": s.PixFmt, "r_frame_rate": s.RFrameRate, "frame_rate": s.FrameRate,
			})
		}
		got[e.Name()] = map[string]any{"file": specs.File, "streams": streams}
	}
	if len(got) == 0 {
		t.Fatal("no ffprobe fixture")
	}

	checkParity(t, "specs", got)
}

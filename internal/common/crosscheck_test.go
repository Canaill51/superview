package common

// Go/Rust cross-check: the same clip converted by Go's PerformEncoding and by
// the Rust bench (see rustHarnessEnv).
//
// Reproducibility, measured with the bench alone and with ffmpeg alone (same
// argv, same input, several runs): libx264 with its default thread count (the
// core count) under -b:v/-maxrate gives a different decoded hash from one run to
// the next; with -threads 1 it is stable. libx265 is not stable even with one
// thread on one CPU: about one run in five differs bit for bit (about 70 dB
// apart). That is why the tests pin one CPU and one thread, and why the x265
// cases are compared by similarity and not by hash.
//
// What this does NOT cover: the thread-count computation with several cores
// (Go and Rust are always both made to use one thread here), and the libx265
// cap at 16 threads, which nothing compares between Go and Rust. The libx265
// rate and -x265-params are seen only through the bench echoing its own
// bitrate argument and through parity_encode_args.rs (identical argv); at this
// clip's size the PSNR does not see a wrong rate (1 Mbit/s against 2 Mbit/s
// measured at 63 dB on the rich clip). A defect specific to HEVC on the Rust
// side is therefore covered by that argv test only.

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"sort"
	"strconv"
	"strings"
	"syscall"
	"testing"
	"time"
)

// rustHarnessEnv names the Rust test bench binary (rust/crates/superview-harness).
//
// This file is a bridge between the Go reference and its Rust port: Go has no
// windowless executable, so the comparison has to be a Go test that runs the
// bench. It disappears with Go in phase 8 of the port (DEC-43). It is never
// required outside the dedicated "cross-check" CI job, which sets the variable.
const rustHarnessEnv = "SUPERVIEW_RUST_HARNESS"

// requireRustHarnessEnv is set to "1" by that CI job: a missing bench is then a
// failure and not a skip, the same pattern as SUPERVIEW_REQUIRE_FFMPEG, so the
// job cannot go green having compared nothing.
const requireRustHarnessEnv = "SUPERVIEW_REQUIRE_RUST_HARNESS"

// crossBitrate is the same fixed rate on both sides. The encoder is fixed too:
// left to themselves Go and Rust could pick different hardware encoders and the
// comparison would mean nothing.
const crossBitrate = 2_000_000

// harnessBinary returns the bench's path, skipping when the variable is absent
// (failing instead under SUPERVIEW_REQUIRE_RUST_HARNESS=1) and failing when it
// is set but unusable.
func harnessBinary(t *testing.T) string {
	t.Helper()
	path := os.Getenv(rustHarnessEnv)
	if path == "" {
		if os.Getenv(requireRustHarnessEnv) == "1" {
			t.Fatalf("%s=1 but %s is not set", requireRustHarnessEnv, rustHarnessEnv)
		}
		t.Skipf("%s is not set: the Go/Rust cross-check only runs in its CI job", rustHarnessEnv)
	}
	info, err := os.Stat(path)
	if err != nil {
		t.Fatalf("%s=%q but the bench cannot be found: %v", rustHarnessEnv, path, err)
	}
	if info.IsDir() || info.Mode()&0o111 == 0 {
		t.Fatalf("%s=%q is not an executable file", rustHarnessEnv, path)
	}
	return path
}

// runHarness runs the bench and returns its JSON object. A non-zero exit is
// tolerated here (the JSON then says why); the callers check "ok".
//
// The bench runs pinned to one CPU (taskset), and the Go side is given
// EncoderThreads=1 to match. Measured: with ffmpeg's default thread count (the
// core count, 16 here) libx264 under -b:v/-maxrate is not deterministic, and
// the bench alone gives a different decoded hash from one run to the next,
// identical argv included. One core means "-threads 1" on both sides, which
// is reproducible, so a remaining difference is a real one.
func runHarness(t *testing.T, bin string, args ...string) map[string]any {
	t.Helper()
	taskset, err := exec.LookPath("taskset")
	if err != nil {
		t.Fatalf("taskset is needed to make the bench single-threaded: %v", err)
	}
	cpu := firstAllowedCPU(t)
	ctx, cancel := context.WithTimeout(context.Background(), 5*time.Minute)
	defer cancel()
	cmd := exec.CommandContext(ctx, taskset, append([]string{"-c", cpu, bin}, args...)...)
	// SIGTERM, not the default SIGKILL: the bench then stops its own ffmpeg
	// instead of leaving it running.
	cmd.Cancel = func() error { return cmd.Process.Signal(syscall.SIGTERM) }
	cmd.WaitDelay = 30 * time.Second
	var stderr strings.Builder
	cmd.Stderr = &stderr
	out, err := cmd.Output()
	var decoded map[string]any
	if jerr := json.Unmarshal(out, &decoded); jerr != nil {
		if _, isExit := err.(*exec.ExitError); isExit && len(out) == 0 {
			t.Fatalf("harness %v failed before printing anything (taskset -c %s refused, or the bench crashed): %v\nstderr: %s",
				args, cpu, err, stderr.String())
		}
		t.Fatalf("harness %v: stdout is not a JSON object (%v; run error %v)\nstdout: %s\nstderr: %s",
			args, jerr, err, out, stderr.String())
	}
	return decoded
}

// firstAllowedCPU returns the first CPU this process may run on, read from
// Cpus_allowed_list ("0-15" gives 0, "3,5" gives 3), so that taskset is never
// asked for a CPU a container or cpuset forbids.
func firstAllowedCPU(t *testing.T) string {
	t.Helper()
	status, err := os.ReadFile("/proc/self/status")
	if err != nil {
		t.Fatalf("cannot read /proc/self/status to pick a CPU (Linux only): %v", err)
	}
	m := regexp.MustCompile(`(?m)^Cpus_allowed_list:\s*(\d+)`).FindSubmatch(status)
	if m == nil {
		t.Fatalf("no Cpus_allowed_list in /proc/self/status")
	}
	return string(m[1])
}

// decodedFingerprint hashes the decoded frames of a file (L-36: the container
// headers always differ, and the value is only ever compared within one run).
func decodedFingerprint(t *testing.T, path, pixFmt string) string {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-v", "error", "-i", path,
		"-f", "rawvideo", "-pix_fmt", pixFmt, "-").Output()
	if err != nil {
		t.Fatalf("ffmpeg decode of %s failed: %v", path, err)
	}
	if len(out) == 0 {
		t.Fatalf("ffmpeg decoded nothing from %s: an empty hash would match anything", path)
	}
	sum := sha256.Sum256(out)
	return hex.EncodeToString(sum[:])
}

// ffprobeCSV runs ffprobe and returns the trimmed csv output.
func ffprobeCSV(t *testing.T, path string, args ...string) string {
	t.Helper()
	full := append([]string{"-v", "error"}, args...)
	full = append(full, "-of", "csv=p=0", path)
	out, err := exec.Command("ffprobe", full...).Output()
	if err != nil {
		t.Fatalf("ffprobe %v failed: %v", args, err)
	}
	return strings.TrimSpace(string(out))
}

// audioPacketHash hashes the compressed audio packets of every track
// (streamhash, per stream), so that an audio track re-encoded, truncated or
// dropped on one side shows without depending on the video comparison.
func audioPacketHash(t *testing.T, path string) string {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-v", "error", "-i", path,
		"-map", "0:a?", "-c", "copy", "-f", "streamhash", "-hash", "sha256", "-").Output()
	if err != nil {
		t.Fatalf("ffmpeg audio hash of %s failed: %v", path, err)
	}
	return strings.ReplaceAll(strings.TrimSpace(string(out)), "\n", ";")
}

// describeOutput is one summary of everything the conversion is meant to
// preserve, independent of the PSNR fallback: video parameters and frame count,
// every audio track's parameters and packets, and the recording date.
func describeOutput(t *testing.T, path string) string {
	t.Helper()
	video := ffprobeCSV(t, path, "-select_streams", "v:0", "-show_entries",
		"stream=codec_name,width,height,pix_fmt,codec_tag_string,nb_frames,avg_frame_rate,sample_aspect_ratio")
	duration := ffprobeCSV(t, path, "-select_streams", "v:0", "-show_entries", "stream=duration")
	if d, err := strconv.ParseFloat(duration, 64); err == nil {
		duration = fmt.Sprintf("%.2f", d)
	}
	audio := ffprobeCSV(t, path, "-select_streams", "a", "-show_entries",
		"stream=codec_name,sample_rate,channels")
	created := ffprobeCSV(t, path, "-show_entries", "format_tags=creation_time")
	return fmt.Sprintf("video=%q duration=%s audio=%q audio_packets=%q creation_time=%q",
		video, duration, strings.ReplaceAll(audio, "\n", ";"), audioPacketHash(t, path), created)
}

// crossMinPSNR is the similarity (dB) below which two x265 outputs are
// considered different videos. Measured on this clip, two encodes of the same
// command differ by about 69 dB at worst; a wrong geometry lands far under 40.
// A wrong rate does NOT: 1 Mbit/s against 2 Mbit/s measured 63 dB here, the
// one-second clip saturating. The requested rate is checked through the bench's
// JSON (which echoes its argument) and, for libx264, through the exact hash.
const crossMinPSNR = 50.0

// averagePSNR compares the decoded frames of two files with ffmpeg's psnr
// filter and returns the average, +Inf when they are identical.
func averagePSNR(t *testing.T, a, b string) float64 {
	t.Helper()
	out, err := exec.Command("ffmpeg", "-hide_banner", "-i", a, "-i", b,
		"-lavfi", "[0:v][1:v]psnr", "-f", "null", "-").CombinedOutput()
	if err != nil {
		t.Fatalf("ffmpeg psnr of %s and %s failed: %v\n%s", a, b, err, out)
	}
	m := regexp.MustCompile(`average:(\S+)`).FindSubmatch(out)
	if m == nil {
		t.Fatalf("no PSNR average in ffmpeg's output:\n%s", out)
	}
	value, err := strconv.ParseFloat(string(m[1]), 64)
	if err != nil {
		t.Fatalf("bad PSNR %q: %v", m[1], err)
	}
	return value
}

type crossCase struct {
	name    string
	rich    bool
	encoder string
	squeeze bool
	// similar lets the decoded frames differ slightly (see crossMinPSNR). Only
	// libx265 needs it: with the same argv, even one thread and one CPU, two
	// runs of ffmpeg's libx265 sometimes produce different streams (measured:
	// the bench against itself, and ffmpeg alone). libx264 on one thread is
	// reproducible and is compared exactly.
	similar bool
}

// TestCrossCheck_GoAndRustProduceTheSameVideo converts one clip with Go and
// with the Rust bench, with the same encoder, rate and squeeze, and compares
// the decoded frames, the ffprobe summary and what the bench reports.
//
// Counter-proof: pass --squeeze to one side only and the cases redden; a
// different rate for the bench reddens through the JSON check and, for libx264,
// the hash (not through the PSNR of the libx265 cases).
func TestCrossCheck_GoAndRustProduceTheSameVideo(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}
	bin := harnessBinary(t)

	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		skipWithoutFFmpeg(t, "CheckFfmpeg failed: %v", err)
	}

	cases := []crossCase{
		{name: "plain_x264", encoder: "libx264"},
		{name: "plain_x264_squeeze", encoder: "libx264", squeeze: true},
		{name: "rich_x265", rich: true, encoder: "libx265", similar: true},
		{name: "rich_x265_squeeze", rich: true, encoder: "libx265", squeeze: true, similar: true},
	}
	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if !strings.Contains(ffmpeg["encoders"], tc.encoder) {
				skipWithoutFFmpeg(t, "%s not available in this ffmpeg build", tc.encoder)
			}
			var input string
			pixFmt := "yuv420p"
			if tc.rich {
				input = makeRichTestClip(t, 320, 240)
				pixFmt = "yuv420p10le"
			} else {
				input = makeTestClip(t, 640, 480, 3)
			}
			dir := t.TempDir()
			outGo := filepath.Join(dir, "out_go.mp4")
			outRust := filepath.Join(dir, "out_rust.mp4")

			cfg := DefaultConfig()
			cfg.EncoderThreads = 1 // matches the bench pinned to one CPU
			ui := &integrationUI{bitrate: crossBitrate, encoder: tc.encoder, squeeze: tc.squeeze}
			if err := PerformEncoding(cfg, input, outGo, ui, ffmpeg, make(chan struct{})); err != nil {
				t.Fatalf("Go PerformEncoding: %v", err)
			}
			goHW := GetLastHardwareAccelerationSummary()

			args := []string{"convert", input, outRust,
				"--encoder", tc.encoder, "--bitrate", strconv.Itoa(crossBitrate)}
			if tc.squeeze {
				args = append(args, "--squeeze")
			}
			report := runHarness(t, bin, args...)
			if report["ok"] != true {
				t.Fatalf("the bench failed: %v", report)
			}

			goPrint := decodedFingerprint(t, outGo, pixFmt)
			rustPrint := decodedFingerprint(t, outRust, pixFmt)
			goInfo := describeOutput(t, outGo)
			rustInfo := describeOutput(t, outRust)
			reportJSON, err := json.Marshal(report)
			if err != nil {
				t.Fatalf("cannot re-encode the bench JSON: %v", err)
			}
			details := fmt.Sprintf("\n go   : %s\n       %s\n rust : %s\n       %s\n bench JSON: %s\n go hardware: %q",
				goPrint, goInfo, rustPrint, rustInfo, reportJSON, goHW)

			if goInfo != rustInfo {
				t.Errorf("ffprobe summaries differ:%s", details)
			}
			if goPrint != rustPrint {
				if !tc.similar {
					t.Errorf("decoded frames differ:%s", details)
				} else if psnr := averagePSNR(t, outGo, outRust); psnr < crossMinPSNR {
					t.Errorf("decoded frames differ (PSNR %.1f dB, minimum %.0f):%s", psnr, crossMinPSNR, details)
				} else {
					t.Logf("libx265 streams differ bit for bit but agree to %.1f dB", psnr)
				}
			}
			if report["encoder"] != tc.encoder {
				t.Errorf("bench encoder = %v, Go used %q:%s", report["encoder"], tc.encoder, details)
			}
			if got, ok := report["bitrate"].(float64); !ok || int(got) != crossBitrate {
				t.Errorf("bench bitrate = %v, Go used %d:%s", report["bitrate"], crossBitrate, details)
			}
			if report["hardware_acceleration"] != goHW {
				t.Errorf("hardware_acceleration: bench %v, Go %q:%s",
					report["hardware_acceleration"], goHW, details)
			}
		})
	}
}

// TestCrossCheck_UsableEncodersAreTheSame compares, as sorted sets, the
// encoders the bench's probe finds usable with the ones Go's probe finds.
func TestCrossCheck_UsableEncodersAreTheSame(t *testing.T) {
	if testing.Short() {
		t.Skip("skipping integration test in short mode")
	}
	bin := harnessBinary(t)

	ffmpeg, err := CheckFfmpeg(nil)
	if err != nil {
		skipWithoutFFmpeg(t, "CheckFfmpeg failed: %v", err)
	}
	var goUsable []string
	for _, p := range ProbeHardwareSupport(context.Background(), ffmpeg).Results {
		if p.Usable {
			goUsable = append(goUsable, p.Encoder)
		}
	}

	report := runHarness(t, bin, "probe")
	if report["ok"] != true {
		t.Fatalf("the bench probe failed: %v", report)
	}
	entries, _ := report["probe"].([]any)
	var rustUsable []string
	for _, e := range entries {
		m, _ := e.(map[string]any)
		if m["usable"] == true {
			name, _ := m["encoder"].(string)
			rustUsable = append(rustUsable, name)
		}
	}
	if len(entries) == 0 {
		t.Fatalf("the bench probed no encoder at all: %v", report)
	}

	sort.Strings(goUsable)
	sort.Strings(rustUsable)
	if strings.Join(goUsable, ",") != strings.Join(rustUsable, ",") {
		t.Errorf("usable encoders differ:\n go   : %v\n rust : %v", goUsable, rustUsable)
	}
}

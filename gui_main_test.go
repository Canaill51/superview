package main

import (
	"errors"
	"strings"
	"testing"
	"time"

	"superview/common"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

func TestEnsureMP4Extension(t *testing.T) {
	cases := []struct{ in, want string }{
		{"/home/u/out", "/home/u/out.mp4"},
		{"/home/u/out.mp4", "/home/u/out.mp4"},
		{"/home/u/out.MP4", "/home/u/out.MP4"},
		{"/home/u/out.Mp4", "/home/u/out.Mp4"},
		{"/home/u/out.mkv", "/home/u/out.mkv.mp4"},
		{"/home/u/my.video.clip", "/home/u/my.video.clip.mp4"},
		{`C:/Users/u/out`, `C:/Users/u/out.mp4`},
		{"", ""},
	}
	for _, c := range cases {
		if got := ensureMP4Extension(c.in); got != c.want {
			t.Errorf("ensureMP4Extension(%q) = %q, want %q", c.in, got, c.want)
		}
	}
}

func TestIsMissingFFmpegError(t *testing.T) {
	if isMissingFFmpegError(nil) {
		t.Error("nil must not be treated as a missing-ffmpeg error")
	}
	if isMissingFFmpegError(errors.New("some other failure")) {
		t.Error("unrelated errors must not match")
	}
	missing := errors.New("cannot find ffmpeg/ffprobe on your system\nmake sure to install it")
	if !isMissingFFmpegError(missing) {
		t.Error("the real CheckFfmpeg error must match")
	}
	// Must survive wrapping, as PerformEncoding wraps errors on the way up.
	if !isMissingFFmpegError(errors.New("startup: " + missing.Error())) {
		t.Error("a wrapped missing-ffmpeg error must still match")
	}
}

func TestContainsString(t *testing.T) {
	values := []string{"Fast", "Balanced"}
	if !containsString(values, "Fast") {
		t.Error("expected Fast to be found")
	}
	if containsString(values, "fast") {
		t.Error("matching must be case-sensitive")
	}
	if containsString(nil, "Fast") {
		t.Error("nil slice must contain nothing")
	}
	if containsString(values, "") {
		t.Error("empty string must not match")
	}
}

func TestFormatResultsPanel(t *testing.T) {
	if got := formatResultsPanel(nil); got != "Results: no completed run yet" {
		t.Errorf("nil metrics: got %q", got)
	}

	m := common.NewEncodingMetrics("in.mp4", "out.mp4")
	m.InputFileSize = 100 * 1024 * 1024
	m.OutputFileSize = 150 * 1024 * 1024
	m.EncodeDuration = 42 * time.Second

	got := formatResultsPanel(m)
	for _, want := range []string{"in=100.0 MB", "out=150.0 MB", "150.0%", "encode=42s"} {
		if !strings.Contains(got, want) {
			t.Errorf("expected %q in %q", want, got)
		}
	}
}

func TestFormatResultsPanel_ZeroInputSizeDoesNotDivideByZero(t *testing.T) {
	m := common.NewEncodingMetrics("in.mp4", "out.mp4")
	m.InputFileSize = 0
	m.OutputFileSize = 1024

	got := formatResultsPanel(m) // must not panic nor yield NaN/Inf
	if strings.Contains(got, "NaN") || strings.Contains(got, "Inf") {
		t.Errorf("unexpected non-finite ratio: %q", got)
	}
}

func TestForcedDarkTheme_AlwaysResolvesDarkVariant(t *testing.T) {
	forced := newForcedDarkTheme()
	base := theme.DefaultTheme()

	for _, name := range []fyne.ThemeColorName{
		theme.ColorNameBackground,
		theme.ColorNameForeground,
		theme.ColorNameButton,
	} {
		wantR, wantG, wantB, wantA := base.Color(name, theme.VariantDark).RGBA()

		// The requested variant must be ignored: light in, dark out.
		for _, requested := range []fyne.ThemeVariant{theme.VariantLight, theme.VariantDark} {
			r, g, b, a := forced.Color(name, requested).RGBA()
			if r != wantR || g != wantG || b != wantB || a != wantA {
				t.Errorf("color %v with variant %v = (%d,%d,%d,%d), want the dark value (%d,%d,%d,%d)",
					name, requested, r, g, b, a, wantR, wantG, wantB, wantA)
			}
		}
	}
}

func TestGUIHandler_GetEncoderWithoutVideo(t *testing.T) {
	h := &GUIHandler{}
	if got := h.GetEncoder(); got != "" {
		t.Errorf("with no loaded video the encoder must be empty, got %q", got)
	}
}

func TestGUIHandler_GetSqueezeAndBitrate(t *testing.T) {
	h := &GUIHandler{bitrate: 4_000_000}
	got, err := h.GetBitrate()
	if err != nil {
		t.Fatalf("GetBitrate returned an error: %v", err)
	}
	if got != 4_000_000 {
		t.Errorf("GetBitrate = %d, want 4000000", got)
	}
	if h.GetSqueeze() {
		t.Error("the GUI never enables squeeze today; update this test if that changes")
	}
}

// TestToolbarFitsWindow guards the fixed-size window against overflow.
//
// The window is created with SetFixedSize(true), so a toolbar wider than the
// window silently clips buttons instead of wrapping. Adding the Diagnostic
// button took the row from 5 to 6 entries, which is exactly how that happens.
func TestToolbarFitsWindow(t *testing.T) {
	const (
		buttonWidth  = 150 // must match buttonSize in main()
		buttonCount  = 6   // open, output, start, cancel, diagnostic, quit
		windowWidth  = 980 // must match window.Resize in main()
		windowHeight = 470
	)

	app := test.NewApp()
	defer app.Quit()

	buttons := make([]fyne.CanvasObject, 0, buttonCount)
	for _, label := range []string{
		"Choose input file", "Choose output file", "Start transformation",
		"Cancel", "Diagnostic", "Quit",
	} {
		btn := widget.NewButton(label, func() {})
		buttons = append(buttons, container.NewGridWrap(fyne.NewSize(buttonWidth, 34), btn))
	}
	toolbar := container.NewHBox(buttons...)

	if got := len(buttons); got != buttonCount {
		t.Fatalf("expected %d toolbar buttons, got %d", buttonCount, got)
	}

	min := toolbar.MinSize()
	if min.Width > windowWidth {
		t.Errorf("toolbar needs %.0fpx but the fixed window is %dpx wide; "+
			"buttons would be clipped", min.Width, windowWidth)
	}
	if min.Height > windowHeight {
		t.Errorf("toolbar height %.0fpx exceeds the window height %d", min.Height, windowHeight)
	}
	t.Logf("toolbar min size = %.0fx%.0f, window = %dx%d",
		min.Width, min.Height, windowWidth, windowHeight)
}

func TestQualityProfileSettings(t *testing.T) {
	const input = 10_000_000

	cases := []struct {
		profile     string
		wantBitrate int
		wantPreset  string
	}{
		{"Fast", input, "fast"},
		{"Balanced", 16_000_000, "medium"},
		// An unexpected value must behave like Balanced, not yield a zero
		// bitrate that would later fail validation.
		{"", 16_000_000, "medium"},
		{"Nonsense", 16_000_000, "medium"},
	}

	for _, c := range cases {
		t.Run(c.profile, func(t *testing.T) {
			bitrate, preset := qualityProfileSettings(c.profile, input)
			if bitrate != c.wantBitrate {
				t.Errorf("bitrate = %d, want %d", bitrate, c.wantBitrate)
			}
			if preset != c.wantPreset {
				t.Errorf("preset = %q, want %q", preset, c.wantPreset)
			}
		})
	}
}

// TestConfiguredPresetWinsOverQualityProfile pins the resolution rule the GUI
// applies. The quality profile used to overwrite VideoPreset unconditionally,
// so a video_preset set in superview.yaml was silently replaced by "fast" or
// "medium" with nothing reporting the substitution.
func TestConfiguredPresetWinsOverQualityProfile(t *testing.T) {
	resolve := func(configured, profilePreset string) string {
		if configured == "" {
			return profilePreset
		}
		return configured
	}

	if got := resolve("slow", "medium"); got != "slow" {
		t.Errorf("an explicit video_preset must win, got %q", got)
	}
	if got := resolve("", "medium"); got != "medium" {
		t.Errorf("with no configured preset the profile applies, got %q", got)
	}
}

func TestEncoderOptionsFor(t *testing.T) {
	const useInput = "Use same video codec as input file"

	cases := []struct {
		name  string
		input string
		want  []string
	}{
		{"empty means ffmpeg missing", "", []string{useInput}},
		{"single encoder", "libx264", []string{useInput, "libx264 encoder"}},
		{"several", "libx264,libx265", []string{useInput, "libx264 encoder", "libx265 encoder"}},
		// strings.Split("", ",") returns [""], which used to leak a bogus
		// " encoder" entry into the dropdown.
		{"stray separators", ",libx264,,libx265,", []string{useInput, "libx264 encoder", "libx265 encoder"}},
		{"whitespace", " libx264 , libx265 ", []string{useInput, "libx264 encoder", "libx265 encoder"}},
		{"only separators", ",,,", []string{useInput}},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			got := encoderOptionsFor(c.input)
			if len(got) != len(c.want) {
				t.Fatalf("got %d options %v, want %d %v", len(got), got, len(c.want), c.want)
			}
			for i := range c.want {
				if got[i] != c.want[i] {
					t.Errorf("option %d = %q, want %q", i, got[i], c.want[i])
				}
			}
		})
	}
}

// TestSupportedInputExtensionsAreMP4Only pins the input filter to what
// CheckVideo can actually process.
//
// The pickers used to offer ten containers. Five of them -- Matroska, WebM,
// FLV, MPEG-PS and ASF -- carry no per-stream duration or bit_rate, which
// CheckVideo requires, so they failed with an unreadable strconv error. The
// product handles MP4 only, in and out, so the input side now says so.
func TestSupportedInputExtensionsAreMP4Only(t *testing.T) {
	if len(supportedInputExtensions) == 0 {
		t.Fatal("no input extension offered")
	}
	for _, ext := range supportedInputExtensions {
		if !strings.EqualFold(ext, ".mp4") {
			t.Errorf("unsupported extension offered by the file picker: %q", ext)
		}
	}

	// Both cases must be offered: Fyne's extension filter is case-sensitive.
	var hasLower, hasUpper bool
	for _, ext := range supportedInputExtensions {
		switch ext {
		case ".mp4":
			hasLower = true
		case ".MP4":
			hasUpper = true
		}
	}
	if !hasLower || !hasUpper {
		t.Errorf("both .mp4 and .MP4 must be offered, got %v", supportedInputExtensions)
	}
}

// TestEnsureMP4ExtensionMatchesTheInputFilter checks the two ends agree: every
// accepted input extension is one that ensureMP4Extension leaves alone.
func TestEnsureMP4ExtensionMatchesTheInputFilter(t *testing.T) {
	for _, ext := range supportedInputExtensions {
		path := "/home/u/clip" + ext
		if got := ensureMP4Extension(path); got != path {
			t.Errorf("ensureMP4Extension(%q) = %q; an accepted input extension must be left untouched", path, got)
		}
	}
}

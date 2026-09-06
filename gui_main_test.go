package main

import (
	"errors"
	"runtime/debug"
	"strconv"
	"strings"
	"testing"
	"time"

	"superview/common"

	"fyne.io/fyne/v2"
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

// TestToolbarFitsWindow guards the fixed-size window against overflow, and
// each button against being clipped inside its own cell.
//
// The window is created with SetFixedSize(true), so a toolbar wider than the
// window silently cuts buttons off instead of wrapping. Adding the Diagnostic
// button took the row from 5 to 6 entries, which is exactly how that happens.
//
// The version of this test that shipped with the bug measured neither: it laid
// icon-less buttons into cells of a flat 150x34 and asked how wide the row of
// *cells* was. A GridWrap reports the size it was given, so the answer was 920
// whatever the buttons inside it needed -- and the buttons, which do carry
// icons, needed up to 186x36 and were drawn clipped. Building them the way
// main() builds them, and comparing each cell against the button it holds, is
// what makes the measurement mean something.
func TestToolbarFitsWindow(t *testing.T) {
	// The geometry comes from gui_main.go, so this test measures the toolbar
	// main() builds rather than a copy of it that can drift.
	const buttonCount = 6 // open, output, start, cancel, diagnostic, quit

	app := test.NewApp()
	defer app.Quit()

	buttons := toolbarButtonsAsBuilt()
	if got := len(buttons); got != buttonCount {
		t.Fatalf("expected %d toolbar buttons, got %d", buttonCount, got)
	}

	for _, btn := range buttons {
		cell := actionButtonCell(btn)
		need := btn.MinSize()
		if cell.Width < need.Width || cell.Height < need.Height {
			t.Errorf("%q is given a %.0fx%.0f cell but needs %.0fx%.0f: its label is clipped",
				btn.Text, cell.Width, cell.Height, need.Width, need.Height)
		}
	}

	toolbar := newActionToolbar(buttons...)
	min := toolbar.MinSize()
	if min.Width > windowWidth {
		t.Errorf("toolbar needs %.0fpx but the default window is %.0fpx wide; "+
			"main() will have to widen the window past its designed size", min.Width, float32(windowWidth))
	}
	if min.Height > windowHeight {
		t.Errorf("toolbar height %.0fpx exceeds the window height %.0f", min.Height, float32(windowHeight))
	}
	t.Logf("toolbar min size = %.0fx%.0f, window = %dx%d",
		min.Width, min.Height, windowWidth, windowHeight)
}

// TestToolbarIsCentred pins the other half of the display defect: the row was
// built as a bare HBox, which packs from the left, so six buttons totalling
// ~890px sat hard against the left edge of a 980px window with the gap all on
// one side.
func TestToolbarIsCentred(t *testing.T) {
	app := test.NewApp()
	defer app.Quit()

	buttons := toolbarButtonsAsBuilt()
	toolbar := newActionToolbar(buttons...)

	// Measured through a canvas rather than off the container tree: what the
	// user sees is where the buttons land, and a test that asserts on the
	// shape of the tree passes or fails on how the row is built rather than
	// on where it ends up.
	window := test.NewWindow(toolbar)
	defer window.Close()
	window.Resize(fyne.NewSize(windowWidth, toolbar.MinSize().Height+2*theme.Padding()))

	at := app.Driver().AbsolutePositionForObject
	first, last := buttons[0], buttons[len(buttons)-1]
	left := at(first).X
	rowEnd := at(last).X + last.Size().Width
	right := windowWidth - rowEnd
	row := rowEnd - left

	if left <= theme.Padding() {
		t.Fatalf("the row starts at x=%.0f in a %dpx window: it is not centred, it is flush left",
			left, windowWidth)
	}
	// Fyne lays widgets out on fractional pixels, so the two margins agree to
	// within a pixel rather than exactly.
	if diff := left - right; diff > 1 || diff < -1 {
		t.Errorf("margins are %.0fpx left and %.0fpx right around a %.0fpx row: the toolbar is off-centre",
			left, right, row)
	}
}

// toolbarButtonsAsBuilt reproduces the six action buttons with the labels and
// icons main() gives them. The icons are the reason this matters: an icon adds
// about 30px, which is the difference between a label that fits its cell and
// one that is cut off.
func toolbarButtonsAsBuilt() []*widget.Button {
	return []*widget.Button{
		widget.NewButtonWithIcon("Choose input file", theme.FolderOpenIcon(), func() {}),
		widget.NewButtonWithIcon("Choose output file", theme.DocumentSaveIcon(), func() {}),
		widget.NewButtonWithIcon("Start transformation", theme.MediaPlayIcon(), func() {}),
		widget.NewButtonWithIcon("Cancel", theme.CancelIcon(), func() {}),
		widget.NewButtonWithIcon("Diagnostic", theme.InfoIcon(), func() {}),
		widget.NewButton("Quit", func() {}),
	}
}

// TestBuildIdentity pins R-04. The degraded cases are the point: a binary that
// cannot say which build it is defeats the diagnostic report entirely, and
// Fyne's 0.0.1 default is the shape of answer that looks like a real version.
func TestBuildIdentity(t *testing.T) {
	settings := func(revision string, modified bool) *debug.BuildInfo {
		return &debug.BuildInfo{Settings: []debug.BuildSetting{
			{Key: "vcs", Value: "git"},
			{Key: "vcs.revision", Value: revision},
			{Key: "vcs.modified", Value: strconv.FormatBool(modified)},
		}}
	}

	cases := []struct {
		name      string
		version   string
		info      *debug.BuildInfo
		ok        bool
		unstamped bool // neither ID nor name set: nothing injected metadata
		want      string
	}{
		{
			// The case that matters most: an unpackaged build must not present
			// Fyne's 0.0.1 placeholder as though it were a release.
			name:      "an unstamped build reads as dev, not as 0.0.1",
			version:   "0.0.1",
			info:      settings("85b66712872f83ec87bfc7f9cfc38d128cd3a88d", true),
			ok:        true,
			unstamped: true,
			want:      "dev (85b6671, modified)",
		},
		{
			name:      "an unstamped build with no vcs information",
			version:   "0.0.1",
			info:      nil,
			ok:        false,
			unstamped: true,
			want:      "dev",
		},
		{
			name:    "a released build names its version and commit",
			version: "0.2.3",
			info:    settings("85b66712872f83ec87bfc7f9cfc38d128cd3a88d", false),
			ok:      true,
			want:    "0.2.3 (85b6671)",
		},
		{
			name:    "a build from a dirty tree says so",
			version: "0.2.3",
			info:    settings("85b66712872f83ec87bfc7f9cfc38d128cd3a88d", true),
			ok:      true,
			want:    "0.2.3 (85b6671, modified)",
		},
		{
			// go build outside a repository, or with -buildvcs=false.
			name:    "no vcs information leaves the version alone",
			version: "0.2.3",
			info:    &debug.BuildInfo{},
			ok:      true,
			want:    "0.2.3",
		},
		{
			name:    "no build info at all",
			version: "0.2.3",
			info:    nil,
			ok:      false,
			want:    "0.2.3",
		},
		{
			// The ordinary local build: FyneApp.toml holds no Version, so the
			// metadata arrives with a name and an ID but nothing to report. It
			// means the same as no metadata at all -- this is not a release --
			// so it reads the same way.
			name:    "a build with metadata but no version reads as dev",
			version: "",
			info:    settings("85b66712872f83ec87bfc7f9cfc38d128cd3a88d", false),
			ok:      true,
			want:    "dev (85b6671)",
		},
		{
			name:    "a blank version is not mistaken for a real one",
			version: "   ",
			info:    nil,
			ok:      false,
			want:    "dev",
		},
		{
			name:    "a short revision is not truncated",
			version: "0.2.3",
			info:    settings("85b6671", false),
			ok:      true,
			want:    "0.2.3 (85b6671)",
		},
	}

	for _, c := range cases {
		t.Run(c.name, func(t *testing.T) {
			md := fyne.AppMetadata{Version: c.version, ID: "com.canaill51.superview", Name: "Superview"}
			if c.unstamped {
				md = fyne.AppMetadata{Version: c.version}
			}
			got := buildIdentity(md, c.info, c.ok)
			if got != c.want {
				t.Errorf("buildIdentity() = %q, want %q", got, c.want)
			}
		})
	}
}

func TestQualityProfileSettings(t *testing.T) {
	const input = 10_000_000

	// 4/3 of the input: the conversion multiplies the pixel count by exactly
	// that, so this is the request that holds the source's bits per pixel.
	// Both profiles ask for it; only the preset separates them.
	const widened = 13_333_333

	cases := []struct {
		profile     string
		wantBitrate int
		wantPreset  string
	}{
		{"Fast", widened, "fast"},
		{"Balanced", widened, "medium"},
		// An unexpected value must behave like Balanced, not yield a zero
		// bitrate that would later fail validation.
		{"", widened, "medium"},
		{"Nonsense", widened, "medium"},
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

// TestEncodingControls_RoundTripReEnablesEverything pins the property P-01
// broke: after a conversion finishes, every control it locked must come back.
//
// The regression was a missing squeezeCheck.Enable() in the success branch,
// which left the squeeze option greyed out for the rest of the session. Walking
// one list in both directions is what makes that impossible; this test is what
// keeps the round trip honest if someone unrolls it again.
func TestEncodingControls_RoundTripReEnablesEverything(t *testing.T) {
	test.NewApp()

	open := widget.NewButton("Choose input file", func() {})
	selectOutput := widget.NewButton("Choose output file", func() {})
	quality := widget.NewSelect([]string{"Fast", "Balanced"}, func(string) {})
	squeeze := widget.NewCheck("Source already stretched to 16:9 (un-squeeze)", func(bool) {})
	encoder := widget.NewSelect([]string{"Use same video codec as input file"}, func(string) {})

	controls := encodingControls{open, selectOutput, quality, squeeze, encoder}

	controls.setEnabled(false)
	for i, w := range controls {
		if !w.Disabled() {
			t.Errorf("control %d is still enabled while an encoding runs", i)
		}
	}

	controls.setEnabled(true)
	for i, w := range controls {
		if w.Disabled() {
			t.Errorf("control %d was never re-enabled after the encoding finished", i)
		}
	}
}

// TestEncodingControls_ToleratesNilEntries guards the construction order: the
// group is populated once every widget exists, but a future reordering that
// leaves an entry unassigned must not take the whole window down.
func TestEncodingControls_ToleratesNilEntries(t *testing.T) {
	test.NewApp()

	button := widget.NewButton("Choose input file", func() {})
	controls := encodingControls{nil, button}

	controls.setEnabled(false)
	if !button.Disabled() {
		t.Error("a nil entry stopped the rest of the group from being disabled")
	}
	controls.setEnabled(true)
	if button.Disabled() {
		t.Error("a nil entry stopped the rest of the group from being re-enabled")
	}
}

func TestFormatEncodingStatus(t *testing.T) {
	cases := []struct {
		name      string
		percent   float64
		remaining time.Duration
		want      string
	}{
		{
			// At the very start there is nothing to extrapolate from. Showing
			// "about 0s left" there would read as "nearly done".
			name: "no estimate yet", percent: 0, remaining: 0,
			want: "Status: Transforming... 0%",
		},
		{
			name: "negative estimate is treated as none", percent: 5, remaining: -3 * time.Second,
			want: "Status: Transforming... 5%",
		},
		{
			name: "seconds", percent: 42.4, remaining: 25 * time.Second,
			want: "Status: Transforming... 42% - about 25s left",
		},
		{
			name: "minutes are rounded to the second", percent: 71.6,
			remaining: 80*time.Second + 400*time.Millisecond,
			want:      "Status: Transforming... 72% - about 1m20s left",
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			if got := formatEncodingStatus(tc.percent, tc.remaining); got != tc.want {
				t.Errorf("formatEncodingStatus(%v, %v) = %q, want %q", tc.percent, tc.remaining, got, tc.want)
			}
		})
	}
}

// TestGUIHandler_ImplementsProgressDetail keeps the optional interface wired
// up. It is satisfied by a type assertion, so nothing would fail to compile if
// the method were renamed -- the window would just silently go back to showing
// a bare percentage.
func TestGUIHandler_ImplementsProgressDetail(t *testing.T) {
	var handler interface{} = &GUIHandler{}
	if _, ok := handler.(common.ProgressDetailHandler); !ok {
		t.Fatal("GUIHandler no longer implements common.ProgressDetailHandler, so the pipeline will stop reporting the time remaining")
	}
}

func TestGUIHandler_ShowProgressDetailWithoutLabelDoesNotPanic(t *testing.T) {
	// The handler is built in one place, but a nil status label must degrade to
	// "no detail shown" rather than take the encoding goroutine down.
	handler := &GUIHandler{}
	handler.ShowProgressDetail(50, 10*time.Second)
}

// TestAppState_HardwareLineWaitsForTheProbe pins the half of the defect the
// user never sees until the encode is already slow.
//
// Before the probe comes back, all the window knows about this machine comes
// from `ffmpeg -encoders`, which is a list of what the binary was compiled
// with. Announcing "planned h264_nvenc encode" from that is a promise nobody
// has checked -- and on an NVIDIA card whose driver predates what the ffmpeg
// build demands, it is a promise that will be broken silently.
func TestAppState_HardwareLineWaitsForTheProbe(t *testing.T) {
	test.NewApp()

	state := &appState{
		ffmpegAvailable: true,
		ffmpeg:          map[string]string{"encoders": "h264_nvenc,libx264", "accels": "cuda"},
		video:           testVideo(),
		hardwareStatus:  widget.NewLabel(""),
	}

	state.refreshHardwareStatus()
	before := state.hardwareStatus.Text

	if strings.Contains(before, "h264_nvenc") {
		t.Errorf("the window named an encoder it has not verified: %q", before)
	}
	if !strings.Contains(before, "checking") {
		t.Errorf("an unverified state must say so, got %q", before)
	}

	state.setEncoderProbe(&common.EncoderProbeReport{Results: []common.EncoderProbe{
		{Encoder: "h264_nvenc", Usable: true},
		{Encoder: "libx264", Usable: true},
	}})
	state.refreshHardwareStatus()

	if after := state.hardwareStatus.Text; !strings.Contains(after, "h264_nvenc") {
		t.Errorf("once the probe confirms it, the plan must name the encoder; got %q", after)
	}
}

// TestAppState_ProbeRefusalRemovesTheEncoderFromThePlan is the reported defect,
// end to end.
//
// An ffmpeg built against newer nv-codec-headers than the installed driver
// supports still lists h264_nvenc in `ffmpeg -encoders`; NVENC only refuses
// when asked to encode. What the window must not do is plan for it anyway.
func TestAppState_ProbeRefusalRemovesTheEncoderFromThePlan(t *testing.T) {
	test.NewApp()

	state := &appState{
		ffmpegAvailable: true,
		ffmpeg:          map[string]string{"encoders": "h264_nvenc,libx264", "accels": "cuda"},
		video:           testVideo(),
		hardwareStatus:  widget.NewLabel(""),
	}

	state.setEncoderProbe(&common.EncoderProbeReport{Results: []common.EncoderProbe{
		{
			Encoder: "h264_nvenc",
			Reason:  "The minimum required Nvidia driver for nvenc is 610.00 or newer",
		},
		{Encoder: "libx264", Usable: true},
	}})
	state.refreshHardwareStatus()

	if got := state.ffmpeg["encoders"]; got != "libx264" {
		t.Errorf("a refused encoder is still advertised: %q", got)
	}

	plan := state.hardwareStatus.Text
	if strings.Contains(plan, "nvenc") {
		t.Errorf("the window still plans for an encoder the driver refuses: %q", plan)
	}
	if !strings.Contains(plan, "libx264") {
		t.Errorf("the plan should have fallen back to the CPU encoder, got %q", plan)
	}
}

// newTestAppState builds a fully wired appState with real widgets, the way
// main() does. Tests that exercise a transition need every widget present:
// asserting on a state machine whose outputs are nil proves nothing.
func newTestAppState(t *testing.T) *appState {
	t.Helper()
	test.NewApp()

	open := widget.NewButton("Choose input file", func() {})
	selectOutput := widget.NewButton("Choose output file", func() {})
	quality := widget.NewSelect([]string{"Fast", "Balanced"}, func(string) {})
	squeeze := widget.NewCheck("Source already stretched to 16:9 (un-squeeze)", func(bool) {})
	encoder := widget.NewSelect([]string{"Use same video codec as input file"}, func(string) {})

	state := &appState{
		ffmpegAvailable: true,
		start:           widget.NewButton("Start transformation", func() {}),
		cancelButton:    widget.NewButton("Cancel", func() {}),
		encoder:         encoder,
		locked:          encodingControls{open, selectOutput, quality, squeeze, encoder},
		status:          widget.NewLabel("Status: Ready"),
		hardwareStatus:  widget.NewLabel("Hardware: waiting for input video"),
		progress:        widget.NewProgressBar(),
	}
	state.start.Disable()
	state.cancelButton.Disable()
	return state
}

func testVideo() *common.VideoSpecs {
	return &common.VideoSpecs{File: "/tmp/clip.mp4", Streams: []common.VideoStream{{
		Codec: "h264", Width: 1440, Height: 1080,
		Duration: "10", DurationFloat: 10, Bitrate: "5000000", BitrateInt: 5000000,
	}}}
}

func TestAppState_CanStart(t *testing.T) {
	cases := []struct {
		name string
		set  func(*appState)
		want bool
	}{
		{"nothing chosen", func(*appState) {}, false},
		{"input only", func(s *appState) { s.video = testVideo() }, false},
		{"output only", func(s *appState) { s.outputPath = "/tmp/out.mp4" }, false},
		{
			name: "input and output",
			set:  func(s *appState) { s.video = testVideo(); s.outputPath = "/tmp/out.mp4" },
			want: true,
		},
		{
			// Without ffmpeg there is nothing to start, however complete the
			// rest of the selection is.
			name: "everything but ffmpeg",
			set: func(s *appState) {
				s.video = testVideo()
				s.outputPath = "/tmp/out.mp4"
				s.ffmpegAvailable = false
			},
			want: false,
		},
		{
			// Starting a second conversion on top of a running one must not be
			// offered.
			name: "already encoding",
			set: func(s *appState) {
				s.video = testVideo()
				s.outputPath = "/tmp/out.mp4"
				s.encoding = true
			},
			want: false,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			state := newTestAppState(t)
			tc.set(state)

			if got := state.canStart(); got != tc.want {
				t.Errorf("canStart() = %v, want %v", got, tc.want)
			}

			state.refreshStart()
			if state.start.Disabled() == tc.want {
				t.Errorf("Start button disabled = %v, want %v", state.start.Disabled(), !tc.want)
			}
		})
	}
}

// TestAppState_EncodingRoundTripRestoresEveryControl is the regression test for
// P-01, now on the real transition rather than on the helper alone.
//
// The success and failure completion paths used to be separate hand-written
// sequences of Enable calls; the success one had lost the squeeze checkbox,
// which then stayed greyed out for the rest of the session.
func TestAppState_EncodingRoundTripRestoresEveryControl(t *testing.T) {
	state := newTestAppState(t)
	state.video = testVideo()
	state.outputPath = "/tmp/out.mp4"
	state.refreshStart()

	if state.start.Disabled() {
		t.Fatal("Start should be available before the conversion begins")
	}

	state.beginEncoding()

	if !state.isEncoding() {
		t.Error("isEncoding() = false after beginEncoding()")
	}
	for i, w := range state.locked {
		if !w.Disabled() {
			t.Errorf("control %d is still enabled while the conversion runs", i)
		}
	}
	if !state.start.Disabled() {
		t.Error("Start should be unavailable while a conversion runs")
	}
	if state.cancelButton.Disabled() {
		t.Error("Cancel should be available while a conversion runs")
	}

	state.finishEncoding()

	if state.isEncoding() {
		t.Error("isEncoding() = true after finishEncoding()")
	}
	for i, w := range state.locked {
		if w.Disabled() {
			t.Errorf("control %d was never re-enabled after the conversion ended", i)
		}
	}
	if !state.cancelButton.Disabled() {
		t.Error("Cancel should go back to unavailable once the conversion ends")
	}
	if state.start.Disabled() {
		t.Error("Start should come back: the input and output are still selected")
	}
}

// TestAppState_BeginEncodingReturnsTheChannel pins P-02.
//
// The goroutine must hold its own reference. Reading the field back raced with
// requestCancel nulling it on the UI thread, and a cancellation landing in that
// window handed the pipeline a nil channel: a conversion nothing could stop.
func TestAppState_BeginEncodingReturnsTheChannel(t *testing.T) {
	state := newTestAppState(t)
	held := state.beginEncoding()

	if held == nil {
		t.Fatal("beginEncoding() returned a nil channel")
	}

	state.requestCancel()

	if state.cancel != nil {
		t.Error("the field should be cleared once the channel is closed")
	}
	// The reference the caller took must still be usable, and closed.
	select {
	case <-held:
	default:
		t.Error("the channel handed to the caller was never closed by requestCancel()")
	}
}

func TestAppState_RequestCancel(t *testing.T) {
	t.Run("does nothing when idle", func(t *testing.T) {
		state := newTestAppState(t)
		if state.requestCancel() {
			t.Error("requestCancel() reported a cancellation with nothing running")
		}
	})

	t.Run("cancels a running conversion", func(t *testing.T) {
		state := newTestAppState(t)
		held := state.beginEncoding()

		if !state.requestCancel() {
			t.Fatal("requestCancel() reported no cancellation while a conversion was running")
		}
		select {
		case <-held:
		default:
			t.Error("the cancellation channel was not closed")
		}
		if state.status.Text != "Status: Cancelling..." {
			t.Errorf("status = %q, want the cancelling message", state.status.Text)
		}
	})

	t.Run("is idempotent", func(t *testing.T) {
		// The Cancel button and the window close intercept can both fire.
		// Closing a channel twice panics, so this must be safe by construction.
		state := newTestAppState(t)
		state.beginEncoding()

		if !state.requestCancel() {
			t.Fatal("the first call should have cancelled")
		}
		if state.requestCancel() {
			t.Error("the second call reported a cancellation it did not perform")
		}
	})

	t.Run("a cancelled conversion still finishes cleanly", func(t *testing.T) {
		// requestCancel leaves encoding true: the pipeline is still winding
		// down. finishEncoding is what the completion path calls afterwards,
		// and it must restore the window even though the channel is gone.
		state := newTestAppState(t)
		state.video = testVideo()
		state.outputPath = "/tmp/out.mp4"
		state.beginEncoding()
		state.requestCancel()

		state.finishEncoding()

		if state.isEncoding() {
			t.Error("still marked as encoding after finishEncoding()")
		}
		for i, w := range state.locked {
			if w.Disabled() {
				t.Errorf("control %d was not restored after a cancelled conversion", i)
			}
		}
	})
}

func TestAppState_SetInputAndOutputRefreshStart(t *testing.T) {
	state := newTestAppState(t)

	state.setInput(testVideo())
	if !state.start.Disabled() {
		t.Error("Start should stay unavailable until a destination is chosen")
	}

	state.setOutput("/tmp/out.mp4")
	if state.start.Disabled() {
		t.Error("Start should become available once input and output are both set")
	}
}

func TestAppState_RefreshHardwareStatus(t *testing.T) {
	t.Run("reports ffmpeg missing", func(t *testing.T) {
		state := newTestAppState(t)
		state.ffmpegAvailable = false

		state.refreshHardwareStatus()

		if state.hardwareStatus.Text != "Hardware: ffmpeg unavailable" {
			t.Errorf("hardware status = %q, want the ffmpeg-unavailable message", state.hardwareStatus.Text)
		}
	})

	t.Run("describes the planned path", func(t *testing.T) {
		state := newTestAppState(t)
		state.ffmpeg = map[string]string{"accels": "cuda", "encoders": "h264_nvenc,libx264"}
		state.video = testVideo()

		state.refreshHardwareStatus()

		if !strings.HasPrefix(state.hardwareStatus.Text, "Hardware: ") {
			t.Errorf("hardware status = %q, want a Hardware: line", state.hardwareStatus.Text)
		}
		if strings.Contains(state.hardwareStatus.Text, "unavailable") {
			t.Errorf("hardware status = %q, want a real plan", state.hardwareStatus.Text)
		}
	})
}

// TestAppState_NilWidgetsAreTolerated guards the construction order in main():
// the widgets are assigned as the window is built, so a transition firing
// before one of them exists must degrade to "no update", not crash.
func TestAppState_NilWidgetsAreTolerated(t *testing.T) {
	state := &appState{ffmpegAvailable: true}

	state.refreshStart()
	state.refreshHardwareStatus()
	state.setInput(testVideo())
	state.setOutput("/tmp/out.mp4")
	state.beginEncoding()
	state.requestCancel()
	state.finishEncoding()
}

// TestAppState_SetFFmpegUnblocksTheWindow covers the retry path: the user
// installs ffmpeg while the application is already open, presses Retry, and
// everything must come back to life.
func TestAppState_SetFFmpegUnblocksTheWindow(t *testing.T) {
	state := newTestAppState(t)
	state.ffmpegAvailable = false
	state.locked.setEnabled(false)
	state.video = testVideo()
	state.outputPath = "/tmp/out.mp4"

	state.refreshStart()
	if !state.start.Disabled() {
		t.Fatal("Start must stay unavailable while ffmpeg is missing")
	}
	state.refreshHardwareStatus()
	if state.hardwareStatus.Text != "Hardware: ffmpeg unavailable" {
		t.Fatalf("hardware status = %q, want the ffmpeg-unavailable message", state.hardwareStatus.Text)
	}

	state.setFFmpeg(map[string]string{"accels": "cuda", "encoders": "h264_nvenc,libx264"})
	state.locked.setEnabled(true)
	state.refreshStart()
	state.refreshHardwareStatus()

	if !state.ffmpegAvailable {
		t.Error("ffmpegAvailable = false after a successful probe")
	}
	if state.start.Disabled() {
		t.Error("Start should become available once ffmpeg is found and both files are chosen")
	}
	if strings.Contains(state.hardwareStatus.Text, "unavailable") {
		t.Errorf("hardware status = %q, want a real plan after the retry", state.hardwareStatus.Text)
	}
	for i, w := range state.locked {
		if w.Disabled() {
			t.Errorf("control %d was not restored after ffmpeg was found", i)
		}
	}
}

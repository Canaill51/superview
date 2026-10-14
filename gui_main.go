package main

import (
	"context"
	_ "embed"
	"errors"
	"fmt"
	"image/color"
	"io"
	"log/slog"
	"net/url"
	"os"
	"path/filepath"
	"runtime/debug"
	"strings"
	"superview/common"
	"sync"
	"sync/atomic"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/app"
	"fyne.io/fyne/v2/container"
	"fyne.io/fyne/v2/dialog"
	"fyne.io/fyne/v2/storage"
	"fyne.io/fyne/v2/theme"
	"fyne.io/fyne/v2/widget"
)

//go:embed Icon.png
var appIconPNG []byte

const installURL = "https://github.com/Canaill51/superview?tab=readme-ov-file#download-and-install"

// maxLogFileBytes caps the diagnostic log; past this size it restarts empty.
const maxLogFileBytes = 5 << 20 // 5 MiB

// Window and toolbar geometry.
//
// The window is fixed-size, so a toolbar wider than it pushes buttons off the
// edge rather than wrapping -- which is how adding the Diagnostic button,
// taking the row from five entries to six, could have gone unnoticed. Two
// things keep that from clipping anything: actionButtonCell never returns a
// cell smaller than the button it holds, and main() widens the window to its
// content rather than trusting windowWidth to be enough. The floor below is a
// floor, not a size: it stops "Quit" from shrinking to a chip beside "Start
// transformation", and nothing more.
//
// TestToolbarFitsWindow reads these constants so it cannot disagree with what
// main() actually builds; it used to carry its own copies of the numbers with
// a comment asking the reader to keep them in step.
const (
	actionButtonMinWidth  = 140
	actionButtonMinHeight = 36
	windowWidth           = 980
	windowHeight          = 470
)

// actionButtonCell is the cell one toolbar button gets: its own minimum,
// widened to actionButtonMinWidth so the short labels keep company with the
// long ones. A cell of a flat 150x34 used to be handed to every button
// regardless, which cut the label off the three that carry a long one --
// "Start transformation" alone needs 186px -- and cropped all six by two
// pixels vertically.
func actionButtonCell(btn *widget.Button) fyne.Size {
	return btn.MinSize().Max(fyne.NewSize(actionButtonMinWidth, actionButtonMinHeight))
}

// newActionToolbar lays the action buttons out in one centred row.
//
// The centring is the point of the wrapper: an HBox packs from the left, so
// the row sat against the left edge with the whole leftover width -- 60px of
// it -- pooled on the right.
func newActionToolbar(buttons ...*widget.Button) fyne.CanvasObject {
	cells := make([]fyne.CanvasObject, 0, len(buttons))
	for _, btn := range buttons {
		cells = append(cells, container.NewGridWrap(actionButtonCell(btn), btn))
	}
	return container.NewCenter(container.NewHBox(cells...))
}

const (
	prefQualityProfile   = "ui.quality_profile"
	prefEncoderSelection = "ui.encoder_selection"
	prefSqueezeSource    = "ui.squeeze_source"
)

// encoderOptionsFor builds the codec dropdown entries from ffmpeg's encoder
// list. strings.Split("", ",") yields [""], not an empty slice, so without the
// empty filter a bogus " encoder" entry appears whenever ffmpeg is missing or
// exposes no encoder matching the configured codecs.
func encoderOptionsFor(encoderList string) []string {
	options := []string{"Use same video codec as input file"}
	for _, enc := range strings.Split(encoderList, ",") {
		enc = strings.TrimSpace(enc)
		if enc == "" {
			continue
		}
		options = append(options, enc+" encoder")
	}
	return options
}

// widenedPixelRatio is how much the conversion multiplies the pixel count.
//
// Widening 4:3 to 16:9 at constant height takes the width from 4h/3 to 16h/9,
// so the frame carries exactly 4/3 as many pixels. Asking for 4/3 of the input
// bitrate is therefore what preserves the bits per pixel of the source.
//
// It used to be 1.6 here -- the geometric ratio plus 20%, a margin nothing
// documented. Measurement (Q-01) put the multiplier that actually matches the
// source's own quality at 1.19 by XPSNR and 1.30 by SSIM, agreeing to the third
// decimal between hevc_nvenc and libx265. So 4/3 already carries a small margin
// of its own, and 1.6 was buying quality *beyond* the source for 20% more file.
const widenedPixelRatio = 4.0 / 3.0

// qualityProfileSettings maps a GUI quality profile onto an output bitrate and
// an ffmpeg preset.
//
// Both profiles ask for the same bitrate. They differ by encoder preset alone,
// which is what "Fast" claims to be: quicker, not coarser. Leaving the bitrate
// at the input's measured 0.94 dB below the source's own quality on 97% of
// frames, with nothing in the interface saying so.
func qualityProfileSettings(profile string, inputBitrate int) (bitrate int, preset string) {
	bitrate = int(float64(inputBitrate) * widenedPixelRatio)
	switch profile {
	case "Fast":
		return bitrate, "fast"
	default: // "Balanced" and any unexpected value
		return bitrate, "medium"
	}
}

// buildIdentity names the running binary, for the diagnostic report and the
// log the README asks users to attach to a bug report.
//
// Two sources, and both are needed. Fyne's metadata carries whatever
// "fyne package --app-version" stamped, which is how the release workflow
// passes the tag. A plain "go build" carries no such stamp, so it has no
// version to report -- and must not borrow one. Go's VCS stamping settles the
// rest, because the revision is exact and vcs.modified says whether the tree
// was clean. Verified to survive "fyne package", which is what the release
// builds with.
//
// "dev" covers both ways a binary can arrive unstamped, because they mean the
// same thing to the person reading a bug report -- this did not come from a
// release:
//
//   - No Fyne metadata at all. md.Version is then Fyne's placeholder of 0.0.1,
//     a string that reads exactly like a real version. Fyne's own test for "no
//     metadata was injected" is that neither ID nor name is set; use the same
//     one rather than matching on the placeholder, which is theirs to change.
//   - Metadata present but no version, which is the ordinary local build:
//     FyneApp.toml deliberately holds no Version, so that the number can only
//     ever come from the tag.
func buildIdentity(md fyne.AppMetadata, info *debug.BuildInfo, ok bool) string {
	version := strings.TrimSpace(md.Version)
	if version == "" || (md.ID == "" && md.Name == "") {
		version = "dev"
	}
	if !ok || info == nil {
		return version
	}

	revision := ""
	modified := false
	for _, setting := range info.Settings {
		switch setting.Key {
		case "vcs.revision":
			revision = setting.Value
		case "vcs.modified":
			modified = setting.Value == "true"
		}
	}
	if revision == "" {
		return version
	}
	if len(revision) > 7 {
		revision = revision[:7]
	}
	if modified {
		return fmt.Sprintf("%s (%s, modified)", version, revision)
	}
	return fmt.Sprintf("%s (%s)", version, revision)
}

// supportedInputExtensions lists the extensions offered by the file pickers.
// Superview reads and writes MP4 only; the output path is already forced to
// .mp4 by ensureMP4Extension, and the input side now matches.
var supportedInputExtensions = []string{".mp4", ".MP4"}

// ensureMP4Extension appends ".mp4" unless the path already carries it.
// The comparison is case-insensitive so "CLIP.MP4" is left alone.
func ensureMP4Extension(path string) string {
	if path == "" {
		return path
	}
	if strings.EqualFold(filepath.Ext(path), ".mp4") {
		return path
	}
	return path + ".mp4"
}

// isMissingFFmpegError reports whether an error is the "ffmpeg not installed"
// case, which gets a dedicated dialog carrying an install link.
func isMissingFFmpegError(err error) bool {
	return err != nil && strings.Contains(err.Error(), "cannot find ffmpeg/ffprobe")
}

func containsString(values []string, target string) bool {
	for _, value := range values {
		if value == target {
			return true
		}
	}
	return false
}

// showPrerequisiteDialog reports the "ffmpeg is not installed" case with an
// install link. When onRetry is non-nil the dialog also offers a Retry button:
// users almost always install ffmpeg right after reading this message, and
// without a retry they would have to restart the application to be noticed.
// Returns false if err is some other failure, which the caller must then report.
func showPrerequisiteDialog(window fyne.Window, err error, onRetry func()) bool {
	if !isMissingFFmpegError(err) {
		return false
	}

	parsedURL, parseErr := url.Parse(installURL)
	if parseErr != nil {
		dialog.ShowError(err, window)
		return true
	}

	content := container.NewVBox(
		widget.NewLabel("cannot find ffmpeg/ffprobe on your system"),
		widget.NewLabel("make sure to install it first:"),
		widget.NewHyperlink(installURL, parsedURL),
	)

	if onRetry == nil {
		dialog.NewCustom("Error", "OK", content, window).Show()
		return true
	}

	content.Add(widget.NewLabel("Already installed it? Use Retry."))
	dialog.NewCustomConfirm("Error", "Retry", "Close", content, func(retry bool) {
		if retry {
			onRetry()
		}
	}, window).Show()
	return true
}

// forcedDarkTheme pins the app to the dark variant.
//
// theme.DarkTheme() does the same thing but is deprecated (it ignores the user's
// system preference and is slated for removal in Fyne v3). The documented
// replacement is a custom theme that resolves colors against a fixed variant,
// which is what this does while inheriting every other theme value.
type forcedDarkTheme struct{ fyne.Theme }

func newForcedDarkTheme() fyne.Theme {
	return &forcedDarkTheme{Theme: theme.DefaultTheme()}
}

func (t *forcedDarkTheme) Color(name fyne.ThemeColorName, _ fyne.ThemeVariant) color.Color {
	return t.Theme.Color(name, theme.VariantDark)
}

// encodingControls groups every widget that must be inert while a conversion
// runs.
//
// It exists so the busy and the idle transition share a single definition of
// "every control". They used to be two hand-written sequences of
// Enable/Disable calls -- one where the encoding starts, one in each completion
// branch -- and they had already drifted apart: the success branch was missing
// squeezeCheck.Enable(), so the "Source already stretched" option stayed greyed
// out for the rest of the session after the first successful conversion.
type encodingControls []fyne.Disableable

// setEnabled applies one decision to the whole group. Round-tripping it
// (false then true) must leave every control exactly as it started, which is
// the property TestEncodingControls_RoundTripReEnablesEverything pins.
func (c encodingControls) setEnabled(enabled bool) {
	for _, w := range c {
		if w == nil {
			continue
		}
		if enabled {
			w.Enable()
		} else {
			w.Disable()
		}
	}
}

// startingEncoderStatus is what the window says between "Start transformation"
// and the first progress figure ffmpeg produces.
//
// That gap is not instant: ffmpeg reports out_time_ms=N/A until a frame has come
// out of the filter graph, which on a 15 Mpx frame is seconds and can be a
// minute. Saying so is the difference between a window that looks stuck and one
// that is visibly working.
const startingEncoderStatus = "Status: Starting the encoder... (no progress until the first frame)"

// formatEncodingStatus builds the line shown while a conversion runs.
//
// The estimate is left out entirely until there is one. Rendering a zero as
// "about 0s left" would read as "nearly done" at the very moment the encode is
// starting, which is the opposite of the truth. The test is on the *rounded*
// value: one under half a second used to pass "> 0" and print "0s" anyway.
func formatEncodingStatus(percent float64, remaining time.Duration) string {
	rounded := remaining.Round(time.Second)
	if rounded <= 0 {
		return fmt.Sprintf("Status: Transforming... %.0f%%", percent)
	}
	return fmt.Sprintf("Status: Transforming... %.0f%% - about %s left", percent, rounded)
}

// GUIHandler implements UIHandler for GUI interface.
//
// Every choice it answers for is a plain value captured on the UI thread when
// the conversion starts. It used to hold the codec dropdown itself and read
// Selected from the encoding goroutine, while the startup probe could be
// rewriting it on the main thread through SetSelected: a data race, and an
// encoder that could differ from the one on screen when Start was pressed.
type GUIHandler struct {
	window      fyne.Window
	bitrate     int
	encoderName string
	squeeze     bool
	inlineBar   *widget.ProgressBar
	status      *widget.Label
	ffmpeg      map[string]string
	video       *common.VideoSpecs
	logger      *slog.Logger
}

func (h *GUIHandler) ShowError(err error) {
	fyne.Do(func() {
		if showPrerequisiteDialog(h.window, err, nil) {
			return
		}
		dialog.ShowError(err, h.window)
	})
}

func (h *GUIHandler) ShowInfo(msg string) {
	fyne.Do(func() {
		dialog.ShowInformation("Done", msg, h.window)
	})
}

func (h *GUIHandler) ShowProgress(percent float64) {
	fyne.Do(func() {
		if h.inlineBar != nil {
			h.inlineBar.SetValue(percent / 100)
		}
	})
}

// ShowProgressDetail implements common.ProgressDetailHandler, which the
// pipeline uses when the handler offers it. Without it the window showed a bare
// percentage for a job that runs for minutes on 4K footage.
func (h *GUIHandler) ShowProgressDetail(percent float64, remaining time.Duration) {
	if h.status == nil {
		return
	}
	text := formatEncodingStatus(percent, remaining)
	fyne.Do(func() {
		h.status.SetText(text)
	})
}

func (h *GUIHandler) GetBitrate() (int, error) {
	return h.bitrate, nil
}

func (h *GUIHandler) GetEncoder() string {
	if h.video == nil || len(h.video.Streams) == 0 {
		return ""
	}
	return h.encoderName
}

func (h *GUIHandler) GetSqueeze() bool {
	return h.squeeze
}

func formatResultsPanel(metrics *common.EncodingMetrics) string {
	if metrics == nil {
		return "Results: no completed run yet"
	}

	inputMB := float64(metrics.InputFileSize) / 1024.0 / 1024.0
	outputMB := float64(metrics.OutputFileSize) / 1024.0 / 1024.0
	ratio := 0.0
	if metrics.InputFileSize > 0 {
		ratio = (float64(metrics.OutputFileSize) / float64(metrics.InputFileSize)) * 100.0
	}

	return fmt.Sprintf(
		"Results: in=%.1f MB | out=%.1f MB (%.1f%%) | total=%s | encode=%s",
		inputMB,
		outputMB,
		ratio,
		metrics.ElapsedTime().Round(time.Second).String(),
		metrics.EncodeDuration.Round(time.Millisecond).String(),
	)
}

// appState holds everything about a conversion session that outlives a single
// callback, together with the widgets those transitions drive.
//
// It exists so the transitions can be tested. They used to be closures over
// local variables of main(), reachable by no test at all -- and that is exactly
// where the two defects of the fourth review pass were living: a widget left
// disabled in one branch of the completion path, and a cancellation channel
// read from the encoding goroutine while the UI thread could null it.
//
// Every method is meant to run on the Fyne UI thread, which is where all the
// callbacks that use them execute. The one exception is documented on
// beginEncoding.
type appState struct {
	// What the user has chosen so far.
	video      *common.VideoSpecs
	outputPath string

	// The environment, refreshed whenever ffmpeg is probed.
	ffmpeg          map[string]string
	ffmpegAvailable bool

	// encoderProbe is what the advertised encoders answered when actually
	// asked to encode a frame. Nil until the probe finishes; the hardware line
	// says so rather than claiming a path it has not verified.
	encoderProbe *common.EncoderProbeReport

	// squeeze mirrors the "source already stretched" checkbox, because the
	// hardware line depends on it: it changes the size of the output frame, and
	// the size is exactly what a hardware encoder can refuse.
	squeeze bool

	// qualityProfile is the quality dropdown's current value, "Fast" or
	// "Balanced". It was a variable captured by closure in main(), which the
	// GUI state contract forbids: nothing could read it but the two callbacks.
	qualityProfile string

	// prefs is where the user's choices are persisted. Nil in a state built
	// without an application, in which case nothing is saved.
	prefs fyne.Preferences

	// restoringSelection is true while the codec dropdown is being set by the
	// program rather than by the user -- at startup, and whenever the encoder
	// list is rebuilt after a probe. The dropdown's OnChanged fires either way,
	// and must not persist a programmatic fallback: a start without ffmpeg, or
	// with a driver that refuses hevc_nvenc, used to overwrite the saved
	// "hevc_nvenc encoder" with "Use same video codec..." for good.
	restoringSelection bool

	// hardwareStatusGen counts refreshes of the hardware line, so a slow
	// verification cannot overwrite a newer answer. Every refresh takes the next
	// number; a background result applies only while its number is still the
	// current one.
	hardwareStatusGen int

	// Encoding lifecycle. cancel is non-nil exactly while a conversion is
	// running and has not been cancelled yet; requestCancel is the only place
	// allowed to close it, because closing a channel twice panics.
	//
	// done is non-nil exactly while an encoding goroutine exists. The goroutine
	// closes it once PerformEncoding has returned -- the partial file removed,
	// the session directory gone -- and shutdown waits on it, so that quitting
	// the application cannot leave an ffmpeg running on its own.
	encoding bool
	cancel   chan struct{}
	done     <-chan struct{}

	// shuttingDown is set by shutdown and read by the completion path, which
	// may run after the Fyne loop has stopped. Closures still queued when the
	// loop drains are discarded, never run (runGL in fyne v2.8.2); but a
	// fyne.Do called *after* the drain executes inline on the calling
	// goroutine, against a window that no longer exists. Atomic because that
	// late call is on the encoding goroutine while the main goroutine is the
	// one that set it.
	shuttingDown atomic.Bool

	// shutdownGaveUp records that a wait in shutdown has already run out. The
	// window's close hook and the line after ShowAndRun() both call shutdown,
	// and without it the second call waited the whole grace period again: 40 s
	// instead of the 20 s documented.
	shutdownGaveUp atomic.Bool

	// shutdownGrace bounds the wait in shutdown. Zero means the default; tests
	// set it short so a goroutine that never unwinds is proved, not waited for.
	shutdownGrace time.Duration

	// quit ends the application. It is what the completion path calls when
	// the system has asked the process to stop; nil in a state without an app.
	quit func()

	// Widgets the transitions drive.
	//
	// Each is nil-checked rather than assumed: they are assigned as main()
	// builds the window, so a reordering there would otherwise turn a missing
	// assignment into a crash instead of a missing update. It also lets a test
	// build only the part of the state machine it is exercising.
	window         fyne.Window
	start          *widget.Button
	cancelButton   *widget.Button
	encoder        *widget.Select
	locked         encodingControls
	requiresFFmpeg encodingControls
	inputLabel     *widget.Label
	outputLabel    *widget.Label
	status         *widget.Label
	results        *widget.Label
	hardwareStatus *widget.Label
	progress       *widget.ProgressBar
}

// shutdownGrace is how long an exit waits for the encoding goroutine before
// giving up on it. The slowest thing the goroutine can be inside is the
// pre-encode probe, which has a 15 s timeout of its own and does not watch the
// cancel channel; ffmpeg itself is killed within milliseconds of a cancel.
const shutdownGrace = 20 * time.Second

// windowControls names the widgets main() builds, so that the list of what
// must be inert during a conversion is computed by one function the tests can
// call with real widgets, rather than written out in main() where no test
// reaches it. That is how the Diagnostic button stayed clickable through an
// encode: it was added to the toolbar and never to the list.
type windowControls struct {
	open, selectOutput, diagnostic *widget.Button
	quality, encoder               *widget.Select
	squeeze                        *widget.Check
}

// locked is every control that changes what the next conversion will do, or
// that would run a GPU probe beside a hardware encode. Quit is deliberately
// not in it: it cancels safely now, and a user must always be able to leave.
func (c windowControls) locked() encodingControls {
	return encodingControls{c.open, c.selectOutput, c.quality, c.squeeze, c.encoder, c.diagnostic}
}

// requiresFFmpeg is every control that has nothing to do without ffmpeg. It is
// locked() minus Diagnostic, and the difference is the point: the two lists
// were one, so a start without ffmpeg greyed out the very tool that reports
// "ffmpeg not found" -- the one the README tells the user to press.
func (c windowControls) requiresFFmpeg() encodingControls {
	return encodingControls{c.open, c.selectOutput, c.quality, c.squeeze, c.encoder}
}

// applyFFmpegAvailability enables or disables what needs ffmpeg according to
// whether the last probe found it: at startup, and after a successful retry.
func (s *appState) applyFFmpegAvailability() {
	s.requiresFFmpeg.setEnabled(s.ffmpegAvailable)
}

// isEncoding reports whether a conversion is currently running.
func (s *appState) isEncoding() bool { return s.encoding }

// canStart reports whether the Start button should be available.
//
// All four conditions matter: no input, no destination, no ffmpeg, or a
// conversion already running each make starting meaningless.
func (s *appState) canStart() bool {
	return s.video != nil && s.outputPath != "" && s.ffmpegAvailable && !s.encoding
}

// refreshStart applies canStart to the button.
func (s *appState) refreshStart() {
	if s.start == nil {
		return
	}
	if s.canStart() {
		s.start.Enable()
	} else {
		s.start.Disable()
	}
}

// setInput records a validated input video and refreshes what depends on it.
func (s *appState) setInput(video *common.VideoSpecs) {
	s.video = video
	s.refreshHardwareStatus()
	s.refreshStart()
}

// setOutput records the destination and refreshes what depends on it.
func (s *appState) setOutput(path string) {
	s.outputPath = path
	s.refreshStart()
}

// chosenFileArrivesTooLate reports, and logs, a picker result that lands while
// a conversion runs.
//
// The pickers run off the UI thread and the window stays interactive while
// zenity or PowerShell is open, so a user can press Start and then validate
// the picker. Its result used to be applied anyway: state.video replaced under
// the running encode, the status line overwritten, a GPU probe started beside
// a hardware encode. It is dropped instead; the user can choose again after.
func (s *appState) chosenFileArrivesTooLate(kind, path string) bool {
	if !s.isEncoding() {
		return false
	}
	common.GetLogger().Info("Ignoring a file chosen during a conversion",
		slog.String("kind", kind), slog.String("path", path))
	return true
}

// applyChosenInput validates the file the input picker returned and makes it
// the input, unless a conversion has started since the picker opened.
func (s *appState) applyChosenInput(path string) {
	if s.chosenFileArrivesTooLate("input", path) {
		return
	}
	video, err := common.CheckVideo(path)
	if err != nil {
		if s.status != nil {
			s.status.SetText("Status: Invalid input")
		}
		if s.window != nil {
			dialog.ShowError(err, s.window)
		}
		return
	}
	s.setInput(video)
	s.verifyHardwareInBackground()
	if s.inputLabel != nil {
		s.inputLabel.SetText(filepath.Base(video.File))
	}
	if s.status != nil {
		s.status.SetText("Status: Input loaded")
	}
}

// applyChosenOutput makes the path the output picker returned the
// destination, .mp4 enforced, unless a conversion has started since the picker
// opened.
func (s *appState) applyChosenOutput(path string) {
	if s.chosenFileArrivesTooLate("output", path) {
		return
	}
	s.setOutput(ensureMP4Extension(path))
	if s.outputLabel != nil {
		s.outputLabel.SetText(filepath.Base(s.outputPath))
	}
	if s.status != nil {
		s.status.SetText("Status: Output selected")
	}
}

// setFFmpeg records a successful ffmpeg probe.
func (s *appState) setFFmpeg(ffmpeg map[string]string) {
	s.ffmpeg = ffmpeg
	s.ffmpegAvailable = true
}

// refreshHardwareStatus rewrites the line describing the path the next
// conversion will take.
func (s *appState) refreshHardwareStatus() {
	if s.hardwareStatus == nil {
		return
	}
	if !s.ffmpegAvailable {
		s.hardwareStatus.SetText("Hardware: ffmpeg unavailable")
		return
	}

	if s.encoderProbe == nil {
		s.hardwareStatus.SetText("Hardware: checking which encoders this machine accepts...")
		return
	}

	selection := ""
	if s.encoder != nil {
		selection = common.ParseEncoderSelection(s.encoder.Selected)
	}

	s.hardwareStatus.SetText(common.DescribeHardwareAccelerationPlan(s.ffmpeg, s.video, selection))

	// Any refresh invalidates a verification still in flight: the answer it is
	// carrying was computed for the state this call has just replaced.
	s.hardwareStatusGen++
}

// verifyHardwareInBackground asks the planned hardware encoder whether it will
// take the frame this file will actually produce, and rewrites the line when the
// answer changes it.
//
// Separate from refreshHardwareStatus, and deliberately so. That one is
// synchronous -- it is called from widget callbacks that must not wait, and from
// tests that read the label on the next line. Folding a goroutine into it made
// the label a value nobody could read without racing the verification. So the
// slow half is its own method, called from the places where the *inputs* to the
// verdict change: the file, the squeeze checkbox, the codec selection, and the
// startup probe that decides which encoders are even eligible.
//
// A probe not asked before costs about 0.30 s; the answers are cached for the
// session, so every repeat is free.
func (s *appState) verifyHardwareInBackground() {
	if s.hardwareStatus == nil || s.video == nil || !s.ffmpegAvailable || s.encoderProbe == nil {
		return
	}

	selection := ""
	if s.encoder != nil {
		selection = common.ParseEncoderSelection(s.encoder.Selected)
	}

	generation := s.hardwareStatusGen
	ffmpeg, video, squeeze := s.ffmpeg, s.video, s.squeeze

	go func() {
		verified := common.DescribeVerifiedHardwarePlan(context.Background(), ffmpeg, video, squeeze, selection)
		fyne.Do(func() { s.applyVerifiedHardwareLine(generation, verified) })
	}()
}

// applyVerifiedHardwareLine writes a verified line, unless the state it was
// computed for has moved on.
//
// The generation is what makes a slow verdict harmless: a user who ticks the
// squeeze box twice in a second starts two verifications, and the first must not
// land after the second and describe a geometry that is no longer selected.
func (s *appState) applyVerifiedHardwareLine(generation int, text string) {
	if s.hardwareStatus == nil || generation != s.hardwareStatusGen {
		return
	}
	s.hardwareStatus.SetText(text)
}

// setEncoderProbe records the probe verdicts and drops the encoders that
// refused the work from the capability map.
//
// From here on every question asked of s.ffmpeg -- which encoder to plan for,
// what to offer in the dropdown, what to actually run -- is answered from what
// this machine did, not from what the binary was compiled with.
func (s *appState) setEncoderProbe(report *common.EncoderProbeReport) {
	s.encoderProbe = report
	s.ffmpeg = common.ApplyEncoderProbe(s.ffmpeg, report)
}

// setEncoding moves the window between its idle and busy shapes.
//
// One list of controls, walked in both directions. Writing the two transitions
// as separate sequences of Enable/Disable calls is what let them drift apart
// before: the success path had lost the squeeze checkbox, which then stayed
// greyed out for the rest of the session.
func (s *appState) setEncoding(inProgress bool) {
	s.encoding = inProgress
	s.locked.setEnabled(!inProgress)

	if inProgress {
		if s.progress != nil {
			s.progress.SetValue(0)
		}
		if s.start != nil {
			s.start.Disable()
		}
		if s.cancelButton != nil {
			s.cancelButton.Enable()
		}
		return
	}

	if s.cancelButton != nil {
		s.cancelButton.Disable()
	}
	s.refreshStart()
}

// beginEncoding marks a conversion as started and returns the channel that
// stops it.
//
// The channel is *returned* rather than left for the caller to read back from
// the struct. The encoding goroutine must hold its own reference: reading
// s.cancel from inside it raced with requestCancel nulling the field on the UI
// thread, and a cancellation landing in that window handed the pipeline a nil
// channel -- a run that the Cancel button could no longer stop at all.
//
// The second channel goes the other way: the goroutine must close it when it
// has nothing left to unwind, and runEncoding is what does. shutdown waits on
// the receive side.
func (s *appState) beginEncoding() (cancel <-chan struct{}, done chan<- struct{}) {
	s.cancel = make(chan struct{})
	finished := make(chan struct{})
	s.done = finished
	s.setEncoding(true)
	return s.cancel, finished
}

// runEncoding is the body of the encoding goroutine: it runs encode, releases
// done, and hands the outcome to completeEncoding on the UI thread.
//
// done is closed *before* the completion is posted, not by a defer at the end.
// Two reasons, both about who would otherwise wait for whom:
//
//   - The close cannot happen inside the fyne.Do closure, because the one
//     waiting on it is the Fyne main thread, which is exactly the one that
//     would have to run that closure.
//   - Once Fyne's loop has drained, fyne.Do runs inline on this goroutine. A
//     system stop then takes completeEncoding to s.quit(), window.Close(), the
//     close hook, and shutdown -- here, on this goroutine, waiting on a done
//     that only this goroutine's deferred close would ever release: twenty
//     seconds of a process waiting for itself. Closed first, there is nothing
//     left to wait for, wherever shutdown is called from.
//
// Closing it early is honest: when encode returns, PerformEncoding has killed
// and reaped ffmpeg, removed the partial file and the session directory, which
// is everything done promises. The deferred release keeps the old guarantee
// that a panic cannot leave an exit waiting.
func (s *appState) runEncoding(done chan<- struct{}, output string, encode func() error) {
	var once sync.Once
	release := func() { once.Do(func() { close(done) }) }
	defer release()

	err := encode()
	release()
	fyne.Do(func() { s.completeEncoding(err, output) })
}

// finishEncoding returns the window to its idle shape once a conversion ends,
// however it ended.
func (s *appState) finishEncoding() {
	s.cancel = nil
	s.done = nil
	s.setEncoding(false)
}

// completeEncoding is the single completion path, run on the UI thread once
// PerformEncoding has returned with err. The output path is for the message
// that says where the finished file is.
//
// Four outcomes, and they used to be two. Every error landed in the failure
// branch, so a user who pressed Cancel read "Status: Failed" and then had to
// dismiss an Error dialog saying "encoding interrupted by user" -- the message
// had been corrected to say that, the branch never was.
func (s *appState) completeEncoding(err error, output string) {
	// The window is on its way out: a fyne.Do posted after Fyne's loop has
	// drained runs this inline on the encoding goroutine, against widgets with
	// no canvas and a window with nothing to show a dialog in.
	if s.shuttingDown.Load() {
		return
	}

	// A system signal is an order, not a failure to report. systemd stops
	// the whole application unit when the kernel's OOM killer takes one of
	// its processes, and holding the window open at that point only delays
	// the SIGKILL that follows -- which would leave the working file and the
	// session directory behind. PerformEncoding has already unwound both by
	// the time it returns here, so the one thing left is to obey.
	if errors.Is(err, common.ErrStoppedBySignal) {
		common.GetLogger().Error("Quitting: the system asked the application to stop",
			slog.String("error", err.Error()),
		)
		if s.quit != nil {
			s.quit()
		}
		return
	}

	if s.hardwareStatus != nil {
		s.hardwareStatus.SetText(common.GetLastHardwareAccelerationSummary())
	}
	s.finishEncoding()

	switch {
	case err == nil:
		s.setStatusAndResults("Status: Completed", formatResultsPanel(common.GetLastEncodingMetrics()))
		if s.progress != nil {
			s.progress.SetValue(1)
		}
		if s.window != nil {
			dialog.ShowInformation("Done", "Transform complete. Output file:\n"+output, s.window)
		}
	case errors.Is(err, common.ErrCancelled):
		// The user asked for this. There is nothing to report, and certainly
		// not an error.
		s.setStatusAndResults("Status: Cancelled", "Results: last run cancelled")
	default:
		s.setStatusAndResults("Status: Failed", "Results: last run failed")
		if s.window != nil && !showPrerequisiteDialog(s.window, err, nil) {
			dialog.ShowError(err, s.window)
		}
	}
}

func (s *appState) setStatusAndResults(status, results string) {
	if s.status != nil {
		s.status.SetText(status)
	}
	if s.results != nil {
		s.results.SetText(results)
	}
}

// shutdown is what every exit goes through: it asks a running conversion to
// stop and waits, within shutdownGrace, for its goroutine to have unwound --
// ffmpeg killed and reaped, the partial file removed, the session directory
// gone. Returns whether it got there, and is safe to call any number of times.
//
// Three exits, one method. The Quit button used to call app.Quit() straight
// away, so the window vanished while ffmpeg kept encoding into the working
// file until the end; the close intercept slept one second and hoped; and
// Fyne's own SIGINT/SIGTERM handler quit its loop with no regard for either.
// Two hooks route all of them here: window.SetOnClosed, which Fyne runs
// synchronously inside window.Close() and so inside app.Quit(); and the line
// after ShowAndRun() in main(), because Fyne's signal handler stops the driver
// without closing any window. Verified against fyne v2.8.2:
// internal/driver/glfw/window.go (Close), app/app.go (Quit) and
// internal/driver/glfw/driver_desktop.go (catchTerm).
//
// It blocks the Fyne main thread, and that is safe: the goroutine never needs
// that thread to finish. Its fyne.Do calls enqueue on an unbounded queue and
// return at once, and the channel it closes is closed by the goroutine
// itself, not by one of those closures -- and before the completion is
// posted, so a shutdown reached from that completion finds it closed (see
// runEncoding).
//
// The wait happens once. After it has run out, every later call returns false
// at once: the close hook and the line after ShowAndRun() would otherwise each
// spend the full grace period.
func (s *appState) shutdown(reason string) bool {
	s.shuttingDown.Store(true)
	done := s.done
	if done == nil {
		return true
	}
	select {
	case <-done:
		return true
	default:
	}
	if s.shutdownGaveUp.Load() {
		return false
	}

	s.requestCancel()

	logger := common.GetLogger()
	logger.Info("Waiting for the conversion to unwind before quitting", slog.String("reason", reason))

	grace := s.shutdownGrace
	if grace <= 0 {
		grace = shutdownGrace
	}
	timer := time.NewTimer(grace)
	defer timer.Stop()

	select {
	case <-done:
		logger.Info("Conversion unwound, quitting", slog.String("reason", reason))
		return true
	case <-timer.C:
		s.shutdownGaveUp.Store(true)
		logger.Error("Quitting without waiting any longer: the conversion did not unwind in time",
			slog.String("reason", reason),
			slog.Duration("waited", grace),
		)
		return false
	}
}

// refreshEncoderOptions rebuilds the codec dropdown from the current encoder
// list: at startup, after the probe has dropped the encoders that refused to
// work, and after a successful ffmpeg retry takes the list from empty to
// whatever the freshly installed binary exposes.
//
// The selection it ends on is the saved preference when offered, else the
// current one when still offered, else the first option. The preference comes
// first because it is the user's last choice by construction -- every click
// saves it -- while the current selection may be a fallback the program made:
// a start without ffmpeg leaves "Use same video codec...", and a retry that
// then finds the saved hevc_nvenc used to keep the fallback.
// Whatever it picks is set under restoringSelection, so the dropdown's
// OnChanged does not persist a fallback as if the user had chosen it.
func (s *appState) refreshEncoderOptions() {
	if s.encoder == nil {
		return
	}
	options := encoderOptionsFor(s.ffmpeg["encoders"])
	s.encoder.Options = options

	wanted := ""
	if s.prefs != nil {
		wanted = s.prefs.String(prefEncoderSelection)
	}
	if !containsString(options, wanted) {
		wanted = s.encoder.Selected
	}
	if !containsString(options, wanted) {
		wanted = options[0]
	}

	s.restoringSelection = true
	s.encoder.SetSelected(wanted)
	s.restoringSelection = false
	s.encoder.Refresh()
}

// encoderChanged is the codec dropdown's OnChanged. The preference is saved
// only for a choice the user made; a selection the program restored is not
// one, however much the widget fires the same callback for both.
func (s *appState) encoderChanged(selection string) {
	if !s.restoringSelection && s.prefs != nil {
		s.prefs.SetString(prefEncoderSelection, selection)
	}
	s.refreshHardwareStatus()
	s.verifyHardwareInBackground()
}

// requestCancel stops a running conversion. It is the single place allowed to
// close the channel, and nulls the field in the same breath, so calling it
// twice -- the Cancel button and then the window close intercept, say -- is
// harmless rather than a panic.
//
// Returns whether it actually cancelled anything.
func (s *appState) requestCancel() bool {
	if !s.encoding || s.cancel == nil {
		return false
	}
	close(s.cancel)
	s.cancel = nil
	if s.status != nil {
		s.status.SetText("Status: Cancelling...")
	}
	return true
}

// loadStartupConfig loads the configuration and never returns nil.
//
// A file LoadConfig refuses -- an unknown key, min_bitrate above max_bitrate --
// is logged with LoadConfig's own words and the environment alone is tried; when
// the environment is what is inconsistent, even that fails, and the built-in
// defaults apply. The second LoadConfig used to have its error discarded, which
// was harmless only while LoadConfig("") could not fail.
func loadStartupConfig(configPath string, log *slog.Logger) *common.Config {
	cfg, err := common.LoadConfig(configPath)
	if err == nil {
		return cfg
	}
	log.Error("Failed to load configuration, ignoring the file",
		slog.String("path", configPath), slog.String("error", err.Error()))
	if cfg, err = common.LoadConfig(""); err == nil {
		return cfg
	}
	log.Error("Environment configuration rejected, using the built-in defaults",
		slog.String("error", err.Error()))
	return common.DefaultConfig()
}

func main() {
	// Every piece of session state lives here, so the transitions that read and
	// write it are methods a test can call rather than closures over locals.
	state := &appState{ffmpegAvailable: true}
	var encoder *widget.Select

	// Diagnostics go to a capped log file under the user cache directory, not
	// to stdout (there is no console) and not to io.Discard (which made every
	// failure unreportable). Falls back to discarding if the file cannot be
	// opened -- never block startup over logging.
	logWriter := io.Discard
	logPath := ""
	if logFile, path, logErr := common.OpenLogFile("superview", maxLogFileBytes); logErr == nil {
		logWriter = logFile
		logPath = path
		defer func() { _ = logFile.Close() }()
	}

	gui_logger := slog.New(slog.NewTextHandler(logWriter, &slog.HandlerOptions{
		Level: slog.LevelInfo,
	}))
	common.SetLogger(gui_logger)
	common.SetObservabilityHandler(common.NewDefaultObservabilityHandler(gui_logger))
	if logPath != "" {
		gui_logger.Info("Superview starting", slog.String("log_file", logPath))
	}

	// Load configuration (from superview.yaml or env vars).
	// The path is resolved against the executable directory and the per-user
	// config directory, not just the working directory, which is arbitrary when
	// the app is started from a desktop launcher.
	configPath := common.ResolveConfigPath()
	cfg := loadStartupConfig(configPath, gui_logger)

	// Re-create the logger now that the configured level is known.
	gui_logger = slog.New(slog.NewTextHandler(logWriter, &slog.HandlerOptions{
		Level: common.ParseLogLevel(cfg.LogLevel),
	}))
	common.SetLogger(gui_logger)
	common.SetObservabilityHandler(common.NewDefaultObservabilityHandler(gui_logger))
	gui_logger.Info("Configuration resolved",
		slog.String("config_file", configPath),
		slog.String("log_level", cfg.LogLevel),
	)

	app := app.NewWithID("com.canaill51.superview")
	iconResource := fyne.NewStaticResource("Icon.png", appIconPNG)
	app.SetIcon(iconResource)
	app.Settings().SetTheme(newForcedDarkTheme())

	// Nothing used to say which binary was running -- not the window, not the
	// log, not the diagnostic the README asks users to attach to bug reports.
	// That was tolerable while a single release existed; three now do, their
	// outputs differ in size, and one carries the squeeze seam.
	buildInfo, haveBuildInfo := debug.ReadBuildInfo()
	identity := buildIdentity(app.Metadata(), buildInfo, haveBuildInfo)
	gui_logger.Info("Superview build", slog.String("build", identity))

	window := app.NewWindow("Superview " + identity)
	window.SetIcon(iconResource)
	prefs := app.Preferences()
	state.window = window
	state.prefs = prefs
	state.quit = app.Quit

	// Every way out of the application waits for a running conversion to
	// unwind -- see appState.shutdown for the three paths and why two hooks
	// are needed. This one covers app.Quit(): Fyne calls it synchronously from
	// window.Close(), before the window is torn down.
	window.SetOnClosed(func() {
		state.shutdown("window closing")
	})

	// confirmQuit is the one way to leave from inside the window: the system's
	// close request and the Quit button both come here. During a conversion it
	// asks first; the Quit button used to skip the question and the wait both.
	confirmQuit := func() {
		if !state.isEncoding() {
			app.Quit()
			return
		}
		dialog.NewCustomConfirm(
			"Cancel and quit",
			"Yes",
			"No",
			widget.NewLabel("An encoding is in progress. Do you want to cancel and quit?"),
			func(confirm bool) {
				if confirm {
					// app.Quit closes the window, and closing it is what
					// cancels the conversion and waits for it (SetOnClosed).
					app.Quit()
				}
				// Otherwise do nothing: the user withdrew the close.
			},
			window,
		).Show()
	}
	window.SetCloseIntercept(confirmQuit)

	title := widget.NewLabel("Superview")
	title.Alignment = fyne.TextAlignCenter
	title.TextStyle = fyne.TextStyle{Bold: true}

	subtitle := widget.NewLabel("Convert 4:3 to 16:9 in one workflow")
	subtitle.Alignment = fyne.TextAlignCenter
	subtitle.Wrapping = fyne.TextWrapWord

	selectedFile := widget.NewLabel("No file selected")
	selectedFile.Alignment = fyne.TextAlignLeading
	selectedFile.Wrapping = fyne.TextWrapWord

	selectedOutput := widget.NewLabel("No file selected")
	selectedOutput.Alignment = fyne.TextAlignLeading
	selectedOutput.Wrapping = fyne.TextWrapWord
	state.inputLabel = selectedFile
	state.outputLabel = selectedOutput

	state.qualityProfile = prefs.String(prefQualityProfile)
	if !containsString([]string{"Fast", "Balanced"}, state.qualityProfile) {
		state.qualityProfile = "Balanced"
	}

	status := widget.NewLabel("Status: Ready")
	status.Alignment = fyne.TextAlignLeading
	status.TextStyle = fyne.TextStyle{Bold: true}
	status.Wrapping = fyne.TextWrapWord
	state.status = status
	results := widget.NewLabel("Results: no completed run yet")
	results.Alignment = fyne.TextAlignLeading
	results.Wrapping = fyne.TextWrapWord
	state.results = results
	hardwareStatus := widget.NewLabel("Hardware: waiting for input video")
	hardwareStatus.Alignment = fyne.TextAlignLeading
	hardwareStatus.Wrapping = fyne.TextWrapWord
	state.hardwareStatus = hardwareStatus
	progressBar := widget.NewProgressBar()
	progressBar.SetValue(0)
	state.progress = progressBar
	qualityProfileSelect := widget.NewSelect([]string{"Fast", "Balanced"}, func(s string) {
		state.qualityProfile = s
		prefs.SetString(prefQualityProfile, s)
	})
	qualityProfileSelect.Alignment = fyne.TextAlignCenter
	qualityProfileSelect.SetSelected(state.qualityProfile)
	qualityProfileLabel := widget.NewLabel("Quality")
	qualityProfileLabel.Alignment = fyne.TextAlignLeading

	// Squeeze mode: the source already holds a 4:3 capture stretched to 16:9,
	// so GeneratePGM un-stretches the centre instead of widening the frame.
	//
	// The label used to name GoPro SuperView specifically. That promised more
	// than the code delivers: upstream documents this option for cameras like
	// the Caddx Tarsier and states plainly that the algorithm is not a 1:1 copy
	// of GoPro's. The curve is an approximation of the inverse stretch, useful
	// for any camera that stores a stretched 4:3 frame -- which does include
	// GoPro's SuperView modes, just not with GoPro's own maths.
	squeezeCheck := widget.NewCheck("Source already stretched to 16:9 (un-squeeze)", func(checked bool) {
		state.squeeze = checked
		prefs.SetBool(prefSqueezeSource, checked)
		// The output frame changes width with this box, and a hardware encoder
		// refuses on width. The line has to be recomputed, not just kept.
		state.refreshHardwareStatus()
		state.verifyHardwareInBackground()
	})
	state.squeeze = prefs.Bool(prefSqueezeSource)
	squeezeCheck.SetChecked(state.squeeze)

	var open *widget.Button
	var selectOutput *widget.Button
	var cancel *widget.Button
	var start *widget.Button

	// Declared up front because the retry callback below closes over it; it is
	// assigned once the encoder dropdown exists, before any callback can run.
	var startEncoderProbe func()

	start = widget.NewButtonWithIcon("Start transformation", theme.MediaPlayIcon(), func() {
		if state.video == nil {
			dialog.ShowInformation("No input", "Please open an input video first.", window)
			return
		}
		if state.outputPath == "" {
			dialog.ShowInformation("No output", "Please choose an output file first.", window)
			return
		}

		startEncoding := func(uri string) {
			effectiveCfg := *cfg
			video := state.video

			// GPU-friendly quality strategy: keep hardware encoders and scale bitrate by profile.
			inputBitrate := video.Streams[0].BitrateInt
			profileBitrate, profilePreset := qualityProfileSettings(state.qualityProfile, inputBitrate)

			// An explicit video_preset in the configuration wins over the
			// profile's. Previously the profile always overwrote it, so a user
			// who set video_preset in superview.yaml silently got "fast" or
			// "medium" instead, with nothing reporting the substitution.
			if effectiveCfg.VideoPreset == "" {
				effectiveCfg.VideoPreset = profilePreset
			}

			if profileBitrate < cfg.MinBitrate {
				profileBitrate = cfg.MinBitrate
			}
			if cfg.MaxBitrate > 0 && profileBitrate > cfg.MaxBitrate {
				profileBitrate = cfg.MaxBitrate
			}

			// Not "Transforming... 0%": ffmpeg produces no progress figure at all
			// until the first frame leaves the filter graph, and on a large frame
			// that takes seconds to a minute. One user watched a bar sit at zero for
			// 81 seconds and cancelled a conversion that was working.
			status.SetText(startingEncoderStatus)

			// Everything the goroutine will be asked for is read here, on the
			// UI thread, while the widgets are still the user's to change. The
			// dropdown in particular: its Selected is rewritten on this thread
			// when a probe lands, and was read from the goroutine until E-15.
			encoderName := common.ParseEncoderSelection(encoder.Selected)
			squeeze := state.squeeze

			// beginEncoding hands back the channels rather than leaving the
			// goroutine to read them off the struct, which is what used to race
			// with a cancellation nulling the field on the UI thread.
			cancelChannel, done := state.beginEncoding()
			ffmpeg := state.ffmpeg

			go state.runEncoding(done, uri, func() error {
				handler := &GUIHandler{
					window:      window,
					bitrate:     profileBitrate,
					encoderName: encoderName,
					squeeze:     squeeze,
					inlineBar:   progressBar,
					status:      status,
					ffmpeg:      ffmpeg,
					video:       video,
					logger:      common.GetLogger(),
				}

				return common.PerformEncoding(&effectiveCfg, video.File, uri, handler, ffmpeg, cancelChannel)
			})
		}

		if _, statErr := os.Stat(state.outputPath); statErr == nil {
			dialog.NewCustomConfirm(
				"Overwrite output file",
				"Yes",
				"No",
				widget.NewLabel("The selected output file already exists. Overwrite it?"),
				func(confirm bool) {
					if confirm {
						startEncoding(state.outputPath)
					}
				},
				window,
			).Show()
			return
		} else if !os.IsNotExist(statErr) {
			dialog.ShowError(fmt.Errorf("cannot access output file: %w", statErr), window)
			return
		}

		startEncoding(state.outputPath)

	})
	start.Disable()
	state.start = start
	cancel = widget.NewButtonWithIcon("Cancel", theme.CancelIcon(), func() {
		state.requestCancel()
	})
	cancel.Disable()
	state.cancelButton = cancel

	// The native pickers run off the UI thread. zenity or PowerShell holds the
	// call for as long as its dialog is open, and on the UI thread that froze
	// the window: no repaint, no answer to the compositor's pings, and on
	// Windows one to two seconds of a dead button before the picker even
	// appeared. The button is held down meanwhile so a second click cannot
	// open a second picker, and comes back only if nothing has locked it since.
	// The rest of the window stays live, so the result can arrive after Start:
	// applyChosenInput and applyChosenOutput drop it then.
	open = widget.NewButtonWithIcon("Choose input file", theme.FolderOpenIcon(), func() {
		open.Disable()
		go func() {
			uri, err := chooseInputFileNative()
			fyne.Do(func() {
				if !state.isEncoding() {
					open.Enable()
				}
				if err != nil {
					fd := dialog.NewFileOpen(func(file fyne.URIReadCloser, err error) {
						if err == nil && file == nil {
							common.GetLogger().Debug("File opening cancelled by user")
							return
						}
						if err != nil {
							dialog.ShowError(err, window)
							return
						}

						// URI().Path() is the documented accessor; it yields "/home/u/a.mp4"
						// on Unix and "C:/Users/u/a.mp4" on Windows, both usable as-is.
						fallbackURI := file.URI().Path()
						err = file.Close()
						if err != nil {
							fyne.LogError("Failed to close stream", err)
						}
						state.applyChosenInput(fallbackURI)
					}, window)
					// MP4 only, in and out. The wider list this replaced advertised ten
					// containers, five of which CheckVideo rejects outright because
					// Matroska, WebM, FLV, MPEG-PS and ASF carry no per-stream duration
					// or bit_rate -- users got an unreadable strconv error instead.
					fd.SetFilter(storage.NewExtensionFileFilter(supportedInputExtensions))
					fd.Show()
					return
				}
				if uri == "" {
					common.GetLogger().Debug("File opening cancelled by user")
					return
				}
				state.applyChosenInput(uri)
			})
		}()
	})

	selectOutput = widget.NewButtonWithIcon("Choose output file", theme.DocumentSaveIcon(), func() {
		selectOutput.Disable()
		go func() {
			uri, err := chooseOutputFileNative()
			fyne.Do(func() {
				if !state.isEncoding() {
					selectOutput.Enable()
				}
				if err != nil {
					dialog.ShowFileSave(func(file fyne.URIWriteCloser, err error) {
						if err == nil && file == nil {
							common.GetLogger().Debug("File saving cancelled by user")
							return
						}
						if err != nil {
							dialog.ShowError(err, window)
							return
						}

						path := file.URI().Path()
						err = file.Close()
						if err != nil {
							fyne.LogError("Failed to close stream", err)
						}
						state.applyChosenOutput(path)
					}, window)
					return
				}
				if uri == "" {
					common.GetLogger().Debug("File saving cancelled by user")
					return
				}
				state.applyChosenOutput(uri)
			})
		}()
	})

	probedFFmpeg, err := common.CheckFfmpeg(cfg)
	if err != nil {
		state.ffmpegAvailable = false

		// Re-probe on demand. Failed lookups are deliberately not cached, so
		// this picks up an ffmpeg installed while the app was already running.
		var retryFFmpeg func()
		retryFFmpeg = func() {
			common.ResetToolResolutionCache()
			probed, probeErr := common.CheckFfmpeg(cfg)
			if probeErr != nil {
				common.GetLogger().Warn("ffmpeg still unavailable after retry",
					slog.String("error", probeErr.Error()))
				if !showPrerequisiteDialog(window, probeErr, retryFFmpeg) {
					dialog.ShowError(probeErr, window)
				}
				return
			}

			state.setFFmpeg(probed)
			common.GetLogger().Info("ffmpeg found on retry",
				slog.String("version", probed["version"]))

			state.refreshEncoderOptions()
			startEncoderProbe()
			// The same list the startup path disabled, computed by the same
			// function.
			state.applyFFmpegAvailability()
			status.SetText("Status: Ready")
			state.refreshHardwareStatus()
			state.refreshStart()
		}

		if !showPrerequisiteDialog(window, err, retryFFmpeg) {
			dialog.ShowError(err, window)
		}
		status.SetText("Status: ffmpeg unavailable")
		hardwareStatus.SetText("Hardware: ffmpeg unavailable")
	} else {
		state.setFFmpeg(probedFFmpeg)
	}

	encoder = widget.NewSelect(nil, state.encoderChanged)
	state.encoder = encoder
	encoder.Alignment = fyne.TextAlignCenter
	// Fills the options and restores the saved selection, under the flag that
	// keeps a fallback from being saved over it.
	state.refreshEncoderOptions()

	// System diagnostics: ffmpeg/ffprobe availability, free disk, memory, CPU.
	// Runs off the UI thread because the checks shell out to ffmpeg.
	//
	// Built here, before the locked list, because it belongs in it: CheckHealth
	// probes every hardware encoder, and two probes on one GPU beside a running
	// hardware encode can make a working encoder fail -- so the report a user
	// was asked to attach could show a functional NVENC as refused.
	diagnosticBtn := widget.NewButtonWithIcon("Diagnostic", theme.InfoIcon(), func() {
		const runningDiagnostic = "Status: Running diagnostic..."
		statusBefore := status.Text
		status.SetText(runningDiagnostic)
		go func() {
			health := common.CheckHealth(cfg)
			// The build goes first: it is the one line that tells a maintainer
			// which binary produced everything below it.
			report := "Superview build: " + identity + "\n\n" + common.GetHealthReport(health)
			// The log path used to be written only *into* the log, so a user
			// asked to attach their logs to a bug report had no way to find
			// them. This dialog is where they already come looking.
			if logPath != "" {
				report += "\nLog file: " + logPath + "\n"
			} else {
				report += "\nLog file: unavailable (diagnostics are being discarded)\n"
			}
			// Mirror the report into the log file so a user can attach it to a
			// bug report without retyping what the dialog showed.
			common.LogHealth(common.GetLogger(), health)

			fyne.Do(func() {
				// Built here, on the UI thread, like every widget. Under
				// migrated_fynedo -- which fyne package sets now that
				// FyneApp.toml enables fyneDo -- Fyne's internal locks are
				// no-ops, and a widget touched off the UI thread is a race.
				reportLabel := widget.NewLabel(report)
				reportLabel.TextStyle = fyne.TextStyle{Monospace: true}
				reportLabel.Wrapping = fyne.TextWrapWord

				// The line goes back to what it said before, and only if
				// nothing has written over "Running diagnostic..." since: a
				// conversion started meanwhile owns it, and without ffmpeg a
				// blanket "Ready" would contradict the report in the dialog.
				if status.Text == runningDiagnostic {
					status.SetText(statusBefore)
				}
				content := container.NewVScroll(reportLabel)
				content.SetMinSize(fyne.NewSize(520, 300))
				dialog.NewCustom("System diagnostic", "Close", content, window).Show()
			})
		}()
	})

	controls := windowControls{
		open:         open,
		selectOutput: selectOutput,
		diagnostic:   diagnosticBtn,
		quality:      qualityProfileSelect,
		encoder:      encoder,
		squeeze:      squeezeCheck,
	}
	state.locked = controls.locked()
	state.requiresFFmpeg = controls.requiresFFmpeg()

	// Nothing but Diagnostic is usable without ffmpeg, and Diagnostic is what
	// says so. This runs here rather than up in the probe's error branch
	// because the codec dropdown does not exist yet at that point -- which is
	// why it used to be left enabled among four disabled controls, the same
	// kind of hand-written sequence that produced P-01.
	if !state.ffmpegAvailable {
		state.applyFFmpegAvailability()
	}

	// Ask the encoders whether they actually work, off the UI thread.
	//
	// It takes a few hundred milliseconds per encoder -- far too long to spend
	// before the window appears, and far too important to skip: until it comes
	// back, everything known about this machine's hardware comes from a list
	// ffmpeg compiled in, which cannot see the driver.
	startEncoderProbe = func() {
		if !state.ffmpegAvailable {
			return
		}
		capabilities := state.ffmpeg
		go func() {
			report := common.ProbeHardwareSupport(context.Background(), capabilities)
			fyne.Do(func() {
				state.setEncoderProbe(report)
				state.refreshEncoderOptions()
				state.refreshHardwareStatus()
				state.verifyHardwareInBackground()
				state.refreshStart()
			})
		}()
	}
	startEncoderProbe()

	codecLabel := widget.NewLabel("Video codec")
	codecLabel.Alignment = fyne.TextAlignLeading

	header := container.NewVBox(title, subtitle)

	quitBtn := widget.NewButton("Quit", confirmQuit)
	toolbar := newActionToolbar(open, selectOutput, start, cancel, diagnosticBtn, quitBtn)

	sourceForm := widget.NewForm(
		widget.NewFormItem("Input file", selectedFile),
		widget.NewFormItem("Output file", selectedOutput),
	)

	optionsForm := widget.NewForm(
		widget.NewFormItem("Quality profile", qualityProfileSelect),
		widget.NewFormItem("Video codec", encoder),
		widget.NewFormItem("Input format", squeezeCheck),
	)

	leftPanel := container.NewVBox(
		sourceForm,
		optionsForm,
		status,
		results,
		hardwareStatus,
	)

	bottomBar := container.NewBorder(
		nil,
		nil,
		widget.NewLabel("Progress"),
		nil,
		progressBar,
	)

	mainContent := container.NewVBox(
		toolbar,
		leftPanel,
		bottomBar,
	)

	content := container.NewBorder(
		header,
		nil,
		nil,
		nil,
		mainContent,
	)

	window.SetContent(content)

	// The window cannot be resized by the user, so it has to be at least as
	// big as what it holds: a longer label on any button would otherwise leave
	// part of the toolbar outside the frame, with nothing on screen to say so.
	window.Resize(fyne.NewSize(windowWidth, windowHeight).Max(content.MinSize()))
	window.SetFixedSize(true)

	window.ShowAndRun()

	// The second hook, for Fyne's own SIGINT/SIGTERM handler: it stops the
	// driver without closing any window, so SetOnClosed never ran, Run()
	// returned, and main() used to return with it -- while EncodeVideo was
	// still killing ffmpeg and PerformEncoding still removing the partial
	// file. Idempotent: after a Quit or a window close there is nothing left
	// to wait for and this returns at once.
	state.shutdown("event loop stopped")
}

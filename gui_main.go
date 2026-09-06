package main

import (
	_ "embed"
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

const requirementsURL = "https://github.com/Canaill51/superview?tab=readme-ov-file#requirements"

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

	parsedURL, parseErr := url.Parse(requirementsURL)
	if parseErr != nil {
		dialog.ShowError(err, window)
		return true
	}

	content := container.NewVBox(
		widget.NewLabel("cannot find ffmpeg/ffprobe on your system"),
		widget.NewLabel("make sure to install it first:"),
		widget.NewHyperlink(requirementsURL, parsedURL),
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

// formatEncodingStatus builds the line shown while a conversion runs.
//
// The estimate is left out entirely until there is one. Rendering a zero as
// "about 0s left" would read as "nearly done" at the very moment the encode is
// starting, which is the opposite of the truth.
func formatEncodingStatus(percent float64, remaining time.Duration) string {
	if remaining <= 0 {
		return fmt.Sprintf("Status: Transforming... %.0f%%", percent)
	}
	return fmt.Sprintf("Status: Transforming... %.0f%% - about %s left",
		percent, remaining.Round(time.Second))
}

// GUIHandler implements UIHandler for GUI interface
type GUIHandler struct {
	window    fyne.Window
	bitrate   int
	encoder   *widget.Select
	squeeze   bool
	inlineBar *widget.ProgressBar
	status    *widget.Label
	ffmpeg    map[string]string
	video     *common.VideoSpecs
	logger    *slog.Logger
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
	return common.ParseEncoderSelection(h.encoder.Selected)
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

	// Encoding lifecycle. cancel is non-nil exactly while a conversion is
	// running and has not been cancelled yet; requestCancel is the only place
	// allowed to close it, because closing a channel twice panics.
	encoding bool
	cancel   chan struct{}

	// Widgets the transitions drive.
	//
	// Each is nil-checked rather than assumed: they are assigned as main()
	// builds the window, so a reordering there would otherwise turn a missing
	// assignment into a crash instead of a missing update. It also lets a test
	// build only the part of the state machine it is exercising.
	start          *widget.Button
	cancelButton   *widget.Button
	encoder        *widget.Select
	locked         encodingControls
	status         *widget.Label
	hardwareStatus *widget.Label
	progress       *widget.ProgressBar
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

	selection := ""
	if s.encoder != nil {
		selection = common.ParseEncoderSelection(s.encoder.Selected)
	}
	s.hardwareStatus.SetText(common.DescribeHardwareAccelerationPlan(s.ffmpeg, s.video, selection))
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
func (s *appState) beginEncoding() <-chan struct{} {
	s.cancel = make(chan struct{})
	s.setEncoding(true)
	return s.cancel
}

// finishEncoding returns the window to its idle shape once a conversion ends,
// however it ended.
func (s *appState) finishEncoding() {
	s.cancel = nil
	s.setEncoding(false)
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
	common.RegisterObservabilityHandler(common.NewDefaultObservabilityHandler(gui_logger))
	if logPath != "" {
		gui_logger.Info("Superview starting", slog.String("log_file", logPath))
	}

	// Load configuration (from superview.yaml or env vars).
	// The path is resolved against the executable directory and the per-user
	// config directory, not just the working directory, which is arbitrary when
	// the app is started from a desktop launcher.
	configPath := common.ResolveConfigPath()
	cfg, err := common.LoadConfig(configPath)
	if err != nil {
		gui_logger.Error("Failed to load configuration", slog.String("error", err.Error()))
		// Continue with current/default configuration to avoid nil dereference.
		cfg, _ = common.LoadConfig("")
	}

	// Re-create the logger now that the configured level is known.
	gui_logger = slog.New(slog.NewTextHandler(logWriter, &slog.HandlerOptions{
		Level: common.ParseLogLevel(cfg.LogLevel),
	}))
	common.SetLogger(gui_logger)
	common.RegisterObservabilityHandler(common.NewDefaultObservabilityHandler(gui_logger))
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

	// Interception de la fermeture de la fenêtre principale
	window.SetCloseIntercept(func() {
		if state.isEncoding() {
			dialog.NewCustomConfirm(
				"Cancel and quit",
				"Yes",
				"No",
				widget.NewLabel("An encoding is in progress. Do you want to cancel and quit?"),
				func(confirm bool) {
					if confirm {
						state.requestCancel()
						// Let the encoding goroutine clean up, then quit.
						go func() {
							time.Sleep(1 * time.Second)
							app.Quit()
						}()
					}
					// Sinon, ne rien faire (l'utilisateur a annulé la fermeture)
				},
				window,
			).Show()
		} else {
			app.Quit()
		}
	})

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

	selectedQualityProfile := prefs.String(prefQualityProfile)
	if !containsString([]string{"Fast", "Balanced"}, selectedQualityProfile) {
		selectedQualityProfile = "Balanced"
	}

	status := widget.NewLabel("Status: Ready")
	status.Alignment = fyne.TextAlignLeading
	status.TextStyle = fyne.TextStyle{Bold: true}
	status.Wrapping = fyne.TextWrapWord
	state.status = status
	results := widget.NewLabel("Results: no completed run yet")
	results.Alignment = fyne.TextAlignLeading
	results.Wrapping = fyne.TextWrapWord
	hardwareStatus := widget.NewLabel("Hardware: waiting for input video")
	hardwareStatus.Alignment = fyne.TextAlignLeading
	hardwareStatus.Wrapping = fyne.TextWrapWord
	state.hardwareStatus = hardwareStatus
	progressBar := widget.NewProgressBar()
	progressBar.SetValue(0)
	state.progress = progressBar
	qualityProfileSelect := widget.NewSelect([]string{"Fast", "Balanced"}, func(s string) {
		selectedQualityProfile = s
		prefs.SetString(prefQualityProfile, s)
	})
	qualityProfileSelect.Alignment = fyne.TextAlignCenter
	qualityProfileSelect.SetSelected(selectedQualityProfile)
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
	squeezeSource := prefs.Bool(prefSqueezeSource)
	squeezeCheck := widget.NewCheck("Source already stretched to 16:9 (un-squeeze)", func(checked bool) {
		squeezeSource = checked
		prefs.SetBool(prefSqueezeSource, checked)
	})
	squeezeCheck.SetChecked(squeezeSource)

	var open *widget.Button
	var selectOutput *widget.Button
	var cancel *widget.Button
	var start *widget.Button

	// Declared up front because the button callbacks below close over it; it is
	// assigned once the encoder dropdown exists, before any callback can run.
	var refreshEncoderOptions func()

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
			profileBitrate, profilePreset := qualityProfileSettings(selectedQualityProfile, inputBitrate)

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

			status.SetText("Status: Transforming...")

			// beginEncoding hands back the channel rather than leaving the
			// goroutine to read it off the struct, which is what used to race
			// with a cancellation nulling the field on the UI thread.
			cancelChannel := state.beginEncoding()
			ffmpeg := state.ffmpeg

			go func() {
				handler := &GUIHandler{
					window:    window,
					bitrate:   profileBitrate,
					encoder:   encoder,
					squeeze:   squeezeSource,
					inlineBar: progressBar,
					status:    status,
					ffmpeg:    ffmpeg,
					video:     video,
					logger:    common.GetLogger(),
				}

				if err := common.PerformEncoding(&effectiveCfg, video.File, uri, handler, ffmpeg, cancelChannel); err != nil {
					fyne.Do(func() {
						status.SetText("Status: Failed")
						results.SetText("Results: last run failed")
						hardwareStatus.SetText(common.GetLastHardwareAccelerationSummary())
						state.finishEncoding()
					})
					handler.ShowError(err)
					return
				}

				lastMetrics := common.GetLastEncodingMetrics()
				resultsText := formatResultsPanel(lastMetrics)
				fyne.Do(func() {
					status.SetText("Status: Completed")
					results.SetText(resultsText)
					hardwareStatus.SetText(common.GetLastHardwareAccelerationSummary())
					state.finishEncoding()
					progressBar.SetValue(1)
				})
				handler.ShowInfo("Transform complete. Output file:\n" + uri)
			}()
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

	open = widget.NewButtonWithIcon("Choose input file", theme.FolderOpenIcon(), func() {
		uri, err := chooseInputFileNative()
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

				video, err := common.CheckVideo(fallbackURI)
				if err != nil {
					status.SetText("Status: Invalid input")
					dialog.ShowError(err, window)
					return
				}
				state.setInput(video)
				selectedFile.SetText(filepath.Base(video.File))
				status.SetText("Status: Input loaded")
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

		video, err := common.CheckVideo(uri)
		if err != nil {
			status.SetText("Status: Invalid input")
			dialog.ShowError(err, window)
			return
		}
		state.setInput(video)
		selectedFile.SetText(filepath.Base(video.File))
		status.SetText("Status: Input loaded")
	})

	selectOutput = widget.NewButtonWithIcon("Choose output file", theme.DocumentSaveIcon(), func() {
		uri, err := chooseOutputFileNative()
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
				state.setOutput(ensureMP4Extension(path))
				selectedOutput.SetText(filepath.Base(state.outputPath))
				status.SetText("Status: Output selected")
			}, window)
			return
		}
		if uri == "" {
			common.GetLogger().Debug("File saving cancelled by user")
			return
		}
		state.setOutput(ensureMP4Extension(uri))
		selectedOutput.SetText(filepath.Base(state.outputPath))
		status.SetText("Status: Output selected")
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

			refreshEncoderOptions()
			// The same list the encoding transition uses: a control that must
			// come back after ffmpeg appears is exactly one that must come back
			// after a conversion ends.
			state.locked.setEnabled(true)
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

	encoderOptions := encoderOptionsFor(state.ffmpeg["encoders"])
	encoder = widget.NewSelect(encoderOptions, func(selection string) {
		prefs.SetString(prefEncoderSelection, selection)
		state.refreshHardwareStatus()
	})
	state.encoder = encoder
	encoder.Alignment = fyne.TextAlignCenter
	savedEncoderSelection := prefs.String(prefEncoderSelection)
	if !containsString(encoderOptions, savedEncoderSelection) {
		savedEncoderSelection = encoderOptions[0]
	}
	encoder.SetSelected(savedEncoderSelection)

	// Rebuild the dropdown after a successful retry, when the encoder list goes
	// from empty to whatever the freshly installed ffmpeg exposes.
	refreshEncoderOptions = func() {
		options := encoderOptionsFor(state.ffmpeg["encoders"])
		encoder.Options = options
		previous := encoder.Selected
		if !containsString(options, previous) {
			previous = options[0]
		}
		encoder.SetSelected(previous)
		encoder.Refresh()
	}
	state.locked = encodingControls{open, selectOutput, qualityProfileSelect, squeezeCheck, encoder}

	// Nothing is usable without ffmpeg. This runs here rather than up in the
	// probe's error branch because the codec dropdown does not exist yet at
	// that point -- which is why it used to be left enabled among four disabled
	// controls, the same kind of hand-written sequence that produced P-01.
	if !state.ffmpegAvailable {
		state.locked.setEnabled(false)
	}

	codecLabel := widget.NewLabel("Video codec")
	codecLabel.Alignment = fyne.TextAlignLeading

	header := container.NewVBox(title, subtitle)

	// System diagnostics: ffmpeg/ffprobe availability, free disk, memory, CPU.
	// Runs off the UI thread because the checks shell out to ffmpeg.
	diagnosticBtn := widget.NewButtonWithIcon("Diagnostic", theme.InfoIcon(), func() {
		status.SetText("Status: Running diagnostic...")
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

			reportLabel := widget.NewLabel(report)
			reportLabel.TextStyle = fyne.TextStyle{Monospace: true}
			reportLabel.Wrapping = fyne.TextWrapWord

			fyne.Do(func() {
				status.SetText("Status: Ready")
				content := container.NewVScroll(reportLabel)
				content.SetMinSize(fyne.NewSize(520, 300))
				dialog.NewCustom("System diagnostic", "Close", content, window).Show()
			})
		}()
	})

	quitBtn := widget.NewButton("Quit", func() {
		app.Quit()
	})
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
}

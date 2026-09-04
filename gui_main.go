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

// qualityProfileSettings maps a GUI quality profile onto an output bitrate and
// an ffmpeg preset. The output is widened from 4:3 to 16:9, so "Balanced"
// raises the bitrate to keep the perceived quality of the source.
func qualityProfileSettings(profile string, inputBitrate int) (bitrate int, preset string) {
	switch profile {
	case "Fast":
		return inputBitrate, "fast"
	default: // "Balanced" and any unexpected value
		return int(float64(inputBitrate) * 1.6), "medium"
	}
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

// GUIHandler implements UIHandler for GUI interface
type GUIHandler struct {
	window    fyne.Window
	bitrate   int
	encoder   *widget.Select
	squeeze   bool
	inlineBar *widget.ProgressBar
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

func main() {
	var video *common.VideoSpecs
	var outputPath string
	var ffmpeg map[string]string
	var encoder *widget.Select

	// Nouveaux champs d'état pour l'annulation
	var encodingInProgress bool
	var cancelEncoding chan struct{}

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

	window := app.NewWindow("Superview")
	window.SetIcon(iconResource)
	prefs := app.Preferences()

	// Single idempotent cancellation path, shared by the Cancel button and the
	// window close intercept. Closing the channel twice panics, so the only
	// place allowed to close it sets it to nil in the same breath.
	// Assigned below, once the status label exists; every caller runs later.
	var requestCancel func()

	// Interception de la fermeture de la fenêtre principale
	window.SetCloseIntercept(func() {
		if encodingInProgress {
			dialog.NewCustomConfirm(
				"Cancel and quit",
				"Yes",
				"No",
				widget.NewLabel("An encoding is in progress. Do you want to cancel and quit?"),
				func(confirm bool) {
					if confirm {
						if requestCancel != nil {
							requestCancel()
						}
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
	results := widget.NewLabel("Results: no completed run yet")
	results.Alignment = fyne.TextAlignLeading
	results.Wrapping = fyne.TextWrapWord
	hardwareStatus := widget.NewLabel("Hardware: waiting for input video")
	hardwareStatus.Alignment = fyne.TextAlignLeading
	hardwareStatus.Wrapping = fyne.TextWrapWord
	progressBar := widget.NewProgressBar()
	progressBar.SetValue(0)
	qualityProfileSelect := widget.NewSelect([]string{"Fast", "Balanced"}, func(s string) {
		selectedQualityProfile = s
		prefs.SetString(prefQualityProfile, s)
	})
	qualityProfileSelect.Alignment = fyne.TextAlignCenter
	qualityProfileSelect.SetSelected(selectedQualityProfile)
	qualityProfileLabel := widget.NewLabel("Quality")
	qualityProfileLabel.Alignment = fyne.TextAlignLeading

	// Squeeze mode: the source is already stretched to 16:9 (GoPro's own
	// SuperView recording modes). GeneratePGM then un-stretches the centre
	// instead of widening the frame.
	squeezeSource := prefs.Bool(prefSqueezeSource)
	squeezeCheck := widget.NewCheck("Source already stretched (GoPro SuperView)", func(checked bool) {
		squeezeSource = checked
		prefs.SetBool(prefSqueezeSource, checked)
	})
	squeezeCheck.SetChecked(squeezeSource)

	var open *widget.Button
	var selectOutput *widget.Button
	var cancel *widget.Button
	var start *widget.Button

	ffmpegAvailable := true

	// Declared up front because the button callbacks below close over them;
	// both are assigned before any callback can run.
	var refreshStart func()
	var updateHardwareStatus func()
	var refreshEncoderOptions func()

	setEncodingState := func(inProgress bool) {
		encodingInProgress = inProgress
		if inProgress {
			progressBar.SetValue(0)
		}
	}

	requestCancel = func() {
		if !encodingInProgress || cancelEncoding == nil {
			return
		}
		close(cancelEncoding)
		cancelEncoding = nil
		status.SetText("Status: Cancelling...")
	}

	start = widget.NewButtonWithIcon("Start transformation", theme.MediaPlayIcon(), func() {
		if video == nil {
			dialog.ShowInformation("No input", "Please open an input video first.", window)
			return
		}
		if outputPath == "" {
			dialog.ShowInformation("No output", "Please choose an output file first.", window)
			return
		}

		startEncoding := func(uri string) {
			effectiveCfg := *cfg

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
			progressBar.SetValue(0)
			open.Disable()
			selectOutput.Disable()
			qualityProfileSelect.Disable()
			squeezeCheck.Disable()
			encoder.Disable()
			start.Disable()
			cancel.Enable()

			// Activation de l'état d'encodage et initialisation du canal d'annulation
			setEncodingState(true)
			cancelEncoding = make(chan struct{})

			go func() {
				handler := &GUIHandler{
					window:    window,
					bitrate:   profileBitrate,
					encoder:   encoder,
					squeeze:   squeezeSource,
					inlineBar: progressBar,
					ffmpeg:    ffmpeg,
					video:     video,
					logger:    common.GetLogger(),
				}

				if err := common.PerformEncoding(&effectiveCfg, video.File, uri, handler, ffmpeg, cancelEncoding); err != nil {
					fyne.Do(func() {
						status.SetText("Status: Failed")
						results.SetText("Results: last run failed")
						hardwareStatus.SetText(common.GetLastHardwareAccelerationSummary())
						setEncodingState(false)
						cancelEncoding = nil
						open.Enable()
						selectOutput.Enable()
						qualityProfileSelect.Enable()
						squeezeCheck.Enable()
						encoder.Enable()
						cancel.Disable()
						refreshStart()
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
					progressBar.SetValue(1)
					setEncodingState(false)
					cancelEncoding = nil
					open.Enable()
					selectOutput.Enable()
					qualityProfileSelect.Enable()
					encoder.Enable()
					cancel.Disable()
					refreshStart()
				})
				handler.ShowInfo("Transform complete. Output file:\n" + uri)
			}()
		}

		if _, statErr := os.Stat(outputPath); statErr == nil {
			dialog.NewCustomConfirm(
				"Overwrite output file",
				"Yes",
				"No",
				widget.NewLabel("The selected output file already exists. Overwrite it?"),
				func(confirm bool) {
					if confirm {
						startEncoding(outputPath)
					}
				},
				window,
			).Show()
			return
		} else if !os.IsNotExist(statErr) {
			dialog.ShowError(fmt.Errorf("cannot access output file: %w", statErr), window)
			return
		}

		startEncoding(outputPath)

	})
	start.Disable()
	cancel = widget.NewButtonWithIcon("Cancel", theme.CancelIcon(), func() {
		requestCancel()
	})
	cancel.Disable()

	refreshStart = func() {
		if video != nil && outputPath != "" && ffmpegAvailable && !encodingInProgress {
			start.Enable()
		} else {
			start.Disable()
		}
	}

	updateHardwareStatus = func() {
		if !ffmpegAvailable {
			hardwareStatus.SetText("Hardware: ffmpeg unavailable")
			return
		}

		hardwareStatus.SetText(common.DescribeHardwareAccelerationPlan(ffmpeg, video, common.ParseEncoderSelection(encoder.Selected)))
	}

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

				video, err = common.CheckVideo(fallbackURI)
				if err != nil {
					status.SetText("Status: Invalid input")
					dialog.ShowError(err, window)
					return
				}
				selectedFile.SetText(filepath.Base(video.File))
				status.SetText("Status: Input loaded")
				updateHardwareStatus()
				refreshStart()
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

		video, err = common.CheckVideo(uri)
		if err != nil {
			status.SetText("Status: Invalid input")
			dialog.ShowError(err, window)
			return
		}
		selectedFile.SetText(filepath.Base(video.File))
		status.SetText("Status: Input loaded")
		updateHardwareStatus()
		refreshStart()
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
				outputPath = ensureMP4Extension(path)
				selectedOutput.SetText(filepath.Base(outputPath))
				status.SetText("Status: Output selected")
				refreshStart()
			}, window)
			return
		}
		if uri == "" {
			common.GetLogger().Debug("File saving cancelled by user")
			return
		}
		outputPath = ensureMP4Extension(uri)
		selectedOutput.SetText(filepath.Base(outputPath))
		status.SetText("Status: Output selected")
		refreshStart()
	})

	ffmpeg, err = common.CheckFfmpeg(cfg)
	if err != nil {
		ffmpegAvailable = false

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

			ffmpeg = probed
			ffmpegAvailable = true
			common.GetLogger().Info("ffmpeg found on retry",
				slog.String("version", ffmpeg["version"]))

			refreshEncoderOptions()
			open.Enable()
			selectOutput.Enable()
			qualityProfileSelect.Enable()
			squeezeCheck.Enable()
			encoder.Enable()
			status.SetText("Status: Ready")
			updateHardwareStatus()
			refreshStart()
		}

		if !showPrerequisiteDialog(window, err, retryFFmpeg) {
			dialog.ShowError(err, window)
		}
		open.Disable()
		selectOutput.Disable()
		qualityProfileSelect.Disable()
		squeezeCheck.Disable()
		status.SetText("Status: ffmpeg unavailable")
		hardwareStatus.SetText("Hardware: ffmpeg unavailable")
	}

	encoderOptions := encoderOptionsFor(ffmpeg["encoders"])
	encoder = widget.NewSelect(encoderOptions, func(s string) {
		prefs.SetString(prefEncoderSelection, s)
		updateHardwareStatus()
	})
	encoder.Alignment = fyne.TextAlignCenter
	savedEncoderSelection := prefs.String(prefEncoderSelection)
	if !containsString(encoderOptions, savedEncoderSelection) {
		savedEncoderSelection = encoderOptions[0]
	}
	encoder.SetSelected(savedEncoderSelection)

	// Rebuild the dropdown after a successful retry, when the encoder list goes
	// from empty to whatever the freshly installed ffmpeg exposes.
	refreshEncoderOptions = func() {
		options := encoderOptionsFor(ffmpeg["encoders"])
		encoder.Options = options
		previous := encoder.Selected
		if !containsString(options, previous) {
			previous = options[0]
		}
		encoder.SetSelected(previous)
		encoder.Refresh()
	}
	codecLabel := widget.NewLabel("Video codec")
	codecLabel.Alignment = fyne.TextAlignLeading

	buttonSize := fyne.NewSize(150, 34)
	alignActionButton := func(btn *widget.Button) fyne.CanvasObject {
		return container.NewGridWrap(buttonSize, btn)
	}

	header := container.NewVBox(title, subtitle)

	// System diagnostics: ffmpeg/ffprobe availability, free disk, memory, CPU.
	// Runs off the UI thread because the checks shell out to ffmpeg.
	diagnosticBtn := widget.NewButtonWithIcon("Diagnostic", theme.InfoIcon(), func() {
		status.SetText("Status: Running diagnostic...")
		go func() {
			health := common.CheckHealth(cfg)
			report := common.GetHealthReport(health)
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
	toolbar := container.NewHBox(
		alignActionButton(open),
		alignActionButton(selectOutput),
		alignActionButton(start),
		alignActionButton(cancel),
		alignActionButton(diagnosticBtn),
		alignActionButton(quitBtn),
	)

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

	window.Resize(fyne.NewSize(980, 470))
	window.SetFixedSize(true)

	window.ShowAndRun()
}

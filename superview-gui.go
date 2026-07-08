//go:build windows
// +build windows

package main

import (
	_ "embed"
	"fmt"
	"io"
	"log/slog"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"superview/common"
	"syscall"
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

func showPrerequisiteDialog(window fyne.Window, err error) bool {
	if err == nil || !strings.Contains(err.Error(), "cannot find ffmpeg/ffprobe") {
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
	dialog.NewCustom("Error", "OK", content, window).Show()
	return true
}

func runCommandAndGetPath(name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	prepareNativeDialogCommand(cmd)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func prepareNativeDialogCommand(cmd *exec.Cmd) {
	cmd.SysProcAttr = &syscall.SysProcAttr{HideWindow: true}
}

func normalizeNativeDialogResult(path string, err error) (string, error) {
	return common.NormalizeNativeDialogResult(path, err)
}

func chooseInputFileNative() (string, error) {
	script := strings.Join([]string{
		"Add-Type -AssemblyName System.Windows.Forms",
		"$dialog = New-Object System.Windows.Forms.OpenFileDialog",
		"$dialog.Title = 'Select input video'",
		"$dialog.Filter = 'Video Files|*.mp4;*.MP4;*.mov;*.MOV;*.mkv;*.MKV;*.avi;*.AVI;*.m4v;*.M4V;*.webm;*.WEBM;*.flv;*.FLV;*.wmv;*.WMV;*.mpeg;*.MPEG;*.mpg;*.MPG|All Files|*.*'",
		"$dialog.CheckFileExists = $true",
		"$dialog.Multiselect = $false",
		"if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($dialog.FileName) }",
	}, "; ")
	path, runErr := runCommandAndGetPath("powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script)
	return normalizeNativeDialogResult(path, runErr)
}

func chooseOutputFileNative() (string, error) {
	script := strings.Join([]string{
		"Add-Type -AssemblyName System.Windows.Forms",
		"$dialog = New-Object System.Windows.Forms.SaveFileDialog",
		"$dialog.Title = 'Save output video'",
		"$dialog.Filter = 'MP4 Video|*.mp4|All Files|*.*'",
		"$dialog.DefaultExt = 'mp4'",
		"$dialog.AddExtension = $true",
		"$dialog.OverwritePrompt = $true",
		"$dialog.FileName = 'output.mp4'",
		"if ($dialog.ShowDialog() -eq [System.Windows.Forms.DialogResult]::OK) { [Console]::Out.Write($dialog.FileName) }",
	}, "; ")
	path, runErr := runCommandAndGetPath("powershell", "-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script)
	return normalizeNativeDialogResult(path, runErr)
}

// GUIHandler implements UIHandler for GUI interface
type GUIHandler struct {
	window   fyne.Window
	bitrate  int
	encoder  *widget.Select
	progress *dialog.ProgressDialog
	ffmpeg   map[string]string
	video    *common.VideoSpecs
	logger   *slog.Logger
}

func (h *GUIHandler) ShowError(err error) {
	fyne.Do(func() {
		if showPrerequisiteDialog(h.window, err) {
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
		h.progress.SetValue(percent / 100)
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
	return false
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

	// Initialize logger for GUI (suppress to avoid cluttering the UI)
	gui_logger := slog.New(slog.NewTextHandler(io.Discard, &slog.HandlerOptions{
		Level: slog.LevelDebug,
	}))
	common.SetLogger(gui_logger)
	common.RegisterObservabilityHandler(common.NewDefaultObservabilityHandler(gui_logger))

	// Load configuration (from superview.yaml or env vars)
	cfg, err := common.LoadConfig("superview.yaml")
	if err != nil {
		gui_logger.Error("Failed to load configuration", slog.String("error", err.Error()))
		// Continue with current/default configuration to avoid nil dereference.
		cfg = common.GetConfig()
	} else {
		common.SetConfig(cfg)
	}

	app := app.NewWithID("com.canaill51.superview")
	iconResource := fyne.NewStaticResource("Icon.png", appIconPNG)
	app.SetIcon(iconResource)
	app.Settings().SetTheme(theme.DarkTheme())

	window := app.NewWindow("Superview")
	window.SetIcon(iconResource)

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
						// Demande d'annulation
						if cancelEncoding != nil {
							close(cancelEncoding)
						}
						// On laisse la goroutine d'encodage nettoyer, puis on ferme l'app après un court délai
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

	subtitle := widget.NewLabel("Transform your video in 3 simple steps")
	subtitle.Alignment = fyne.TextAlignCenter

	selectedFile := widget.NewLabel("No input file selected")

	selectedOutput := widget.NewLabel("No output file selected")

	selectedQualityProfile := "Balanced" // Default quality profile

	status := widget.NewLabel("Status: Ready")
	status.Alignment = fyne.TextAlignCenter
	status.TextStyle = fyne.TextStyle{Bold: true}
	results := widget.NewLabel("Results: no completed run yet")
	selectionSummary := widget.NewLabel("Profile: Balanced | Codec: Auto")
	selectionSummary.Alignment = fyne.TextAlignCenter
	selectionSummary.TextStyle = fyne.TextStyle{Italic: true}

	var updateSelectionSummary func()

	qualityProfileSelect := widget.NewSelect([]string{"Fast", "Balanced"}, func(s string) {
		selectedQualityProfile = s
		if updateSelectionSummary != nil {
			updateSelectionSummary()
		}
	})
	qualityProfileSelect.Alignment = fyne.TextAlignCenter
	qualityProfileSelect.SetSelected("Balanced")
	qualityProfileLabel := widget.NewLabel("Quality")
	qualityProfileLabel.Alignment = fyne.TextAlignCenter
	qualityHint := widget.NewLabel("Fast: faster encode, smaller output | Balanced: best visual quality")
	qualityHint.Alignment = fyne.TextAlignCenter

	start := widget.NewButtonWithIcon("3) Start Superview transform", theme.MediaPlayIcon(), func() {
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
			profileBitrate := video.Streams[0].BitrateInt
			switch selectedQualityProfile {
			case "Fast":
				profileBitrate = int(float64(video.Streams[0].BitrateInt) * 1.0)
				effectiveCfg.VideoPreset = "fast"
			case "Balanced":
				profileBitrate = int(float64(video.Streams[0].BitrateInt) * 1.6)
				effectiveCfg.VideoPreset = "medium"
			default:
				profileBitrate = int(float64(video.Streams[0].BitrateInt) * 1.6)
				effectiveCfg.VideoPreset = "medium"
			}

			if profileBitrate < cfg.MinBitrate {
				profileBitrate = cfg.MinBitrate
			}
			if cfg.MaxBitrate > 0 && profileBitrate > cfg.MaxBitrate {
				profileBitrate = cfg.MaxBitrate
			}

			common.SetConfig(&effectiveCfg)

			prog := dialog.NewProgress("Transforming", "Superview is processing your video...", window)
			prog.Show()
			status.SetText("Status: Transforming...")

			// Activation de l'état d'encodage et initialisation du canal d'annulation
			encodingInProgress = true
			cancelEncoding = make(chan struct{})

			go func() {
				handler := &GUIHandler{
					window:   window,
					bitrate:  profileBitrate,
					encoder:  encoder,
					progress: prog,
					ffmpeg:   ffmpeg,
					video:    video,
					logger:   common.GetLogger(),
				}

				if err := common.PerformEncoding(video.File, uri, handler, ffmpeg, cancelEncoding); err != nil {
					fyne.Do(func() {
						prog.Hide()
						status.SetText("Status: Failed")
						results.SetText("Results: last run failed")
					})
					handler.ShowError(err)
					encodingInProgress = false
					return
				}

				lastMetrics := common.GetLastEncodingMetrics()
				resultsText := formatResultsPanel(lastMetrics)
				fyne.Do(func() {
					prog.Hide()
					status.SetText("Status: Completed")
					results.SetText(resultsText)
				})
				handler.ShowInfo("Transform complete. Output file:\n" + uri)
				encodingInProgress = false
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

	refreshStart := func() {
		if video != nil && outputPath != "" {
			start.Enable()
		} else {
			start.Disable()
		}
	}

	open := widget.NewButtonWithIcon("1) Choose input file", theme.FolderOpenIcon(), func() {
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

				fallbackURI := strings.ReplaceAll(file.URI().String(), "file://", "")
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
				selectedFile.SetText("Input: " + filepath.Base(video.File))
				status.SetText("Status: Input loaded")
				refreshStart()
			}, window)
			fd.SetFilter(storage.NewExtensionFileFilter([]string{".mp4", ".avi", ".mov", ".mkv", ".m4v", ".webm", ".flv", ".wmv", ".mpeg", ".mpg", ".MP4", ".AVI", ".MOV", ".MKV", ".M4V", ".WEBM", ".FLV", ".WMV", ".MPEG", ".MPG"}))
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
		selectedFile.SetText("Input: " + filepath.Base(video.File))
		status.SetText("Status: Input loaded")
		refreshStart()
	})

	selectOutput := widget.NewButtonWithIcon("2) Choose output file", theme.DocumentSaveIcon(), func() {
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

				path := strings.ReplaceAll(file.URI().String(), "file://", "")
				err = file.Close()
				if err != nil {
					fyne.LogError("Failed to close stream", err)
				}
				if filepath.Ext(strings.ToLower(path)) != ".mp4" {
					path += ".mp4"
				}
				outputPath = path
				selectedOutput.SetText("Output: " + filepath.Base(outputPath))
				status.SetText("Status: Output selected")
				refreshStart()
			}, window)
			return
		}
		if uri == "" {
			common.GetLogger().Debug("File saving cancelled by user")
			return
		}
		if filepath.Ext(strings.ToLower(uri)) != ".mp4" {
			uri += ".mp4"
		}
		outputPath = uri
		selectedOutput.SetText("Output: " + filepath.Base(outputPath))
		status.SetText("Status: Output selected")
		refreshStart()
	})

	ffmpeg, err = common.CheckFfmpeg()
	if err != nil {
		if !showPrerequisiteDialog(window, err) {
			dialog.ShowError(err, window)
		}
		open.Disable()
		selectOutput.Disable()
		status.SetText("Status: ffmpeg unavailable")
	}

	encoderOptions := []string{"Use same video codec as input file"}

	for _, enc := range strings.Split(ffmpeg["encoders"], ",") {
		encoderOptions = append(encoderOptions, enc+" encoder")
	}
	encoder = widget.NewSelect(encoderOptions, func(s string) {
		if updateSelectionSummary != nil {
			updateSelectionSummary()
		}
	})
	encoder.Alignment = fyne.TextAlignCenter
	encoder.SetSelected(encoderOptions[0])
	codecLabel := widget.NewLabel("Video codec")
	codecLabel.Alignment = fyne.TextAlignCenter

	updateSelectionSummary = func() {
		codec := common.ParseEncoderSelection(encoder.Selected)
		if codec == "" {
			codec = "Auto"
		}
		selectionSummary.SetText(fmt.Sprintf("Profile: %s | Codec: %s", selectedQualityProfile, codec))
	}
	updateSelectionSummary()

	buttonSize := fyne.NewSize(300, 40)
	selectSize := fyne.NewSize(360, 40)
	centerButton := func(btn *widget.Button) fyne.CanvasObject {
		return container.NewCenter(container.NewGridWrap(buttonSize, btn))
	}
	centerSelect := func(sel *widget.Select) fyne.CanvasObject {
		return container.NewCenter(container.NewGridWrap(selectSize, sel))
	}

	header := container.NewVBox(
		subtitle,
		widget.NewSeparator(),
	)

	quitBtn := widget.NewButton("Quit", func() {
		app.Quit()
	})

	flow := widget.NewForm(
		widget.NewFormItem("", centerButton(open)),
		widget.NewFormItem("", container.NewCenter(selectedFile)),
		widget.NewFormItem("", qualityProfileLabel),
		widget.NewFormItem("", centerSelect(qualityProfileSelect)),
		widget.NewFormItem("", container.NewCenter(qualityHint)),
		widget.NewFormItem("", codecLabel),
		widget.NewFormItem("", centerSelect(encoder)),
		widget.NewFormItem("", container.NewCenter(selectionSummary)),
		widget.NewFormItem("", widget.NewLabel(" ")),
		widget.NewFormItem("", centerButton(selectOutput)),
		widget.NewFormItem("", container.NewCenter(selectedOutput)),
		widget.NewFormItem("", centerButton(start)),
		widget.NewFormItem("", container.NewCenter(results)),
	)

	window.SetContent(container.NewVBox(
		header,
		flow,
		status,
		centerButton(quitBtn),
	))

	window.Resize(fyne.NewSize(700, 420))

	window.ShowAndRun()
}

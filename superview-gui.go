package main

import (
	"errors"
	"fmt"
	"io"
	"log/slog"
	"os/exec"
	"path/filepath"
	"runtime"
	"strings"
	"superview/common"

	"fyne.io/fyne"
	"fyne.io/fyne/app"
	"fyne.io/fyne/container"
	"fyne.io/fyne/dialog"
	"fyne.io/fyne/storage"
	"fyne.io/fyne/theme"
	"fyne.io/fyne/widget"
)

func runCommandAndGetPath(name string, args ...string) (string, error) {
	cmd := exec.Command(name, args...)
	out, err := cmd.Output()
	if err != nil {
		return "", err
	}
	return strings.TrimSpace(string(out)), nil
}

func normalizeNativeDialogResult(path string, err error) (string, error) {
	if err == nil {
		return strings.TrimSpace(path), nil
	}

	var exitErr *exec.ExitError
	if errors.As(err, &exitErr) {
		if exitErr.ExitCode() == 1 || exitErr.ExitCode() == 255 {
			return "", nil
		}
	}

	return "", err
}

func chooseInputFileNative() (string, error) {
	if runtime.GOOS == "linux" {
		if _, err := exec.LookPath("zenity"); err == nil {
			path, runErr := runCommandAndGetPath(
				"zenity",
				"--file-selection",
				"--title=Select input video",
				"--file-filter=Videos | *.mp4 *.MP4 *.mov *.MOV *.mkv *.MKV *.avi *.AVI *.m4v *.M4V *.webm *.WEBM *.flv *.FLV *.wmv *.WMV *.mpeg *.MPEG *.mpg *.MPG",
				"--file-filter=All files | *",
			)
			return normalizeNativeDialogResult(path, runErr)
		}
		if _, err := exec.LookPath("kdialog"); err == nil {
			path, runErr := runCommandAndGetPath("kdialog", "--getopenfilename", "", "Videos (*.mp4 *.MP4 *.mov *.MOV *.mkv *.MKV *.avi *.AVI *.m4v *.M4V *.webm *.WEBM *.flv *.FLV *.wmv *.WMV *.mpeg *.MPEG *.mpg *.MPG)")
			return normalizeNativeDialogResult(path, runErr)
		}
	}
	return "", fmt.Errorf("native file dialog not available on this system")
}

func chooseOutputFileNative() (string, error) {
	if runtime.GOOS == "linux" {
		if _, err := exec.LookPath("zenity"); err == nil {
			path, runErr := runCommandAndGetPath("zenity", "--file-selection", "--save", "--confirm-overwrite", "--title=Save output video", "--filename=output.mp4")
			return normalizeNativeDialogResult(path, runErr)
		}
		if _, err := exec.LookPath("kdialog"); err == nil {
			path, runErr := runCommandAndGetPath("kdialog", "--getsavefilename", "output.mp4", "Videos (*.mp4)")
			return normalizeNativeDialogResult(path, runErr)
		}
	}
	return "", fmt.Errorf("native file dialog not available on this system")
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
	dialog.ShowError(err, h.window)
}

func (h *GUIHandler) ShowInfo(msg string) {
	dialog.ShowInformation("Done", msg, h.window)
}

func (h *GUIHandler) ShowProgress(percent float64) {
	h.progress.SetValue(percent / 100)
}

func (h *GUIHandler) GetBitrate() (int, error) {
	return h.bitrate, nil
}

func (h *GUIHandler) GetEncoder() string {
	if h.video == nil || len(h.video.Streams) == 0 {
		return ""
	}
	selected := strings.TrimSpace(h.encoder.Selected)
	if selected == "Use same video codec as input file" {
		return ""
	}
	parts := strings.Fields(selected)
	if len(parts) == 0 {
		return ""
	}
	return parts[0]
}

func (h *GUIHandler) GetSqueeze() bool {
	return false
}

func main() {
	var video *common.VideoSpecs
	var outputPath string
	var ffmpeg map[string]string
	var encoder *widget.Select

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
		// Continue with defaults
	} else {
		common.SetConfig(cfg)
	}

	app := app.New()
	app.Settings().SetTheme(theme.DarkTheme())

	window := app.NewWindow("Superview")

	subtitle := widget.NewLabel("Transform your video in 3 simple steps")
	subtitle.Alignment = fyne.TextAlignCenter

	fixedBitrate := cfg.MaxBitrate
	if fixedBitrate <= 0 {
		fixedBitrate = 50000000
	}

	selectedFile := widget.NewLabel("No input file selected")
	selectedFile.Wrapping = fyne.TextWrapWord

	selectedOutput := widget.NewLabel("No output file selected")
	selectedOutput.Wrapping = fyne.TextWrapWord

	status := widget.NewLabel("Status: Ready")
	status.Alignment = fyne.TextAlignCenter
	status.TextStyle = fyne.TextStyle{Bold: true}

	start := widget.NewButtonWithIcon("3) Start Superview transform", theme.MediaPlayIcon(), func() {
		if video == nil {
			dialog.ShowInformation("No input", "Please open an input video first.", window)
			return
		}
		if outputPath == "" {
			dialog.ShowInformation("No output", "Please choose an output file first.", window)
			return
		}

		uri := outputPath

		prog := dialog.NewProgress("Transforming", "Superview is processing your video...", window)
		prog.Show()
		status.SetText("Status: Transforming...")

		go func() {
			handler := &GUIHandler{
				window:   window,
				bitrate:  fixedBitrate,
				encoder:  encoder,
				progress: prog,
				ffmpeg:   ffmpeg,
				video:    video,
				logger:   common.GetLogger(),
			}

			if err := common.PerformEncoding(video.File, uri, handler, ffmpeg); err != nil {
				prog.Hide()
				status.SetText("Status: Failed")
				handler.ShowError(err)
				return
			}

			prog.Hide()
			status.SetText("Status: Completed")
			handler.ShowInfo("Transform complete. Output file:\n" + uri)
		}()

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
		dialog.ShowError(err, window)
		open.Disable()
		selectOutput.Disable()
		status.SetText("Status: ffmpeg unavailable")
	}

	encoderOptions := []string{"Use same video codec as input file"}
	centerLabel := func(text string) string {
		return "   " + text + "   "
	}
	for i := range encoderOptions {
		encoderOptions[i] = centerLabel(encoderOptions[i])
	}

	for _, enc := range strings.Split(ffmpeg["encoders"], ",") {
		encoderOptions = append(encoderOptions, centerLabel(enc+" encoder"))
	}
	encoder = widget.NewSelect(encoderOptions, func(s string) {

	})
	encoder.SetSelected(encoderOptions[0])
	codecLabel := widget.NewLabel("Output codec")
	codecLabel.Alignment = fyne.TextAlignCenter

	buttonSize := fyne.NewSize(300, 40)
	selectSize := fyne.NewSize(360, 40)
	centerButton := func(btn *widget.Button) fyne.CanvasObject {
		return container.NewCenter(container.NewGridWrap(buttonSize, btn))
	}
	centerSelect := func(sel *widget.Select) fyne.CanvasObject {
		return container.NewCenter(container.NewGridWrap(selectSize, sel))
	}

	header := widget.NewVBox(
		subtitle,
		widget.NewSeparator(),
	)

	quitBtn := widget.NewButton("Quit", func() {
		app.Quit()
	})

	flow := widget.NewForm(
		widget.NewFormItem("", centerButton(open)),
		widget.NewFormItem("", selectedFile),
		widget.NewFormItem("", codecLabel),
		widget.NewFormItem("", centerSelect(encoder)),
		widget.NewFormItem("", centerButton(selectOutput)),
		widget.NewFormItem("", selectedOutput),
		widget.NewFormItem("", centerButton(start)),
	)

	window.SetContent(widget.NewVBox(
		header,
		flow,
		status,
		centerButton(quitBtn),
	))

	window.Resize(fyne.NewSize(700, 420))

	window.ShowAndRun()
}

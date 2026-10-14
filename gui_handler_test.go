package main

import (
	"errors"
	"testing"
	"time"

	"fyne.io/fyne/v2"
	"fyne.io/fyne/v2/test"
	"fyne.io/fyne/v2/widget"
)

// The GUIHandler.Show* methods are what the encoding goroutine calls to reach
// the window. They were at 0 % (E-19): a ShowProgress dividing by the wrong
// factor, or a ShowError that stopped routing a missing ffmpeg to the dialog
// carrying the install link, would have shipped green. The test driver runs
// fyne.Do synchronously, so the widgets can be read on the next line.

// overlayObjects lists what the topmost dialog on the window is drawing, or
// nil when no dialog is open.
func overlayObjects(w fyne.Window) []fyne.CanvasObject {
	top := w.Canvas().Overlays().Top()
	if top == nil {
		return nil
	}
	return test.LaidOutObjects(top)
}

// overlayHasInstallLink reports whether the open dialog carries the install
// hyperlink, which only the prerequisite dialog does.
func overlayHasInstallLink(w fyne.Window) bool {
	for _, o := range overlayObjects(w) {
		if link, ok := o.(*widget.Hyperlink); ok && link.URL != nil && link.URL.String() == installURL {
			return true
		}
	}
	return false
}

// TestGUIHandler_ShowProgressSetsTheBarAsAFraction catches a ShowProgress that
// passes the percentage through, or divides by the wrong factor: the bar takes
// a fraction in [0, 1], the pipeline reports 0-100.
func TestGUIHandler_ShowProgressSetsTheBarAsAFraction(t *testing.T) {
	test.NewApp()
	bar := widget.NewProgressBar()
	h := &GUIHandler{inlineBar: bar}

	h.ShowProgress(42)

	if got := bar.Value; got < 0.4199 || got > 0.4201 {
		t.Errorf("progress bar = %v after ShowProgress(42), want 0.42", got)
	}
}

// TestGUIHandler_ShowProgressWithoutBarDoesNotPanic guards the nil check: a
// handler built before the bar exists must degrade to no update.
func TestGUIHandler_ShowProgressWithoutBarDoesNotPanic(t *testing.T) {
	test.NewApp()
	(&GUIHandler{}).ShowProgress(42)
}

// TestGUIHandler_ShowProgressDetailWritesTheStatusLine catches a handler that
// stops forwarding the remaining time, or writes something other than what
// formatEncodingStatus produces -- the line the user watches for minutes.
func TestGUIHandler_ShowProgressDetailWritesTheStatusLine(t *testing.T) {
	test.NewApp()
	status := widget.NewLabel("Status: Ready")
	h := &GUIHandler{status: status}

	h.ShowProgressDetail(42, 90*time.Second)

	if want := formatEncodingStatus(42, 90*time.Second); status.Text != want {
		t.Errorf("status = %q, want %q", status.Text, want)
	}
}

// TestGUIHandler_ShowInfoOpensADialogWithTheMessage catches a ShowInfo that
// stops showing anything: the completion message is where the user learns
// where the file went.
func TestGUIHandler_ShowInfoOpensADialogWithTheMessage(t *testing.T) {
	test.NewApp()
	w := test.NewTempWindow(t, widget.NewLabel(""))
	h := &GUIHandler{window: w}

	h.ShowInfo("Saved to /tmp/out.mp4")

	found := false
	for _, o := range overlayObjects(w) {
		if label, ok := o.(*widget.Label); ok && label.Text == "Saved to /tmp/out.mp4" {
			found = true
		}
	}
	if !found {
		t.Error("ShowInfo opened no dialog carrying the message")
	}
}

// TestGUIHandler_ShowErrorRoutesAMissingFFmpegToThePrerequisiteDialog catches
// a ShowError that shows the bare error for a missing ffmpeg, losing the
// install link, and one that shows the install link for an unrelated failure.
func TestGUIHandler_ShowErrorRoutesAMissingFFmpegToThePrerequisiteDialog(t *testing.T) {
	test.NewApp()

	t.Run("missing ffmpeg gets the install link", func(t *testing.T) {
		w := test.NewTempWindow(t, widget.NewLabel(""))
		(&GUIHandler{window: w}).ShowError(errors.New("cannot find ffmpeg/ffprobe on your system"))

		if !overlayHasInstallLink(w) {
			t.Error("a missing ffmpeg was reported without the install link")
		}
	})

	t.Run("another error gets a plain error dialog", func(t *testing.T) {
		w := test.NewTempWindow(t, widget.NewLabel(""))
		(&GUIHandler{window: w}).ShowError(errors.New("ffmpeg exited with code 1"))

		if w.Canvas().Overlays().Top() == nil {
			t.Fatal("ShowError opened no dialog")
		}
		if overlayHasInstallLink(w) {
			t.Error("an unrelated failure was reported as a missing ffmpeg")
		}
	})
}

// TestShowPrerequisiteDialog_ClaimsOnlyTheMissingFFmpegError pins the return
// value callers rely on: false means "not mine, report it yourself". A dialog
// that claimed every error would swallow real failures behind an install
// link; one that claimed none would leave the user without it.
func TestShowPrerequisiteDialog_ClaimsOnlyTheMissingFFmpegError(t *testing.T) {
	test.NewApp()
	missing := errors.New("cannot find ffmpeg/ffprobe on your system")

	for _, retry := range []func(){nil, func() {}} {
		w := test.NewTempWindow(t, widget.NewLabel(""))
		if !showPrerequisiteDialog(w, missing, retry) {
			t.Errorf("showPrerequisiteDialog(missing ffmpeg, retry=%v) = false, want true", retry != nil)
		}
		if !overlayHasInstallLink(w) {
			t.Errorf("the prerequisite dialog (retry=%v) does not carry the install link", retry != nil)
		}
	}

	w := test.NewTempWindow(t, widget.NewLabel(""))
	if showPrerequisiteDialog(w, errors.New("ffmpeg exited with code 1"), nil) {
		t.Error("showPrerequisiteDialog claimed an unrelated error")
	}
	if w.Canvas().Overlays().Top() != nil {
		t.Error("showPrerequisiteDialog opened a dialog for an error it does not handle")
	}
	if showPrerequisiteDialog(w, nil, nil) {
		t.Error("showPrerequisiteDialog claimed a nil error")
	}
}

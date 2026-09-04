package common

import (
	"errors"
	"sync"
	"testing"
	"time"
)

type testObsHandler struct {
	mu            sync.Mutex
	eventCount    int
	progressCount int
	errorCount    int
	completeCount int
	ch            chan string
}

func (h *testObsHandler) OnEvent(event *EncodingEvent) {
	h.mu.Lock()
	h.eventCount++
	h.mu.Unlock()
	h.ch <- "event"
}

func (h *testObsHandler) OnProgress(percent float64, message string) {
	h.mu.Lock()
	h.progressCount++
	h.mu.Unlock()
	h.ch <- "progress"
}

func (h *testObsHandler) OnError(err error, context map[string]interface{}) {
	h.mu.Lock()
	h.errorCount++
	h.mu.Unlock()
	h.ch <- "error"
}

func (h *testObsHandler) OnComplete(metrics *EncodingMetrics) {
	h.mu.Lock()
	h.completeCount++
	h.mu.Unlock()
	h.ch <- "complete"
}

func TestEventRecorder_FillsTimestamp(t *testing.T) {
	r := NewEventRecorder()
	h := &testObsHandler{ch: make(chan string, 4)}
	r.RegisterHandler(h)

	e := &EncodingEvent{EventType: "start", Message: "start"}
	r.RecordEvent(e)

	if e.Timestamp.IsZero() {
		t.Fatal("expected the timestamp to be filled in")
	}
	select {
	case <-h.ch:
	default:
		t.Fatal("handler should have been called synchronously")
	}
}

// TestEventRecorder_DispatchIsSynchronousAndOrdered pins the behaviour that
// replaced the previous "go handler.OnEvent(...)" fan-out. One goroutine per
// event meant progress events -- emitted several times a second -- could be
// logged out of order, and events recorded just before exit could be lost.
func TestEventRecorder_DispatchIsSynchronousAndOrdered(t *testing.T) {
	r := NewEventRecorder()

	var seen []string
	r.RegisterHandler(&orderRecordingHandler{onEvent: func(e *EncodingEvent) {
		seen = append(seen, e.EventType)
	}})

	for _, kind := range []string{"start", "progress", "progress", "complete"} {
		r.RecordEvent(&EncodingEvent{EventType: kind})
	}

	// No synchronisation here on purpose: if dispatch were still asynchronous,
	// this slice would be racy and incomplete.
	want := []string{"start", "progress", "progress", "complete"}
	if len(seen) != len(want) {
		t.Fatalf("expected %d events delivered inline, got %d (%v)", len(want), len(seen), seen)
	}
	for i := range want {
		if seen[i] != want[i] {
			t.Fatalf("event %d = %q, want %q (order not preserved: %v)", i, seen[i], want[i], seen)
		}
	}
}

// orderRecordingHandler records calls without any locking, so a regression to
// asynchronous dispatch shows up under -race.
type orderRecordingHandler struct {
	onEvent func(*EncodingEvent)
}

func (h *orderRecordingHandler) OnEvent(e *EncodingEvent)              { h.onEvent(e) }
func (h *orderRecordingHandler) OnProgress(float64, string)            {}
func (h *orderRecordingHandler) OnError(error, map[string]interface{}) {}
func (h *orderRecordingHandler) OnComplete(*EncodingMetrics)           {}

func TestEventRecorder_DispatchHandlers(t *testing.T) {
	r := NewEventRecorder()
	h := &testObsHandler{ch: make(chan string, 16)}
	r.RegisterHandler(h)

	r.RecordProgress(42, "progress")
	r.RecordError(errors.New("boom"), map[string]interface{}{"stage": "encode"})
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.RecordCompletion(10)
	r.RecordCompletion(m)

	deadline := time.After(2 * time.Second)
	received := 0
	for received < 6 {
		select {
		case <-h.ch:
			received++
		case <-deadline:
			t.Fatalf("timeout waiting handler callbacks, got %d", received)
		}
	}
}

func TestSetGetLastEncodingMetrics(t *testing.T) {
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	SetLastEncodingMetrics(m)
	got := GetLastEncodingMetrics()
	if got != m {
		t.Fatal("expected same metrics pointer")
	}
}

func TestSetGetLastHardwareAccelerationSummary(t *testing.T) {
	SetLastHardwareAccelerationSummary("Hardware: used h264_nvenc encode + D3D11VA decode")
	got := GetLastHardwareAccelerationSummary()
	if got != "Hardware: used h264_nvenc encode + D3D11VA decode" {
		t.Fatalf("unexpected hardware summary: %q", got)
	}
}

func TestGlobalRecorderFunctions(t *testing.T) {
	old := globalEventRecorder
	defer func() { globalEventRecorder = old }()

	r := NewEventRecorder()
	globalEventRecorder = r
	h := &testObsHandler{ch: make(chan string, 8)}
	RegisterObservabilityHandler(h)

	RecordEncodingEvent(&EncodingEvent{EventType: "start", Message: "starting"})
	RecordEncodingProgress(10, "p")
	RecordEncodingError(errors.New("err"), map[string]interface{}{"k": "v"})
	m := NewEncodingMetrics("in.mp4", "out.mp4")
	m.RecordCompletion(1)
	RecordEncodingCompletion(m)

	deadline := time.After(2 * time.Second)
	received := 0
	for received < 7 {
		select {
		case <-h.ch:
			received++
		case <-deadline:
			t.Fatalf("timeout waiting global callbacks, got %d", received)
		}
	}
}

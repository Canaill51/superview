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

func TestEventRecorder_RecordEventAndHistory(t *testing.T) {
	r := NewEventRecorder()
	e := &EncodingEvent{EventType: "start", Message: "start"}
	r.RecordEvent(e)

	history := r.GetEventHistory()
	if len(history) != 1 {
		t.Fatalf("expected 1 event, got %d", len(history))
	}
	if history[0].Timestamp.IsZero() {
		t.Fatal("expected timestamp to be auto-filled")
	}
}

func TestEventRecorder_MaxHistory(t *testing.T) {
	r := NewEventRecorder()
	r.maxEvents = 2
	r.RecordEvent(&EncodingEvent{EventType: "e1"})
	r.RecordEvent(&EncodingEvent{EventType: "e2"})
	r.RecordEvent(&EncodingEvent{EventType: "e3"})

	history := r.GetEventHistory()
	if len(history) != 2 {
		t.Fatalf("expected 2 events, got %d", len(history))
	}
	if history[0].EventType != "e2" || history[1].EventType != "e3" {
		t.Fatalf("unexpected retained history: %+v", history)
	}
}

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

	if len(r.GetEventHistory()) != 4 {
		t.Fatalf("expected 4 events in history, got %d", len(r.GetEventHistory()))
	}
}

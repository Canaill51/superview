package common

import (
	"fmt"
	"log/slog"
	"sync"
	"time"
)

// EncodingEvent represents a discrete event during the encoding lifecycle.
// Events are recorded for observability and debugging purposes.
type EncodingEvent struct {
	EventType  string                 // "start", "progress", "complete", "error", "warning"
	Timestamp  time.Time              // When the event occurred
	Message    string                 // Human-readable description
	Attributes map[string]interface{} // Key-value metadata

	InputFile  string // Source file
	OutputFile string // Destination file
}

// ObservabilityHandler defines the interface for recording encoding events.
// Implementations can send events to logging systems, metrics platforms, etc.
type ObservabilityHandler interface {
	// OnEvent is called when an encoding event occurs (start, progress, complete, error)
	OnEvent(event *EncodingEvent)

	// OnProgress is called with updated progress percentage (0-100)
	OnProgress(percent float64, message string)

	// OnError is called when an encoding error occurs
	OnError(err error, context map[string]interface{})

	// OnComplete is called when encoding finishes successfully
	OnComplete(metrics *EncodingMetrics)
}

// DefaultObservabilityHandler provides basic logging of encoding events.
// All events are logged via slog at appropriate levels.
type DefaultObservabilityHandler struct {
	logger *slog.Logger
}

// NewDefaultObservabilityHandler creates a handler that logs to the provided logger.
func NewDefaultObservabilityHandler(logger *slog.Logger) *DefaultObservabilityHandler {
	if logger == nil {
		logger = slog.Default()
	}
	return &DefaultObservabilityHandler{logger: logger}
}

// OnEvent logs the encoding event at appropriate level.
func (h *DefaultObservabilityHandler) OnEvent(event *EncodingEvent) {
	if event == nil {
		return
	}

	level := slog.LevelInfo
	switch event.EventType {
	case "error":
		level = slog.LevelError
	case "warning":
		level = slog.LevelWarn
	case "start":
		level = slog.LevelInfo
	case "complete":
		level = slog.LevelInfo
	case "progress":
		level = slog.LevelDebug
	}

	if h.logger.Enabled(nil, level) {
		args := []interface{}{
			slog.String("event_type", event.EventType),
			slog.String("message", event.Message),
			slog.Time("timestamp", event.Timestamp),
		}

		// Add file paths if present
		if event.InputFile != "" {
			args = append(args, slog.String("input_file", event.InputFile))
		}
		if event.OutputFile != "" {
			args = append(args, slog.String("output_file", event.OutputFile))
		}

		// Add custom attributes
		if len(event.Attributes) > 0 {
			for key, val := range event.Attributes {
				args = append(args, slog.Any(key, val))
			}
		}

		h.logger.Log(nil, level, "encoding_event", args...)
	}
}

// OnProgress logs progress updates at debug level.
func (h *DefaultObservabilityHandler) OnProgress(percent float64, message string) {
	h.logger.Debug("encoding progress",
		slog.Float64("progress_percent", percent),
		slog.String("message", message),
	)
}

// OnError logs encoding errors at error level.
func (h *DefaultObservabilityHandler) OnError(err error, context map[string]interface{}) {
	args := []interface{}{slog.String("error", err.Error())}

	for key, val := range context {
		args = append(args, slog.Any(key, val))
	}

	h.logger.Error("encoding failed", args...)
}

// OnComplete logs successful encoding completion.
func (h *DefaultObservabilityHandler) OnComplete(metrics *EncodingMetrics) {
	if metrics == nil {
		h.logger.Info("encoding completed")
		return
	}

	metrics.LogMetrics(h.logger)
}

// EventRecorder manages multiple observability handlers and formats event recording.
// It allows plugins/handlers to receive encoding events for custom processing.
type EventRecorder struct {
	mu       sync.RWMutex
	handlers []ObservabilityHandler
	events   []*EncodingEvent // Event history (last 1000)
	maxEvents int
}

// NewEventRecorder creates a new event recorder with default capacity.
func NewEventRecorder() *EventRecorder {
	return &EventRecorder{
		handlers:  make([]ObservabilityHandler, 0),
		events:    make([]*EncodingEvent, 0, 1000),
		maxEvents: 1000,
	}
}

// RegisterHandler adds a new observability handler.
func (r *EventRecorder) RegisterHandler(handler ObservabilityHandler) {
	if handler == nil {
		return
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	r.handlers = append(r.handlers, handler)
}

// RecordEvent records an encoding event to all registered handlers.
func (r *EventRecorder) RecordEvent(event *EncodingEvent) {
	if event == nil {
		return
	}

	// Ensure timestamp
	if event.Timestamp.IsZero() {
		event.Timestamp = time.Now().UTC()
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	// Add to history
	r.events = append(r.events, event)
	if len(r.events) > r.maxEvents {
		r.events = r.events[1:]
	}

	// Dispatch to all handlers
	for _, handler := range r.handlers {
		// Non-blocking dispatch
		go handler.OnEvent(event)
	}
}

// RecordProgress records a progress update.
func (r *EventRecorder) RecordProgress(percent float64, message string) {
	r.RecordEvent(&EncodingEvent{
		EventType: "progress",
		Message:   message,
		Attributes: map[string]interface{}{
			"progress_percent": percent,
		},
	})

	// Also dispatch to progress handlers
	r.mu.RLock()
	handlers := r.handlers
	r.mu.RUnlock()

	for _, handler := range handlers {
		go handler.OnProgress(percent, message)
	}
}

// RecordError records an encoding error.
func (r *EventRecorder) RecordError(err error, context map[string]interface{}) {
	if err == nil {
		return
	}

	attrs := map[string]interface{}{"error_type": fmt.Sprintf("%T", err)}
	for key, val := range context {
		attrs[key] = val
	}

	r.RecordEvent(&EncodingEvent{
		EventType:  "error",
		Message:    err.Error(),
		Attributes: attrs,
	})

	// Also dispatch to error handlers
	r.mu.RLock()
	handlers := r.handlers
	r.mu.RUnlock()

	for _, handler := range handlers {
		go handler.OnError(err, context)
	}
}

// RecordCompletion records successful encoding completion.
func (r *EventRecorder) RecordCompletion(metrics *EncodingMetrics) {
	if metrics == nil {
		return
	}

	r.RecordEvent(&EncodingEvent{
		EventType: "complete",
		Message:   fmt.Sprintf("Encoding completed: %s -> %s", metrics.InputFile, metrics.OutputFile),
		Attributes: map[string]interface{}{
			"elapsed_seconds":    metrics.ElapsedTime().Seconds(),
			"output_size_bytes":  metrics.OutputFileSize,
			"compression_ratio":  metrics.CompressionRatio,
			"encoding_speed_fps": metrics.EncodingSpeed,
		},
	})

	// Also dispatch to complete handlers
	r.mu.RLock()
	handlers := r.handlers
	r.mu.RUnlock()

	for _, handler := range handlers {
		go handler.OnComplete(metrics)
	}
}

// GetEventHistory returns the recorded event history (last 1000 events).
// This is useful for diagnostics and replay.
func (r *EventRecorder) GetEventHistory() []*EncodingEvent {
	r.mu.RLock()
	defer r.mu.RUnlock()

	// Return copy to prevent external modification
	result := make([]*EncodingEvent, len(r.events))
	copy(result, r.events)
	return result
}

// ClearHistory clears the recorded event history.
func (r *EventRecorder) ClearHistory() {
	r.mu.Lock()
	defer r.mu.Unlock()

	r.events = r.events[:0]
}

// Global event recorder instance
var globalEventRecorder *EventRecorder = NewEventRecorder()

// Global metrics from last encoding (for CLI/GUI reporting)
var lastEncodingMetrics *EncodingMetrics
var metricsMutex sync.RWMutex

// GetLastEncodingMetrics returns the metrics from the last encoding operation.
// Returns nil if no encoding has been performed yet.
func GetLastEncodingMetrics() *EncodingMetrics {
	metricsMutex.RLock()
	defer metricsMutex.RUnlock()
	return lastEncodingMetrics
}

// SetLastEncodingMetrics updates the last encoding metrics (called by PerformEncoding).
// This is primarily for CLI/GUI to access encoding performance data.
func SetLastEncodingMetrics(metrics *EncodingMetrics) {
	metricsMutex.Lock()
	defer metricsMutex.Unlock()
	lastEncodingMetrics = metrics
}

// RegisterObservabilityHandler registers a global observability handler.
// This allows UI code to subscribe to encoding events.
func RegisterObservabilityHandler(handler ObservabilityHandler) {
	if globalEventRecorder != nil {
		globalEventRecorder.RegisterHandler(handler)
	}
}

// RecordEncodingEvent records an event to the global recorder.
func RecordEncodingEvent(event *EncodingEvent) {
	if globalEventRecorder != nil {
		globalEventRecorder.RecordEvent(event)
	}
}

// RecordEncodingProgress records progress to the global recorder.
func RecordEncodingProgress(percent float64, message string) {
	if globalEventRecorder != nil {
		globalEventRecorder.RecordProgress(percent, message)
	}
}

// RecordEncodingError records an error to the global recorder.
func RecordEncodingError(err error, context map[string]interface{}) {
	if globalEventRecorder != nil {
		globalEventRecorder.RecordError(err, context)
	}
}

// RecordEncodingCompletion records completion to the global recorder.
func RecordEncodingCompletion(metrics *EncodingMetrics) {
	if globalEventRecorder != nil {
		globalEventRecorder.RecordCompletion(metrics)
	}
}

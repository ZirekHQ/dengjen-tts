package dengjen

/*
#include "libdengjen.h"
*/
import "C"
import (
	"runtime/cgo"
	"unsafe"
)

// EventType mirrors libdengjen's SYNTH_EVENT_* constants.
type EventType int32

const (
	EventSpeech   EventType = C.SYNTH_EVENT_SPEECH
	EventFinished EventType = C.SYNTH_EVENT_FINISHED
	EventError    EventType = C.SYNTH_EVENT_ERROR
)

// SynthesisEvent is one event delivered during a streaming Speak call.
type SynthesisEvent struct {
	Type EventType
	Data []byte // populated for EventSpeech; a copy, safe to retain
	Err  error  // populated for EventError
}

// testHandleDeleted, when non-nil, is invoked synchronously immediately
// after a cgo.Handle backing a Speak call's onEvent is deleted (from either
// this trampoline or Speak's own synchronous-error path in speak.go). It
// exists solely so tests can assert deterministically that Delete() was
// actually called. A GC-finalizer-based test was tried first and proved
// unreliable in practice: whether the underlying native synthesis library's
// background threads let the deleted handle's closure become collectible
// promptly turned out to depend on native thread-pool scheduling outside
// Go's control, not on whether Delete() ran -- so observing Delete() via
// this hook, rather than inferring it through the garbage collector, is the
// only way to test this deterministically. Nil in production; only ever
// set from a _test.go file.
var testHandleDeleted func()

//export goDengjenSpeechCallback
func goDengjenSpeechCallback(event C.struct_SynthesisEvent, userData unsafe.Pointer) C.uint8_t {
	h := cgo.Handle(uintptr(userData))
	onEvent, _ := h.Value().(func(SynthesisEvent) bool)

	goEvent := SynthesisEvent{Type: EventType(event.event_type)}
	switch event.event_type {
	case C.SYNTH_EVENT_SPEECH:
		if event.len > 0 {
			goEvent.Data = C.GoBytes(unsafe.Pointer(event.data), C.int(event.len))
		}
	case C.SYNTH_EVENT_ERROR:
		if event.error_ptr != nil {
			goEvent.Err = &FFIError{
				Code:    int32(event.error_ptr.code),
				Message: C.GoString(event.error_ptr.message),
			}
		}
	}
	// SAFETY (Go side): event was produced by exactly one SpeechSynthesisCallback
	// invocation (this one) and is freed here exactly once, per
	// libdengjenFreeSynthesisEvent's documented contract.
	C.libdengjenFreeSynthesisEvent(event)

	wantsMore := onEvent == nil || onEvent(goEvent)

	// A Finished/Error event is the normal end of the stream, but onEvent
	// returning false also ends it -- iterate_stream (Rust side) stops
	// iterating and returns without ever delivering a terminal event in that
	// case. Either condition means this is the last time the trampoline will
	// run for this handle, so the handle must be deleted here; the `||` (not
	// two independent `if`s) is load-bearing, since a Finished/Error event
	// whose onEvent call also returns false must still only delete once.
	terminal := event.event_type == C.SYNTH_EVENT_FINISHED || event.event_type == C.SYNTH_EVENT_ERROR
	if terminal || !wantsMore {
		h.Delete()
		if testHandleDeleted != nil {
			testHandleDeleted()
		}
	}
	if wantsMore {
		return 0
	}
	return 1
}

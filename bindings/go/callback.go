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

	if event.event_type == C.SYNTH_EVENT_FINISHED || event.event_type == C.SYNTH_EVENT_ERROR {
		h.Delete()
	}
	if wantsMore {
		return 0
	}
	return 1
}

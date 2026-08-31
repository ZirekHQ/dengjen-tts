package dengjen

/*
#include "libdengjen.h"
*/
import "C"
import (
	"runtime"
	"sync"
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

// callbackToken is deliberately field-free: Pinner.Pin pins only the object
// directly addressed, not pointers nested inside it, so a closure embedded
// in the pinned struct itself panics at runtime ("cgo argument has Go
// pointer to unpinned Go pointer"). The closure lives in callbackRegistry
// instead, which never crosses into C memory.
type callbackToken struct{}

type callbackEntry struct {
	fn     func(SynthesisEvent) bool
	pinner runtime.Pinner
}

var (
	callbackRegistryMu sync.Mutex
	callbackRegistry   = map[*callbackToken]*callbackEntry{}
)

func registerCallback(fn func(SynthesisEvent) bool) *callbackToken {
	token := &callbackToken{}
	entry := &callbackEntry{fn: fn}
	entry.pinner.Pin(token)

	callbackRegistryMu.Lock()
	callbackRegistry[token] = entry
	callbackRegistryMu.Unlock()
	return token
}

// releaseCallback is idempotent: a no-op if token was already released.
func releaseCallback(token *callbackToken) {
	callbackRegistryMu.Lock()
	entry, ok := callbackRegistry[token]
	delete(callbackRegistry, token)
	callbackRegistryMu.Unlock()
	if !ok {
		return
	}
	entry.pinner.Unpin()
	if testCallbackDataReleased != nil {
		testCallbackDataReleased()
	}
}

// testCallbackDataReleased, when non-nil, fires synchronously right after a
// release. A GC-finalizer-based test was tried first and was flaky -- the
// native library's background threads affect how promptly the released
// closure becomes collectible, unrelated to whether release actually ran --
// so tests observe release directly via this hook instead. Nil in
// production; only ever set from a _test.go file.
var testCallbackDataReleased func()

//export goDengjenSpeechCallback
func goDengjenSpeechCallback(event C.struct_SynthesisEvent, userData unsafe.Pointer) C.uint8_t {
	token := (*callbackToken)(userData)
	callbackRegistryMu.Lock()
	entry := callbackRegistry[token]
	callbackRegistryMu.Unlock()
	var onEvent func(SynthesisEvent) bool
	if entry != nil {
		onEvent = entry.fn
	}

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
	// libdengjenFreeSynthesisEvent's documented contract -- which now includes the
	// error message above, so this call site must not free it itself (that would
	// double-free the string libdengjenFreeSynthesisEvent is about to reclaim).
	C.libdengjenFreeSynthesisEvent(event)

	wantsMore := onEvent == nil || onEvent(goEvent)

	// onEvent returning false also ends the stream (iterate_stream never
	// delivers a terminal event in that case), so the `||` is load-bearing:
	// a Finished/Error event whose onEvent call also returns false must
	// still only release once.
	terminal := event.event_type == C.SYNTH_EVENT_FINISHED || event.event_type == C.SYNTH_EVENT_ERROR
	if terminal || !wantsMore {
		releaseCallback(token)
	}
	if wantsMore {
		return 0
	}
	return 1
}

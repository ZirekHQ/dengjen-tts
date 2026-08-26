package dengjen

/*
#include <stdlib.h>
#include "libdengjen.h"

// cgo generates the extern declaration for the //export'd
// goDengjenSpeechCallback (defined in callback.go) into _cgo_export.h, but
// that header is itself a product of cgo processing every preamble in this
// package -- including this one -- so #include-ing it here is a chicken-and-
// egg problem (cgo fails with "_cgo_export.h: No such file or directory").
// Declare the same prototype directly instead; it must match the signature
// of goDengjenSpeechCallback exactly.
extern uint8_t goDengjenSpeechCallback(struct SynthesisEvent event, void *userData);
*/
import "C"
import (
	"runtime/cgo"
	"unsafe"
)

// Synthesis mode constants, matching SYNTH_MODE_* in libdengjen.h.
const (
	SynthModeLazy     int32 = C.SYNTH_MODE_LAZY
	SynthModeParallel int32 = C.SYNTH_MODE_PARALLEL
	SynthModeRealtime int32 = C.SYNTH_MODE_REALTIME
)

// SynthesisParams controls how a Speak/SpeakToFile call synthesizes and
// post-processes audio, mirroring libdengjen's SynthesisParams struct.
type SynthesisParams struct {
	Mode              int32
	Rate              uint8 // 0-100
	Volume            uint8 // 0-100
	Pitch             uint8 // 0-100
	AppendedSilenceMs uint32
	Nonblocking       bool
}

func boolToUint8(b bool) C.uint8_t {
	if b {
		return 1
	}
	return 0
}

// SpeakToFile synthesizes text and writes it as a WAV file at outFilename.
// The bool return reports whether the file was written, independent of err
// (mirrors libdengjenSpeakToFile's own two-part success signal).
func (v *Voice) SpeakToFile(text string, params SynthesisParams, outFilename string) (bool, error) {
	if v.ptr == nil {
		return false, &FFIError{Message: "voice is closed"}
	}
	cText := C.CString(text)
	defer C.free(unsafe.Pointer(cText))
	cOutFilename := C.CString(outFilename)
	defer C.free(unsafe.Pointer(cOutFilename))

	cParams := C.struct_SynthesisParams{
		mode:                 C.int32_t(params.Mode),
		rate:                 C.uint8_t(params.Rate),
		volume:               C.uint8_t(params.Volume),
		pitch:                C.uint8_t(params.Pitch),
		appended_silence_ms:  C.uint32_t(params.AppendedSilenceMs),
		nonblocking:          boolToUint8(params.Nonblocking),
	}
	var cErr C.struct_ExternError
	wrote := C.libdengjenSpeakToFile(v.ptr, C.FfiStr(cText), cParams, C.FfiStr(cOutFilename), &cErr)
	if err := checkError(cErr); err != nil {
		return false, err
	}
	return wrote != 0, nil
}

// Speak synthesizes text and streams the resulting audio to onEvent, one
// event at a time. onEvent returns true to keep receiving events, false to
// stop early. If onEvent runs to the natural end of the stream, the last
// event delivered is EventFinished or EventError; if onEvent instead returns
// false, the stream stops immediately at whatever event triggered that, and
// no further event (in particular, no EventFinished) is delivered for this
// call. If params.Nonblocking is true, Speak returns immediately and onEvent
// continues firing from another goroutine until the stream ends by either of
// those means.
func (v *Voice) Speak(text string, params SynthesisParams, onEvent func(SynthesisEvent) bool) error {
	if v.ptr == nil {
		return &FFIError{Message: "voice is closed"}
	}
	cText := C.CString(text)
	defer C.free(unsafe.Pointer(cText))

	h := cgo.NewHandle(onEvent)

	cParams := C.struct_SynthesisParams{
		mode:                C.int32_t(params.Mode),
		rate:                C.uint8_t(params.Rate),
		volume:              C.uint8_t(params.Volume),
		pitch:               C.uint8_t(params.Pitch),
		appended_silence_ms: C.uint32_t(params.AppendedSilenceMs),
		callback:            C.SpeechSynthesisCallback(C.goDengjenSpeechCallback),
		nonblocking:         boolToUint8(params.Nonblocking),
		user_data:           unsafe.Pointer(h),
	}

	var cErr C.struct_ExternError
	C.libdengjenSpeak(v.ptr, C.FfiStr(cText), cParams, &cErr)
	if err := checkError(cErr); err != nil {
		// The callback is guaranteed to never fire for a call that reports an
		// error here (traced in the design spec, §"Streaming"), so this call
		// site — not the trampoline — owns deleting the handle.
		h.Delete()
		if testHandleDeleted != nil {
			testHandleDeleted()
		}
		return err
	}
	return nil
}

// Cancel interrupts the most recently started realtime-mode Speak call on
// this voice, if one is currently running; it has no effect otherwise (lazy
// and parallel-mode syntheses cannot be interrupted this way). The onEvent
// callback still receives a final EventFinished after a successful
// cancellation — there is no separate "cancelled" event.
//
// Callers must not call Cancel concurrently with Close on the same voice —
// that races a use-after-free that only the caller can avoid, mirroring
// libdengjenCancel's own documented contract.
func (v *Voice) Cancel() error {
	if v.ptr == nil {
		return &FFIError{Message: "voice is closed"}
	}
	var cErr C.struct_ExternError
	C.libdengjenCancel(v.ptr, &cErr)
	return checkError(cErr)
}

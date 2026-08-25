package dengjen

/*
#include <stdlib.h>
#include "libdengjen.h"
*/
import "C"
import "unsafe"

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

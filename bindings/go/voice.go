package dengjen

/*
#include <stdlib.h>
#include "libdengjen.h"
*/
import "C"
import (
	"runtime"
	"unsafe"
)

// Voice is a loaded dengjen-tts voice model. The zero value is not usable;
// construct one with LoadVoice. Callers must call Close when done; a
// finalizer is registered as a backstop, not a substitute for explicit Close.
type Voice struct {
	ptr *C.struct_DengjenVoice
}

// AudioInfo describes a voice's output audio format.
type AudioInfo struct {
	SampleRate  uint32
	NumChannels uint32
	SampleWidth uint32
}

// LoadVoice loads a voice model from a manifest at configPath (the same
// config.json/.onnx.json shape every other dengjen-tts frontend accepts).
func LoadVoice(configPath string) (*Voice, error) {
	cPath := C.CString(configPath)
	defer C.free(unsafe.Pointer(cPath))

	var cErr C.struct_ExternError
	ptr := C.libdengjenLoadVoiceFromConfigPath(C.FfiStr(cPath), &cErr)
	if err := checkError(cErr); err != nil {
		return nil, err
	}
	v := &Voice{ptr: ptr}
	runtime.SetFinalizer(v, (*Voice).Close)
	return v, nil
}

// Close releases the voice's native resources. Safe to call more than once.
func (v *Voice) Close() error {
	if v.ptr == nil {
		return nil
	}
	// SAFETY (Go side): libdengjenUnloadDengjenVoice requires voice_ptr to be
	// non-null and well-aligned, which holds here since it was just produced
	// by a successful libdengjenLoadVoiceFromConfigPath call and hasn't been
	// freed yet (guarded by the nil-check above).
	C.libdengjenUnloadDengjenVoice(v.ptr)
	v.ptr = nil
	runtime.SetFinalizer(v, nil)
	return nil
}

// AudioInfo returns this voice's output audio format.
func (v *Voice) AudioInfo() (AudioInfo, error) {
	if v.ptr == nil {
		return AudioInfo{}, &FFIError{Message: "voice is closed"}
	}
	var cInfo C.struct_AudioInfo
	var cErr C.struct_ExternError
	C.libdengjenGetAudioInfo(v.ptr, &cInfo, &cErr)
	if err := checkError(cErr); err != nil {
		return AudioInfo{}, err
	}
	return AudioInfo{
		SampleRate:  uint32(cInfo.sample_rate),
		NumChannels: uint32(cInfo.num_channels),
		SampleWidth: uint32(cInfo.sample_width),
	}, nil
}

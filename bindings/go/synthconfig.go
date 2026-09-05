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

type SynthConfig struct {
	Speaker     uint32
	LengthScale float32
	NoiseScale  float32
	NoiseW      float32
}

func (v *Voice) PiperDefaultSynthConfig() (SynthConfig, error) {
	if v.ptr == nil {
		return SynthConfig{}, ErrVoiceClosed
	}
	var cErr C.struct_ExternError
	cCfg := C.libdengjenGetPiperDefaultSynthConfig(v.ptr, &cErr)
	if err := checkError(cErr); err != nil {
		return SynthConfig{}, err
	}
	defer C.libdengjenFreePiperSynthConfig(cCfg)
	runtime.KeepAlive(v)
	return SynthConfig{
		Speaker:     uint32(cCfg.speaker),
		LengthScale: float32(cCfg.length_scale),
		NoiseScale:  float32(cCfg.noise_scale),
		NoiseW:      float32(cCfg.noise_w),
	}, nil
}

func (v *Voice) SetPiperSynthConfig(cfg SynthConfig) error {
	if v.ptr == nil {
		return ErrVoiceClosed
	}
	cCfg := C.struct_PiperSynthConfig{
		speaker:      C.uint32_t(cfg.Speaker),
		length_scale: C.float(cfg.LengthScale),
		noise_scale:  C.float(cfg.NoiseScale),
		noise_w:      C.float(cfg.NoiseW),
	}
	var cErr C.struct_ExternError
	C.libdengjenSetPiperSynthConfig(v.ptr, cCfg, &cErr)
	runtime.KeepAlive(v)
	return checkError(cErr)
}

func (v *Voice) SetSynthesisParameter(key string, value float32) error {
	if v.ptr == nil {
		return ErrVoiceClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	var cErr C.struct_ExternError
	C.libdengjenSetSynthesisParameter(v.ptr, C.FfiStr(cKey), C.float(value), &cErr)
	runtime.KeepAlive(v)
	return checkError(cErr)
}

func (v *Voice) SynthesisParameter(key string) (value float32, ok bool, err error) {
	if v.ptr == nil {
		return 0, false, ErrVoiceClosed
	}
	cKey := C.CString(key)
	defer C.free(unsafe.Pointer(cKey))
	var cValue C.float
	var cErr C.struct_ExternError
	found := C.libdengjenGetSynthesisParameter(v.ptr, C.FfiStr(cKey), &cValue, &cErr)
	if err := checkError(cErr); err != nil {
		return 0, false, err
	}
	runtime.KeepAlive(v)
	return float32(cValue), bool(found), nil
}

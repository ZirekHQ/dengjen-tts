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




type Voice struct {
	ptr *C.struct_DengjenVoice
}


type AudioInfo struct {
	SampleRate  uint32
	NumChannels uint32
	SampleWidth uint32
}



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


func (v *Voice) Close() error {
	if v.ptr == nil {
		return nil
	}
	
	
	
	
	C.libdengjenUnloadDengjenVoice(v.ptr)
	v.ptr = nil
	runtime.SetFinalizer(v, nil)
	return nil
}

func (v *Voice) AudioInfo() (AudioInfo, error) {
	if v.ptr == nil {
		return AudioInfo{}, ErrVoiceClosed
	}
	var cInfo C.struct_AudioInfo
	var cErr C.struct_ExternError
	C.libdengjenGetAudioInfo(v.ptr, &cInfo, &cErr)
	if err := checkError(cErr); err != nil {
		return AudioInfo{}, err
	}
	runtime.KeepAlive(v)
	return AudioInfo{
		SampleRate:  uint32(cInfo.sample_rate),
		NumChannels: uint32(cInfo.num_channels),
		SampleWidth: uint32(cInfo.sample_width),
	}, nil
}

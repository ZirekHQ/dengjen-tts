package dengjen

/*
#include "libdengjen.h"
*/
import "C"
import "unsafe"

// FFIError wraps an error reported by libdengjen: a numeric code (matching the
// ERROR_CODE_* constants in libdengjen.h) and a human-readable message.
type FFIError struct {
	Code    int32
	Message string
}

func (e *FFIError) Error() string {
	return e.Message
}

// checkError converts a C ExternError into a Go error, freeing the C-owned
// message string if present. Returns nil for a success code (0).
func checkError(cErr C.struct_ExternError) error {
	if cErr.code == 0 {
		return nil
	}
	msg := C.GoString(cErr.message)
	if cErr.message != nil {
		C.libdengjenFreeString((*C.int8_t)(unsafe.Pointer(cErr.message)))
	}
	return &FFIError{Code: int32(cErr.code), Message: msg}
}

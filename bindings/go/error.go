package dengjen

/*
#include "libdengjen.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

// FFIError wraps an error reported by libdengjen: a numeric code (matching the
// ERROR_CODE_* constants in libdengjen.h) and a human-readable message.
type FFIError struct {
	Code    int32
	Message string
}

func (e *FFIError) Error() string {
	if e.Message == "" {
		return fmt.Sprintf("libdengjen error %d", e.Code)
	}
	return e.Message
}

// ErrVoiceClosed is returned by Voice methods once the voice has been Closed.
// Code -2 is a Go-binding-only sentinel; it doesn't collide with any
// libdengjen ErrorCode (SUCCESS=0, PANIC=-1, INVALID_HANDLE=-1000, and the
// domain error_codes are all >= 16).
var ErrVoiceClosed = &FFIError{Code: -2, Message: "voice is closed"}

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

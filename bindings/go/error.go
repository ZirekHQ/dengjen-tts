package dengjen

/*
#include <stdlib.h>
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

// smokeTestLoadVoiceFromConfigPath is a thin, unexported wrapper around the
// raw C call, used only by dengjen_test.go's cgo-boundary smoke test. Go
// does not permit "import \"C\"" inside a _test.go file (see go/build's
// badGoFile check for isTest files), so any code that touches cgo types or
// calls must live in a regular .go file; the test itself calls this
// function without needing to import "C". Later tasks that add the real
// LoadVoice/Voice API will likely subsume and remove this helper.
func smokeTestLoadVoiceFromConfigPath(configPath string) (unsafe.Pointer, error) {
	cPath := C.CString(configPath)
	defer C.free(unsafe.Pointer(cPath))

	var cErr C.struct_ExternError
	ptr := C.libdengjenLoadVoiceFromConfigPath(C.FfiStr(cPath), &cErr)
	return unsafe.Pointer(ptr), checkError(cErr)
}

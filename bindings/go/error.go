package dengjen

/*
#include "libdengjen.h"
*/
import "C"
import (
	"fmt"
)



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





var ErrVoiceClosed = &FFIError{Code: -2, Message: "voice is closed"}



func checkError(cErr C.struct_ExternError) error {
	if cErr.code == 0 {
		return nil
	}
	msg := C.GoString(cErr.message)
	if cErr.message != nil {
		C.libdengjenFreeString(cErr.message)
	}
	return &FFIError{Code: int32(cErr.code), Message: msg}
}

package dev.dengjen;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;

/** Converts a native ExternError out-param into a thrown DengjenException, freeing the C-owned message string. */
final class ErrorChecks {
    private ErrorChecks() {}

    static void checkAndThrow(MemorySegment externError) {
        int code = externError.get(ValueLayout.JAVA_INT, DengjenLayouts.EXTERN_ERROR_CODE_OFFSET);
        if (code == 0) {
            return;
        }
        MemorySegment messagePtr = externError.get(ValueLayout.ADDRESS, DengjenLayouts.EXTERN_ERROR_MESSAGE_OFFSET);
        String message = readAndFreeMessage(messagePtr);
        throw new DengjenException(ErrorCode.fromCode(code), message);
    }

    // A pointer returned from a native call comes back as a zero-length
    // MemorySegment; it must be reinterpreted to a usable size before it can
    // be dereferenced.
    static String readAndFreeMessage(MemorySegment messagePtr) {
        if (messagePtr.equals(MemorySegment.NULL)) {
            return null;
        }
        String message = messagePtr.reinterpret(Long.MAX_VALUE).getString(0);
        try {
            DengjenLib.FREE_STRING.invokeExact(messagePtr);
        } catch (Throwable t) {
            throw new IllegalStateException("libdengjenFreeString downcall failed", t);
        }
        return message;
    }
}

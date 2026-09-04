package io.github.zirekhq.dengjen;

import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;





final class ErrorChecks {
  private ErrorChecks() {}

  static void checkAndThrow(MemorySegment externError) {
    int code = externError.get(ValueLayout.JAVA_INT, DengjenLayouts.EXTERN_ERROR_CODE_OFFSET);
    if (code == 0) {
      return;
    }
    MemorySegment messagePtr =
        externError.get(ValueLayout.ADDRESS, DengjenLayouts.EXTERN_ERROR_MESSAGE_OFFSET);
    String message = readAndFreeMessage(messagePtr);
    throw new DengjenException(ErrorCode.fromCode(code), message);
  }

  
  
  
  static String readAndFreeMessage(MemorySegment messagePtr) {
    if (messagePtr.equals(MemorySegment.NULL)) {
      return null;
    }
    String message = readMessage(messagePtr);
    try {
      DengjenLib.FREE_STRING.invokeExact(messagePtr);
    } catch (Throwable t) {
      throw new IllegalStateException("libdengjenFreeString downcall failed", t);
    }
    return message;
  }

  
  
  
  static String readMessage(MemorySegment messagePtr) {
    if (messagePtr.equals(MemorySegment.NULL)) {
      return null;
    }
    return messagePtr.reinterpret(Long.MAX_VALUE).getString(0);
  }
}

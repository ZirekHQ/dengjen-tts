package io.github.zirekhq.dengjen;

import static org.junit.jupiter.api.Assertions.assertNull;

import java.lang.foreign.MemorySegment;
import org.junit.jupiter.api.Test;

class ErrorChecksTest {
  @Test
  void readMessageReturnsNullForANullPointerWithoutTouchingNativeMemory() {
    assertNull(ErrorChecks.readMessage(MemorySegment.NULL));
  }

  @Test
  void readAndFreeMessageReturnsNullForANullPointerWithoutTouchingNativeMemory() {
    assertNull(ErrorChecks.readAndFreeMessage(MemorySegment.NULL));
  }

  @Test
  void checkAndThrowIsANoopForASuccessCode() {
    var externError =
        java.lang.foreign.Arena.ofAuto().allocate(DengjenLayouts.EXTERN_ERROR.byteSize());
    externError.set(
        java.lang.foreign.ValueLayout.JAVA_INT, DengjenLayouts.EXTERN_ERROR_CODE_OFFSET, 0);
    ErrorChecks.checkAndThrow(externError);
  }
}

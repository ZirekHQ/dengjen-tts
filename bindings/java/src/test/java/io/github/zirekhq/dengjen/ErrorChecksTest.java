package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import java.lang.foreign.MemorySegment;
import org.junit.jupiter.api.Test;

class ErrorChecksTest {
  @Test
  void readMessageReturnsNullForANullPointerWithoutTouchingNativeMemory() {
    assertThat(ErrorChecks.readMessage(MemorySegment.NULL)).isNull();
  }

  @Test
  void readAndFreeMessageReturnsNullForANullPointerWithoutTouchingNativeMemory() {
    assertThat(ErrorChecks.readAndFreeMessage(MemorySegment.NULL)).isNull();
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

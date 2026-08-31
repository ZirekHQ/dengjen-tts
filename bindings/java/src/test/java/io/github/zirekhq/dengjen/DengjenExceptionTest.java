package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

class DengjenExceptionTest {
  @Test
  void usesTheGivenMessageWhenPresent() {
    var ex = new DengjenException(ErrorCode.NULL_POINTER, "voice_ptr was null");
    assertThat(ex.getMessage()).isEqualTo("voice_ptr was null");
    assertThat(ex.errorCode()).isEqualTo(ErrorCode.NULL_POINTER);
  }

  @Test
  void fallsBackToADescriptiveMessageWhenNoneIsGiven() {
    var ex = new DengjenException(ErrorCode.UNKNOWN_ERROR, "");
    assertThat(ex.getMessage()).isEqualTo("UNKNOWN_ERROR (no message from libdengjen)");
  }

  @Test
  void fallsBackToADescriptiveMessageWhenMessageIsNull() {
    var ex = new DengjenException(ErrorCode.UNKNOWN_ERROR, null);
    assertThat(ex.getMessage()).isEqualTo("UNKNOWN_ERROR (no message from libdengjen)");
  }
}

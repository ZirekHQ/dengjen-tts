package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

class ErrorCodeTest {
  @Test
  void mapsKnownCodes() {
    assertThat(ErrorCode.fromCode(22)).isEqualTo(ErrorCode.NULL_POINTER);
    assertThat(ErrorCode.fromCode(-1)).isEqualTo(ErrorCode.PANIC);
    assertThat(ErrorCode.fromCode(-1000)).isEqualTo(ErrorCode.INVALID_HANDLE);
    assertThat(ErrorCode.fromCode(25)).isEqualTo(ErrorCode.UNSUPPORTED_OPERATION);
  }

  @Test
  void fallsBackToUnknownForAnUnrecognizedCode() {
    assertThat(ErrorCode.fromCode(999)).isEqualTo(ErrorCode.UNKNOWN_ERROR);
  }

  @Test
  void codeRoundTrips() {
    assertThat(ErrorCode.NULL_POINTER.code()).isEqualTo(22);
  }
}

package io.github.zirekhq.dengjen;

public enum ErrorCode {
  PANIC(-1),
  INVALID_HANDLE(-1000),
  INVALID_SYNTHESIS_MODE(16),
  FAILED_TO_LOAD_RESOURCE(17),
  PHONEMIZATION_ERROR(18),
  OPERATION_ERROR(19),
  INVALID_UTF8_SEQUENCE(20),
  UNKNOWN_ERROR(21),
  NULL_POINTER(22),
  INFERENCE_ERROR(23),
  INVALID_CONFIGURATION(24),
  UNSUPPORTED_OPERATION(25);

  private final int code;

  ErrorCode(int code) {
    this.code = code;
  }

  public int code() {
    return code;
  }

  static ErrorCode fromCode(int code) {
    for (ErrorCode value : values()) {
      if (value.code == code) {
        return value;
      }
    }
    return UNKNOWN_ERROR;
  }
}

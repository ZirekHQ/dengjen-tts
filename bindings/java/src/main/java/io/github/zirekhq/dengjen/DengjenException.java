package io.github.zirekhq.dengjen;


public final class DengjenException extends RuntimeException {
  private final ErrorCode errorCode;

  DengjenException(ErrorCode errorCode, String message) {
    super(
        message != null && !message.isEmpty()
            ? message
            : errorCode.name() + " (no message from libdengjen)");
    this.errorCode = errorCode;
  }

  public ErrorCode errorCode() {
    return errorCode;
  }
}

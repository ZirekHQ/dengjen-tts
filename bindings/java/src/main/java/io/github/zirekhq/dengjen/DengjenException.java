package io.github.zirekhq.dengjen;

/** An error reported by libdengjen: an {@link ErrorCode} plus a human-readable message. */
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

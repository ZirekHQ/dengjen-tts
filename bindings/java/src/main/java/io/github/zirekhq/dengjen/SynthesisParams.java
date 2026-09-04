package io.github.zirekhq.dengjen;





public record SynthesisParams(
    SynthesisMode mode,
    int rate,
    int volume,
    int pitch,
    int appendedSilenceMs,
    boolean nonblocking) {

  public SynthesisParams {
    requireRange(rate, "rate");
    requireRange(volume, "volume");
    requireRange(pitch, "pitch");
  }

  
  public SynthesisParams(
      SynthesisMode mode, int rate, int volume, int pitch, int appendedSilenceMs) {
    this(mode, rate, volume, pitch, appendedSilenceMs, false);
  }

  private static void requireRange(int value, String name) {
    if (value < 0 || value > 100) {
      throw new IllegalArgumentException(name + " must be 0-100, got " + value);
    }
  }
}

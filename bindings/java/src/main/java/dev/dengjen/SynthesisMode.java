package dev.dengjen;

/** Mirrors the SYNTH_MODE_* constants in libdengjen.h. */
public enum SynthesisMode {
  LAZY(0),
  PARALLEL(1),
  REALTIME(2);

  private final int value;

  SynthesisMode(int value) {
    this.value = value;
  }

  int value() {
    return value;
  }
}

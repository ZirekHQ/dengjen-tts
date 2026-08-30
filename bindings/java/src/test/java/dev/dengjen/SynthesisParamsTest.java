package dev.dengjen;

import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class SynthesisParamsTest {
  @Test
  void rejectsAnOutOfRangeRate() {
    assertThrows(
        IllegalArgumentException.class,
        () -> new SynthesisParams(SynthesisMode.LAZY, 101, 0, 0, 0));
  }

  @Test
  void defaultsNonblockingToFalse() {
    var params = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0);
    assertFalse(params.nonblocking());
  }
}

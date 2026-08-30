package io.github.zirekhq.dengjen;

import static org.junit.jupiter.api.Assertions.assertEquals;
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
  void rejectsOutOfRangeRateBelowZero() {
    assertThrows(
        IllegalArgumentException.class, () -> new SynthesisParams(SynthesisMode.LAZY, -1, 0, 0, 0));
  }

  @Test
  void rejectsOutOfRangeVolume() {
    assertThrows(
        IllegalArgumentException.class, () -> new SynthesisParams(SynthesisMode.LAZY, 0, -1, 0, 0));
    assertThrows(
        IllegalArgumentException.class,
        () -> new SynthesisParams(SynthesisMode.LAZY, 0, 101, 0, 0));
  }

  @Test
  void rejectsOutOfRangePitch() {
    assertThrows(
        IllegalArgumentException.class, () -> new SynthesisParams(SynthesisMode.LAZY, 0, 0, -1, 0));
    assertThrows(
        IllegalArgumentException.class,
        () -> new SynthesisParams(SynthesisMode.LAZY, 0, 0, 101, 0));
  }

  @Test
  void acceptsBoundaryRateValues() {
    assertEquals(0, new SynthesisParams(SynthesisMode.LAZY, 0, 0, 0, 0).rate());
    assertEquals(100, new SynthesisParams(SynthesisMode.LAZY, 100, 0, 0, 0).rate());
  }

  @Test
  void acceptsBoundaryVolumeValues() {
    assertEquals(0, new SynthesisParams(SynthesisMode.LAZY, 0, 0, 0, 0).volume());
    assertEquals(100, new SynthesisParams(SynthesisMode.LAZY, 0, 100, 0, 0).volume());
  }

  @Test
  void acceptsBoundaryPitchValues() {
    assertEquals(0, new SynthesisParams(SynthesisMode.LAZY, 0, 0, 0, 0).pitch());
    assertEquals(100, new SynthesisParams(SynthesisMode.LAZY, 0, 0, 100, 0).pitch());
  }

  @Test
  void defaultsNonblockingToFalse() {
    var params = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0);
    assertFalse(params.nonblocking());
  }
}

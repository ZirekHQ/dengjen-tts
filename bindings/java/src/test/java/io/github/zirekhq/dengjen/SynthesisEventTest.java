package io.github.zirekhq.dengjen;

import static org.junit.jupiter.api.Assertions.assertArrayEquals;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertNotEquals;
import static org.junit.jupiter.api.Assertions.assertNotSame;
import static org.junit.jupiter.api.Assertions.assertNull;
import static org.junit.jupiter.api.Assertions.assertTrue;

import org.junit.jupiter.api.Test;

class SynthesisEventTest {
  @Test
  void equalsComparesDataByContentNotReference() {
    var a = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    var b = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertEquals(a, b);
    assertEquals(a.hashCode(), b.hashCode());
  }

  @Test
  void equalsDetectsDifferingDataContent() {
    var a = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    var b = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 4}, null);
    assertNotEquals(a, b);
  }

  @Test
  void equalsComparesTypeAndError() {
    var error = new DengjenException(ErrorCode.PANIC, "boom");
    var a = new SynthesisEvent(EventType.ERROR, new byte[0], error);
    var b = new SynthesisEvent(EventType.FINISHED, new byte[0], error);
    assertNotEquals(a, b);
  }

  @Test
  void toStringIncludesTheDataContentNotAnArrayDump() {
    var event = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertTrue(event.toString().contains("[1, 2, 3]"));
  }

  @Test
  void constructorDefensivelyCopiesTheDataArray() {
    byte[] original = {1, 2, 3};
    var event = new SynthesisEvent(EventType.SPEECH, original, null);

    original[0] = 99;

    assertArrayEquals(new byte[] {1, 2, 3}, event.data());
  }

  @Test
  void dataAccessorReturnsAFreshCopyEachTime() {
    var event = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);

    byte[] first = event.data();
    first[0] = 99;

    assertArrayEquals(new byte[] {1, 2, 3}, event.data());
    assertNotSame(first, event.data());
  }

  @Test
  void dataMayBeNull() {
    var event = new SynthesisEvent(EventType.FINISHED, null, null);
    assertNull(event.data());
  }
}

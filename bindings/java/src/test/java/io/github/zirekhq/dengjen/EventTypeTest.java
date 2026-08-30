package io.github.zirekhq.dengjen;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

import org.junit.jupiter.api.Test;

class EventTypeTest {
  @Test
  void fromValueMapsKnownCodesToEnumConstants() {
    assertEquals(EventType.SPEECH, EventType.fromValue(0));
    assertEquals(EventType.FINISHED, EventType.fromValue(1));
    assertEquals(EventType.ERROR, EventType.fromValue(2));
  }

  @Test
  void fromValueThrowsForUnrecognizedCode() {
    assertThrows(IllegalArgumentException.class, () -> EventType.fromValue(99));
  }
}

package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import org.junit.jupiter.api.Test;

class EventTypeTest {
  @Test
  void fromValueMapsKnownCodesToEnumConstants() {
    assertThat(EventType.fromValue(0)).isEqualTo(EventType.SPEECH);
    assertThat(EventType.fromValue(1)).isEqualTo(EventType.FINISHED);
    assertThat(EventType.fromValue(2)).isEqualTo(EventType.ERROR);
  }

  @Test
  void fromValueThrowsForUnrecognizedCode() {
    assertThatThrownBy(() -> EventType.fromValue(99)).isInstanceOf(IllegalArgumentException.class);
  }
}

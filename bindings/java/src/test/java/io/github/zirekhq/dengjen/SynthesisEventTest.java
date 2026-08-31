package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

class SynthesisEventTest {
  @Test
  void equalsComparesDataByContentNotReference() {
    var a = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    var b = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertThat(a).isEqualTo(b);
    assertThat(a.hashCode()).isEqualTo(b.hashCode());
  }

  @Test
  void equalsDetectsDifferingDataContent() {
    var a = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    var b = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 4}, null);
    assertThat(a).isNotEqualTo(b);
  }

  @Test
  void equalsComparesTypeAndError() {
    var error = new DengjenException(ErrorCode.PANIC, "boom");
    var a = new SynthesisEvent(EventType.ERROR, new byte[0], error);
    var b = new SynthesisEvent(EventType.FINISHED, new byte[0], error);
    assertThat(a).isNotEqualTo(b);
  }

  @Test
  void toStringIncludesTheDataContentNotAnArrayDump() {
    var event = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertThat(event.toString()).contains("[1, 2, 3]");
  }

  @Test
  void constructorDefensivelyCopiesTheDataArray() {
    byte[] original = {1, 2, 3};
    var event = new SynthesisEvent(EventType.SPEECH, original, null);

    original[0] = 99;

    assertThat(event.data()).containsExactly(1, 2, 3);
  }

  @Test
  void dataAccessorReturnsAFreshCopyEachTime() {
    var event = new SynthesisEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);

    byte[] first = event.data();
    first[0] = 99;

    assertThat(event.data()).containsExactly(1, 2, 3);
    assertThat(first).isNotSameAs(event.data());
  }

  @Test
  void dataMayBeNull() {
    var event = new SynthesisEvent(EventType.FINISHED, null, null);
    assertThat(event.data()).isNull();
  }
}

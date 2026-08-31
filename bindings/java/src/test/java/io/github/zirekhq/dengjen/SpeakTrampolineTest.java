package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import org.junit.jupiter.api.Test;

class SpeakTrampolineTest {
  @Test
  void decodedEventEqualsIsReflexive() {
    var a = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertThat(a).isEqualTo(a);
  }

  @Test
  void decodedEventEqualsRejectsAnObjectOfADifferentType() {
    var a = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertThat(a).isNotEqualTo("not a DecodedEvent");
  }

  @Test
  void decodedEventEqualsComparesDataByContentNotReference() {
    var a = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    var b = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertThat(a).isEqualTo(b);
    assertThat(a.hashCode()).isEqualTo(b.hashCode());
  }

  @Test
  void decodedEventEqualsDetectsDifferingDataContent() {
    var a = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    var b = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 4}, null);
    assertThat(a).isNotEqualTo(b);
  }

  @Test
  void decodedEventEqualsComparesTypeAndError() {
    var error = new DengjenException(ErrorCode.PANIC, "boom");
    var a = new SpeakTrampoline.DecodedEvent(EventType.ERROR, new byte[0], error);
    var b = new SpeakTrampoline.DecodedEvent(EventType.FINISHED, new byte[0], error);
    assertThat(a).isNotEqualTo(b);
  }

  @Test
  void decodedEventToStringIncludesTheDataContentNotAnArrayDump() {
    var decoded = new SpeakTrampoline.DecodedEvent(EventType.SPEECH, new byte[] {1, 2, 3}, null);
    assertThat(decoded.toString()).contains("[1, 2, 3]");
  }
}

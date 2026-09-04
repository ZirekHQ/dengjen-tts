package io.github.zirekhq.dengjen;

import java.util.Arrays;
import java.util.Objects;

public record SynthesisEvent(EventType type, byte[] data, DengjenException error) {

  public SynthesisEvent {
    data = data == null ? null : data.clone();
  }

  @Override
  public byte[] data() {
    return data == null ? null : data.clone();
  }

  @Override
  public boolean equals(Object obj) {
    if (this == obj) {
      return true;
    }
    if (!(obj instanceof SynthesisEvent other)) {
      return false;
    }
    return type == other.type
        && Arrays.equals(data, other.data)
        && Objects.equals(error, other.error);
  }

  @Override
  public int hashCode() {
    return Objects.hash(type, Arrays.hashCode(data), error);
  }

  @Override
  public String toString() {
    return "SynthesisEvent[type="
        + type
        + ", data="
        + Arrays.toString(data)
        + ", error="
        + error
        + "]";
  }
}

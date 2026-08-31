package io.github.zirekhq.dengjen;

import java.util.Arrays;
import java.util.Objects;

/** One event delivered during a streaming speak() call. */
public record SynthesisEvent(EventType type, byte[] data, DengjenException error) {
  // Defensively copy on the way in and back out: `data` would otherwise be a mutable array a
  // caller could reach into and change out from under this (supposedly immutable) record, or
  // that this record's own array is shared into if the caller retains it.
  public SynthesisEvent {
    data = data == null ? null : data.clone();
  }

  @Override
  public byte[] data() {
    return data == null ? null : data.clone();
  }

  // Records derive equals/hashCode/toString field-by-field, which for an array field means
  // reference identity and a `[B@...` dump instead of comparing/printing its content -- override
  // all three to use the array's actual bytes.
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

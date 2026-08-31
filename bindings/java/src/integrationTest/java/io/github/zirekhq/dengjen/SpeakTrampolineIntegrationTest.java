package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import org.junit.jupiter.api.Test;

/**
 * Exercises SpeakTrampoline.invoke directly against synthetic native events, covering defensive
 * paths a real speak() call can't reliably (or safely) trigger: an event arriving for a trampoline
 * that's already been released, and a malformed/undersized event segment. Both synthetic segments
 * are constructed to be safe for the real native free call to process -- either matching the exact
 * shape libdengjen itself produces for a real empty event, or too small for the JVM's Foreign
 * Function & Memory API to ever hand to native code at all (it bounds-checks in Java before any
 * downcall is attempted).
 */
class SpeakTrampolineIntegrationTest {
  @Test
  void invokeStopsTheStreamWhenNoTrampolineIsRegisteredForTheEventsUserData() {
    try (Arena arena = Arena.ofConfined()) {
      MemorySegment event = arena.allocate(DengjenLayouts.SYNTHESIS_EVENT);
      // SYNTH_EVENT_FINISHED=1 (libdengjen.h): a genuine "stream ended, no payload" event -- the
      // realistic shape of a late event arriving after this call's trampoline was already
      // released (e.g. the handler returned false and stopped the stream, but a
      // still-in-flight event was already queued on the native side).
      event.set(ValueLayout.JAVA_INT, DengjenLayouts.SYNTHESIS_EVENT_TYPE_OFFSET, 1);
      event.set(ValueLayout.JAVA_LONG, DengjenLayouts.SYNTHESIS_EVENT_LEN_OFFSET, 0L);
      // A dangling-but-non-null, well-aligned address, not the segment's zero-initialized
      // default: Rust never leaks a literal null data pointer (Vec/Box invariants forbid it,
      // even for an empty allocation) -- leak_bytes(Vec::new()) produces NonNull::dangling(),
      // which for u8 is exactly align_of::<u8>() == 1. This is bit-for-bit what a real empty
      // event's data pointer looks like, so libdengjenFreeSynthesisEvent frees it exactly as it
      // would a genuine one.
      event.set(
          ValueLayout.ADDRESS,
          DengjenLayouts.SYNTHESIS_EVENT_DATA_OFFSET,
          MemorySegment.ofAddress(1));
      event.set(
          ValueLayout.ADDRESS, DengjenLayouts.SYNTHESIS_EVENT_ERROR_PTR_OFFSET, MemorySegment.NULL);

      // An id no SpeakTrampoline.create() call in this process could ever have produced --
      // NEXT_ID starts at 1 and only ever increments.
      MemorySegment unregisteredUserData = MemorySegment.ofAddress(Long.MAX_VALUE);

      byte result = SpeakTrampoline.invoke(event, unregisteredUserData);

      assertThat(result)
          .as("no registered trampoline for this event -- the stream must be told to stop")
          .isEqualTo((byte) 1);
    }
  }

  @Test
  void invokeTreatsAZeroLengthSpeechEventAsCarryingNoPayload() {
    try (Arena arena = Arena.ofConfined()) {
      MemorySegment event = arena.allocate(DengjenLayouts.SYNTHESIS_EVENT);
      // SYNTH_EVENT_SPEECH=0, len=0: a real speak() call never emits this (every SPEECH chunk
      // libdengjen produces carries at least one sample), but the len>0 guard exists specifically
      // to make that assumption explicit rather than load-bearing -- worth pinning directly.
      event.set(ValueLayout.JAVA_INT, DengjenLayouts.SYNTHESIS_EVENT_TYPE_OFFSET, 0);
      event.set(ValueLayout.JAVA_LONG, DengjenLayouts.SYNTHESIS_EVENT_LEN_OFFSET, 0L);
      event.set(
          ValueLayout.ADDRESS,
          DengjenLayouts.SYNTHESIS_EVENT_DATA_OFFSET,
          MemorySegment.ofAddress(1));
      event.set(
          ValueLayout.ADDRESS, DengjenLayouts.SYNTHESIS_EVENT_ERROR_PTR_OFFSET, MemorySegment.NULL);

      SynthesisEvent[] received = new SynthesisEvent[1];
      SpeakTrampoline trampoline =
          SpeakTrampoline.create(
              event2 -> {
                received[0] = event2;
                return true;
              });
      try {
        byte result = SpeakTrampoline.invoke(event, trampoline.userData());
        assertThat(result)
            .as("SPEECH with wantsMore=true keeps the stream going")
            .isEqualTo((byte) 0);
        assertThat(received[0].data()).isEmpty();
      } finally {
        trampoline.release();
      }
    }
  }

  @Test
  void invokeTreatsAnErrorEventWithANullErrorPointerAsCarryingNoError() {
    try (Arena arena = Arena.ofConfined()) {
      MemorySegment event = arena.allocate(DengjenLayouts.SYNTHESIS_EVENT);
      // SYNTH_EVENT_ERROR=2 with a null error_ptr: real error events always carry a non-null
      // ExternError (see with_error in capi/src/lib.rs), but the null check exists specifically
      // to make that assumption explicit rather than load-bearing -- worth pinning directly.
      event.set(ValueLayout.JAVA_INT, DengjenLayouts.SYNTHESIS_EVENT_TYPE_OFFSET, 2);
      event.set(ValueLayout.JAVA_LONG, DengjenLayouts.SYNTHESIS_EVENT_LEN_OFFSET, 0L);
      event.set(
          ValueLayout.ADDRESS,
          DengjenLayouts.SYNTHESIS_EVENT_DATA_OFFSET,
          MemorySegment.ofAddress(1));
      event.set(
          ValueLayout.ADDRESS, DengjenLayouts.SYNTHESIS_EVENT_ERROR_PTR_OFFSET, MemorySegment.NULL);

      SynthesisEvent[] received = new SynthesisEvent[1];
      SpeakTrampoline trampoline =
          SpeakTrampoline.create(
              event2 -> {
                received[0] = event2;
                return true;
              });
      boolean[] released = {false};
      Runnable prevHook = SpeakTrampoline.testCallReleased.get();
      SpeakTrampoline.testCallReleased.set(() -> released[0] = true);
      try {
        byte result = SpeakTrampoline.invoke(event, trampoline.userData());
        // The return value tracks wantsMore alone, not terminal -- ERROR being terminal only
        // affects whether release() runs, which the hook below verifies directly.
        assertThat(result)
            .as("ERROR with wantsMore=true keeps the stream going")
            .isEqualTo((byte) 0);
        assertThat(received[0].error()).isNull();
        assertThat(released[0])
            .as("ERROR is terminal -- invoke() must release on its own")
            .isTrue();
      } finally {
        SpeakTrampoline.testCallReleased.set(prevHook);
        trampoline.release();
      }
    }
  }

  @Test
  void invokeSurvivesAnUndersizedEventSegmentAndStillStopsTheStream() {
    try (Arena arena = Arena.ofConfined()) {
      // 1 byte: too small to hold even the first field of a real SynthesisEvent. Every read
      // this triggers -- including the retry inside invoke's own defensive catch, which
      // attempts to free the same segment -- is an IndexOutOfBoundsException the JVM's Foreign
      // Function & Memory API raises in Java before any native call is dispatched, never a real
      // native fault.
      MemorySegment tooSmall = arena.allocate(1);
      MemorySegment unregisteredUserData = MemorySegment.ofAddress(Long.MAX_VALUE - 1);

      byte result = SpeakTrampoline.invoke(tooSmall, unregisteredUserData);

      assertThat(result)
          .as("a malformed event must degrade to \"stop the stream\", never propagate or crash")
          .isEqualTo((byte) 1);
    }
  }
}

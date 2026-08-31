package io.github.zirekhq.dengjen;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.atomic.AtomicLong;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Binds a Java SynthesisEventHandler to the native SpeechSynthesisCallback a speak() call streams
 * its events through.
 *
 * <p>This deliberately mirrors bindings/go's design -- one process-wide callback function pointer
 * plus a registry keyed by the call's opaque user_data -- rather than allocating a fresh upcall
 * stub per call and freeing it when the stream ends. A per-call stub cannot be freed safely: an
 * upcall stub is a HotSpot code blob owned by its Arena, and the only moment a streaming call knows
 * it has ended is *inside* the final upcall, i.e. while that blob's own frame is still on the
 * stack. Closing the Arena there frees the blob under a live frame, and the next stack walk over
 * that thread (any safepoint or handshake -- including the global handshake another thread's
 * shared-Arena close triggers) crashes the VM in vframeStreamCommon::next(). That is a reproducible
 * SIGSEGV, not a theoretical one.
 *
 * <p>So the stub lives in Arena.global() and is never freed, and the only per-call resource is a
 * registry entry -- exactly the cgo.Handle bindings/go allocates per call. Releasing that entry is
 * what {@link #release()} does, and it carries bindings/go's leak-on-early-stop fix verbatim.
 */
final class SpeakTrampoline {
  /**
   * Test hook: invoked synchronously right after a call's registry entry is released, on both the
   * natural-termination and early-stop paths. Null in production; only ever set from a test.
   * Mirrors bindings/go's testHandleDeleted, for the same reason: releasing the entry here is
   * already deterministic (no GC involved), but this hook still lets a test assert the *specific*
   * code path actually ran, not just that events stopped arriving.
   */
  static final AtomicReference<Runnable> testCallReleased = new AtomicReference<>();

  private static final Map<Long, SpeakTrampoline> REGISTRY = new ConcurrentHashMap<>();
  // Starts at 1: id 0 would marshal to a NULL user_data, which is what a
  // callback-less call (speakToFile) passes.
  private static final AtomicLong NEXT_ID = new AtomicLong(1);

  private static final MemorySegment STUB;

  static {
    MethodHandle invokeHandle;
    try {
      invokeHandle =
          MethodHandles.lookup()
              .findStatic(
                  SpeakTrampoline.class,
                  "invoke",
                  MethodType.methodType(byte.class, MemorySegment.class, MemorySegment.class));
    } catch (ReflectiveOperationException e) {
      throw new ExceptionInInitializerError(e);
    }
    STUB =
        Linker.nativeLinker()
            .upcallStub(
                invokeHandle,
                FunctionDescriptor.of(
                    ValueLayout.JAVA_BYTE, DengjenLayouts.SYNTHESIS_EVENT, ValueLayout.ADDRESS),
                Arena.global());
  }

  private final SynthesisEventHandler handler;
  private final long id;

  private SpeakTrampoline(SynthesisEventHandler handler, long id) {
    this.handler = handler;
    this.id = id;
  }

  /**
   * Registers handler for one speak() call. The caller owns the registration until the stream ends
   * or it calls {@link #release()}.
   */
  static SpeakTrampoline create(SynthesisEventHandler handler) {
    SpeakTrampoline trampoline = new SpeakTrampoline(handler, NEXT_ID.getAndIncrement());
    REGISTRY.put(trampoline.id, trampoline);
    return trampoline;
  }

  /** The single native SpeechSynthesisCallback every speak() call streams through. */
  static MemorySegment stubPointer() {
    return STUB;
  }

  /** This call's opaque user_data: the registry key the stub uses to find this trampoline again. */
  MemorySegment userData() {
    return MemorySegment.ofAddress(id);
  }

  /**
   * Releases this call's registry entry. Safe to call more than once; only the first call has an
   * effect.
   */
  void release() {
    if (REGISTRY.remove(id) != null) {
      Runnable hook = testCallReleased.get();
      if (hook != null) {
        hook.run();
      }
    }
  }

  // Invoked from native code on whatever thread libdengjen's synthesis
  // pipeline runs on -- must not let an exception escape across this FFI
  // boundary (undefined behavior), so a handler exception is caught here
  // and treated as "stop early" instead, the same contract bindings/go's
  // onEvent documents (must not panic across the boundary).
  private static byte invoke(MemorySegment eventSegment, MemorySegment userData) {
    SpeakTrampoline trampoline = null;
    boolean freed = false;
    try {
      trampoline = REGISTRY.get(userData.address());

      int eventType =
          eventSegment.get(ValueLayout.JAVA_INT, DengjenLayouts.SYNTHESIS_EVENT_TYPE_OFFSET);
      long len = eventSegment.get(ValueLayout.JAVA_LONG, DengjenLayouts.SYNTHESIS_EVENT_LEN_OFFSET);
      MemorySegment dataPtr =
          eventSegment.get(ValueLayout.ADDRESS, DengjenLayouts.SYNTHESIS_EVENT_DATA_OFFSET);
      MemorySegment errorPtr =
          eventSegment.get(ValueLayout.ADDRESS, DengjenLayouts.SYNTHESIS_EVENT_ERROR_PTR_OFFSET);

      EventType type = EventType.fromValue(eventType);
      byte[] data =
          (type == EventType.SPEECH && len > 0)
              ? dataPtr.reinterpret(len).toArray(ValueLayout.JAVA_BYTE)
              : new byte[0];
      DengjenException error = null;
      if (type == EventType.ERROR && !errorPtr.equals(MemorySegment.NULL)) {
        MemorySegment err = errorPtr.reinterpret(DengjenLayouts.EXTERN_ERROR.byteSize());
        int code = err.get(ValueLayout.JAVA_INT, DengjenLayouts.EXTERN_ERROR_CODE_OFFSET);
        MemorySegment messagePtr =
            err.get(ValueLayout.ADDRESS, DengjenLayouts.EXTERN_ERROR_MESSAGE_OFFSET);
        error = new DengjenException(ErrorCode.fromCode(code), ErrorChecks.readMessage(messagePtr));
      }

      // event was produced by exactly one SpeechSynthesisCallback invocation (this one) and
      // is freed here exactly once, per libdengjenFreeSynthesisEvent's documented contract --
      // which now includes the error message read above, so it must not be freed a second
      // time here (hence readMessage, not readAndFreeMessage).
      try {
        DengjenLib.FREE_SYNTHESIS_EVENT.invokeExact(eventSegment);
      } catch (Throwable t) {
        throw new IllegalStateException("libdengjenFreeSynthesisEvent downcall failed", t);
      }
      freed = true;

      if (trampoline == null) {
        return 1; // already released -- tell the stream to stop
      }

      boolean wantsMore = trampoline.handler.onEvent(new SynthesisEvent(type, data, error));

      // A Finished/Error event is the normal end of the stream, but the
      // handler returning false also ends it -- either way this is the
      // last time this trampoline runs for this call, so its registration
      // must be released here. The `||` (not two independent branches) is
      // load-bearing: a terminal event whose handler also returns false
      // must still only release once. Mirrors bindings/go's callback.go
      // `terminal || !wantsMore` fix exactly.
      boolean terminal = type == EventType.FINISHED || type == EventType.ERROR;
      if (terminal || !wantsMore) {
        trampoline.release();
      }
      return (byte) (wantsMore ? 0 : 1);
    } catch (Throwable t) {
      // Must not let anything escape across this FFI boundary (undefined behavior in native
      // code): an unrecognized event type, a handler-thrown exception or Error, a downcall
      // failure, or anything else unexpected all degrade to "stop the stream" instead of
      // crashing the process. The native event is freed here if the throw happened before
      // FREE_SYNTHESIS_EVENT above got a chance to run, and the registry entry is released
      // either way so a failure here can't leak it.
      if (!freed) {
        try {
          DengjenLib.FREE_SYNTHESIS_EVENT.invokeExact(eventSegment);
        } catch (Throwable freeFailure) {
          // Best-effort: already unwinding from t, and a downcall failure here would only
          // replace one unrecoverable-for-this-event problem with another.
        }
      }
      if (trampoline != null) {
        trampoline.release();
      }
      return 1;
    }
  }
}

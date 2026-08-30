package dev.dengjen;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.ref.Reference;
import java.util.concurrent.locks.ReentrantLock;

/**
 * A loaded dengjen-tts voice model. Not usable after {@link #close()}.
 *
 * Not safe to call close() concurrently with any other method on the same
 * Voice from another thread while that other call is in flight -- the same
 * contract libdengjenCancel/libdengjenUnloadDengjenVoice already document.
 * The lock below guards only the ptr field itself (making close() and a
 * post-close call race-free and idempotent); it does not serialize an
 * in-flight native call against a concurrent close().
 */
public final class Voice implements AutoCloseable {
    private final ReentrantLock lock = new ReentrantLock();
    private MemorySegment ptr;

    private Voice(MemorySegment ptr) {
        this.ptr = ptr;
    }

    /** Loads a voice model from a manifest at configPath (the same config.json/.onnx.json shape every other dengjen-tts frontend accepts). */
    public static Voice load(String configPath) {
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cPath = arena.allocateFrom(configPath);
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            MemorySegment ptr;
            try {
                ptr = (MemorySegment) DengjenLib.LOAD_VOICE_FROM_CONFIG_PATH.invokeExact(cPath, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenLoadVoiceFromConfigPath downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
            return new Voice(ptr);
        }
    }

    /** Releases this voice's native resources. Safe to call more than once. */
    @Override
    public void close() {
        lock.lock();
        try {
            if (ptr == null) {
                return;
            }
            MemorySegment closingPtr = ptr;
            ptr = null;
            try {
                DengjenLib.UNLOAD_DENGJEN_VOICE.invokeExact(closingPtr);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenUnloadDengjenVoice downcall failed", t);
            }
        } finally {
            lock.unlock();
        }
    }

    private MemorySegment requireOpenPtr() {
        lock.lock();
        try {
            if (ptr == null) {
                throw new DengjenException(ErrorCode.INVALID_HANDLE, "voice is closed");
            }
            return ptr;
        } finally {
            lock.unlock();
        }
    }

    /** Returns this voice's output audio format. */
    public AudioInfo getAudioInfo() {
        MemorySegment voicePtr = requireOpenPtr();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cInfo = arena.allocate(DengjenLayouts.AUDIO_INFO);
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            try {
                DengjenLib.GET_AUDIO_INFO.invokeExact(voicePtr, cInfo, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenGetAudioInfo downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
            return new AudioInfo(
                    cInfo.get(ValueLayout.JAVA_INT, DengjenLayouts.AUDIO_INFO_SAMPLE_RATE_OFFSET),
                    cInfo.get(ValueLayout.JAVA_INT, DengjenLayouts.AUDIO_INFO_NUM_CHANNELS_OFFSET),
                    cInfo.get(ValueLayout.JAVA_INT, DengjenLayouts.AUDIO_INFO_SAMPLE_WIDTH_OFFSET));
        } finally {
            // The Java analog of bindings/go's runtime.KeepAlive(v): ensures
            // this Voice (and so its ptr field) isn't treated as unreachable
            // until the native call above has returned.
            Reference.reachabilityFence(this);
        }
    }
}

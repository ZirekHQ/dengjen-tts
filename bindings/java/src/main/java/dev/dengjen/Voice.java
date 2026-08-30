package dev.dengjen;

import java.lang.foreign.Arena;
import java.lang.foreign.MemorySegment;
import java.lang.foreign.ValueLayout;
import java.lang.ref.Reference;
import java.util.Optional;
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

    /** Returns this voice's default synthesis parameters (Piper-family voices). */
    public PiperSynthConfig getDefaultSynthConfig() {
        MemorySegment voicePtr = requireOpenPtr();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            MemorySegment cfgPtr;
            try {
                cfgPtr = (MemorySegment) DengjenLib.GET_PIPER_DEFAULT_SYNTH_CONFIG.invokeExact(voicePtr, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenGetPiperDefaultSynthConfig downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
            MemorySegment cfg = cfgPtr.reinterpret(DengjenLayouts.PIPER_SYNTH_CONFIG.byteSize());
            PiperSynthConfig result = new PiperSynthConfig(
                    cfg.get(ValueLayout.JAVA_INT, DengjenLayouts.PIPER_SYNTH_CONFIG_SPEAKER_OFFSET),
                    cfg.get(ValueLayout.JAVA_FLOAT, DengjenLayouts.PIPER_SYNTH_CONFIG_LENGTH_SCALE_OFFSET),
                    cfg.get(ValueLayout.JAVA_FLOAT, DengjenLayouts.PIPER_SYNTH_CONFIG_NOISE_SCALE_OFFSET),
                    cfg.get(ValueLayout.JAVA_FLOAT, DengjenLayouts.PIPER_SYNTH_CONFIG_NOISE_W_OFFSET));
            try {
                DengjenLib.FREE_PIPER_SYNTH_CONFIG.invokeExact(cfgPtr);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenFreePiperSynthConfig downcall failed", t);
            }
            return result;
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Updates this voice's fallback synthesis parameters. */
    public void setSynthConfig(PiperSynthConfig config) {
        MemorySegment voicePtr = requireOpenPtr();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cfg = arena.allocate(DengjenLayouts.PIPER_SYNTH_CONFIG);
            cfg.set(ValueLayout.JAVA_INT, DengjenLayouts.PIPER_SYNTH_CONFIG_SPEAKER_OFFSET, config.speaker());
            cfg.set(ValueLayout.JAVA_FLOAT, DengjenLayouts.PIPER_SYNTH_CONFIG_LENGTH_SCALE_OFFSET, config.lengthScale());
            cfg.set(ValueLayout.JAVA_FLOAT, DengjenLayouts.PIPER_SYNTH_CONFIG_NOISE_SCALE_OFFSET, config.noiseScale());
            cfg.set(ValueLayout.JAVA_FLOAT, DengjenLayouts.PIPER_SYNTH_CONFIG_NOISE_W_OFFSET, config.noiseW());
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            try {
                DengjenLib.SET_PIPER_SYNTH_CONFIG.invokeExact(voicePtr, cfg, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenSetPiperSynthConfig downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Sets a single named parameter on the voice's fallback synthesis config (model-agnostic -- works for any backend's own parameter names). */
    public void setSynthesisParameter(String key, float value) {
        MemorySegment voicePtr = requireOpenPtr();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cKey = arena.allocateFrom(key);
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            try {
                DengjenLib.SET_SYNTHESIS_PARAMETER.invokeExact(voicePtr, cKey, value, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenSetSynthesisParameter downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /** Reads a single named parameter from the voice's fallback synthesis config. Empty if the key was never set (not an error). */
    public Optional<Float> getSynthesisParameter(String key) {
        MemorySegment voicePtr = requireOpenPtr();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cKey = arena.allocateFrom(key);
            MemorySegment outValue = arena.allocate(ValueLayout.JAVA_FLOAT);
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            boolean found;
            try {
                found = (boolean) DengjenLib.GET_SYNTHESIS_PARAMETER.invokeExact(voicePtr, cKey, outValue, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenGetSynthesisParameter downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
            return found ? Optional.of(outValue.get(ValueLayout.JAVA_FLOAT, 0)) : Optional.empty();
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Synthesizes text and writes it as a WAV file at outFilename. The
     * boolean return reports whether the file was written, independent of
     * any thrown exception (mirrors libdengjenSpeakToFile's own two-part
     * success signal). params.nonblocking() has no effect here --
     * speakToFile is always synchronous; it only applies to speak().
     */
    public boolean speakToFile(String text, SynthesisParams params, String outFilename) {
        MemorySegment voicePtr = requireOpenPtr();
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cText = arena.allocateFrom(text);
            MemorySegment cOutFilename = arena.allocateFrom(outFilename);
            MemorySegment cParams = SynthesisParamsMarshaller.allocate(arena, params, MemorySegment.NULL, MemorySegment.NULL);
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            byte wrote;
            try {
                wrote = (byte) DengjenLib.SPEAK_TO_FILE.invokeExact(voicePtr, cText, cParams, cOutFilename, outError);
            } catch (Throwable t) {
                throw new IllegalStateException("libdengjenSpeakToFile downcall failed", t);
            }
            ErrorChecks.checkAndThrow(outError);
            return wrote != 0;
        } finally {
            Reference.reachabilityFence(this);
        }
    }

    /**
     * Synthesizes text and streams the resulting audio to handler, one
     * event at a time. handler returns true to keep receiving events, false
     * to stop early. If handler runs to the natural end of the stream, the
     * last event delivered has type FINISHED or ERROR; if handler instead
     * returns false, the stream stops immediately at whatever event
     * triggered that, and no further event (in particular, no FINISHED) is
     * delivered for this call. If params.nonblocking() is true, speak()
     * returns immediately and handler continues firing from a
     * native-managed thread until the stream ends by either of those means.
     *
     * handler must not throw and must return promptly -- an exception
     * thrown from handler is caught here, treated as "stop early", and
     * never propagated back across this FFI boundary, since letting one
     * unwind across the native call frames that invoke it would be
     * undefined behavior. If handler needs to fail the caller, record the
     * failure itself and check for it after speak() returns.
     */
    public void speak(String text, SynthesisParams params, SynthesisEventHandler handler) {
        MemorySegment voicePtr = requireOpenPtr();
        SpeakTrampoline trampoline = SpeakTrampoline.create(handler);
        try (Arena arena = Arena.ofConfined()) {
            MemorySegment cText = arena.allocateFrom(text);
            MemorySegment cParams = SynthesisParamsMarshaller.allocate(
                    arena, params, SpeakTrampoline.stubPointer(), trampoline.userData());
            MemorySegment outError = arena.allocate(DengjenLayouts.EXTERN_ERROR);
            try {
                DengjenLib.SPEAK.invokeExact(voicePtr, cText, cParams, outError);
            } catch (Throwable t) {
                trampoline.release();
                throw new IllegalStateException("libdengjenSpeak downcall failed", t);
            }
            try {
                ErrorChecks.checkAndThrow(outError);
            } catch (DengjenException e) {
                // The trampoline is guaranteed to never fire for a call
                // that reports an error here (mirrors bindings/go's
                // speak.go: the callback never runs if libdengjenSpeak
                // itself reports an error), so this call site -- not the
                // trampoline -- owns releasing the registration in that case.
                trampoline.release();
                throw e;
            }
        } finally {
            Reference.reachabilityFence(this);
        }
    }
}

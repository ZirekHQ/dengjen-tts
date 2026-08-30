package dev.dengjen;

import java.lang.foreign.Arena;
import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;
import java.nio.file.Path;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BOOLEAN;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

/**
 * MethodHandles for every libdengjen* C function
 * (crates/frontends/capi/libdengjen.h). Loading this class loads the native
 * library; see {@link #load()} for the search path convention (mirrors
 * bindings/go/dengjen.go's cgo LDFLAGS: relative to the built module,
 * resolving ../../target/release from the process's working directory).
 */
final class DengjenLib {
    private static final Linker LINKER = Linker.nativeLinker();
    private static final SymbolLookup LOOKUP = load();

    private static SymbolLookup load() {
        String override = System.getProperty("dengjen.native.library.path");
        Path dir = override != null ? Path.of(override) : Path.of("..", "..", "target", "release");
        Path libPath = dir.resolve(System.mapLibraryName("libdengjen"));
        return SymbolLookup.libraryLookup(libPath, Arena.global());
    }

    private static MethodHandle handle(String symbol, FunctionDescriptor descriptor) {
        return LINKER.downcallHandle(
                LOOKUP.find(symbol).orElseThrow(() -> new IllegalStateException("missing symbol: " + symbol)),
                descriptor);
    }

    static final MethodHandle FREE_STRING =
            handle("libdengjenFreeString", FunctionDescriptor.ofVoid(ADDRESS));

    static final MethodHandle FREE_PIPER_SYNTH_CONFIG =
            handle("libdengjenFreePiperSynthConfig", FunctionDescriptor.ofVoid(ADDRESS));

    static final MethodHandle FREE_SYNTHESIS_EVENT =
            handle("libdengjenFreeSynthesisEvent", FunctionDescriptor.ofVoid(DengjenLayouts.SYNTHESIS_EVENT));

    static final MethodHandle LOAD_VOICE_FROM_CONFIG_PATH =
            handle("libdengjenLoadVoiceFromConfigPath", FunctionDescriptor.of(ADDRESS, ADDRESS, ADDRESS));

    static final MethodHandle UNLOAD_DENGJEN_VOICE =
            handle("libdengjenUnloadDengjenVoice", FunctionDescriptor.ofVoid(ADDRESS));

    static final MethodHandle GET_AUDIO_INFO =
            handle("libdengjenGetAudioInfo", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, ADDRESS));

    static final MethodHandle GET_PIPER_DEFAULT_SYNTH_CONFIG =
            handle("libdengjenGetPiperDefaultSynthConfig", FunctionDescriptor.of(ADDRESS, ADDRESS, ADDRESS));

    static final MethodHandle SET_PIPER_SYNTH_CONFIG =
            handle("libdengjenSetPiperSynthConfig",
                    FunctionDescriptor.ofVoid(ADDRESS, DengjenLayouts.PIPER_SYNTH_CONFIG, ADDRESS));

    static final MethodHandle SET_SYNTHESIS_PARAMETER =
            handle("libdengjenSetSynthesisParameter", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, JAVA_FLOAT, ADDRESS));

    static final MethodHandle GET_SYNTHESIS_PARAMETER =
            handle("libdengjenGetSynthesisParameter",
                    FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, ADDRESS, ADDRESS));

    static final MethodHandle SPEAK =
            handle("libdengjenSpeak", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, DengjenLayouts.SYNTHESIS_PARAMS, ADDRESS));

    static final MethodHandle CANCEL =
            handle("libdengjenCancel", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS));

    static final MethodHandle SPEAK_TO_FILE =
            handle("libdengjenSpeakToFile",
                    FunctionDescriptor.of(JAVA_BYTE, ADDRESS, ADDRESS, DengjenLayouts.SYNTHESIS_PARAMS, ADDRESS, ADDRESS));

    private DengjenLib() {}
}

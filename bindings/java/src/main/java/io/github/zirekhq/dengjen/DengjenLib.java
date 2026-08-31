package io.github.zirekhq.dengjen;

import static java.lang.foreign.ValueLayout.ADDRESS;
import static java.lang.foreign.ValueLayout.JAVA_BOOLEAN;
import static java.lang.foreign.ValueLayout.JAVA_BYTE;
import static java.lang.foreign.ValueLayout.JAVA_FLOAT;

import java.lang.foreign.FunctionDescriptor;
import java.lang.foreign.Linker;
import java.lang.foreign.SymbolLookup;
import java.lang.invoke.MethodHandle;

/**
 * MethodHandles for every libdengjen* C function (crates/frontends/capi/libdengjen.h). Loading
 * this class loads the native library; see {@link NativeLibraryLoader} for how it's found.
 */
final class DengjenLib {
  private static final Linker LINKER = Linker.nativeLinker();
  private static final SymbolLookup LOOKUP = NativeLibraryLoader.load();

  private static MethodHandle handle(String symbol, FunctionDescriptor descriptor) {
    return LINKER.downcallHandle(
        LOOKUP
            .find(symbol)
            .orElseThrow(() -> new IllegalStateException("missing symbol: " + symbol)),
        descriptor);
  }

  static final MethodHandle FREE_STRING =
      handle("libdengjenFreeString", FunctionDescriptor.ofVoid(ADDRESS));

  static final MethodHandle FREE_PIPER_SYNTH_CONFIG =
      handle("libdengjenFreePiperSynthConfig", FunctionDescriptor.ofVoid(ADDRESS));

  static final MethodHandle FREE_SYNTHESIS_EVENT =
      handle(
          "libdengjenFreeSynthesisEvent",
          FunctionDescriptor.ofVoid(DengjenLayouts.SYNTHESIS_EVENT));

  static final MethodHandle LOAD_VOICE_FROM_CONFIG_PATH =
      handle("libdengjenLoadVoiceFromConfigPath", FunctionDescriptor.of(ADDRESS, ADDRESS, ADDRESS));

  static final MethodHandle UNLOAD_DENGJEN_VOICE =
      handle("libdengjenUnloadDengjenVoice", FunctionDescriptor.ofVoid(ADDRESS));

  static final MethodHandle GET_AUDIO_INFO =
      handle("libdengjenGetAudioInfo", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, ADDRESS));

  static final MethodHandle GET_PIPER_DEFAULT_SYNTH_CONFIG =
      handle(
          "libdengjenGetPiperDefaultSynthConfig", FunctionDescriptor.of(ADDRESS, ADDRESS, ADDRESS));

  static final MethodHandle SET_PIPER_SYNTH_CONFIG =
      handle(
          "libdengjenSetPiperSynthConfig",
          FunctionDescriptor.ofVoid(ADDRESS, DengjenLayouts.PIPER_SYNTH_CONFIG, ADDRESS));

  static final MethodHandle SET_SYNTHESIS_PARAMETER =
      handle(
          "libdengjenSetSynthesisParameter",
          FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, JAVA_FLOAT, ADDRESS));

  static final MethodHandle GET_SYNTHESIS_PARAMETER =
      handle(
          "libdengjenGetSynthesisParameter",
          FunctionDescriptor.of(JAVA_BOOLEAN, ADDRESS, ADDRESS, ADDRESS, ADDRESS));

  static final MethodHandle SPEAK =
      handle(
          "libdengjenSpeak",
          FunctionDescriptor.ofVoid(ADDRESS, ADDRESS, DengjenLayouts.SYNTHESIS_PARAMS, ADDRESS));

  static final MethodHandle CANCEL =
      handle("libdengjenCancel", FunctionDescriptor.ofVoid(ADDRESS, ADDRESS));

  static final MethodHandle SPEAK_TO_FILE =
      handle(
          "libdengjenSpeakToFile",
          FunctionDescriptor.of(
              JAVA_BYTE, ADDRESS, ADDRESS, DengjenLayouts.SYNTHESIS_PARAMS, ADDRESS, ADDRESS));

  private DengjenLib() {}
}

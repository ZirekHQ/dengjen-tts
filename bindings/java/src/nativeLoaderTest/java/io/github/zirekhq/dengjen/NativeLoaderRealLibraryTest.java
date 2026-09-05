package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;

import java.lang.foreign.SymbolLookup;
import org.junit.jupiter.api.Test;

class NativeLoaderRealLibraryTest {
  @Test
  void loadsTheNativeLibraryFromAClassifierJarOnTheClasspath() {
    assertThat(System.getProperty("dengjen.native.library.path"))
        .as("classifier-JAR test must not use the native library override")
        .isNull();

    SymbolLookup lookup = NativeLibraryLoader.load();

    assertThat(lookup.find("libdengjenFreeString")).isPresent();
  }
}

package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import java.io.IOException;
import java.nio.file.Files;
import org.junit.jupiter.api.Test;

class NativeLibraryLoaderTest {
  @Test
  void extractsAClasspathResourceToATempFile() throws IOException {
    var path =
        NativeLibraryLoader.extractResource(
            "fixture/hello.txt", NativeLibraryLoaderTest.class.getClassLoader());

    assertThat(Files.readString(path)).isEqualTo("hello native world\n");
  }

  @Test
  void failsClearlyWhenTheResourceIsMissing() {
    assertThatThrownBy(
            () ->
                NativeLibraryLoader.extractResource(
                    "natives/does-not-exist/libdengjen.so",
                    NativeLibraryLoaderTest.class.getClassLoader()))
        .isInstanceOf(IllegalStateException.class)
        .hasMessageContaining("natives/does-not-exist/libdengjen.so");
  }
}

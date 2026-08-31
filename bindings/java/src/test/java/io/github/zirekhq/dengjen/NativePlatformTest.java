package io.github.zirekhq.dengjen;

import static org.assertj.core.api.Assertions.assertThat;
import static org.assertj.core.api.Assertions.assertThatThrownBy;

import org.junit.jupiter.api.Test;

class NativePlatformTest {
  @Test
  void mapsLinuxX86_64() {
    assertThat(NativePlatform.classifier("Linux", "amd64")).isEqualTo("linux-x86_64");
    assertThat(NativePlatform.classifier("Linux", "x86_64")).isEqualTo("linux-x86_64");
  }

  @Test
  void mapsLinuxAarch64() {
    assertThat(NativePlatform.classifier("Linux", "aarch64")).isEqualTo("linux-aarch64");
    assertThat(NativePlatform.classifier("Linux", "arm64")).isEqualTo("linux-aarch64");
  }

  @Test
  void mapsWindowsX64() {
    assertThat(NativePlatform.classifier("Windows 11", "amd64")).isEqualTo("windows-x64");
  }

  @Test
  void mapsMacosAarch64() {
    assertThat(NativePlatform.classifier("Mac OS X", "aarch64")).isEqualTo("macos-aarch64");
  }

  @Test
  void rejectsWindowsOnArm() {
    assertThatThrownBy(() -> NativePlatform.classifier("Windows 11", "aarch64"))
        .isInstanceOf(IllegalStateException.class)
        .hasMessageContaining("aarch64");
  }

  @Test
  void rejectsAnUnknownOs() {
    assertThatThrownBy(() -> NativePlatform.classifier("SunOS", "x86_64"))
        .isInstanceOf(IllegalStateException.class)
        .hasMessageContaining("SunOS");
  }
}

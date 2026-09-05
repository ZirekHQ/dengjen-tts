package io.github.zirekhq.dengjen;

import java.util.Locale;

final class NativePlatform {
  private static final String WINDOWS = "windows";
  private static final String AARCH64 = "aarch64";

  private NativePlatform() {}

  static String classifier(String osName, String osArch) {
    String os = os(osName);
    String arch = arch(os, osName, osArch);
    return os + "-" + arch;
  }

  private static String os(String osName) {
    String lower = osName.toLowerCase(Locale.ROOT);
    if (lower.contains("linux")) {
      return "linux";
    }
    if (lower.contains("mac") || lower.contains("darwin")) {
      return "macos";
    }
    if (lower.contains(WINDOWS)) {
      return WINDOWS;
    }
    throw new IllegalStateException("unsupported OS for a dengjen native library: " + osName);
  }

  private static String arch(String os, String osName, String osArch) {
    String lower = osArch.toLowerCase(Locale.ROOT);
    boolean isArm64 = lower.equals(AARCH64) || lower.equals("arm64");
    boolean isX64 = lower.equals("x86_64") || lower.equals("amd64") || lower.equals("x64");
    if (os.equals(WINDOWS)) {
      if (isX64) {
        return "x64";
      }
      throw new IllegalStateException(
          "unsupported Windows architecture for a dengjen native library: " + osArch);
    }
    if (os.equals("macos")) {
      if (isArm64) {
        return AARCH64;
      }
      throw new IllegalStateException(
          "unsupported macOS architecture for a dengjen native library: " + osArch);
    }
    if (isArm64) {
      return AARCH64;
    }
    if (isX64) {
      return "x86_64";
    }
    throw new IllegalStateException(
        "unsupported " + osName + " architecture for a dengjen native library: " + osArch);
  }
}

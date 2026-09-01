package io.github.zirekhq.dengjen;

import java.util.Locale;

/**
 * Maps the running JVM's {@code os.name}/{@code os.arch} to one of the four native-library
 * classifiers this project publishes (see the Java bindings Maven Central publish design doc).
 * Deliberately narrow: anything outside those four combinations is a platform this project does not
 * ship a native artifact for yet, and callers need a clear error rather than a guess.
 */
final class NativePlatform {
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
    if (lower.contains("windows")) {
      return "windows";
    }
    throw new IllegalStateException("unsupported OS for a dengjen native library: " + osName);
  }

  private static String arch(String os, String osName, String osArch) {
    String lower = osArch.toLowerCase(Locale.ROOT);
    boolean isArm64 = lower.equals("aarch64") || lower.equals("arm64");
    boolean isX64 = lower.equals("x86_64") || lower.equals("amd64") || lower.equals("x64");
    if (os.equals("windows")) {
      if (isX64) {
        return "x64";
      }
      throw new IllegalStateException(
          "unsupported Windows architecture for a dengjen native library: " + osArch);
    }
    if (os.equals("macos")) {
      if (isArm64) {
        return "aarch64";
      }
      throw new IllegalStateException(
          "unsupported macOS architecture for a dengjen native library: " + osArch);
    }
    if (isArm64) {
      return "aarch64";
    }
    if (isX64) {
      return "x86_64";
    }
    throw new IllegalStateException(
        "unsupported " + osName + " architecture for a dengjen native library: " + osArch);
  }
}

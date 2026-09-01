package io.github.zirekhq.dengjen;

import java.io.IOException;
import java.io.InputStream;
import java.io.UncheckedIOException;
import java.lang.foreign.Arena;
import java.lang.foreign.SymbolLookup;
import java.nio.file.FileSystems;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.attribute.FileAttribute;
import java.nio.file.attribute.PosixFilePermissions;

/**
 * Resolves the native {@code libdengjen} shared library for {@link DengjenLib}. Two paths:
 *
 * <ul>
 *   <li>{@code -Ddengjen.native.library.path=<file>} — an explicit override, used by this module's
 *       own test suites (see {@code build.gradle.kts}) and available to any consumer who wants to
 *       point at a native library they built or placed themselves.
 *   <li>Otherwise: detect the running platform (see {@link NativePlatform}) and look for that
 *       platform's native library as a classpath resource under {@code natives/<classifier>/},
 *       which is exactly what this project's per-platform classifier jars contain. The resource is
 *       copied to a temp file — {@code SymbolLookup.libraryLookup} needs a real filesystem path,
 *       not an in-jar one.
 * </ul>
 */
final class NativeLibraryLoader {
  private NativeLibraryLoader() {}

  // Owner-only permissions for the extracted native library: the system temp directory is
  // world-writable on multi-user hosts, and this file gets loaded and executed. POSIX permissions
  // aren't supported on Windows, so fall back to the platform default there.
  private static final FileAttribute<?>[] OWNER_ONLY_PERMISSIONS =
      FileSystems.getDefault().supportedFileAttributeViews().contains("posix")
          ? new FileAttribute<?>[] {
            PosixFilePermissions.asFileAttribute(PosixFilePermissions.fromString("rw-------"))
          }
          : new FileAttribute<?>[0];

  static SymbolLookup load() {
    String override = System.getProperty("dengjen.native.library.path");
    if (override != null) {
      return SymbolLookup.libraryLookup(Path.of(override), Arena.global());
    }
    String classifier =
        NativePlatform.classifier(System.getProperty("os.name"), System.getProperty("os.arch"));
    String libraryName = System.mapLibraryName("libdengjen");
    String resourcePath = "natives/" + classifier + "/" + libraryName;
    Path extracted;
    try {
      extracted = extractResource(resourcePath, NativeLibraryLoader.class.getClassLoader());
    } catch (IOException e) {
      throw new UncheckedIOException("failed to extract native library " + resourcePath, e);
    }
    return SymbolLookup.libraryLookup(extracted, Arena.global());
  }

  static Path extractResource(String resourcePath, ClassLoader loader) throws IOException {
    try (InputStream in = loader.getResourceAsStream(resourcePath)) {
      if (in == null) {
        throw new IllegalStateException(
            "no native library found on the classpath at '"
                + resourcePath
                + "' -- add a runtimeOnly dependency on the matching "
                + "io.github.zirekhq.dengjen:dengjen-java-bindings:<version>:<classifier> "
                + "artifact, or set -Ddengjen.native.library.path to a library you built "
                + "yourself");
      }
      Path extracted =
          Files.createTempFile(
              "dengjen-native-", "-" + resourcePath.replace('/', '_'), OWNER_ONLY_PERMISSIONS);
      extracted.toFile().deleteOnExit();
      Files.copy(in, extracted, StandardCopyOption.REPLACE_EXISTING);
      return extracted;
    }
  }
}

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















final class NativeLibraryLoader {
  private NativeLibraryLoader() {}

  
  
  
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

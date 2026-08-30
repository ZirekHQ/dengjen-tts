package dev.dengjen;

import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * Exercises the bindings end-to-end against a real trained voice, following the Rust convention
 * (e.g. dengjen-tts-kokoro's e2e_real_voice.rs and the CLI's kokoro_e2e.rs): gated behind
 * DENGJEN_KOKORO_TEST_VOICE_CONFIG, soft-skipping when it's unset rather than failing the build.
 */
class RealVoiceE2ETest {
  @Test
  void synthesizesRealAudioFromARealVoice(@TempDir Path tempDir) throws IOException {
    String configPath = System.getenv("DENGJEN_KOKORO_TEST_VOICE_CONFIG");
    assumeTrue(
        configPath != null,
        "set DENGJEN_KOKORO_TEST_VOICE_CONFIG to a real Kokoro voice config to run this test");

    try (Voice voice = Voice.load(configPath)) {
      Path outPath = tempDir.resolve("output.wav");
      SynthesisParams params = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0);
      boolean wrote = voice.speakToFile("Hello, world!", params, outPath.toString());
      assertTrue(wrote);

      byte[] data = Files.readAllBytes(outPath);
      assertTrue(data.length > 44, "expected a WAV file with audio samples beyond the header");
    }
  }
}

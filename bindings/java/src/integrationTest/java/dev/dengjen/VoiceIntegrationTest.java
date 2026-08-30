package dev.dengjen;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.Optional;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;
import static org.junit.jupiter.api.Assertions.assertTrue;

class VoiceIntegrationTest {
    @Test
    void loadReportsAnErrorForAMissingConfigPath() {
        assertThrows(DengjenException.class,
                () -> Voice.load("/nonexistent-dengjen-java-binding-test.json"));
    }

    @Test
    void loadAudioInfoAndClose(@TempDir Path tempDir) {
        Voice voice = Voice.load(TestFixtures.syntheticPiperConfigPath(tempDir).toString());
        AudioInfo info = voice.getAudioInfo();
        assertEquals(22050, info.sampleRate());
        assertEquals(1, info.numChannels());

        voice.close();
        assertDoesNotThrow(voice::close); // close must be idempotent
    }

    @Test
    void synthConfigRoundTrip(@TempDir Path tempDir) {
        try (Voice voice = Voice.load(TestFixtures.syntheticPiperConfigPath(tempDir).toString())) {
            PiperSynthConfig cfg = voice.getDefaultSynthConfig();
            assertEquals(0.667f, cfg.noiseScale(), 0.0001f);

            voice.setSynthConfig(new PiperSynthConfig(cfg.speaker(), 2.0f, cfg.noiseScale(), cfg.noiseW()));

            voice.setSynthesisParameter("noise_scale", 0.5f);
            Optional<Float> value = voice.getSynthesisParameter("noise_scale");
            assertTrue(value.isPresent());
            assertEquals(0.5f, value.get(), 0.0001f);

            assertTrue(voice.getSynthesisParameter("no_such_key").isEmpty());
        }
    }

    @Test
    void speakToFileWritesAWavFile(@TempDir Path tempDir) throws IOException {
        try (Voice voice = Voice.load(TestFixtures.syntheticPiperConfigPath(tempDir).toString())) {
            Path outPath = tempDir.resolve("output.wav");
            SynthesisParams params = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0);
            boolean wrote = voice.speakToFile("Test.", params, outPath.toString());
            assertTrue(wrote);

            byte[] data = Files.readAllBytes(outPath);
            // BATCH_OUTPUT_SAMPLES=8000 in generate_synthetic_piper.py: a
            // 44-byte RIFF/WAVE header plus 8000 mono i16 samples. Matches
            // bindings/go's equivalent assertion.
            assertEquals(44 + 8000 * 2, data.length);
        }
    }
}

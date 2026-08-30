package dev.dengjen;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.file.Path;

import static org.junit.jupiter.api.Assertions.assertDoesNotThrow;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertThrows;

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
}

package dev.dengjen;

import org.junit.jupiter.api.Test;

import static org.junit.jupiter.api.Assertions.assertEquals;

class DengjenLayoutsTest {
    @Test
    void externErrorMatchesTheCStructLayout() {
        assertEquals(16, DengjenLayouts.EXTERN_ERROR.byteSize());
        assertEquals(0, DengjenLayouts.EXTERN_ERROR_CODE_OFFSET);
        assertEquals(8, DengjenLayouts.EXTERN_ERROR_MESSAGE_OFFSET);
    }

    @Test
    void piperSynthConfigMatchesTheCStructLayout() {
        assertEquals(16, DengjenLayouts.PIPER_SYNTH_CONFIG.byteSize());
        assertEquals(0, DengjenLayouts.PIPER_SYNTH_CONFIG_SPEAKER_OFFSET);
        assertEquals(4, DengjenLayouts.PIPER_SYNTH_CONFIG_LENGTH_SCALE_OFFSET);
        assertEquals(8, DengjenLayouts.PIPER_SYNTH_CONFIG_NOISE_SCALE_OFFSET);
        assertEquals(12, DengjenLayouts.PIPER_SYNTH_CONFIG_NOISE_W_OFFSET);
    }

    @Test
    void audioInfoMatchesTheCStructLayout() {
        assertEquals(12, DengjenLayouts.AUDIO_INFO.byteSize());
        assertEquals(0, DengjenLayouts.AUDIO_INFO_SAMPLE_RATE_OFFSET);
        assertEquals(4, DengjenLayouts.AUDIO_INFO_NUM_CHANNELS_OFFSET);
        assertEquals(8, DengjenLayouts.AUDIO_INFO_SAMPLE_WIDTH_OFFSET);
    }

    @Test
    void synthesisEventMatchesTheCStructLayout() {
        assertEquals(32, DengjenLayouts.SYNTHESIS_EVENT.byteSize());
        assertEquals(0, DengjenLayouts.SYNTHESIS_EVENT_TYPE_OFFSET);
        assertEquals(8, DengjenLayouts.SYNTHESIS_EVENT_ERROR_PTR_OFFSET);
        assertEquals(16, DengjenLayouts.SYNTHESIS_EVENT_LEN_OFFSET);
        assertEquals(24, DengjenLayouts.SYNTHESIS_EVENT_DATA_OFFSET);
    }

    @Test
    void synthesisParamsMatchesTheCStructLayout() {
        assertEquals(40, DengjenLayouts.SYNTHESIS_PARAMS.byteSize());
        assertEquals(0, DengjenLayouts.SYNTHESIS_PARAMS_MODE_OFFSET);
        assertEquals(4, DengjenLayouts.SYNTHESIS_PARAMS_RATE_OFFSET);
        assertEquals(5, DengjenLayouts.SYNTHESIS_PARAMS_VOLUME_OFFSET);
        assertEquals(6, DengjenLayouts.SYNTHESIS_PARAMS_PITCH_OFFSET);
        assertEquals(8, DengjenLayouts.SYNTHESIS_PARAMS_APPENDED_SILENCE_MS_OFFSET);
        assertEquals(16, DengjenLayouts.SYNTHESIS_PARAMS_CALLBACK_OFFSET);
        assertEquals(24, DengjenLayouts.SYNTHESIS_PARAMS_NONBLOCKING_OFFSET);
        assertEquals(32, DengjenLayouts.SYNTHESIS_PARAMS_USER_DATA_OFFSET);
    }
}

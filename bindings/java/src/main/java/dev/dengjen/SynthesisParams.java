package dev.dengjen;

/** Controls how a speak()/speakToFile() call synthesizes and post-processes audio, mirroring libdengjen's SynthesisParams struct. */
public record SynthesisParams(
        SynthesisMode mode, int rate, int volume, int pitch, int appendedSilenceMs, boolean nonblocking) {

    public SynthesisParams {
        requireRange(rate, "rate");
        requireRange(volume, "volume");
        requireRange(pitch, "pitch");
    }

    /** Convenience constructor defaulting nonblocking to false. */
    public SynthesisParams(SynthesisMode mode, int rate, int volume, int pitch, int appendedSilenceMs) {
        this(mode, rate, volume, pitch, appendedSilenceMs, false);
    }

    private static void requireRange(int value, String name) {
        if (value < 0 || value > 100) {
            throw new IllegalArgumentException(name + " must be 0-100, got " + value);
        }
    }
}

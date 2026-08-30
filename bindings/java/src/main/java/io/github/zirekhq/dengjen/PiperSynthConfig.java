package io.github.zirekhq.dengjen;

/**
 * Mirrors libdengjen's PiperSynthConfig: the tunable synthesis parameters exposed by Piper-family
 * voices.
 */
public record PiperSynthConfig(int speaker, float lengthScale, float noiseScale, float noiseW) {}

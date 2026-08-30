package io.github.zirekhq.dengjen;

/** A voice's output audio format. */
public record AudioInfo(int sampleRate, int numChannels, int sampleWidth) {}

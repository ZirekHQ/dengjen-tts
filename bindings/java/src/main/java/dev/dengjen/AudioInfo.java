package dev.dengjen;

/** A voice's output audio format. */
public record AudioInfo(int sampleRate, int numChannels, int sampleWidth) {}

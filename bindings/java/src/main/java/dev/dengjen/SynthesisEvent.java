package dev.dengjen;

/** One event delivered during a streaming speak() call. */
public record SynthesisEvent(EventType type, byte[] data, DengjenException error) {}

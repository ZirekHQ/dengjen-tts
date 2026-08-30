package io.github.zirekhq.dengjen;

@FunctionalInterface
public interface SynthesisEventHandler {
  /** Return true to keep receiving events, false to stop the stream early. */
  boolean onEvent(SynthesisEvent event);
}

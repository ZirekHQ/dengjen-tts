package io.github.zirekhq.dengjen;

@FunctionalInterface
public interface SynthesisEventHandler {
  
  boolean onEvent(SynthesisEvent event);
}

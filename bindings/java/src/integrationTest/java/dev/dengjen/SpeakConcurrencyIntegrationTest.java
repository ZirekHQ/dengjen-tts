package dev.dengjen;

import static org.junit.jupiter.api.Assertions.assertEquals;

import java.nio.file.Path;
import java.util.concurrent.atomic.AtomicInteger;
import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

/**
 * Guards the one hazard that no functional test can see: freeing a native upcall stub while an
 * invocation of it is still on the stack.
 *
 * <p>SpeakTrampoline deliberately shares a single process-wide stub allocated in Arena.global()
 * rather than allocating one per call and freeing it when the stream ends -- see the rationale on
 * that class. A refactor back to per-call stubs would still pass every test in
 * VoiceIntegrationTest, because the crash needs a stack walk to cross a freed code blob's frame,
 * and that needs concurrent traffic: blocking calls releasing on their own thread while nonblocking
 * streams are mid-upcall on libdengjen's pool threads. This test manufactures exactly that. Under
 * the per-call-stub design it crashed the JVM (SIGSEGV in vframeStreamCommon::next, via
 * CloseScopedMemoryClosure) reliably across isolated runs; under the current design it passes.
 *
 * <p>What it demonstrates, precisely: under real concurrent load, every one of the calls below
 * releases its registry entry exactly once, and no upcall stub is ever freed while a frame of it is
 * live (the latter shows up as the VM staying up at all, not as an assertion). It does not
 * distinguish invoke()'s `terminal || !wantsMore` from two independent ifs -- release() is
 * idempotent either way, so that condition is load-bearing for intent, not for anything this test
 * can observe.
 */
class SpeakConcurrencyIntegrationTest {
  private static final int THREADS = 6;
  private static final int CALLS_PER_THREAD = 100;

  @Test
  void concurrentBlockingAndNonblockingStreamsReleaseEveryCallExactlyOnce(@TempDir Path tempDir)
      throws Exception {
    try (Voice voice = Voice.load(TestFixtures.syntheticPiperConfigPath(tempDir).toString())) {
      SynthesisParams blocking = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0);
      SynthesisParams nonblocking = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0, true);
      int expected = THREADS * CALLS_PER_THREAD;

      AtomicInteger releases = new AtomicInteger();
      SpeakTrampoline.testCallReleased = releases::incrementAndGet;
      try {
        Thread[] threads = new Thread[THREADS];
        for (int t = 0; t < THREADS; t++) {
          SynthesisParams params = t % 2 == 0 ? nonblocking : blocking;
          threads[t] =
              new Thread(
                  () -> {
                    for (int i = 0; i < CALLS_PER_THREAD; i++) {
                      // True for the fixture's 16000-byte SPEECH event,
                      // false for the zero-length FINISHED one, so every
                      // call ends on a terminal event whose handler also
                      // returned false -- the release path where both of
                      // invoke()'s two end-of-stream reasons fire at once.
                      voice.speak("Test.", params, event -> event.data().length % 3 != 0);
                    }
                  });
          threads[t].start();
        }
        for (Thread thread : threads) {
          thread.join();
        }

        // Nonblocking streams outlive speak(), so their releases land on
        // a pool thread after the loop above finishes.
        long deadline = System.nanoTime() + 60_000_000_000L;
        while (releases.get() < expected && System.nanoTime() < deadline) {
          Thread.sleep(20);
        }
        assertEquals(
            expected,
            releases.get(),
            "every speak() call must release its registration exactly once");
      } finally {
        // A pool thread from a just-completed speak() call may still be
        // in-flight when the test method returns. If it invokes the hook
        // after another test has reset or reused it, that stale call could
        // corrupt the concurrent-test counters. This sleep is an imperfect
        // but necessary guard against that narrow window.
        Thread.sleep(500);
        SpeakTrampoline.testCallReleased = null;
      }
    }
  }
}

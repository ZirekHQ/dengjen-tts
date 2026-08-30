package dev.dengjen;

import org.junit.jupiter.api.Test;
import org.junit.jupiter.api.io.TempDir;

import java.nio.file.Path;
import java.util.concurrent.atomic.AtomicInteger;

import static org.junit.jupiter.api.Assertions.assertEquals;

/**
 * Guards the one hazard that no functional test can see: freeing a native
 * upcall stub while an invocation of it is still on the stack.
 *
 * SpeakTrampoline deliberately shares a single process-wide stub allocated in
 * Arena.global() rather than allocating one per call and freeing it when the
 * stream ends -- see the rationale on that class. A refactor back to per-call
 * stubs would still pass every test in VoiceIntegrationTest, because the crash
 * needs a stack walk to cross a freed code blob's frame, and that needs
 * concurrent traffic: blocking calls releasing on their own thread while
 * nonblocking streams are mid-upcall on libdengjen's pool threads. This test
 * manufactures exactly that. Under the per-call-stub design it crashed the JVM
 * (SIGSEGV in vframeStreamCommon::next, via CloseScopedMemoryClosure) on about
 * one run in two; under the current design it passes.
 */
class SpeakConcurrencyIntegrationTest {
    private static final int THREADS = 6;
    private static final int CALLS_PER_THREAD = 100;

    @Test
    void concurrentBlockingAndNonblockingStreamsReleaseEveryCallExactlyOnce(@TempDir Path tempDir) throws Exception {
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
                    threads[t] = new Thread(() -> {
                        for (int i = 0; i < CALLS_PER_THREAD; i++) {
                            // True for the fixture's 16000-byte SPEECH event,
                            // false for the zero-length FINISHED one -- so every
                            // call ends on a terminal event whose handler also
                            // returned false, the case where releasing under two
                            // independent ifs instead of `terminal || !wantsMore`
                            // would release twice.
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
                assertEquals(expected, releases.get(),
                        "every speak() call must release its registration exactly once");
            } finally {
                // Let any straggling pool thread finish its upcall before the
                // hook it may still call is unset.
                Thread.sleep(500);
                SpeakTrampoline.testCallReleased = null;
            }
        }
    }
}

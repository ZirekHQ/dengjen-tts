# dengjen (Java bindings)

Java bindings for [`libdengjen`](../../crates/frontends/capi), the C API for
[dengjen-tts](../../README.md), built on the Java FFM API
(`java.lang.foreign`).

## Prerequisites

- JDK 25 (the Gradle toolchain pins 25, the LTS release where the FFM API is
  stable and non-preview; FFM has been stable since Java 22)
- A Rust toolchain (to build `libdengjen` from source — see the parent
  repository's own build prerequisites)

## Building and testing

```bash
make test    # builds libdengjen in release mode, then runs unit + integration tests
```

`make native` alone builds just the native library, without running tests.

## Usage

```java
try (Voice voice = Voice.load("/path/to/voice/config.json")) {
    AudioInfo info = voice.getAudioInfo();

    SynthesisParams params = new SynthesisParams(SynthesisMode.LAZY, 10, 100, 50, 0);
    voice.speakToFile("Hello, world.", params, "out.wav");
}
```

For streaming synthesis (audio delivered incrementally via a callback):

```java
voice.speak("Hello, world.", params, event -> {
    switch (event.type()) {
        case SPEECH -> { /* event.data() is a chunk of raw PCM audio */ }
        case FINISHED -> { /* stream complete */ }
        case ERROR -> { /* event.error() describes what went wrong */ }
    }
    return true; // keep receiving events; return false to stop early
});
```

## Callback safety

The handler passed to `speak` must not throw and must return promptly — an
exception is caught and treated as "stop early" rather than propagated,
since letting one unwind across the native call frames that invoke it would
be undefined behavior at this FFI boundary.

## Known limitations

- This module is usable only from within a checkout of the parent
  [dengjen-tts](https://github.com/ZirekHQ/dengjen-tts) repository today —
  `DengjenLib` resolves the built native library via a path relative to the
  process's working directory (`../../target/release`), which only exists
  inside that checkout. It is not a published, standalone dependency. No
  prebuilt binaries are published either way — every consumer builds
  `libdengjen` from source (see `Makefile`).
- Every JVM invocation of a consumer built against this module needs
  `--enable-native-access=ALL-UNNAMED` (or the module-qualified equivalent)
  to avoid a native-access warning — `build.gradle.kts` already adds this
  for the module's own tests; a downstream consumer must add it themselves.
- This is a thin wrapper: `speak`'s callback shape mirrors the C API
  closely rather than offering a fully idiomatic Java redesign
  (`CompletableFuture`, reactive streams).

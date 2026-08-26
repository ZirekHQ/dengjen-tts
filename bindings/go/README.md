# dengjen (Go bindings)

Go bindings for [`libdengjen`](../../crates/frontends/capi), the C API for
[dengjen-tts](../../README.md).

## Prerequisites

- Go 1.22+
- A Rust toolchain (to build `libdengjen` from source — see the parent
  repository's own build prerequisites)
- A C compiler (for cgo)

## Building and testing

```bash
make test    # builds libdengjen in release mode, then runs `go test ./...`
```

`make native` alone builds just the native library, without running tests.

## Usage

```go
import dengjen "github.com/ZirekHQ/dengjen-tts/bindings/go"

voice, err := dengjen.LoadVoice("/path/to/voice/config.json")
if err != nil {
    log.Fatal(err)
}
defer voice.Close()

params := dengjen.SynthesisParams{Mode: dengjen.SynthModeLazy, Rate: 10, Volume: 100, Pitch: 50}
_, err = voice.SpeakToFile("Hello, world.", params, "out.wav")
```

For streaming synthesis (audio delivered incrementally via a callback):

```go
err = voice.Speak("Hello, world.", params, func(e dengjen.SynthesisEvent) bool {
    switch e.Type {
    case dengjen.EventSpeech:
        // e.Data is a chunk of raw PCM audio
    case dengjen.EventFinished:
        // stream complete
    case dengjen.EventError:
        // e.Err describes what went wrong
    }
    return true // keep receiving events; return false to stop early
})
```

## Callback safety

`onEvent` (passed to `Speak`) must not panic and must not call
`runtime.Goexit` (which `testing.T.Fatal` does internally) — either would
unwind across the C/Rust call frames that invoke it, which is undefined
behavior at this FFI boundary. Recover from panics inside your own callback
if there's any chance it might panic.

## Known limitations

- This module is usable only from within a checkout of the parent
  [dengjen-tts](https://github.com/ZirekHQ/dengjen-tts) repository today —
  its cgo build flags (`dengjen.go`) resolve `libdengjen.h` and the built
  native library via paths relative to this directory
  (`../../crates/frontends/capi`, `../../target/release`), which only exist
  inside that checkout. It is not `go get`-able as a standalone dependency.
  No prebuilt binaries are published either way — every consumer builds
  `libdengjen` from source (see `Makefile`).
- A consumer binary built against this module needs `LD_LIBRARY_PATH` (or
  an equivalent runtime library search path) pointing at the directory
  holding the built `libdengjen` shared library at *run time*, not just at
  build/test time — `make test`'s `LD_LIBRARY_PATH=$(TARGET_DIR)` wiring
  only covers the test binary.
- This is a thin wrapper: `Speak`'s callback shape mirrors the C API closely
  rather than offering a fully idiomatic Go redesign (channels,
  `context.Context`-driven cancellation).

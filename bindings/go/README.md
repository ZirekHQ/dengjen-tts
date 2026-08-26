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

## Known limitations

- No prebuilt binaries are published — every consumer builds `libdengjen`
  from source (see `Makefile`).
- This is a thin wrapper: `Speak`'s callback shape mirrors the C API closely
  rather than offering a fully idiomatic Go redesign (channels,
  `context.Context`-driven cancellation).

# libdengjen

C API bindings for the dengjen-tts speech synthesis engine.

Exposes `extern "C"` functions such as `libdengjenLoadVoiceFromConfigPath`, `libdengjenSpeak`, and `libdengjenSpeakToFile`, delivering audio and completion events to a caller-supplied `SpeechSynthesisCallback` via a `SynthesisEvent` struct; the C header is generated from this crate with `cbindgen`.

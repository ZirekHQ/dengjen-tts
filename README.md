# Dengjen

[Project board](https://github.com/orgs/ZirekHQ/projects/1) — live roadmap and status for this repo's issues.

A cross-platform Rust engine for neural TTS models.

## Features

* **Models**: [Piper](https://github.com/rhasspy/piper), [Kokoro](https://github.com/hexgrad/kokoro), and [MeloTTS](https://github.com/myshell-ai/MeloTTS) ONNX voices, including Kokoro per-voice style embeddings
* **Phonemization**: eSpeak-ng (100+ languages, IPA output) and Arabic diacritization via `libtashkeel`
* **Multi-speaker voices**: select by `speaker_id`
* **Streaming synthesis**: chunked output (`chunk_size`/`chunk_padding`) and a realtime gRPC stream
* **Prosody control**: rate, pitch, and volume via `sonic-sys` (libsonic)
* **Synthesis modes**: lazy, parallel, and batched, selectable per request
* **Bindings**: native Rust, C-API (`libdengjen`), Python (`pyo3`), gRPC (any language over the wire), and a CLI

Not yet supported, tracked in the issues linked below:

* Native Go/Java/Kotlin bindings — [#46](https://github.com/ZirekHQ/dengjen/issues/46)
* MeloTTS model support — [#47](https://github.com/ZirekHQ/dengjen/issues/47)
* Matcha-TTS model loader — [#48](https://github.com/ZirekHQ/dengjen/issues/48)

Out of scope: RHVoice-style formant/statistical synthesis is a different synthesis paradigm from
this engine's neural-ONNX pipeline and isn't planned.


# Crates

- `espeak-phonemizer`: Converts text to `IPA` phonemes using a patched version of eSpeak-ng
- `dengjen-model`: Handles model loading and inference using `onnxruntime` via `ort`
- `dengjen-tts`: Wraps `DengjenModel` and adds synthesized speech post-processing, including changing prosody. Also provides different modes of parallelism.
- `dengjen-tts-grpc`: [GRPC](https://grpc.io/) frontend for dengjen
- `libdengjen`: C-API binding to dengjen
- `dengjen-tts-python`: Python bindings to `dengjen-tts` using `pyo3`
- `sonic-sys`: Rust FFI bindings to [Sonic](https://github.com/waywardgeek/sonic): a `C` library for controlling various aspects of generated speech, such as rate, volume, and pitch

# Building

Dengjen pulls its native dependencies (eSpeak-ng, Sonic, ...) in as git submodules, so clone recursively:

```sh
git clone --recurse-submodules https://github.com/ZirekHQ/dengjen-tts.git
cd dengjen-tts
```

If you already cloned without `--recurse-submodules`, fetch them with:

```sh
git submodule update --init --recursive
```

Then build the workspace, or just the CLI frontend:

```sh
cargo build --release --workspace
# or just the CLI
cargo build --release -p dengjen-tts-cli
```

The resulting binary is at `target/release/dengjen` (`dengjen.exe` on Windows).

eSpeak-ng needs its `espeak-ng-data` directory at runtime. Dengjen looks for it next to the running executable by default; if it lives elsewhere (or you're running via `cargo run`), point to it with:

```sh
export DENGJEN_ESPEAKNG_DATA_DIRECTORY=/path/to/directory/containing/espeak-ng-data
```

On Windows you also need `espeak-ng.dll` on your `PATH` — see "A note on testing" below.

# Synthesizing speech

Dengjen synthesizes speech from a [Piper](https://github.com/rhasspy/piper) voice. Download a voice's `.onnx` model and matching `.onnx.json` config from the [Piper voices](https://huggingface.co/rhasspy/piper-voices) repository and keep both files together, e.g. `voices/en_US-lessac-medium.onnx` and `voices/en_US-lessac-medium.onnx.json`.

A voice doesn't have to come from the official Piper voices repository — any VITS-family ONNX export using the same 3/4-input tensor convention (phoneme ids, lengths, scales, optional speaker id) can be loaded by writing a matching `.onnx.json` manifest with `"model_type": "vits"`. The minimal required fields are `audio` (sample rate), `inference` (`noise_scale`/`length_scale`/`noise_w`), and `phoneme_id_map` (the symbol vocabulary); `phoneme_type` selects the phonemizer (`espeak` is the default if omitted, and needs an `espeak.voice` entry — `text`, `hebrew`, and `pinyin` don't).

A [MeloTTS](https://github.com/myshell-ai/MeloTTS) voice — its own VITS-derived ONNX export, with a `tones` tensor alongside phone ids — is loaded with `"model_type": "melotts"`. The required fields are `audio` (sample rate), `inference` (`noise_scale`/`length_scale`/`noise_scale_w`), `phone_id_map` and `tone_id_map` (the phone and tone symbol vocabularies), and `model_path`; `phonemizer` selects the phonemization backend and must be one of `{"type": "espeak", "voice": "<espeak-ng voice name>"}` (covers English, Spanish, French, Japanese, Korean) or `{"type": "pinyin", "model_dir": "<g2pW model directory>"}` (Chinese, with real tone extraction — requires building with the `pinyin` feature).

Synthesize text from a file to a WAV file, using the `dengjen-cli` frontend:

```sh
cargo run --release -p dengjen-tts-cli -- voices/en_US-lessac-medium.onnx.json \
    -f input.txt \
    -o output.wav
```

Or send a single request as JSON on stdin and capture the WAV bytes written to stdout:

```sh
echo '{"text": "Hello world"}' | cargo run --release -p dengjen-tts-cli -- voices/en_US-lessac-medium.onnx.json > output.wav
```

Run `dengjen --help` (or `cargo run --release -p dengjen-tts-cli -- --help`) for the full list of options, including synthesis mode, speaker id, rate/pitch/volume, and streaming chunk size.

# A note on testing

Some packages, such as `espeak-phonemizer`, include tests. Running `cargo test` from the root of the workspace will likely fail, because `cargo` does not load `config` from sub packages when ran from the workspace root.

On Windows you need to add `espeak-ng.dll` to the library search path by modifying the **PATH** environment variable.

For example, to add `espeak-ng.dll` to your path when building for the `x86_64-pc-windows-msvc` target, run the following command before `cargo test`:

```cmd
set PATH=%PATH%;{repo_path}\deps\windows\espeak-ng-build\i686\bin
```

Replace `repo_path` with the absolute path to the repository.

Then `cd` to the package, and run `cargo test` from there.

# License

Licensed under the GNU General Public License v3.0 or later (GPL-3.0-or-later) — see
[LICENSE](LICENSE). dengjen began as a fork of [Sonata](https://github.com/mush42/sonata) by
Musharraf Omer, originally MIT-licensed; see [NOTICE](NOTICE) for retained attribution.


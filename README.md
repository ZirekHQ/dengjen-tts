# Dengjen

A cross-platform Rust engine for neural TTS models.

## Features

* **Models**: [Piper](https://github.com/rhasspy/piper) and [Kokoro](https://github.com/hexgrad/kokoro) ONNX voices, including Kokoro per-voice style embeddings
* **Phonemization**: eSpeak-ng (100+ languages, IPA output) and Arabic diacritization via `libtashkeel`
* **Multi-speaker voices**: select by `speaker_id`
* **Streaming synthesis**: chunked output (`chunk_size`/`chunk_padding`) and a realtime gRPC stream
* **Prosody control**: rate, pitch, and volume via `sonic-sys` (libsonic)
* **Synthesis modes**: lazy, parallel, and batched, selectable per request
* **Bindings**: native Rust, C-API (`libdengjen`), Python (`pyo3`), gRPC (any language over the wire), and a CLI

Not yet supported, tracked in the issues linked below:

* GPU execution providers (CUDA/CoreML/DirectML) — [#45](https://github.com/ZirekHQ/dengjen/issues/45)
* Native Go/Java/Kotlin bindings — [#46](https://github.com/ZirekHQ/dengjen/issues/46)
* MeloTTS model support — [#47](https://github.com/ZirekHQ/dengjen/issues/47)
* Generic VITS/Matcha-TTS model loader — [#48](https://github.com/ZirekHQ/dengjen/issues/48)

Out of scope: RHVoice-style formant/statistical synthesis is a different synthesis paradigm from
this engine's neural-ONNX pipeline and isn't planned.


# Crates

- `espeak-phonemizer`: Converts text to `IPA` phonemes using a patched version of eSpeak-ng
- `dengjen-model`: Handles model loading and inference using `onnxruntime` via `ort`
- `dengjen-synth`: Wraps `DengjenModel` and adds synthesized speech post-processing, including changing prosody. Also provides different modes of parallelism.
- `dengjen-grpc`: [GRPC](https://grpc.io/) frontend for dengjen
- `libdengjen`: C-API binding to dengjen
- `dengjen-python`: Python bindings to `dengjen-synth` using `pyo3`
- `sonic-sys`: Rust FFI bindings to [Sonic](https://github.com/waywardgeek/sonic): a `C` library for controlling various aspects of generated speech, such as rate, volume, and pitch

# Building

Dengjen pulls its native dependencies (eSpeak-ng, Sonic, libtashkeel, ...) in as git submodules, so clone recursively:

```sh
git clone --recurse-submodules https://github.com/austek/dengjen.git
cd dengjen
```

If you already cloned without `--recurse-submodules`, fetch them with:

```sh
git submodule update --init --recursive
```

Then build the workspace, or just the CLI frontend:

```sh
cargo build --release --workspace
# or just the CLI
cargo build --release -p dengjen-cli
```

The resulting binary is at `target/release/dengjen` (`dengjen.exe` on Windows).

eSpeak-ng needs its `espeak-ng-data` directory at runtime. Dengjen looks for it next to the running executable by default; if it lives elsewhere (or you're running via `cargo run`), point to it with:

```sh
export DENGJEN_ESPEAKNG_DATA_DIRECTORY=/path/to/directory/containing/espeak-ng-data
```

On Windows you also need `espeak-ng.dll` on your `PATH` — see "A note on testing" below.

# Synthesizing speech

Dengjen synthesizes speech from a [Piper](https://github.com/rhasspy/piper) voice. Download a voice's `.onnx` model and matching `.onnx.json` config from the [Piper voices](https://huggingface.co/rhasspy/piper-voices) repository and keep both files together, e.g. `voices/en_US-lessac-medium.onnx` and `voices/en_US-lessac-medium.onnx.json`.

Synthesize text from a file to a WAV file, using the `dengjen-cli` frontend:

```sh
cargo run --release -p dengjen-cli -- voices/en_US-lessac-medium.onnx.json \
    -f input.txt \
    -o output.wav
```

Or send a single request as JSON on stdin and capture the WAV bytes written to stdout:

```sh
echo '{"text": "Hello world"}' | cargo run --release -p dengjen-cli -- voices/en_US-lessac-medium.onnx.json > output.wav
```

Run `dengjen --help` (or `cargo run --release -p dengjen-cli -- --help`) for the full list of options, including synthesis mode, speaker id, rate/pitch/volume, and streaming chunk size.

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


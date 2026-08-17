import os
from pydengjen import Dengjen, PiperModel, AudioOutputConfig


MODEL_PATH = "../dengjen/synth/models/rt/config.json"
SENTENCES = [
    "Technology is not inevitable.",
    "Powerful drivers must exist in order for people to keep pushing the envelope and continue demanding more and more from a particular field of knowledge.",
    "Cheaper Communications",
    "The first and most important driver is our demand for ever cheaper and easier communications.",
    "All of human society depends on communications.",
]


def main():
    os.environ["ORT_DYLIB_PATH"] = "../target/debug/onnxruntime.dll"
    os.environ["DENGJEN_ESPEAKNG_DATA_DIRECTORY"] = "../deps/windows/espeak-ng-build"

    piper_model = PiperModel(MODEL_PATH)
    synth = Dengjen.with_piper(piper_model)

    synth.synthesize_to_file(
        "output.wav",
        "\n".join(SENTENCES),
        AudioOutputConfig(None, None, None, 0),
    )

    stream = synth.synthesize_streamed(
        "\n".join(SENTENCES),
        chunk_size=72,
        chunk_padding=3
    )
    for audio in stream:
        print(f"Chunk len in bytes: {len(audio)}")

    lazy_stream = synth.synthesize_lazy("\n".join(SENTENCES))
    for wave_samples in lazy_stream:
        wave_bytes = wave_samples.get_wave_bytes()
        print(f"Lazy chunk len in bytes: {len(wave_bytes)}")

    parallel_stream = synth.synthesize_parallel("\n".join(SENTENCES))
    for wave_samples in parallel_stream:
        wave_bytes = wave_samples.get_wave_bytes()
        print(f"Parallel chunk len in bytes: {len(wave_bytes)}")


if __name__ == "__main__":
    main()

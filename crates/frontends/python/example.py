import os

from pydengjen import AudioOutputConfig, Dengjen, PiperModel

VOICE_CONFIG_PATH = "../dengjen/synth/models/rt/config.json"

DEMO_LINES = [
    "The weather today is unusually calm.",
    "Long passages reveal how a voice handles pacing, breathing, and emphasis across many consecutive clauses, which is why they matter for evaluation.",
    "Short Bursts",
    "Short phrases are useful for checking startup latency and clipping at boundaries.",
    "A synthesizer is judged as much by its transitions between sounds as by the sounds themselves.",
]


def configure_runtime_environment():
    os.environ["ORT_DYLIB_PATH"] = "../target/debug/onnxruntime.dll"
    os.environ["DENGJEN_ESPEAKNG_DATA_DIRECTORY"] = "../deps/windows/espeak-ng-build"


def build_synthesizer():
    return Dengjen.with_piper(PiperModel(VOICE_CONFIG_PATH))


def demo_batch_synthesis(synth, text):
    synth.synthesize_to_file(
        "output.wav",
        text,
        AudioOutputConfig(None, None, None, 0),
    )
    print("wrote batch synthesis result to output.wav")


def demo_streamed_synthesis(synth, text):
    chunks = synth.synthesize_streamed(text, chunk_size=72, chunk_padding=3)
    for index, chunk in enumerate(chunks):
        print(f"streamed chunk {index}: {len(chunk)} bytes")


def demo_lazy_synthesis(synth, text):
    for index, samples in enumerate(synth.synthesize_lazy(text)):
        print(f"lazy chunk {index}: {len(samples.get_wave_bytes())} bytes")


def demo_parallel_synthesis(synth, text):
    for index, samples in enumerate(synth.synthesize_parallel(text)):
        print(f"parallel chunk {index}: {len(samples.get_wave_bytes())} bytes")


def main():
    configure_runtime_environment()
    synth = build_synthesizer()
    text = "\n".join(DEMO_LINES)

    demo_batch_synthesis(synth, text)
    demo_streamed_synthesis(synth, text)
    demo_lazy_synthesis(synth, text)
    demo_parallel_synthesis(synth, text)


if __name__ == "__main__":
    main()

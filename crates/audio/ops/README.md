# dengjen-audio-ops

Audio post-processing primitives shared across dengjen-tts's speech synthesis pipeline: sample buffers, Hann windowing, and WAV encoding.

Exposes `Audio`/`AudioSamples`/`AudioInfo` sample types alongside functions for writing synthesized audio out as WAV, either to a buffer or to a file.

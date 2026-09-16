# dengjen-tts-melotts

MeloTTS neural TTS model backend for the dengjen-tts speech synthesis engine.

Runs inference via `ort` over phone/tone pairs produced by a feature-gated phonemizer backend (espeak or pinyin), and supports multi-speaker models via a speaker name/ID map.

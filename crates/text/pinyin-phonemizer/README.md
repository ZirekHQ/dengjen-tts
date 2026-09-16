# Pinyin phonemizer

Mandarin pinyin grapheme-to-phoneme conversion for the dengjen-tts speech synthesis engine.

Resolves each character's pinyin via dictionary lookup, falling back to a G2PW ONNX model to disambiguate polyphonic characters, then maps bopomofo output to pinyin.

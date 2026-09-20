#!/usr/bin/env bash
# Usage: DENGJEN_EXAMPLE_VOICE=/path/voice.onnx.json call_sequence.sh [output.pcm]
# Writes raw 16-bit little-endian PCM (no WAV header) to the output file. The sample rate is not written; read audio.sampleRate from the LoadVoice reply.
# Needs grpcurl and jq on PATH and a running dengjen-tts-grpc. Optional: DENGJEN_GRPC_ADDRESS (default 127.0.0.1:49314), DENGJEN_PROTO_DIR (directory holding dengjen_grpc.proto, default .).
set -euo pipefail

ADDRESS="${DENGJEN_GRPC_ADDRESS:-127.0.0.1:49314}"
PROTO_DIR="${DENGJEN_PROTO_DIR:-.}"
VOICE="${DENGJEN_EXAMPLE_VOICE:?set DENGJEN_EXAMPLE_VOICE to a voice manifest (.onnx.json)}"
OUT="${1:-output.pcm}"

call() {
    local method="$1"
    shift
    grpcurl -plaintext -import-path "$PROTO_DIR" -proto dengjen_grpc.proto "$@" "$ADDRESS" "$method"
}

# tag::version[]
call dengjen_grpc.DengjenGrpc/GetDengjenVersion -d '{}'
# end::version[]

# tag::load[]
voice_key=$(jq -n --arg path "$VOICE" '{path: $path}' \
    | call dengjen_grpc.DengjenGrpc/LoadVoice -d @ | jq -r '.voiceKey')
# end::load[]
echo "voice_key=$voice_key"

# tag::synthesize[]
jq -n --arg key "$voice_key" --arg text "Hello from the gRPC server." \
    '{voice_key: $key, text: $text, synthesis_mode: "MODE_LAZY"}' \
    | call dengjen_grpc.DengjenGrpc/SynthesizeUtterance -d @ \
    | jq -r '.audioBytes // empty' | while read -r chunk; do printf '%s' "$chunk" | base64 -d; done > "$OUT"
# end::synthesize[]
echo "wrote $OUT ($(wc -c < "$OUT") bytes)"

# crates/dengjen/models/piper/tests/fixtures/generate_synthetic_piper.py
# Regenerate with: python3 generate_synthetic_piper.py
# Requires: pip install onnx numpy
#
# Builds minimal ONNX graphs shaped like Piper's VITS batch and streaming
# (encoder/decoder) models, with placeholder (non-speech) output — mirrors
# dengjen-kokoro's synthetic_kokoro.onnx approach: a hand-built graph, not an
# exported real model, sized only to exercise the inference/streaming
# plumbing end-to-end.
import numpy as np
from onnx import helper, TensorProto, numpy_helper

BATCH_OUTPUT_SAMPLES = 8000
ENCODER_NUM_FRAMES = 200
ENCODER_CHANNELS = 1
HOP_LENGTH = 256


def build_batch_model(path):
    input_ids = helper.make_tensor_value_info("input", TensorProto.INT64, [1, None])
    input_lengths = helper.make_tensor_value_info("input_lengths", TensorProto.INT64, [1])
    scales = helper.make_tensor_value_info("scales", TensorProto.FLOAT, [3])
    output = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 1, BATCH_OUTPUT_SAMPLES])

    audio_const = numpy_helper.from_array(
        np.zeros((1, 1, BATCH_OUTPUT_SAMPLES), dtype=np.float32), name="audio_const"
    )
    identity = helper.make_node("Identity", inputs=["audio_const"], outputs=["output"])

    graph = helper.make_graph(
        [identity],
        "synthetic_piper_batch",
        [input_ids, input_lengths, scales],
        [output],
        initializer=[audio_const],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    with open(path, "wb") as f:
        f.write(model.SerializeToString())


def build_encoder_model(path):
    input_ids = helper.make_tensor_value_info("input", TensorProto.INT64, [1, None])
    input_lengths = helper.make_tensor_value_info("input_lengths", TensorProto.INT64, [1])
    scales = helper.make_tensor_value_info("scales", TensorProto.FLOAT, [3])
    z_out = helper.make_tensor_value_info(
        "z", TensorProto.FLOAT, [1, ENCODER_CHANNELS, ENCODER_NUM_FRAMES]
    )
    y_mask_out = helper.make_tensor_value_info(
        "y_mask", TensorProto.FLOAT, [1, 1, ENCODER_NUM_FRAMES]
    )

    z_const = numpy_helper.from_array(
        np.zeros((1, ENCODER_CHANNELS, ENCODER_NUM_FRAMES), dtype=np.float32), name="z_const"
    )
    y_mask_const = numpy_helper.from_array(
        np.ones((1, 1, ENCODER_NUM_FRAMES), dtype=np.float32), name="y_mask_const"
    )
    z_identity = helper.make_node("Identity", inputs=["z_const"], outputs=["z"])
    y_mask_identity = helper.make_node("Identity", inputs=["y_mask_const"], outputs=["y_mask"])

    graph = helper.make_graph(
        [z_identity, y_mask_identity],
        "synthetic_piper_encoder",
        [input_ids, input_lengths, scales],
        [z_out, y_mask_out],
        initializer=[z_const, y_mask_const],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    with open(path, "wb") as f:
        f.write(model.SerializeToString())


def build_decoder_model(path):
    # z/y_mask are declared with a dynamic time dimension: the streaming
    # decoder is invoked once per chunk with a differently-sized slice of
    # the encoder's z/y_mask along axis 2 (AdaptiveMelChunker). Output
    # length must scale with that slice's time dimension * hop_length, or
    # SpeechStreamer's audio-index slicing (also scaled by hop_length) will
    # go out of bounds on any chunk after the first.
    z_in = helper.make_tensor_value_info("z", TensorProto.FLOAT, [1, ENCODER_CHANNELS, None])
    y_mask_in = helper.make_tensor_value_info("y_mask", TensorProto.FLOAT, [1, 1, None])
    output = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 1, None])

    hop_length_const = numpy_helper.from_array(
        np.array([HOP_LENGTH], dtype=np.int64), name="hop_length_const"
    )
    leading_dims_const = numpy_helper.from_array(
        np.array([1, 1], dtype=np.int64), name="leading_dims_const"
    )

    z_shape = helper.make_node("Shape", inputs=["z"], outputs=["z_shape"])
    time_dim = helper.make_node(
        "Gather", inputs=["z_shape", "time_index_const"], outputs=["time_dim"]
    )
    samples_dim = helper.make_node(
        "Mul", inputs=["time_dim", "hop_length_const"], outputs=["samples_dim"]
    )
    out_shape = helper.make_node(
        "Concat", inputs=["leading_dims_const", "samples_dim"], outputs=["out_shape"], axis=0
    )
    fill_output = helper.make_node(
        "ConstantOfShape",
        inputs=["out_shape"],
        outputs=["output"],
        value=numpy_helper.from_array(np.array([0.0], dtype=np.float32)),
    )
    time_index_const = numpy_helper.from_array(np.array([2], dtype=np.int64), name="time_index_const")

    graph = helper.make_graph(
        [z_shape, time_dim, samples_dim, out_shape, fill_output],
        "synthetic_piper_decoder",
        [z_in, y_mask_in],
        [output],
        initializer=[hop_length_const, leading_dims_const, time_index_const],
    )
    model = helper.make_model(graph, opset_imports=[helper.make_opsetid("", 17)])
    with open(path, "wb") as f:
        f.write(model.SerializeToString())


if __name__ == "__main__":
    import pathlib

    fixtures_dir = pathlib.Path(__file__).parent
    build_batch_model(fixtures_dir / "synthetic_piper_batch.onnx")
    build_encoder_model(fixtures_dir / "synthetic_piper_encoder.onnx")
    build_decoder_model(fixtures_dir / "synthetic_piper_decoder.onnx")
    print("Wrote synthetic_piper_batch.onnx, synthetic_piper_encoder.onnx, synthetic_piper_decoder.onnx")

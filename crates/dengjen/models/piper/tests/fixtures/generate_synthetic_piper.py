







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
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    
    z_in = helper.make_tensor_value_info("z", TensorProto.FLOAT, [1, ENCODER_CHANNELS, None])
    y_mask_in = helper.make_tensor_value_info("y_mask", TensorProto.FLOAT, [1, 1, None])
    output = helper.make_tensor_value_info("output", TensorProto.FLOAT, [1, 1, None])

    hop_length_const = numpy_helper.from_array(
        np.array([HOP_LENGTH], dtype=np.int64), name="hop_length_const"
    )
    leading_dims_const = numpy_helper.from_array(
        np.array([1, 1], dtype=np.int64), name="leading_dims_const"
    )
    time_index_const = numpy_helper.from_array(np.array([2], dtype=np.int64), name="time_index_const")
    squeeze_axes_const = numpy_helper.from_array(np.array([0], dtype=np.int64), name="squeeze_axes_const")
    range_start_const = numpy_helper.from_array(np.array(0, dtype=np.int64), name="range_start_const")
    range_delta_const = numpy_helper.from_array(np.array(1, dtype=np.int64), name="range_delta_const")

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
    samples_dim_scalar = helper.make_node(
        "Squeeze", inputs=["samples_dim", "squeeze_axes_const"], outputs=["samples_dim_scalar"]
    )
    ramp = helper.make_node(
        "Range",
        inputs=["range_start_const", "samples_dim_scalar", "range_delta_const"],
        outputs=["ramp"],
    )
    ramp_float = helper.make_node("Cast", inputs=["ramp"], outputs=["ramp_float"], to=TensorProto.FLOAT)
    reshape_output = helper.make_node(
        "Reshape", inputs=["ramp_float", "out_shape"], outputs=["output"]
    )

    graph = helper.make_graph(
        [z_shape, time_dim, samples_dim, out_shape, samples_dim_scalar, ramp, ramp_float, reshape_output],
        "synthetic_piper_decoder",
        [z_in, y_mask_in],
        [output],
        initializer=[
            hop_length_const,
            leading_dims_const,
            time_index_const,
            squeeze_axes_const,
            range_start_const,
            range_delta_const,
        ],
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

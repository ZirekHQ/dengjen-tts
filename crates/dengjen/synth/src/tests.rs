mod dev_utils;

use dengjen_synth::DengjenResult;

#[test]
fn test_lazy_stream() -> DengjenResult<()> {
    let (synth, text, output_config) = dev_utils::gen_params("std");
    let stream = synth
        .synthesize_lazy(text, output_config)?
        .map(|ar| ar.map(|a| a.samples));
    dev_utils::iterate_stream(stream)
}

#[test]
fn test_parallel_stream() -> DengjenResult<()> {
    let (synth, text, output_config) = dev_utils::gen_params("std");
    let stream = synth
        .synthesize_parallel(text, output_config)?
        .map(|ar| ar.map(|a| a.samples));
    dev_utils::iterate_stream(stream)
}

#[test]
fn test_realtime_stream() -> DengjenResult<()> {
    let (synth, text, output_config) = dev_utils::gen_params("rt");
    let stream = synth.synthesize_streamed(
        text,
        output_config,
        72,
        3,
        dengjen_core::CancellationToken::new(),
    )?;
    dev_utils::iterate_stream(stream)
}

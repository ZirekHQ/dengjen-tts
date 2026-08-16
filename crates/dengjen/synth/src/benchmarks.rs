mod dev_utils;

fn main() {
    dev_utils::init();
    divan::main();
}

#[divan::bench_group(sample_count = 20, sample_size = 10)]
mod speech_streams {
    use super::*;
    use divan::{black_box, Bencher};

    #[divan::bench(threads = 4)]
    fn bench_lazy_stream(bencher: Bencher) {
        bencher
            .with_inputs(|| dev_utils::gen_params("std"))
            .bench_local_refs(|(synth, text, output_config)| {
                let audio_stream = synth
                    .synthesize_lazy(text.clone(), output_config.clone())
                    .unwrap()
                    .map(|chunk_result| chunk_result.map(|chunk| chunk.samples));
                dev_utils::iterate_stream(black_box(audio_stream)).unwrap();
            });
    }

    #[divan::bench]
    fn bench_parallel_stream(bencher: Bencher) {
        bencher
            .with_inputs(|| dev_utils::gen_params("std"))
            .bench_local_refs(|(synth, text, output_config)| {
                let audio_stream = synth
                    .synthesize_parallel(text.clone(), output_config.clone())
                    .unwrap()
                    .map(|chunk_result| chunk_result.map(|chunk| chunk.samples));
                dev_utils::iterate_stream(black_box(audio_stream)).unwrap();
            });
    }

    #[divan::bench]
    fn bench_realtime_stream(bencher: Bencher) {
        bencher
            .with_inputs(|| dev_utils::gen_params("rt"))
            .bench_local_refs(|(synth, text, output_config)| {
                let audio_stream = synth
                    .synthesize_streamed(
                        text.clone(),
                        output_config.clone(),
                        72,
                        3,
                        dengjen_tts_core::CancellationToken::new(),
                    )
                    .unwrap();
                dev_utils::iterate_stream(black_box(audio_stream)).unwrap();
            });
    }

    #[divan::bench]
    fn bench_lazy_stream_latency(bencher: Bencher) {
        bencher
            .with_inputs(|| dev_utils::gen_params("std"))
            .bench_local_refs(|(synth, text, output_config)| {
                let mut chunk_iter = black_box(
                    synth
                        .synthesize_lazy(text.clone(), output_config.clone())
                        .unwrap(),
                );
                let first_chunk = chunk_iter.next().unwrap().unwrap();
                let _ = first_chunk.as_wave_bytes().len();
            });
    }

    #[divan::bench]
    fn bench_parallel_stream_latency(bencher: Bencher) {
        bencher
            .with_inputs(|| dev_utils::gen_params("std"))
            .bench_local_refs(|(synth, text, output_config)| {
                let mut chunk_iter = black_box(
                    synth
                        .synthesize_parallel(text.clone(), output_config.clone())
                        .unwrap(),
                );
                let first_chunk = chunk_iter.next().unwrap().unwrap();
                let _ = first_chunk.as_wave_bytes().len();
            });
    }

    #[divan::bench]
    fn bench_realtime_stream_latency(bencher: Bencher) {
        bencher
            .with_inputs(|| dev_utils::gen_params("rt"))
            .bench_local_refs(|(synth, text, output_config)| {
                let mut chunk_iter = black_box(
                    synth
                        .synthesize_streamed(
                            text.clone(),
                            output_config.clone(),
                            72,
                            3,
                            dengjen_tts_core::CancellationToken::new(),
                        )
                        .unwrap(),
                );
                let first_chunk = chunk_iter.next().unwrap().unwrap();
                let _ = first_chunk.as_wave_bytes().len();
            });
    }
}

use audio_ops::AudioSamples;
use divan::Bencher;

const SAMPLE_COUNT: usize = 44_100 * 10;

fn overlap_pair_source() -> impl Fn() -> (AudioSamples, AudioSamples) {
    let template: Vec<f32> = (0..SAMPLE_COUNT).map(|idx| idx as f32).collect();
    move || (template.clone().into(), template.clone().into())
}

#[divan::bench]
fn bench_overlap_with(divan_bencher: Bencher) {
    divan_bencher
        .with_inputs(overlap_pair_source())
        .bench_refs(|(left, right)| left.overlap_with(right));
}

fn main() {
    divan::main();
}

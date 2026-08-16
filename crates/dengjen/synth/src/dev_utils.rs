use core::hint::black_box;
use dengjen_piper::from_config_path;
use dengjen_synth::{
    AudioOutputConfig, AudioSamples, DengjenModel, DengjenResult, DengjenSpeechSynthesizer,
};
use once_cell::sync::Lazy;
use std::path::PathBuf;
use std::sync::Arc;

const TEXT: &[&'static str] = &[
    "No field advances on its own; someone always has to want the next improvement badly enough to build it, fund it, or demand it from those who can.",
    "Faster Networks",
    "Chief among these wants is the pressure to move information faster and more reliably, because nearly every modern institution now runs on top of a network."
];

static STD_VOICE: Lazy<Arc<dyn DengjenModel + Send + Sync>> = Lazy::new(|| {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("std")
        .join("model.onnx.json");
    from_config_path(&path).unwrap()
});

static RT_VOICE: Lazy<Arc<dyn DengjenModel + Send + Sync>> = Lazy::new(|| {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("models")
        .join("rt")
        .join("config.json");
    from_config_path(&path).unwrap()
});

#[allow(dead_code)]
pub fn init() {
    Lazy::force(&STD_VOICE);
    Lazy::force(&RT_VOICE);
}

pub fn gen_params(kind: &str) -> (DengjenSpeechSynthesizer, String, Option<AudioOutputConfig>) {
    let voice = match kind {
        "std" => Arc::clone(&STD_VOICE),
        "rt" => Arc::clone(&RT_VOICE),
        _ => panic!("Unrecognized voice kind requested."),
    };

    let synthesizer = DengjenSpeechSynthesizer::new(voice).unwrap();
    let joined_text = TEXT.join("\n");
    let audio_config = Some(AudioOutputConfig {
        rate: Some(50),
        volume: Some(50),
        pitch: Some(50),
        appended_silence_ms: None,
    });

    (synthesizer, joined_text, audio_config)
}

#[inline(always)]
pub fn iterate_stream(
    stream: impl Iterator<Item = DengjenResult<AudioSamples>>,
) -> DengjenResult<()> {
    for chunk_result in stream {
        let audio_samples = chunk_result?;
        let _ = black_box(audio_samples.as_wave_bytes());
    }
    Ok(())
}

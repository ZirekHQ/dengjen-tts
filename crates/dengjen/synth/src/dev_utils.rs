use core::hint::black_box;
use dengjen_tts::{
    AudioOutputConfig, AudioSamples, DengjenModel, DengjenResult, DengjenSpeechSynthesizer,
};
use dengjen_tts_piper::from_config_path;
use once_cell::sync::Lazy;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const TEXT: &[&str] = &[
    "No field advances on its own; someone always has to want the next improvement badly enough to build it, fund it, or demand it from those who can.",
    "Faster Networks",
    "Chief among these wants is the pressure to move information faster and more reliably, because nearly every modern institution now runs on top of a network.",
];

fn fixture_model_path(segments: &[&str]) -> PathBuf {
    segments
        .iter()
        .fold(PathBuf::from(env!("CARGO_MANIFEST_DIR")), |dir, segment| {
            dir.join(segment)
        })
}

fn load_voice(config_path: &Path) -> Arc<dyn DengjenModel + Send + Sync> {
    from_config_path(config_path).unwrap()
}

static STD_VOICE: Lazy<Arc<dyn DengjenModel + Send + Sync>> =
    Lazy::new(|| load_voice(&fixture_model_path(&["models", "std", "model.onnx.json"])));

static RT_VOICE: Lazy<Arc<dyn DengjenModel + Send + Sync>> =
    Lazy::new(|| load_voice(&fixture_model_path(&["models", "rt", "config.json"])));

#[allow(dead_code)]
pub fn init() {
    Lazy::force(&STD_VOICE);
    Lazy::force(&RT_VOICE);
}

pub fn gen_params(kind: &str) -> (DengjenSpeechSynthesizer, String, Option<AudioOutputConfig>) {
    let voice = match kind {
        "std" => Arc::clone(&STD_VOICE),
        "rt" => Arc::clone(&RT_VOICE),
        other => panic!("unrecognized voice kind requested: {other}"),
    };

    let synthesizer = DengjenSpeechSynthesizer::new(voice).unwrap();
    let text = TEXT.join("\n");
    let output_config = Some(AudioOutputConfig {
        rate: Some(50),
        volume: Some(50),
        pitch: Some(50),
        appended_silence_ms: None,
    });

    (synthesizer, text, output_config)
}

#[inline(always)]
pub fn iterate_stream(
    stream: impl Iterator<Item = DengjenResult<AudioSamples>>,
) -> DengjenResult<()> {
    for chunk in stream {
        black_box(chunk?.as_wave_bytes());
    }
    Ok(())
}

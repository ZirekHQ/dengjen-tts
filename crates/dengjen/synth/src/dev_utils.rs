use core::hint::black_box;
use dengjen_tts::{
    AudioOutputConfig, AudioSamples, DengjenModel, DengjenResult, DengjenSpeechSynthesizer,
};
use dengjen_tts_piper::from_config_path;
use once_cell::sync::OnceCell;
use std::path::PathBuf;
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

const STD_VOICE_FIXTURE: &[&str] = &["models", "std", "model.onnx.json"];
const RT_VOICE_FIXTURE: &[&str] = &["models", "rt", "config.json"];

static STD_VOICE: OnceCell<Arc<dyn DengjenModel + Send + Sync>> = OnceCell::new();
static RT_VOICE: OnceCell<Arc<dyn DengjenModel + Send + Sync>> = OnceCell::new();

fn load_voice(
    cell: &OnceCell<Arc<dyn DengjenModel + Send + Sync>>,
    segments: &[&str],
) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>> {
    cell.get_or_try_init(|| from_config_path(&fixture_model_path(segments)))
        .map(Arc::clone)
}

/// Returns `Ok(None)` when the fixture backing `kind` isn't present on disk, so
/// callers can skip rather than fail on machines without real Piper voices
/// (see CONTRIBUTING.md#benchmarks and issue #220). A present-but-invalid
/// fixture surfaces as `Err` rather than a silent skip.
pub fn gen_params(
    kind: &str,
) -> DengjenResult<Option<(DengjenSpeechSynthesizer, String, Option<AudioOutputConfig>)>> {
    let (fixture_path, segments, cell) = match kind {
        "std" => (
            fixture_model_path(STD_VOICE_FIXTURE),
            STD_VOICE_FIXTURE,
            &STD_VOICE,
        ),
        "rt" => (
            fixture_model_path(RT_VOICE_FIXTURE),
            RT_VOICE_FIXTURE,
            &RT_VOICE,
        ),
        other => panic!("unrecognized voice kind requested: {other}"),
    };
    if !fixture_path.exists() {
        return Ok(None);
    }

    let voice = load_voice(cell, segments)?;
    let synthesizer = DengjenSpeechSynthesizer::new(voice)?;
    let text = TEXT.join("\n");
    let output_config = Some(AudioOutputConfig {
        rate: Some(50),
        volume: Some(50),
        pitch: Some(50),
        appended_silence_ms: None,
    });

    Ok(Some((synthesizer, text, output_config)))
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

#[cfg(test)]
mod tests {
    #[test]
    fn fixture_model_path_joins_segments_onto_the_crate_manifest_dir() {
        let path = super::fixture_model_path(&["models", "std", "model.onnx.json"]);
        assert!(path.starts_with(env!("CARGO_MANIFEST_DIR")));
        assert!(path.ends_with("models/std/model.onnx.json"));
    }

    #[test]
    fn fixture_model_path_with_no_segments_is_just_the_manifest_dir() {
        let path = super::fixture_model_path(&[]);
        assert_eq!(path, std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")));
    }
}

#![forbid(unsafe_code)]

use clap::Parser;
use dengjen_tts::{
    AudioOutputConfig, AudioSamples, CancellationToken, DengjenModel, DengjenResult,
    DengjenSpeechSynthesizer, SynthesisConfig,
};
use dengjen_tts_piper::PiperSynthesisConfig;
use serde::Deserialize;
use std::fs::File;
use std::io::{self, prelude::*};
use std::path::PathBuf;

static INIT_ORT_ENVIRONMENT: std::sync::Once = std::sync::Once::new();

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
enum SynthesisMode {
    #[default]
    Lazy,
    Parallel,
    Realtime,
}

impl std::str::FromStr for SynthesisMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.to_lowercase();
        match normalized.as_str() {
            "lazy" => Ok(Self::Lazy),
            "parallel" => Ok(Self::Parallel),
            "realtime" => Ok(Self::Realtime),
            _ => Err(format!("Unknown synthesis mode: `{}`", s)),
        }
    }
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Model config
    config: PathBuf,
    /// Input text file (default `stdin`)
    #[arg(short = 'f', long, value_name = "INPUT_FILE")]
    input_file: Option<PathBuf>,
    /// Output file (default `stdout`)
    #[arg(short, long, value_name = "OUTPUT_FILE")]
    output_file: Option<PathBuf>,
    /// Synthesis mode (default `Lazy`)
    #[arg(long)]
    mode: Option<SynthesisMode>,
    /// Speaker ID for multi-speaker models (default `0`)
    #[arg(long)]
    speaker_id: Option<u32>,
    /// Piper length scale (default `model_default from config file`)
    #[arg(long)]
    length_scale: Option<f32>,
    /// Piper noise scale (default `model_default from config file`)
    #[arg(long)]
    noise_scale: Option<f32>,
    /// Piper noise width (default `model_default from config file`)
    #[arg(long)]
    noise_w: Option<f32>,
    /// Speaking rate [0 - 100] (default `50`)
    #[arg(long)]
    rate: Option<u8>,
    /// Speech pitch [0 - 100] (default `50`)
    #[arg(long)]
    pitch: Option<u8>,
    /// Speech volume [0 - 100] (default `75`)
    #[arg(long)]
    volume: Option<u8>,
    /// Extra silence (in milliseconds) to append to the end of each sentence (default `0`)
    #[arg(long)]
    silence: Option<u32>,
    /// Chunk granularity to stream (unit is backend-specific; e.g. Piper mel frames)
    #[arg(long)]
    chunk_size: Option<usize>,
    /// Number of mel frames to use for padding current chunk (improves naturalness)
    #[arg(long)]
    chunk_padding: Option<usize>,
}

#[derive(Deserialize, Default)]
struct SynthesisRequest {
    text: String,
    mode: Option<SynthesisMode>,
    speaker_id: Option<u32>,
    length_scale: Option<f32>,
    noise_scale: Option<f32>,
    noise_w: Option<f32>,
    rate: Option<u8>,
    pitch: Option<u8>,
    volume: Option<u8>,
    appended_silence_ms: Option<u32>,
    chunk_size: Option<usize>,
    chunk_padding: Option<usize>,
}

impl SynthesisRequest {
    fn as_piper_synth_config(&self, default_config: &PiperSynthesisConfig) -> PiperSynthesisConfig {
        let speaker = self.speaker_id.map(|id| id as i64);
        let length_scale = match self.length_scale {
            Some(v) => v,
            None => default_config.length_scale,
        };
        let noise_scale = match self.noise_scale {
            Some(v) => v,
            None => default_config.noise_scale,
        };
        let noise_w = match self.noise_w {
            Some(v) => v,
            None => default_config.noise_w,
        };

        PiperSynthesisConfig {
            speaker,
            length_scale,
            noise_scale,
            noise_w,
        }
    }

    fn as_audio_output_config(&self) -> AudioOutputConfig {
        AudioOutputConfig {
            rate: self.rate,
            pitch: self.pitch,
            volume: self.volume,
            appended_silence_ms: self.appended_silence_ms,
        }
    }
}

fn enable_logging() {
    let env = env_logger::Env::default().filter_or("DENGJEN_LOG", "info");
    env_logger::Builder::from_env(env).init();
}

fn get_synthesis_request_from_stdin() -> anyhow::Result<SynthesisRequest> {
    let mut line = String::new();
    io::stdin().read_line(&mut line)?;
    Ok(serde_json::from_str(&line)?)
}

fn process_synthesis_request<W: Write>(
    args: &Cli,
    synth: &DengjenSpeechSynthesizer,
    default_synth_config: &PiperSynthesisConfig,
    req: SynthesisRequest,
    writer: &mut W,
) -> anyhow::Result<()> {
    let piper_config = req.as_piper_synth_config(default_synth_config);
    synth.set_fallback_synthesis_config(&SynthesisConfig::Piper(piper_config))?;
    let output_config = Some(req.as_audio_output_config());

    if let Some(output_file) = &args.output_file {
        if req.mode.is_some() {
            log::warn!("Synthesis mode has no effect when output-file is set");
        }
        return synth
            .synthesize_to_file(output_file, req.text, output_config)
            .map_err(anyhow::Error::from);
    }

    match req.mode.unwrap_or_default() {
        SynthesisMode::Lazy => {
            let samples = synth
                .synthesize_lazy(req.text, output_config)?
                .map(|res| res.map(|aud| aud.samples));
            consume_stream(samples, writer)
        }
        SynthesisMode::Parallel => {
            let samples = synth
                .synthesize_parallel(req.text, output_config)?
                .map(|res| res.map(|aud| aud.samples));
            consume_stream(samples, writer)
        }
        SynthesisMode::Realtime => {
            let chunk_size = req.chunk_size.unwrap_or(100);
            let chunk_padding = req.chunk_padding.unwrap_or(3);
            let samples = synth.synthesize_streamed(
                req.text,
                output_config,
                chunk_size,
                chunk_padding,
                CancellationToken::new(),
            )?;
            consume_stream(samples, writer)
        }
    }
}

fn consume_stream(
    stream: impl Iterator<Item = DengjenResult<AudioSamples>>,
    writer: &mut impl Write,
) -> anyhow::Result<()> {
    for chunk in stream {
        let audio = chunk?;
        writer.write_all(&audio.as_wave_bytes())?;
        writer.flush()?;
    }
    Ok(())
}

#[cfg(test)]
mod synthesis_processing_tests {
    use super::*;
    use dengjen_tts::{
        Audio, AudioInfo, AudioStreamIterator, DengjenAudioResult, DengjenError, Phonemes,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;

    struct FakeDengjenModel {
        speakers: HashMap<i64, String>,
        fallback_config: Mutex<SynthesisConfig>,
        fail_speak: bool,
    }

    impl FakeDengjenModel {
        fn new() -> Self {
            Self::with_failure(false)
        }

        fn failing() -> Self {
            Self::with_failure(true)
        }

        fn with_failure(fail_speak: bool) -> Self {
            Self {
                speakers: HashMap::from([(0, "alice".to_string())]),
                fallback_config: Mutex::new(SynthesisConfig::Piper(PiperSynthesisConfig {
                    speaker: Some(0),
                    noise_scale: 0.667,
                    length_scale: 1.0,
                    noise_w: 0.8,
                })),
                fail_speak,
            }
        }
    }

    impl DengjenModel for FakeDengjenModel {
        fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
            Ok(AudioInfo {
                sample_rate: 22050,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec![text.to_string()]))
        }
        fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            phoneme_batches
                .into_iter()
                .map(|p| self.speak_one_sentence(p))
                .collect()
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            if self.fail_speak {
                return Err(DengjenError::OperationError("synthesis failed".to_string()));
            }
            Ok(Audio::new(
                AudioSamples::new(vec![0.0, 0.25, -0.25, 0.5]),
                22050,
                Some(1.0),
            ))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(self.fallback_config.lock().unwrap().clone())
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(self.fallback_config.lock().unwrap().clone())
        }
        fn set_fallback_synthesis_config(
            &self,
            synthesis_config: &SynthesisConfig,
        ) -> DengjenResult<()> {
            *self.fallback_config.lock().unwrap() = synthesis_config.clone();
            Ok(())
        }
        fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
            Ok(Some(&self.speakers))
        }
        fn supports_streaming_output(&self) -> bool {
            true
        }
        fn stream_synthesis(
            &self,
            _phonemes: String,
            _chunk_size: usize,
            _chunk_padding: usize,
            _cancel_token: CancellationToken,
        ) -> DengjenResult<AudioStreamIterator<'_>> {
            // `DengjenModel::stream_synthesis` defaults to
            // `Err(UnsupportedOperation(...))` — `RealtimeSpeechStream` calls it
            // unconditionally, so without this override the Realtime-mode test
            // below would fail before ever reaching `consume_stream`.
            if self.fail_speak {
                return Err(DengjenError::OperationError("synthesis failed".to_string()));
            }
            Ok(Box::new(std::iter::once(Ok(AudioSamples::new(vec![
                0.0, 0.25, -0.25, 0.5,
            ])))))
        }
    }

    fn fake_synth() -> DengjenSpeechSynthesizer {
        DengjenSpeechSynthesizer::new(std::sync::Arc::new(FakeDengjenModel::new())).unwrap()
    }

    fn failing_synth() -> DengjenSpeechSynthesizer {
        DengjenSpeechSynthesizer::new(std::sync::Arc::new(FakeDengjenModel::failing())).unwrap()
    }

    fn default_config() -> PiperSynthesisConfig {
        PiperSynthesisConfig::default()
    }

    fn cli_with_output_file(path: Option<std::path::PathBuf>) -> Cli {
        Cli {
            config: std::path::PathBuf::new(),
            input_file: None,
            output_file: path,
            mode: None,
            speaker_id: None,
            length_scale: None,
            noise_scale: None,
            noise_w: None,
            rate: None,
            pitch: None,
            volume: None,
            silence: None,
            chunk_size: None,
            chunk_padding: None,
        }
    }

    #[test]
    fn process_synthesis_request_lazy_mode_writes_pcm_bytes_to_the_writer() {
        let synth = fake_synth();
        let args = cli_with_output_file(None);
        let req = SynthesisRequest {
            text: "hello".to_string(),
            mode: Some(SynthesisMode::Lazy),
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer).unwrap();

        // Not an exact byte count: `process_synthesis_request` always wraps
        // output through `AudioOutputConfig::apply` (real Sonic FFI, even with
        // every field `None`), which this test isn't exercising — that's
        // `audio-ops`'s own tested responsibility (Phase 3). Non-empty output
        // is the right assertion here: it proves Lazy mode reached
        // `consume_stream` at all.
        assert!(!buffer.is_empty());
    }

    #[test]
    fn process_synthesis_request_parallel_mode_writes_pcm_bytes_to_the_writer() {
        let synth = fake_synth();
        let args = cli_with_output_file(None);
        let req = SynthesisRequest {
            text: "hello".to_string(),
            mode: Some(SynthesisMode::Parallel),
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer).unwrap();

        assert!(!buffer.is_empty());
    }

    #[test]
    fn process_synthesis_request_realtime_mode_writes_pcm_bytes_to_the_writer() {
        let synth = fake_synth();
        let args = cli_with_output_file(None);
        let req = SynthesisRequest {
            text: "hello".to_string(),
            mode: Some(SynthesisMode::Realtime),
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer).unwrap();

        assert!(!buffer.is_empty());
    }

    #[test]
    fn process_synthesis_request_defaults_to_lazy_mode_when_unset() {
        let synth = fake_synth();
        let args = cli_with_output_file(None);
        let req = SynthesisRequest {
            text: "hello".to_string(),
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer).unwrap();

        assert!(!buffer.is_empty());
    }

    #[test]
    fn process_synthesis_request_propagates_a_synthesis_error() {
        let synth = failing_synth();
        let args = cli_with_output_file(None);
        let req = SynthesisRequest {
            text: "hello".to_string(),
            mode: Some(SynthesisMode::Lazy),
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        let result = process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer);

        assert!(result.is_err());
    }

    #[test]
    fn process_synthesis_request_writes_to_the_output_file_when_set_instead_of_the_writer() {
        let synth = fake_synth();
        let dir = std::env::temp_dir().join(format!("dengjen-tts-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("process_synthesis_request_writes_to_the_output_file.wav");
        let args = cli_with_output_file(Some(path.clone()));
        let req = SynthesisRequest {
            text: "hello".to_string(),
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer).unwrap();

        assert!(path.exists());
        assert!(
            buffer.is_empty(),
            "output-file mode must not also write to the writer"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn consume_stream_stops_at_the_first_error_without_writing_further_chunks() {
        let stream: Vec<DengjenResult<AudioSamples>> = vec![
            Ok(AudioSamples::new(vec![0.0, 0.5])),
            Err(DengjenError::OperationError("boom".to_string())),
            Ok(AudioSamples::new(vec![0.0, 0.5])),
        ];
        let mut buffer: Vec<u8> = Vec::new();

        let result = consume_stream(stream.into_iter(), &mut buffer);

        assert!(result.is_err());
        assert_eq!(buffer.len(), 2 * 2); // only the first chunk's 2 samples were written
    }
}

fn init_ort_environment() {
    INIT_ORT_ENVIRONMENT.call_once(commit_onnxruntime);
}

fn commit_onnxruntime() {
    let providers = [
        #[cfg(feature = "cuda")]
        ort::execution_providers::CUDA::default().build(),
        ort::execution_providers::CPU::default().build(),
    ];
    let ok = ort::init()
        .with_name("dengjen")
        .with_execution_providers(providers)
        .commit();
    assert!(ok, "Failed to initialize onnxruntime");
}

fn detect_model_type(config_path: &std::path::Path) -> anyhow::Result<String> {
    let raw = std::fs::read_to_string(config_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&raw)?;
    let model_type = parsed
        .get("model_type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("piper");
    Ok(model_type.to_owned())
}

fn load_voice(
    config_path: &std::path::Path,
) -> anyhow::Result<std::sync::Arc<dyn dengjen_tts::DengjenModel + Send + Sync>> {
    let model_type = detect_model_type(config_path)?;
    if model_type == "kokoro" {
        return Ok(dengjen_tts_kokoro::from_config_path(config_path)?);
    }
    Ok(dengjen_tts_piper::from_config_path(config_path)?)
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn detect_model_type_recognizes_kokoro() {
        let dir = std::env::temp_dir().join("dengjen_cli_dispatch_test_kokoro");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_config(&dir, "config.json", r#"{"model_type": "kokoro"}"#);
        assert_eq!(detect_model_type(&path).unwrap(), "kokoro");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_model_type_defaults_to_piper_when_field_absent() {
        // Real Piper .onnx.json configs have no model_type field at all.
        let dir = std::env::temp_dir().join("dengjen_cli_dispatch_test_piper_default");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_config(&dir, "config.json", r#"{"audio": {"sample_rate": 22050}}"#);
        assert_eq!(detect_model_type(&path).unwrap(), "piper");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_model_type_errors_on_malformed_json() {
        let dir = std::env::temp_dir().join("dengjen_cli_dispatch_test_malformed");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_config(&dir, "config.json", "{ not valid");
        assert!(detect_model_type(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}

fn main() -> anyhow::Result<()> {
    enable_logging();
    init_ort_environment();

    let mut args = Cli::parse();

    let synth = {
        let voice = load_voice(&args.config)?;
        DengjenSpeechSynthesizer::new(voice)?
    };
    log::info!("Using model config: `{}`", args.config.display());
    // Non-Piper backends (e.g. Kokoro) return SynthesisConfig::None here; their
    // set_fallback_synthesis_config ignores whatever default we pass, so this
    // default is inert for them.
    let default_synth_config: PiperSynthesisConfig = match synth.get_default_synthesis_config()? {
        SynthesisConfig::Piper(cfg) => cfg,
        SynthesisConfig::None => PiperSynthesisConfig::default(),
    };
    if let Some(ref input_filename) = args.input_file {
        let mut input_buffer = String::new();
        let mut file = File::open(input_filename)?;
        file.read_to_string(&mut input_buffer)?;
        let req = SynthesisRequest {
            text: input_buffer,
            mode: args.mode.clone(),
            speaker_id: args.speaker_id,
            length_scale: args.length_scale,
            noise_scale: args.noise_scale,
            noise_w: args.noise_w,
            rate: args.rate,
            volume: args.volume,
            pitch: args.pitch,
            appended_silence_ms: args.silence,
            chunk_size: args.chunk_size,
            chunk_padding: args.chunk_padding,
        };
        process_synthesis_request(&args, &synth, &default_synth_config, req)?;
    } else {
        for i in 0.. {
            args.output_file = args.output_file.map(|file| {
                let enumerated_filename = format!(
                    "{}-{}.{}",
                    file.file_stem()
                        .expect("Invalid output file name")
                        .to_string_lossy(),
                    i + 1,
                    file.extension()
                        .expect("Invalid output file name")
                        .to_string_lossy()
                );
                file.with_file_name(enumerated_filename)
            });
            match get_synthesis_request_from_stdin() {
                Ok(req) => {
                    process_synthesis_request(&args, &synth, &default_synth_config, req)?;
                    if let Some(ref file) = args.output_file {
                        log::info!("Wrote output to file: {}", file.display());
                    }
                }
                Err(e) => log::error!("Invalid json input. Error: {}", e),
            };
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn synthesis_mode_from_str_parses_known_values_case_insensitively() {
        assert!(matches!(
            SynthesisMode::from_str("Lazy"),
            Ok(SynthesisMode::Lazy)
        ));
        assert!(matches!(
            SynthesisMode::from_str("PARALLEL"),
            Ok(SynthesisMode::Parallel)
        ));
        assert!(matches!(
            SynthesisMode::from_str("realtime"),
            Ok(SynthesisMode::Realtime)
        ));
    }

    #[test]
    fn synthesis_mode_from_str_returns_an_error_instead_of_panicking_on_unknown_value() {
        assert!(SynthesisMode::from_str("bogus").is_err());
    }

    #[test]
    fn as_piper_synth_config_falls_back_to_defaults_when_fields_are_none() {
        let default_config = PiperSynthesisConfig {
            speaker: Some(0),
            length_scale: 1.0,
            noise_scale: 0.667,
            noise_w: 0.8,
        };
        let req = SynthesisRequest {
            text: "hello".to_string(),
            ..Default::default()
        };
        let result = req.as_piper_synth_config(&default_config);
        assert_eq!(result.speaker, None);
        assert_eq!(result.length_scale, 1.0);
        assert_eq!(result.noise_scale, 0.667);
        assert_eq!(result.noise_w, 0.8);
    }

    #[test]
    fn as_piper_synth_config_overrides_defaults_when_fields_are_set() {
        let default_config = PiperSynthesisConfig::default();
        let req = SynthesisRequest {
            text: "hello".to_string(),
            speaker_id: Some(3),
            length_scale: Some(2.0),
            ..Default::default()
        };
        let result = req.as_piper_synth_config(&default_config);
        assert_eq!(result.speaker, Some(3));
        assert_eq!(result.length_scale, 2.0);
    }

    #[test]
    fn as_audio_output_config_carries_over_all_fields() {
        let req = SynthesisRequest {
            text: "hello".to_string(),
            rate: Some(80),
            pitch: Some(40),
            volume: Some(90),
            appended_silence_ms: Some(200),
            ..Default::default()
        };
        let config = req.as_audio_output_config();
        assert_eq!(config.rate, Some(80));
        assert_eq!(config.pitch, Some(40));
        assert_eq!(config.volume, Some(90));
        assert_eq!(config.appended_silence_ms, Some(200));
    }
}

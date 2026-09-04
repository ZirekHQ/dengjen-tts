#![forbid(unsafe_code)]

use clap::Parser;
use dengjen_tts::{
    AudioOutputConfig, AudioSamples, CancellationToken, DengjenModel, DengjenResult,
    DengjenSpeechSynthesizer, StreamMode, SynthesisConfig,
};
#[cfg(test)]
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

fn parse_param(s: &str) -> Result<(String, f32), String> {
    let (key, value) = s
        .split_once('=')
        .ok_or_else(|| format!("expected KEY=VALUE, got `{s}`"))?;
    let value: f32 = value
        .parse()
        .map_err(|_| format!("`{value}` is not a valid float"))?;
    Ok((key.to_string(), value))
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    
    config: PathBuf,
    
    #[arg(short = 'f', long, value_name = "INPUT_FILE")]
    input_file: Option<PathBuf>,
    
    #[arg(short, long, value_name = "OUTPUT_FILE")]
    output_file: Option<PathBuf>,
    
    #[arg(long)]
    mode: Option<SynthesisMode>,
    
    #[arg(long)]
    speaker_id: Option<u32>,
    
    #[arg(long)]
    length_scale: Option<f32>,
    
    #[arg(long)]
    noise_scale: Option<f32>,
    
    #[arg(long)]
    noise_w: Option<f32>,
    
    #[arg(long)]
    rate: Option<u8>,
    
    #[arg(long)]
    pitch: Option<u8>,
    
    #[arg(long)]
    volume: Option<u8>,
    
    #[arg(long)]
    silence: Option<u32>,
    
    #[arg(long)]
    chunk_size: Option<usize>,
    
    #[arg(long)]
    chunk_padding: Option<usize>,
    
    
    
    #[arg(long, value_parser = parse_param)]
    param: Vec<(String, f32)>,
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
    #[serde(default)]
    parameters: Vec<(String, f32)>,
}

impl SynthesisRequest {
    
    
    
    
    
    fn as_synthesis_config(&self, default_config: &SynthesisConfig) -> SynthesisConfig {
        let mut config = default_config.clone();
        for (key, value) in &self.parameters {
            config.parameters.insert(key.clone(), *value);
        }
        if let Some(length_scale) = self.length_scale {
            config.parameters.insert(
                dengjen_tts_piper::synth_config::LENGTH_SCALE.to_string(),
                length_scale,
            );
        }
        if let Some(noise_scale) = self.noise_scale {
            config.parameters.insert(
                dengjen_tts_piper::synth_config::NOISE_SCALE.to_string(),
                noise_scale,
            );
        }
        if let Some(noise_w) = self.noise_w {
            config.parameters.insert(
                dengjen_tts_piper::synth_config::NOISE_W.to_string(),
                noise_w,
            );
        }
        if let Some(speaker_id) = self.speaker_id {
            config.speaker = Some(i64::from(speaker_id));
        }
        config
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

fn read_synthesis_request<R: BufRead>(reader: &mut R) -> anyhow::Result<Option<SynthesisRequest>> {
    let mut line = String::new();
    match reader.read_line(&mut line) {
        Ok(0) => return Ok(None),
        Ok(_) => {}
        Err(err) => {
            log::error!("Failed to read from stdin: {}", err);
            return Ok(None);
        }
    }
    Ok(Some(serde_json::from_str(&line)?))
}

fn get_synthesis_request_from_stdin() -> anyhow::Result<Option<SynthesisRequest>> {
    read_synthesis_request(&mut io::stdin().lock())
}

fn process_synthesis_request<W: Write>(
    args: &Cli,
    synth: &DengjenSpeechSynthesizer,
    default_synth_config: &SynthesisConfig,
    req: SynthesisRequest,
    writer: &mut W,
) -> anyhow::Result<()> {
    let synthesis_config = req.as_synthesis_config(default_synth_config);
    synth.set_fallback_synthesis_config(&synthesis_config)?;
    let output_config = Some(req.as_audio_output_config());

    if let Some(output_file) = &args.output_file {
        if req.mode.is_some() {
            log::warn!("Synthesis mode has no effect when output-file is set");
        }
        return synth
            .synthesize_to_file(output_file, req.text, output_config)
            .map_err(anyhow::Error::from);
    }

    let mode = match req.mode.unwrap_or_default() {
        SynthesisMode::Lazy => StreamMode::Lazy,
        SynthesisMode::Parallel => StreamMode::Parallel,
        SynthesisMode::Realtime => StreamMode::Realtime {
            chunk_size: req.chunk_size.unwrap_or(100),
            chunk_padding: req.chunk_padding.unwrap_or(3),
            cancel_token: CancellationToken::new(),
        },
    };
    let samples = synth.synthesize_samples(req.text, output_config, mode)?;
    consume_stream(samples, writer)
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
        fallback_config: Mutex<PiperSynthesisConfig>,
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
                fallback_config: Mutex::new(PiperSynthesisConfig {
                    speaker: Some(0),
                    noise_scale: 0.667,
                    length_scale: 1.0,
                    noise_w: 0.8,
                }),
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
        fn get_default_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(Some(SynthesisConfig::from(
                &*self.fallback_config.lock().unwrap(),
            )))
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<Option<SynthesisConfig>> {
            Ok(Some(SynthesisConfig::from(
                &*self.fallback_config.lock().unwrap(),
            )))
        }
        fn set_fallback_synthesis_config(
            &self,
            synthesis_config: &SynthesisConfig,
        ) -> DengjenResult<()> {
            *self.fallback_config.lock().unwrap() = PiperSynthesisConfig::from(synthesis_config);
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

    fn default_config() -> SynthesisConfig {
        SynthesisConfig::default()
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
            param: Vec::new(),
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
    fn process_synthesis_request_warns_but_still_writes_to_file_when_mode_is_also_set() {
        let synth = fake_synth();
        let dir = std::env::temp_dir().join(format!("dengjen-tts-cli-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("process_synthesis_request_warns_but_still_writes_to_file.wav");
        let args = cli_with_output_file(Some(path.clone()));
        let req = SynthesisRequest {
            text: "hello".to_string(),
            mode: Some(SynthesisMode::Lazy), 
            ..Default::default()
        };
        let mut buffer: Vec<u8> = Vec::new();

        process_synthesis_request(&args, &synth, &default_config(), req, &mut buffer).unwrap();

        assert!(path.exists());
        assert!(
            buffer.is_empty(),
            "output-file mode must not also write to the writer, even when mode is set"
        );
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn a_default_parameter_not_covered_by_any_named_field_survives_into_the_synthesized_request() {
        
        
        
        let mut default_config = SynthesisConfig::default();
        default_config
            .parameters
            .insert("custom_knob".to_string(), 4.5);
        let req = SynthesisRequest {
            text: "hello".to_string(),
            ..Default::default()
        };
        let result = req.as_synthesis_config(&default_config);
        assert_eq!(result.parameters.get("custom_knob"), Some(&4.5));
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
        assert_eq!(buffer.len(), 2 * 2); 
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

fn load_voice(
    config_path: &std::path::Path,
) -> anyhow::Result<std::sync::Arc<dyn dengjen_tts::DengjenModel + Send + Sync>> {
    let model_type = dengjen_tts::detect_model_type(config_path)?;
    if model_type == "kokoro" {
        return Ok(dengjen_tts_kokoro::from_config_path(config_path)?);
    }
    if model_type == "melotts" {
        return Ok(dengjen_tts_melotts::from_config_path(config_path)?);
    }
    Ok(dengjen_tts_piper::from_config_path(config_path)?)
}

fn main() -> anyhow::Result<()> {
    enable_logging();
    init_ort_environment();

    let mut cli = Cli::parse();
    let synthesizer = build_synthesizer(&cli.config)?;
    let default_synth_config = resolve_default_synthesis_config(&synthesizer)?;

    if let Some(input_path) = cli.input_file.take() {
        return synthesize_from_file(&cli, &input_path, &synthesizer, &default_synth_config);
    }
    synthesize_from_stdin_forever(cli, &synthesizer, &default_synth_config)
}

fn build_synthesizer(config_path: &std::path::Path) -> anyhow::Result<DengjenSpeechSynthesizer> {
    let voice = load_voice(config_path)?;
    let synthesizer = DengjenSpeechSynthesizer::new(voice)?;
    log::info!("Using model config: `{}`", config_path.display());
    Ok(synthesizer)
}

fn resolve_default_synthesis_config(
    synthesizer: &DengjenSpeechSynthesizer,
) -> anyhow::Result<SynthesisConfig> {
    
    
    
    Ok(synthesizer
        .get_default_synthesis_config()?
        .unwrap_or_default())
}

fn synthesis_request_from_cli(cli: &Cli, text: String) -> SynthesisRequest {
    SynthesisRequest {
        text,
        mode: cli.mode.clone(),
        speaker_id: cli.speaker_id,
        length_scale: cli.length_scale,
        noise_scale: cli.noise_scale,
        noise_w: cli.noise_w,
        rate: cli.rate,
        pitch: cli.pitch,
        volume: cli.volume,
        appended_silence_ms: cli.silence,
        chunk_size: cli.chunk_size,
        chunk_padding: cli.chunk_padding,
        parameters: cli.param.clone(),
    }
}

fn synthesize_from_file(
    cli: &Cli,
    input_path: &std::path::Path,
    synthesizer: &DengjenSpeechSynthesizer,
    default_synth_config: &SynthesisConfig,
) -> anyhow::Result<()> {
    let mut file = File::open(input_path)?;
    let mut text = String::new();
    file.read_to_string(&mut text)?;
    let request = synthesis_request_from_cli(cli, text);
    process_synthesis_request(
        cli,
        synthesizer,
        default_synth_config,
        request,
        &mut io::stdout().lock(),
    )
}

fn synthesize_from_stdin_forever(
    mut cli: Cli,
    synthesizer: &DengjenSpeechSynthesizer,
    default_synth_config: &SynthesisConfig,
) -> anyhow::Result<()> {
    let mut request_count: u64 = 0;
    loop {
        request_count += 1;
        cli.output_file = cli
            .output_file
            .take()
            .map(|path| enumerate_output_path(path, request_count));

        match get_synthesis_request_from_stdin() {
            Ok(Some(request)) => {
                process_synthesis_request(
                    &cli,
                    synthesizer,
                    default_synth_config,
                    request,
                    &mut io::stdout().lock(),
                )?;
                if let Some(output_path) = &cli.output_file {
                    log::info!("Wrote output to file: {}", output_path.display());
                }
            }
            Ok(None) => {
                log::info!("stdin closed, exiting");
                return Ok(());
            }
            Err(err) => log::error!("Invalid json input. Error: {}", err),
        }
    }
}

fn enumerate_output_path(path: PathBuf, suffix: u64) -> PathBuf {
    let stem = path
        .file_stem()
        .expect("Invalid output file name")
        .to_string_lossy();
    let extension = path
        .extension()
        .expect("Invalid output file name")
        .to_string_lossy();
    let enumerated_name = format!("{}-{}.{}", stem, suffix, extension);
    path.with_file_name(enumerated_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    struct FailingReader;

    impl Read for FailingReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("simulated I/O failure"))
        }
    }

    #[test]
    fn read_synthesis_request_returns_none_on_io_error_instead_of_looping_forever() {
        let mut reader = io::BufReader::new(FailingReader);
        let result = read_synthesis_request(&mut reader);
        assert!(matches!(result, Ok(None)));
    }

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
    fn as_synthesis_config_falls_back_to_defaults_when_fields_are_none() {
        let default_config = SynthesisConfig::from(&PiperSynthesisConfig {
            speaker: Some(0),
            length_scale: 1.0,
            noise_scale: 0.667,
            noise_w: 0.8,
        });
        let req = SynthesisRequest {
            text: "hello".to_string(),
            ..Default::default()
        };
        let result = req.as_synthesis_config(&default_config);
        assert_eq!(result.speaker, Some(0));
        assert_eq!(result.parameters.get("length_scale"), Some(&1.0));
        assert_eq!(result.parameters.get("noise_scale"), Some(&0.667));
        assert_eq!(result.parameters.get("noise_w"), Some(&0.8));
    }

    #[test]
    fn as_synthesis_config_overrides_defaults_when_fields_are_set() {
        let default_config = SynthesisConfig::from(&PiperSynthesisConfig::default());
        let req = SynthesisRequest {
            text: "hello".to_string(),
            speaker_id: Some(3),
            length_scale: Some(2.0),
            ..Default::default()
        };
        let result = req.as_synthesis_config(&default_config);
        assert_eq!(result.speaker, Some(3));
        assert_eq!(result.parameters.get("length_scale"), Some(&2.0));
    }

    #[test]
    fn synthesis_request_parameters_flow_into_the_generic_synthesis_config() {
        let default_config = SynthesisConfig::from(&PiperSynthesisConfig::default());
        let req = SynthesisRequest {
            text: "hello".to_string(),
            parameters: vec![("custom_knob".to_string(), 1.25)],
            ..Default::default()
        };
        let synthesis_config = req.as_synthesis_config(&default_config);
        assert_eq!(synthesis_config.parameters.get("custom_knob"), Some(&1.25));
    }

    #[test]
    fn as_synthesis_config_merges_generic_parameters_the_named_fields_dont_cover() {
        let default_config = SynthesisConfig::from(&PiperSynthesisConfig {
            speaker: Some(0),
            length_scale: 1.0,
            noise_scale: 0.667,
            noise_w: 0.8,
        });
        let req = SynthesisRequest {
            text: "hello".to_string(),
            parameters: vec![("length_scale".to_string(), 2.5)],
            ..Default::default()
        };
        let result = req.as_synthesis_config(&default_config);
        assert_eq!(result.parameters.get("length_scale"), Some(&2.5));
    }

    #[test]
    fn as_synthesis_config_prefers_the_named_field_over_a_conflicting_parameters_key() {
        let default_config = SynthesisConfig::from(&PiperSynthesisConfig {
            speaker: Some(0),
            length_scale: 1.0,
            noise_scale: 0.667,
            noise_w: 0.8,
        });
        let req = SynthesisRequest {
            text: "hello".to_string(),
            length_scale: Some(3.0),
            parameters: vec![("length_scale".to_string(), 9.9)],
            ..Default::default()
        };
        let result = req.as_synthesis_config(&default_config);
        assert_eq!(
            result.parameters.get("length_scale"),
            Some(&3.0),
            "named field must win over a conflicting parameters key"
        );
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

    #[test]
    fn enumerate_output_path_inserts_the_suffix_before_the_extension() {
        let result = enumerate_output_path(PathBuf::from("out.wav"), 1);
        assert_eq!(result, PathBuf::from("out-1.wav"));
    }

    #[test]
    fn enumerate_output_path_is_cumulative_across_repeated_calls() {
        
        
        let first = enumerate_output_path(PathBuf::from("out.wav"), 1);
        let second = enumerate_output_path(first, 2);
        assert_eq!(second, PathBuf::from("out-1-2.wav"));
    }

    #[test]
    #[should_panic(expected = "Invalid output file name")]
    fn enumerate_output_path_panics_on_a_path_with_no_extension() {
        enumerate_output_path(PathBuf::from("noext"), 1);
    }
}

use clap::Parser;
use serde::Deserialize;
use dengjen_piper::PiperSynthesisConfig;
use dengjen_synth::{
    AudioOutputConfig, AudioSamples, CancellationToken, DengjenModel, DengjenResult,
    DengjenSpeechSynthesizer,
};
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

    fn from_str(other: &str) -> Result<Self, Self::Err> {
        match other.to_lowercase().as_str() {
            "lazy" => Ok(Self::Lazy),
            "parallel" => Ok(Self::Parallel),
            "realtime" => Ok(Self::Realtime),
            _ => Err(format!("Unknown synthesis mode: `{}`", other)),
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
    /// Number of mel frames to stream for each chunk
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
        PiperSynthesisConfig {
            speaker: self.speaker_id.map(i64::from),
            length_scale: self.length_scale.unwrap_or(default_config.length_scale),
            noise_scale: self.noise_scale.unwrap_or(default_config.noise_scale),
            noise_w: self.noise_w.unwrap_or(default_config.noise_w),
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
    env_logger::Builder::from_env(env_logger::Env::default().filter_or("DENGJEN_LOG", "info"))
        .init();
}

fn get_synthesis_request_from_stdin() -> anyhow::Result<SynthesisRequest> {
    let mut input_buffer = String::new();
    let stdin = io::stdin();
    stdin.read_line(&mut input_buffer)?;
    let req: SynthesisRequest = serde_json::from_str(&input_buffer)?;
    Ok(req)
}

fn process_synthesis_request(
    args: &Cli,
    synth: &DengjenSpeechSynthesizer,
    default_synth_config: &PiperSynthesisConfig,
    req: SynthesisRequest,
) -> anyhow::Result<()> {
    synth.set_fallback_synthesis_config(&req.as_piper_synth_config(default_synth_config))?;
    let output_config = Some(req.as_audio_output_config());
    if let Some(output_file) = args.output_file.as_ref() {
        if req.mode.is_some() {
            log::warn!("Synthesis mode has no effect when output-file is set");
        }
        synth.synthesize_to_file(output_file, req.text, output_config)?;
        return Ok(());
    }
    match req.mode.unwrap_or_default() {
        SynthesisMode::Lazy => {
            let stream = synth
                .synthesize_lazy(req.text, output_config)?
                .map(|res| res.map(|aud| aud.samples));
            consume_stream(stream)?
        }
        SynthesisMode::Parallel => {
            let stream = synth
                .synthesize_parallel(req.text, output_config)?
                .map(|res| res.map(|aud| aud.samples));
            consume_stream(stream)?
        }
        SynthesisMode::Realtime => {
            let stream = synth.synthesize_streamed(
                req.text,
                output_config,
                req.chunk_size.unwrap_or(100),
                req.chunk_padding.unwrap_or(3),
                CancellationToken::new(),
            )?;
            consume_stream(stream)?
        }
    };
    Ok(())
}

fn write_to_stdout(data: &[u8]) -> anyhow::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(data)?;
    stdout.flush()?;
    Ok(())
}

#[inline(always)]
fn consume_stream(stream: impl Iterator<Item = DengjenResult<AudioSamples>>) -> anyhow::Result<()> {
    for result in stream {
        let audio = result?;
        let wav_bytes = audio.as_wave_bytes();
        write_to_stdout(&wav_bytes)?;
    }
    Ok(())
}

fn init_ort_environment() {
    INIT_ORT_ENVIRONMENT.call_once(|| {
        let execution_providers = [
            #[cfg(feature = "cuda")]
            ort::execution_providers::CUDA::default().build(),
            ort::execution_providers::CPU::default().build(),
        ];
        let committed = ort::init()
            .with_name("dengjen")
            .with_execution_providers(execution_providers)
            .commit();
        assert!(committed, "Failed to initialize onnxruntime");
    });
}

fn main() -> anyhow::Result<()> {
    enable_logging();
    init_ort_environment();

    let mut args = Cli::parse();

    let synth = {
        let voice = dengjen_piper::from_config_path(&args.config)?;
        DengjenSpeechSynthesizer::new(voice)?
    };
    log::info!("Using model config: `{}`", args.config.display());
    let default_synth_config: PiperSynthesisConfig = *synth
        .get_default_synthesis_config()?
        .downcast()
        .expect("Invalid default synthesis config. Expected Piper config.");
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
        assert!(matches!(SynthesisMode::from_str("Lazy"), Ok(SynthesisMode::Lazy)));
        assert!(matches!(SynthesisMode::from_str("PARALLEL"), Ok(SynthesisMode::Parallel)));
        assert!(matches!(SynthesisMode::from_str("realtime"), Ok(SynthesisMode::Realtime)));
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

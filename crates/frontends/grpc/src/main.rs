#![forbid(unsafe_code)]

use dengjen_tts_core::{CancellationToken, DengjenError, DengjenModel, DengjenResult, SynthesisConfig};
use dengjen_tts::{AudioOutputConfig, DengjenSpeechStreamLazy, DengjenSpeechSynthesizer};
use grpc::dengjen_grpc_server::{DengjenGrpc, DengjenGrpcServer};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Server;
use tonic::{Request, Response, Status};
use xxhash_rust::xxh3::xxh3_64;

const DEFAULT_DENGJEN_GRPC_SERVER_PORT: u16 = 49314;
const VOICE_ID_REDUCTION_FACTOR: u64 = 10000000000000;

type DengjenGrpcResult<T> = Result<T, DengjenGrpcError>;

pub mod grpc {
    tonic::include_proto!("dengjen_grpc");
}

#[derive(Debug)]
enum DengjenGrpcError {
    DengjenError(DengjenError),
    VoiceNotFound(String),
}

impl std::error::Error for DengjenGrpcError {}

impl std::fmt::Display for DengjenGrpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DengjenError(e) => write!(f, "{}", e),
            Self::VoiceNotFound(msg) => write!(f, "{}", msg),
        }
    }
}

impl From<DengjenError> for DengjenGrpcError {
    fn from(err: DengjenError) -> Self {
        Self::DengjenError(err)
    }
}

impl From<DengjenGrpcError> for Status {
    fn from(err: DengjenGrpcError) -> Self {
        match err {
            DengjenGrpcError::DengjenError(e) => match e {
                DengjenError::FailedToLoadResource(msg) => Status::aborted(msg),
                DengjenError::PhonemizationError(msg) => Status::aborted(msg),
                DengjenError::InferenceError(msg) => Status::internal(msg),
                DengjenError::InvalidConfiguration(msg) => Status::invalid_argument(msg),
                DengjenError::UnsupportedOperation(msg) => Status::unimplemented(msg),
                DengjenError::OperationError(msg) => Status::unknown(msg),
            },
            DengjenGrpcError::VoiceNotFound(msg) => Status::not_found(msg),
        }
    }
}

/// A loaded voice, backed by a speech synthesizer wrapping some `DengjenModel`.
struct Voice(Arc<DengjenSpeechSynthesizer>);

impl Voice {
    fn new(model: Arc<dyn DengjenModel + Send + Sync>) -> DengjenResult<Self> {
        Ok(Self(Arc::new(DengjenSpeechSynthesizer::new(model)?)))
    }

    /// `DengjenSpeechSynthesizer` implements `DengjenModel` by delegating to the
    /// wrapped model, so the synthesizer itself can stand in as the model reference.
    fn model_ref(&self) -> &dyn DengjenModel {
        self.synth_ref()
    }

    fn synth_ref(&self) -> &DengjenSpeechSynthesizer {
        &self.0
    }
}

/// Registry of loaded voices, keyed by voice ID.
struct DengjenGrpcService(RwLock<HashMap<String, Voice>>);

impl DengjenGrpcService {
    fn new() -> Self {
        Self(RwLock::new(HashMap::new()))
    }

    /// Derives a stable voice ID from a canonicalized config path.
    fn voice_id_for_path(config_path: &std::path::Path) -> String {
        let canonical_path = config_path
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .into_owned();
        (xxh3_64(canonical_path.as_bytes()) / VOICE_ID_REDUCTION_FACTOR).to_string()
    }

    fn load_voice_from_config(&self, config_path: PathBuf) -> DengjenGrpcResult<grpc::VoiceInfo> {
        if !config_path.is_file() {
            return Err(DengjenGrpcError::VoiceNotFound(format!(
                "Config file does not exists: `{}`",
                config_path.display()
            )));
        }
        let voice_id = Self::voice_id_for_path(&config_path);
        if let Some(voice) = self.0.read().unwrap().get(&voice_id) {
            return self.build_voice_info(voice_id, voice.model_ref());
        }

        let model = dengjen_tts_piper::from_config_path(&config_path)?;
        log::info!(
            "Loaded Vits voice from: `{}`. Voice ID: {}",
            config_path.display(),
            voice_id
        );
        let voice = Voice::new(model)?;
        let voice_info = self.build_voice_info(voice_id.clone(), voice.model_ref())?;
        self.0.write().unwrap().insert(voice_id, voice);
        Ok(voice_info)
    }

    fn open_synthesis_stream(
        &self,
        voice_id: &str,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenGrpcResult<DengjenSpeechStreamLazy> {
        let voices = self.0.read().unwrap();
        let voice = voices.get(voice_id).ok_or_else(|| {
            DengjenGrpcError::VoiceNotFound(format!(
                "A voice with the key `{}` has not been loaded",
                voice_id
            ))
        })?;
        Ok(voice.synth_ref().synthesize_lazy(text, output_config)?)
    }

    fn build_voice_info(
        &self,
        voice_id: String,
        model: &(impl DengjenModel + ?Sized),
    ) -> DengjenGrpcResult<grpc::VoiceInfo> {
        let audio_info = model.audio_output_info()?;
        let speakers = model.get_speakers()?.cloned().unwrap_or_default();
        let language = model.get_language()?;
        let synth_options = Self::synth_options_from_default_config(model)?;
        Ok(grpc::VoiceInfo {
            voice_id,
            synth_options: Some(synth_options),
            language,
            speakers,
            audio: Some(grpc::AudioInfo {
                sample_rate: audio_info.sample_rate as u32,
                num_channels: audio_info.num_channels as u32,
                sample_width: audio_info.sample_width as u32,
            }),
            supports_streaming_output: Some(model.supports_streaming_output()),
            quality: None,
        })
    }

    /// Turns an already-resolved Piper config into gRPC-facing `SynthesisOptions`,
    /// looking up the speaker's display name when set and otherwise deferring to
    /// `on_unset_speaker` (the two config sources disagree on the unset fallback).
    fn synth_options_from_piper_config(
        model: &(impl DengjenModel + ?Sized),
        config: dengjen_tts_core::PiperSynthesisConfig,
        on_unset_speaker: impl FnOnce() -> DengjenGrpcResult<Option<String>>,
    ) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let speaker = match config.speaker {
            Some(ref speaker_id) => model.speaker_id_to_name(speaker_id)?,
            None => on_unset_speaker()?,
        };
        Ok(grpc::SynthesisOptions {
            speaker,
            length_scale: Some(config.length_scale),
            noise_scale: Some(config.noise_scale),
            noise_w: Some(config.noise_w),
        })
    }

    fn synth_options_from_default_config(
        model: &(impl DengjenModel + ?Sized),
    ) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let config = match model.get_default_synthesis_config()? {
            SynthesisConfig::Piper(config) => config,
            SynthesisConfig::None => {
                return Err(DengjenError::InvalidConfiguration(
                    "Invalid synthesis config for Vits model".to_string(),
                )
                .into())
            }
        };
        Self::synth_options_from_piper_config(model, config, || Ok(Some("Default".to_string())))
    }

    /// Reads a model's fallback synthesis config as gRPC-facing `SynthesisOptions`,
    /// resolving an unset speaker to speaker ID `0`'s name.
    fn synth_options_from_model(
        model: &(impl DengjenModel + ?Sized),
    ) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let config = match model.get_fallback_synthesis_config()? {
            SynthesisConfig::Piper(config) => config,
            SynthesisConfig::None => {
                return Err(DengjenError::InvalidConfiguration(
                    "Invalid synthesis config for Vits model".to_string(),
                )
                .into())
            }
        };
        Self::synth_options_from_piper_config(model, config, || Ok(model.speaker_id_to_name(&0)?))
    }

    fn lookup_synth_options(&self, voice_id: &str) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let voices = self.0.read().unwrap();
        let voice = voices.get(voice_id).ok_or_else(|| {
            DengjenGrpcError::VoiceNotFound(format!(
                "A voice with the key `{}` has not been loaded",
                voice_id
            ))
        })?;
        Self::synth_options_from_model(voice.model_ref())
    }

    /// Applies each `Some` field of `synth_opts` onto the voice's fallback synthesis
    /// config, leaving unset fields (and an unresolvable speaker name) untouched.
    fn apply_synth_options(
        &self,
        voice_id: &str,
        synth_opts: grpc::SynthesisOptions,
    ) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let voices = self.0.read().unwrap();
        let voice = voices.get(voice_id).ok_or_else(|| {
            DengjenGrpcError::VoiceNotFound(format!(
                "A voice with the key `{}` has not been loaded",
                voice_id
            ))
        })?;
        let model = voice.model_ref();
        let mut config = match model.get_fallback_synthesis_config()? {
            SynthesisConfig::Piper(config) => config,
            SynthesisConfig::None => {
                return Err(DengjenError::InvalidConfiguration(
                    "Could not set synthesis parameters ".to_string(),
                )
                .into())
            }
        };
        if let Some(speaker_name) = synth_opts.speaker {
            if let Some(speaker_id) = model.speaker_name_to_id(&speaker_name)? {
                config.speaker = Some(speaker_id);
            }
        }
        if let Some(length_scale) = synth_opts.length_scale {
            config.length_scale = length_scale;
        }
        if let Some(noise_scale) = synth_opts.noise_scale {
            config.noise_scale = noise_scale;
        }
        if let Some(noise_w) = synth_opts.noise_w {
            config.noise_w = noise_w;
        }
        model.set_fallback_synthesis_config(&SynthesisConfig::Piper(config))?;
        Self::synth_options_from_model(model)
    }
}

/// Converts the optional proto `SpeechArgs` into synth's `AudioOutputConfig`,
/// narrowing each field from the proto's wider integer types.
fn output_config_from_speech_args(args: Option<grpc::SpeechArgs>) -> Option<AudioOutputConfig> {
    args.map(|args| AudioOutputConfig {
        rate: args.rate.map(|v| v as u8),
        volume: args.volume.map(|v| v as u8),
        pitch: args.pitch.map(|v| v as u8),
        appended_silence_ms: args.appended_silence_ms,
    })
}

/// Drains a blocking synthesis-result iterator into a channel, mapping each
/// produced item to its wire message via `to_message`. Stops early either by
/// forwarding a mapped error (once, then returning) or silently once the
/// receiver side has gone away (`blocking_send` failing).
fn drain_stream_into_channel<Chunk, Message>(
    stream: impl Iterator<Item = DengjenResult<Chunk>>,
    tx: mpsc::Sender<Result<Message, Status>>,
    to_message: impl Fn(Chunk) -> Message,
) {
    for chunk_result in stream {
        let chunk = match chunk_result {
            Ok(chunk) => chunk,
            Err(e) => {
                tx.blocking_send(Err(DengjenGrpcError::from(e).into())).ok();
                return;
            }
        };
        if tx.blocking_send(Ok(to_message(chunk))).is_err() {
            return;
        }
    }
}

#[tonic::async_trait]
impl DengjenGrpc for DengjenGrpcService {
    async fn get_dengjen_version(
        &self,
        _request: Request<grpc::Empty>,
    ) -> Result<Response<grpc::Version>, Status> {
        Ok(Response::new(grpc::Version {
            version: env!("CARGO_PKG_VERSION").into(),
        }))
    }

    async fn load_voice(
        &self,
        request: Request<grpc::VoicePath>,
    ) -> Result<Response<grpc::VoiceInfo>, Status> {
        let config_path = PathBuf::from(request.into_inner().config_path);
        let voice_info = self.load_voice_from_config(config_path)?;
        Ok(Response::new(voice_info))
    }

    async fn get_voice_info(
        &self,
        request: Request<grpc::VoiceIdentifier>,
    ) -> Result<Response<grpc::VoiceInfo>, Status> {
        let voice_id = request.into_inner().voice_id;
        let voices = self.0.read().unwrap();
        let voice = voices.get(&voice_id).ok_or_else(|| {
            DengjenGrpcError::VoiceNotFound(format!(
                "A voice with the key `{}` has not been loaded",
                voice_id
            ))
        })?;
        let voice_info = self.build_voice_info(voice_id, voice.model_ref())?;
        Ok(Response::new(voice_info))
    }

    async fn get_synthesis_options(
        &self,
        request: Request<grpc::VoiceIdentifier>,
    ) -> Result<Response<grpc::SynthesisOptions>, Status> {
        let voice_id = request.into_inner().voice_id;
        let synth_opts = self.lookup_synth_options(&voice_id)?;
        Ok(Response::new(synth_opts))
    }

    async fn set_synthesis_options(
        &self,
        request: Request<grpc::VoiceSynthesisOptions>,
    ) -> Result<Response<grpc::SynthesisOptions>, Status> {
        let req = request.into_inner();
        let synth_opts = req
            .synthesis_options
            .ok_or_else(|| Status::invalid_argument("No synthesis options provided"))?;
        let new_synth_opts = self.apply_synth_options(&req.voice_id, synth_opts)?;
        Ok(Response::new(new_synth_opts))
    }

    type SynthesizeUtteranceStream = ReceiverStream<Result<grpc::SynthesisResult, Status>>;
    async fn synthesize_utterance(
        &self,
        request: Request<grpc::Utterance>,
    ) -> Result<Response<Self::SynthesizeUtteranceStream>, Status> {
        let req = request.into_inner();
        let output_config = output_config_from_speech_args(req.speech_args);
        let dengjen_stream = self.open_synthesis_stream(&req.voice_id, req.text, output_config)?;

        let (tx, rx) = mpsc::channel(512);
        tokio::task::spawn_blocking(move || {
            drain_stream_into_channel(dengjen_stream, tx, |wav| grpc::SynthesisResult {
                wav_samples: wav.as_wave_bytes(),
                rtf: wav.real_time_factor().unwrap_or_default(),
            });
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    type SynthesizeUtteranceRealtimeStream = ReceiverStream<Result<grpc::WaveSamples, Status>>;
    async fn synthesize_utterance_realtime(
        &self,
        request: Request<grpc::Utterance>,
    ) -> Result<Response<Self::SynthesizeUtteranceRealtimeStream>, Status> {
        let req = request.into_inner();
        let output_config = output_config_from_speech_args(req.speech_args);
        let synth = {
            let voices = self.0.read().unwrap();
            let voice = voices.get(&req.voice_id).ok_or_else(|| {
                DengjenGrpcError::VoiceNotFound(format!(
                    "A voice with the key `{}` has not been loaded",
                    req.voice_id
                ))
            })?;
            Arc::clone(&voice.0)
        };

        let (tx, rx) = mpsc::channel(512);
        tokio::task::spawn_blocking(move || {
            let realtime_speech_stream = match synth.synthesize_streamed(
                req.text,
                output_config,
                55,
                3,
                CancellationToken::new(),
            ) {
                Ok(stream) => stream,
                Err(e) => {
                    tx.blocking_send(Err(DengjenGrpcError::from(e).into())).ok();
                    return;
                }
            };
            drain_stream_into_channel(realtime_speech_stream, tx, |wav| grpc::WaveSamples {
                wav_samples: wav.as_wave_bytes(),
            });
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

/// Initializes the global logger, defaulting to `info` level unless overridden
/// via the `DENGJEN_GRPC` environment variable.
fn setup_logging() {
    let log_filter = env_logger::Env::default().filter_or("DENGJEN_GRPC", "info");
    env_logger::init_from_env(log_filter);
}

/// Commits a CPU-backed ONNX Runtime environment named `"dengjen"` as the
/// process-wide default. Returns `false` (without panicking) if commit fails.
fn init_ort_environment() -> bool {
    let cpu_provider = ort::execution_providers::CPU::default().build();
    ort::init()
        .with_name("dengjen")
        .with_execution_providers([cpu_provider])
        .commit()
}

/// Resolves the TCP port to listen on from `DENGJEN_GRPC_SERVER_PORT`,
/// falling back to [`DEFAULT_DENGJEN_GRPC_SERVER_PORT`] when the variable is
/// unset or does not parse as a `u16`.
fn resolve_listen_port() -> u16 {
    std::env::var("DENGJEN_GRPC_SERVER_PORT")
        .ok()
        .and_then(|raw_port| raw_port.parse().ok())
        .unwrap_or(DEFAULT_DENGJEN_GRPC_SERVER_PORT)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    // A failed ORT init is logged but not fatal: voice loading will surface
    // its own error later if the runtime is genuinely unusable.
    if !init_ort_environment() {
        log::error!("Could not initialize onnxruntime environment");
    }

    let addr = SocketAddr::new(
        IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        resolve_listen_port(),
    );
    log::info!("Starting Dengjen GRPC server at address: {}", addr);

    let server = DengjenGrpcServer::new(DengjenGrpcService::new());
    Server::builder().add_service(server).serve(addr).await?;

    Ok(())
}

#[cfg(test)]
mod error_mapping_tests {
    use super::*;

    #[test]
    fn voice_not_found_displays_its_message_verbatim() {
        let err = DengjenGrpcError::VoiceNotFound("no such voice".to_string());
        assert_eq!(err.to_string(), "no such voice");
    }

    #[test]
    fn dengjen_error_display_delegates_to_the_wrapped_error() {
        let inner = DengjenError::OperationError("boom".to_string());
        let err = DengjenGrpcError::from(inner);
        assert_eq!(
            err.to_string(),
            DengjenError::OperationError("boom".to_string()).to_string()
        );
    }

    #[test]
    fn voice_not_found_maps_to_status_not_found() {
        let status: Status = DengjenGrpcError::VoiceNotFound("x".to_string()).into();
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert_eq!(status.message(), "x");
    }

    #[test]
    fn failed_to_load_resource_maps_to_status_aborted() {
        let status: Status =
            DengjenGrpcError::from(DengjenError::FailedToLoadResource("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Aborted);
    }

    #[test]
    fn phonemization_error_maps_to_status_aborted() {
        let status: Status =
            DengjenGrpcError::from(DengjenError::PhonemizationError("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Aborted);
    }

    #[test]
    fn inference_error_maps_to_status_internal() {
        let status: Status =
            DengjenGrpcError::from(DengjenError::InferenceError("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn invalid_configuration_maps_to_status_invalid_argument() {
        let status: Status =
            DengjenGrpcError::from(DengjenError::InvalidConfiguration("x".into())).into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn unsupported_operation_maps_to_status_unimplemented() {
        let status: Status =
            DengjenGrpcError::from(DengjenError::UnsupportedOperation("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[test]
    fn operation_error_maps_to_status_unknown() {
        let status: Status =
            DengjenGrpcError::from(DengjenError::OperationError("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Unknown);
    }
}

#[cfg(test)]
mod voice_loading_tests {
    use super::*;
    use dengjen_tts_core::{Audio, AudioInfo as CoreAudioInfo, DengjenAudioResult, Phonemes};
    use std::collections::HashMap as StdHashMap;

    struct FakeModel {
        speakers: StdHashMap<i64, String>,
    }

    impl DengjenModel for FakeModel {
        fn audio_output_info(&self) -> DengjenResult<CoreAudioInfo> {
            Ok(CoreAudioInfo {
                sample_rate: 22050,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["fake".to_string()]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Ok(Audio::new(Default::default(), 22050, None))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::Piper(Default::default()))
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::Piper(Default::default()))
        }
        fn set_fallback_synthesis_config(&self, _c: &SynthesisConfig) -> DengjenResult<()> {
            Ok(())
        }
        fn get_speakers(&self) -> DengjenResult<Option<&StdHashMap<i64, String>>> {
            Ok(Some(&self.speakers))
        }
        fn get_language(&self) -> DengjenResult<Option<String>> {
            Ok(Some("en-US".to_string()))
        }
        fn supports_streaming_output(&self) -> bool {
            true
        }
    }

    fn service_with_voice(voice_id: &str, model: FakeModel) -> DengjenGrpcService {
        let service = DengjenGrpcService::new();
        let voice = Voice::new(Arc::new(model)).unwrap();
        service
            .0
            .write()
            .unwrap()
            .insert(voice_id.to_string(), voice);
        service
    }

    #[test]
    fn load_voice_from_config_reports_voice_not_found_for_a_missing_config_path() {
        let service = DengjenGrpcService::new();
        let result = service.load_voice_from_config(PathBuf::from("/does/not/exist.json"));
        match result {
            Err(DengjenGrpcError::VoiceNotFound(msg)) => {
                assert!(msg.contains("/does/not/exist.json"), "message was: {msg}");
            }
            other => panic!("expected VoiceNotFound, got {other:?}"),
        }
    }

    #[test]
    fn open_synthesis_stream_reports_voice_not_found_for_an_unloaded_voice() {
        let service = DengjenGrpcService::new();
        let result = service.open_synthesis_stream("missing", "hi".to_string(), None);
        assert!(matches!(result, Err(DengjenGrpcError::VoiceNotFound(_))));
    }

    #[test]
    fn open_synthesis_stream_succeeds_for_a_loaded_voice() {
        let model = FakeModel {
            speakers: StdHashMap::new(),
        };
        let service = service_with_voice("v1", model);
        let result = service.open_synthesis_stream("v1", "hi".to_string(), None);
        assert!(result.is_ok());
    }

    #[test]
    fn get_voice_info_reports_speakers_audio_and_language() {
        let mut speakers = StdHashMap::new();
        speakers.insert(1i64, "Alice".to_string());
        let model = FakeModel { speakers };
        let service = service_with_voice("v1", model);
        let voices = service.0.read().unwrap();
        let voice = voices.get("v1").unwrap();
        let info = service
            .build_voice_info("v1".to_string(), voice.model_ref())
            .unwrap();
        assert_eq!(info.voice_id, "v1");
        assert_eq!(info.language.as_deref(), Some("en-US"));
        assert_eq!(info.speakers.get(&1), Some(&"Alice".to_string()));
        assert_eq!(info.supports_streaming_output, Some(true));
        let audio = info.audio.unwrap();
        assert_eq!(audio.sample_rate, 22050);
        assert_eq!(audio.num_channels, 1);
        assert_eq!(audio.sample_width, 2);
    }

    #[test]
    fn get_voice_info_defaults_speaker_name_to_default_when_none_configured() {
        let model = FakeModel {
            speakers: StdHashMap::new(),
        };
        let service = service_with_voice("v1", model);
        let voices = service.0.read().unwrap();
        let voice = voices.get("v1").unwrap();
        let info = service
            .build_voice_info("v1".to_string(), voice.model_ref())
            .unwrap();
        assert_eq!(
            info.synth_options.unwrap().speaker.as_deref(),
            Some("Default")
        );
    }
}

#[cfg(test)]
mod synth_options_tests {
    use super::*;
    use dengjen_tts_core::{
        Audio, AudioInfo as CoreAudioInfo, DengjenAudioResult, Phonemes, PiperSynthesisConfig,
    };
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex as StdMutex;

    struct ConfigurableModel {
        speakers: StdHashMap<i64, String>,
        config: StdMutex<PiperSynthesisConfig>,
    }

    impl DengjenModel for ConfigurableModel {
        fn audio_output_info(&self) -> DengjenResult<CoreAudioInfo> {
            Ok(CoreAudioInfo {
                sample_rate: 22050,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<Phonemes> {
            Ok(Phonemes::from(vec!["fake".to_string()]))
        }
        fn speak_batch(&self, _phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> DengjenAudioResult {
            Ok(Audio::new(Default::default(), 22050, None))
        }
        fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::Piper(self.config.lock().unwrap().clone()))
        }
        fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
            Ok(SynthesisConfig::Piper(self.config.lock().unwrap().clone()))
        }
        fn set_fallback_synthesis_config(&self, c: &SynthesisConfig) -> DengjenResult<()> {
            if let SynthesisConfig::Piper(new_config) = c {
                *self.config.lock().unwrap() = new_config.clone();
            }
            Ok(())
        }
        fn get_speakers(&self) -> DengjenResult<Option<&StdHashMap<i64, String>>> {
            Ok(Some(&self.speakers))
        }
    }

    fn service_with_voice(voice_id: &str, model: ConfigurableModel) -> DengjenGrpcService {
        let service = DengjenGrpcService::new();
        let voice = Voice::new(Arc::new(model)).unwrap();
        service
            .0
            .write()
            .unwrap()
            .insert(voice_id.to_string(), voice);
        service
    }

    #[test]
    fn lookup_synth_options_reports_voice_not_found_for_an_unloaded_voice() {
        let service = DengjenGrpcService::new();
        assert!(matches!(
            service.lookup_synth_options("missing"),
            Err(DengjenGrpcError::VoiceNotFound(_))
        ));
    }

    #[test]
    fn apply_synth_options_reports_voice_not_found_for_an_unloaded_voice() {
        let service = DengjenGrpcService::new();
        let opts = grpc::SynthesisOptions {
            speaker: None,
            length_scale: None,
            noise_scale: None,
            noise_w: None,
        };
        assert!(matches!(
            service.apply_synth_options("missing", opts),
            Err(DengjenGrpcError::VoiceNotFound(_))
        ));
    }

    #[test]
    fn lookup_synth_options_reads_the_models_fallback_config_defaulting_speaker_to_id_zero() {
        let mut speakers = StdHashMap::new();
        speakers.insert(0i64, "Narrator".to_string());
        let model = ConfigurableModel {
            speakers,
            config: StdMutex::new(PiperSynthesisConfig {
                speaker: None,
                noise_scale: 0.5,
                length_scale: 1.0,
                noise_w: 0.2,
            }),
        };
        let service = service_with_voice("v1", model);
        let opts = service.lookup_synth_options("v1").unwrap();
        assert_eq!(opts.speaker.as_deref(), Some("Narrator"));
        assert_eq!(opts.noise_scale, Some(0.5));
        assert_eq!(opts.length_scale, Some(1.0));
        assert_eq!(opts.noise_w, Some(0.2));
    }

    #[test]
    fn apply_synth_options_updates_only_the_provided_fields_and_persists_them() {
        let mut speakers = StdHashMap::new();
        speakers.insert(0i64, "Narrator".to_string());
        speakers.insert(7i64, "Robot".to_string());
        let model = ConfigurableModel {
            speakers,
            config: StdMutex::new(PiperSynthesisConfig {
                speaker: Some(0),
                noise_scale: 0.5,
                length_scale: 1.0,
                noise_w: 0.2,
            }),
        };
        let service = service_with_voice("v1", model);

        let update = grpc::SynthesisOptions {
            speaker: Some("Robot".to_string()),
            length_scale: Some(2.0),
            noise_scale: None,
            noise_w: None,
        };
        let result = service.apply_synth_options("v1", update).unwrap();
        assert_eq!(result.speaker.as_deref(), Some("Robot"));
        assert_eq!(result.length_scale, Some(2.0));
        assert_eq!(
            result.noise_scale,
            Some(0.5),
            "unset field must be left unchanged"
        );
        assert_eq!(
            result.noise_w,
            Some(0.2),
            "unset field must be left unchanged"
        );

        let persisted = service.lookup_synth_options("v1").unwrap();
        assert_eq!(
            persisted.speaker.as_deref(),
            Some("Robot"),
            "change must persist on the model"
        );
    }

    #[test]
    fn apply_synth_options_ignores_an_unknown_speaker_name() {
        let mut speakers = StdHashMap::new();
        speakers.insert(0i64, "Narrator".to_string());
        let model = ConfigurableModel {
            speakers,
            config: StdMutex::new(PiperSynthesisConfig {
                speaker: Some(0),
                noise_scale: 0.5,
                length_scale: 1.0,
                noise_w: 0.2,
            }),
        };
        let service = service_with_voice("v1", model);
        let update = grpc::SynthesisOptions {
            speaker: Some("NoSuchSpeaker".to_string()),
            length_scale: None,
            noise_scale: None,
            noise_w: None,
        };
        let result = service.apply_synth_options("v1", update).unwrap();
        assert_eq!(
            result.speaker.as_deref(),
            Some("Narrator"),
            "unknown speaker name must leave the current speaker unchanged"
        );
    }
}

use grpc::dengjen_grpc_server::{DengjenGrpc, DengjenGrpcServer};
use dengjen_core::{CancellationToken, DengjenError, DengjenModel, DengjenResult, SynthesisConfig};
use dengjen_synth::{AudioOutputConfig, DengjenSpeechStreamLazy, DengjenSpeechSynthesizer};
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

    fn _load_dengjen_voice(&self, config_path: PathBuf) -> DengjenGrpcResult<grpc::VoiceInfo> {
        if !config_path.is_file() {
            return Err(DengjenGrpcError::VoiceNotFound(format!(
                "Config file does not exists: `{}`",
                config_path.display()
            )));
        }
        let voice_id = Self::voice_id_for_path(&config_path);
        if let Some(voice) = self.0.read().unwrap().get(&voice_id) {
            return self._get_voice_info(voice_id, voice.model_ref());
        }

        let model = dengjen_piper::from_config_path(&config_path)?;
        log::info!(
            "Loaded Vits voice from: `{}`. Voice ID: {}",
            config_path.display(),
            voice_id
        );
        let voice = Voice::new(model)?;
        let voice_info = self._get_voice_info(voice_id.clone(), voice.model_ref())?;
        self.0.write().unwrap().insert(voice_id, voice);
        Ok(voice_info)
    }

    fn _create_speech_synthesis_stream(
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

    fn _get_voice_info(
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
        let speaker = match config.speaker {
            Some(ref sid) => model.speaker_id_to_name(sid)?,
            None => Some("Default".to_string()),
        };
        Ok(grpc::SynthesisOptions {
            speaker,
            length_scale: Some(config.length_scale),
            noise_scale: Some(config.noise_scale),
            noise_w: Some(config.noise_w),
        })
    }

    fn _get_synth_options_from_model(
        &self,
        model: &(impl DengjenModel + ?Sized),
    ) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let synth_config = match model.get_fallback_synthesis_config()? {
            SynthesisConfig::Piper(config) => config,
            SynthesisConfig::None => {
                return Err(DengjenError::InvalidConfiguration(
                    "Invalid synthesis config for Vits model".to_string(),
                )
                .into())
            }
        };
        let speaker = match synth_config.speaker {
            Some(ref sid) => model.speaker_id_to_name(sid)?,
            None => model.speaker_id_to_name(&0)?,
        };
        Ok(grpc::SynthesisOptions {
            speaker,
            length_scale: Some(synth_config.length_scale),
            noise_scale: Some(synth_config.noise_scale),
            noise_w: Some(synth_config.noise_w),
        })
    }
    fn _get_synth_options(&self, voice_id: &str) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let voices = self.0.read().unwrap();
        let voice = match voices.get(voice_id) {
            Some(voice) => voice,
            None => {
                return Err(DengjenGrpcError::VoiceNotFound(format!(
                    "A voice with the key `{}` has not been loaded",
                    voice_id
                )))
            }
        };
        self._get_synth_options_from_model(voice.model_ref())
    }
    fn _set_synth_options(
        &self,
        voice_id: &str,
        synth_opts: grpc::SynthesisOptions,
    ) -> DengjenGrpcResult<grpc::SynthesisOptions> {
        let voices = self.0.read().unwrap();
        let voice = match voices.get(voice_id) {
            Some(voice) => voice,
            None => {
                return Err(DengjenGrpcError::VoiceNotFound(format!(
                    "A voice with the key `{}` has not been loaded",
                    voice_id
                )))
            }
        };
        let model = voice.model_ref();
        let mut synth_config = match model.get_fallback_synthesis_config()? {
            SynthesisConfig::Piper(config) => config,
            SynthesisConfig::None => {
                return Err(DengjenError::InvalidConfiguration(
                    "Could not set synthesis parameters ".to_string(),
                )
                .into())
            }
        };
        if let Some(sname) = synth_opts.speaker {
            if let Some(sid) = model.speaker_name_to_id(&sname)? {
                synth_config.speaker = Some(sid)
            }
        }
        if let Some(length_scale) = synth_opts.length_scale {
            synth_config.length_scale = length_scale;
        }
        if let Some(noise_scale) = synth_opts.noise_scale {
            synth_config.noise_scale = noise_scale;
        }
        if let Some(noise_w) = synth_opts.noise_w {
            synth_config.noise_w = noise_w;
        }
        model.set_fallback_synthesis_config(&SynthesisConfig::Piper(synth_config))?;
        self._get_synth_options_from_model(model)
    }
}

#[tonic::async_trait]
impl DengjenGrpc for DengjenGrpcService {
    async fn get_dengjen_version(
        &self,
        _request: Request<grpc::Empty>,
    ) -> Result<Response<grpc::Version>, Status> {
        let version = grpc::Version {
            version: env!("CARGO_PKG_VERSION").into(),
        };
        return Ok(Response::new(version));
    }
    async fn load_voice(
        &self,
        _request: Request<grpc::VoicePath>,
    ) -> Result<Response<grpc::VoiceInfo>, Status> {
        let voice_path = _request.into_inner();
        let config_path = PathBuf::from(voice_path.config_path);
        let voice_info = self._load_dengjen_voice(config_path)?;
        Ok(Response::new(voice_info))
    }
    async fn get_voice_info(
        &self,
        _request: Request<grpc::VoiceIdentifier>,
    ) -> Result<Response<grpc::VoiceInfo>, Status> {
        let voice_id = _request.into_inner().voice_id;
        let voices = self.0.read().unwrap();
        let voice = match voices.get(&voice_id) {
            Some(voice) => voice,
            None => {
                return Err(DengjenGrpcError::VoiceNotFound(format!(
                    "A voice with the key `{}` has not been loaded",
                    voice_id
                )))?
            }
        };
        let voice_info = self._get_voice_info(voice_id, voice.model_ref())?;
        Ok(Response::new(voice_info))
    }
    async fn get_synthesis_options(
        &self,
        _request: Request<grpc::VoiceIdentifier>,
    ) -> Result<Response<grpc::SynthesisOptions>, Status> {
        let voice_id = _request.into_inner().voice_id;
        let synth_opts = self._get_synth_options(&voice_id)?;
        Ok(Response::new(synth_opts))
    }
    async fn set_synthesis_options(
        &self,
        _request: Request<grpc::VoiceSynthesisOptions>,
    ) -> Result<Response<grpc::SynthesisOptions>, Status> {
        let req = _request.into_inner();
        let synth_opts = match req.synthesis_options {
            Some(opts) => opts,
            None => {
                let status = Status::invalid_argument("No synthesis options provided");
                return Err(status);
            }
        };
        let new_synth_opts = self._set_synth_options(&req.voice_id, synth_opts)?;
        let response = Response::new(new_synth_opts);
        Ok(response)
    }
    type SynthesizeUtteranceStream = ReceiverStream<Result<grpc::SynthesisResult, Status>>;
    async fn synthesize_utterance(
        &self,
        _request: Request<grpc::Utterance>,
    ) -> Result<Response<Self::SynthesizeUtteranceStream>, Status> {
        let req = _request.into_inner();
        let output_config = req.speech_args.map(|args| AudioOutputConfig {
            rate: args.rate.map(|i| i as u8),
            volume: args.volume.map(|i| i as u8),
            pitch: args.pitch.map(|i| i as u8),
            appended_silence_ms: args.appended_silence_ms,
        });
        let dengjen_stream =
            self._create_speech_synthesis_stream(&req.voice_id, req.text, output_config)?;
        let (tx, rx) = mpsc::channel(512);
        tokio::task::spawn_blocking(move || {
            for wav_result in dengjen_stream {
                let wav = match wav_result {
                    Ok(wav) => wav,
                    Err(e) => {
                        let err = Err(DengjenGrpcError::from(e).into());
                        tx.blocking_send(err).ok();
                        return;
                    }
                };
                let synth_result = grpc::SynthesisResult {
                    wav_samples: wav.as_wave_bytes(),
                    rtf: wav.real_time_factor().unwrap_or_default(),
                };
                if tx.blocking_send(Ok(synth_result)).is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
    type SynthesizeUtteranceRealtimeStream = ReceiverStream<Result<grpc::WaveSamples, Status>>;
    async fn synthesize_utterance_realtime(
        &self,
        _request: Request<grpc::Utterance>,
    ) -> Result<Response<Self::SynthesizeUtteranceRealtimeStream>, Status> {
        let req = _request.into_inner();
        let output_config = req.speech_args.map(|args| AudioOutputConfig {
            rate: args.rate.map(|i| i as u8),
            volume: args.volume.map(|i| i as u8),
            pitch: args.pitch.map(|i| i as u8),
            appended_silence_ms: args.appended_silence_ms,
        });
        let voice_id = &req.voice_id;
        let voices = self.0.read().unwrap();
        let voice = match voices.get(voice_id) {
            Some(voice) => voice,
            None => {
                return Err(DengjenGrpcError::VoiceNotFound(format!(
                    "A voice with the key `{}` has not been loaded",
                    voice_id
                ))
                .into())
            }
        };
        let synth = Arc::clone(&voice.0);
        let (tx, rx) = mpsc::channel(512);
        tokio::task::spawn_blocking(move || {
            let stream_result =
                synth.synthesize_streamed(req.text, output_config, 55, 3, CancellationToken::new());
            let realtime_speech_stream = match stream_result {
                Ok(stream) => stream,
                Err(e) => {
                    let err = Err(DengjenGrpcError::from(e).into());
                    tx.blocking_send(err).ok();
                    return;
                }
            };
            for wav_result in realtime_speech_stream {
                let wav = match wav_result {
                    Ok(wav) => wav,
                    Err(e) => {
                        let err = Err(DengjenGrpcError::from(e).into());
                        tx.blocking_send(err).ok();
                        return;
                    }
                };
                let synth_result = grpc::WaveSamples {
                    wav_samples: wav.as_wave_bytes(),
                };
                if tx.blocking_send(Ok(synth_result)).is_err() {
                    return;
                }
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

fn setup_logging() {
    env_logger::Builder::from_env(env_logger::Env::default().filter_or("DENGJEN_GRPC", "info"))
        .init();
}

fn init_ort_environment() -> bool {
    ort::init()
        .with_name("dengjen")
        .with_execution_providers([
            ort::execution_providers::CPU::default().build()
        ])
        .commit()
}


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    setup_logging();

    if !init_ort_environment()  {
        log::error!("Could not initialize onnxruntime environment");
    }

    let port = std::env::var("DENGJEN_GRPC_SERVER_PORT")
        .map(|val| val.parse().unwrap_or(DEFAULT_DENGJEN_GRPC_SERVER_PORT))
        .unwrap_or(DEFAULT_DENGJEN_GRPC_SERVER_PORT);
    let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), port);

    let service = DengjenGrpcService::new();
    let server = DengjenGrpcServer::new(service);

    log::info!("Starting Dengjen GRPC server at address: {}", addr);

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
        assert_eq!(err.to_string(), DengjenError::OperationError("boom".to_string()).to_string());
    }

    #[test]
    fn voice_not_found_maps_to_status_not_found() {
        let status: Status = DengjenGrpcError::VoiceNotFound("x".to_string()).into();
        assert_eq!(status.code(), tonic::Code::NotFound);
        assert_eq!(status.message(), "x");
    }

    #[test]
    fn failed_to_load_resource_maps_to_status_aborted() {
        let status: Status = DengjenGrpcError::from(DengjenError::FailedToLoadResource("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Aborted);
    }

    #[test]
    fn phonemization_error_maps_to_status_aborted() {
        let status: Status = DengjenGrpcError::from(DengjenError::PhonemizationError("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Aborted);
    }

    #[test]
    fn inference_error_maps_to_status_internal() {
        let status: Status = DengjenGrpcError::from(DengjenError::InferenceError("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Internal);
    }

    #[test]
    fn invalid_configuration_maps_to_status_invalid_argument() {
        let status: Status = DengjenGrpcError::from(DengjenError::InvalidConfiguration("x".into())).into();
        assert_eq!(status.code(), tonic::Code::InvalidArgument);
    }

    #[test]
    fn unsupported_operation_maps_to_status_unimplemented() {
        let status: Status = DengjenGrpcError::from(DengjenError::UnsupportedOperation("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Unimplemented);
    }

    #[test]
    fn operation_error_maps_to_status_unknown() {
        let status: Status = DengjenGrpcError::from(DengjenError::OperationError("x".into())).into();
        assert_eq!(status.code(), tonic::Code::Unknown);
    }
}

#[cfg(test)]
mod voice_loading_tests {
    use super::*;
    use dengjen_core::{Audio, AudioInfo as CoreAudioInfo, DengjenAudioResult, Phonemes};
    use std::collections::HashMap as StdHashMap;

    struct FakeModel {
        speakers: StdHashMap<i64, String>,
    }

    impl DengjenModel for FakeModel {
        fn audio_output_info(&self) -> DengjenResult<CoreAudioInfo> {
            Ok(CoreAudioInfo { sample_rate: 22050, num_channels: 1, sample_width: 2 })
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
        service.0.write().unwrap().insert(voice_id.to_string(), voice);
        service
    }

    #[test]
    fn load_dengjen_voice_reports_voice_not_found_for_a_missing_config_path() {
        let service = DengjenGrpcService::new();
        let result = service._load_dengjen_voice(PathBuf::from("/does/not/exist.json"));
        match result {
            Err(DengjenGrpcError::VoiceNotFound(msg)) => {
                assert!(msg.contains("/does/not/exist.json"), "message was: {msg}");
            }
            other => panic!("expected VoiceNotFound, got {other:?}"),
        }
    }

    #[test]
    fn create_speech_synthesis_stream_reports_voice_not_found_for_an_unloaded_voice() {
        let service = DengjenGrpcService::new();
        let result = service._create_speech_synthesis_stream("missing", "hi".to_string(), None);
        assert!(matches!(result, Err(DengjenGrpcError::VoiceNotFound(_))));
    }

    #[test]
    fn create_speech_synthesis_stream_succeeds_for_a_loaded_voice() {
        let model = FakeModel { speakers: StdHashMap::new() };
        let service = service_with_voice("v1", model);
        let result = service._create_speech_synthesis_stream("v1", "hi".to_string(), None);
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
        let info = service._get_voice_info("v1".to_string(), voice.model_ref()).unwrap();
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
        let model = FakeModel { speakers: StdHashMap::new() };
        let service = service_with_voice("v1", model);
        let voices = service.0.read().unwrap();
        let voice = voices.get("v1").unwrap();
        let info = service._get_voice_info("v1".to_string(), voice.model_ref()).unwrap();
        assert_eq!(info.synth_options.unwrap().speaker.as_deref(), Some("Default"));
    }
}


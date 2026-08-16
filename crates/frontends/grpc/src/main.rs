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

struct Voice(Arc<DengjenSpeechSynthesizer>);

impl Voice {
    fn new(model: Arc<dyn DengjenModel + Send + Sync>) -> DengjenResult<Self> {
        let synth = Arc::new(DengjenSpeechSynthesizer::new(model)?);
        Ok(Self(synth))
    }
    fn model_ref(&self) -> &dyn DengjenModel {
        self.synth_ref()
    }
    fn synth_ref(&self) -> &DengjenSpeechSynthesizer {
        self.0.as_ref()
    }
}

struct DengjenGrpcService(RwLock<HashMap<String, Voice>>);

impl DengjenGrpcService {
    fn new() -> Self {
        Self(Default::default())
    }
    fn _load_dengjen_voice(&self, config_path: PathBuf) -> DengjenGrpcResult<grpc::VoiceInfo> {
        let voice_id = if config_path.is_file() {
            let voice_path = config_path
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .into_owned();
            (xxh3_64(voice_path.as_bytes()) / VOICE_ID_REDUCTION_FACTOR).to_string()
        } else {
            return Err(DengjenGrpcError::VoiceNotFound(format!(
                "Config file does not exists: `{}`",
                config_path.display()
            )));
        };
        if let Some(voice) = (self.0.read().unwrap()).get(&voice_id) {
            return self._get_voice_info(voice_id, voice.model_ref());
        }
        let piper_model = dengjen_piper::from_config_path(&config_path)?;
        log::info!(
            "Loaded Vits voice from: `{}`. Voice ID: {}",
            config_path.display(),
            voice_id
        );
        let voice = Voice::new(piper_model)?;
        let voice_info = self._get_voice_info(voice_id.clone(), voice.model_ref())?;
        (self.0.write().unwrap()).insert(voice_id, voice);
        Ok(voice_info)
    }
    fn _create_speech_synthesis_stream(
        &self,
        voice_id: &str,
        text: String,
        output_config: Option<AudioOutputConfig>,
    ) -> DengjenGrpcResult<DengjenSpeechStreamLazy> {
        match (self.0.read().unwrap()).get(voice_id) {
            Some(voice) => Ok(voice.synth_ref().synthesize_lazy(text, output_config)?),
            None => Err(DengjenGrpcError::VoiceNotFound(format!(
                "A voice with the key `{}` has not been loaded",
                voice_id
            ))),
        }
    }
    fn _get_voice_info(
        &self,
        voice_id: String,
        model: &(impl DengjenModel + ?Sized),
    ) -> DengjenGrpcResult<grpc::VoiceInfo> {
        let wav_info = model.audio_output_info()?;
        let speakers = model.get_speakers()?;
        let language = model.get_language()?;
        let audio_info = grpc::AudioInfo {
            sample_rate: wav_info.sample_rate as u32,
            num_channels: wav_info.num_channels as u32,
            sample_width: wav_info.sample_width as u32,
        };
        let synth_options = {
            let default_synth_config = match model.get_default_synthesis_config()? {
                SynthesisConfig::Piper(config) => config,
                SynthesisConfig::None => {
                    return Err(DengjenError::InvalidConfiguration(
                        "Invalid synthesis config for Vits model".to_string(),
                    )
                    .into())
                }
            };
            let speaker = match default_synth_config.speaker {
                Some(ref sid) => model.speaker_id_to_name(sid)?,
                None => Some("Default".to_string()),
            };
            grpc::SynthesisOptions {
                speaker,
                length_scale: Some(default_synth_config.length_scale),
                noise_scale: Some(default_synth_config.noise_scale),
                noise_w: Some(default_synth_config.noise_w),
            }
        };
        Ok(grpc::VoiceInfo {
            voice_id,
            synth_options: Some(synth_options),
            language,
            speakers: speakers.cloned().unwrap_or_default(),
            audio: Some(audio_info),
            supports_streaming_output: Some(model.supports_streaming_output()),
            quality: None,
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


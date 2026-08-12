use crate::config::ModelConfig;
use crate::inference::{create_inference_session, reversed_mapping};
use crate::phonemize::{create_tashkeel_engine, TashkeelEngine};
use crate::VitsModelCommons;
use dengjen_core::{
    Audio, AudioInfo, AudioSamples, AudioStreamIterator, CancellationToken, DengjenAudioResult,
    DengjenError, DengjenModel, DengjenResult, Phonemes, PiperSynthesisConfig, SynthesisConfig,
};
use ndarray::{Array, Array1, Array2, ArrayView, Axis, Dim, IxDynImpl};
use ort::session::{Session, SessionOutputs};
use ort::value::{Tensor, TensorRef};
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};

const MIN_CHUNK_SIZE: isize = 44;
const MAX_CHUNK_SIZE: usize = 1024;

pub struct VitsStreamingModel {
    synth_config: RwLock<PiperSynthesisConfig>,
    config: ModelConfig,
    speaker_map: HashMap<i64, String>,
    encoder_model: Mutex<Session>,
    decoder_model: Arc<Mutex<Session>>,
    #[cfg_attr(not(all(feature = "tashkeel", feature = "espeak")), allow(dead_code))]
    tashkeel_engine: Option<TashkeelEngine>,
}

impl VitsStreamingModel {
    pub(crate) fn from_config(
        config: ModelConfig,
        synth_config: PiperSynthesisConfig,
        encoder_path: &Path,
        decoder_path: &Path,
    ) -> DengjenResult<Self> {
        let encoder_model = match create_inference_session(encoder_path) {
            Ok(model) => model,
            Err(err) => {
                return Err(DengjenError::InferenceError(format!(
                    "Failed to initialize onnxruntime inference session: `{}`",
                    err
                )))
            }
        };
        let decoder_model = match create_inference_session(decoder_path) {
            Ok(model) => Arc::new(Mutex::new(model)),
            Err(err) => {
                return Err(DengjenError::InferenceError(format!(
                    "Failed to initialize onnxruntime inference session: `{}`",
                    err
                )))
            }
        };
        let speaker_map = reversed_mapping(&config.speaker_id_map);
        let tashkeel_engine = create_tashkeel_engine(&config)?;
        Ok(Self {
            synth_config: RwLock::new(synth_config),
            config,
            speaker_map,
            encoder_model: Mutex::new(encoder_model),
            decoder_model,
            tashkeel_engine,
        })
    }

    fn infer_with_values(&self, input_phonemes: Vec<i64>) -> DengjenAudioResult {
        let timer = std::time::Instant::now();
        let encoder_output = self.infer_encoder(input_phonemes)?;
        let audio = encoder_output.infer_decoder(self.decoder_model.as_ref())?;
        let inference_ms = timer.elapsed().as_millis() as f32;
        Ok(Audio::new(
            audio,
            self.config.audio.sample_rate as usize,
            Some(inference_ms),
        ))
    }
    fn infer_encoder(&self, input_phonemes: Vec<i64>) -> DengjenResult<EncoderOutputs> {
        let synth_config = self.synth_config.read().unwrap();

        let input_len = input_phonemes.len();
        let phoneme_inputs = Array2::<i64>::from_shape_vec((1, input_len), input_phonemes).unwrap();
        let input_lengths = Array1::<i64>::from_iter([input_len as i64]);

        let scales = Array1::<f32>::from_iter([
            synth_config.noise_scale,
            synth_config.length_scale,
            synth_config.noise_w,
        ]);

        let speaker_id = if self.config.num_speakers > 1 {
            let sid = synth_config.speaker.unwrap_or(0);
            Some(Array1::<i64>::from_iter([sid]))
        } else {
            None
        };

        let mut session = self.encoder_model.lock().unwrap();
        {
            let outputs = if let Some(sid_tensor) = speaker_id.clone() {
                let inputs = ort::inputs![
                    Tensor::from_array(phoneme_inputs).unwrap(),
                    Tensor::from_array(input_lengths).unwrap(),
                    Tensor::from_array(scales).unwrap(),
                    Tensor::from_array(sid_tensor).unwrap(),
                ];
                session.run(inputs)
            } else {
                let inputs = ort::inputs![
                    Tensor::from_array(phoneme_inputs).unwrap(),
                    Tensor::from_array(input_lengths).unwrap(),
                    Tensor::from_array(scales).unwrap(),
                ];
                session.run(inputs)
            };
            match outputs {
                Ok(ort_values) => EncoderOutputs::from_values(ort_values),
                Err(e) => Err(DengjenError::InferenceError(format!(
                    "Failed to run model inference. Error: {}",
                    e
                ))),
            }
        }
    }
}

impl VitsModelCommons for VitsStreamingModel {
    fn get_synth_config(&self) -> &RwLock<PiperSynthesisConfig> {
        &self.synth_config
    }
    fn get_config(&self) -> &ModelConfig {
        &self.config
    }
    fn get_speaker_map(&self) -> &HashMap<i64, String> {
        &self.speaker_map
    }
    fn get_tashkeel_engine(&self) -> Option<&TashkeelEngine> {
        self.tashkeel_engine.as_ref()
    }
}

impl DengjenModel for VitsStreamingModel {
    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        self.do_phonemize_text(text)
    }

    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phoneme_batches = Vec::from_iter(
            phoneme_batches
                .into_iter()
                .map(|phonemes| self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id)),
        );
        let mut retval = Vec::new();
        for phonemes in phoneme_batches.into_iter() {
            retval.push(self.infer_with_values(phonemes)?);
        }
        Ok(retval)
    }
    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phonemes = self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id);
        self.infer_with_values(phonemes)
    }
    fn get_default_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        Ok(SynthesisConfig::Piper(self.factory_synthesis_config()))
    }
    fn get_fallback_synthesis_config(&self) -> DengjenResult<SynthesisConfig> {
        Ok(SynthesisConfig::Piper(self.synth_config.read().unwrap().clone()))
    }
    fn set_fallback_synthesis_config(&self, synthesis_config: &SynthesisConfig) -> DengjenResult<()> {
        match synthesis_config {
            SynthesisConfig::Piper(new_config) => self._do_set_default_synth_config(new_config),
            SynthesisConfig::None => Err(DengjenError::InvalidConfiguration(
                "Piper models require a PiperSynthesisConfig".to_string(),
            )),
        }
    }
    fn get_language(&self) -> DengjenResult<Option<String>> {
        Ok(self.language())
    }
    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(Some(self.get_speaker_map()))
    }
    fn speaker_name_to_id(&self, name: &str) -> DengjenResult<Option<i64>> {
        Ok(self.config.speaker_id_map.get(name).copied())
    }
    fn properties(&self) -> DengjenResult<HashMap<String, String>> {
        Ok(self.get_properties())
    }
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        self.get_audio_output_info()
    }
    fn supports_streaming_output(&self) -> bool {
        true
    }
    fn stream_synthesis(
        &self,
        phonemes: String,
        chunk_size: usize,
        chunk_padding: usize,
        cancel_token: CancellationToken,
    ) -> DengjenResult<AudioStreamIterator<'_>> {
        // Skip the encoder pass entirely rather than erroring: a cancellation must stay silent.
        if cancel_token.is_cancelled() {
            return Ok(Box::new(std::iter::empty()));
        }
        let (pad_id, bos_id, eos_id) = self.get_meta_ids();
        let phonemes = self.phonemes_to_input_ids(&phonemes, pad_id, bos_id, eos_id);
        let encoder_outputs = self.infer_encoder(phonemes)?;
        let streamer = Box::new(SpeechStreamer::new(
            Arc::clone(&self.decoder_model),
            encoder_outputs,
            chunk_size,
            chunk_padding,
            self.config.hop_length.unwrap_or(256),
            cancel_token,
        ));
        Ok(streamer)
    }
}

struct EncoderOutputs {
    z: Array<f32, Dim<IxDynImpl>>,
    y_mask: Array<f32, Dim<IxDynImpl>>,
    #[allow(dead_code)]
    p_duration: Option<Array<f32, Dim<IxDynImpl>>>,
    g: Array<f32, Dim<IxDynImpl>>,
}

impl EncoderOutputs {
    #[inline(always)]
    fn from_values(values: SessionOutputs) -> DengjenResult<Self> {
        let z = {
            let (shape, data) = match values["z"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::InferenceError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            Array::from_shape_vec(shape.to_ixdyn(), data.to_vec()).unwrap()
        };
        let y_mask = {
            let (shape, data) = match values["y_mask"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::InferenceError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            Array::from_shape_vec(shape.to_ixdyn(), data.to_vec()).unwrap()
        };
        let p_duration = if values.contains_key("p_duration") {
            let (shape, data) = match values["p_duration"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::InferenceError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            Some(Array::from_shape_vec(shape.to_ixdyn(), data.to_vec()).unwrap())
        } else {
            None
        };
        let g = if values.contains_key("g") {
            let (shape, data) = match values["g"].try_extract_tensor::<f32>() {
                Ok(out) => out,
                Err(e) => {
                    return Err(DengjenError::InferenceError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    )))
                }
            };
            Array::from_shape_vec(shape.to_ixdyn(), data.to_vec()).unwrap()
        } else {
            Array1::<f32>::from_iter([]).into_dyn()
        };
        Ok(Self { z, y_mask, p_duration, g })
    }
    fn infer_decoder(&self, session: &Mutex<Session>) -> DengjenResult<AudioSamples> {
        let mut session = session.lock().unwrap();
        let session_outputs = if self.g.is_empty() {
            let inputs = ort::inputs![
                TensorRef::from_array_view(self.z.view()).unwrap(),
                TensorRef::from_array_view(self.y_mask.view()).unwrap(),
            ];
            session.run(inputs)
        } else {
            let inputs = ort::inputs![
                TensorRef::from_array_view(self.z.view()).unwrap(),
                TensorRef::from_array_view(self.y_mask.view()).unwrap(),
                TensorRef::from_array_view(self.g.view()).unwrap(),
            ];
            session.run(inputs)
        };
        let outputs = match session_outputs {
            Ok(out) => out,
            Err(e) => {
                return Err(DengjenError::InferenceError(format!(
                    "Failed to run model inference. Error: {}",
                    e
                )))
            }
        };
        match outputs[0].try_extract_tensor::<f32>() {
            Ok((_, out)) => Ok(Vec::from(out).into()),
            Err(e) => Err(DengjenError::InferenceError(format!(
                "Failed to run model inference. Error: {}",
                e
            ))),
        }
    }
}

struct SpeechStreamer {
    decoder_model: Arc<Mutex<Session>>,
    encoder_outputs: EncoderOutputs,
    mel_chunker: AdaptiveMelChunker,
    one_shot: bool,
    cancel_token: CancellationToken,
}

impl SpeechStreamer {
    fn new(
        decoder_model: Arc<Mutex<Session>>,
        encoder_outputs: EncoderOutputs,
        chunk_size: usize,
        chunk_padding: usize,
        hop_length: usize,
        cancel_token: CancellationToken,
    ) -> Self {
        let num_frames = encoder_outputs.z.shape()[2];
        let mel_chunker = AdaptiveMelChunker::new(
            num_frames as isize,
            chunk_size as isize,
            chunk_padding as isize,
            hop_length as isize,
        );
        let one_shot = num_frames <= (chunk_size * 2 + (chunk_padding * 2));
        Self {
            decoder_model,
            encoder_outputs,
            mel_chunker,
            one_shot,
            cancel_token,
        }
    }
    fn synthesize_chunk(
        &mut self,
        mel_index: ndarray::Slice,
        audio_index: ndarray::Slice,
    ) -> DengjenResult<AudioSamples> {
        // println!("Mel index: {:?}\nAudio Index: {:?}", mel_index, audio_index);
        let audio = {
            let session: Arc<Mutex<Session>> = Arc::clone(&self.decoder_model);
            let mut session = session.lock().unwrap();
            let z_view = self.encoder_outputs.z.view();
            let y_mask_view = self.encoder_outputs.y_mask.view();
            let z_chunk = z_view.slice_axis(Axis(2), mel_index);
            let y_mask_chunk = y_mask_view.slice_axis(Axis(2), mel_index);
            let outputs = if self.encoder_outputs.g.is_empty() {
                let inputs = ort::inputs![
                    TensorRef::from_array_view(z_chunk).unwrap(),
                    TensorRef::from_array_view(y_mask_chunk).unwrap(),
                ];
                session.run(inputs)
            } else {
                let inputs = ort::inputs![
                    TensorRef::from_array_view(z_chunk).unwrap(),
                    TensorRef::from_array_view(y_mask_chunk).unwrap(),
                    TensorRef::from_array_view(self.encoder_outputs.g.view()).unwrap(),
                ];
                session.run(inputs)
            };
            let outputs = outputs
                .map_err(|e| {
                    DengjenError::InferenceError(format!(
                        "Failed to run model inference. Error: {}",
                        e
                    ))
                })?;
            let (shape, data) = outputs[0].try_extract_tensor::<f32>().map_err(|e| {
                DengjenError::InferenceError(format!("Failed to run model inference. Error: {}", e))
            })?;
            let audio_view = ArrayView::from_shape(shape.to_ixdyn(), data)
                .map_err(|e| DengjenError::with_message(format!("Invalid model audio output shape: {}", e)))?;
            self.process_chunk_audio(audio_view, audio_index)?
        };
        Ok(audio)
    }
    #[inline(always)]
    fn process_chunk_audio(
        &mut self,
        audio_view: ArrayView<f32, Dim<IxDynImpl>>,
        audio_index: ndarray::Slice,
    ) -> DengjenResult<AudioSamples> {
        let mut audio: AudioSamples = audio_view
            .slice_axis(Axis(2), audio_index)
            .as_slice()
            .ok_or_else(|| DengjenError::with_message("Invalid model audio output"))?
            .to_vec()
            .into();
        audio.crossfade(42);
        Ok(audio)
    }
}

impl Iterator for SpeechStreamer {
    type Item = DengjenResult<AudioSamples>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.cancel_token.is_cancelled() {
            return None;
        }
        let (mel_index, audio_index) = self.mel_chunker.next()?;
        if self.one_shot {
            self.mel_chunker.consume();
            Some(
                self.encoder_outputs
                    .infer_decoder(self.decoder_model.as_ref()),
            )
        } else {
            Some(self.synthesize_chunk(mel_index, audio_index))
        }
    }
}

struct AdaptiveMelChunker {
    num_frames: isize,
    chunk_size: usize,
    chunk_padding: isize,
    hop_length: isize,
    last_end_index: Option<isize>,
    step: usize
}

impl AdaptiveMelChunker {
    fn new(num_frames: isize, chunk_size: isize, chunk_padding: isize, hop_length: isize) -> Self {
        Self {
            num_frames,
            chunk_size: chunk_size as usize,
            chunk_padding,
            hop_length,
            last_end_index: Some(0),
            step: 1
        }
    }
    fn consume(&mut self) {
        self.last_end_index = None;
    }
}

impl Iterator for AdaptiveMelChunker {
    type Item = (ndarray::Slice, ndarray::Slice);

    fn next(&mut self) -> Option<Self::Item> {
        let last_index = self.last_end_index?;
        let chunk_size = (self.chunk_size * self.step).min(MAX_CHUNK_SIZE);
        let (start_index, end_index): (isize, Option<isize>);
        let (start_padding, end_padding): (isize, Option<isize>);
        if last_index == 0 {
            start_index = 0;
            start_padding = 0;
        } else {
            start_index = last_index - (self.chunk_padding * 2);
            start_padding = self.chunk_padding;
        }
        let chunk_end = last_index + chunk_size as isize + self.chunk_padding;
        let remaining_frames = self.num_frames - chunk_end;
        if remaining_frames <= MIN_CHUNK_SIZE {
            end_index = None;
            end_padding = None;
        } else {
            end_index = Some(chunk_end);
            end_padding = Some(-self.chunk_padding)
        }
        self.step += 1;
        self.last_end_index = end_index;
        let chunk_index = ndarray::Slice::new(start_index, end_index, 1);
        let audio_index = ndarray::Slice::new(
            start_padding * self.hop_length,
            end_padding.map(|i| i * self.hop_length),
            1,
        );
        Some((chunk_index, audio_index))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adaptive_mel_chunker_scales_audio_index_by_default_hop_length() {
        let mut chunker = AdaptiveMelChunker::new(5000, 100, 10, 256);
        let _first = chunker.next().unwrap();
        let (_, audio_index) = chunker.next().unwrap();
        assert_eq!(audio_index.start, 10 * 256);
        assert_eq!(audio_index.end, Some(-10 * 256));
    }

    #[test]
    fn adaptive_mel_chunker_scales_audio_index_by_custom_hop_length() {
        let mut chunker = AdaptiveMelChunker::new(5000, 100, 10, 100);
        let _first = chunker.next().unwrap();
        let (_, audio_index) = chunker.next().unwrap();
        assert_eq!(audio_index.start, 10 * 100);
        assert_eq!(audio_index.end, Some(-10 * 100));
    }

    #[test]
    fn adaptive_mel_chunker_terminates_when_remaining_frames_fall_below_minimum() {
        // num_frames=50, chunk_size=10, chunk_padding=5, hop_length=10:
        // chunk_end = 0 + 10 + 5 = 15; remaining = 50 - 15 = 35 <= MIN_CHUNK_SIZE (44),
        // so this chunk is terminal (end_index/end_padding = None) and the next
        // call must return None (iterator exhausted).
        let mut chunker = AdaptiveMelChunker::new(50, 10, 5, 10);
        let (mel_index, audio_index) = chunker.next().unwrap();
        assert_eq!(mel_index.end, None);
        assert_eq!(audio_index.end, None);
        assert!(chunker.next().is_none());
    }
}

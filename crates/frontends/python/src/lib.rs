// PyO3's #[pymethods] expands each impl into a hidden wrapper item, which
// trips rustc's non_local_definitions lint as a false positive; an #[allow]
// on the impl itself doesn't survive the macro expansion, so it's silenced
// crate-wide here instead.
#![allow(non_local_definitions)]

use dengjen_core::{DengjenError, DengjenModel, Audio, AudioInfo};
use dengjen_synth::{
    AudioOutputConfig, DengjenSpeechStreamLazy, DengjenSpeechStreamParallel,
    DengjenSpeechSynthesizer, RealtimeSpeechStream
};
use dengjen_piper::PiperSynthesisConfig;
use libtashkeel_core::{LibtashkeelResult, DynamicInferenceEngine as TashkeelInferenceEngine, do_tashkeel};
use once_cell::sync::Lazy;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

static LIBTASHKEEL_ENGINE: Lazy<LibtashkeelResult<TashkeelInferenceEngine>>=
    Lazy::new(|| libtashkeel_core::create_inference_engine(None));
type PyDengjenResult<T> = Result<T, PyDengjenError>;

create_exception!(
    piper,
    DengjenException,
    PyException,
    "Base Exception for all exceptions raised by piper."
);


struct PyDengjenError(DengjenError);

impl From<PyDengjenError> for PyErr {
    fn from(other: PyDengjenError) -> Self {
        DengjenException::new_err(other.0.to_string())
    }
}

impl From<DengjenError> for PyDengjenError {
    fn from(other: DengjenError) -> Self {
        Self(other)
    }
}

#[pyclass(weakref, module = "piper", frozen)]
#[pyo3(name = "AudioInfo")]
struct PyWaveInfo(AudioInfo);

#[pymethods]
impl PyWaveInfo {
    #[getter]
    fn get_sample_rate(&self) -> usize {
        self.0.sample_rate
    }
    #[getter]
    fn get_num_channels(&self) -> usize {
        self.0.num_channels
    }
    #[getter]
    fn get_sample_width(&self) -> usize {
        self.0.sample_width
    }
}

impl From<AudioInfo> for PyWaveInfo {
    fn from(other: AudioInfo) -> Self {
        Self(other)
    }
}

#[pyclass(weakref, module = "piper", frozen)]
#[pyo3(name = "AudioOutputConfig")]
#[derive(Clone)]
struct PyAudioOutputConfig(AudioOutputConfig);

#[pymethods]
impl PyAudioOutputConfig {
    #[new]
    fn new(
        rate: Option<u8>,
        volume: Option<u8>,
        pitch: Option<u8>,
        appended_silence_ms: Option<u32>,
    ) -> Self {
        Self(AudioOutputConfig {
            rate,
            volume,
            pitch,
            appended_silence_ms,
        })
    }
}

impl From<PyAudioOutputConfig> for AudioOutputConfig {
    fn from(other: PyAudioOutputConfig) -> Self {
        other.0
    }
}

#[pyclass(weakref, module = "piper", frozen)]
struct WaveSamples(Audio);

#[pymethods]
impl WaveSamples {
    fn get_wave_bytes(&self, py: Python) -> PyObject {
        let bytes_vec = py.allow_threads(move || self.0.as_wave_bytes());
        PyBytes::new(py, &bytes_vec).into()
    }
    fn save_to_file(&self, filename: &str) -> PyDengjenResult<()> {
        Ok(self.0.save_to_file(&PathBuf::from(filename)).map_err(DengjenError::from)?)
    }
    #[getter]
    fn sample_rate(&self) -> usize {
        self.0.info.sample_rate
    }
    #[getter]
    fn num_channels(&self) -> usize {
        self.0.info.num_channels
    }
    #[getter]
    fn sample_width(&self) -> usize {
        self.0.info.sample_width
    }
    #[getter]
    fn inference_ms(&self) -> Option<f32> {
        self.0.inference_ms()
    }
    #[getter]
    fn duration_ms(&self) -> f32 {
        self.0.duration_ms()
    }
    #[getter]
    fn real_time_factor(&self) -> Option<f32> {
        self.0.real_time_factor()
    }
}

#[pyclass(weakref, module = "piper")]
struct LazySpeechStream(DengjenSpeechStreamLazy);

impl From<DengjenSpeechStreamLazy> for LazySpeechStream {
    fn from(other: DengjenSpeechStreamLazy) -> Self {
        Self(other)
    }
}

#[pymethods]
impl LazySpeechStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python) -> Option<WaveSamples> {
        let next_item = py.allow_threads(|| self.0.next());
        let audio_result = next_item?;
        match audio_result {
            Ok(audio_data) => Some(WaveSamples(audio_data)),
            Err(e) => {
                PyErr::from(PyDengjenError::from(e)).restore(py);
                None
            }
        }
    }
}

#[pyclass(weakref, module = "piper")]
struct ParallelSpeechStream(DengjenSpeechStreamParallel);

impl From<DengjenSpeechStreamParallel> for ParallelSpeechStream {
    fn from(other: DengjenSpeechStreamParallel) -> Self {
        Self(other)
    }
}

#[pymethods]
impl ParallelSpeechStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python) -> Option<WaveSamples> {
        let next_item = py.allow_threads(|| self.0.next());
        let audio_result = next_item?;
        match audio_result {
            Ok(audio_data) => Some(WaveSamples(audio_data)),
            Err(e) => {
                PyErr::from(PyDengjenError::from(e)).restore(py);
                None
            }
        }
    }
}

#[pyclass(weakref, module = "piper")]
struct PyRealtimeSpeechStream(RealtimeSpeechStream);

#[pymethods]
impl PyRealtimeSpeechStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python) -> Option<PyObject> {
        let result = py.allow_threads(|| self.0.next())?;
        match result {
            Ok(samples) => Some(PyBytes::new(py, &samples.as_wave_bytes()).into()),
            Err(e) => {
                PyErr::from(PyDengjenError::from(e)).restore(py);
                None
            }
        }
    }
}

#[pyclass(weakref, module = "piper")]
struct PiperScales {
    #[allow(dead_code)]
    length_scale: f32,
    #[allow(dead_code)]
    noise_scale: f32,
    #[allow(dead_code)]
    noise_w: f32,
}

#[pymethods]
impl PiperScales {
    #[new]
    fn new(length_scale: f32, noise_scale: f32, noise_w: f32) -> PyDengjenResult<Self> {
        Ok(Self {
            length_scale,
            noise_scale,
            noise_w,
        })
    }
}

#[pyclass(weakref, module = "piper")]
#[pyo3(name = "PiperModel")]
struct PiperModel(Arc<dyn DengjenModel + Send + Sync>);

#[pymethods]
impl PiperModel {
    #[new]
    fn new(config_path: &str) -> PyDengjenResult<Self> {
        let vits =
            dengjen_piper::from_config_path(&PathBuf::from(config_path))?;
        Ok(Self(vits))
    }
    #[getter]
    fn get_speaker(&self) -> PyDengjenResult<Option<String>> {
        match self
            .0
            .get_fallback_synthesis_config()?
            .downcast_ref::<PiperSynthesisConfig>()
        {
            Some(synth_config) => match synth_config.speaker {
                Some(sid) => Ok(self.0.speaker_id_to_name(&sid)?),
                None => Ok(None),
            },
            None => Ok(None),
        }
    }
    #[setter]
    fn set_speaker(&self, name: String) -> PyDengjenResult<()> {
        let sid = match self.0.speaker_name_to_id(&name)? {
            Some(sname) => sname,
            None => {
                return Err(DengjenError::OperationError(format!(
                    "A speaker with the given name `{}` was not found",
                    name
                ))
                .into())
            }
        };
        match self
            .0
            .get_fallback_synthesis_config()?
            .downcast::<PiperSynthesisConfig>()
        {
            Ok(mut synth_config) => {
                synth_config.speaker = Some(sid);
                Ok(self.0.set_fallback_synthesis_config(&synth_config)?)
            }
            Err(_) => {
                Err(DengjenError::OperationError("Cannot set synthesis config".to_string()).into())
            }
        }
    }
    fn get_scales(&self) -> PyDengjenResult<PiperScales> {
        match self
            .0
            .get_fallback_synthesis_config()?
            .downcast::<PiperSynthesisConfig>()
        {
            Ok(synth_config) => Ok(PiperScales {
                length_scale: synth_config.length_scale,
                noise_scale: synth_config.noise_scale,
                noise_w: synth_config.noise_w,
            }),
            Err(_) => {
                Err(DengjenError::OperationError("Cannot set synthesis config".to_string()).into())
            }
        }
    }
    fn set_scales(&self, length_scale: f32, noise_scale: f32, noise_w: f32) -> PyDengjenResult<()> {
        match self
            .0
            .get_fallback_synthesis_config()?
            .downcast::<PiperSynthesisConfig>()
        {
            Ok(mut synth_config) => {
                synth_config.length_scale = length_scale;
                synth_config.noise_scale = noise_scale;
                synth_config.noise_w = noise_w;
                Ok(self.0.set_fallback_synthesis_config(&synth_config)?)
            }
            Err(_) => {
                Err(DengjenError::OperationError("Cannot set synthesis config".to_string()).into())
            }
        }
    }
}

#[pyclass(weakref, module = "piper", frozen)]
struct Dengjen(Arc<DengjenSpeechSynthesizer>);

#[pymethods]
impl Dengjen {
    #[staticmethod]
    fn with_piper(vits_model: &PiperModel) -> PyDengjenResult<Self> {
        let model = Arc::clone(&vits_model.0);
        let synthesizer = Arc::new(DengjenSpeechSynthesizer::new(model)?);
        Ok(Self(synthesizer))
    }
    fn synthesize(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
    ) -> PyDengjenResult<LazySpeechStream> {
        self.synthesize_lazy(text, audio_output_config)
    }

    fn synthesize_lazy(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
    ) -> PyDengjenResult<LazySpeechStream> {
        Ok(self
            .0
            .synthesize_lazy(text, audio_output_config.map(|o| o.into()))?
            .into())
    }

    fn synthesize_parallel(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
    ) -> PyDengjenResult<ParallelSpeechStream> {
        Ok(self
            .0
            .synthesize_parallel(text, audio_output_config.map(|o| o.into()))?
            .into())
    }

    fn synthesize_streamed(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
        chunk_size: Option<usize>,
        chunk_padding: Option<usize>,
    ) -> PyDengjenResult<PyRealtimeSpeechStream> {
        let stream = self.0.synthesize_streamed(
            text,
            audio_output_config.map(|o| o.into()),
            chunk_size.unwrap_or(45),
            chunk_padding.unwrap_or(3),
        )?;
        Ok(PyRealtimeSpeechStream(stream))
    }

    fn synthesize_to_file(
        &self,
        filename: &str,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
    ) -> PyDengjenResult<()> {
        self.0
            .synthesize_to_file(&PathBuf::from(filename), text, audio_output_config.map(|o| o.into()))?;
        Ok(())
    }
    #[getter]
    fn language(&self) -> PyDengjenResult<Option<String>> {
        Ok(self.0.get_language()?)
    }
    #[getter]
    fn speakers(&self) -> PyDengjenResult<Option<HashMap<i64, String>>> {
        Ok(self.0.get_speakers()?.cloned())
    }
    fn get_audio_output_info(&self) -> PyDengjenResult<PyWaveInfo> {
        Ok(self.0.audio_output_info()?.into())
    }
}

#[pyfunction]
pub fn phonemize_text(
    text: &str,
    language: &str,
    phoneme_separator: Option<char>,
    remove_lang_switch_flags: Option<bool>,
    remove_stress: Option<bool>,
    use_tashkeel: Option<bool>
) -> PyResult<Vec<String>> {
    let use_tashkeel = (language  == "ar") && use_tashkeel.unwrap_or(true);
    let text = if use_tashkeel {
        let engine= match LIBTASHKEEL_ENGINE.as_ref() {
            Ok(eng) => eng,
            Err(e) => return Err(DengjenException::new_err(e.to_string()))
        };
        match do_tashkeel(engine, text, None, false) {
            Ok(mashkool) => std::borrow::Cow::from(mashkool),
            Err(e) => return Err(DengjenException::new_err(e.to_string()))
        }
    } else {
        std::borrow::Cow::from(text)
    };
    match espeak_phonemizer::text_to_phonemes(
        &text,
        language,
        phoneme_separator.or(None),
        remove_lang_switch_flags.unwrap_or(true),
        remove_stress.unwrap_or(false)
    ) {
        Ok(phonemes) => Ok(phonemes),
        Err(e) => Err(DengjenException::new_err(e.to_string()))
    }
}


/// A fast, local neural text-to-speech engine
#[pymodule]
fn pydengjen(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add("DengjenException", _py.get_type::<DengjenException>())?;
    m.add_class::<Dengjen>()?;
    m.add_class::<PiperModel>()?;
    m.add_class::<PiperScales>()?;
    m.add_class::<PyAudioOutputConfig>()?;
    m.add_class::<WaveSamples>()?;
    m.add_class::<LazySpeechStream>()?;
    m.add_class::<ParallelSpeechStream>()?;
    m.add_class::<PyRealtimeSpeechStream>()?;
    m.add_function(wrap_pyfunction!(phonemize_text, m)?)?;
    Ok(())
}

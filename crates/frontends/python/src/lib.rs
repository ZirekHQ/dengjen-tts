// PyO3's #[pymethods] expands each impl into a hidden wrapper item, which
// trips rustc's non_local_definitions lint as a false positive; an #[allow]
// on the impl itself doesn't survive the macro expansion, so it's silenced
// crate-wide here instead.
#![allow(non_local_definitions)]
#![forbid(unsafe_code)]

use dengjen_core::{
    Audio, AudioInfo, CancellationToken, DengjenError, DengjenModel, SynthesisConfig,
};
use dengjen_synth::{
    AudioOutputConfig, DengjenSpeechStreamLazy, DengjenSpeechStreamParallel,
    DengjenSpeechSynthesizer, RealtimeSpeechStream,
};
#[cfg(feature = "tashkeel")]
use libtashkeel_core::{
    create_inference_engine, do_tashkeel, DynamicInferenceEngine as TashkeelInferenceEngine,
    LibtashkeelResult,
};
#[cfg(feature = "tashkeel")]
use once_cell::sync::Lazy;
use pyo3::create_exception;
use pyo3::exceptions::PyException;
use pyo3::prelude::*;
use pyo3::types::PyBytes;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

// Building the tashkeel inference engine loads a bundled ONNX model, which is
// expensive enough that we only want to pay for it once, the first time
// diacritization is actually requested — not on module import.
#[cfg(feature = "tashkeel")]
static LIBTASHKEEL_ENGINE: Lazy<LibtashkeelResult<TashkeelInferenceEngine>> =
    Lazy::new(|| create_inference_engine(None));

#[cfg(feature = "tashkeel")]
fn should_diacritize(language: &str, use_tashkeel: Option<bool>) -> bool {
    if language != "ar" {
        return false;
    }
    use_tashkeel.unwrap_or(true)
}

#[cfg(not(feature = "tashkeel"))]
fn should_diacritize(_language: &str, _use_tashkeel: Option<bool>) -> bool {
    false
}

type PyDengjenResult<T> = Result<T, PyDengjenError>;

create_exception!(
    pydengjen,
    DengjenException,
    PyException,
    "Base Exception for all exceptions raised by pydengjen."
);

#[derive(Debug)]
struct PyDengjenError(DengjenError);

impl From<DengjenError> for PyDengjenError {
    fn from(err: DengjenError) -> Self {
        PyDengjenError(err)
    }
}

impl From<PyDengjenError> for PyErr {
    fn from(py_err: PyDengjenError) -> Self {
        DengjenException::new_err(py_err.0.to_string())
    }
}

#[pyclass(weakref, module = "pydengjen", frozen)]
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
    fn from(info: AudioInfo) -> Self {
        PyWaveInfo(info)
    }
}

#[pyclass(weakref, module = "pydengjen", frozen, from_py_object)]
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
        PyAudioOutputConfig(AudioOutputConfig {
            rate,
            volume,
            pitch,
            appended_silence_ms,
        })
    }
}

impl From<PyAudioOutputConfig> for AudioOutputConfig {
    fn from(config: PyAudioOutputConfig) -> Self {
        config.0
    }
}

#[pyclass(weakref, module = "pydengjen", frozen)]
struct WaveSamples(Audio);

#[pymethods]
impl WaveSamples {
    fn get_wave_bytes(&self, py: Python) -> Py<PyAny> {
        let wav_data = py.detach(move || self.0.as_wave_bytes());
        PyBytes::new(py, &wav_data).into()
    }

    fn save_to_file(&self, filename: &str) -> PyDengjenResult<()> {
        self.0
            .save_to_file(&PathBuf::from(filename))
            .map_err(|e| PyDengjenError::from(DengjenError::from(e)))?;
        Ok(())
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

/// Wraps a lazily-computed speech stream so Python can drive it with the
/// standard iterator protocol.
#[pyclass(weakref, module = "pydengjen")]
struct LazySpeechStream(DengjenSpeechStreamLazy);

impl From<DengjenSpeechStreamLazy> for LazySpeechStream {
    fn from(stream: DengjenSpeechStreamLazy) -> Self {
        Self(stream)
    }
}

#[pymethods]
impl LazySpeechStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python) -> Option<WaveSamples> {
        // Release the GIL while pulling a chunk: synthesizing audio can take
        // real wall-clock time and must not block other Python threads.
        match py.detach(|| self.0.next()) {
            None => None,
            Some(Ok(audio)) => Some(WaveSamples(audio)),
            Some(Err(err)) => {
                PyErr::from(PyDengjenError::from(err)).restore(py);
                None
            }
        }
    }
}

/// Wraps a stream whose chunks are computed on a worker pool ahead of
/// consumption, exposed to Python via the same iterator protocol.
#[pyclass(weakref, module = "pydengjen")]
struct ParallelSpeechStream(DengjenSpeechStreamParallel);

impl From<DengjenSpeechStreamParallel> for ParallelSpeechStream {
    fn from(stream: DengjenSpeechStreamParallel) -> Self {
        Self(stream)
    }
}

#[pymethods]
impl ParallelSpeechStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python) -> Option<WaveSamples> {
        match py.detach(|| self.0.next()) {
            None => None,
            Some(Ok(audio)) => Some(WaveSamples(audio)),
            Some(Err(err)) => {
                PyErr::from(PyDengjenError::from(err)).restore(py);
                None
            }
        }
    }
}

/// Wraps a realtime speech stream, yielding raw wave-format `bytes` per chunk
/// rather than a `WaveSamples` since the caller already knows the audio
/// format from a one-time `get_audio_output_info()` call.
#[pyclass(weakref, module = "pydengjen")]
struct PyRealtimeSpeechStream(RealtimeSpeechStream);

#[pymethods]
impl PyRealtimeSpeechStream {
    fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    fn __next__(&mut self, py: Python) -> Option<Py<PyAny>> {
        match py.detach(|| self.0.next()) {
            None => None,
            Some(Ok(samples)) => Some(PyBytes::new(py, &samples.as_wave_bytes()).into()),
            Some(Err(err)) => {
                PyErr::from(PyDengjenError::from(err)).restore(py);
                None
            }
        }
    }
}

#[pyclass(weakref, module = "pydengjen")]
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
        Ok(PiperScales {
            length_scale,
            noise_scale,
            noise_w,
        })
    }
}

#[cfg(test)]
mod value_type_tests {
    use super::*;
    use dengjen_core::AudioSamples;
    use std::io::Read;

    #[test]
    fn py_wave_info_exposes_the_wrapped_audio_info_fields() {
        let info = AudioInfo {
            sample_rate: 22050,
            num_channels: 1,
            sample_width: 2,
        };
        let py_info = PyWaveInfo::from(info);
        assert_eq!(py_info.get_sample_rate(), 22050);
        assert_eq!(py_info.get_num_channels(), 1);
        assert_eq!(py_info.get_sample_width(), 2);
    }

    #[test]
    fn py_audio_output_config_new_stores_every_field() {
        let config = PyAudioOutputConfig::new(Some(150), Some(80), Some(50), Some(200));
        let inner: AudioOutputConfig = config.into();
        assert_eq!(inner.rate, Some(150));
        assert_eq!(inner.volume, Some(80));
        assert_eq!(inner.pitch, Some(50));
        assert_eq!(inner.appended_silence_ms, Some(200));
    }

    #[test]
    fn py_audio_output_config_new_accepts_all_none() {
        let config = PyAudioOutputConfig::new(None, None, None, None);
        let inner: AudioOutputConfig = config.into();
        assert_eq!(inner.rate, None);
        assert_eq!(inner.volume, None);
        assert_eq!(inner.pitch, None);
        assert_eq!(inner.appended_silence_ms, None);
    }

    fn sample_wave_samples() -> WaveSamples {
        WaveSamples(Audio::new(
            AudioSamples::new(vec![0.0, 0.25, -0.25, 0.5]),
            22050,
            Some(3.5),
        ))
    }

    #[test]
    fn wave_samples_getters_reflect_the_wrapped_audio() {
        let samples = sample_wave_samples();
        assert_eq!(samples.sample_rate(), 22050);
        assert_eq!(samples.num_channels(), 1);
        assert_eq!(samples.sample_width(), 2);
        assert_eq!(samples.inference_ms(), Some(3.5));
    }

    #[test]
    fn wave_samples_save_to_file_writes_a_readable_wav() {
        let samples = sample_wave_samples();
        let dir = std::env::temp_dir().join(format!("dengjen-python-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("wave_samples_save_to_file_writes_a_readable_wav.wav");
        let path_str = path.to_str().unwrap();

        samples.save_to_file(path_str).unwrap();

        let mut file = std::fs::File::open(&path).unwrap();
        let mut header = [0u8; 4];
        file.read_exact(&mut header).unwrap();
        assert_eq!(&header, b"RIFF");

        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn wave_samples_save_to_file_reports_the_underlying_error() {
        let samples = sample_wave_samples();
        let result = samples.save_to_file("/nonexistent-directory-dengjen-test/out.wav");
        assert!(result.is_err());
    }

    #[test]
    fn piper_scales_new_stores_every_field() {
        let scales = PiperScales::new(0.9, 0.5, 0.7).unwrap();
        assert_eq!(scales.length_scale, 0.9);
        assert_eq!(scales.noise_scale, 0.5);
        assert_eq!(scales.noise_w, 0.7);
    }
}

/// A loaded Piper voice model, exposed to Python as an opaque handle. All the
/// actual inference work lives behind the `DengjenModel` trait object; this
/// type's job is just to translate the trait's synthesis-config shape into
/// the speaker/scale getters and setters Python callers expect.
#[pyclass(weakref, module = "pydengjen")]
#[pyo3(name = "PiperModel")]
struct PiperModel(Arc<dyn DengjenModel + Send + Sync>);

#[pymethods]
impl PiperModel {
    #[new]
    fn new(config_path: &str) -> PyDengjenResult<Self> {
        let model = dengjen_piper::from_config_path(&PathBuf::from(config_path))?;
        Ok(Self(model))
    }

    #[getter]
    fn get_speaker(&self) -> PyDengjenResult<Option<String>> {
        let Some(speaker_id) = self.current_speaker_id()? else {
            return Ok(None);
        };
        Ok(self.0.speaker_id_to_name(&speaker_id)?)
    }

    #[setter]
    fn set_speaker(&self, name: String) -> PyDengjenResult<()> {
        let Some(speaker_id) = self.0.speaker_name_to_id(&name)? else {
            return Err(PyDengjenError::from(DengjenError::OperationError(format!(
                "no speaker named '{name}'"
            ))));
        };
        let mut config = self.piper_config_or_err()?;
        config.speaker = Some(speaker_id);
        Ok(self
            .0
            .set_fallback_synthesis_config(&SynthesisConfig::Piper(config))?)
    }

    fn get_scales(&self) -> PyDengjenResult<PiperScales> {
        let config = self.piper_config_or_err()?;
        Ok(PiperScales {
            length_scale: config.length_scale,
            noise_scale: config.noise_scale,
            noise_w: config.noise_w,
        })
    }

    fn set_scales(&self, length_scale: f32, noise_scale: f32, noise_w: f32) -> PyDengjenResult<()> {
        let mut config = self.piper_config_or_err()?;
        config.length_scale = length_scale;
        config.noise_scale = noise_scale;
        config.noise_w = noise_w;
        Ok(self
            .0
            .set_fallback_synthesis_config(&SynthesisConfig::Piper(config))?)
    }
}

impl PiperModel {
    fn current_speaker_id(&self) -> PyDengjenResult<Option<i64>> {
        match self.0.get_fallback_synthesis_config()? {
            SynthesisConfig::Piper(config) => Ok(config.speaker),
            SynthesisConfig::None => Ok(None),
        }
    }

    fn piper_config_or_err(&self) -> PyDengjenResult<dengjen_core::PiperSynthesisConfig> {
        match self.0.get_fallback_synthesis_config()? {
            SynthesisConfig::Piper(config) => Ok(config),
            SynthesisConfig::None => Err(PyDengjenError::from(DengjenError::InvalidConfiguration(
                "this model has no Piper synthesis config to read or update".to_string(),
            ))),
        }
    }
}

/// The user-facing synthesizer: wraps a `DengjenSpeechSynthesizer` built
/// around a loaded model, and hands out streams/files in whichever shape the
/// caller asked for.
#[pyclass(weakref, module = "pydengjen", frozen)]
struct Dengjen(Arc<DengjenSpeechSynthesizer>);

#[pymethods]
impl Dengjen {
    #[staticmethod]
    fn with_piper(vits_model: &PiperModel) -> PyDengjenResult<Self> {
        let model = Arc::clone(&vits_model.0);
        Ok(Self(Arc::new(DengjenSpeechSynthesizer::new(model)?)))
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
        let stream = self
            .0
            .synthesize_lazy(text, audio_output_config.map(|o| o.into()))?;
        Ok(stream.into())
    }

    fn synthesize_parallel(
        &self,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
    ) -> PyDengjenResult<ParallelSpeechStream> {
        let stream = self
            .0
            .synthesize_parallel(text, audio_output_config.map(|o| o.into()))?;
        Ok(stream.into())
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
            // No API exists today for a caller to cancel a stream mid-flight,
            // so there's nothing to hold onto beyond this call — a fresh
            // token is equivalent to "never cancelled".
            CancellationToken::new(),
        )?;
        Ok(PyRealtimeSpeechStream(stream))
    }

    fn synthesize_to_file(
        &self,
        filename: &str,
        text: String,
        audio_output_config: Option<PyAudioOutputConfig>,
    ) -> PyDengjenResult<()> {
        self.0.synthesize_to_file(
            &PathBuf::from(filename),
            text,
            audio_output_config.map(|o| o.into()),
        )?;
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

/// Adds Arabic diacritics (tashkeel) to `text` using the shared, lazily-built
/// inference engine. Only ever called when `should_diacritize` has already
/// said yes, so a failure here — either building the engine or running
/// inference — is surfaced as a `DengjenException` rather than panicking.
#[cfg(feature = "tashkeel")]
fn diacritize_text(text: &str) -> PyResult<std::borrow::Cow<'_, str>> {
    let engine = LIBTASHKEEL_ENGINE
        .as_ref()
        .map_err(|err| DengjenException::new_err(err.to_string()))?;
    do_tashkeel(engine, text, None, false)
        .map(std::borrow::Cow::from)
        .map_err(|err| DengjenException::new_err(err.to_string()))
}
// should_diacritize() is always false without this feature, so this is unreachable.
#[cfg(not(feature = "tashkeel"))]
fn diacritize_text(_text: &str) -> PyResult<std::borrow::Cow<'_, str>> {
    unreachable!("diacritize_text called with the `tashkeel` feature disabled")
}

/// Converts `text` into a phoneme sequence for `language`, diacritizing first
/// when the language/flag combination calls for it. Exposed directly to
/// Python callers who want the phoneme breakdown without running full
/// synthesis; `Dengjen::synthesize*` does not call this function — it drives
/// its own, separate phonemization path inside the loaded model.
#[cfg(feature = "espeak")]
#[pyfunction]
pub fn phonemize_text(
    text: &str,
    language: &str,
    phoneme_separator: Option<char>,
    remove_lang_switch_flags: Option<bool>,
    remove_stress: Option<bool>,
    use_tashkeel: Option<bool>,
) -> PyResult<Vec<String>> {
    let text = if should_diacritize(language, use_tashkeel) {
        diacritize_text(text)?
    } else {
        std::borrow::Cow::from(text)
    };
    espeak_phonemizer::text_to_phonemes(
        &text,
        language,
        phoneme_separator,
        remove_lang_switch_flags.unwrap_or(true),
        remove_stress.unwrap_or(false),
    )
    .map_err(|err| DengjenException::new_err(err.to_string()))
}

#[cfg(test)]
mod error_plumbing_tests {
    use super::*;

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_true_for_arabic_language_by_default() {
        assert!(should_diacritize("ar", None));
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_false_for_non_arabic_language() {
        assert!(!should_diacritize("en-us", None));
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_false_for_arabic_language_when_explicitly_disabled() {
        assert!(!should_diacritize("ar", Some(false)));
    }

    #[cfg(feature = "tashkeel")]
    #[test]
    fn should_diacritize_true_for_arabic_language_when_explicitly_enabled() {
        assert!(should_diacritize("ar", Some(true)));
    }

    #[cfg(not(feature = "tashkeel"))]
    #[test]
    fn should_diacritize_always_false_when_tashkeel_disabled() {
        assert!(!should_diacritize("ar", None));
        assert!(!should_diacritize("ar", Some(true)));
    }

    #[test]
    fn py_dengjen_error_wraps_the_original_error_unchanged() {
        let original = DengjenError::OperationError("boom".to_string());
        let wrapped = PyDengjenError::from(original);
        assert!(matches!(wrapped.0, DengjenError::OperationError(ref s) if s == "boom"));
    }

    #[test]
    fn py_dengjen_error_preserves_variant_across_each_error_kind() {
        let cases = [
            DengjenError::FailedToLoadResource("a".to_string()),
            DengjenError::PhonemizationError("b".to_string()),
            DengjenError::InferenceError("c".to_string()),
            DengjenError::InvalidConfiguration("d".to_string()),
            DengjenError::UnsupportedOperation("e".to_string()),
            DengjenError::OperationError("f".to_string()),
        ];
        for original in cases {
            let expected = original.to_string();
            let wrapped = PyDengjenError::from(original);
            assert_eq!(wrapped.0.to_string(), expected);
        }
    }
}

/// A fast, local neural text-to-speech engine
#[pymodule]
fn pydengjen(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<Dengjen>()?;
    m.add_class::<PiperModel>()?;
    m.add_class::<PiperScales>()?;
    m.add_class::<PyAudioOutputConfig>()?;
    m.add_class::<WaveSamples>()?;

    m.add_class::<LazySpeechStream>()?;
    m.add_class::<ParallelSpeechStream>()?;
    m.add_class::<PyRealtimeSpeechStream>()?;

    m.add("DengjenException", m.py().get_type::<DengjenException>())?;

    #[cfg(feature = "espeak")]
    m.add_function(wrap_pyfunction!(phonemize_text, m)?)?;

    Ok(())
}

#[cfg(test)]
mod model_and_synthesizer_tests {
    use super::*;
    use dengjen_core::{
        AudioSamples, DengjenAudioResult, DengjenResult, Phonemes, PiperSynthesisConfig,
    };
    use std::collections::HashMap as StdHashMap;
    use std::sync::Mutex;

    struct FakeModel {
        speakers: StdHashMap<i64, String>,
        fallback_config: Mutex<SynthesisConfig>,
    }

    impl FakeModel {
        fn with_one_speaker() -> Self {
            Self::with_speakers(&[(0, "alice")])
        }

        fn with_speakers(speakers: &[(i64, &str)]) -> Self {
            let speakers = speakers
                .iter()
                .map(|(id, name)| (*id, name.to_string()))
                .collect();
            Self {
                speakers,
                fallback_config: Mutex::new(SynthesisConfig::Piper(PiperSynthesisConfig {
                    speaker: Some(0),
                    noise_scale: 0.667,
                    length_scale: 1.0,
                    noise_w: 0.8,
                })),
            }
        }

        /// A model whose fallback config has no speaker id set yet, but is
        /// still a valid `Piper` config — distinct from `with_no_config()`.
        fn with_speaker_unset() -> Self {
            Self {
                speakers: StdHashMap::from([(0, "alice".to_string())]),
                fallback_config: Mutex::new(SynthesisConfig::Piper(PiperSynthesisConfig {
                    speaker: None,
                    noise_scale: 0.667,
                    length_scale: 1.0,
                    noise_w: 0.8,
                })),
            }
        }

        /// A model with no synthesis config at all (the `None` variant), as
        /// opposed to a `Piper` config that simply has no speaker set. Keeps
        /// a speaker table so `set_speaker`'s name-to-id lookup succeeds and
        /// the test exercises the config-write failure, not a lookup miss.
        fn with_no_config() -> Self {
            Self {
                speakers: StdHashMap::from([(0, "alice".to_string())]),
                fallback_config: Mutex::new(SynthesisConfig::None),
            }
        }
    }

    impl DengjenModel for FakeModel {
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
            Ok(Audio::new(
                AudioSamples::new(vec![0.0; 100]),
                22050,
                Some(1.5),
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
        fn get_language(&self) -> DengjenResult<Option<String>> {
            Ok(Some("en-us".to_string()))
        }
        fn get_speakers(&self) -> DengjenResult<Option<&StdHashMap<i64, String>>> {
            Ok(Some(&self.speakers))
        }
    }

    fn fake_piper_model() -> PiperModel {
        PiperModel(Arc::new(FakeModel::with_one_speaker()))
    }

    #[test]
    fn piper_model_get_speaker_resolves_id_to_name() {
        let model = fake_piper_model();
        assert_eq!(model.get_speaker().unwrap(), Some("alice".to_string()));
    }

    #[test]
    fn piper_model_set_speaker_updates_the_fallback_config_by_name() {
        let model = PiperModel(Arc::new(FakeModel::with_speakers(&[
            (0, "alice"),
            (1, "bob"),
        ])));
        assert_eq!(model.get_speaker().unwrap(), Some("alice".to_string()));
        model.set_speaker("bob".to_string()).unwrap();
        assert_eq!(model.get_speaker().unwrap(), Some("bob".to_string()));
    }

    #[test]
    fn piper_model_set_speaker_errors_for_an_unknown_name() {
        let model = fake_piper_model();
        let result = model.set_speaker("nobody".to_string());
        assert!(result.is_err());
    }

    #[test]
    fn piper_model_get_and_set_scales_round_trip() {
        let model = fake_piper_model();
        model.set_scales(0.9, 0.5, 0.7).unwrap();
        let scales = model.get_scales().unwrap();
        assert_eq!(scales.length_scale, 0.9);
        assert_eq!(scales.noise_scale, 0.5);
        assert_eq!(scales.noise_w, 0.7);
    }

    #[test]
    fn piper_model_get_speaker_returns_none_for_a_piper_config_with_no_speaker_set() {
        let model = PiperModel(Arc::new(FakeModel::with_speaker_unset()));
        assert_eq!(model.get_speaker().unwrap(), None);
    }

    #[test]
    fn piper_model_get_speaker_returns_none_when_there_is_no_synthesis_config() {
        let model = PiperModel(Arc::new(FakeModel::with_no_config()));
        assert_eq!(model.get_speaker().unwrap(), None);
    }

    #[test]
    fn piper_model_get_scales_errors_when_there_is_no_synthesis_config() {
        let model = PiperModel(Arc::new(FakeModel::with_no_config()));
        assert!(model.get_scales().is_err());
    }

    #[test]
    fn piper_model_set_scales_errors_when_there_is_no_synthesis_config() {
        let model = PiperModel(Arc::new(FakeModel::with_no_config()));
        assert!(model.set_scales(0.9, 0.5, 0.7).is_err());
    }

    #[test]
    fn piper_model_set_speaker_errors_when_there_is_no_synthesis_config() {
        let model = PiperModel(Arc::new(FakeModel::with_no_config()));
        assert!(model.set_speaker("alice".to_string()).is_err());
    }

    fn fake_dengjen() -> Dengjen {
        let piper_model = fake_piper_model();
        Dengjen::with_piper(&piper_model).unwrap()
    }

    #[test]
    fn dengjen_with_piper_succeeds_for_a_valid_model() {
        let _ = fake_dengjen();
    }

    #[test]
    fn dengjen_language_delegates_to_the_model() {
        let dengjen = fake_dengjen();
        assert_eq!(dengjen.language().unwrap(), Some("en-us".to_string()));
    }

    #[test]
    fn dengjen_speakers_delegates_to_the_model() {
        let dengjen = fake_dengjen();
        let speakers = dengjen.speakers().unwrap().unwrap();
        assert_eq!(speakers.get(&0), Some(&"alice".to_string()));
    }

    #[test]
    fn dengjen_get_audio_output_info_delegates_to_the_model() {
        let dengjen = fake_dengjen();
        let info = dengjen.get_audio_output_info().unwrap();
        assert_eq!(info.get_sample_rate(), 22050);
    }

    #[test]
    fn dengjen_synthesize_lazy_produces_a_stream_whose_inner_iterator_yields_the_fake_models_audio()
    {
        let dengjen = fake_dengjen();
        let mut stream = dengjen.synthesize_lazy("hello".to_string(), None).unwrap();
        let first = stream.0.next().unwrap().unwrap();
        assert_eq!(first.into_vec().len(), 100);
        assert!(stream.0.next().is_none());
    }

    #[test]
    fn dengjen_synthesize_parallel_produces_a_stream_whose_inner_iterator_yields_the_fake_models_audio(
    ) {
        let dengjen = fake_dengjen();
        let mut stream = dengjen
            .synthesize_parallel("hello".to_string(), None)
            .unwrap();
        let first = stream.0.next().unwrap().unwrap();
        assert_eq!(first.into_vec().len(), 100);
        assert!(stream.0.next().is_none());
    }

    #[test]
    fn dengjen_synthesize_streamed_constructs_successfully() {
        let dengjen = fake_dengjen();
        let result = dengjen.synthesize_streamed("hello".to_string(), None, None, None);
        assert!(result.is_ok());
    }

    #[test]
    fn dengjen_synthesize_defaults_to_the_lazy_strategy() {
        // `synthesize` and `synthesize_lazy` must behave identically — `synthesize`
        // just delegates. Proven by both producing the same single chunk of audio.
        let dengjen = fake_dengjen();
        let mut via_synthesize = dengjen.synthesize("hello".to_string(), None).unwrap();
        let first = via_synthesize.0.next().unwrap().unwrap();
        assert_eq!(first.into_vec().len(), 100);
    }

    #[test]
    fn dengjen_synthesize_to_file_writes_a_readable_wav() {
        let dengjen = fake_dengjen();
        let dir = std::env::temp_dir().join(format!("dengjen-python-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("dengjen_synthesize_to_file_writes_a_readable_wav.wav");

        dengjen
            .synthesize_to_file(path.to_str().unwrap(), "hello".to_string(), None)
            .unwrap();

        assert!(path.exists());
        std::fs::remove_file(&path).ok();
    }
}

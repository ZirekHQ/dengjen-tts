use ffi_support::{call_with_result, define_string_destructor, ErrorCode, ExternError, FfiStr};
use dengjen_core::{AudioSamples, CancellationToken, DengjenError, DengjenModel, DengjenResult};
use dengjen_synth::{AudioOutputConfig, DengjenSpeechSynthesizer, SYNTHESIS_THREAD_POOL};
use std::any::Any;
use std::ops::Deref;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};

pub type SpeechSynthesisCallback = extern "C" fn(SynthesisEvent) -> u8;
define_string_destructor!(_internal_libdengjenFreeString);
ffi_support::implement_into_ffi_by_pointer!(DengjenVoice);
ffi_support::define_box_destructor!(DengjenVoice, _internal_libdengjenUnloadDengjenVoice);
ffi_support::implement_into_ffi_by_pointer!(PiperSynthConfig);
ffi_support::define_box_destructor!(PiperSynthConfig, _internal_libdengjenFreePiperSynthConfig);

static INIT_ORT_ENVIRONMENT: Once = Once::new();

pub mod error_codes {
    pub const INVALID_SYNTHESIS_MODE: i32 = 16;
    pub const FAILED_TO_LOAD_RESOURCE: i32 = 17;
    pub const PHONEMIZATION_ERROR: i32 = 18;
    pub const OPERATION_ERROR: i32 = 19;
    pub const INVALID_UTF8_SEQUENCE: i32 = 20;
    pub const UNKNOWN_ERROR: i32 = 21;
    pub const NULL_POINTER: i32 = 22;
}

pub mod synth_event {
    pub const SYNTH_EVENT_SPEECH: i32 = 0;
    pub const SYNTH_EVENT_FINISHED: i32 = 1;
    pub const SYNTH_EVENT_ERROR: i32 = 2;
}

pub mod synth_mode {
    pub const SYNTH_MODE_LAZY: i32 = 0;
    pub const SYNTH_MODE_PARALLEL: i32 = 1;
    pub const SYNTH_MODE_REALTIME: i32 = 2;
}

pub struct DengjenVoice {
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    active_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
}

impl From<DengjenSpeechSynthesizer> for DengjenVoice {
    fn from(other: DengjenSpeechSynthesizer) -> Self {
        Self {
            synth: AssertUnwindSafe(Arc::new(other)),
            active_cancel_token: Arc::new(Mutex::new(None)),
        }
    }
}

impl Deref for DengjenVoice {
    type Target = DengjenSpeechSynthesizer;

    fn deref(&self) -> &Self::Target {
        &self.synth
    }
}

impl<T> AsRef<T> for DengjenVoice
where
    T: ?Sized,
    <DengjenVoice as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        self.deref().as_ref()
    }
}

#[derive(Debug)]
pub struct DengjenFFIError(i32, String);

impl DengjenFFIError {
    fn invalid_utf8() -> Self {
        Self(
            error_codes::INVALID_UTF8_SEQUENCE,
            "Invalid utf-8 input.".to_string(),
        )
    }
    fn invalid_synthesis_mode() -> Self {
        Self(error_codes::INVALID_SYNTHESIS_MODE, "Invalid synthesis mode".to_string())
    }
    fn null_pointer(param_name: &str) -> Self {
        Self(error_codes::NULL_POINTER, format!("`{}` must not be null", param_name))
    }
}

impl From<DengjenError> for DengjenFFIError {
    fn from(other: DengjenError) -> Self {
        let (code, message) = match other {
            DengjenError::FailedToLoadResource(msg) => (error_codes::FAILED_TO_LOAD_RESOURCE, msg),
            DengjenError::PhonemizationError(msg) => (error_codes::PHONEMIZATION_ERROR, msg),
            DengjenError::OperationError(msg) => (error_codes::OPERATION_ERROR, msg),
        };
        Self(code, message)
    }
}

impl From<DengjenFFIError> for ExternError {
    fn from(other: DengjenFFIError) -> Self {
        let err_code = ErrorCode::new(other.0);
        ExternError::new_error(err_code, other.1)
    }
}

pub type DengjenFFIResult<T> = Result<T, DengjenFFIError>;

#[repr(C)]
pub struct SynthesisEvent {
    event_type: i32,
    error_ptr: *mut ExternError,
    len: i64, // usize causes issues with JNI
    data: *mut u8,
}

impl SynthesisEvent {
    fn with_speech(speech: Vec<u8>) -> Self {
        let mut buf = speech.into_boxed_slice();
        let data = buf.as_mut_ptr();
        let len = buf.len();
        std::mem::forget(buf);
        Self {
            event_type: synth_event::SYNTH_EVENT_SPEECH,
            error_ptr: std::ptr::null_mut(),
            len: len as i64,
            data,
        }
    }
    fn with_error(error: impl Into<ExternError>) -> Self {
        let mut event = Self::with_speech(Vec::with_capacity(0));
        event.event_type = synth_event::SYNTH_EVENT_ERROR;
        event.error_ptr = Box::into_raw(Box::new(error.into()));
        event
    }
    fn with_finished() -> Self {
        let mut event = Self::with_speech(Vec::with_capacity(0));
        event.event_type = synth_event::SYNTH_EVENT_FINISHED;
        event.error_ptr = std::ptr::null_mut();
        event
    }
}

#[repr(C)]
pub struct AudioInfo {
    sample_rate: u32,
    num_channels: u32,
    sample_width: u32,
}

#[derive(Clone)]
#[repr(C)]
pub struct SynthesisParams {
    mode: i32,
    rate: u8,
    volume: u8,
    pitch: u8,
    appended_silence_ms: u32,
    callback: SpeechSynthesisCallback,
    nonblocking: u8,
}

impl SynthesisParams {
    fn as_synth_output_config(&self) -> AudioOutputConfig {
        AudioOutputConfig {
            rate: Some(self.rate),
            volume: Some(self.volume),
            pitch: Some(self.pitch),
            appended_silence_ms: Some(self.appended_silence_ms),
        }
    }
}

#[repr(C)]
pub struct PiperSynthConfig {
    speaker: u32,
    length_scale: f32,
    noise_scale: f32,
    noise_w: f32,
}

impl PiperSynthConfig {
    fn as_piper_synth_config(&self) -> dengjen_piper::PiperSynthesisConfig {
        dengjen_piper::PiperSynthesisConfig {
            speaker: Some(self.speaker.into()),
            noise_scale: self.noise_scale,
            length_scale: self.length_scale,
            noise_w: self.noise_w,
        }
    }
}

/// # Safety
/// Pointer must be non-null and well alighned
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenFreeString(string_ptr: *mut i8) {
    _internal_libdengjenFreeString(string_ptr)
}

/// # Safety
/// Pointer must be non-null and well alighned
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenFreePiperSynthConfig(synth_config: *mut PiperSynthConfig) {
    _internal_libdengjenFreePiperSynthConfig(synth_config)
}
/// # Safety
/// Pointer must be non-null and well alighned
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenFreeSynthesisEvent(event: SynthesisEvent) {
    ffi_support::abort_on_panic::with_abort_on_panic(|| {
        if !event.error_ptr.is_null() {
            drop(Box::from_raw(event.error_ptr));
        }
        let s = std::slice::from_raw_parts_mut(event.data, event.len as usize);
        drop(Box::from_raw(s as *mut [u8]));
    });
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn libdengjenLoadVoiceFromConfigPath(
    config_path_ptr: FfiStr,
    out_error: &mut ExternError,
) -> *mut DengjenVoice {
    call_with_result(out_error, move || _load_piper_voice(config_path_ptr))
}

/// # Safety
/// Pointer must be non-null and well alighned
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenUnloadDengjenVoice(voice_ptr: *mut DengjenVoice) {
    _internal_libdengjenUnloadDengjenVoice(voice_ptr)
}

/// # Safety
/// If non-null, `voice_ptr` and `audio_info_ptr` must be well-aligned and point to a valid
/// `DengjenVoice` and `AudioInfo` respectively. A null pointer is handled gracefully (returns a
/// NULL_POINTER error via `out_error`).
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenGetAudioInfo(
    voice_ptr: *mut DengjenVoice,
    audio_info_ptr: *mut AudioInfo,
    out_error: &mut ExternError,
) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    let Some(audio_info_mut) = audio_info_ptr.as_mut() else {
        *out_error = DengjenFFIError::null_pointer("audio_info_ptr").into();
        return;
    };
    let mut audio_info = AssertUnwindSafe(audio_info_mut);
    call_with_result(out_error, move || {
        voice
            .audio_output_info()
            .map(|a| {
                audio_info.sample_rate = a.sample_rate as u32;
                audio_info.num_channels = a.num_channels as u32;
                audio_info.sample_width = a.sample_width as u32;
            })
            .map_err(DengjenFFIError::from)
    })
}

/// # Safety
/// If non-null, the pointer must be well-aligned and point to a valid `DengjenVoice`. A null
/// pointer is handled gracefully (returns a NULL_POINTER error via `out_error`).
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenGetPiperDefaultSynthConfig(
    voice_ptr: *mut DengjenVoice,
    out_error: &mut ExternError,
) -> *mut PiperSynthConfig {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return std::ptr::null_mut();
    };
    call_with_result(out_error, move || {
        let synth_config = voice
            .get_default_synthesis_config()
            .map_err(DengjenFFIError::from)?;
        match synth_config.downcast_ref::<dengjen_piper::PiperSynthesisConfig>() {
            Some(config) => Ok(PiperSynthConfig {
                speaker: config.speaker.map(|sid| sid as u32).unwrap_or_default(),
                length_scale: config.length_scale,
                noise_scale: config.noise_scale,
                noise_w: config.noise_w,
            }),
            None => Err(DengjenFFIError(
                error_codes::UNKNOWN_ERROR,
                "Cannot retrieve Piper's default synthesis config".to_string(),
            )),
        }
    })
}

/// # Safety
/// If non-null, the pointer must be well-aligned and point to a valid `DengjenVoice`. A null
/// pointer is handled gracefully (returns a NULL_POINTER error via `out_error`).
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSetPiperSynthConfig(
    voice_ptr: *mut DengjenVoice,
    synth_config: PiperSynthConfig,
    out_error: &mut ExternError,
) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    call_with_result(out_error, move || {
        let piper_synth_config = synth_config.as_piper_synth_config();
        let config = &piper_synth_config as &dyn Any;
        voice
            .set_fallback_synthesis_config(config)
            .map_err(DengjenFFIError::from)
    })
}

/// # Safety
/// If non-null, the pointer must be well-aligned and point to a valid `DengjenVoice`. A null
/// pointer is handled gracefully (returns a NULL_POINTER error via `out_error`).
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSpeak(
    voice_ptr: *mut DengjenVoice,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_error: &mut ExternError,
) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    let synth = AssertUnwindSafe(Arc::clone(&voice.synth));
    let cancel_slot = Arc::clone(&voice.active_cancel_token);
    call_with_result(out_error, move || _synthesize(synth, cancel_slot, text_ptr, params))
}

/// # Safety
/// If non-null, the pointer must be well-aligned and point to a valid `DengjenVoice`. A null
/// pointer is handled gracefully (returns a NULL_POINTER error via `out_error`). Safe to call
/// from a different thread than the one that called `libdengjenSpeak` — that's the point.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenCancel(voice_ptr: *mut DengjenVoice, out_error: &mut ExternError) {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return;
    };
    call_with_result(out_error, move || _cancel(voice))
}

/// # Safety
/// If non-null, the pointer must be well-aligned and point to a valid `DengjenVoice`. A null
/// pointer is handled gracefully (returns a NULL_POINTER error via `out_error`).
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSpeakToFile(
    voice_ptr: *mut DengjenVoice,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_filename_ptr: FfiStr,
    out_error: &mut ExternError,
) -> u8 {
    let Some(voice) = voice_ptr.as_ref() else {
        *out_error = DengjenFFIError::null_pointer("voice_ptr").into();
        return 0;
    };
    let synth = AssertUnwindSafe(Arc::clone(&voice.synth));
    call_with_result(out_error, move || {
        Ok::<u8, DengjenFFIError>(
            _synthesize_to_file(synth, text_ptr, params, out_filename_ptr).is_ok() as u8,
        )
    })
}

fn init_ort_environment()  {
    INIT_ORT_ENVIRONMENT.call_once(|| {
        let execution_providers = [
            #[cfg(target_os = "android")]
            ort::execution_providers::NNAPI::default().build(),
            #[cfg(target_os = "ios")]
            ort::execution_providers::CoreML::default().build(),
            ort::execution_providers::CPU::default().build(),
        ];
        let committed = ort::init()
            .with_name("dengjen")
            .with_execution_providers(execution_providers)
            .commit();
        assert!(committed, "Failed to initialize onnxruntime");
    });
}

fn _load_piper_voice(config_path_ptr: FfiStr) -> DengjenFFIResult<DengjenVoice> {
    init_ort_environment();
    let config_path = config_path_ptr
        .into_opt_string()
        .ok_or_else(DengjenFFIError::invalid_utf8)?;
    let config_path = PathBuf::from(config_path);
    let piper_model = dengjen_piper::from_config_path(&config_path)?;
    let synth = DengjenSpeechSynthesizer::new(piper_model)?;
    Ok(synth.into())
}

fn _cancel(voice: &DengjenVoice) -> DengjenFFIResult<()> {
    if let Some(token) = voice.active_cancel_token.lock().unwrap().as_ref() {
        token.cancel();
    }
    Ok(())
}

/// Clears `slot` back to `None` on drop, but only if it still holds the token this guard
/// was created for — protects against two concurrent realtime speaks on the same voice
/// (nonblocking mode) where a finishing call must not clobber a still-in-flight sibling's
/// token. Clearing on `Drop` (rather than after `iterate_stream` returns) also covers the
/// early-return-on-error path through `?`.
struct CancelSlotGuard {
    slot: Arc<Mutex<Option<CancellationToken>>>,
    token: CancellationToken,
}

impl Drop for CancelSlotGuard {
    fn drop(&mut self) {
        let mut slot = self.slot.lock().unwrap();
        if let Some(current) = slot.as_ref() {
            if current.points_to_same_flag(&self.token) {
                *slot = None;
            }
        }
    }
}

fn _synthesize(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    cancel_slot: Arc<Mutex<Option<CancellationToken>>>,
    text_ptr: FfiStr,
    params: SynthesisParams,
) -> DengjenFFIResult<()> {
    let text = text_ptr
        .into_opt_string()
        .ok_or_else(DengjenFFIError::invalid_utf8)?;
    if params.nonblocking != 0 {
        SYNTHESIS_THREAD_POOL.spawn(move || {
            let callback = params.callback;
            if let Err(e) = _do_synthesize(synth, cancel_slot, text, params) {
                let event = SynthesisEvent::with_error(e);
                callback(event);
            }
        });
    } else {
        _do_synthesize(synth, cancel_slot, text, params)?;
    }
    Ok(())
}

fn _do_synthesize(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    cancel_slot: Arc<Mutex<Option<CancellationToken>>>,
    text: String,
    params: SynthesisParams,
) -> DengjenFFIResult<()> {
    let audio_output_config = Some(params.as_synth_output_config());
    match params.mode {
        synth_mode::SYNTH_MODE_LAZY => {
            let stream = synth
                .synthesize_lazy(text, audio_output_config)?
                .map(|wr| wr.map(|aud| aud.samples));
            iterate_stream(stream, params.callback)
        }
        synth_mode::SYNTH_MODE_PARALLEL => {
            let stream = synth
                .synthesize_parallel(text, audio_output_config)?
                .map(|wr| wr.map(|aud| aud.samples));
            iterate_stream(stream, params.callback)
        }
        synth_mode::SYNTH_MODE_REALTIME => {
            let cancel_token = CancellationToken::new();
            *cancel_slot.lock().unwrap() = Some(cancel_token.clone());
            let _clear_on_drop = CancelSlotGuard { slot: Arc::clone(&cancel_slot), token: cancel_token.clone() };
            let stream = synth.synthesize_streamed(text, audio_output_config, 72, 3, cancel_token)?;
            iterate_stream(stream, params.callback)
        }
        _ => Err(DengjenFFIError::invalid_synthesis_mode())
    }
}

#[inline(always)]
fn iterate_stream(
    stream: impl Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'static,
    callback: SpeechSynthesisCallback,
) -> DengjenFFIResult<()> {
    for result in stream {
        match result {
            Ok(audio) => {
                let wav_bytes = audio.as_wave_bytes();
                let event = SynthesisEvent::with_speech(wav_bytes);
                if callback(event) != 0 {
                    return Ok(());
                }
            }
            Err(e) => {
                let event = SynthesisEvent::with_error(DengjenFFIError::from(e));
                callback(event);
                return Ok(());
            }
        };
    }
    callback(SynthesisEvent::with_finished());
    Ok(())
}

fn _synthesize_to_file(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_filename_ptr: FfiStr,
) -> DengjenFFIResult<()> {
    let text = text_ptr
        .into_opt_string()
        .ok_or_else(DengjenFFIError::invalid_utf8)?;
    let out_filename = out_filename_ptr
        .into_opt_string()
        .ok_or_else(DengjenFFIError::invalid_utf8)
        .map(PathBuf::from)?;
    synth.synthesize_to_file(&out_filename, text, Some(params.as_synth_output_config()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ffi_support::ExternError;

    extern "C" fn noop_callback(_event: SynthesisEvent) -> u8 {
        1
    }

    fn synth_params() -> SynthesisParams {
        SynthesisParams {
            mode: synth_mode::SYNTH_MODE_LAZY,
            rate: 50,
            volume: 100,
            pitch: 50,
            appended_silence_ms: 0,
            callback: noop_callback,
            nonblocking: 0,
        }
    }

    #[test]
    fn get_audio_info_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let mut audio_info = AudioInfo { sample_rate: 0, num_channels: 0, sample_width: 0 };
        unsafe {
            libdengjenGetAudioInfo(std::ptr::null_mut(), &mut audio_info, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn get_piper_default_synth_config_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let result =
            unsafe { libdengjenGetPiperDefaultSynthConfig(std::ptr::null_mut(), &mut out_error) };
        assert!(result.is_null());
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn set_piper_synth_config_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let synth_config =
            PiperSynthConfig { speaker: 0, length_scale: 1.0, noise_scale: 1.0, noise_w: 1.0 };
        unsafe {
            libdengjenSetPiperSynthConfig(std::ptr::null_mut(), synth_config, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn speak_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let text = std::ffi::CString::new("hello").unwrap();
        unsafe {
            libdengjenSpeak(
                std::ptr::null_mut(),
                FfiStr::from_cstr(&text),
                synth_params(),
                &mut out_error,
            );
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn speak_to_file_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let text = std::ffi::CString::new("hello").unwrap();
        let filename = std::ffi::CString::new("out.wav").unwrap();
        let result = unsafe {
            libdengjenSpeakToFile(
                std::ptr::null_mut(),
                FfiStr::from_cstr(&text),
                synth_params(),
                FfiStr::from_cstr(&filename),
                &mut out_error,
            )
        };
        assert_eq!(result, 0);
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn cancel_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        unsafe {
            libdengjenCancel(std::ptr::null_mut(), &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
    }

    #[test]
    fn cancel_slot_guard_clears_the_slot_when_it_still_holds_its_own_token() {
        let slot = Arc::new(Mutex::new(None));
        let token = CancellationToken::new();
        *slot.lock().unwrap() = Some(token.clone());

        drop(CancelSlotGuard { slot: Arc::clone(&slot), token });

        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn cancel_slot_guard_does_not_clobber_a_different_tokens_slot() {
        // Simulates two concurrent nonblocking realtime speaks on one voice: A's guard must
        // not clear the slot once B has taken it over, or B silently becomes uncancellable.
        let slot = Arc::new(Mutex::new(None));
        let token_a = CancellationToken::new();
        let token_b = CancellationToken::new();
        *slot.lock().unwrap() = Some(token_a.clone());
        let guard_a = CancelSlotGuard { slot: Arc::clone(&slot), token: token_a };

        *slot.lock().unwrap() = Some(token_b.clone());
        drop(guard_a);

        let held = slot.lock().unwrap();
        assert!(held.as_ref().unwrap().points_to_same_flag(&token_b));
    }

    #[test]
    fn error_codes_round_trip_through_dengjen_ffi_error() {
        let cases = [
            (DengjenError::FailedToLoadResource("x".into()), error_codes::FAILED_TO_LOAD_RESOURCE),
            (DengjenError::PhonemizationError("x".into()), error_codes::PHONEMIZATION_ERROR),
            (DengjenError::OperationError("x".into()), error_codes::OPERATION_ERROR),
        ];
        for (err, expected_code) in cases {
            let ffi_err: DengjenFFIError = err.into();
            assert_eq!(ffi_err.0, expected_code);
        }
    }
}

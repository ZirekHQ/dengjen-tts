use dengjen_tts::{AudioOutputConfig, DengjenSpeechSynthesizer, SYNTHESIS_THREAD_POOL};
use dengjen_tts_core::{
    AudioSamples, CancellationToken, DengjenError, DengjenModel, DengjenResult,
};
use ffi_support::{call_with_result, define_string_destructor, ErrorCode, ExternError, FfiStr};
use std::ops::Deref;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, Once};

/// The C function-pointer signature every streaming synthesis callback must match.
pub type SpeechSynthesisCallback = extern "C" fn(SynthesisEvent) -> u8;

define_string_destructor!(_internal_libdengjenFreeString);
ffi_support::implement_into_ffi_by_pointer!(DengjenVoice);
ffi_support::define_box_destructor!(DengjenVoice, _internal_libdengjenUnloadDengjenVoice);
ffi_support::implement_into_ffi_by_pointer!(PiperSynthConfig);
ffi_support::define_box_destructor!(PiperSynthConfig, _internal_libdengjenFreePiperSynthConfig);

/// Guards the one-time onnxruntime environment setup so repeated voice loads don't re-init it.
static INIT_ORT_ENVIRONMENT: Once = Once::new();

/// FFI error codes — part of the C ABI (`libdengjen.h`); names and values are frozen.
pub mod error_codes {
    pub const INVALID_SYNTHESIS_MODE: i32 = 16;
    pub const FAILED_TO_LOAD_RESOURCE: i32 = 17;
    pub const PHONEMIZATION_ERROR: i32 = 18;
    pub const OPERATION_ERROR: i32 = 19;
    pub const INVALID_UTF8_SEQUENCE: i32 = 20;
    pub const UNKNOWN_ERROR: i32 = 21;
    pub const NULL_POINTER: i32 = 22;
    pub const INFERENCE_ERROR: i32 = 23;
    pub const INVALID_CONFIGURATION: i32 = 24;
    pub const UNSUPPORTED_OPERATION: i32 = 25;
}

/// `SynthesisEvent::event_type` values — part of the C ABI, same stability requirement as `error_codes`.
pub mod synth_event {
    pub const SYNTH_EVENT_SPEECH: i32 = 0;
    pub const SYNTH_EVENT_FINISHED: i32 = 1;
    pub const SYNTH_EVENT_ERROR: i32 = 2;
}

/// `SynthesisParams::mode` values — part of the C ABI, same stability requirement as `error_codes`.
pub mod synth_mode {
    pub const SYNTH_MODE_LAZY: i32 = 0;
    pub const SYNTH_MODE_PARALLEL: i32 = 1;
    pub const SYNTH_MODE_REALTIME: i32 = 2;
}

/// An opaque, loaded voice handed back to C callers as a raw pointer.
pub struct DengjenVoice {
    /// Wrapped in AssertUnwindSafe because panics must be caught at unsafe extern "C" boundaries, not unwound.
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    /// Cancellation token for the current realtime synthesis, if any; lets cancel() stop it.
    active_cancel_token: Arc<Mutex<Option<CancellationToken>>>,
}

impl DengjenVoice {
    /// Pairs an already-shared synthesizer handle with a fresh, empty cancellation slot.
    fn wrapping(synth: Arc<DengjenSpeechSynthesizer>) -> Self {
        Self {
            active_cancel_token: Arc::new(Mutex::new(None)),
            synth: AssertUnwindSafe(synth),
        }
    }
}

impl From<DengjenSpeechSynthesizer> for DengjenVoice {
    fn from(synthesizer: DengjenSpeechSynthesizer) -> Self {
        Self::wrapping(Arc::new(synthesizer))
    }
}

impl Deref for DengjenVoice {
    type Target = DengjenSpeechSynthesizer;

    fn deref(&self) -> &Self::Target {
        &self.synth
    }
}

// A `DengjenVoice` stands in for `&DengjenSpeechSynthesizer` (or anything that in turn converts
// to) anywhere a caller only needs `T`, without exposing the wrapper's own fields.
impl<T> AsRef<T> for DengjenVoice
where
    T: ?Sized,
    <DengjenVoice as Deref>::Target: AsRef<T>,
{
    fn as_ref(&self) -> &T {
        (**self.synth).as_ref()
    }
}

/// This crate's internal error type: an `error_codes` value paired with a human-readable message.
#[derive(Debug)]
pub struct DengjenFFIError(i32, String);

impl DengjenFFIError {
    /// Shared constructor the three FFI-boundary-specific errors below route through.
    fn with_code(code: i32, message: impl Into<String>) -> Self {
        Self(code, message.into())
    }

    /// A string argument from C was not valid UTF-8.
    fn invalid_utf8() -> Self {
        Self::with_code(
            error_codes::INVALID_UTF8_SEQUENCE,
            "input string is not valid UTF-8",
        )
    }

    /// A `SynthesisParams::mode` value didn't match any `synth_mode` constant.
    fn invalid_synthesis_mode() -> Self {
        Self::with_code(
            error_codes::INVALID_SYNTHESIS_MODE,
            "synthesis mode is not a recognized value",
        )
    }

    /// A required pointer argument was null. `param_name` identifies which one, for the caller.
    fn null_pointer(param_name: &str) -> Self {
        Self::with_code(
            error_codes::NULL_POINTER,
            format!("parameter `{param_name}` must not be null"),
        )
    }
}

impl From<DengjenError> for DengjenFFIError {
    fn from(error: DengjenError) -> Self {
        // Which `error_codes` constant applies depends only on the variant, not its payload.
        let code = match &error {
            DengjenError::FailedToLoadResource(_) => error_codes::FAILED_TO_LOAD_RESOURCE,
            DengjenError::PhonemizationError(_) => error_codes::PHONEMIZATION_ERROR,
            DengjenError::InferenceError(_) => error_codes::INFERENCE_ERROR,
            DengjenError::InvalidConfiguration(_) => error_codes::INVALID_CONFIGURATION,
            DengjenError::UnsupportedOperation(_) => error_codes::UNSUPPORTED_OPERATION,
            DengjenError::OperationError(_) => error_codes::OPERATION_ERROR,
        };
        // Every variant carries exactly one message string; take it unchanged.
        let (DengjenError::FailedToLoadResource(message)
        | DengjenError::PhonemizationError(message)
        | DengjenError::InferenceError(message)
        | DengjenError::InvalidConfiguration(message)
        | DengjenError::UnsupportedOperation(message)
        | DengjenError::OperationError(message)) = error;
        Self::with_code(code, message)
    }
}

impl From<DengjenFFIError> for ExternError {
    fn from(error: DengjenFFIError) -> Self {
        ExternError::new_error(ErrorCode::new(error.0), error.1)
    }
}

/// This crate's internal result alias, used throughout the private synthesis helpers.
pub type DengjenFFIResult<T> = Result<T, DengjenFFIError>;

#[repr(C)]
pub struct SynthesisEvent {
    event_type: i32,
    error_ptr: *mut ExternError,
    len: i64, // usize causes issues with JNI
    data: *mut u8,
}

impl SynthesisEvent {
    /// Leaks `bytes`' backing allocation into a raw `(len, pointer)` pair for a C caller to
    /// read; `libdengjenFreeSynthesisEvent` reclaims it later via the same length.
    fn leak_bytes(bytes: Vec<u8>) -> (i64, *mut u8) {
        let boxed: Box<[u8]> = bytes.into_boxed_slice();
        let len = boxed.len() as i64;
        let ptr = Box::into_raw(boxed) as *mut u8;
        (len, ptr)
    }

    fn with_speech(speech: Vec<u8>) -> Self {
        let (len, data) = Self::leak_bytes(speech);
        Self {
            event_type: synth_event::SYNTH_EVENT_SPEECH,
            error_ptr: std::ptr::null_mut(),
            len,
            data,
        }
    }

    fn with_error(error: impl Into<ExternError>) -> Self {
        let (len, data) = Self::leak_bytes(Vec::new());
        Self {
            event_type: synth_event::SYNTH_EVENT_ERROR,
            error_ptr: Box::into_raw(Box::new(error.into())),
            len,
            data,
        }
    }

    fn with_finished() -> Self {
        let (len, data) = Self::leak_bytes(Vec::new());
        Self {
            event_type: synth_event::SYNTH_EVENT_FINISHED,
            error_ptr: std::ptr::null_mut(),
            len,
            data,
        }
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
        let &Self {
            rate,
            volume,
            pitch,
            appended_silence_ms,
            ..
        } = self;
        AudioOutputConfig {
            appended_silence_ms: Some(appended_silence_ms),
            pitch: Some(pitch),
            rate: Some(rate),
            volume: Some(volume),
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
    fn as_piper_synth_config(&self) -> dengjen_tts_piper::PiperSynthesisConfig {
        let &Self {
            speaker,
            length_scale,
            noise_scale,
            noise_w,
        } = self;
        dengjen_tts_piper::PiperSynthesisConfig {
            speaker: Some(i64::from(speaker)),
            length_scale,
            noise_scale,
            noise_w,
        }
    }
}

/// Dereferences `ptr` and hands back a shared reference, treating null the way this C ABI
/// expects errors to surface: a `NULL_POINTER` `DengjenFFIError` tagged with `param_name` is
/// written into `out_error` and `None` comes back instead of triggering undefined behavior.
///
/// # Safety
/// If non-null, `ptr` must be well-aligned and point to a live, initialized `T`.
unsafe fn require_ref<'a, T>(
    ptr: *const T,
    param_name: &str,
    out_error: &mut ExternError,
) -> Option<&'a T> {
    // SAFETY: caller's obligation is this function's own `# Safety` doc.
    match unsafe { ptr.as_ref() } {
        Some(value) => Some(value),
        None => {
            *out_error = DengjenFFIError::null_pointer(param_name).into();
            None
        }
    }
}

/// Exclusive-access counterpart of [`require_ref`]; identical null handling.
///
/// # Safety
/// If non-null, `ptr` must be well-aligned, point to a live, initialized `T`, and have no other
/// live reference to it for the duration of `'a`.
unsafe fn require_mut<'a, T>(
    ptr: *mut T,
    param_name: &str,
    out_error: &mut ExternError,
) -> Option<&'a mut T> {
    // SAFETY: caller's obligation is this function's own `# Safety` doc.
    match unsafe { ptr.as_mut() } {
        Some(value) => Some(value),
        None => {
            *out_error = DengjenFFIError::null_pointer(param_name).into();
            None
        }
    }
}

/// # Safety
/// `string_ptr` must be non-null and well-aligned.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenFreeString(string_ptr: *mut i8) {
    // SAFETY: this function's own `# Safety` doc is the destructor's contract; nothing else
    // happens to `string_ptr` before the hand-off.
    unsafe { _internal_libdengjenFreeString(string_ptr) };
}

/// # Safety
/// `synth_config` must be non-null and well-aligned.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenFreePiperSynthConfig(synth_config: *mut PiperSynthConfig) {
    // SAFETY: this function's own `# Safety` doc is the destructor's contract; nothing else
    // happens to `synth_config` before the hand-off.
    unsafe { _internal_libdengjenFreePiperSynthConfig(synth_config) };
}

/// # Safety
/// `event` must be a value previously returned by a `SpeechSynthesisCallback` invocation (i.e.
/// built by `SynthesisEvent::with_speech`/`with_error`/`with_finished`), and must be passed to
/// this function at most once.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenFreeSynthesisEvent(event: SynthesisEvent) {
    // A panic unwinding across this extern "C" boundary is undefined behavior; abort instead.
    ffi_support::abort_on_panic::with_abort_on_panic(|| {
        let SynthesisEvent {
            error_ptr,
            data,
            len,
            ..
        } = event;
        // SAFETY: every `SynthesisEvent` constructor builds `data`/`len` via `leak_bytes`, which
        // leaks a `Box<[u8]>` of exactly `len` bytes through `Box::into_raw`. Rebuilding the fat
        // pointer with `slice_from_raw_parts_mut` and handing it straight to `Box::from_raw`
        // reclaims that allocation without first materializing a reference to memory this
        // function doesn't own yet — the caller's one-and-only-free obligation is this
        // function's `# Safety` doc.
        drop(unsafe { Box::from_raw(std::ptr::slice_from_raw_parts_mut(data, len as usize)) });
        if !error_ptr.is_null() {
            // SAFETY: only `with_error` sets a non-null `error_ptr`, via
            // `Box::into_raw(Box::new(..))`; this is that box's one and only reclaim.
            drop(unsafe { Box::from_raw(error_ptr) });
        }
    });
}

#[no_mangle]
#[allow(non_snake_case)]
pub extern "C" fn libdengjenLoadVoiceFromConfigPath(
    config_path_ptr: FfiStr,
    out_error: &mut ExternError,
) -> *mut DengjenVoice {
    let load_from_config = move || _load_voice(config_path_ptr);
    call_with_result(out_error, load_from_config)
}

/// # Safety
/// `voice_ptr` must be non-null and well-aligned.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenUnloadDengjenVoice(voice_ptr: *mut DengjenVoice) {
    // SAFETY: this function's own `# Safety` doc is the destructor's contract; nothing else
    // happens to `voice_ptr` before the hand-off.
    unsafe { _internal_libdengjenUnloadDengjenVoice(voice_ptr) };
}

/// # Safety
/// If non-null, `voice_ptr` and `audio_info_ptr` must each be well-aligned and point to a valid
/// `DengjenVoice`/`AudioInfo`. Either being null is handled gracefully: a NULL_POINTER error is
/// written to `out_error` and `audio_info_ptr` is left untouched.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenGetAudioInfo(
    voice_ptr: *mut DengjenVoice,
    audio_info_ptr: *mut AudioInfo,
    out_error: &mut ExternError,
) {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return;
    };
    // SAFETY: `audio_info_ptr` carries this function's own non-null-or-valid contract.
    let Some(audio_info) = (unsafe { require_mut(audio_info_ptr, "audio_info_ptr", out_error) })
    else {
        return;
    };
    let mut out = AssertUnwindSafe(audio_info);
    call_with_result(out_error, move || match voice.audio_output_info() {
        Ok(info) => {
            out.sample_rate = info.sample_rate as u32;
            out.num_channels = info.num_channels as u32;
            out.sample_width = info.sample_width as u32;
            Ok(())
        }
        Err(e) => Err(DengjenFFIError::from(e)),
    })
}

/// # Safety
/// If non-null, `voice_ptr` must be well-aligned and point to a valid `DengjenVoice`. Passing
/// null is handled gracefully: a NULL_POINTER error is written to `out_error` and a null pointer
/// is returned.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenGetPiperDefaultSynthConfig(
    voice_ptr: *mut DengjenVoice,
    out_error: &mut ExternError,
) -> *mut PiperSynthConfig {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return std::ptr::null_mut();
    };
    call_with_result(out_error, move || {
        let config = voice
            .get_default_synthesis_config()
            .map_err(DengjenFFIError::from)?
            .ok_or_else(|| {
                DengjenFFIError::with_code(
                    error_codes::INVALID_CONFIGURATION,
                    "voice has no default Piper synthesis config to return",
                )
            })?;
        let piper_config = dengjen_tts_piper::PiperSynthesisConfig::from(&config);
        Ok::<_, DengjenFFIError>(PiperSynthConfig {
            speaker: piper_config.speaker.map_or(0, |sid| sid as u32),
            length_scale: piper_config.length_scale,
            noise_scale: piper_config.noise_scale,
            noise_w: piper_config.noise_w,
        })
    })
}

/// # Safety
/// If non-null, `voice_ptr` must be well-aligned and point to a valid `DengjenVoice`. Passing
/// null is handled gracefully: a NULL_POINTER error is written to `out_error`.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSetPiperSynthConfig(
    voice_ptr: *mut DengjenVoice,
    synth_config: PiperSynthConfig,
    out_error: &mut ExternError,
) {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return;
    };
    let new_config = dengjen_tts_core::SynthesisConfig::from(&synth_config.as_piper_synth_config());
    call_with_result(out_error, move || {
        voice
            .set_fallback_synthesis_config(&new_config)
            .map_err(DengjenFFIError::from)
    })
}

/// Sets a single named entry in the voice's generic synthesis `parameters` map. This is an
/// additive escape hatch alongside the named-field setters (e.g. [`libdengjenSetPiperSynthConfig`]):
/// `key` may be any string, but whether it has any effect depends on the loaded backend — a
/// backend that doesn't recognize `key` silently ignores it. Piper only recognizes
/// `length_scale`, `noise_scale`, and `noise_w`; Kokoro ignores all three (it has no tunable
/// synthesis parameters).
///
/// # Safety
/// If non-null, `voice_ptr` must be well-aligned and point to a valid `DengjenVoice`. `key_ptr`
/// must be a valid, NUL-terminated UTF-8 C string for the duration of this call. Passing a null
/// `voice_ptr` is handled gracefully: a NULL_POINTER error is written to `out_error`.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSetSynthesisParameter(
    voice_ptr: *mut DengjenVoice,
    key_ptr: FfiStr,
    value: f32,
    out_error: &mut ExternError,
) {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return;
    };
    call_with_result(out_error, move || {
        let Some(key) = key_ptr.into_opt_string() else {
            return Err(DengjenFFIError::invalid_utf8());
        };
        let mut config = voice
            .get_fallback_synthesis_config()
            .map_err(DengjenFFIError::from)?
            .unwrap_or_default();
        config.parameters.insert(key, value);
        voice
            .set_fallback_synthesis_config(&config)
            .map_err(DengjenFFIError::from)
    })
}

/// Reads a single named entry from the voice's generic synthesis `parameters` map — the
/// read-side counterpart to [`libdengjenSetSynthesisParameter`]. Returns `true` and writes
/// `*out_value_ptr` if `key` is present in the voice's current generic parameters map; returns
/// `false` (not an error) if `key` is absent, or if the loaded backend has no synthesis config
/// at all (e.g. Kokoro).
///
/// # Safety
/// If non-null, `voice_ptr` and `out_value_ptr` must each be well-aligned and point to a valid
/// `DengjenVoice`/`f32`. `key_ptr` must be a valid, NUL-terminated UTF-8 C string for the
/// duration of this call. Either `voice_ptr` or `out_value_ptr` being null is handled
/// gracefully: a NULL_POINTER error is written to `out_error`, `false` is returned, and
/// `out_value_ptr` (if non-null) is left untouched.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenGetSynthesisParameter(
    voice_ptr: *mut DengjenVoice,
    key_ptr: FfiStr,
    out_value_ptr: *mut f32,
    out_error: &mut ExternError,
) -> bool {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return false;
    };
    // SAFETY: `out_value_ptr` carries this function's own non-null-or-valid contract.
    let Some(out_value) = (unsafe { require_mut(out_value_ptr, "out_value_ptr", out_error) })
    else {
        return false;
    };
    let mut out_value = AssertUnwindSafe(out_value);
    (call_with_result(out_error, move || {
        let Some(key) = key_ptr.into_opt_string() else {
            return Err(DengjenFFIError::invalid_utf8());
        };
        let config = voice
            .get_fallback_synthesis_config()
            .map_err(DengjenFFIError::from)?
            .unwrap_or_default();
        match config.parameters.get(&key) {
            Some(value) => {
                **out_value = *value;
                Ok::<_, DengjenFFIError>(true)
            }
            None => Ok(false),
        }
    }) as u8)
        != 0
}

/// # Safety
/// If non-null, `voice_ptr` must be well-aligned and point to a valid `DengjenVoice`. Passing
/// null is handled gracefully: a NULL_POINTER error is written to `out_error`.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSpeak(
    voice_ptr: *mut DengjenVoice,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_error: &mut ExternError,
) {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return;
    };
    let owned_synth = AssertUnwindSafe(Arc::clone(&voice.synth));
    let owned_cancel_slot = Arc::clone(&voice.active_cancel_token);
    call_with_result(out_error, move || {
        _synthesize(owned_synth, owned_cancel_slot, text_ptr, params)
    })
}

/// # Safety
/// If non-null, `voice_ptr` must be well-aligned and point to a valid `DengjenVoice`. Passing
/// null is handled gracefully: a NULL_POINTER error is written to `out_error`.
///
/// This is meant to be called from a different thread than the one running `libdengjenSpeak` —
/// cross-thread cancellation is the entire purpose. Because of that, the caller must guarantee
/// the same voice is not torn down via `libdengjenUnloadDengjenVoice` while, or immediately
/// after, this call is in flight; racing the two is a use-after-free only the caller can avoid.
///
/// # Behaviour
/// - Has no effect unless a realtime-mode synthesis is currently running on this voice; lazy
///   and parallel syntheses cannot be interrupted this way.
/// - Only the most recently started realtime synthesis on this voice is reachable. If
///   nonblocking mode started two realtime syntheses concurrently, the earlier one cannot be
///   cancelled from here.
/// - The streaming callback still receives `SYNTH_EVENT_FINISHED` after a successful
///   cancellation — the callback sequence alone can't tell a cancelled stream from one that ran
///   to completion.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenCancel(
    voice_ptr: *mut DengjenVoice,
    out_error: &mut ExternError,
) {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return;
    };
    call_with_result(out_error, || _cancel(&voice.active_cancel_token))
}

/// # Safety
/// If non-null, `voice_ptr` must be well-aligned and point to a valid `DengjenVoice`. Passing
/// null is handled gracefully: a NULL_POINTER error is written to `out_error` and `0` is
/// returned.
#[no_mangle]
#[allow(non_snake_case)]
pub unsafe extern "C" fn libdengjenSpeakToFile(
    voice_ptr: *mut DengjenVoice,
    text_ptr: FfiStr,
    params: SynthesisParams,
    out_filename_ptr: FfiStr,
    out_error: &mut ExternError,
) -> u8 {
    // SAFETY: `voice_ptr` carries this function's own non-null-or-valid contract.
    let Some(voice) = (unsafe { require_ref(voice_ptr, "voice_ptr", out_error) }) else {
        return 0;
    };
    let owned_synth = AssertUnwindSafe(Arc::clone(&voice.synth));
    call_with_result(out_error, move || {
        let wrote_file = _synthesize_to_file(owned_synth, text_ptr, params, out_filename_ptr);
        Ok::<u8, DengjenFFIError>(u8::from(wrote_file.is_ok()))
    })
}

/// Builds and commits the onnxruntime environment. Runs at most once per process
/// (`INIT_ORT_ENVIRONMENT`); a later call after a successful commit is a no-op.
fn init_ort_environment() {
    INIT_ORT_ENVIRONMENT.call_once(|| {
        // CPU always goes last: it's the universal fallback once every platform-specific
        // provider ahead of it has had a chance to claim the workload.
        #[cfg(target_os = "android")]
        let execution_providers = vec![
            ort::execution_providers::NNAPI::default().build(),
            ort::execution_providers::CPU::default().build(),
        ];
        #[cfg(target_os = "ios")]
        let execution_providers = vec![
            ort::execution_providers::CoreML::default().build(),
            ort::execution_providers::CPU::default().build(),
        ];
        #[cfg(not(any(target_os = "android", target_os = "ios")))]
        let execution_providers = vec![ort::execution_providers::CPU::default().build()];

        let committed = ort::init()
            .with_name("dengjen")
            .with_execution_providers(execution_providers)
            .commit();
        // Startup can't recover from this: without a committed environment no model can load.
        assert!(committed, "Failed to initialize onnxruntime");
    });
}

fn load_voice(config_path: &std::path::Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>> {
    let model_type = dengjen_tts::detect_model_type(config_path)?;
    if model_type == "kokoro" {
        return dengjen_tts_kokoro::from_config_path(config_path);
    }
    dengjen_tts_piper::from_config_path(config_path)
}

fn _load_voice(config_path_ptr: FfiStr) -> DengjenFFIResult<DengjenVoice> {
    init_ort_environment();
    let Some(config_path) = config_path_ptr.into_opt_string() else {
        return Err(DengjenFFIError::invalid_utf8());
    };
    let model = load_voice(&PathBuf::from(config_path))?;
    let synth = DengjenSpeechSynthesizer::new(model)?;
    Ok(synth.into())
}

fn _cancel(cancel_slot: &Arc<Mutex<Option<CancellationToken>>>) -> DengjenFFIResult<()> {
    let held_token = cancel_slot.lock().unwrap();
    if let Some(active_token) = held_token.as_ref() {
        active_token.cancel();
    }
    Ok(())
}

/// RAII: on drop, resets `slot` to `None` — but only when it still holds the exact token this
/// guard was built with. Two nonblocking realtime speaks on the same voice can overlap, and a
/// finishing guard whose token has already been superseded by a still-running sibling must
/// leave that sibling's slot alone. Running on every `Drop` (not just the happy path) also
/// covers the early return through `?` on a synthesis error.
struct CancelSlotGuard {
    slot: Arc<Mutex<Option<CancellationToken>>>,
    token: CancellationToken,
}

impl Drop for CancelSlotGuard {
    fn drop(&mut self) {
        let mut held_token = self.slot.lock().unwrap();
        let still_owns_slot = held_token
            .as_ref()
            .is_some_and(|current| current.points_to_same_flag(&self.token));
        if still_owns_slot {
            *held_token = None;
        }
    }
}

fn _synthesize(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    cancel_slot: Arc<Mutex<Option<CancellationToken>>>,
    text_ptr: FfiStr,
    params: SynthesisParams,
) -> DengjenFFIResult<()> {
    let Some(text) = text_ptr.into_opt_string() else {
        return Err(DengjenFFIError::invalid_utf8());
    };
    if params.nonblocking == 0 {
        return _do_synthesize(synth, cancel_slot, text, params);
    }
    // Control has already returned to the caller by the time this runs on the pool thread, so
    // a synthesis error has nowhere to go but through the callback the caller is still holding.
    let report_to_caller = params.callback;
    SYNTHESIS_THREAD_POOL.spawn(move || {
        if let Err(error) = _do_synthesize(synth, cancel_slot, text, params) {
            report_to_caller(SynthesisEvent::with_error(error));
        }
    });
    Ok(())
}

fn _do_synthesize(
    synth: AssertUnwindSafe<Arc<DengjenSpeechSynthesizer>>,
    cancel_slot: Arc<Mutex<Option<CancellationToken>>>,
    text: String,
    params: SynthesisParams,
) -> DengjenFFIResult<()> {
    // Tuned defaults for realtime chunking, not placeholders — do not change.
    const REALTIME_CHUNK_SIZE: usize = 72;
    const REALTIME_CHUNK_PADDING: usize = 3;

    let output_config = Some(params.as_synth_output_config());
    let callback = params.callback;
    match params.mode {
        synth_mode::SYNTH_MODE_LAZY => {
            let stream = synth
                .synthesize_lazy(text, output_config)?
                .map(|item| item.map(|audio| audio.samples));
            iterate_stream(stream, callback)
        }
        synth_mode::SYNTH_MODE_PARALLEL => {
            let stream = synth
                .synthesize_parallel(text, output_config)?
                .map(|item| item.map(|audio| audio.samples));
            iterate_stream(stream, callback)
        }
        synth_mode::SYNTH_MODE_REALTIME => {
            let cancel_token = CancellationToken::new();
            *cancel_slot.lock().unwrap() = Some(cancel_token.clone());
            let _release_slot_on_drop = CancelSlotGuard {
                slot: cancel_slot,
                token: cancel_token.clone(),
            };
            let stream = synth.synthesize_streamed(
                text,
                output_config,
                REALTIME_CHUNK_SIZE,
                REALTIME_CHUNK_PADDING,
                cancel_token,
            )?;
            iterate_stream(stream, callback)
        }
        _ => Err(DengjenFFIError::invalid_synthesis_mode()),
    }
}

#[inline(always)]
fn iterate_stream(
    stream: impl Iterator<Item = DengjenResult<AudioSamples>> + Send + Sync + 'static,
    callback: SpeechSynthesisCallback,
) -> DengjenFFIResult<()> {
    for item in stream {
        let audio = match item {
            Ok(audio) => audio,
            Err(error) => {
                callback(SynthesisEvent::with_error(DengjenFFIError::from(error)));
                return Ok(());
            }
        };
        let caller_wants_more = callback(SynthesisEvent::with_speech(audio.as_wave_bytes())) == 0;
        if !caller_wants_more {
            return Ok(());
        }
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
    let (Some(text), Some(out_filename)) = (
        text_ptr.into_opt_string(),
        out_filename_ptr.into_opt_string(),
    ) else {
        return Err(DengjenFFIError::invalid_utf8());
    };
    synth
        .synthesize_to_file(
            &PathBuf::from(out_filename),
            text,
            Some(params.as_synth_output_config()),
        )
        .map_err(DengjenFFIError::from)
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
        let mut audio_info = AudioInfo {
            sample_rate: 0,
            num_channels: 0,
            sample_width: 0,
        };
        unsafe {
            libdengjenGetAudioInfo(std::ptr::null_mut(), &mut audio_info, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
        // SAFETY: `out_error`'s message is never freed by `Drop` per ffi_support's documented
        // contract (real FFI consumers free it via `libdengjenFreeString`); release it here so
        // the test doesn't leak.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn get_piper_default_synth_config_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let result =
            unsafe { libdengjenGetPiperDefaultSynthConfig(std::ptr::null_mut(), &mut out_error) };
        assert!(result.is_null());
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn set_piper_synth_config_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let synth_config = PiperSynthConfig {
            speaker: 0,
            length_scale: 1.0,
            noise_scale: 1.0,
            noise_w: 1.0,
        };
        unsafe {
            libdengjenSetPiperSynthConfig(std::ptr::null_mut(), synth_config, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn set_synthesis_parameter_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let key = FfiStr::from_cstr(std::ffi::CStr::from_bytes_with_nul(b"noise_scale\0").unwrap());
        unsafe {
            libdengjenSetSynthesisParameter(std::ptr::null_mut(), key, 0.5, &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn load_voice_errors_on_a_missing_config_path() {
        let path = std::path::Path::new("/nonexistent-dengjen-capi-load-voice-test.json");
        assert!(load_voice(path).is_err());
    }

    fn write_temp_config(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, contents).unwrap();
        path
    }

    #[test]
    fn load_voice_routes_kokoro_model_type_toward_the_kokoro_loader() {
        let dir = std::env::temp_dir().join("dengjen_capi_load_voice_test_kokoro");
        std::fs::create_dir_all(&dir).unwrap();
        // A syntactically valid but incomplete Kokoro config: detect_model_type reads it fine,
        // but dengjen_tts_kokoro::from_config_path's own RawKokoroVoiceConfig requires
        // `model_path` (crates/dengjen/models/kokoro/src/config.rs:8), which this JSON omits.
        // If this had instead fallen through to Piper's loader, the error would name a
        // Piper-required field (`audio`) instead — so asserting on `model_path` specifically
        // proves the Kokoro branch was actually taken, not just that some error occurred.
        let path = write_temp_config(&dir, "config.json", r#"{"model_type": "kokoro"}"#);
        let err = match load_voice(&path) {
            Err(e) => format!("{}", e),
            Ok(_) => panic!("expected an error for an incomplete Kokoro config"),
        };
        assert!(
            err.contains("model_path"),
            "expected a Kokoro-loader error naming the missing `model_path` field, got: {err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_voice_routes_vits_model_type_toward_the_piper_loader() {
        let dir = std::env::temp_dir().join("dengjen_capi_load_voice_test_vits");
        std::fs::create_dir_all(&dir).unwrap();
        // A syntactically valid but incomplete Piper/VITS config, missing Piper's required
        // `audio` field. Asserting on `audio` (rather than just "some error occurred") proves
        // the Piper branch was taken, not the Kokoro one.
        let path = write_temp_config(&dir, "config.json", r#"{"model_type": "vits"}"#);
        let err = match load_voice(&path) {
            Err(e) => format!("{}", e),
            Ok(_) => panic!("expected an error for an incomplete VITS config"),
        };
        assert!(
            err.contains("audio"),
            "expected a Piper-loader error naming the missing `audio` field, got: {err}"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    /// Minimal stand-in for `DengjenModel` so a real `DengjenVoice` can be built without an
    /// ONNX-backed model — this crate has no such fixture (nothing here loads real Piper/Kokoro
    /// model files), so this is the only way to reach `libdengjenSetSynthesisParameter`'s
    /// post-null-voice-check code path (the key conversion) with a non-null `voice_ptr`.
    struct FakeModel {
        fallback_config: Mutex<Option<dengjen_tts_core::SynthesisConfig>>,
    }

    impl DengjenModel for FakeModel {
        fn audio_output_info(&self) -> DengjenResult<dengjen_tts_core::AudioInfo> {
            Ok(dengjen_tts_core::AudioInfo {
                sample_rate: 16000,
                num_channels: 1,
                sample_width: 2,
            })
        }
        fn phonemize_text(&self, _text: &str) -> DengjenResult<dengjen_tts_core::Phonemes> {
            Ok(dengjen_tts_core::Phonemes::from(Vec::<String>::new()))
        }
        fn speak_batch(
            &self,
            _phoneme_batches: Vec<String>,
        ) -> DengjenResult<Vec<dengjen_tts_core::Audio>> {
            Ok(Vec::new())
        }
        fn speak_one_sentence(&self, _phonemes: String) -> dengjen_tts_core::DengjenAudioResult {
            Ok(dengjen_tts_core::Audio::new(
                AudioSamples::from(Vec::new()),
                16000,
                None,
            ))
        }
        fn get_default_synthesis_config(
            &self,
        ) -> DengjenResult<Option<dengjen_tts_core::SynthesisConfig>> {
            Ok(None)
        }
        fn get_fallback_synthesis_config(
            &self,
        ) -> DengjenResult<Option<dengjen_tts_core::SynthesisConfig>> {
            Ok(self.fallback_config.lock().unwrap().clone())
        }
        fn set_fallback_synthesis_config(
            &self,
            synthesis_config: &dengjen_tts_core::SynthesisConfig,
        ) -> DengjenResult<()> {
            *self.fallback_config.lock().unwrap() = Some(synthesis_config.clone());
            Ok(())
        }
    }

    fn fake_voice() -> DengjenVoice {
        let model: Arc<dyn DengjenModel + Send + Sync> = Arc::new(FakeModel {
            fallback_config: Mutex::new(None),
        });
        DengjenVoice::from(DengjenSpeechSynthesizer::new(model).unwrap())
    }

    #[test]
    fn set_synthesis_parameter_null_key_returns_invalid_utf8_error_without_panicking() {
        let mut voice = fake_voice();
        let mut out_error = ExternError::default();
        // SAFETY: constructing a null `FfiStr` is the whole point of this test — no C caller is
        // dereferencing it, `libdengjenSetSynthesisParameter` must reject it gracefully instead.
        let null_key = unsafe { FfiStr::from_raw(std::ptr::null()) };
        unsafe {
            libdengjenSetSynthesisParameter(&mut voice, null_key, 0.5, &mut out_error);
        }
        assert_eq!(
            out_error.get_code().code(),
            error_codes::INVALID_UTF8_SEQUENCE
        );
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn get_synthesis_parameter_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        let key = FfiStr::from_cstr(std::ffi::CStr::from_bytes_with_nul(b"custom_knob\0").unwrap());
        let mut value: f32 = 0.0;
        let found = unsafe {
            libdengjenGetSynthesisParameter(std::ptr::null_mut(), key, &mut value, &mut out_error)
        };
        assert!(!found);
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn get_synthesis_parameter_returns_false_for_a_key_that_was_never_set() {
        let mut voice = fake_voice();
        let mut out_error = ExternError::default();
        let key = FfiStr::from_cstr(std::ffi::CStr::from_bytes_with_nul(b"custom_knob\0").unwrap());
        let mut value: f32 = 0.0;
        let found =
            unsafe { libdengjenGetSynthesisParameter(&mut voice, key, &mut value, &mut out_error) };
        assert!(!found);
        assert!(out_error.get_code().is_success());
    }

    #[test]
    fn get_synthesis_parameter_round_trips_a_value_set_via_set_synthesis_parameter() {
        let mut voice = fake_voice();
        let mut out_error = ExternError::default();
        let key = FfiStr::from_cstr(std::ffi::CStr::from_bytes_with_nul(b"custom_knob\0").unwrap());
        unsafe {
            libdengjenSetSynthesisParameter(&mut voice, key, 1.25, &mut out_error);
        }
        assert!(out_error.get_code().is_success());

        let mut value: f32 = 0.0;
        let key2 =
            FfiStr::from_cstr(std::ffi::CStr::from_bytes_with_nul(b"custom_knob\0").unwrap());
        let found = unsafe {
            libdengjenGetSynthesisParameter(&mut voice, key2, &mut value, &mut out_error)
        };
        assert!(found);
        assert!(out_error.get_code().is_success());
        assert_eq!(value, 1.25);
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
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
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
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn cancel_null_voice_returns_null_pointer_error_without_panicking() {
        let mut out_error = ExternError::default();
        unsafe {
            libdengjenCancel(std::ptr::null_mut(), &mut out_error);
        }
        assert_eq!(out_error.get_code().code(), error_codes::NULL_POINTER);
        // SAFETY: see `get_audio_info_null_voice_returns_null_pointer_error_without_panicking`.
        unsafe { out_error.manually_release() };
    }

    #[test]
    fn cancel_cancels_the_token_held_in_the_slot() {
        let token = CancellationToken::new();
        let slot = Arc::new(Mutex::new(Some(token.clone())));

        assert!(_cancel(&slot).is_ok());

        assert!(token.is_cancelled());
    }

    #[test]
    fn cancel_on_an_empty_slot_is_a_noop() {
        let slot: Arc<Mutex<Option<CancellationToken>>> = Arc::new(Mutex::new(None));

        assert!(_cancel(&slot).is_ok());

        assert!(slot.lock().unwrap().is_none());
    }

    #[test]
    fn cancel_slot_guard_clears_the_slot_when_it_still_holds_its_own_token() {
        let slot = Arc::new(Mutex::new(None));
        let token = CancellationToken::new();
        *slot.lock().unwrap() = Some(token.clone());

        drop(CancelSlotGuard {
            slot: Arc::clone(&slot),
            token,
        });

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
        let guard_a = CancelSlotGuard {
            slot: Arc::clone(&slot),
            token: token_a,
        };

        *slot.lock().unwrap() = Some(token_b.clone());
        drop(guard_a);

        let held = slot.lock().unwrap();
        assert!(held.as_ref().unwrap().points_to_same_flag(&token_b));
    }

    #[test]
    fn error_codes_round_trip_through_dengjen_ffi_error() {
        let cases = [
            (
                DengjenError::FailedToLoadResource("x".into()),
                error_codes::FAILED_TO_LOAD_RESOURCE,
            ),
            (
                DengjenError::PhonemizationError("x".into()),
                error_codes::PHONEMIZATION_ERROR,
            ),
            (
                DengjenError::InferenceError("x".into()),
                error_codes::INFERENCE_ERROR,
            ),
            (
                DengjenError::InvalidConfiguration("x".into()),
                error_codes::INVALID_CONFIGURATION,
            ),
            (
                DengjenError::UnsupportedOperation("x".into()),
                error_codes::UNSUPPORTED_OPERATION,
            ),
            (
                DengjenError::OperationError("x".into()),
                error_codes::OPERATION_ERROR,
            ),
        ];
        for (err, expected_code) in cases {
            let ffi_err: DengjenFFIError = err.into();
            assert_eq!(ffi_err.0, expected_code);
        }
    }
}

#[cfg(test)]
mod abi_struct_tests {
    use super::*;

    #[test]
    fn synthesis_event_with_speech_carries_the_pcm_bytes_and_a_null_error_pointer() {
        let event = SynthesisEvent::with_speech(vec![1, 2, 3, 4]);

        assert_eq!(event.event_type, synth_event::SYNTH_EVENT_SPEECH);
        assert!(event.error_ptr.is_null());
        assert_eq!(event.len, 4);
        // SAFETY: `event.data`/`event.len` were just produced by `with_speech` from a
        // 4-byte `Vec`, so the pointer is valid for exactly that many reads.
        let bytes = unsafe { std::slice::from_raw_parts(event.data, event.len as usize) };
        assert_eq!(bytes, &[1, 2, 3, 4]);

        // SAFETY: `event` came from `with_speech`, one of the constructors
        // `libdengjenFreeSynthesisEvent`'s `# Safety` doc covers; `error_ptr` is
        // null on this path so there's no message to leak.
        unsafe { libdengjenFreeSynthesisEvent(event) };
    }

    #[test]
    fn synthesis_event_with_error_carries_a_non_null_error_pointer_and_empty_data() {
        let event = SynthesisEvent::with_error(DengjenFFIError::invalid_utf8());

        assert_eq!(event.event_type, synth_event::SYNTH_EVENT_ERROR);
        assert!(!event.error_ptr.is_null());
        assert_eq!(event.len, 0);

        // SAFETY: `error_ptr` was produced by `Box::into_raw` inside `with_error`
        // and hasn't been freed yet; reclaiming ownership here lets us call
        // `manually_release()` (it consumes `self`) on the message before the box
        // itself is dropped. `libdengjenFreeSynthesisEvent` does NOT do this — a
        // pre-existing gap in the shipped function that leaks the message on every
        // real synthesis-error event, out of scope to fix in this rewrite (see the
        // plan's Global Constraints) — so calling that function directly here
        // would leak the message under this crate's ASan CI gate.
        let boxed_error = unsafe { Box::from_raw(event.error_ptr) };
        assert_eq!(
            boxed_error.get_code().code(),
            error_codes::INVALID_UTF8_SEQUENCE
        );
        unsafe { boxed_error.manually_release() };

        // SAFETY: `event.data`/`event.len` were produced by `leak_bytes(Vec::new())` inside
        // `with_error` via `Box::into_raw` and haven't been freed yet.
        unsafe {
            let s = std::slice::from_raw_parts_mut(event.data, event.len as usize);
            drop(Box::from_raw(s as *mut [u8]));
        }
    }

    #[test]
    fn synthesis_event_with_finished_carries_a_null_error_pointer_and_empty_data() {
        let event = SynthesisEvent::with_finished();

        assert_eq!(event.event_type, synth_event::SYNTH_EVENT_FINISHED);
        assert!(event.error_ptr.is_null());
        assert_eq!(event.len, 0);

        // SAFETY: see `synthesis_event_with_speech_...` above — same shape, no error to leak.
        unsafe { libdengjenFreeSynthesisEvent(event) };
    }

    extern "C" fn noop_callback(_event: SynthesisEvent) -> u8 {
        0
    }

    #[test]
    fn as_synth_output_config_carries_over_all_fields() {
        let params = SynthesisParams {
            mode: synth_mode::SYNTH_MODE_LAZY,
            rate: 60,
            volume: 80,
            pitch: 40,
            appended_silence_ms: 250,
            callback: noop_callback,
            nonblocking: 0,
        };

        let config = params.as_synth_output_config();

        assert_eq!(config.rate, Some(60));
        assert_eq!(config.volume, Some(80));
        assert_eq!(config.pitch, Some(40));
        assert_eq!(config.appended_silence_ms, Some(250));
    }

    #[test]
    fn as_piper_synth_config_carries_over_speaker_and_synthesis_tuning_fields() {
        let synth_config = PiperSynthConfig {
            speaker: 3,
            length_scale: 1.2,
            noise_scale: 0.5,
            noise_w: 0.9,
        };

        let piper_config = synth_config.as_piper_synth_config();

        assert_eq!(piper_config.speaker, Some(3));
        assert_eq!(piper_config.length_scale, 1.2);
        assert_eq!(piper_config.noise_scale, 0.5);
        assert_eq!(piper_config.noise_w, 0.9);
    }
}

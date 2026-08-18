//! Raw FFI declarations for the espeak-ng C library.
//!
//! This module hand-declares only the three `espeak_*` entry points this
//! crate actually calls, plus the constants those calls need. Every name,
//! type and value here is checked against the vendored C header
//! (`deps/espeak-ng/src/include/espeak/speak_lib.h`) rather than chosen for
//! Rust style, so the C naming convention (`espeak_ERROR`, `espeakCHARS_UTF8`,
//! ...) is kept on purpose: it lets a reader diff this file against the
//! header directly instead of translating names in their head.
#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::{c_char, c_int};

/// Mirrors the C `espeak_ERROR` enum; only the success case is used here.
pub type espeak_ERROR = c_int;
pub const espeak_ERROR_EE_OK: espeak_ERROR = 0;

/// Mirrors the C `espeak_AUDIO_OUTPUT` enum; only the retrieval mode is used.
pub type espeak_AUDIO_OUTPUT = c_int;
pub const espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL: espeak_AUDIO_OUTPUT = 1;

/// `espeak_Initialize` option bit: keep the process alive after init instead
/// of calling `exit()` on failure.
pub const espeakINITIALIZE_DONT_EXIT: u32 = 0x8000;
/// `espeak_Initialize` option bit: report phonemes using IPA names rather
/// than espeak-ng's own phoneme mnemonics.
pub const espeakINITIALIZE_PHONEME_IPA: u32 = 0x0002;
/// `espeak_TextToPhonemesWithTerminator` text-mode flag: the input text is
/// UTF-8 encoded.
pub const espeakCHARS_UTF8: u32 = 1;

extern "C" {
    /// Selects the voice used by later calls, by espeak-ng voice name
    /// (e.g. `"en"`). Must be called before `espeak_TextToPhonemesWithTerminator`.
    pub fn espeak_SetVoiceByName(name: *const c_char) -> espeak_ERROR;

    /// Starts up the espeak-ng engine. `path` points at the directory
    /// containing `espeak-ng-data`; `output` selects how synthesized audio
    /// is returned (this crate always passes the retrieval mode, since it
    /// never plays audio itself, only extracts phonemes).
    pub fn espeak_Initialize(
        output: espeak_AUDIO_OUTPUT,
        buflength: c_int,
        path: *const c_char,
        options: c_int,
    ) -> c_int;

    /// Converts the next clause of `*textptr` (up to a sentence-ending or
    /// clause-breaking punctuation mark) into phonemes, advancing `*textptr`
    /// past the consumed text and writing the clause's terminator code
    /// through `terminator`. Returns a pointer to the phoneme string, owned
    /// by espeak-ng and valid only until the next call.
    pub fn espeak_TextToPhonemesWithTerminator(
        textptr: *mut *const c_char,
        textmode: c_int,
        phonememode: c_int,
        terminator: *mut c_int,
    ) -> *const c_char;
}

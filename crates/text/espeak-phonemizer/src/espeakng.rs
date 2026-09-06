#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::os::raw::{c_char, c_int};

use dengjen_espeak_rs_sys as _;

pub type espeak_ERROR = c_int;
pub const espeak_ERROR_EE_OK: espeak_ERROR = 0;

pub type espeak_AUDIO_OUTPUT = c_int;
pub const espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL: espeak_AUDIO_OUTPUT = 1;

pub const espeakINITIALIZE_DONT_EXIT: u32 = 0x8000;

pub const espeakINITIALIZE_PHONEME_IPA: u32 = 0x0002;
pub const espeakCHARS_UTF8: u32 = 1;

extern "C" {

    pub fn espeak_SetVoiceByName(name: *const c_char) -> espeak_ERROR;

    pub fn espeak_Initialize(
        output: espeak_AUDIO_OUTPUT,
        buflength: c_int,
        path: *const c_char,
        options: c_int,
    ) -> c_int;

    pub fn espeak_TextToPhonemesWithTerminator(
        textptr: *mut *const c_char,
        textmode: c_int,
        phonememode: c_int,
        terminator: *mut c_int,
    ) -> *const c_char;
}

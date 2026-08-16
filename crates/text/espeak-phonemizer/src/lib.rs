mod espeakng;

use ffi_support::{rust_string_to_c, FfiStr};
use once_cell::sync::Lazy;
use regex::Regex;
use std::env;
use std::error::Error;
use std::ffi;
use std::ffi::CString;
use std::fmt;
use std::path::PathBuf;
use std::sync::Mutex;

pub type ESpeakResult<T> = Result<T, ESpeakError>;

const CLAUSE_INTONATION_FULL_STOP: i32 = 0x00000000;
const CLAUSE_INTONATION_COMMA: i32 = 0x00001000;
const CLAUSE_INTONATION_QUESTION: i32 = 0x00002000;
const CLAUSE_INTONATION_EXCLAMATION: i32 = 0x00003000;
const CLAUSE_TYPE_SENTENCE: i32 = 0x00080000;
/// Name of the environment variable that points to the directory that contains `espeak-ng-data` directory
/// only needed if `espeak-ng-data` directory is not in the expected location (i.e. eSpeak-ng is not installed system wide)
const DENGJEN_ESPEAKNG_DATA_DIRECTORY: &str = "DENGJEN_ESPEAKNG_DATA_DIRECTORY";

#[derive(Debug, Clone)]
pub struct ESpeakError(pub String);

impl Error for ESpeakError {}

impl fmt::Display for ESpeakError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "eSpeak-ng Error :{}", self.0)
    }
}

static LANG_SWITCH_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"\([^)]*\)").unwrap());
static STRESS_PATTERN: Lazy<Regex> = Lazy::new(|| Regex::new(r"[ˈˌ]").unwrap());
static ESPEAKNG_INIT: Lazy<ESpeakResult<()>> = Lazy::new(init_espeakng);
static ESPEAK_LOCK: Mutex<()> = Mutex::new(());

fn init_espeakng() -> ESpeakResult<()> {
    let data_dir = match env::var(DENGJEN_ESPEAKNG_DATA_DIRECTORY) {
        Ok(directory) => PathBuf::from(directory),
        Err(_) => env::current_exe().unwrap().parent().unwrap().to_path_buf(),
    };
    let es_data_path_ptr = if data_dir.join("espeak-ng-data").exists() {
        rust_string_to_c(data_dir.display().to_string())
    } else {
        std::ptr::null()
    };
    // SAFETY: `es_data_path_ptr` is either null or a valid, NUL-terminated pointer produced by
    // `rust_string_to_c` immediately above. This runs inside the `ESPEAKNG_INIT` `Lazy`, so
    // eSpeak-ng's global state cannot be touched by another call concurrently with this one.
    let es_sample_rate = unsafe {
        espeakng::espeak_Initialize(
            espeakng::espeak_AUDIO_OUTPUT_AUDIO_OUTPUT_RETRIEVAL,
            0,
            es_data_path_ptr,
            espeakng::espeakINITIALIZE_DONT_EXIT as i32,
        )
    };
    if es_sample_rate <= 0 {
        Err(ESpeakError(format!(
            "Failed to initialize eSpeak-ng. Try setting `{}` environment variable to the directory that contains the `espeak-ng-data` directory. Error code: `{}`",
            DENGJEN_ESPEAKNG_DATA_DIRECTORY,
            es_sample_rate
        )))
    } else {
        Ok(())
    }
}

fn clause_break_suffix(terminator: ffi::c_int) -> &'static str {
    match terminator & 0x0000F000 {
        CLAUSE_INTONATION_FULL_STOP => ".",
        CLAUSE_INTONATION_COMMA => ",",
        CLAUSE_INTONATION_QUESTION => "?",
        CLAUSE_INTONATION_EXCLAMATION => "!",
        _ => "",
    }
}

pub fn text_to_phonemes(
    text: &str,
    language: &str,
    phoneme_separator: Option<char>,
    remove_lang_switch_flags: bool,
    remove_stress: bool,
) -> ESpeakResult<Vec<String>> {
    let mut phonemes = Vec::new();
    for line in text.lines() {
        phonemes.append(&mut phonemize_line(
            line,
            language,
            phoneme_separator,
            remove_lang_switch_flags,
            remove_stress,
        )?)
    }
    Ok(phonemes)
}

fn phonemize_line(
    text: &str,
    language: &str,
    phoneme_separator: Option<char>,
    remove_lang_switch_flags: bool,
    remove_stress: bool,
) -> ESpeakResult<Vec<String>> {
    let _guard = ESPEAK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if let Err(ref e) = Lazy::force(&ESPEAKNG_INIT) {
        return Err(e.clone());
    }
    let language_c = CString::new(language)
        .map_err(|_| ESpeakError(format!("Failed to set eSpeak-ng voice to: `{}` ", language)))?;
    // SAFETY: `language_c` is a valid, NUL-terminated CString kept alive across this call.
    // eSpeak-ng is not thread-safe; the `_guard` held above serializes every call into it.
    let set_voice_res = unsafe { espeakng::espeak_SetVoiceByName(language_c.as_ptr()) };
    if set_voice_res != espeakng::espeak_ERROR_EE_OK {
        return Err(ESpeakError(format!(
            "Failed to set eSpeak-ng voice to: `{}` ",
            language
        )));
    }
    let calculated_phoneme_mode = match phoneme_separator {
        Some(c) => ((c as u32) << 8u32) | espeakng::espeakINITIALIZE_PHONEME_IPA,
        None => espeakng::espeakINITIALIZE_PHONEME_IPA,
    };
    let phoneme_mode: i32 = calculated_phoneme_mode.try_into().unwrap();
    let mut sentence_phonemes = Vec::new();
    let mut phonemes = String::new();
    let text_c = CString::new(text).map_err(|_| {
        ESpeakError(
            "Text passed to eSpeak-ng contains a NUL byte and cannot be processed".to_string(),
        )
    })?;
    let mut text_c_char = text_c.as_ptr();
    let text_c_char_ptr = std::ptr::addr_of_mut!(text_c_char);
    let mut terminator: ffi::c_int = 0;
    let terminator_ptr: *mut ffi::c_int = &mut terminator;
    while !text_c_char.is_null() {
        // SAFETY: `text_c_char_ptr` points at a valid, NUL-terminated C string owned by
        // `text_c`/`text_c_char`, which outlives this call; `terminator_ptr` is a valid `&mut
        // c_int` reinterpreted as a raw pointer. The `_guard` held for the whole function
        // serializes access to eSpeak-ng, so `res` — a pointer into eSpeak-ng's own internal
        // buffer — stays valid for `FfiStr::from_raw` until the next call under the same lock.
        let ph_str = unsafe {
            let res = espeakng::espeak_TextToPhonemesWithTerminator(
                text_c_char_ptr,
                espeakng::espeakCHARS_UTF8.try_into().unwrap(),
                phoneme_mode,
                terminator_ptr,
            );
            FfiStr::from_raw(res)
        };
        phonemes.push_str(&ph_str.into_string());
        phonemes.push_str(clause_break_suffix(terminator));
        if (terminator & CLAUSE_TYPE_SENTENCE) == CLAUSE_TYPE_SENTENCE {
            sentence_phonemes.push(std::mem::take(&mut phonemes));
        }
    }
    if !phonemes.is_empty() {
        sentence_phonemes.push(std::mem::take(&mut phonemes));
    }
    if remove_lang_switch_flags {
        sentence_phonemes = sentence_phonemes
            .into_iter()
            .map(|sent| LANG_SWITCH_PATTERN.replace_all(&sent, "").into_owned())
            .collect();
    }
    if remove_stress {
        sentence_phonemes = sentence_phonemes
            .into_iter()
            .map(|sent| STRESS_PATTERN.replace_all(&sent, "").into_owned())
            .collect();
    }
    Ok(sentence_phonemes)
}

// ==============================

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT_ALICE: &str =
        "Who are you? said the Caterpillar. Replied Alice , rather shyly, I hardly know, sir!";

    #[test]
    fn test_basic_en() -> ESpeakResult<()> {
        let text = "test";
        let expected = "tˈɛst.";
        let phonemes = text_to_phonemes(text, "en-US", None, false, false)?.join("");
        assert_eq!(phonemes, expected);
        Ok(())
    }

    #[test]
    fn test_it_splits_sentences() -> ESpeakResult<()> {
        let phonemes = text_to_phonemes(TEXT_ALICE, "en-US", None, false, false)?;
        assert_eq!(phonemes.len(), 3);
        Ok(())
    }

    #[test]
    fn test_it_adds_phoneme_separator() -> ESpeakResult<()> {
        let text = "test";
        let expected = "t_ˈɛ_s_t.";
        let phonemes = text_to_phonemes(text, "en-US", Some('_'), false, false)
            .unwrap()
            .join("");
        assert_eq!(phonemes, expected);
        Ok(())
    }

    #[test]
    fn test_it_preserves_clause_breakers() -> ESpeakResult<()> {
        let phonemes = text_to_phonemes(TEXT_ALICE, "en-US", None, false, false)?.join("");
        let clause_breakers = ['.', ',', '?', '!'];
        for c in clause_breakers {
            assert_eq!(
                phonemes.contains(c),
                true,
                "Clause breaker `{}` not preserved",
                c
            );
        }
        Ok(())
    }

    #[test]
    fn test_arabic() -> ESpeakResult<()> {
        let text = "مَرْحَبَاً بِكَ أَيُّهَا الْرَّجُلْ";
        let expected = "mˈarħabˌaː bikˌa ʔaˈiːuhˌaː alrrˈadʒul.";
        let phonemes = text_to_phonemes(text, "ar", None, false, false)?.join("");
        assert_eq!(phonemes, expected);
        Ok(())
    }

    #[test]
    fn test_lang_switch_flags() -> ESpeakResult<()> {
        let text = "Hello معناها مرحباً";

        let with_lang_switch = text_to_phonemes(text, "ar", None, false, false)?.join("");
        assert_eq!(with_lang_switch.contains("(en)"), true);
        assert_eq!(with_lang_switch.contains("(ar)"), true);

        let without_lang_switch = text_to_phonemes(text, "ar", None, true, false)?.join("");
        assert_eq!(without_lang_switch.contains("(en)"), false);
        assert_eq!(without_lang_switch.contains("(ar)"), false);

        Ok(())
    }

    #[test]
    fn test_stress() -> ESpeakResult<()> {
        let stress_markers = ['ˈ', 'ˌ'];

        let with_stress = text_to_phonemes(TEXT_ALICE, "en-US", None, false, false)?.join("");
        assert_eq!(with_stress.contains(stress_markers), true);

        let without_stress = text_to_phonemes(TEXT_ALICE, "en-US", None, false, true)?.join("");
        assert_eq!(without_stress.contains(stress_markers), false);

        Ok(())
    }
    #[test]
    fn test_line_splitting() -> ESpeakResult<()> {
        let text = "Hello\nThere\nAnd\nWelcome";
        let phoneme_paragraphs = text_to_phonemes(text, "en-US", None, false, false)?;
        assert_eq!(phoneme_paragraphs.len(), 4);
        Ok(())
    }

    #[test]
    fn test_empty_input_returns_no_phonemes() -> ESpeakResult<()> {
        let phonemes = text_to_phonemes("", "en-US", None, false, false)?;
        assert_eq!(phonemes, Vec::<String>::new());
        Ok(())
    }

    #[test]
    fn test_interior_nul_byte_returns_err_instead_of_panicking() {
        let result = text_to_phonemes("hello\0world", "en-US", None, false, false);
        assert!(result.is_err());

        let result = text_to_phonemes("hello", "en\0US", None, false, false);
        assert!(result.is_err());
    }

    #[test]
    fn concurrent_calls_do_not_corrupt_each_others_output() {
        use std::thread;

        let en_text = "test";
        let en_expected = "tˈɛst.";
        let ar_text = "مَرْحَبَاً بِكَ أَيُّهَا الْرَّجُلْ";
        let ar_expected = "mˈarħabˌaː bikˌa ʔaˈiːuhˌaː alrrˈadʒul.";

        let handles: Vec<_> = (0..8)
            .map(|i| {
                thread::spawn(move || {
                    for _ in 0..25 {
                        if i % 2 == 0 {
                            let result = text_to_phonemes(en_text, "en-US", None, false, false)
                                .unwrap()
                                .join("");
                            assert_eq!(result, en_expected);
                        } else {
                            let result = text_to_phonemes(ar_text, "ar", None, false, false)
                                .unwrap()
                                .join("");
                            assert_eq!(result, ar_expected);
                        }
                    }
                })
            })
            .collect();

        for handle in handles {
            handle.join().expect("worker thread panicked");
        }
    }
}

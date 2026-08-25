#![forbid(unsafe_code)]

mod dictionary;
mod g2pw;
mod numbers;
mod syllable;
mod tokenize;

use dengjen_tts_core::{DengjenError, DengjenResult, Phonemes};
use dictionary::{load_dictionaries, resolve_char, Dictionaries};
use g2pw::{create_g2pw_engine, G2pwConfig, G2pwEngine};
use std::collections::HashMap;
use std::path::Path;

pub struct PinyinEngine {
    g2pw: G2pwEngine,
    char2phonemes: HashMap<char, Vec<usize>>,
    dictionaries: Dictionaries,
    bopomofo_to_pinyin: HashMap<String, String>,
}

fn load_error(cause: impl std::fmt::Display) -> DengjenError {
    DengjenError::PhonemizationError(format!(
        "Failed to load pinyin phonemizer model files: {cause}"
    ))
}

/// Loads all six files this backend needs from `model_dir`: `g2pw.onnx`,
/// `tokenizer.json`, `POLYPHONIC_CHARS.txt`, `MONOPHONIC_CHARS.txt`,
/// `char_bopomofo_dict.json`, `bopomofo_to_pinyin_wo_tune_dict.json`. All
/// caller-supplied — this crate never downloads anything.
pub fn create_pinyin_engine(model_dir: &Path) -> DengjenResult<PinyinEngine> {
    let dictionaries = load_dictionaries(
        &model_dir.join("MONOPHONIC_CHARS.txt"),
        &model_dir.join("POLYPHONIC_CHARS.txt"),
        &model_dir.join("char_bopomofo_dict.json"),
    )?;
    let (labels, char2phonemes) = dictionary::get_phoneme_labels(&dictionaries.polyphonic_chars);
    let mut chars: Vec<char> = char2phonemes.keys().copied().collect();
    chars.sort();

    let g2pw = create_g2pw_engine(
        &model_dir.join("g2pw.onnx"),
        &model_dir.join("tokenizer.json"),
        G2pwConfig {
            labels,
            char2phonemes: char2phonemes.clone(),
            window_size: 32,
            max_len: 512,
            chars,
        },
    )?;

    let raw = std::fs::read_to_string(model_dir.join("bopomofo_to_pinyin_wo_tune_dict.json"))
        .map_err(load_error)?;
    let bopomofo_to_pinyin: HashMap<String, String> =
        serde_json::from_str(&raw).map_err(load_error)?;

    Ok(PinyinEngine {
        g2pw,
        char2phonemes,
        dictionaries,
        bopomofo_to_pinyin,
    })
}

fn strip_quotation_marks(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{201C}' | '\u{201D}' | '"'))
        .collect()
}

fn split_into_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if matches!(c, '\u{3002}' | '！' | '？') {
            sentences.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        sentences.push(current);
    }
    sentences
}

fn assemble_syllable_phonemes(initial: &str, finale: &str, tone: char) -> Vec<String> {
    let ini = if initial.is_empty() {
        "\u{00D8}"
    } else {
        initial
    };
    vec![ini.to_string(), finale.to_string(), tone.to_string()]
}

fn phonemize_sentence(engine: &PinyinEngine, sentence: &str) -> DengjenResult<Vec<String>> {
    let chars: Vec<char> = sentence.chars().collect();
    let mut phonemes = Vec::new();
    for (i, &c) in chars.iter().enumerate() {
        let bopomofo = resolve_char(
            &engine.dictionaries,
            &engine.g2pw,
            &engine.char2phonemes,
            sentence,
            i,
        )?;
        let Some(bopomofo) = bopomofo else {
            // Not resolvable via any dictionary or the model -- treat as punctuation:
            // pass through if it's a recognized pause/terminal character, drop otherwise.
            if matches!(
                c,
                '\u{3002}'
                    | '.'
                    | '\u{FF1F}'
                    | '?'
                    | '\u{FF01}'
                    | '!'
                    | '\u{2014}'
                    | '\u{2026}'
                    | '\u{3001}'
                    | '\u{FF0C}'
                    | ','
                    | '\u{FF1A}'
                    | ':'
                    | '\u{FF1B}'
                    | ';'
                    | ' '
            ) {
                phonemes.push(c.to_string());
            }
            continue;
        };
        let Some(pinyin) =
            syllable::convert_bopomofo_to_pinyin(&bopomofo, &engine.bopomofo_to_pinyin)
        else {
            phonemes.push(bopomofo);
            continue;
        };
        let Some((initial, finale, tone)) = syllable::split_initial_final_tone(&pinyin) else {
            phonemes.push(pinyin);
            continue;
        };
        phonemes.extend(assemble_syllable_phonemes(&initial, &finale, tone));
    }
    Ok(phonemes)
}

/// Converts Chinese text to pinyin phoneme strings (initial/final/tone/pause
/// symbols), re-tokenized downstream by `map_phonemes_to_ids`'s longest-match
/// lookup against a voice's `phoneme_id_map`. This was flagged (#94) as a
/// possible collision risk — verified against two real zh-CN voices
/// (`zh_CN-chaowen-medium`, `zh_CN-xiao_ya-medium`, rhasspy/piper-voices,
/// identical 85-entry `phoneme_id_map`): every `initial×final×tone`
/// combination (4,200 total) re-segments to its original three tokens under
/// this engine's greedy longest-match algorithm. No collision; no fix needed.
pub fn text_to_pinyin_phonemes(engine: &PinyinEngine, text: &str) -> DengjenResult<Phonemes> {
    let stripped = strip_quotation_marks(text);
    let sentences: Vec<String> = split_into_sentences(&stripped)
        .into_iter()
        .map(|s| numbers::normalize_numbers(&s))
        .collect();

    let mut out = Vec::with_capacity(sentences.len());
    for sentence in &sentences {
        let phonemes = phonemize_sentence(engine, sentence)?;
        out.push(phonemes.concat());
    }
    Ok(Phonemes::from(out))
}

#[cfg(test)]
mod tests {
    #[test]
    fn split_into_sentences_splits_on_chinese_terminal_punctuation_keeping_the_punctuation() {
        let sentences = super::split_into_sentences("你好。再见！");
        assert_eq!(sentences, vec!["你好。".to_string(), "再见！".to_string()]);
    }

    #[test]
    fn split_into_sentences_keeps_a_trailing_fragment_with_no_terminal_punctuation() {
        let sentences = super::split_into_sentences("你好。还有更多");
        assert_eq!(
            sentences,
            vec!["你好。".to_string(), "还有更多".to_string()]
        );
    }

    #[test]
    fn split_into_sentences_on_empty_input_yields_no_sentences() {
        assert_eq!(super::split_into_sentences(""), Vec::<String>::new());
    }

    #[test]
    fn strip_quotation_marks_removes_curly_and_straight_double_quotes() {
        assert_eq!(super::strip_quotation_marks("“你好”\"再见\""), "你好再见");
    }

    #[test]
    fn assemble_syllable_phonemes_emits_zero_initial_marker_for_a_zero_initial_syllable() {
        let phonemes = super::assemble_syllable_phonemes("", "ai", '3');
        assert_eq!(
            phonemes,
            vec!["\u{00D8}".to_string(), "ai".to_string(), "3".to_string()]
        );
    }

    #[test]
    fn assemble_syllable_phonemes_emits_a_real_initial_when_present() {
        let phonemes = super::assemble_syllable_phonemes("zh", "ang", '2');
        assert_eq!(
            phonemes,
            vec!["zh".to_string(), "ang".to_string(), "2".to_string()]
        );
    }

    #[test]
    fn text_to_pinyin_phonemes_produces_output_for_a_simple_sentence_with_a_real_model() {
        let Ok(model_dir) = std::env::var("DENGJEN_PINYIN_TEST_MODEL_DIR") else {
            eprintln!("skipping: DENGJEN_PINYIN_TEST_MODEL_DIR not set");
            return;
        };
        let engine = super::create_pinyin_engine(std::path::Path::new(&model_dir)).unwrap();
        let phonemes = super::text_to_pinyin_phonemes(&engine, "你好").unwrap();
        assert_eq!(phonemes.num_sentences(), 1);
        assert!(!phonemes.sentences()[0].is_empty());
    }
}

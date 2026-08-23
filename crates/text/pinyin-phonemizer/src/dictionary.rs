//! Three-tier character-to-bopomofo resolution, ported from
//! OHF-Voice/piper1-gpl's `src/piper/g2pw_onnx.py`, itself an inference-only
//! port of GitYCC/g2pW (Apache-2.0). Resolution order (checked in
//! `_prepare_data`, upstream lines 511-523): polyphonic characters go to the
//! g2pW model; everything else resolves from `monophonic_chars_dict` first,
//! `char_bopomofo_dict` second.

use crate::g2pw::G2pwEngine;
use dengjen_tts_core::{DengjenError, DengjenResult};
use std::collections::HashMap;
use std::path::Path;

pub(crate) struct Dictionaries {
    pub monophonic: HashMap<char, String>,
    pub char_bopomofo: HashMap<char, String>,
    // read once lib.rs's orchestration (a later task) builds a real config from it
    pub polyphonic_chars: Vec<(char, String)>,
}

fn read_error(path: &Path, cause: impl std::fmt::Display) -> DengjenError {
    DengjenError::PhonemizationError(format!(
        "Failed to read pinyin dictionary file {}: {cause}",
        path.display()
    ))
}

fn read_tab_separated_char_string_pairs(path: &Path) -> DengjenResult<Vec<(char, String)>> {
    let content = std::fs::read_to_string(path).map_err(|e| read_error(path, e))?;
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| {
            let mut parts = line.splitn(2, '\t');
            let char_part = parts.next().unwrap_or_default();
            let value_part = parts.next().unwrap_or_default();
            let c = char_part.chars().next().ok_or_else(|| {
                DengjenError::PhonemizationError(format!(
                    "Malformed line in {}: {line:?}",
                    path.display()
                ))
            })?;
            Ok((c, value_part.to_string()))
        })
        .collect()
}

pub(crate) fn load_dictionaries(
    monophonic_path: &Path,
    polyphonic_path: &Path,
    char_bopomofo_path: &Path,
) -> DengjenResult<Dictionaries> {
    let monophonic = read_tab_separated_char_string_pairs(monophonic_path)?
        .into_iter()
        .collect();
    let polyphonic_chars = read_tab_separated_char_string_pairs(polyphonic_path)?;

    let raw = std::fs::read_to_string(char_bopomofo_path)
        .map_err(|e| read_error(char_bopomofo_path, e))?;
    let parsed: HashMap<String, Vec<String>> =
        serde_json::from_str(&raw).map_err(|e| read_error(char_bopomofo_path, e))?;
    let char_bopomofo = parsed
        .into_iter()
        .filter_map(|(k, v)| {
            let c = k.chars().next()?;
            let first = v.into_iter().next()?;
            Some((c, first))
        })
        .collect();

    Ok(Dictionaries {
        monophonic,
        char_bopomofo,
        polyphonic_chars,
    })
}

pub(crate) fn get_phoneme_labels(
    polyphonic_chars: &[(char, String)],
) -> (Vec<String>, HashMap<char, Vec<usize>>) {
    let mut labels: Vec<String> = polyphonic_chars
        .iter()
        .map(|(_, phoneme)| phoneme.clone())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    labels.sort();

    let mut char2phonemes: HashMap<char, Vec<usize>> = HashMap::new();
    for (c, phoneme) in polyphonic_chars {
        if let Some(idx) = labels.iter().position(|l| l == phoneme) {
            char2phonemes.entry(*c).or_default().push(idx);
        }
    }
    (labels, char2phonemes)
}

/// Dictionary-only resolution (no model call) — used both directly for
/// non-polyphonic characters and as the "does this even need the model"
/// pre-check `resolve_char` performs before calling into `g2pw`.
pub(crate) fn resolve_char_dictionary_only(dictionaries: &Dictionaries, c: char) -> Option<String> {
    dictionaries
        .monophonic
        .get(&c)
        .or_else(|| dictionaries.char_bopomofo.get(&c))
        .cloned()
}

pub(crate) fn resolve_char(
    dictionaries: &Dictionaries,
    g2pw: &G2pwEngine,
    char2phonemes: &HashMap<char, Vec<usize>>,
    text: &str,
    char_index: usize,
) -> DengjenResult<Option<String>> {
    let chars: Vec<char> = text.chars().collect();
    let c = *chars.get(char_index).ok_or_else(|| {
        DengjenError::PhonemizationError(format!(
            "char_index {char_index} is out of bounds for text {text:?}"
        ))
    })?;

    if char2phonemes.contains_key(&c) {
        return g2pw.resolve_polyphonic(text, char_index).map(Some);
    }

    Ok(resolve_char_dictionary_only(dictionaries, c))
}

#[cfg(test)]
mod tests {
    #[test]
    fn get_phoneme_labels_sorts_labels_and_indexes_char_to_phonemes() {
        let polyphonic_chars = vec![
            ('行', "ㄒㄧㄥˊ".to_string()),
            ('行', "ㄏㄤˊ".to_string()),
            ('长', "ㄔㄤˊ".to_string()),
        ];
        let (labels, char2phonemes) = super::get_phoneme_labels(&polyphonic_chars);
        assert_eq!(labels.len(), 3);
        let xing_idx = labels.iter().position(|l| l == "ㄒㄧㄥˊ").unwrap();
        let hang_idx = labels.iter().position(|l| l == "ㄏㄤˊ").unwrap();
        let mut got = char2phonemes.get(&'行').cloned().unwrap();
        got.sort();
        let mut want = vec![xing_idx, hang_idx];
        want.sort();
        assert_eq!(got, want);
    }

    #[test]
    fn resolve_char_prefers_monophonic_over_char_bopomofo_when_both_present() {
        let dictionaries = super::Dictionaries {
            monophonic: [('好', "ㄏㄠˇ".to_string())].into_iter().collect(),
            char_bopomofo: [('好', "ㄏㄠˋ".to_string())].into_iter().collect(),
            polyphonic_chars: vec![],
        };
        let result = super::resolve_char_dictionary_only(&dictionaries, '好');
        assert_eq!(result, Some("ㄏㄠˇ".to_string()));
    }

    #[test]
    fn resolve_char_falls_back_to_char_bopomofo_when_not_monophonic() {
        let dictionaries = super::Dictionaries {
            monophonic: std::collections::HashMap::new(),
            char_bopomofo: [('你', "ㄋㄧˇ".to_string())].into_iter().collect(),
            polyphonic_chars: vec![],
        };
        let result = super::resolve_char_dictionary_only(&dictionaries, '你');
        assert_eq!(result, Some("ㄋㄧˇ".to_string()));
    }

    #[test]
    fn resolve_char_dictionary_only_returns_none_for_an_unresolved_character() {
        let dictionaries = super::Dictionaries {
            monophonic: std::collections::HashMap::new(),
            char_bopomofo: std::collections::HashMap::new(),
            polyphonic_chars: vec![],
        };
        assert_eq!(
            super::resolve_char_dictionary_only(&dictionaries, '。'),
            None
        );
    }
}

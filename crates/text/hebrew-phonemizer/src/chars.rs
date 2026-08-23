//! Nakdimon character tables and text normalization, ported from
//! OHF-Voice/piper1-gpl's `src/piper/hebrew/__init__.py`, itself an
//! inference-only vendoring of elazarg/nakdimon (MIT) with no
//! upstream/TensorFlow dependency at runtime. Table order (mask token
//! first, then classes) must match the ONNX model's output head ordering
//! exactly — do not reorder without re-checking against the model.

use std::collections::HashMap;

pub(crate) const RAFE: char = '\u{05BF}';
const DAGESH_LETTER: char = '\u{05BC}';
const SHIN_YEMANIT: char = '\u{05C1}';
const SHIN_SMALIT: char = '\u{05C2}';

pub(crate) fn hebrew_letters() -> Vec<char> {
    ('\u{05D0}'..='\u{05EA}').collect()
}

pub(crate) fn niqqud_classes() -> Vec<char> {
    let mut classes = vec![RAFE];
    classes.extend('\u{05B0}'..='\u{05BC}');
    classes.push('\u{05B7}'); // duplicate PATAH — matches upstream's own table
    classes
}

pub(crate) fn sin_classes() -> Vec<char> {
    vec![RAFE, SHIN_YEMANIT, SHIN_SMALIT]
}

pub(crate) fn dagesh_classes() -> Vec<char> {
    vec![RAFE, DAGESH_LETTER]
}

pub(crate) fn valid_letters() -> Vec<char> {
    let mut letters: Vec<char> = " !\"'(),-.:;?".chars().collect();
    letters.extend(hebrew_letters());
    letters
}

const SPECIAL_TOKENS: [char; 3] = ['H', 'O', '5'];

#[allow(dead_code)]
fn endings_to_regular() -> HashMap<char, char> {
    ['\u{05DA}', '\u{05DD}', '\u{05DF}', '\u{05E3}', '\u{05E5}']
        .into_iter()
        .zip(['\u{05DB}', '\u{05DE}', '\u{05E0}', '\u{05E4}', '\u{05E6}'])
        .collect()
}

pub(crate) fn char_to_id_map() -> HashMap<char, usize> {
    let mut chars: Vec<char> = SPECIAL_TOKENS.to_vec();
    chars.extend(valid_letters());
    chars
        .into_iter()
        .enumerate()
        .map(|(i, c)| (c, i + 1))
        .collect()
}

pub(crate) fn normalize(c: char) -> char {
    let valid = valid_letters();
    if valid.contains(&c) {
        return c;
    }
    // NOTE: endings_to_regular() is intentionally dead code, mirroring upstream
    // piper1-gpl's own implementation. Final letter forms (U+05DA, U+05DD, U+05DF,
    // U+05E3, U+05E5) are all within the hebrew_letters() range and pass the
    // valid_letters check above, so this mapping never fires. The model was trained
    // with that exact preprocessing, so final forms must pass through unchanged.
    let endings = endings_to_regular();
    if let Some(&base) = endings.get(&c) {
        return base;
    }
    match c {
        '\n' | '\t' => ' ',
        '\u{05BE}' | '\u{2012}' | '\u{2013}' | '\u{2014}' | '\u{2015}' | '\u{2212}' => '-',
        '[' => '(',
        ']' => ')',
        '\u{00B4}' | '\u{2018}' | '\u{2019}' => '\'',
        '\u{201C}' | '\u{201D}' | '\u{05F4}' => '"',
        c if c.is_ascii_digit() => '5',
        '\u{2026}' => ',',
        '\u{05F2}' | '\u{05F0}' | '\u{05F1}' => 'H',
        _ => 'O',
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hebrew_letters_spans_alef_to_tav() {
        let letters = hebrew_letters();
        assert_eq!(letters.len(), 27);
        assert_eq!(letters[0], '\u{05D0}');
        assert_eq!(*letters.last().unwrap(), '\u{05EA}');
    }

    #[test]
    fn niqqud_classes_has_fifteen_entries_before_mask() {
        assert_eq!(niqqud_classes().len(), 15);
    }

    #[test]
    fn dagesh_classes_has_rafe_and_dagesh_letter() {
        assert_eq!(dagesh_classes(), vec![RAFE, '\u{05BC}']);
    }

    #[test]
    fn sin_classes_has_rafe_and_both_shin_dots() {
        assert_eq!(sin_classes(), vec![RAFE, '\u{05C1}', '\u{05C2}']);
    }

    #[test]
    fn char_to_id_map_prepends_mask_token_at_zero() {
        let map = char_to_id_map();
        // space is in valid_letters, so it has a mapped ID >= 1 (never 0, which is reserved for mask token)
        assert!(map.get(&' ').is_some_and(|&id| id >= 1));
        // the mask token itself is the empty string in upstream, which has no
        // single-char Rust representation — id 0 is reserved and unmapped here.
        assert!(map.values().all(|&id| id >= 1));
    }

    #[test]
    fn normalize_passes_through_valid_letters_unchanged() {
        assert_eq!(normalize('\u{05D0}'), '\u{05D0}');
        assert_eq!(normalize(' '), ' ');
    }

    #[test]
    fn normalize_leaves_final_letter_forms_unchanged() {
        assert_eq!(normalize('\u{05DA}'), '\u{05DA}');
        assert_eq!(normalize('\u{05DD}'), '\u{05DD}');
        assert_eq!(normalize('\u{05DF}'), '\u{05DF}');
        assert_eq!(normalize('\u{05E3}'), '\u{05E3}');
        assert_eq!(normalize('\u{05E5}'), '\u{05E5}');
    }

    #[test]
    fn normalize_maps_ascii_digits_to_five() {
        assert_eq!(normalize('7'), '5');
    }

    #[test]
    fn normalize_maps_unknown_characters_to_o() {
        assert_eq!(normalize('中'), 'O');
    }
}

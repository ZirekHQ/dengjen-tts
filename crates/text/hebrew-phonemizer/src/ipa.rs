//! Rule-based Hebrew-to-IPA grapheme-to-phoneme conversion, ported from
//! OHF-Voice/piper1-gpl's `src/piper/hebrew/hebrew_ipa.py` for exact
//! compatibility with real piper1-gpl-trained he-IL voices.

use std::collections::HashMap;
use unicode_normalization::UnicodeNormalization;

const TAAMIM_START: u32 = 0x0591;
const TAAMIM_END: u32 = 0x05AF;
const DAGESH: char = '\u{05BC}';
const SHIN_DOT: char = '\u{05C1}';
const SIN_DOT: char = '\u{05C2}';
const GERESH: char = '\u{05F3}';

const ALEF: char = '\u{05D0}';
const BET: char = '\u{05D1}';
const GIMEL: char = '\u{05D2}';
const DALET: char = '\u{05D3}';
const HE: char = '\u{05D4}';
const VAV: char = '\u{05D5}';
const ZAYIN: char = '\u{05D6}';
const HET: char = '\u{05D7}';
const TET: char = '\u{05D8}';
const YOD: char = '\u{05D9}';
const KAF: char = '\u{05DB}';
const KAF_FINAL: char = '\u{05DA}';
const LAMED: char = '\u{05DC}';
const MEM: char = '\u{05DE}';
const MEM_FINAL: char = '\u{05DD}';
const NUN: char = '\u{05E0}';
const NUN_FINAL: char = '\u{05DF}';
const SAMEKH: char = '\u{05E1}';
const AYIN: char = '\u{05E2}';
const PE: char = '\u{05E4}';
const PE_FINAL: char = '\u{05E3}';
const TSADI: char = '\u{05E6}';
const TSADI_FINAL: char = '\u{05E5}';
const QOF: char = '\u{05E7}';
const RESH: char = '\u{05E8}';
const SHIN: char = '\u{05E9}';
const TAV: char = '\u{05EA}';

#[allow(dead_code)]
fn final_form_base(c: char) -> char {
    match c {
        KAF_FINAL => KAF,
        MEM_FINAL => MEM,
        NUN_FINAL => NUN,
        PE_FINAL => PE,
        TSADI_FINAL => TSADI,
        other => other,
    }
}

#[allow(dead_code)]
fn geresh_digraphs() -> HashMap<String, &'static str> {
    HashMap::from([
        (format!("{GIMEL}{GERESH}"), "d\u{0361}\u{0292}"),
        (format!("{ZAYIN}{GERESH}"), "\u{0292}"),
        (format!("{TSADI}{GERESH}"), "t\u{0361}\u{0283}"),
    ])
}

#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Glyph {
    pub base: String,
    pub marks: Vec<char>,
}

#[allow(dead_code)]
impl Glyph {
    fn plain(base: char) -> Self {
        Glyph {
            base: base.to_string(),
            marks: Vec::new(),
        }
    }

    pub(crate) fn has(&self, mark: char) -> bool {
        self.marks.contains(&mark)
    }

    pub(crate) fn any(&self, marks: &[char]) -> bool {
        marks.iter().any(|m| self.marks.contains(m))
    }
}

#[allow(dead_code)]
fn strip_taamim(s: &str) -> String {
    s.chars()
        .filter(|&c| !(TAAMIM_START..=TAAMIM_END).contains(&(c as u32)))
        .collect()
}

#[allow(dead_code)]
pub(crate) fn iter_glyphs(word: &str) -> Vec<Glyph> {
    let normalized: String = strip_taamim(word).nfc().collect();
    let mut glyphs: Vec<Glyph> = Vec::new();
    for ch in normalized.chars() {
        if unicode_normalization::char::is_combining_mark(ch) {
            if let Some(last) = glyphs.last_mut() {
                last.marks.push(ch);
            }
        } else {
            glyphs.push(Glyph::plain(ch));
        }
    }
    glyphs
}

#[allow(dead_code)]
pub(crate) fn apply_geresh_digraphs(glyphs: Vec<Glyph>) -> Vec<Glyph> {
    let digraphs = geresh_digraphs();
    let mut out = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        let g = &glyphs[i];
        if i + 1 < glyphs.len() && glyphs[i + 1].base == GERESH.to_string() {
            let key = format!("{}{}", g.base, glyphs[i + 1].base);
            if let Some(&ipa) = digraphs.get(&key) {
                out.push(Glyph {
                    base: format!("<IPA:{ipa}>"),
                    marks: Vec::new(),
                });
                i += 2;
                continue;
            }
        }
        out.push(g.clone());
        i += 1;
    }
    out
}

#[allow(dead_code)]
pub(crate) fn map_consonant(base: char, marks: &[char], is_final: bool) -> String {
    let b = final_form_base(base);
    let has = |m: char| marks.contains(&m);

    if b == ALEF || b == AYIN {
        return "<GLT>".to_string();
    }
    if b == HE {
        return if is_final && !has(DAGESH) {
            String::new()
        } else {
            "h".to_string()
        };
    }
    if b == YOD {
        return "j".to_string();
    }
    if b == VAV {
        return "v".to_string();
    }
    if b == SHIN {
        return if has(SHIN_DOT) {
            "\u{0283}".to_string()
        } else if has(SIN_DOT) {
            "s".to_string()
        } else {
            "\u{0283}".to_string()
        };
    }
    if b == BET {
        return if has(DAGESH) { "b" } else { "v" }.to_string();
    }
    if b == KAF {
        return if has(DAGESH) { "k" } else { "\u{03C7}" }.to_string();
    }
    if b == PE {
        return if has(DAGESH) { "p" } else { "f" }.to_string();
    }
    if b == GIMEL {
        return "g".to_string();
    }
    if b == DALET {
        return "d".to_string();
    }
    if b == HET {
        return "\u{03C7}".to_string();
    }
    if b == TET {
        return "t".to_string();
    }
    if b == LAMED {
        return "l".to_string();
    }
    if b == MEM {
        return "m".to_string();
    }
    if b == NUN {
        return "n".to_string();
    }
    if b == SAMEKH {
        return "s".to_string();
    }
    if b == TSADI {
        return "t\u{0361}s".to_string();
    }
    if b == QOF {
        return "k".to_string();
    }
    if b == RESH {
        return "\u{0281}".to_string();
    }
    if b == TAV {
        return "t".to_string();
    }
    if b == ZAYIN {
        return "z".to_string();
    }
    String::new()
}

const SHEVA: char = '\u{05B0}';
const HATAF_SEGOL: char = '\u{05B1}';
const HATAF_PATAH: char = '\u{05B2}';
const HATAF_QAMATS: char = '\u{05B3}';
const HIRIQ: char = '\u{05B4}';
const TSERE: char = '\u{05B5}';
const SEGOL: char = '\u{05B6}';
const PATAH: char = '\u{05B7}';
const QAMATS: char = '\u{05B8}';
const HOLAM: char = '\u{05B9}';
const QUBUTZ: char = '\u{05BB}';
const QAMATS_QATAN: char = '\u{05C7}';

#[allow(dead_code)]
pub(crate) fn map_basic_vowel(g: &Glyph) -> (String, bool) {
    if g.has(QAMATS_QATAN) {
        return ("o".to_string(), true);
    }
    if g.has(QUBUTZ) {
        return ("u".to_string(), true);
    }
    if g.has(HIRIQ) {
        return ("i".to_string(), true);
    }
    if g.has(TSERE) || g.has(SEGOL) {
        return ("e".to_string(), true);
    }
    if g.has(PATAH) || g.has(QAMATS) || g.has(HATAF_PATAH) {
        return ("a".to_string(), true);
    }
    if g.has(HATAF_SEGOL) {
        return ("e".to_string(), true);
    }
    if g.has(HATAF_QAMATS) {
        return ("o".to_string(), true);
    }
    if g.has(SHEVA) {
        return ("\u{0259}".to_string(), false);
    }
    (String::new(), false)
}

#[allow(dead_code)]
pub(crate) fn is_shuruk(g: &Glyph) -> bool {
    if g.base != VAV.to_string() || !g.has(DAGESH) {
        return false;
    }
    let other_vowel_marks = [
        HOLAM,
        HIRIQ,
        TSERE,
        SEGOL,
        PATAH,
        QAMATS,
        QUBUTZ,
        QAMATS_QATAN,
        SHEVA,
        HATAF_SEGOL,
        HATAF_PATAH,
        HATAF_QAMATS,
    ];
    !g.marks.iter().any(|m| other_vowel_marks.contains(m))
}

#[allow(dead_code)]
pub(crate) fn is_holam_male(g: &Glyph) -> bool {
    g.base == VAV.to_string() && g.has(HOLAM) && !g.has(DAGESH)
}

fn has_any_vowel_mark(marks: &[char]) -> bool {
    let vowel_marks = [
        SHEVA,
        HATAF_SEGOL,
        HATAF_PATAH,
        HATAF_QAMATS,
        HIRIQ,
        TSERE,
        SEGOL,
        PATAH,
        QAMATS,
        HOLAM,
        QUBUTZ,
        QAMATS_QATAN,
    ];
    marks.iter().any(|m| vowel_marks.contains(m))
}

#[allow(dead_code)]
pub(crate) fn is_hiriq_yod(curr: &Glyph, next: Option<&Glyph>) -> bool {
    let Some(next) = next else { return false };
    curr.has(HIRIQ) && next.base == YOD.to_string() && !has_any_vowel_mark(&next.marks)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iter_glyphs_groups_combining_marks_with_the_preceding_base_letter() {
        let glyphs = iter_glyphs("\u{05D1}\u{05B7}"); // bet + patah
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].base, "\u{05D1}");
        assert_eq!(glyphs[0].marks, vec!['\u{05B7}']);
    }

    #[test]
    fn iter_glyphs_strips_cantillation_marks_first() {
        let glyphs = iter_glyphs("\u{05D1}\u{0591}\u{05B7}"); // bet + etnahta + patah
        assert_eq!(glyphs.len(), 1);
        assert_eq!(glyphs[0].marks, vec!['\u{05B7}']);
    }

    #[test]
    fn apply_geresh_digraphs_collapses_gimel_geresh_into_a_placeholder() {
        let glyphs = vec![
            Glyph {
                base: "\u{05D2}".to_string(),
                marks: vec![],
            }, // gimel
            Glyph {
                base: "\u{05F3}".to_string(),
                marks: vec![],
            }, // geresh
        ];
        let out = apply_geresh_digraphs(glyphs);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].base, "<IPA:d\u{0361}\u{0292}>");
    }

    #[test]
    fn map_consonant_bet_alternates_on_dagesh() {
        assert_eq!(map_consonant('\u{05D1}', &[], false), "v");
        assert_eq!(map_consonant('\u{05D1}', &['\u{05BC}'], false), "b");
    }

    #[test]
    fn map_consonant_final_he_without_mappiq_is_silent() {
        assert_eq!(map_consonant('\u{05D4}', &[], true), "");
        assert_eq!(map_consonant('\u{05D4}', &['\u{05BC}'], true), "h");
    }

    #[test]
    fn map_consonant_final_forms_map_to_their_base_letter_sound() {
        assert_eq!(map_consonant('\u{05DA}', &[], true), "\u{03C7}"); // kaf-final, no dagesh
    }

    #[test]
    fn map_consonant_shin_uses_shin_dot_or_sin_dot() {
        assert_eq!(map_consonant('\u{05E9}', &['\u{05C1}'], false), "\u{0283}");
        assert_eq!(map_consonant('\u{05E9}', &['\u{05C2}'], false), "s");
    }

    #[test]
    fn map_consonant_alef_and_ayin_are_placeholder_glottal() {
        assert_eq!(map_consonant('\u{05D0}', &[], false), "<GLT>");
        assert_eq!(map_consonant('\u{05E2}', &[], false), "<GLT>");
    }

    #[test]
    fn map_basic_vowel_reads_each_niqqud_mark() {
        let cases = [
            ('\u{05B8}', "a"), // qamats
            ('\u{05B7}', "a"), // patah
            ('\u{05B4}', "i"), // hiriq
            ('\u{05B5}', "e"), // tsere
            ('\u{05B6}', "e"), // segol
            ('\u{05B9}', ""),  // holam handled separately (mater cases), not here
        ];
        for (mark, expected) in cases {
            if mark == '\u{05B9}' {
                continue; // holam-on-consonant alone is not one of the basic cases
            }
            let g = Glyph {
                base: "x".to_string(),
                marks: vec![mark],
            };
            let (ipa, is_vocalic) = map_basic_vowel(&g);
            assert_eq!(ipa, expected);
            assert!(is_vocalic);
        }
    }

    #[test]
    fn map_basic_vowel_sheva_is_a_placeholder_schwa() {
        let g = Glyph {
            base: "x".to_string(),
            marks: vec!['\u{05B0}'],
        };
        let (ipa, is_vocalic) = map_basic_vowel(&g);
        assert_eq!(ipa, "\u{0259}");
        assert!(!is_vocalic);
    }

    #[test]
    fn map_basic_vowel_no_marks_is_not_vocalic() {
        let g = Glyph {
            base: "x".to_string(),
            marks: vec![],
        };
        let (ipa, is_vocalic) = map_basic_vowel(&g);
        assert_eq!(ipa, "");
        assert!(!is_vocalic);
    }

    #[test]
    fn is_shuruk_detects_vav_with_only_a_dagesh() {
        let g = Glyph {
            base: VAV.to_string(),
            marks: vec!['\u{05BC}'],
        };
        assert!(is_shuruk(&g));
    }

    #[test]
    fn is_shuruk_false_when_another_vowel_mark_is_present() {
        let g = Glyph {
            base: VAV.to_string(),
            marks: vec!['\u{05BC}', '\u{05B7}'],
        };
        assert!(!is_shuruk(&g));
    }

    #[test]
    fn is_holam_male_detects_vav_with_holam_and_no_dagesh() {
        let g = Glyph {
            base: VAV.to_string(),
            marks: vec!['\u{05B9}'],
        };
        assert!(is_holam_male(&g));
    }

    #[test]
    fn is_hiriq_yod_detects_hiriq_followed_by_bare_yod() {
        let curr = Glyph {
            base: "x".to_string(),
            marks: vec!['\u{05B4}'],
        };
        let next = Glyph {
            base: YOD.to_string(),
            marks: vec![],
        };
        assert!(is_hiriq_yod(&curr, Some(&next)));
    }

    #[test]
    fn is_hiriq_yod_false_when_next_yod_has_its_own_vowel() {
        let curr = Glyph {
            base: "x".to_string(),
            marks: vec!['\u{05B4}'],
        };
        let next = Glyph {
            base: YOD.to_string(),
            marks: vec!['\u{05B7}'],
        };
        assert!(!is_hiriq_yod(&curr, Some(&next)));
    }
}

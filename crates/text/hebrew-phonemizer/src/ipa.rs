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

fn geresh_digraphs() -> HashMap<String, &'static str> {
    HashMap::from([
        (format!("{GIMEL}{GERESH}"), "d\u{0361}\u{0292}"),
        (format!("{ZAYIN}{GERESH}"), "\u{0292}"),
        (format!("{TSADI}{GERESH}"), "t\u{0361}\u{0283}"),
    ])
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Glyph {
    pub base: String,
    pub marks: Vec<char>,
}

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
}

fn strip_taamim(s: &str) -> String {
    s.chars()
        .filter(|&c| !(TAAMIM_START..=TAAMIM_END).contains(&(c as u32)))
        .collect()
}

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

pub(crate) fn is_hiriq_yod(curr: &Glyph, next: Option<&Glyph>) -> bool {
    let Some(next) = next else { return false };
    curr.has(HIRIQ) && next.base == YOD.to_string() && !has_any_vowel_mark(&next.marks)
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Segment {
    pub onset: Vec<String>,
    pub nucleus: String,
    pub coda: Vec<String>,
    pub dagesh: bool,
}

pub(crate) fn word_to_segments(word: &str) -> Vec<Segment> {
    let glyphs = apply_geresh_digraphs(iter_glyphs(word));

    let mut segs: Vec<Segment> = Vec::new();
    let mut onset: Vec<String> = Vec::new();
    let mut i = 0;
    while i < glyphs.len() {
        let g = &glyphs[i];
        let next = glyphs.get(i + 1);
        let is_final = i == glyphs.len() - 1;

        // Real upstream piper1-gpl's `_word_to_segments` has no special case
        // for geresh-digraph placeholder glyphs (base like "<IPA:...>"):
        // they fall through to the ordinary consonant/vowel handling below,
        // where `map_consonant('<', ...)` matches no branch and returns "",
        // silently dropping the digraph's sound. This is upstream's actual
        // (bug-like but intentional-for-fidelity) behavior, so it is not
        // special-cased here either — see word_to_segments_geresh_digraph_is_silently_dropped_matching_upstream.
        //
        // Every Glyph is built from exactly one base char
        // (see Glyph::plain / iter_glyphs), so this is always Some; '\0'
        // is an unreachable, harmless fallback that simply maps to no
        // consonant rather than panicking.
        let base_char = g.base.chars().next().unwrap_or('\0');

        if is_shuruk(g) {
            segs.push(Segment {
                onset: std::mem::take(&mut onset),
                nucleus: "u".to_string(),
                ..Default::default()
            });
            i += 1;
            continue;
        }
        if is_hiriq_yod(g, next) {
            let cons = map_consonant(base_char, &g.marks, false);
            if !cons.is_empty() && cons != "<GLT>" {
                onset.push(cons);
            }
            segs.push(Segment {
                onset: std::mem::take(&mut onset),
                nucleus: "i".to_string(),
                ..Default::default()
            });
            i += 2;
            continue;
        }
        if is_holam_male(g) {
            segs.push(Segment {
                onset: std::mem::take(&mut onset),
                nucleus: "o".to_string(),
                ..Default::default()
            });
            i += 1;
            continue;
        }

        let cons = map_consonant(base_char, &g.marks, is_final);
        let (vowel, is_vocalic) = map_basic_vowel(g);

        if is_vocalic {
            if !cons.is_empty() && cons != "<GLT>" {
                onset.push(cons);
            }
            segs.push(Segment {
                onset: std::mem::take(&mut onset),
                nucleus: vowel,
                ..Default::default()
            });
        } else if g.has(SHEVA) {
            if !cons.is_empty() && cons != "<GLT>" {
                onset.push(cons);
            }
            segs.push(Segment {
                onset: std::mem::take(&mut onset),
                nucleus: "\u{0259}".to_string(),
                dagesh: g.has(DAGESH),
                ..Default::default()
            });
        } else if cons == "<GLT>" {
            onset.push("\u{0294}".to_string());
        } else if !cons.is_empty() {
            onset.push(cons);
        }
        i += 1;
    }

    if !onset.is_empty() {
        if let Some(last) = segs.last_mut() {
            last.coda.extend(onset);
        }
    }
    segs
}

fn resolve_sheva_and_qamats(mut segs: Vec<Segment>) -> Vec<Segment> {
    let mut prev_silenced_shva = false;
    for i in 0..segs.len() {
        if segs[i].nucleus != "\u{0259}" {
            prev_silenced_shva = false;
            continue;
        }
        let dagesh_chazak = segs[i].dagesh && i > 0;
        let na = i == 0 || dagesh_chazak || prev_silenced_shva;

        if na {
            segs[i].nucleus = "e".to_string();
            prev_silenced_shva = false;
        } else {
            let onset = std::mem::take(&mut segs[i].onset);
            if i > 0 {
                segs[i - 1].coda.extend(onset);
            }
            segs[i].nucleus = String::new();
            prev_silenced_shva = true;
        }
    }

    let mut merged: Vec<Segment> = Vec::new();
    for s in segs {
        if s.nucleus.is_empty() {
            if let Some(last) = merged.last_mut() {
                last.coda.extend(s.onset);
            } else {
                merged.push(s);
            }
            continue;
        }
        merged.push(s);
    }
    merged
}

fn syllabify_to_ipa(segs: &[Segment]) -> String {
    // Segholate-style exception: exactly two syllables where the last one
    // is closed (has a coda) are stressed on the first syllable instead of
    // the (otherwise default) last one.
    let stress_index = match segs {
        [_, second] if !second.coda.is_empty() => 0,
        _ => segs.len().saturating_sub(1),
    };

    segs.iter()
        .enumerate()
        .map(|(idx, s)| {
            let onset: String = s.onset.concat();
            let coda: String = s.coda.concat();
            if idx == stress_index {
                format!("{onset}\u{02C8}{}{coda}", s.nucleus)
            } else {
                format!("{onset}{}{coda}", s.nucleus)
            }
        })
        .collect()
}

const GLOTTAL: char = '\u{0294}';
const STRESS: char = '\u{02C8}';

// Real upstream uses three lookahead regex substitutions to clean up the
// `<GLT>` glottal-stop placeholder:
//   ʔ(?=ˈ?[aeiouə])  -> ʔ   (keep before a vowel: a no-op self-replace)
//   (?<=^)ʔ          -> ʔ   (keep word-initial: also a no-op self-replace)
//   ʔ(?=[^aeiouəˈ]|$) -> "" (drop before a consonant or at end of string)
// Only the third substitution has any actual effect; the first two rewrite
// a match to itself. Rust's `regex` crate has no lookaround support at all
// (by design, for its linear-time guarantee), so this reproduces the net
// effect directly: keep a glottal stop only when the character immediately
// after it (skipping at most one stress mark) is a vowel or schwa; drop it
// otherwise, including at end of string. `syllabify_to_ipa` only ever
// places a stress mark directly before a (non-empty) vowel nucleus, so
// "skip one stress mark, then check for a vowel" and upstream's literal
// "next char is one of aeiouəˈ" are equivalent for every string this
// function actually produces.
fn strip_unrealized_glottal_stops(ipa: &str) -> String {
    let chars: Vec<char> = ipa.chars().collect();
    let mut out = String::with_capacity(ipa.len());
    for (i, &c) in chars.iter().enumerate() {
        if c != GLOTTAL {
            out.push(c);
            continue;
        }
        let after_stress = if chars.get(i + 1) == Some(&STRESS) {
            i + 2
        } else {
            i + 1
        };
        let followed_by_vowel = matches!(
            chars.get(after_stress),
            Some('a' | 'e' | 'i' | 'o' | 'u' | '\u{0259}')
        );
        if followed_by_vowel {
            out.push(c);
        }
    }
    out
}

pub(crate) fn hebrew_word_to_ipa(word: &str) -> String {
    let segs = resolve_sheva_and_qamats(word_to_segments(word));
    let ipa = syllabify_to_ipa(&segs);
    let ipa = strip_unrealized_glottal_stops(&ipa);

    // Drop the affricate tie bar so t͡s/t͡ʃ/d͡ʒ become plain phoneme pairs
    // that exist in Piper's default IPA id map.
    ipa.replace('\u{0361}', "")
}

pub(crate) fn hebrew_to_ipa(text: &str) -> String {
    let normalized: String = strip_taamim(text).nfc().collect();
    normalized
        .split_whitespace()
        .map(hebrew_word_to_ipa)
        .collect::<Vec<_>>()
        .join(" ")
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

    #[test]
    fn word_to_segments_bet_patah_is_one_segment_with_a_vowel() {
        let segs = word_to_segments("\u{05D1}\u{05B7}"); // bet + patah, no dagesh
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].onset, vec!["v".to_string()]); // no dagesh -> map_consonant gives "v"
        assert_eq!(segs[0].nucleus, "a");
    }

    #[test]
    fn word_to_segments_shuruk_carries_the_preceding_onset_into_a_bare_u_nucleus() {
        // Verified against piper1-gpl's real `_word_to_segments`: the he's "h"
        // consonant has no vowel of its own, so it carries forward as the onset
        // of the shuruk's "u" segment rather than forming its own segment.
        let segs = word_to_segments("\u{05D4}\u{05D5}\u{05BC}"); // he + vav+dagesh (shuruk)
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].onset, vec!["h".to_string()]);
        assert_eq!(segs[0].nucleus, "u");
    }

    #[test]
    fn word_to_segments_trailing_consonant_becomes_a_coda() {
        let segs = word_to_segments("\u{05D1}\u{05B7}\u{05EA}"); // bet+patah, tav (no vowel)
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].coda, vec!["t".to_string()]);
    }

    #[test]
    fn word_to_segments_sheva_creates_a_placeholder_segment_recording_dagesh() {
        let segs = word_to_segments("\u{05D1}\u{05BC}\u{05B0}"); // bet+dagesh+sheva
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].nucleus, "\u{0259}");
        assert!(segs[0].dagesh);
    }

    #[test]
    fn word_to_segments_geresh_digraph_is_silently_dropped_matching_upstream() {
        // Verified against real upstream piper1-gpl's `_word_to_segments`:
        // it has no special case for the geresh-digraph placeholder glyph.
        // The placeholder falls through to the ordinary consonant path,
        // where `map_consonant` matches no branch on a multi-char base and
        // returns "", so the digraph's sound is silently dropped rather
        // than emitted anywhere in onset/coda.
        let segs = word_to_segments("\u{05D2}\u{05F3}\u{05D1}\u{05B7}"); // gimel+geresh+bet+patah
        let digraph_ipa = "d\u{0361}\u{0292}";
        for seg in &segs {
            assert!(!seg.onset.iter().any(|s| s == digraph_ipa));
            assert!(!seg.coda.iter().any(|s| s == digraph_ipa));
        }
        // The bet+patah still produces its own "a" segment (onset "v",
        // bet has no dagesh); the digraph itself contributes nothing.
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].nucleus, "a");
        assert_eq!(segs[0].onset, vec!["v".to_string()]);
    }

    #[test]
    fn hebrew_word_to_ipa_word_initial_sheva_is_realized_as_e() {
        // bet+sheva, resh+patah, kaf-final (no dagesh anywhere). Bet has no
        // dagesh, so `map_consonant` maps it to "v" (begadkefat
        // spirantization), not "b": this word yields "vˈeʁaχ", with the
        // word-initial sheva resolving to "e" per the `na` (i == 0) rule.
        let ipa = hebrew_word_to_ipa("\u{05D1}\u{05B0}\u{05E8}\u{05B7}\u{05DA}");
        assert!(ipa.starts_with('v'));
        assert!(
            !ipa.contains('\u{0259}'),
            "word-initial sheva must resolve to 'e', not stay a schwa"
        );
    }

    #[test]
    fn hebrew_word_to_ipa_stresses_the_final_syllable_by_default() {
        // "shalom" is exactly two syllables with a closed (consonant-final)
        // last syllable, which trips `syllabify_to_ipa`'s segholate-style
        // exception (stress the *first* syllable instead) rather than
        // exercising the default rule this test names. That exception only
        // fires at exactly two syllables, so "shamayim" (three syllables:
        // sha-ma-yim) is used here instead.
        let ipa = hebrew_word_to_ipa(
            "\u{05E9}\u{05B8}\u{05DE}\u{05B7}\u{05D9}\u{05B4}\u{05DD}", // shamayim
        );
        assert!(ipa.contains('\u{02C8}'), "expected a stress mark somewhere");
        let Some(stress_pos) = ipa.find('\u{02C8}') else {
            panic!("expected a stress mark to be present, checked above");
        };
        assert!(
            stress_pos > ipa.len() / 2,
            "default stress should land on the final syllable"
        );
    }

    #[test]
    fn hebrew_word_to_ipa_keeps_glottal_stop_immediately_before_a_vowel() {
        // ayin, vav+holam (o), lamed+qamats (a), mem-final: "olam".
        let ipa = hebrew_word_to_ipa("\u{05E2}\u{05D5}\u{05B9}\u{05DC}\u{05B8}\u{05DD}");
        assert!(
            ipa.starts_with('\u{0294}'),
            "glottal stop before a vowel must survive, got {ipa:?}"
        );
    }

    #[test]
    fn hebrew_word_to_ipa_drops_glottal_stop_before_a_consonant() {
        // alef (no vowel of its own), then bet+patah: the alef's glottal
        // placeholder is carried into the bet's onset ("ʔv"), immediately
        // followed by a consonant rather than a vowel, so it is dropped.
        let ipa = hebrew_word_to_ipa("\u{05D0}\u{05D1}\u{05B7}");
        assert!(
            !ipa.contains('\u{0294}'),
            "glottal stop before a consonant must be dropped, got {ipa:?}"
        );
        assert_eq!(ipa, "v\u{02C8}a");
    }

    #[test]
    fn hebrew_word_to_ipa_drops_affricate_tie_bars() {
        let ipa = hebrew_word_to_ipa("\u{05E6}\u{05B7}\u{05DC}\u{05DD}"); // tsadi-initial word
        assert!(
            !ipa.contains('\u{0361}'),
            "tie bar must be stripped for Piper's IPA map"
        );
    }

    #[test]
    fn hebrew_to_ipa_joins_multiple_words_with_a_space() {
        let ipa = hebrew_to_ipa(
            "\u{05E9}\u{05B8}\u{05DC}\u{05D5}\u{05B9}\u{05DD} \u{05E2}\u{05D5}\u{05B9}\u{05DC}\u{05B8}\u{05DD}",
        );
        assert!(ipa.contains(' '));
        assert_eq!(ipa.split(' ').count(), 2);
    }

    #[test]
    fn hebrew_to_ipa_empty_input_yields_empty_output() {
        assert_eq!(hebrew_to_ipa(""), "");
    }
}

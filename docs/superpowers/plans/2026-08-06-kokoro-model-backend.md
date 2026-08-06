# Kokoro Model Backend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a second `DengjenModel` backend (`dengjen-kokoro`) implementing Kokoro-class TTS, proving
the trait-based backend architecture generalizes beyond Piper, wired into the CLI frontend so it's
invokable end-to-end.

**Architecture:** New crate `crates/dengjen/models/kokoro`, split into focused modules (`config`,
`vocab`, `phonemize`, `voice_style`, `inference`) rather than one large file. Phonemization is a
single espeak-ng path for all languages (not the two-path misaki-rs/espeak-ng split the spec
originally proposed — disproven by a pre-planning spike, see spec amendment) plus a conversion layer
from raw espeak IPA to Kokoro's phoneme vocabulary. CLI gains a small config-sniffing dispatch step.
Testing is a 3-tier pyramid built in from the start: pure-logic unit tests, a checked-in synthetic
ONNX fixture for fast deterministic inference-plumbing tests, and skippable real-voice e2e tests.

**Tech Stack:** `ort` 2.0.0-rc.13 (matching the workspace), `ndarray` 0.17, `serde`/`serde_json`, the
existing in-repo `espeak-phonemizer` crate. No `misaki-rs` (disproven premise, see spec).

## Global Constraints

- Target trait: `dengjen_core::DengjenModel` — no changes to the trait itself.
- Kokoro does **not** implement `stream_synthesis` (stays the trait's default, returns an error) —
  only `speak_one_sentence`/`speak_batch`. See spec's "Streaming" section for why.
- Errors reuse `dengjen_core::DengjenError`'s existing three variants (`FailedToLoadResource`,
  `PhonemizationError`, `OperationError`) — no new error type.
- **This branch is based on `main`, which does not yet have `[workspace.package]`** (the relicense
  in PR #19 hasn't merged). Do not add `license.workspace = true` or any `license` field to the new
  crate's `Cargo.toml` — every other crate in this branch has no license field either; adding one
  here would be inconsistent and is out of scope for this plan.
- ONNX I/O contract (verified against a real working Rust+ort reference implementation, not
  guessed): inputs `input_ids` (i64, shape `(1, seq_len)`), `style` (f32, shape `(1, 256)`), `speed`
  (f32, shape `(1,)`); output a single f32 waveform tensor.
- The synthetic ONNX fixture (Task 5) uses tensor names `input_ids`/`style`/`speed`/`waveform` and
  was generated and verified to run correctly during planning — its generator script is given
  verbatim in Task 5 and has already been executed successfully once.

---

### Task 1: Scaffold `dengjen-kokoro` crate + config manifest parsing

**Files:**
- Create: `crates/dengjen/models/kokoro/Cargo.toml`
- Create: `crates/dengjen/models/kokoro/src/lib.rs`
- Create: `crates/dengjen/models/kokoro/src/config.rs`
- Modify: `Cargo.toml:4-15` (root workspace `members` list)

**Interfaces:**
- Produces: `pub struct KokoroVoiceConfig { pub model_path: PathBuf, pub voices_dir: PathBuf, pub vocab_path: PathBuf, pub sample_rate: u32, pub voices: Vec<String> }` and `pub fn load_config(config_path: &Path) -> DengjenResult<KokoroVoiceConfig>` in `config.rs`, re-exported from `lib.rs`. Later tasks (2-7) consume `KokoroVoiceConfig`'s fields by name.

- [ ] **Step 1: Create the crate's `Cargo.toml`**

```toml
[package]
name = "dengjen-kokoro"
version = "0.1.0"
edition = "2021"

[features]
default = ["espeak"]
espeak = ["dep:espeak-phonemizer"]

[dependencies]
dengjen-core = { path = "../../core" }
espeak-phonemizer = { path = "../../../text/espeak-phonemizer", optional = true }
serde = { version = "1.0.160", features = ["derive"] }
serde_json = "1.0.89"
ndarray = "0.17"

[dependencies.ort]
version = "2.0.0-rc.13"
default-features = false
features = ["std", "ndarray", "tracing", "download-binaries", "tls-rustls", "copy-dylibs", "api-27"]
```

- [ ] **Step 2: Add the crate to the workspace**

In root `Cargo.toml`, add `"crates/dengjen/models/kokoro",` to the `members` list (currently ends
with `"crates/audio/ops",` — add the new line right after `"crates/dengjen/models/piper",` to keep
model backends grouped together):

```toml
members = [
    "crates/dengjen/core",
    "crates/dengjen/synth",
    "crates/dengjen/models/piper",
    "crates/dengjen/models/kokoro",
    "crates/frontends/grpc",
    "crates/frontends/python",
    "crates/frontends/capi",
    "crates/frontends/cli",
    "crates/text/espeak-phonemizer",
    "crates/audio/sonic-sys",
    "crates/audio/ops",
]
```

- [ ] **Step 3: Write the failing test for config parsing**

Create `crates/dengjen/models/kokoro/src/config.rs`:

```rust
use dengjen_core::{DengjenError, DengjenResult};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize)]
struct RawKokoroVoiceConfig {
    model_type: String,
    model_path: String,
    voices_dir: String,
    vocab_path: String,
    sample_rate: u32,
    voices: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct KokoroVoiceConfig {
    pub model_path: PathBuf,
    pub voices_dir: PathBuf,
    pub vocab_path: PathBuf,
    pub sample_rate: u32,
    pub voices: Vec<String>,
}

pub fn load_config(config_path: &Path) -> DengjenResult<KokoroVoiceConfig> {
    let file = std::fs::File::open(config_path).map_err(|e| {
        DengjenError::FailedToLoadResource(format!(
            "Failed to open Kokoro config at `{}`: {}",
            config_path.display(),
            e
        ))
    })?;
    let raw: RawKokoroVoiceConfig = serde_json::from_reader(file).map_err(|e| {
        DengjenError::FailedToLoadResource(format!(
            "Failed to parse Kokoro config at `{}`: {}",
            config_path.display(),
            e
        ))
    })?;
    if raw.model_type != "kokoro" {
        return Err(DengjenError::FailedToLoadResource(format!(
            "Expected model_type \"kokoro\", got \"{}\"",
            raw.model_type
        )));
    }
    let base_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    Ok(KokoroVoiceConfig {
        model_path: base_dir.join(raw.model_path),
        voices_dir: base_dir.join(raw.voices_dir),
        vocab_path: base_dir.join(raw.vocab_path),
        sample_rate: raw.sample_rate,
        voices: raw.voices,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(dir: &std::path::Path, contents: &str) -> PathBuf {
        let path = dir.join("config.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn load_config_parses_valid_manifest_with_paths_relative_to_config_dir() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_test_valid");
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = write_temp_config(
            &dir,
            r#"{
                "model_type": "kokoro",
                "model_path": "model.onnx",
                "voices_dir": "voices",
                "vocab_path": "tokenizer.json",
                "sample_rate": 24000,
                "voices": ["af_heart", "am_adam"]
            }"#,
        );
        let config = load_config(&config_path).unwrap();
        assert_eq!(config.model_path, dir.join("model.onnx"));
        assert_eq!(config.voices_dir, dir.join("voices"));
        assert_eq!(config.vocab_path, dir.join("tokenizer.json"));
        assert_eq!(config.sample_rate, 24000);
        assert_eq!(config.voices, vec!["af_heart".to_string(), "am_adam".to_string()]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_config_errors_on_malformed_json() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_test_malformed");
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = write_temp_config(&dir, "{ not valid json");
        let result = load_config(&config_path);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_config_errors_on_missing_file() {
        let result = load_config(Path::new("/nonexistent/path/config.json"));
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
    }

    #[test]
    fn load_config_errors_on_wrong_model_type() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_test_wrong_type");
        std::fs::create_dir_all(&dir).unwrap();
        let config_path = write_temp_config(
            &dir,
            r#"{
                "model_type": "piper",
                "model_path": "model.onnx",
                "voices_dir": "voices",
                "vocab_path": "tokenizer.json",
                "sample_rate": 24000,
                "voices": []
            }"#,
        );
        let result = load_config(&config_path);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

Create `crates/dengjen/models/kokoro/src/lib.rs`:

```rust
mod config;

pub use config::{load_config, KokoroVoiceConfig};
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml`
Expected: FAIL to compile initially if the crate isn't registered yet — if Step 2 (workspace
members) and this step are done together, instead expect the 4 tests to run and PASS immediately
(there's no red phase here since the implementation is written alongside the test in this
transcription-style step). Confirm all 4 tests pass.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml`
Expected: `4 passed; 0 failed`

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/dengjen/models/kokoro
git commit -m "kokoro: scaffold crate and config manifest parsing"
```

---

### Task 2: Vocab loading + longest-match tokenizer

**Files:**
- Create: `crates/dengjen/models/kokoro/src/vocab.rs`
- Modify: `crates/dengjen/models/kokoro/src/lib.rs`

**Interfaces:**
- Consumes: nothing from Task 1 directly (vocab loading takes a raw `&Path`, not `KokoroVoiceConfig`, to stay independently testable).
- Produces: `pub struct Vocab { ... }` with `pub fn load(vocab_path: &Path) -> DengjenResult<Vocab>`, `pub fn bos_id(&self) -> i64`, `pub fn eos_id(&self) -> i64`, `pub fn tokenize(&self, phonemes: &str) -> Vec<i64>` (returns token IDs WITHOUT BOS/EOS — callers add those). Task 5 (inference) calls `Vocab::load`, `.tokenize()`, `.bos_id()`, `.eos_id()`.

Real-world Kokoro `tokenizer.json` files are HuggingFace-tokenizers-format JSON with the vocabulary
at `["model"]["vocab"]` (confirmed against a real working Rust reference implementation that parses
this exact structure). BOS/EOS both map to the vocab entry `"$"` (also confirmed against that same
reference implementation).

- [ ] **Step 1: Write the failing test**

```rust
// crates/dengjen/models/kokoro/src/vocab.rs
use dengjen_core::{DengjenError, DengjenResult};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;

pub struct Vocab {
    map: HashMap<String, i64>,
    bos_id: i64,
    max_token_chars: usize,
}

impl Vocab {
    pub fn load(vocab_path: &Path) -> DengjenResult<Self> {
        let file = std::fs::File::open(vocab_path).map_err(|e| {
            DengjenError::FailedToLoadResource(format!(
                "Failed to open Kokoro vocab at `{}`: {}",
                vocab_path.display(),
                e
            ))
        })?;
        let root: Value = serde_json::from_reader(file).map_err(|e| {
            DengjenError::FailedToLoadResource(format!(
                "Failed to parse Kokoro vocab at `{}`: {}",
                vocab_path.display(),
                e
            ))
        })?;
        let vocab_obj = root
            .get("model")
            .and_then(|m| m.get("vocab"))
            .and_then(|v| v.as_object())
            .ok_or_else(|| {
                DengjenError::FailedToLoadResource(format!(
                    "No `model.vocab` object found in `{}`",
                    vocab_path.display()
                ))
            })?;
        let mut map = HashMap::with_capacity(vocab_obj.len());
        for (token, id) in vocab_obj {
            let id = id.as_i64().ok_or_else(|| {
                DengjenError::FailedToLoadResource(format!(
                    "Vocab entry `{}` has a non-integer id in `{}`",
                    token,
                    vocab_path.display()
                ))
            })?;
            map.insert(token.clone(), id);
        }
        let bos_id = *map.get("$").ok_or_else(|| {
            DengjenError::FailedToLoadResource(format!(
                "BOS token `$` not found in vocab `{}`",
                vocab_path.display()
            ))
        })?;
        let max_token_chars = map.keys().map(|k| k.chars().count()).max().unwrap_or(1);
        Ok(Self { map, bos_id, max_token_chars })
    }

    pub fn bos_id(&self) -> i64 {
        self.bos_id
    }

    pub fn eos_id(&self) -> i64 {
        self.bos_id
    }

    /// Longest-match tokenization: at each position, try the longest possible
    /// substring first so multi-character phoneme symbols (e.g. the composed
    /// diphthong tokens produced by the espeak-to-Kokoro conversion) are matched
    /// whole rather than split into unknown single characters.
    pub fn tokenize(&self, phonemes: &str) -> Vec<i64> {
        let chars: Vec<char> = phonemes.chars().collect();
        let mut ids = Vec::with_capacity(chars.len());
        let mut i = 0;
        while i < chars.len() {
            let limit = self.max_token_chars.min(chars.len() - i);
            let mut matched = false;
            for len in (1..=limit).rev() {
                let candidate: String = chars[i..i + len].iter().collect();
                if let Some(&id) = self.map.get(&candidate) {
                    ids.push(id);
                    i += len;
                    matched = true;
                    break;
                }
            }
            if !matched {
                i += 1;
            }
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp_vocab(dir: &std::path::Path, contents: &str) -> std::path::PathBuf {
        let path = dir.join("tokenizer.json");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        path
    }

    const SAMPLE_VOCAB_JSON: &str = r#"{
        "model": {
            "vocab": {
                "$": 0,
                "t": 1,
                "ɛ": 2,
                "s": 3,
                "I": 4,
                "ʤ": 5,
                " ": 6
            }
        }
    }"#;

    #[test]
    fn load_parses_model_vocab_and_finds_bos_token() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_vocab_test_valid");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_vocab(&dir, SAMPLE_VOCAB_JSON);
        let vocab = Vocab::load(&path).unwrap();
        assert_eq!(vocab.bos_id(), 0);
        assert_eq!(vocab.eos_id(), 0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_errors_when_model_vocab_missing() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_vocab_test_missing_vocab");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_vocab(&dir, r#"{"model": {}}"#);
        let result = Vocab::load(&path);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_errors_when_bos_token_absent() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_vocab_test_no_bos");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_vocab(&dir, r#"{"model": {"vocab": {"t": 1}}}"#);
        let result = Vocab::load(&path);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tokenize_matches_single_char_symbols() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_vocab_test_tokenize_single");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_vocab(&dir, SAMPLE_VOCAB_JSON);
        let vocab = Vocab::load(&path).unwrap();
        // "test" phoneme string using single-char vocab entries: t, ɛ, s, t
        assert_eq!(vocab.tokenize("tɛst"), vec![1, 2, 3, 1]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tokenize_prefers_longest_match_over_single_chars() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_vocab_test_tokenize_longest");
        std::fs::create_dir_all(&dir).unwrap();
        // Vocab has both "ʤ" (composed) and no single-char entries that could
        // spuriously combine to match a longer string - this proves longest-match
        // picks the whole multi-codepoint symbol "ʤ" as one token (id 5), not
        // some other decomposition.
        let path = write_temp_vocab(&dir, SAMPLE_VOCAB_JSON);
        let vocab = Vocab::load(&path).unwrap();
        assert_eq!(vocab.tokenize("ʤ"), vec![5]);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn tokenize_skips_unknown_characters() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_vocab_test_tokenize_unknown");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_vocab(&dir, SAMPLE_VOCAB_JSON);
        let vocab = Vocab::load(&path).unwrap();
        // "z" is not in the sample vocab - it should be silently skipped, and
        // the surrounding known characters still tokenize correctly.
        assert_eq!(vocab.tokenize("tzs"), vec![1, 3]);
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

Add to `crates/dengjen/models/kokoro/src/lib.rs`:

```rust
mod config;
mod vocab;

pub use config::{load_config, KokoroVoiceConfig};
pub use vocab::Vocab;
```

- [ ] **Step 2: Run test to verify it passes** (written alongside implementation, as in Task 1)

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml vocab::`
Expected: `6 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
git add crates/dengjen/models/kokoro/src/vocab.rs crates/dengjen/models/kokoro/src/lib.rs
git commit -m "kokoro: vocab loading and longest-match tokenizer"
```

---

### Task 3: espeak IPA → Kokoro phoneme conversion

**Files:**
- Create: `crates/dengjen/models/kokoro/src/phonemize.rs`
- Modify: `crates/dengjen/models/kokoro/src/lib.rs`

**Interfaces:**
- Consumes: `espeak_phonemizer::text_to_phonemes` (existing crate, existing signature:
  `text_to_phonemes(text: &str, language: &str, phoneme_separator: Option<char>, remove_lang_switch_flags: bool, remove_stress: bool) -> ESpeakResult<Vec<String>>`).
- Produces: `pub fn text_to_kokoro_phonemes(text: &str, language: &str) -> DengjenResult<String>` in
  `phonemize.rs`. Task 5 (inference) calls this, then passes its output to `Vocab::tokenize`.

**Important — do not add a TIE-marker mode to `espeak-phonemizer`.** An earlier investigation during
planning confirmed the vendored espeak-ng in this repo produces no combining tie bars regardless of
that flag, so the conversion table below is written for plain (no-tie) IPA output using the crate's
existing `text_to_phonemes` function exactly as Piper already calls it — no changes to
`espeak-phonemizer` itself.

The substitution table converts espeak's raw IPA into Kokoro's phoneme notation (composed
diphthongs/affricates as single symbols, matching Kokoro's actual trained vocabulary — derived from
a real reference implementation's target mappings, adapted for the no-tie-bar reality confirmed
above). Order matters: longer patterns must be tried before shorter ones that could spuriously match
part of them (e.g. `"eɪ"` before a bare `"e"` rule) — this implementation applies replacements in a
fixed longest-first order, not by iterating a `HashMap` (whose iteration order is unspecified).

- [ ] **Step 1: Write the failing test**

```rust
// crates/dengjen/models/kokoro/src/phonemize.rs
use dengjen_core::{DengjenError, DengjenResult};

/// Ordered longest-pattern-first. Each entry is (espeak IPA substring, Kokoro phoneme symbol).
const SUBSTITUTIONS: &[(&str, &str)] = &[
    ("aɪ", "I"),
    ("aʊ", "W"),
    ("dʒ", "ʤ"),
    ("eɪ", "A"),
    ("tʃ", "ʧ"),
    ("ɔɪ", "Y"),
    ("oʊ", "O"),
    ("ɚ", "əɹ"),
    ("r", "ɹ"),
    ("x", "k"),
    ("ç", "k"),
    ("ɐ", "ə"),
    ("ɬ", "l"),
    ("ʔ", "t"),
    ("ʲ", ""),
    ("ː", ""),
];

fn espeak_ipa_to_kokoro(ipa: &str) -> String {
    let mut result = ipa.to_string();
    for (from, to) in SUBSTITUTIONS {
        result = result.replace(from, to);
    }
    result
}

pub fn text_to_kokoro_phonemes(text: &str, language: &str) -> DengjenResult<String> {
    let sentences = espeak_phonemizer::text_to_phonemes(text, language, None, false, false)
        .map_err(|e| DengjenError::PhonemizationError(e.to_string()))?;
    Ok(sentences
        .iter()
        .map(|s| espeak_ipa_to_kokoro(s))
        .collect::<Vec<_>>()
        .join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Expected raw IPA values below were captured by actually running this repo's
    // vendored espeak-ng (via espeak_phonemizer::text_to_phonemes) during planning,
    // not invented - see plan Task 3 for how to reproduce.
    #[test]
    fn espeak_ipa_to_kokoro_composes_ai_diphthong() {
        // espeak IPA for "time" is "tˈaɪm" (verified against real espeak-ng)
        assert_eq!(espeak_ipa_to_kokoro("tˈaɪm"), "tˈIm");
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_dz_affricate() {
        // espeak IPA for "job" is "dʒˈɑːb" (verified against real espeak-ng);
        // the length mark on ɑː is also stripped.
        assert_eq!(espeak_ipa_to_kokoro("dʒˈɑːb"), "ʤˈɑb");
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_oi_diphthong() {
        // espeak IPA for "toy" is "tˈɔɪ" (verified against real espeak-ng)
        assert_eq!(espeak_ipa_to_kokoro("tˈɔɪ"), "tˈY");
    }

    #[test]
    fn espeak_ipa_to_kokoro_composes_au_diphthong() {
        // espeak IPA for "house" is "hˈaʊs" (verified against real espeak-ng)
        assert_eq!(espeak_ipa_to_kokoro("hˈaʊs"), "hˈWs");
    }

    #[test]
    fn espeak_ipa_to_kokoro_leaves_plain_phonemes_unchanged() {
        // espeak IPA for "test" is "tˈɛst" (verified against real espeak-ng) - no
        // diphthongs/affricates/length-marks present, so nothing should change.
        assert_eq!(espeak_ipa_to_kokoro("tˈɛst"), "tˈɛst");
    }

    #[test]
    fn text_to_kokoro_phonemes_returns_error_for_unset_voice() {
        // An unrecognized espeak-ng language code should surface as a
        // PhonemizationError, not panic.
        let result = text_to_kokoro_phonemes("hello", "not-a-real-language-code");
        assert!(matches!(result, Err(DengjenError::PhonemizationError(_))));
    }
}
```

Add to `crates/dengjen/models/kokoro/src/lib.rs`:

```rust
mod config;
mod phonemize;
mod vocab;

pub use config::{load_config, KokoroVoiceConfig};
pub use phonemize::text_to_kokoro_phonemes;
pub use vocab::Vocab;
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml phonemize::`
Expected: `6 passed; 0 failed`. If any of the 5 substitution tests fail, do not adjust the expected
value to match whatever the code produces — the expected values are real captured espeak-ng output;
a failure means the substitution logic is wrong, not the test.

- [ ] **Step 3: Verify the syllabic-consonant case is handled or explicitly deferred**

The substitution table above does not yet handle espeak's syllabic-consonant diacritic (combining
U+0329, appearing in words like "button" → roughly `bˈʌtn̩`, where the trailing `n̩` marks a syllabic
nasal). This wasn't exercised by the 5 words verified during planning. Run:

```bash
cargo run --manifest-path crates/dengjen/models/kokoro/Cargo.toml --example probe_syllabic 2>/dev/null || true
```

If no such example exists yet, instead add a temporary `#[test]` that calls
`espeak_phonemizer::text_to_phonemes("button", "en-US", None, false, false)` directly and prints the
result with `eprintln!`, run it with `-- --nocapture`, observe whether U+0329 appears in the output,
then delete the temporary test. If it does appear, add a rule to `SUBSTITUTIONS` that maps the
syllabic-consonant sequence to Kokoro's `ᵊ`-prefixed convention (e.g. `"n\u{0329}"` → `"ᵊn"`) and a
real test case using the observed value, following the same pattern as the other substitution tests
above. If it does not appear (espeak may represent syllabic consonants differently, or not at all,
in this configuration), note that in your task report instead of guessing a rule for input that
never occurs.

- [ ] **Step 4: Commit**

```bash
git add crates/dengjen/models/kokoro/src/phonemize.rs crates/dengjen/models/kokoro/src/lib.rs
git commit -m "kokoro: espeak IPA to Kokoro phoneme conversion"
```

---

### Task 4: Voice style vector loading

**Files:**
- Create: `crates/dengjen/models/kokoro/src/voice_style.rs`
- Modify: `crates/dengjen/models/kokoro/src/lib.rs`

**Interfaces:**
- Produces: `pub struct VoiceStyles { ... }` with `pub fn load(voices_dir: &Path, voices: &[String]) -> DengjenResult<Self>` and `pub fn style_for(&self, voice_name: &str, token_len: usize) -> DengjenResult<ndarray::Array2<f32>>` (shape `(1, 256)`). Task 5 (inference) calls this to get the `style` tensor input.

**The byte layout was confirmed empirically during planning against a real downloaded voice file**
(`onnx-community/Kokoro-82M-v1.0-ONNX`'s `voices/af_heart.bin`), not assumed: each voice is one
separate binary file, `<voices_dir>/<voice_name>.bin` — no header, no metadata, exactly `510 × 256`
little-endian `f32` values in row-major order (verified: `510 × 256 × 4 = 522,240` bytes, matching
the real downloaded file's size exactly). Row index is the token-length-conditioning axis; each row
is that length's 256-dim style vector.

- [ ] **Step 1: Write the failing test**

```rust
// crates/dengjen/models/kokoro/src/voice_style.rs
use dengjen_core::{DengjenError, DengjenResult};
use ndarray::Array2;
use std::collections::HashMap;
use std::path::Path;

const STYLE_DIM: usize = 256;
const MAX_TOKEN_LEN: usize = 510;
const EXPECTED_FILE_BYTES: usize = MAX_TOKEN_LEN * STYLE_DIM * 4;

pub struct VoiceStyles {
    per_voice: HashMap<String, Array2<f32>>,
}

impl VoiceStyles {
    pub fn load(voices_dir: &Path, voices: &[String]) -> DengjenResult<Self> {
        let mut per_voice = HashMap::with_capacity(voices.len());
        for voice_name in voices {
            let path = voices_dir.join(format!("{voice_name}.bin"));
            let bytes = std::fs::read(&path).map_err(|e| {
                DengjenError::FailedToLoadResource(format!(
                    "Failed to read Kokoro voice style file `{}`: {}",
                    path.display(),
                    e
                ))
            })?;
            if bytes.len() != EXPECTED_FILE_BYTES {
                return Err(DengjenError::FailedToLoadResource(format!(
                    "Kokoro voice style file `{}` is {} bytes, expected {} ({} rows x {} dims x 4 bytes)",
                    path.display(),
                    bytes.len(),
                    EXPECTED_FILE_BYTES,
                    MAX_TOKEN_LEN,
                    STYLE_DIM
                )));
            }
            let floats: Vec<f32> = bytes
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                .collect();
            let table = Array2::from_shape_vec((MAX_TOKEN_LEN, STYLE_DIM), floats)
                .map_err(|e| DengjenError::with_message(e.to_string()))?;
            per_voice.insert(voice_name.clone(), table);
        }
        Ok(Self { per_voice })
    }

    pub fn style_for(&self, voice_name: &str, token_len: usize) -> DengjenResult<Array2<f32>> {
        let table = self.per_voice.get(voice_name).ok_or_else(|| {
            DengjenError::OperationError(format!("Unknown Kokoro voice: `{}`", voice_name))
        })?;
        let row_index = token_len.saturating_sub(1).min(MAX_TOKEN_LEN - 1);
        Ok(table
            .slice(ndarray::s![row_index..row_index + 1, ..])
            .to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Writes a synthetic voice style file with the real 510x256 f32 shape, where
    /// row `r`'s 256 values are all `r as f32` - makes it trivial to assert which
    /// row `style_for` picked without needing a real trained voice file.
    fn write_synthetic_voice_file(dir: &Path, voice_name: &str) -> std::path::PathBuf {
        let path = dir.join(format!("{voice_name}.bin"));
        let mut bytes = Vec::with_capacity(EXPECTED_FILE_BYTES);
        for row in 0..MAX_TOKEN_LEN {
            for _ in 0..STYLE_DIM {
                bytes.extend_from_slice(&(row as f32).to_le_bytes());
            }
        }
        std::fs::write(&path, &bytes).unwrap();
        path
    }

    #[test]
    fn load_reads_a_correctly_shaped_voice_file() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_voice_style_test_valid");
        std::fs::create_dir_all(&dir).unwrap();
        write_synthetic_voice_file(&dir, "test_voice");
        let styles = VoiceStyles::load(&dir, &["test_voice".to_string()]).unwrap();
        let row0 = styles.style_for("test_voice", 1).unwrap();
        assert_eq!(row0.shape(), &[1, 256]);
        assert_eq!(row0[[0, 0]], 0.0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_errors_when_voice_file_missing() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_voice_style_test_missing");
        std::fs::create_dir_all(&dir).unwrap();
        let result = VoiceStyles::load(&dir, &["nonexistent_voice".to_string()]);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn load_errors_when_voice_file_is_wrong_size() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_voice_style_test_wrong_size");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("bad_voice.bin"), vec![0u8; 100]).unwrap();
        let result = VoiceStyles::load(&dir, &["bad_voice".to_string()]);
        assert!(matches!(result, Err(DengjenError::FailedToLoadResource(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn style_for_unknown_voice_returns_operation_error() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_voice_style_test_unknown");
        std::fs::create_dir_all(&dir).unwrap();
        write_synthetic_voice_file(&dir, "known_voice");
        let styles = VoiceStyles::load(&dir, &["known_voice".to_string()]).unwrap();
        let result = styles.style_for("nonexistent_voice", 5);
        assert!(matches!(result, Err(DengjenError::OperationError(_))));
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn style_for_returns_the_row_matching_token_length() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_voice_style_test_row_select");
        std::fs::create_dir_all(&dir).unwrap();
        write_synthetic_voice_file(&dir, "test_voice");
        let styles = VoiceStyles::load(&dir, &["test_voice".to_string()]).unwrap();
        // token_len 42 should select row index 41 (token_len - 1), whose synthetic
        // value is 41.0 in every column.
        let result = styles.style_for("test_voice", 42).unwrap();
        assert_eq!(result[[0, 0]], 41.0);
        assert_eq!(result[[0, 255]], 41.0);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn style_for_clamps_token_len_to_available_rows() {
        let dir = std::env::temp_dir().join("dengjen_kokoro_voice_style_test_clamp");
        std::fs::create_dir_all(&dir).unwrap();
        write_synthetic_voice_file(&dir, "test_voice");
        let styles = VoiceStyles::load(&dir, &["test_voice".to_string()]).unwrap();
        // token_len 10000 exceeds the 510 available rows - must clamp to the last
        // row (index 509, synthetic value 509.0), not panic or index out of bounds.
        let result = styles.style_for("test_voice", 10000).unwrap();
        assert_eq!(result.shape(), &[1, 256]);
        assert_eq!(result[[0, 0]], 509.0);
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml voice_style::`
Expected: `6 passed; 0 failed`

- [ ] **Step 3: Commit**

```bash
git add crates/dengjen/models/kokoro/src/voice_style.rs crates/dengjen/models/kokoro/src/lib.rs
git commit -m "kokoro: voice style vector loading"
```

Update `crates/dengjen/models/kokoro/src/lib.rs` to add `mod voice_style; pub use voice_style::VoiceStyles;` alongside the existing module declarations.

---

### Task 5: ONNX inference + `DengjenModel` implementation + Tier 2 synthetic fixture

**Files:**
- Create: `crates/dengjen/models/kokoro/src/inference.rs`
- Modify: `crates/dengjen/models/kokoro/src/lib.rs`
- Create: `crates/dengjen/models/kokoro/tests/fixtures/synthetic_kokoro.onnx` (binary, generated per Step 1 below)
- Test: `crates/dengjen/models/kokoro/tests/synthetic_inference.rs`

**Interfaces:**
- Consumes: `KokoroVoiceConfig` (Task 1), `Vocab` (Task 2), `text_to_kokoro_phonemes` (Task 3), `VoiceStyles` (Task 4).
- Produces: `pub struct KokoroModel { ... }` implementing `dengjen_core::DengjenModel`, and `pub fn from_config_path(config_path: &Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>>` in `lib.rs` — the entry point Task 6 (CLI) calls.

- [ ] **Step 1: Generate the synthetic ONNX test fixture**

This script was written and verified during planning (built, checked, and run successfully via
`onnxruntime` producing the expected `(1, 16000)` output). Set up a Python environment and run it
exactly as given:

```bash
python3 -m venv /tmp/kokoro-fixture-venv
/tmp/kokoro-fixture-venv/bin/pip install onnx numpy
```

Save as `/tmp/gen_synthetic_kokoro.py`:

```python
import onnx
from onnx import helper, TensorProto

input_ids = helper.make_tensor_value_info("input_ids", TensorProto.INT64, ["batch", "seq_len"])
style = helper.make_tensor_value_info("style", TensorProto.FLOAT, ["batch", 256])
speed = helper.make_tensor_value_info("speed", TensorProto.FLOAT, ["batch"])
waveform = helper.make_tensor_value_info("waveform", TensorProto.FLOAT, ["batch", "num_samples"])

seq_len_node = helper.make_node("Shape", ["input_ids"], ["input_shape"])
seq_len_scalar = helper.make_node(
    "Gather", ["input_shape", "seq_len_index"], ["seq_len_scalar"]
)
seq_len_index_init = helper.make_tensor("seq_len_index", TensorProto.INT64, [], [1])

style_sum_node = helper.make_node("ReduceSum", ["style", "style_sum_axes"], ["style_sum"], keepdims=1)
style_sum_axes_init = helper.make_tensor("style_sum_axes", TensorProto.INT64, [1], [1])
seq_len_float_node = helper.make_node("Cast", ["seq_len_scalar"], ["seq_len_float"], to=TensorProto.FLOAT)
seq_len_float_reshaped_node = helper.make_node(
    "Reshape", ["seq_len_float", "scalar_shape"], ["seq_len_float_reshaped"]
)
scalar_shape_init = helper.make_tensor("scalar_shape", TensorProto.INT64, [1], [1])

base_node = helper.make_node("Mul", ["style_sum", "seq_len_float_reshaped"], ["base"])
scaled_node = helper.make_node("Mul", ["base", "speed"], ["scaled"])

repeats_node = helper.make_node(
    "Concat", ["ones_batch_dim", "num_samples_dim"], ["tile_repeats"], axis=0
)
ones_batch_dim_init = helper.make_tensor("ones_batch_dim", TensorProto.INT64, [1], [1])
num_samples_dim_init = helper.make_tensor("num_samples_dim", TensorProto.INT64, [1], [16000])

scaled_reshaped_node = helper.make_node(
    "Reshape", ["scaled", "scaled_shape"], ["scaled_reshaped"]
)
scaled_shape_init = helper.make_tensor("scaled_shape", TensorProto.INT64, [2], [-1, 1])

tile_node = helper.make_node("Tile", ["scaled_reshaped", "tile_repeats"], ["waveform"])

graph = helper.make_graph(
    nodes=[
        seq_len_node,
        seq_len_scalar,
        style_sum_node,
        seq_len_float_node,
        seq_len_float_reshaped_node,
        base_node,
        scaled_node,
        scaled_reshaped_node,
        repeats_node,
        tile_node,
    ],
    name="synthetic-kokoro-fixture",
    inputs=[input_ids, style, speed],
    outputs=[waveform],
    initializer=[
        seq_len_index_init,
        style_sum_axes_init,
        scalar_shape_init,
        ones_batch_dim_init,
        num_samples_dim_init,
        scaled_shape_init,
    ],
)

model = helper.make_model(graph, producer_name="dengjen-kokoro-test-fixture")
model.opset_import[0].version = 17
onnx.checker.check_model(model)
onnx.save(model, "synthetic_kokoro.onnx")
print("OK: model checked and saved")
```

Run it and copy the result into the repo:

```bash
/tmp/kokoro-fixture-venv/bin/python3 /tmp/gen_synthetic_kokoro.py
mkdir -p crates/dengjen/models/kokoro/tests/fixtures
cp synthetic_kokoro.onnx crates/dengjen/models/kokoro/tests/fixtures/synthetic_kokoro.onnx
```

This graph is a deliberately trivial placeholder computation (not real speech synthesis) — it takes
`sum(style) * seq_len * speed` and tiles that scalar into a fixed 16000-sample output. It exists only
to prove the Rust plumbing (tensor construction → `session.run` → output extraction) works against
the real Kokoro I/O contract, not to produce meaningful audio.

- [ ] **Step 2: Write `inference.rs` and the `DengjenModel` implementation**

```rust
// crates/dengjen/models/kokoro/src/inference.rs
use crate::config::KokoroVoiceConfig;
use crate::phonemize::text_to_kokoro_phonemes;
use crate::voice_style::VoiceStyles;
use crate::vocab::Vocab;
use dengjen_core::{
    Audio, AudioInfo, DengjenAudioResult, DengjenError, DengjenModel, DengjenResult, Phonemes,
};
use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::Tensor;
use std::any::Any;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct KokoroModel {
    session: Mutex<Session>,
    vocab: Vocab,
    voice_styles: VoiceStyles,
    sample_rate: u32,
    voices: Vec<String>,
    default_voice: String,
}

impl KokoroModel {
    pub fn from_config(config: KokoroVoiceConfig) -> DengjenResult<Self> {
        let session = Session::builder()
            .map_err(|e| DengjenError::FailedToLoadResource(e.to_string()))?
            .commit_from_file(&config.model_path)
            .map_err(|e| {
                DengjenError::FailedToLoadResource(format!(
                    "Failed to load Kokoro ONNX model at `{}`: {}",
                    config.model_path.display(),
                    e
                ))
            })?;
        let vocab = Vocab::load(&config.vocab_path)?;
        let voice_styles = VoiceStyles::load(&config.voices_dir, &config.voices)?;
        let default_voice = config
            .voices
            .first()
            .cloned()
            .ok_or_else(|| DengjenError::FailedToLoadResource("No voices in config".to_string()))?;
        Ok(Self {
            session: Mutex::new(session),
            vocab,
            voice_styles,
            sample_rate: config.sample_rate,
            voices: config.voices,
            default_voice,
        })
    }

    fn synthesize_phonemes(&self, phonemes: &str) -> DengjenAudioResult {
        let mut token_ids = vec![self.vocab.bos_id()];
        token_ids.extend(self.vocab.tokenize(phonemes));
        token_ids.push(self.vocab.eos_id());

        let input_ids = Array2::from_shape_vec((1, token_ids.len()), token_ids.clone())
            .map_err(|e| DengjenError::with_message(e.to_string()))?;
        let style = self
            .voice_styles
            .style_for(&self.default_voice, token_ids.len())?;
        let speed = Array1::from_vec(vec![1.0f32]);

        let mut session = self.session.lock().unwrap();
        let outputs = session
            .run(ort::inputs![
                Tensor::from_array(input_ids).map_err(|e| DengjenError::with_message(e.to_string()))?,
                Tensor::from_array(style).map_err(|e| DengjenError::with_message(e.to_string()))?,
                Tensor::from_array(speed).map_err(|e| DengjenError::with_message(e.to_string()))?,
            ])
            .map_err(|e| DengjenError::OperationError(format!("Kokoro inference failed: {}", e)))?;
        let (_, data) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| DengjenError::OperationError(format!("Failed to extract Kokoro output: {}", e)))?;
        Ok(Audio::new(data.to_vec().into(), self.sample_rate as usize, None))
    }
}

impl DengjenModel for KokoroModel {
    fn audio_output_info(&self) -> DengjenResult<AudioInfo> {
        Ok(AudioInfo {
            sample_rate: self.sample_rate as usize,
            num_channels: 1,
            sample_width: 2,
        })
    }

    fn phonemize_text(&self, text: &str) -> DengjenResult<Phonemes> {
        let language = "en-US"; // Task 6 revisits per-voice language selection if needed.
        let phonemes = text_to_kokoro_phonemes(text, language)?;
        Ok(Phonemes::from(vec![phonemes]))
    }

    fn speak_batch(&self, phoneme_batches: Vec<String>) -> DengjenResult<Vec<Audio>> {
        phoneme_batches
            .into_iter()
            .map(|p| self.synthesize_phonemes(&p))
            .collect()
    }

    fn speak_one_sentence(&self, phonemes: String) -> DengjenAudioResult {
        self.synthesize_phonemes(&phonemes)
    }

    fn get_default_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(()))
    }

    fn get_fallback_synthesis_config(&self) -> DengjenResult<Box<dyn Any>> {
        Ok(Box::new(()))
    }

    fn set_fallback_synthesis_config(&self, _synthesis_config: &dyn Any) -> DengjenResult<()> {
        Ok(())
    }

    fn get_speakers(&self) -> DengjenResult<Option<&HashMap<i64, String>>> {
        Ok(None)
    }
}
```

Update `crates/dengjen/models/kokoro/src/lib.rs`:

```rust
mod config;
mod inference;
mod phonemize;
mod voice_style;
mod vocab;

use dengjen_core::{DengjenModel, DengjenResult};
use std::path::Path;
use std::sync::Arc;

pub use config::{load_config, KokoroVoiceConfig};
pub use inference::KokoroModel;
pub use phonemize::text_to_kokoro_phonemes;
pub use voice_style::VoiceStyles;
pub use vocab::Vocab;

pub fn from_config_path(config_path: &Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>> {
    let config = load_config(config_path)?;
    let model = KokoroModel::from_config(config)?;
    Ok(Arc::new(model))
}
```

**Note on `get_speakers`:** Kokoro has multiple named voices (not numeric speaker IDs like Piper),
which doesn't map cleanly onto `DengjenModel::get_speakers`'s `HashMap<i64, String>` shape. Returning
`None` here is a deliberate simplification for v1 — voice selection isn't wired end-to-end in this
plan (Task 6's CLI dispatch loads the config's first/default voice only). If per-voice selection is
needed later, that's a follow-up, not silently expanded into this task.

- [ ] **Step 3: Write the Tier 2 synthetic-fixture test**

Integration tests under `tests/` only see the crate's `pub` API (not `pub(crate)` items), so this
test goes through `KokoroModel::from_config` and the public `DengjenModel::speak_one_sentence`
trait method directly with a hand-built phoneme string — it does not need real espeak-ng
phonemization, since `speak_one_sentence` already takes phonemes, not raw text.

```rust
// crates/dengjen/models/kokoro/tests/synthetic_inference.rs
use dengjen_core::DengjenModel;
use dengjen_kokoro::{KokoroModel, KokoroVoiceConfig};
use std::io::Write;
use std::path::PathBuf;

// This test exercises the real inference plumbing (tensor construction, session.run,
// output extraction) against the checked-in synthetic fixture from Task 5 Step 1 - it
// does not assert anything about real speech quality, only that the pipeline runs and
// produces the expected shape/type of output.

const STYLE_DIM: usize = 256;
const MAX_TOKEN_LEN: usize = 510;

fn write_synthetic_voice_file(dir: &std::path::Path, voice_name: &str) {
    let path = dir.join(format!("{voice_name}.bin"));
    let mut bytes = Vec::with_capacity(MAX_TOKEN_LEN * STYLE_DIM * 4);
    for row in 0..MAX_TOKEN_LEN {
        for _ in 0..STYLE_DIM {
            bytes.extend_from_slice(&(row as f32).to_le_bytes());
        }
    }
    std::fs::write(&path, &bytes).unwrap();
}

fn write_minimal_vocab(dir: &std::path::Path) -> PathBuf {
    let path = dir.join("tokenizer.json");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(
        br#"{"model": {"vocab": {"$": 0, "t": 1, "ɛ": 2, "s": 3}}}"#,
    )
    .unwrap();
    path
}

#[test]
fn synthesizes_against_synthetic_fixture_without_panicking() {
    let dir = std::env::temp_dir().join("dengjen_kokoro_synthetic_inference_test");
    std::fs::create_dir_all(&dir).unwrap();
    let voices_dir = dir.join("voices");
    std::fs::create_dir_all(&voices_dir).unwrap();
    write_synthetic_voice_file(&voices_dir, "test_voice");
    let vocab_path = write_minimal_vocab(&dir);

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let model_path = manifest_dir.join("tests/fixtures/synthetic_kokoro.onnx");

    let config = KokoroVoiceConfig {
        model_path,
        voices_dir,
        vocab_path,
        sample_rate: 24000,
        voices: vec!["test_voice".to_string()],
    };
    let model = KokoroModel::from_config(config).expect("failed to load synthetic Kokoro model");

    // "tɛst" phonemes (U+025B is ɛ), tokenizes against the minimal vocab above.
    let audio = model
        .speak_one_sentence("t\u{025b}st".to_string())
        .expect("synthesis against synthetic fixture failed");

    assert_eq!(audio.info.sample_rate, 24000);
    let samples = audio.samples.into_vec();
    assert!(!samples.is_empty(), "expected non-empty output samples");
    // The synthetic graph always outputs exactly 16000 samples (see Task 5 Step 1's
    // generator script) - not asserting sample VALUES, since they're an arbitrary
    // placeholder computation, not real audio.
    assert_eq!(samples.len(), 16000);

    std::fs::remove_dir_all(&dir).ok();
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml`
Expected: all unit tests (Tasks 1-4) plus the new synthetic-fixture integration test pass.

- [ ] **Step 5: Commit**

```bash
git add crates/dengjen/models/kokoro/src/inference.rs crates/dengjen/models/kokoro/src/lib.rs crates/dengjen/models/kokoro/tests/
git commit -m "kokoro: ONNX inference, DengjenModel impl, synthetic fixture test"
```

---

### Task 6: CLI auto-detect dispatch

**Files:**
- Modify: `crates/frontends/cli/src/main.rs`
- Modify: `crates/frontends/cli/Cargo.toml` (add `dengjen-kokoro` dependency)

**Interfaces:**
- Consumes: `dengjen_kokoro::from_config_path(&Path) -> DengjenResult<Arc<dyn DengjenModel + Send + Sync>>` (Task 5), `dengjen_piper::from_config_path` (existing).

- [ ] **Step 1: Add the dependency**

In `crates/frontends/cli/Cargo.toml`, add to `[dependencies]`:

```toml
dengjen-kokoro = { version = "0.1.0", path = "../../dengjen/models/kokoro" }
```

- [ ] **Step 2: Write the failing test for dispatch logic**

Read `crates/frontends/cli/src/main.rs` around where `dengjen_piper::from_config_path(&args.config)?`
is currently called (line 210, per earlier investigation) to see the exact surrounding voice-loading
code before editing. Add a helper function and its test:

```rust
fn detect_model_type(config_path: &std::path::Path) -> anyhow::Result<String> {
    let contents = std::fs::read_to_string(config_path)?;
    let value: serde_json::Value = serde_json::from_str(&contents)?;
    Ok(value
        .get("model_type")
        .and_then(|v| v.as_str())
        .unwrap_or("piper")
        .to_string())
}

fn load_voice(config_path: &std::path::Path) -> anyhow::Result<std::sync::Arc<dyn dengjen_synth::DengjenModel + Send + Sync>> {
    match detect_model_type(config_path)?.as_str() {
        "kokoro" => Ok(dengjen_kokoro::from_config_path(config_path)?),
        _ => Ok(dengjen_piper::from_config_path(config_path)?),
    }
}

#[cfg(test)]
mod dispatch_tests {
    use super::*;
    use std::io::Write;

    fn write_temp_config(dir: &std::path::Path, name: &str, contents: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(contents.as_bytes()).unwrap();
        path
    }

    #[test]
    fn detect_model_type_recognizes_kokoro() {
        let dir = std::env::temp_dir().join("dengjen_cli_dispatch_test_kokoro");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_config(&dir, "config.json", r#"{"model_type": "kokoro"}"#);
        assert_eq!(detect_model_type(&path).unwrap(), "kokoro");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_model_type_defaults_to_piper_when_field_absent() {
        // Real Piper .onnx.json configs have no model_type field at all.
        let dir = std::env::temp_dir().join("dengjen_cli_dispatch_test_piper_default");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_config(&dir, "config.json", r#"{"audio": {"sample_rate": 22050}}"#);
        assert_eq!(detect_model_type(&path).unwrap(), "piper");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn detect_model_type_errors_on_malformed_json() {
        let dir = std::env::temp_dir().join("dengjen_cli_dispatch_test_malformed");
        std::fs::create_dir_all(&dir).unwrap();
        let path = write_temp_config(&dir, "config.json", "{ not valid");
        assert!(detect_model_type(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}
```

- [ ] **Step 3: Wire `load_voice` into the existing call site**

Replace the existing `let voice = dengjen_piper::from_config_path(&args.config)?;` (main.rs:210) with:

```rust
let voice = load_voice(&args.config)?;
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --manifest-path crates/frontends/cli/Cargo.toml dispatch_tests::`
Expected: `3 passed; 0 failed`

Run: `cargo build --manifest-path crates/frontends/cli/Cargo.toml`
Expected: clean build — confirms `load_voice`'s return type unifies both backends correctly (both
`dengjen_piper::from_config_path` and `dengjen_kokoro::from_config_path` must return the exact same
`DengjenResult<Arc<dyn DengjenModel + Send + Sync>>` type for this to compile without additional
conversion).

- [ ] **Step 5: Commit**

```bash
git add crates/frontends/cli/Cargo.toml crates/frontends/cli/src/main.rs
git commit -m "cli: auto-detect Piper vs Kokoro voice configs"
```

---

### Task 7: Tier 3 end-to-end tests (skippable, real voice required)

**Files:**
- Create: `crates/dengjen/models/kokoro/tests/e2e_real_voice.rs`
- Create: `crates/frontends/cli/tests/kokoro_e2e.rs`

**Interfaces:**
- Consumes: `dengjen_kokoro::from_config_path` (Task 5), the CLI binary (Task 6).

These tests require a real downloaded Kokoro voice (model + per-voice `.bin` style files +
`tokenizer.json`) at a
path supplied via an environment variable — they skip gracefully when it's absent, exactly matching
this repo's existing convention for Piper's own real-voice tests (README's documented testing
caveat: real model files are gitignored and not present in a clean checkout).

- [ ] **Step 1: Write the skippable crate-level e2e test**

```rust
// crates/dengjen/models/kokoro/tests/e2e_real_voice.rs
use dengjen_core::DengjenModel;

fn real_voice_config_path() -> Option<std::path::PathBuf> {
    std::env::var("DENGJEN_KOKORO_TEST_VOICE_CONFIG")
        .ok()
        .map(std::path::PathBuf::from)
}

#[test]
fn synthesizes_real_audio_from_a_real_voice() {
    let Some(config_path) = real_voice_config_path() else {
        eprintln!("Skipping: set DENGJEN_KOKORO_TEST_VOICE_CONFIG to a real Kokoro voice config to run this test");
        return;
    };
    let model = dengjen_kokoro::from_config_path(&config_path).expect("failed to load real Kokoro voice");
    let phonemes = model.phonemize_text("Hello, world!").expect("phonemization failed");
    let audio = model
        .speak_one_sentence(phonemes.to_string())
        .expect("synthesis failed");
    assert!(!audio.samples.into_vec().is_empty(), "expected non-empty audio samples");
}
```

- [ ] **Step 2: Write the skippable CLI subprocess e2e test**

Read `crates/frontends/cli/src/main.rs` and any existing test infrastructure in that crate first (if
one exists) to match established conventions for spawning the binary, before writing this new one.

```rust
// crates/frontends/cli/tests/kokoro_e2e.rs
use std::process::Command;

fn real_voice_config_path() -> Option<String> {
    std::env::var("DENGJEN_KOKORO_TEST_VOICE_CONFIG").ok()
}

#[test]
fn cli_synthesizes_from_a_real_kokoro_voice_via_stdin() {
    let Some(config_path) = real_voice_config_path() else {
        eprintln!("Skipping: set DENGJEN_KOKORO_TEST_VOICE_CONFIG to a real Kokoro voice config to run this test");
        return;
    };
    let output = Command::new(env!("CARGO_BIN_EXE_dengjen"))
        .arg(&config_path)
        .arg("-f")
        .arg("/dev/stdin")
        .env("DENGJEN_KOKORO_TEST", "1")
        .output()
        .expect("failed to spawn dengjen-cli");
    assert!(
        output.status.success(),
        "CLI exited with failure: stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output.stdout.is_empty(), "expected WAV bytes on stdout");
}
```

Check the CLI's actual argument parsing (`clap` derive struct near the top of `main.rs`) before
finalizing this test's exact command-line invocation — the sketch above assumes a positional config
path and a `-f` flag for input text matching the README's documented usage
(`dengjen voices/en_US-lessac-medium.onnx.json -f input.txt -o output.wav`), but confirm against the
real `clap` struct rather than assuming the flags are named exactly this way.

- [ ] **Step 3: Run tests to verify they skip gracefully without a real voice**

Run: `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml --test e2e_real_voice`
Expected: test runs and passes (prints the skip message to stderr, does not fail) since
`DENGJEN_KOKORO_TEST_VOICE_CONFIG` is unset in this environment.

Run: `cargo test --manifest-path crates/frontends/cli/Cargo.toml --test kokoro_e2e`
Expected: same graceful skip.

- [ ] **Step 4: Commit**

```bash
git add crates/dengjen/models/kokoro/tests/e2e_real_voice.rs crates/frontends/cli/tests/kokoro_e2e.rs
git commit -m "kokoro: skippable Tier 3 e2e tests for CLI and crate"
```

---

## Final check

- [ ] Run `cargo build --workspace` — clean build.
- [ ] Run `cargo test --manifest-path crates/dengjen/models/kokoro/Cargo.toml` — all tests pass
  (unit tests from Tasks 1-4, the Tier 2 synthetic-fixture test from Task 5, the skippable Tier 3
  test from Task 7).
- [ ] Run `cargo test --manifest-path crates/frontends/cli/Cargo.toml` — all tests pass, including
  the new dispatch tests (Task 6) and skippable e2e test (Task 7).
- [ ] Confirm the CLI still works against a real Piper voice unchanged (no regression) — e.g.
  `cargo run --manifest-path crates/frontends/cli/Cargo.toml -- <existing test piper voice config> -f input.txt -o /tmp/out.wav` if a real Piper voice fixture is available locally.

## Out of scope

- capi/grpc/python wiring for Kokoro — CLI only, per the spec.
- Sample-level streaming chunking for Kokoro — tracked as issue #20.
- Publishing/distributing Kokoro voice files.
- This plan assumes it lands on `main` before or independently of PR #19 (the GPL-3.0-or-later
  relicense) — no `license` field is added to the new crate's `Cargo.toml`. If #19 merges first,
  adding `license.workspace = true` to this crate afterward is a trivial one-line follow-up, not
  part of this plan.

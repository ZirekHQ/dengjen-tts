//! g2pW ONNX polyphone-disambiguation inference, ported from
//! OHF-Voice/piper1-gpl's `src/piper/g2pw_onnx.py`, itself an inference-only
//! port of GitYCC/g2pW (Apache-2.0). See `tokenize.rs`'s module doc for the
//! same attribution.

use crate::tokenize::tokenize_and_map;
use dengjen_tts_core::{DengjenError, DengjenResult};
use ndarray::{Array1, Array2};
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

pub(crate) fn build_phoneme_mask(labels: &[String], candidates: &[usize]) -> Vec<f32> {
    let mut mask = vec![0.0f32; labels.len()];
    for &c in candidates {
        if let Some(slot) = mask.get_mut(c) {
            *slot = 1.0;
        }
    }
    mask
}

/// Mirrors `_FeatureBuilder._truncate_texts`: `start = max(0, query - window/2)`,
/// `end = min(len, query + window/2)`. Integer division matches Python's `//`.
pub(crate) fn truncate_window(
    text_len: usize,
    query_id: usize,
    window_size: usize,
) -> (usize, usize) {
    let half = window_size / 2;
    let start = query_id.saturating_sub(half);
    let end = (query_id + half).min(text_len);
    (start, end)
}

pub(crate) struct G2pwConfig {
    pub labels: Vec<String>,
    pub char2phonemes: HashMap<char, Vec<usize>>,
    pub window_size: usize,
    pub max_len: usize,
    pub chars: Vec<char>,
}

impl G2pwConfig {
    fn chars_index(&self, c: char) -> DengjenResult<usize> {
        self.chars.binary_search(&c).map_err(|_| {
            DengjenError::PhonemizationError(format!(
                "'{c}' is not in the g2pW config's sorted chars list"
            ))
        })
    }
}

pub(crate) struct G2pwEngine {
    session: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    config: G2pwConfig,
}

fn session_init_error(cause: impl std::fmt::Display) -> DengjenError {
    DengjenError::InferenceError(format!(
        "Failed to initialize the g2pW inference session: {cause}"
    ))
}

fn inference_error(cause: impl std::fmt::Display) -> DengjenError {
    DengjenError::InferenceError(format!("g2pW inference failed: {cause}"))
}

pub(crate) fn create_g2pw_engine(
    onnx_model_path: &Path,
    tokenizer_path: &Path,
    config: G2pwConfig,
) -> DengjenResult<G2pwEngine> {
    let session = Session::builder()
        .map_err(session_init_error)?
        .commit_from_file(onnx_model_path)
        .map_err(session_init_error)?;
    let tokenizer = tokenizers::Tokenizer::from_file(tokenizer_path).map_err(session_init_error)?;
    Ok(G2pwEngine {
        session: Mutex::new(session),
        tokenizer,
        config,
    })
}

impl G2pwEngine {
    /// Resolves the bopomofo label for the character at `query_char_index` (a
    /// character index, not byte index) within `text`. Callers must have already
    /// confirmed that character is genuinely polyphonic (present in
    /// `config.char2phonemes`) — the three-tier resolution stage this crate
    /// builds on top of this module is responsible for only calling this for
    /// characters that actually need it.
    pub fn resolve_polyphonic(&self, text: &str, query_char_index: usize) -> DengjenResult<String> {
        let chars: Vec<char> = text.chars().collect();
        let query_char = *chars.get(query_char_index).ok_or_else(|| {
            DengjenError::PhonemizationError(format!(
                "query_char_index {query_char_index} is out of bounds for text {text:?}"
            ))
        })?;
        let candidates = self.config.char2phonemes.get(&query_char).ok_or_else(|| {
            DengjenError::PhonemizationError(format!(
                "'{query_char}' has no known phoneme candidates for g2pW resolution"
            ))
        })?;

        let (win_start, win_end) =
            truncate_window(chars.len(), query_char_index, self.config.window_size);
        let windowed_text: String = chars[win_start..win_end].iter().collect();
        let windowed_query_id = query_char_index - win_start;

        let (tokens, text_to_token) =
            tokenize_and_map(&self.tokenizer, &windowed_text.to_lowercase());
        let token_position = text_to_token
            .get(windowed_query_id)
            .copied()
            .flatten()
            .ok_or_else(|| {
                DengjenError::PhonemizationError(format!(
                    "windowed query position {windowed_query_id} has no token mapping"
                ))
            })?;

        // [CLS] occupies position 0, shifting every real token index by one.
        let truncate_len = self.config.max_len.saturating_sub(2);
        let (final_tokens, position_id) = if tokens.len() > truncate_len {
            let token_start =
                (token_position as isize - (truncate_len / 2) as isize).max(0) as usize;
            let token_start = token_start.min(tokens.len().saturating_sub(truncate_len));
            let token_end = (token_start + truncate_len).min(tokens.len());
            (
                tokens[token_start..token_end].to_vec(),
                token_position - token_start + 1,
            )
        } else {
            (tokens, token_position + 1)
        };

        let mut input_ids: Vec<i64> = vec![self.cls_id()];
        input_ids.extend(final_tokens.iter().map(|t| self.token_to_id(&t.text)));
        input_ids.push(self.sep_id());
        let seq_len = input_ids.len();

        let token_type_ids = vec![0i64; seq_len];
        let attention_mask = vec![1i64; seq_len];
        let phoneme_mask = build_phoneme_mask(&self.config.labels, candidates);

        let input_ids_arr =
            Array2::from_shape_vec((1, seq_len), input_ids).map_err(inference_error)?;
        let token_type_ids_arr =
            Array2::from_shape_vec((1, seq_len), token_type_ids).map_err(inference_error)?;
        let attention_mask_arr =
            Array2::from_shape_vec((1, seq_len), attention_mask).map_err(inference_error)?;
        let char_ids_arr = Array1::from_vec(vec![self.config.chars_index(query_char)? as i64]);
        let position_ids_arr = Array1::from_vec(vec![position_id as i64]);
        let phoneme_mask_arr = Array2::from_shape_vec((1, self.config.labels.len()), phoneme_mask)
            .map_err(inference_error)?;

        let mut session = self.session.lock().map_err(inference_error)?;
        let outputs = session
            .run(ort::inputs![
                "input_ids" => Tensor::from_array(input_ids_arr).map_err(inference_error)?,
                "token_type_ids" => Tensor::from_array(token_type_ids_arr).map_err(inference_error)?,
                "attention_mask" => Tensor::from_array(attention_mask_arr).map_err(inference_error)?,
                "char_ids" => Tensor::from_array(char_ids_arr).map_err(inference_error)?,
                "position_ids" => Tensor::from_array(position_ids_arr).map_err(inference_error)?,
                "phoneme_mask" => Tensor::from_array(phoneme_mask_arr).map_err(inference_error)?,
            ])
            .map_err(inference_error)?;

        if outputs.len() == 0 {
            return Err(inference_error("model produced no output tensors"));
        }
        let (shape, probs) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(inference_error)?;
        // Validate the model's own reported output shape completely before indexing into
        // `self.config.labels` or the probability buffer, rather than trusting a
        // caller-supplied model's output width matches the caller-supplied label list — the
        // same "no panics on model-shape-dependent data" principle applied throughout this
        // engine (e.g. hebrew-phonemizer's nakdimon.rs). Checking only the last dimension
        // would silently accept a wrong-rank tensor whose last dimension happens to match, or
        // a negative (ONNX dynamic-dimension) label count that wraps to a huge usize.
        if shape.len() != 2 {
            return Err(inference_error(format!(
                "expected a rank-2 (batch, num_labels) output tensor, got shape {shape:?}"
            )));
        }
        if shape[0] != 1 {
            return Err(inference_error(format!(
                "expected batch size 1, model produced shape {shape:?}"
            )));
        }
        let num_labels = usize::try_from(shape[1])
            .map_err(|_| inference_error(format!("invalid label dimension in shape {shape:?}")))?;
        if num_labels != self.config.labels.len() || probs.len() < num_labels {
            return Err(inference_error(format!(
                "g2pW model output width {num_labels} (probs len {}) does not match the \
                 configured label count {}",
                probs.len(),
                self.config.labels.len()
            )));
        }
        let predicted = probs[..num_labels]
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(idx, _)| idx)
            .ok_or_else(|| inference_error("empty prediction output"))?;

        self.config.labels.get(predicted).cloned().ok_or_else(|| {
            inference_error(format!("predicted label index {predicted} out of range"))
        })
    }

    fn cls_id(&self) -> i64 {
        self.tokenizer.token_to_id("[CLS]").unwrap_or(101) as i64
    }
    fn sep_id(&self) -> i64 {
        self.tokenizer.token_to_id("[SEP]").unwrap_or(102) as i64
    }
    fn token_to_id(&self, token: &str) -> i64 {
        // 100 = [UNK] in bert-base-chinese's real vocab; verify at integration time.
        self.tokenizer.token_to_id(token).unwrap_or(100) as i64
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn build_phoneme_mask_sets_one_for_each_candidate_label_and_zero_elsewhere() {
        let labels = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let candidates = vec![0usize, 2usize];
        let mask = super::build_phoneme_mask(&labels, &candidates);
        assert_eq!(mask, vec![1.0f32, 0.0, 1.0]);
    }

    #[test]
    fn truncate_window_centers_on_the_query_character_within_window_size() {
        let (start, end) = super::truncate_window(10, 5, 4);
        assert_eq!((start, end), (3, 7));
    }

    #[test]
    fn truncate_window_clamps_to_text_bounds_near_the_start() {
        let (start, end) = super::truncate_window(10, 1, 4);
        assert_eq!((start, end), (0, 3));
    }

    #[test]
    fn truncate_window_clamps_to_text_bounds_near_the_end() {
        let (start, end) = super::truncate_window(10, 9, 4);
        assert_eq!((start, end), (7, 10));
    }

    #[test]
    fn resolve_polyphonic_disambiguates_a_known_polyphonic_character_with_a_real_model() {
        let (Some(onnx_path), Some(tokenizer_path)) = (
            std::env::var("DENGJEN_PINYIN_TEST_MODEL_PATH").ok(),
            std::env::var("DENGJEN_PINYIN_TEST_TOKENIZER_PATH").ok(),
        ) else {
            eprintln!(
                "skipping: DENGJEN_PINYIN_TEST_MODEL_PATH/DENGJEN_PINYIN_TEST_TOKENIZER_PATH not set"
            );
            return;
        };
        // Building a real G2pwConfig needs `dictionary.rs`'s loader wired in here;
        // this test is a documented placeholder without that wiring, not a real
        // assertion yet, kept explicit so it isn't silently forgotten.
        eprintln!(
            "resolve_polyphonic_disambiguates_a_known_polyphonic_character_with_a_real_model \
             requires the crate's dictionary loader to be wired up before it can run for real: \
             {onnx_path} / {tokenizer_path}"
        );
    }
}

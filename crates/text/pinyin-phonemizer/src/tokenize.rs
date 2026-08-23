//! BERT tokenizer wrapper and text/word/token index mapping, ported from
//! OHF-Voice/piper1-gpl's `src/piper/g2pw_onnx.py`, itself an inference-only
//! port of GitYCC/g2pW (Apache-2.0). Index mapping is character-based
//! throughout, matching upstream's Python string indexing — never byte-based.

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Word {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Token {
    pub text: String,
    pub start: usize,
    pub end: usize,
}

pub(crate) fn wordize(text: &str) -> (Vec<Word>, Vec<Option<usize>>) {
    let chars: Vec<char> = text.chars().collect();
    let mut words: Vec<Word> = Vec::new();
    let mut text_to_word: Vec<Option<usize>> = Vec::with_capacity(chars.len());

    let mut i = 0;
    while i < chars.len() {
        if chars[i] == ' ' {
            text_to_word.push(None);
            i += 1;
            continue;
        }
        if chars[i].is_ascii_alphanumeric() {
            let start = i;
            while i < chars.len() && chars[i].is_ascii_alphanumeric() {
                i += 1;
            }
            let word_index = words.len();
            words.push(Word {
                text: chars[start..i].iter().collect(),
                start,
                end: i,
            });
            for _ in start..i {
                text_to_word.push(Some(word_index));
            }
            continue;
        }
        let word_index = words.len();
        words.push(Word {
            text: chars[i].to_string(),
            start: i,
            end: i + 1,
        });
        text_to_word.push(Some(word_index));
        i += 1;
    }

    (words, text_to_word)
}

pub(crate) fn tokenize_and_map(
    tokenizer: &tokenizers::Tokenizer,
    text: &str,
) -> (Vec<Token>, Vec<usize>) {
    let (words, text_to_word) = wordize(text);
    let chars: Vec<char> = text.chars().collect();

    let mut tokens: Vec<Token> = Vec::new();
    let mut text_to_token: Vec<usize> = vec![0; chars.len()];

    for word in &words {
        let encoding = tokenizer
            .encode(word.text.as_str(), false)
            .unwrap_or_else(|_| tokenizers::Encoding::default());
        let word_tokens: Vec<String> = encoding
            .get_tokens()
            .iter()
            .map(|t| t.to_string())
            .collect();

        if word_tokens.is_empty() || word_tokens == ["[UNK]"] {
            let token_index = tokens.len();
            tokens.push(Token {
                text: "[UNK]".to_string(),
                start: word.start,
                end: word.end,
            });
            text_to_token[word.start..word.end].fill(token_index);
            continue;
        }

        let mut cursor = word.start;
        for word_token in word_tokens {
            let stripped = word_token.strip_prefix("##").unwrap_or(&word_token);
            let token_len = stripped.chars().count();
            let token_index = tokens.len();
            tokens.push(Token {
                text: word_token.clone(),
                start: cursor,
                end: cursor + token_len,
            });
            for pos in cursor..cursor + token_len {
                if pos < text_to_token.len() {
                    text_to_token[pos] = token_index;
                }
            }
            cursor += token_len;
        }
    }

    // text_to_word's None (whitespace) positions never had a real word/token, so their
    // text_to_token slot is left at the default 0 (matching upstream's own behavior:
    // index_map_from_text_to_token starts as a copy of index_map_from_text_to_word,
    // and only positions that belong to an actual token ever get overwritten).
    let _ = text_to_word;
    (tokens, text_to_token)
}

#[cfg(test)]
mod tests {
    #[test]
    fn wordize_groups_runs_of_ascii_alphanumerics_into_one_word() {
        let (words, text_to_word) = super::wordize("ab12 你");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "ab12");
        assert_eq!(words[0].start, 0);
        assert_eq!(words[0].end, 4);
        assert_eq!(words[1].text, "你");
        assert_eq!(words[1].start, 5);
        assert_eq!(words[1].end, 6);
        assert_eq!(
            text_to_word,
            vec![Some(0), Some(0), Some(0), Some(0), None, Some(1)]
        );
    }

    #[test]
    fn wordize_treats_every_non_ascii_alnum_character_as_its_own_word() {
        let (words, _text_to_word) = super::wordize("你好");
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "你");
        assert_eq!(words[1].text, "好");
    }

    #[test]
    fn wordize_maps_spaces_to_none_without_producing_a_word() {
        let (words, text_to_word) = super::wordize("  你");
        assert_eq!(words.len(), 1);
        assert_eq!(text_to_word, vec![None, None, Some(0)]);
    }

    #[test]
    fn tokenize_and_map_produces_one_token_per_single_char_word_when_not_split_further() {
        // A real BERT tokenizer for Chinese text typically emits one WordPiece token
        // per Han character (no "##" continuation within a single-character word),
        // so token spans should align 1:1 with wordize's own word spans here.
        let Some(tokenizer) = test_tokenizer() else {
            eprintln!("skipping: DENGJEN_PINYIN_TEST_TOKENIZER_PATH not set");
            return;
        };
        let (tokens, text_to_token) = super::tokenize_and_map(&tokenizer, "你好");
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].start, 0);
        assert_eq!(tokens[0].end, 1);
        assert_eq!(tokens[1].start, 1);
        assert_eq!(tokens[1].end, 2);
        assert_eq!(text_to_token, vec![0, 1]);
    }

    fn test_tokenizer() -> Option<tokenizers::Tokenizer> {
        let path = std::env::var("DENGJEN_PINYIN_TEST_TOKENIZER_PATH").ok()?;
        Some(tokenizers::Tokenizer::from_file(path).unwrap())
    }
}

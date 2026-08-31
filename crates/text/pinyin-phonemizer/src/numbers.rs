//! Chinese number-to-words normalization, scoped to the common patterns named
//! in the design spec (plain digit sequences, percentages, temperatures) —
//! not a general RBNF-equivalent formatter. No Rust crate provides
//! RBNF-grade Chinese number formatting (upstream uses Python's
//! `unicode_rbnf`), so this is original code, not a port.

const DIGITS: [char; 10] = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九'];

fn digit_to_word(d: u32) -> char {
    DIGITS[d as usize]
}

/// Formats an unsigned integer 0-99 using standard Chinese two-digit
/// composition (`十位` + `十` + `个位`, dropping the leading 一 for 10-19).
fn format_small_number(n: u32) -> String {
    if n < 10 {
        return digit_to_word(n).to_string();
    }
    let tens = n / 10;
    let ones = n % 10;
    let mut out = String::new();
    if tens > 1 {
        out.push(digit_to_word(tens));
    }
    out.push('十');
    if ones > 0 {
        out.push(digit_to_word(ones));
    }
    out
}

/// Formats an arbitrary non-negative integer string as Chinese words,
/// digit-by-digit for anything outside the 0-99 range this backend actually
/// needs (percentages/temperatures never exceed two digits in practice, but
/// this stays correct rather than panicking on a longer input).
fn format_integer(digits: &str) -> String {
    if let Ok(n) = digits.parse::<u64>() {
        if n < 100 {
            return format_small_number(n as u32);
        }
    }
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(digit_to_word)
        .collect()
}

/// Reads a decimal fraction digit-by-digit (`76` -> `七六`, never `七十六`): unlike the whole
/// part, digits after the point are always spoken individually.
fn format_fraction_digits(digits: &str) -> String {
    digits
        .chars()
        .filter_map(|c| c.to_digit(10))
        .map(digit_to_word)
        .collect()
}

pub fn normalize_numbers(text: &str) -> String {
    let text = normalize_temperatures(text);
    let text = normalize_percentages(&text);
    normalize_plain_digits(&text)
}

fn normalize_temperatures(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let sign_start = i;
        let negative = chars[i] == '-' || chars[i] == '\u{2212}';
        let digit_start = if negative { i + 1 } else { i };
        let mut j = digit_start;
        while j < chars.len() && chars[j].is_ascii_digit() {
            j += 1;
        }
        if j > digit_start {
            let mut k = j;
            while k < chars.len() && chars[k] == ' ' {
                k += 1;
            }
            let celsius_end = if k < chars.len() && chars[k] == '\u{2103}' {
                Some(k + 1)
            } else if k + 1 < chars.len() && chars[k] == '°' && chars[k + 1] == 'C' {
                Some(k + 2)
            } else {
                None
            };
            if let Some(end) = celsius_end {
                let digits: String = chars[digit_start..j].iter().collect();
                let words = format_integer(&digits);
                if negative {
                    out.push_str(&format!("零下{words}度"));
                } else {
                    out.push_str(&format!("{words}度"));
                }
                i = end;
                continue;
            }
        }
        out.push(chars[sign_start]);
        i += 1;
    }
    out
}

fn normalize_percentages(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    let mut i = 0;
    while i < chars.len() {
        let sign_start = i;
        let negative = chars[i] == '-' || chars[i] == '\u{2212}';
        let digit_start = if negative { i + 1 } else { i };

        let mut integer_end = digit_start;
        while integer_end < chars.len() && chars[integer_end].is_ascii_digit() {
            integer_end += 1;
        }
        let has_integer_digits = integer_end > digit_start;

        let mut fraction_end = integer_end;
        if has_integer_digits && integer_end < chars.len() && chars[integer_end] == '.' {
            fraction_end = integer_end + 1;
            while fraction_end < chars.len() && chars[fraction_end].is_ascii_digit() {
                fraction_end += 1;
            }
        }
        let has_fraction_digits = fraction_end > integer_end + 1;
        let value_end = if has_fraction_digits {
            fraction_end
        } else {
            integer_end
        };

        if has_integer_digits
            && value_end < chars.len()
            && (chars[value_end] == '%' || chars[value_end] == '\u{FF05}')
        {
            let integer_digits: String = chars[digit_start..integer_end].iter().collect();
            let mut words = format_integer(&integer_digits);
            if has_fraction_digits {
                let fractional_digits: String =
                    chars[integer_end + 1..fraction_end].iter().collect();
                words = format!("{words}点{}", format_fraction_digits(&fractional_digits));
            }
            if negative {
                out.push_str(&format!("负百分之{words}"));
            } else {
                out.push_str(&format!("百分之{words}"));
            }
            i = value_end + 1;
            continue;
        }
        out.push(chars[sign_start]);
        i += 1;
    }
    out
}

fn normalize_plain_digits(text: &str) -> String {
    let mut out = String::new();
    let mut digit_run = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digit_run.push(c);
        } else {
            if !digit_run.is_empty() {
                out.push_str(&format_integer(&digit_run));
                digit_run.clear();
            }
            out.push(c);
        }
    }
    if !digit_run.is_empty() {
        out.push_str(&format_integer(&digit_run));
    }
    out
}

#[cfg(test)]
mod tests {
    #[test]
    fn normalize_numbers_spells_out_single_digits() {
        assert_eq!(super::normalize_numbers("我有3个"), "我有三个");
    }

    #[test]
    fn normalize_numbers_spells_out_two_digit_numbers() {
        assert_eq!(super::normalize_numbers("23"), "二十三");
    }

    #[test]
    fn normalize_numbers_handles_a_multiple_of_ten_without_a_trailing_zero_digit() {
        assert_eq!(super::normalize_numbers("20"), "二十");
    }

    #[test]
    fn normalize_numbers_expands_a_percentage_with_bai_fen_zhi_prefix() {
        assert_eq!(super::normalize_numbers("77%"), "百分之七十七");
        assert_eq!(super::normalize_numbers("77％"), "百分之七十七");
    }

    #[test]
    fn normalize_numbers_expands_a_decimal_percentage_reading_the_fraction_digit_by_digit() {
        assert_eq!(super::normalize_numbers("98.76%"), "百分之九十八点七六");
    }

    #[test]
    fn normalize_numbers_expands_a_negative_percentage_with_a_fu_prefix() {
        assert_eq!(super::normalize_numbers("-7%"), "负百分之七");
    }

    #[test]
    fn normalize_numbers_expands_a_negative_decimal_percentage() {
        assert_eq!(super::normalize_numbers("-3.5%"), "负百分之三点五");
    }

    #[test]
    fn format_integer_falls_back_to_digit_by_digit_when_the_value_overflows_u64() {
        assert_eq!(
            super::format_integer("18446744073709551616"),
            "一八四四六七四四零七三七零九五五一六一六"
        );
    }

    #[test]
    fn normalize_numbers_expands_a_celsius_temperature() {
        assert_eq!(super::normalize_numbers("7°C"), "七度");
        assert_eq!(super::normalize_numbers("7℃"), "七度");
    }

    #[test]
    fn normalize_numbers_uses_ling_xia_for_a_negative_temperature() {
        assert_eq!(super::normalize_numbers("-7°C"), "零下七度");
    }

    #[test]
    fn normalize_numbers_leaves_non_numeric_text_unchanged() {
        assert_eq!(super::normalize_numbers("你好"), "你好");
    }
}

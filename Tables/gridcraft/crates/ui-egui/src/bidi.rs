//! Bidirectional text support (UAX #9) for Persian, Arabic, and Hebrew in GridCraft.

use unicode_bidi::BidiClass::{AL, AN, R};
use unicode_bidi::bidi_class;

/// Whether a string contains any strong right-to-left or Arabic characters.
pub fn has_rtl(s: &str) -> bool {
    s.chars().any(|c| matches!(bidi_class(c), R | AL | AN))
}

/// Reorders words and directional runs in a line so that egui's LTR layout
/// renders Persian/Arabic text in the correct visual sequence from right to left.
pub fn reorder_persian(text: &str) -> String {
    if !has_rtl(text) {
        return text.to_string();
    }

    let mut result = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            result.push('\n');
        }
        if !has_rtl(line) {
            result.push_str(line);
            continue;
        }

        // Tokenize line into words and delimiters (spaces, punctuation)
        let mut tokens: Vec<&str> = Vec::new();
        let mut start = 0;
        let mut in_whitespace = false;
        
        for (idx, c) in line.char_indices() {
            let is_ws = c.is_whitespace();
            if idx == 0 {
                in_whitespace = is_ws;
            } else if is_ws != in_whitespace {
                tokens.push(&line[start..idx]);
                start = idx;
                in_whitespace = is_ws;
            }
        }
        if start < line.len() {
            tokens.push(&line[start..]);
        }

        // In an RTL line, the logical sequence of tokens must be laid out in
        // reverse visual order so that when read from right to left, the tokens
        // are encountered in their correct logical order.
        tokens.reverse();
        for token in tokens {
            result.push_str(token);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_persian_reorder_sentence() {
        let input = "سلام تو خیلی خوبی";
        let out = reorder_persian(input);
        assert_eq!(out, "خوبی خیلی تو سلام");
    }

    #[test]
    fn test_persian_with_numbers() {
        let input = "تعداد 10 عدد سیب";
        let out = reorder_persian(input);
        assert_eq!(out, "سیب عدد 10 تعداد");
    }

    #[test]
    fn test_multiline_persian() {
        let input = "سطر اول\nسطر دوم";
        let out = reorder_persian(input);
        assert_eq!(out, "اول سطر\nدوم سطر");
    }

    #[test]
    fn test_english_untouched() {
        let input = "Hello world 123";
        let out = reorder_persian(input);
        assert_eq!(out, "Hello world 123");
    }
}

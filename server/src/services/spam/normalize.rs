//! Body-text normalization for the spam pipeline.
//!
//! The pipeline matches curated phrases as substrings of normalized text, so
//! both the input and the phrase set must agree on a canonical form. Steps:
//!
//! 1. **NFKC** — compatibility decomposition + canonical composition; folds
//!    full-width digits/letters, ligatures (`ﬁ` → `fi`), and stylized variants
//!    onto their plain counterparts.
//! 2. **Lowercase** — Unicode-aware (`String::to_lowercase`).
//! 3. **Strip combining marks** — covers stray diacritics that survive NFKC
//!    (most are absorbed by canonical composition, but combining accents
//!    applied to script that has no precomposed form remain).
//! 4. **Strip zero-width** — `U+200B`, `U+200C`, `U+200D`, `U+FEFF`. These are
//!    the standard "invisible separator" tricks used to break substring
//!    matches without changing how the text *looks*.
//! 5. **Collapse whitespace** — any run of whitespace becomes a single space;
//!    output is trimmed.
//!
//! We deliberately skip homoglyph remapping (Cyrillic ⟷ Latin). The Dart
//! prototype skipped it too; the curated phrase set in `phrases.rs` already
//! contains common obfuscated forms (`'в лuчные сообщенuя'`,
//! `'для yдaлённoгo зaрaбoткa'`), which is more accurate than naive
//! script-folding that would corrupt legitimate Russian text.
//!
//! `xxh3-64` is computed over the output of [`normalize`], so its determinism
//! is what guarantees that the dedup key is stable across re-deliveries of
//! the same logical message.

use unicode_normalization::UnicodeNormalization;
use unicode_normalization::char::is_combining_mark;

const ZERO_WIDTH: &[char] = &['\u{200B}', '\u{200C}', '\u{200D}', '\u{FEFF}'];

pub fn normalize(input: &str) -> String {
    // Pass 1: NFKC + zero-width strip + combining-mark strip.
    // Lowercasing happens in pass 2 because `char::to_lowercase` returns an
    // iterator that may emit multiple chars (`İ` → `i\u{307}`); doing it
    // after combining-mark strip would re-introduce marks we just removed.
    let pass_one: String = input
        .nfkc()
        .filter(|c| !ZERO_WIDTH.contains(c) && !is_combining_mark(*c))
        .flat_map(|c| c.to_lowercase())
        .filter(|c| !is_combining_mark(*c))
        .collect();

    // Pass 2: collapse whitespace + trim.
    let mut out = String::with_capacity(pass_one.len());
    let mut prev_space = true;
    for c in pass_one.chars() {
        if c.is_whitespace() {
            if !prev_space {
                out.push(' ');
                prev_space = true;
            }
        } else {
            out.push(c);
            prev_space = false;
        }
    }
    if out.ends_with(' ') {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lowercases_ascii() {
        assert_eq!(normalize("HELLO World"), "hello world");
    }

    #[test]
    fn collapses_whitespace_and_trims() {
        assert_eq!(normalize("  a   b\tc\nd  "), "a b c d");
    }

    #[test]
    fn strips_zero_width() {
        // "click\u{200B}here" → "clickhere"
        assert_eq!(normalize("click\u{200B}here"), "clickhere");
        assert_eq!(normalize("buy\u{200C}now"), "buynow");
        assert_eq!(normalize("act\u{FEFF}now"), "actnow");
    }

    #[test]
    fn nfkc_folds_compatibility_chars() {
        // ﬃ (U+FB03) is the "ffi" ligature; NFKC decomposes it.
        assert_eq!(normalize("eﬃcient"), "efficient");
        // Full-width digits collapse to ASCII.
        assert_eq!(normalize("１２３"), "123");
    }

    #[test]
    fn strips_combining_marks() {
        // a + combining acute → á; NFKC composes it; lowercase keeps it.
        // a + combining ring (no precomposed form for "a-with-ring-below")
        // remains decomposed; we strip the mark.
        let s = "a\u{0331}"; // a + combining macron below
        assert_eq!(normalize(s), "a");
    }

    #[test]
    fn russian_is_not_mangled() {
        assert_eq!(
            normalize("Быстрый Заработок Без Вложений"),
            "быстрый заработок без вложений"
        );
    }

    #[test]
    fn idempotent() {
        let cases = [
            "Buy NOW for the BEST price",
            "  ВЫГОДНОЕ  ПРЕДЛОЖЕНИЕ  ",
            "click\u{200B}here\u{FEFF}",
            "eﬃcient １２３",
        ];
        for case in cases {
            let once = normalize(case);
            let twice = normalize(&once);
            assert_eq!(once, twice, "not idempotent for {case:?}");
        }
    }

    #[test]
    fn preserves_phrase_substrings() {
        // The whole point: after normalization, phrases from `phrases.rs`
        // must still appear as substrings.
        let normalized = normalize("Hi there!  Click HERE for the BEST price.");
        assert!(normalized.contains("click here"));
        assert!(normalized.contains("best price"));
    }

    #[test]
    fn empty_input_yields_empty() {
        assert_eq!(normalize(""), "");
        assert_eq!(normalize("   "), "");
    }
}

//! Captcha message captions.
//!
//! Three variants share the same slot renderer so the visual contract — four
//! squares filled left-to-right as the user types — stays consistent across
//! the lifecycle (initial post → digit press → wrong attempt → refresh).
//!
//! Filled slot = keycap digit (`1️⃣`), empty slot = white square (`⬜`).
//! Captions are plain text (no `parse_mode`) so user mentions don't need
//! MarkdownV2 escaping.

const SOLUTION_LEN: usize = 4;
const EMPTY_SLOT: &str = "⬜";

/// Render the input buffer as four space-separated slots — keycap digits for
/// typed positions, white squares for empty ones. Non-digit chars in `input`
/// are treated as empty (defensive — the keyboard only emits `0..9`).
pub fn render_slots(input: &str) -> String {
    let chars: Vec<char> = input.chars().collect();
    let mut out = String::with_capacity(SOLUTION_LEN * 5);
    for i in 0..SOLUTION_LEN {
        if i > 0 {
            out.push(' ');
        }
        match chars.get(i) {
            Some(d) if d.is_ascii_digit() => {
                out.push(*d);
                out.push('\u{FE0F}'); // VS-16 emoji presentation
                out.push('\u{20E3}'); // combining enclosing keycap
            }
            _ => out.push_str(EMPTY_SLOT),
        }
    }
    out
}

/// Initial caption posted alongside the captcha image when a fresh user joins
/// (or when an unverified user trips the message gate).
pub fn caption_initial(mention: &str, attempts_left: i16) -> String {
    format!(
        "👋 {mention}, welcome!\n\
         \n\
         🔐 Please solve the captcha to start chatting.\n\
         \n\
         {slots}\n\
         \n\
         🎯 Attempts left: {attempts_left}",
        slots = render_slots(""),
    )
}

/// Caption shown while the user is typing — also used after backspace and
/// after refresh (both reset to whatever buffer is current; refresh always
/// passes an empty string).
pub fn caption_progress(input: &str) -> String {
    format!(
        "🔐 Captcha verification\n\
         \n\
         Enter the 4 digits from the image.\n\
         \n\
         {slots}",
        slots = render_slots(input),
    )
}

/// Caption shown after a wrong (non-final) attempt. The buffer is reset to
/// empty server-side, so we always render four empty slots here.
pub fn caption_wrong(attempts_left: i16) -> String {
    format!(
        "❌ Wrong code, try again.\n\
         \n\
         🎯 Attempts left: {attempts_left}\n\
         \n\
         {slots}",
        slots = render_slots(""),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEYCAP_1: &str = "1\u{FE0F}\u{20E3}";
    const KEYCAP_2: &str = "2\u{FE0F}\u{20E3}";

    #[test]
    fn render_slots_empty() {
        assert_eq!(render_slots(""), "⬜ ⬜ ⬜ ⬜");
    }

    #[test]
    fn render_slots_partial() {
        assert_eq!(render_slots("12"), format!("{KEYCAP_1} {KEYCAP_2} ⬜ ⬜"));
    }

    #[test]
    fn render_slots_full() {
        let s = render_slots("1234");
        assert!(s.contains(KEYCAP_1));
        assert!(s.contains(KEYCAP_2));
        assert!(!s.contains('⬜'));
    }

    #[test]
    fn render_slots_overflow_truncates() {
        // Defensive: the keyboard caps input at SOLUTION_LEN, but if a longer
        // buffer ever leaks in, the renderer must still produce 4 slots.
        let s = render_slots("12345");
        assert_eq!(s.matches('\u{20E3}').count(), 4);
    }

    #[test]
    fn render_slots_non_digit_is_empty() {
        // Defensive parity with `is_ascii_digit` filter — non-digits collapse
        // to empty squares rather than rendering as raw chars.
        assert_eq!(render_slots("a"), "⬜ ⬜ ⬜ ⬜");
    }

    #[test]
    fn caption_initial_has_mention_and_attempts() {
        let c = caption_initial("@alice", 5);
        assert!(c.starts_with("👋 @alice, welcome!"));
        assert!(c.contains("Attempts left: 5"));
        assert!(c.contains("⬜ ⬜ ⬜ ⬜"));
    }

    #[test]
    fn caption_progress_shows_typed_digits() {
        let c = caption_progress("12");
        assert!(c.contains(KEYCAP_1));
        assert!(c.contains(KEYCAP_2));
        assert!(c.contains("Enter the 4 digits"));
    }

    #[test]
    fn caption_wrong_has_attempts_and_empty_slots() {
        let c = caption_wrong(3);
        assert!(c.starts_with("❌ Wrong code, try again."));
        assert!(c.contains("Attempts left: 3"));
        assert!(c.contains("⬜ ⬜ ⬜ ⬜"));
    }

    #[test]
    fn captions_split_sentences_on_separate_lines() {
        // Each emoji-prefixed sentence sits on its own line — the user asked
        // for no run-on lines.
        for c in [
            caption_initial("@alice", 5),
            caption_progress(""),
            caption_wrong(2),
        ] {
            for line in c.lines() {
                // Every non-empty line that contains a sentence-ending period
                // must contain at most one period.
                let periods = line.matches('.').count();
                assert!(periods <= 1, "line has multiple sentences: {line:?}");
            }
        }
    }
}

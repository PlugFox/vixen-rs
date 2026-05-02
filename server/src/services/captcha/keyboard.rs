//! Inline keyboard for the captcha digit-pad.
//!
//! Layout (3 columns × 4 rows):
//!
//! ```text
//! [1] [2] [3]
//! [4] [5] [6]
//! [7] [8] [9]
//! [⌫] [0] [↻]
//! ```
//!
//! Callback data scheme: `vc:{short}:{op}` where `short` is the first 8 hex
//! characters of the challenge UUID and `op` is one of:
//!
//!   * `0`..`9`  — digit press
//!   * `bs`      — backspace (drop last input digit)
//!   * `rf`      — refresh (issue a new solution + image)
//!
//! `short` lets the handler reject stale callbacks from a previous challenge —
//! the current challenge is always looked up by `(chat_id, user_id)`.

use teloxide::types::{InlineKeyboardButton, InlineKeyboardMarkup};
use uuid::Uuid;

pub const CALLBACK_PREFIX: &str = "vc";
/// Same as `CALLBACK_PREFIX` plus the `:` separator. Used as a `&'static str`
/// in the dispatcher's per-update filter so we don't allocate a `String` on
/// every callback via `format!("{CALLBACK_PREFIX}:")`.
pub const CALLBACK_PREFIX_WITH_COLON: &str = "vc:";
pub const OP_BACKSPACE: &str = "bs";
pub const OP_REFRESH: &str = "rf";

/// Build the digit-pad keyboard for the given challenge.
pub fn digit_pad(challenge_id: Uuid) -> InlineKeyboardMarkup {
    let short = short_id(challenge_id);
    let cb = |op: &str| InlineKeyboardButton::callback(label_for(op), data_for(&short, op));
    InlineKeyboardMarkup::new(vec![
        vec![cb("1"), cb("2"), cb("3")],
        vec![cb("4"), cb("5"), cb("6")],
        vec![cb("7"), cb("8"), cb("9")],
        vec![cb(OP_BACKSPACE), cb("0"), cb(OP_REFRESH)],
    ])
}

pub fn short_id(id: Uuid) -> String {
    let bytes = id.as_bytes();
    let mut s = String::with_capacity(8);
    for b in &bytes[..4] {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

pub fn data_for(short: &str, op: &str) -> String {
    format!("{CALLBACK_PREFIX}:{short}:{op}")
}

fn label_for(op: &str) -> String {
    match op {
        OP_BACKSPACE => "⌫".into(),
        OP_REFRESH => "↻".into(),
        d => d.into(),
    }
}

/// Parsed callback payload. `op` is the raw string for the handler to switch on
/// (digits stay as `"0".."9"` so the handler can append directly).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedCallback {
    pub short: String,
    pub op: String,
}

pub fn parse_callback(data: &str) -> Option<ParsedCallback> {
    let mut it = data.splitn(3, ':');
    let prefix = it.next()?;
    if prefix != CALLBACK_PREFIX {
        return None;
    }
    let short = it.next()?.to_owned();
    let op = it.next()?.to_owned();
    if short.len() != 8 {
        return None;
    }
    Some(ParsedCallback { short, op })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_digit_callback() {
        let parsed = parse_callback("vc:0123abcd:7").expect("parsed");
        assert_eq!(parsed.short, "0123abcd");
        assert_eq!(parsed.op, "7");
    }

    #[test]
    fn parses_special_op() {
        assert_eq!(parse_callback("vc:0123abcd:bs").unwrap().op, "bs");
        assert_eq!(parse_callback("vc:0123abcd:rf").unwrap().op, "rf");
    }

    #[test]
    fn rejects_wrong_prefix_or_short() {
        assert!(parse_callback("xx:0123abcd:1").is_none());
        assert!(parse_callback("vc:short:1").is_none());
    }

    #[test]
    fn short_id_is_first_8_hex() {
        let id = Uuid::from_u128(0xdeadbeef_0000_0000_0000_000000000000);
        assert_eq!(short_id(id), "deadbeef");
    }

    #[test]
    fn keyboard_has_4_rows_of_3() {
        let kb = digit_pad(Uuid::nil());
        assert_eq!(kb.inline_keyboard.len(), 4);
        for row in &kb.inline_keyboard {
            assert_eq!(row.len(), 3);
        }
    }
}

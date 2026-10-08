//! What a chat message may hold. The messages themselves travel sealed through the meeting server
//! (`chatroom.rs`, CRYPTO.md section 8) - no longer over the iroh connections, where nothing signed
//! them. Pure: no azul types, unit-tested here.

/// The longest text a message carries, in bytes; a longer one is cut at a character boundary.
pub const MAX_CHAT_BYTES: usize = 2000;

/// `text` trimmed, cut to at most `max` bytes at a character boundary; `None` when nothing is
/// left.
pub fn clean_text(text: &str, max: usize) -> Option<String> {
    let text = cut(text.trim(), max).trim_end();
    (!text.is_empty()).then(|| text.to_string())
}

/// The longest start of `text` of at most `max` bytes that ends at a character boundary.
fn cut(text: &str, max: usize) -> &str {
    if text.len() <= max {
        return text;
    }
    let mut end = max;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_is_trimmed_and_cut_at_a_character_boundary() {
        assert_eq!(clean_text("   ", 100), None);
        assert_eq!(clean_text("  hi \n", 100), Some(String::from("hi")));
        // "ü" is two bytes: a cut in its middle keeps the whole character out.
        assert_eq!(clean_text("aü", 2), Some(String::from("a")));
        assert_eq!(clean_text("aü", 3), Some(String::from("aü")));
        let long = "x".repeat(MAX_CHAT_BYTES + 10);
        assert_eq!(clean_text(&long, MAX_CHAT_BYTES).map(|t| t.len()), Some(MAX_CHAT_BYTES));
    }
}

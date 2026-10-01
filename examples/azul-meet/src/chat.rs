//! The call's chat: what goes on the wire and what the side panel lists.
//!
//! A chat message is a reliable, ordered iroh message to every connected peer (everyone is
//! connected to everyone in a room; nothing is forwarded):
//!
//! ```text
//! [8][version u8 = 1][seq u32][sent_ms u64][len u16][len bytes of UTF-8 text]
//! ```
//!
//! `seq` counts the sender's messages from 1, so a message that arrives twice (a reconnect
//! resends nothing today, but a forwarder might later) is listed once. `sent_ms` is the sender's
//! call clock, shown only as the order the sender wrote in. Bytes after the text are ignored, so a
//! later version can add fields. Pure: no azul types, unit-tested here.

use std::collections::BTreeSet;

/// The kind byte of a chat message (after video 2..5, sync 6 and relay 7).
pub const KIND_CHAT: u8 = 8;
/// The version this side writes.
const VERSION: u8 = 1;
/// The longest text a message carries, in bytes; a longer one is cut at a character boundary.
pub const MAX_CHAT_BYTES: usize = 2000;
/// How many messages the panel keeps; older ones go.
pub const MAX_MESSAGES: usize = 500;

/// A chat message as it travels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WireChat {
    pub seq: u32,
    pub sent_ms: u64,
    pub text: String,
}

/// One line of the chat panel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChatMessage {
    /// The sender's peer key (`routes::peer_key`); this side's own for a message written here.
    pub from: u64,
    /// Written on this side.
    pub mine: bool,
    /// The sender's name when the message arrived.
    pub name: String,
    pub text: String,
}

/// The bytes of a chat message.
pub fn encode_chat(message: &WireChat) -> Vec<u8> {
    let _ = message;
    Vec::new()
}

/// A chat message; `None` for another kind, another version's layout, or a short message.
pub fn decode_chat(bytes: &[u8]) -> Option<WireChat> {
    let _ = bytes;
    None
}

/// `text` trimmed, cut to at most `max` bytes at a character boundary; `None` when nothing is
/// left.
pub fn clean_text(text: &str, max: usize) -> Option<String> {
    let _ = (text, max);
    None
}

/// The panel's messages, oldest first, and how many arrived while it was closed.
#[derive(Debug, Default)]
pub struct ChatLog {
    messages: Vec<ChatMessage>,
    /// (sender key, seq) of every message listed, so a message that arrives twice is listed once.
    seen: BTreeSet<(u64, u32)>,
    /// This side's last `seq`.
    sent: u32,
    unread: u32,
}

impl ChatLog {
    pub fn new() -> Self {
        ChatLog::default()
    }

    /// A message written here as `me` called `name`: listed, and its bytes to send to everyone.
    /// `None` for an empty message.
    pub fn compose(&mut self, me: u64, name: &str, text: &str, now_ms: u64) -> Option<Vec<u8>> {
        let _ = (me, name, text, now_ms);
        None
    }

    /// A message that arrived from the peer `from` called `name`; `panel_open` says whether it
    /// is read at once. True when it was listed (a new message).
    pub fn receive(&mut self, from: u64, name: &str, bytes: &[u8], panel_open: bool) -> bool {
        let _ = (from, name, bytes, panel_open);
        false
    }

    /// The messages, oldest first.
    pub fn messages(&self) -> &[ChatMessage] {
        &self.messages
    }

    /// Messages that arrived while the panel was closed.
    pub fn unread(&self) -> u32 {
        self.unread
    }

    /// The panel was opened: everything is read.
    pub fn mark_read(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    const ADA: u64 = 0xada;
    const BEN: u64 = 0xbe2;

    #[test]
    fn a_chat_message_survives_the_wire() {
        let message = WireChat {
            seq: 7,
            sent_ms: 123_456,
            text: String::from("Hello, Ben! Grüße"),
        };
        let bytes = encode_chat(&message);
        assert_eq!(bytes[0], KIND_CHAT);
        assert_eq!(decode_chat(&bytes), Some(message));
    }

    #[test]
    fn another_kind_a_short_message_or_bad_text_is_no_chat_message() {
        let bytes = encode_chat(&WireChat {
            seq: 1,
            sent_ms: 0,
            text: String::from("hi"),
        });
        assert_eq!(decode_chat(&[]), None);
        assert_eq!(decode_chat(&[2, 1, 0]), None, "a video packet");
        assert_eq!(decode_chat(&bytes[..bytes.len() - 1]), None, "cut short");
        let mut bad = bytes.clone();
        let last = bad.len() - 1;
        bad[last] = 0xff;
        assert_eq!(decode_chat(&bad), None, "not UTF-8");
        let mut longer = bytes;
        longer.extend_from_slice(b"later fields");
        assert_eq!(
            decode_chat(&longer).map(|m| m.text),
            Some(String::from("hi")),
            "bytes after the text are ignored"
        );
    }

    #[test]
    fn a_message_is_trimmed_and_cut_at_a_character_boundary() {
        assert_eq!(clean_text("   ", 100), None);
        assert_eq!(clean_text("  hi \n", 100), Some(String::from("hi")));
        // "ü" is two bytes: a cut in its middle keeps the whole character out.
        assert_eq!(clean_text("aü", 2), Some(String::from("a")));
        assert_eq!(clean_text("aü", 3), Some(String::from("aü")));
    }

    #[test]
    fn what_this_side_writes_is_listed_and_goes_out_numbered() {
        let mut log = ChatLog::new();
        assert_eq!(log.compose(ADA, "Ada", "   ", 5), None, "nothing to send");
        let first = log.compose(ADA, "Ada", "Hi all", 10).expect("bytes to send");
        let second = log.compose(ADA, "Ada", "Second", 20).expect("bytes to send");
        assert_eq!(decode_chat(&first).map(|m| m.seq), Some(1));
        assert_eq!(decode_chat(&second).map(|m| m.seq), Some(2));
        let listed: Vec<(&str, bool)> = log
            .messages()
            .iter()
            .map(|m| (m.text.as_str(), m.mine))
            .collect();
        assert_eq!(listed, vec![("Hi all", true), ("Second", true)]);
        assert_eq!(log.unread(), 0, "what this side writes is read");
    }

    #[test]
    fn a_message_from_a_peer_is_listed_once_and_counts_as_unread_while_the_panel_is_closed() {
        let mut ada = ChatLog::new();
        let bytes = ada.compose(ADA, "Ada", "Can you hear me?", 10).unwrap();
        let mut ben = ChatLog::new();
        assert!(ben.receive(ADA, "Ada", &bytes, false));
        assert!(!ben.receive(ADA, "Ada", &bytes, false), "the same message again");
        assert_eq!(ben.messages().len(), 1);
        let m = &ben.messages()[0];
        assert_eq!((m.from, m.name.as_str(), m.text.as_str(), m.mine), (ADA, "Ada", "Can you hear me?", false));
        assert_eq!(ben.unread(), 1);
        ben.mark_read();
        assert_eq!(ben.unread(), 0);
        let more = ada.compose(ADA, "Ada", "Hello?", 20).unwrap();
        assert!(ben.receive(ADA, "Ada", &more, true));
        assert_eq!(ben.unread(), 0, "read at once in an open panel");
        assert!(!ben.receive(BEN, "Ben", &[1, 0], true), "not a chat message");
    }

    #[test]
    fn the_panel_keeps_the_newest_messages() {
        let mut log = ChatLog::new();
        for i in 0..MAX_MESSAGES + 10 {
            log.compose(ADA, "Ada", &format!("m{i}"), i as u64);
        }
        assert_eq!(log.messages().len(), MAX_MESSAGES);
        assert_eq!(log.messages()[0].text, "m10");
    }
}

//! What the submission port filed in Sent lately, by Message-ID. Apple Mail, Outlook and
//! Thunderbird put their own copy of a sent mail into the Sent mailbox over IMAP (APPEND)
//! right after handing it to SMTP; the bridge has filed the copy that went out (signed) by
//! then, so the IMAP server answers that APPEND with the message already there instead of
//! filing a second one.

use std::{
    collections::VecDeque,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::mime;

/// How long a filed copy is remembered.
pub const KEEP: Duration = Duration::from_secs(15 * 60);
/// At most this many are remembered.
pub const MAX_ENTRIES: usize = 1_000;

#[derive(Debug, Clone)]
struct Entry {
    at: Instant,
    message_id: String,
    mailbox: String,
    name: String,
}

/// Recently filed sent mail.
#[derive(Debug, Default)]
pub struct SentRegistry {
    entries: Mutex<VecDeque<Entry>>,
}

/// A message's Message-ID as compared here: without angle brackets and surrounding blanks,
/// in lower case; `None` when it has none.
#[must_use]
pub fn message_id_of(bytes: &[u8]) -> Option<String> {
    let (end, _) = mime::split(bytes);
    let value = mime::field(&bytes[..end], "Message-ID")?;
    let id = value
        .trim()
        .trim_start_matches('<')
        .trim_end_matches('>')
        .trim()
        .to_ascii_lowercase();
    (!id.is_empty()).then_some(id)
}

impl SentRegistry {
    #[must_use]
    pub fn new() -> SentRegistry {
        SentRegistry::default()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, VecDeque<Entry>> {
        self.entries
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// The message `message_id` was filed as `name` in the mailbox `mailbox`.
    pub fn remember(&self, message_id: &str, mailbox: &str, name: &str) {
        let mut entries = self.lock();
        let now = Instant::now();
        while entries
            .front()
            .is_some_and(|e| now.duration_since(e.at) > KEEP)
            || entries.len() >= MAX_ENTRIES
        {
            entries.pop_front();
        }
        entries.push_back(Entry {
            at: now,
            message_id: message_id.to_ascii_lowercase(),
            mailbox: mailbox.to_string(),
            name: name.to_string(),
        });
    }

    /// Where the message `message_id` was filed within [`KEEP`]: `(mailbox, name)`.
    #[must_use]
    pub fn find(&self, message_id: &str) -> Option<(String, String)> {
        let wanted = message_id.to_ascii_lowercase();
        let now = Instant::now();
        self.lock()
            .iter()
            .rev()
            .find(|e| e.message_id == wanted && now.duration_since(e.at) <= KEEP)
            .map(|e| (e.mailbox.clone(), e.name.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_id_is_compared_without_brackets_or_case() {
        assert_eq!(
            message_id_of(b"Subject: x\r\nMessage-ID:  <M7@Example.org> \r\n\r\nbody"),
            Some("m7@example.org".to_string())
        );
        assert_eq!(message_id_of(b"Subject: x\r\n\r\nbody"), None);
    }

    #[test]
    fn a_filed_copy_is_found_by_its_message_id() {
        let registry = SentRegistry::new();
        registry.remember("m7@example.org", "Sent", "a.eml");
        assert_eq!(
            registry.find("M7@EXAMPLE.ORG"),
            Some(("Sent".to_string(), "a.eml".to_string()))
        );
        assert_eq!(registry.find("other@example.org"), None);
        for i in 0..MAX_ENTRIES + 5 {
            registry.remember(&format!("{i}@x"), "Sent", "n.eml");
        }
        assert_eq!(registry.find("m7@example.org"), None, "the oldest go first");
        assert!(registry.find(&format!("{}@x", MAX_ENTRIES + 4)).is_some());
    }
}

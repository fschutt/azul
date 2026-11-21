//! `data:` URIs: the resources markup embeds in itself - an SVG `<image>`'s
//! picture, an `@font-face`'s font (the SVG of a PDF page carries both).

use std::{
    collections::VecDeque,
    hash::{Hash, Hasher},
    sync::{Mutex, PoisonError},
};

/// The bytes a base64 `data:` URI carries (`data:font/otf;charset=utf-8;base64,...`).
/// `None` for another URI, one that is not base64, or a payload that does not
/// decode.
#[must_use]
pub fn base64_payload(uri: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;

    let (header, data) = uri.trim().strip_prefix("data:")?.split_once(',')?;
    if !header
        .split(';')
        .any(|part| part.trim().eq_ignore_ascii_case("base64"))
    {
        return None;
    }
    // Markup wraps a long payload across lines.
    let data: Vec<u8> = data.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
    base64::engine::general_purpose::STANDARD.decode(data).ok()
}

/// What was made from the last [`Self::capacity`] distinct `data:` URIs: made
/// ONCE per URI - a page DOM rebuilt as it scrolls back into view gets the
/// same picture / font again - and forgotten oldest first, so a document
/// that scrolls through a thousand pages does not keep a thousand fonts.
#[derive(Debug)]
pub struct UriMemo<T> {
    capacity: usize,
    entries: Mutex<VecDeque<(u64, T)>>,
}

impl<T: Clone> UriMemo<T> {
    #[must_use]
    pub const fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: Mutex::new(VecDeque::new()),
        }
    }

    /// How many it remembers.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// The thing `make` made of `uri`'s bytes ([`base64_payload`]), made
    /// again only when `uri` has been forgotten. `None` when the URI carries
    /// no bytes or `make` makes nothing of them (not remembered).
    pub fn get_or_make(&self, uri: &str, make: impl FnOnce(Vec<u8>) -> Option<T>) -> Option<T> {
        let key = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            uri.hash(&mut h);
            h.finish()
        };
        if let Some((_, made)) = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .iter()
            .find(|(k, _)| *k == key)
        {
            return Some(made.clone());
        }
        // Made outside the lock: a font parses for milliseconds.
        let made = make(base64_payload(uri)?)?;
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        // Made meanwhile by another thread: ONE thing per URI (two would be
        // two fonts to every cache keyed by identity).
        if let Some((_, first)) = entries.iter().find(|(k, _)| *k == key) {
            return Some(first.clone());
        }
        if entries.len() >= self.capacity {
            entries.pop_front();
        }
        entries.push_back((key, made.clone()));
        drop(entries);
        Some(made)
    }
}

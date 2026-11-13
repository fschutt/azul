//! A small memo for pictures a renderer would otherwise draw again on every
//! frame from the same inputs: an SVG shape's mask, a text shadow's halo.
//!
//! It is bounded twice, by its entries and by the bytes they hold, and when a
//! new entry would pass either bound it drops the entry used longest ago. A
//! value larger than the whole byte budget is not kept at all.

use std::{collections::HashMap, hash::Hash};

/// What the memo keeps for one key.
#[derive(Debug)]
struct Entry<V> {
    value: V,
    /// What `value` holds, as the caller counted it (a picture's pixel bytes).
    bytes: usize,
    /// The memo's clock when the entry was last asked for or stored.
    last_used: u64,
}

/// A least-recently-used memo from `K` to `V`.
#[derive(Debug)]
pub struct ImageMemo<K, V> {
    entries: HashMap<K, Entry<V>>,
    /// The sum of every entry's `bytes`.
    bytes: usize,
    /// Ticks once per lookup; orders the entries by their last use.
    clock: u64,
    /// The most entries kept at once.
    max_entries: usize,
    /// The most bytes the entries may hold together.
    max_bytes: usize,
}

impl<K: Hash + Eq + Copy, V: Clone> ImageMemo<K, V> {
    /// An empty memo that keeps at most `max_entries` entries holding at most
    /// `max_bytes` bytes.
    #[must_use]
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            entries: HashMap::new(),
            bytes: 0,
            clock: 0,
            max_entries,
            max_bytes,
        }
    }

    /// The value kept for `key`, which counts as a use of it.
    pub fn get(&mut self, key: &K) -> Option<V> {
        self.clock += 1;
        let clock = self.clock;
        self.entries.get_mut(key).map(|entry| {
            entry.last_used = clock;
            entry.value.clone()
        })
    }

    /// Keeps `value` for `key`, dropping the entries used longest ago until
    /// it fits both bounds. A value of more than `max_bytes` is not kept.
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        if bytes > self.max_bytes {
            return;
        }
        if let Some(old) = self.entries.remove(&key) {
            self.bytes -= old.bytes;
        }
        while !self.entries.is_empty()
            && (self.entries.len() >= self.max_entries || self.bytes + bytes > self.max_bytes)
        {
            let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(key, _)| *key)
            else {
                break;
            };
            if let Some(gone) = self.entries.remove(&oldest) {
                self.bytes -= gone.bytes;
            }
        }
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                last_used: self.clock,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::ImageMemo;

    #[test]
    fn the_entry_used_longest_ago_makes_room_for_a_new_one() {
        let mut memo = ImageMemo::new(2, 100);
        memo.insert(1, "one", 10);
        memo.insert(2, "two", 10);
        assert_eq!(memo.get(&1), Some("one"));
        memo.insert(3, "three", 10);
        assert_eq!(memo.get(&2), None, "two was used longest ago");
        assert_eq!(memo.get(&1), Some("one"));
        assert_eq!(memo.get(&3), Some("three"));
    }

    #[test]
    fn a_value_larger_than_the_byte_budget_is_not_kept() {
        let mut memo = ImageMemo::new(8, 100);
        memo.insert(1, "small", 60);
        memo.insert(2, "huge", 101);
        assert_eq!(memo.get(&2), None);
        assert_eq!(memo.get(&1), Some("small"), "and it drove nothing out");
        memo.insert(3, "medium", 60);
        assert_eq!(memo.get(&1), None, "the bytes bound drops the oldest");
    }
}

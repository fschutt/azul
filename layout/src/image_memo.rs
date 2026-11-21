//! A small memo for pictures a renderer would otherwise draw again on every
//! frame from the same inputs: an SVG shape's mask, a text shadow's halo.
//!
//! It is bounded twice, by its entries and by the bytes they hold, and when a
//! new entry would pass either bound it drops the entries used longest ago: as
//! many as it takes, and an eighth of them at least, so that the search for the
//! oldest - a look at every entry - is paid once per eighth of the memo and not
//! once per entry dropped. A value larger than the whole byte budget is not kept
//! at all.

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
    /// Ticks once per lookup and per insert; orders the entries by their last use.
    clock: u64,
    /// How many entries the searches for the oldest have looked at so far.
    scanned: u64,
    /// The most entries kept at once.
    max_entries: usize,
    /// The most bytes the entries may hold together.
    max_bytes: usize,
    /// Entries last used at or after this reading of the clock are never dropped to make room
    /// ([`Self::keep_used_since`]); `None`: any entry may go.
    keep_since: Option<u64>,
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
            scanned: 0,
            max_entries,
            max_bytes,
            keep_since: None,
        }
    }

    /// The clock's reading: it ticks once per lookup and per insert.
    #[must_use]
    pub const fn clock(&self) -> u64 {
        self.clock
    }

    /// From now on, an entry last used at or after the clock reading `since` is not dropped to
    /// make room; the memo then holds more than its bounds for as long as it must. A renderer
    /// passes the reading at the start of the frame before the one it paints: a frame that
    /// uses more pictures than the bound holds must not drop, while it paints, the pictures
    /// it or the frame before used (it would draw each again on every frame). `None` lifts it.
    pub const fn keep_used_since(&mut self, since: Option<u64>) {
        self.keep_since = since;
    }

    /// Bytes the entries hold, as their callers counted them.
    #[must_use]
    pub const fn bytes(&self) -> usize {
        self.bytes
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
        if !self.entries.is_empty()
            && (self.entries.len() >= self.max_entries || self.bytes + bytes > self.max_bytes)
        {
            self.make_room(bytes);
        }
        self.clock += 1;
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

    /// Drops the entries used longest ago until one more of `bytes` fits both
    /// bounds, and an eighth of the entries at least.
    ///
    /// The oldest was found by a look at every entry, once per entry dropped: a
    /// map's zoom that settles puts 1,567 new halos into a memo of 4,096, each
    /// insert at the bound, 6.4 million entries looked at (audit A9 H14). Sorted
    /// once and dropped in a batch, an insert at the bound looks at eight entries
    /// on average, with their order's logarithm.
    fn make_room(&mut self, bytes: usize) {
        let mut oldest: Vec<(u64, K)> = self
            .entries
            .iter()
            .map(|(key, entry)| (entry.last_used, *key))
            .collect();
        self.scanned += oldest.len() as u64;
        oldest.sort_unstable_by_key(|(last_used, _)| *last_used);
        let at_least = (oldest.len() / 8).max(1);
        for (dropped, (last_used, key)) in oldest.into_iter().enumerate() {
            let fits =
                self.entries.len() < self.max_entries && self.bytes + bytes <= self.max_bytes;
            if fits && dropped >= at_least {
                break;
            }
            // Oldest first: from the first entry that is kept on, every entry is.
            if self.keep_since.is_some_and(|since| last_used >= since) {
                break;
            }
            if let Some(gone) = self.entries.remove(&key) {
                self.bytes -= gone.bytes;
            }
        }
    }

    /// How many entries the searches for the oldest have looked at so far: what
    /// a test divides by the inserts.
    #[must_use]
    pub const fn entries_scanned(&self) -> u64 {
        self.scanned
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

/// Guards of audit A5 (`audit/A5.md`).
#[cfg(test)]
mod audit_a5_tests {
    use super::ImageMemo;

    /// A5-06 (A9 H14). An insert into a full memo looked at every entry to find
    /// the oldest: n inserts at the bound were n x n. Counted at 1,024 and at
    /// 4,096 entries, ten times the bound inserted: the entries looked at per
    /// insert do not grow with the memo (under 20 at both sizes; they were the
    /// bound itself, 1,024 and 4,096), and the bounds hold all the way.
    ///
    /// Mutation: `let at_least = 1;` in `make_room` (one entry per search) fails
    /// this: 921 and 3,686 entries looked at per insert.
    #[test]
    fn an_insert_at_the_bound_does_not_look_at_every_entry() {
        for bound in [1024usize, 4096] {
            let mut memo = ImageMemo::<u64, u64>::new(bound, usize::MAX);
            let inserts = 10 * bound as u64;
            for key in 0..inserts {
                memo.insert(key, key, 1024);
                assert!(memo.entries.len() <= bound, "{key}: the entries bound holds");
                assert_eq!(memo.bytes, memo.entries.len() * 1024, "{key}: the bytes add up");
            }
            let per_insert = memo.entries_scanned() / inserts;
            assert!(per_insert < 20, "{bound}: {per_insert} entries looked at per insert");
            assert_eq!(memo.get(&(inserts - 1)), Some(inserts - 1), "the newest is kept");
            assert_eq!(memo.get(&0), None, "the oldest went");
        }
    }

    /// Entries that were only ever stored have an order too: the one stored
    /// first goes first (the clock stood still between lookups, and which of
    /// them went was the map's iteration order).
    #[test]
    fn entries_that_were_never_asked_for_go_in_the_order_they_came() {
        let mut memo = ImageMemo::new(3, 100);
        memo.insert(1, "one", 10);
        memo.insert(2, "two", 10);
        memo.insert(3, "three", 10);
        memo.insert(4, "four", 10);
        assert_eq!(memo.get(&1), None, "the first stored made room");
        assert_eq!(memo.get(&2), Some("two"));
        assert_eq!(memo.get(&4), Some("four"));
        // By bytes, as many as it takes: 30 held, 85 more need two to go.
        memo.insert(5, "five", 85);
        assert_eq!((memo.get(&3), memo.get(&2)), (None, None), "3 was stored before 2 was asked for");
        assert_eq!(memo.get(&4), Some("four"));
        assert_eq!(memo.get(&5), Some("five"));
    }

    /// A1-22: entries used since a given reading of the clock are not dropped to make room;
    /// older ones are. A memo of two entries: 1 is old, 2 and 3 were used since the mark; a
    /// fourth drops 1 and keeps 2 and 3 although the memo then holds three.
    /// Mutation: the check left out of `make_room` drops 2.
    #[test]
    fn entries_used_since_the_mark_are_not_dropped_to_make_room() {
        let mut memo = ImageMemo::new(2, 100);
        memo.insert(1, "one", 10);
        let mark = memo.clock();
        memo.insert(2, "two", 10);
        memo.keep_used_since(Some(mark + 1));
        memo.insert(3, "three", 10);
        assert_eq!(memo.get(&1), None, "one is older than the mark: it made room");
        memo.insert(4, "four", 10);
        assert_eq!((memo.get(&2), memo.get(&3), memo.get(&4)), (Some("two"), Some("three"), Some("four")));
        assert_eq!(memo.bytes(), 30, "over its bound of two entries, by what is in use");
        memo.keep_used_since(None);
        memo.insert(5, "five", 10);
        assert!(memo.bytes() <= 20, "and back inside it once nothing is kept");
    }
}

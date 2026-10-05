//! The history of one measure: a ring of the last N readings, oldest first.
//!
//! The monitor keeps one per measure (the whole CPU, each core, memory,
//! disk read / write, network in / out): 60 readings for the charts at one
//! reading a second, the oldest dropped as a new one arrives. Pushing never
//! allocates once the ring is full - a tick costs one store per measure.

/// A fixed-capacity ring of readings, read oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct History {
    /// The readings; once full, `start` is the oldest.
    values: Vec<f64>,
    /// The index of the oldest reading once the ring is full (0 before).
    start: usize,
    /// How many readings it keeps.
    capacity: usize,
}

impl History {
    /// An empty history of at most `capacity` readings (at least one).
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.max(1);
        Self {
            values: Vec::with_capacity(capacity),
            start: 0,
            capacity,
        }
    }

    /// Adds the newest reading, dropping the oldest when full.
    pub fn push(&mut self, value: f64) {
        if self.values.len() < self.capacity {
            self.values.push(value);
        } else {
            // Full: the oldest slot takes the newest reading, and the next
            // slot is the oldest now.
            self.values[self.start] = value;
            self.start = (self.start + 1) % self.capacity;
        }
    }

    /// How many readings it holds.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether it holds none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// How many readings it keeps at most.
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// The newest reading.
    #[must_use]
    pub fn latest(&self) -> Option<f64> {
        if self.values.is_empty() {
            return None;
        }
        // Not full: the last pushed is the last stored. Full: the one
        // before the oldest.
        let newest = if self.values.len() < self.capacity {
            self.values.len() - 1
        } else {
            (self.start + self.capacity - 1) % self.capacity
        };
        self.values.get(newest).copied()
    }

    /// The readings, oldest first.
    #[must_use]
    pub fn to_vec(&self) -> Vec<f64> {
        let (newer, older) = self.values.split_at(self.start);
        older.iter().chain(newer.iter()).copied().collect()
    }

    /// The largest reading (`None` when empty).
    #[must_use]
    pub fn max(&self) -> Option<f64> {
        self.values.iter().copied().reduce(f64::max)
    }

    /// Forgets every reading (the capacity stays).
    pub fn clear(&mut self) {
        self.values.clear();
        self.start = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_history_is_empty_and_keeps_its_capacity() {
        let h = History::new(60);
        assert!(h.is_empty());
        assert_eq!(h.len(), 0);
        assert_eq!(h.capacity(), 60);
        assert_eq!(h.latest(), None);
        assert_eq!(h.max(), None);
        assert!(h.to_vec().is_empty());
    }

    #[test]
    fn a_history_of_no_capacity_still_keeps_one_reading() {
        let mut h = History::new(0);
        assert_eq!(h.capacity(), 1);
        h.push(3.0);
        h.push(4.0);
        assert_eq!(h.to_vec(), vec![4.0]);
    }

    #[test]
    fn readings_come_back_oldest_first_until_the_ring_is_full() {
        let mut h = History::new(4);
        h.push(1.0);
        h.push(2.0);
        h.push(3.0);
        assert_eq!(h.len(), 3);
        assert_eq!(h.to_vec(), vec![1.0, 2.0, 3.0]);
        assert_eq!(h.latest(), Some(3.0));
    }

    #[test]
    fn a_full_ring_drops_the_oldest_reading_for_the_newest() {
        let mut h = History::new(3);
        for v in 1..=7 {
            h.push(f64::from(v));
        }
        assert_eq!(h.len(), 3);
        assert_eq!(h.to_vec(), vec![5.0, 6.0, 7.0]);
        assert_eq!(h.latest(), Some(7.0));
        assert_eq!(h.max(), Some(7.0));
    }

    #[test]
    fn the_largest_reading_is_found_wherever_the_ring_starts() {
        let mut h = History::new(3);
        for v in [9.0, 1.0, 2.0, 3.0] {
            h.push(v);
        }
        // 9 has been dropped: the largest of 1, 2, 3 is 3.
        assert_eq!(h.max(), Some(3.0));
        h.push(2.5);
        assert_eq!(h.to_vec(), vec![2.0, 3.0, 2.5]);
        assert_eq!(h.max(), Some(3.0));
    }

    #[test]
    fn a_long_run_of_pushes_never_grows_the_ring() {
        let mut h = History::new(60);
        for v in 0..10_000 {
            h.push(f64::from(v));
        }
        assert_eq!(h.len(), 60);
        assert_eq!(h.to_vec().first().copied(), Some(9_940.0));
        assert_eq!(h.latest(), Some(9_999.0));
    }

    #[test]
    fn clearing_forgets_the_readings_but_not_the_capacity() {
        let mut h = History::new(5);
        h.push(1.0);
        h.push(2.0);
        h.clear();
        assert!(h.is_empty());
        assert_eq!(h.capacity(), 5);
        h.push(8.0);
        assert_eq!(h.to_vec(), vec![8.0]);
    }
}

//! The play queue: what plays now and what comes next - an album or a playlist from a track on,
//! shuffled or in order, repeated or not, with "play next" and "add to queue". Plain Rust, tested
//! without a window.
//!
//! The player plays GAPLESSLY: while a track plays, the queue names the one after it
//! ([`PlayQueue::upcoming`]) so the app hands it to the `AudioPlayer` in advance, and moves on
//! ([`PlayQueue::advance`]) when the player reports that the next track is heard.

/// How the queue repeats.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Repeat {
    /// Stop after the last track.
    #[default]
    Off,
    /// Start over after the last track.
    All,
    /// Play the current track again and again.
    One,
}

/// What "previous" does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Previous {
    /// Start the current track over (it played more than [`RESTART_AFTER_S`]).
    Restart,
    /// Play this track.
    Track(String),
}

/// After this many seconds "previous" restarts the track instead of going back.
pub const RESTART_AFTER_S: f64 = 3.0;

/// The queue: track ids, the play order (a permutation when shuffled) and where it is.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PlayQueue {
    /// The tracks, in the order they were queued.
    pub items: Vec<String>,
    /// The play order: indices into `items`.
    order: Vec<usize>,
    /// Where the queue is in `order`.
    pos: Option<usize>,
    pub shuffle: bool,
    pub repeat: Repeat,
    /// The shuffle's random state (xorshift).
    seed: u64,
}

impl PlayQueue {
    /// A queue of `items` playing `items[start]` (in order, not shuffled).
    #[must_use]
    pub fn new(items: Vec<String>, start: usize) -> Self {
        let order: Vec<usize> = (0..items.len()).collect();
        let pos = (!items.is_empty()).then(|| start.min(items.len() - 1));
        Self {
            items,
            order,
            pos,
            shuffle: false,
            repeat: Repeat::Off,
            seed: 0,
        }
    }

    /// The track playing (or to play).
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        let i = *self.order.get(self.pos?)?;
        self.items.get(i).map(String::as_str)
    }

    /// The tracks in play order from the current one on (for the queue panel).
    #[must_use]
    pub fn up_next(&self) -> Vec<&str> {
        let Some(p) = self.pos else {
            return Vec::new();
        };
        self.order[p.min(self.order.len())..]
            .iter()
            .map(|i| self.items[*i].as_str())
            .collect()
    }

    /// Shuffle on (the current track stays current, the rest in a random order from `seed`) or
    /// off (back to the queued order, the current track still current).
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        let current = self.pos.and_then(|p| self.order.get(p).copied());
        let n = self.items.len();
        if on {
            let mut rest: Vec<usize> = (0..n).filter(|i| Some(*i) != current).collect();
            // Fisher-Yates on a xorshift64 stream: the same seed, the same order.
            let mut state = seed | 1;
            for i in (1..rest.len()).rev() {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                let j = (state % (i as u64 + 1)) as usize;
                rest.swap(i, j);
            }
            self.order = current.into_iter().chain(rest).collect();
            self.pos = current.map(|_| 0);
            self.seed = state;
        } else {
            self.order = (0..n).collect();
            self.pos = current;
        }
        self.shuffle = on;
    }

    /// The track that plays after the current one: the same one on repeat-one, the first one
    /// after the last on repeat-all, none after the last otherwise.
    #[must_use]
    pub fn upcoming(&self) -> Option<&str> {
        let next = self.next_pos(true)?;
        self.items.get(self.order[next]).map(String::as_str)
    }

    /// Where the queue goes after the current track (`hold`: repeat-one holds it).
    fn next_pos(&self, hold: bool) -> Option<usize> {
        let p = self.pos?;
        if hold && self.repeat == Repeat::One {
            Some(p)
        } else if p + 1 < self.order.len() {
            Some(p + 1)
        } else if self.repeat != Repeat::Off && !self.order.is_empty() {
            Some(0)
        } else {
            None
        }
    }

    /// Moves on to [`upcoming`](Self::upcoming); `None` (and nothing current) at the end.
    pub fn advance(&mut self) -> Option<String> {
        self.pos = self.next_pos(true);
        self.current().map(str::to_string)
    }

    /// Skips to the next track (the user's "next": repeat-one does not hold it).
    pub fn skip(&mut self) -> Option<String> {
        self.pos = self.next_pos(false);
        self.current().map(str::to_string)
    }

    /// The user's "previous" at `position_s` into the current track.
    pub fn previous(&mut self, position_s: f64) -> Previous {
        if position_s > RESTART_AFTER_S {
            return Previous::Restart;
        }
        match self.pos {
            Some(p) if p > 0 => self.pos = Some(p - 1),
            Some(_) if self.repeat == Repeat::All && self.order.len() > 1 => {
                self.pos = Some(self.order.len() - 1);
            }
            _ => return Previous::Restart,
        }
        self.current()
            .map_or(Previous::Restart, |id| Previous::Track(id.to_string()))
    }

    /// Plays `id` right after the current track.
    pub fn play_next(&mut self, id: String) {
        self.items.push(id);
        let index = self.items.len() - 1;
        match self.pos {
            Some(p) => self.order.insert(p + 1, index),
            None => {
                self.order.push(index);
                self.pos = Some(self.order.len() - 1);
            }
        }
    }

    /// Plays `id` after everything queued.
    pub fn enqueue(&mut self, id: String) {
        self.items.push(id);
        self.order.push(self.items.len() - 1);
        if self.pos.is_none() {
            self.pos = Some(self.order.len() - 1);
        }
    }

    /// Nothing queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("t{i}")).collect()
    }

    #[test]
    fn a_queue_plays_from_the_chosen_track_in_order_and_stops_at_the_end() {
        let mut q = PlayQueue::new(ids(4), 1);
        assert_eq!(q.current(), Some("t1"));
        assert_eq!(q.up_next(), vec!["t1", "t2", "t3"]);
        assert_eq!(q.upcoming(), Some("t2"));
        assert_eq!(q.advance().as_deref(), Some("t2"));
        assert_eq!(q.advance().as_deref(), Some("t3"));
        assert_eq!(q.upcoming(), None, "nothing after the last");
        assert_eq!(q.advance(), None);
        assert_eq!(q.current(), None, "the queue ran out");
    }

    #[test]
    fn repeat_all_starts_over_and_repeat_one_holds_the_track_but_not_against_next() {
        let mut q = PlayQueue::new(ids(3), 2);
        q.repeat = Repeat::All;
        assert_eq!(q.upcoming(), Some("t0"));
        assert_eq!(q.advance().as_deref(), Some("t0"));
        q.repeat = Repeat::One;
        assert_eq!(q.upcoming(), Some("t0"));
        assert_eq!(q.advance().as_deref(), Some("t0"));
        assert_eq!(q.skip().as_deref(), Some("t1"), "the user's next moves on");
    }

    #[test]
    fn shuffle_keeps_the_current_track_plays_every_track_once_and_is_reproducible() {
        let mut q = PlayQueue::new(ids(10), 3);
        q.set_shuffle(true, 42);
        assert_eq!(q.current(), Some("t3"));
        let order = q
            .up_next()
            .iter()
            .map(|s| s.to_string())
            .collect::<Vec<_>>();
        assert_eq!(order.len(), 10);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, {
            let mut all = ids(10);
            all.sort();
            all
        });
        assert_ne!(order, ids(10)[3..].to_vec(), "shuffled");
        let mut again = PlayQueue::new(ids(10), 3);
        again.set_shuffle(true, 42);
        assert_eq!(
            again.up_next(),
            q.up_next(),
            "the same seed, the same order"
        );
        q.advance();
        let now = q.current().map(str::to_string);
        q.set_shuffle(false, 0);
        assert_eq!(
            q.current().map(str::to_string),
            now,
            "unshuffling keeps the track"
        );
    }

    #[test]
    fn previous_restarts_a_track_played_a_while_and_goes_back_otherwise() {
        let mut q = PlayQueue::new(ids(3), 1);
        assert_eq!(q.previous(10.0), Previous::Restart);
        assert_eq!(q.previous(1.0), Previous::Track("t0".into()));
        assert_eq!(q.current(), Some("t0"));
        assert_eq!(
            q.previous(1.0),
            Previous::Restart,
            "at the start: start over"
        );
    }

    #[test]
    fn play_next_comes_right_after_the_current_track_and_enqueue_at_the_end() {
        let mut q = PlayQueue::new(ids(3), 0);
        q.enqueue("x".into());
        q.play_next("n".into());
        assert_eq!(q.up_next(), vec!["t0", "n", "t1", "t2", "x"]);
        let mut empty = PlayQueue::default();
        empty.enqueue("only".into());
        assert_eq!(
            empty.current(),
            Some("only"),
            "the first track queued plays"
        );
    }
}

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
        let _ = (items, start);
        Self::default()
    }

    /// The track playing (or to play).
    #[must_use]
    pub fn current(&self) -> Option<&str> {
        None
    }

    /// The tracks in play order from the current one on (for the queue panel).
    #[must_use]
    pub fn up_next(&self) -> Vec<&str> {
        Vec::new()
    }

    /// Shuffle on (the current track stays current, the rest in a random order from `seed`) or
    /// off (back to the queued order, the current track still current).
    pub fn set_shuffle(&mut self, on: bool, seed: u64) {
        let _ = (on, seed);
    }

    /// The track that plays after the current one: the same one on repeat-one, the first one
    /// after the last on repeat-all, none after the last otherwise.
    #[must_use]
    pub fn upcoming(&self) -> Option<&str> {
        None
    }

    /// Moves on to [`upcoming`](Self::upcoming); `None` (and nothing current) at the end.
    pub fn advance(&mut self) -> Option<String> {
        None
    }

    /// Skips to the next track (the user's "next": repeat-one does not hold it).
    pub fn skip(&mut self) -> Option<String> {
        None
    }

    /// The user's "previous" at `position_s` into the current track.
    pub fn previous(&mut self, position_s: f64) -> Previous {
        let _ = position_s;
        Previous::Restart
    }

    /// Plays `id` right after the current track.
    pub fn play_next(&mut self, id: String) {
        let _ = id;
    }

    /// Plays `id` after everything queued.
    pub fn enqueue(&mut self, id: String) {
        let _ = id;
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

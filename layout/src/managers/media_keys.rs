//! Media keys arriving from OUTSIDE the keyboard stream.
//!
//! On Linux the desktop environment usually grabs the media keys, so
//! `XF86AudioPlay` and friends never reach the application as keysyms - the
//! 9h-i keysym table only sees them when nothing grabbed them. The transport in
//! that case is MPRIS over D-Bus, which arrives on a D-Bus thread rather than
//! in a window's event stream, so it needs a channel like the sensor and HID
//! backends have.
//!
//! What comes out is an ordinary [`VirtualKeyCode`], because that is the
//! contract every other media-key producer already follows: the Win32
//! `WM_APPCOMMAND` arm and the X11/Wayland keysym table both deliver
//! `PlayPause` as a normal key, so an app binding it works unchanged
//! everywhere.

use azul_core::{
    media_session::{MediaControlRequest, SystemAudioChange},
    window::VirtualKeyCode,
};

use super::shared_queue::SharedQueue;

/// A person cannot press play more often than this between frames; anything
/// beyond it is a stuck sender, and an unbounded queue would grow for the life
/// of the process.
const MAX_PENDING: usize = 64;

/// The queues the platform's media controls report through, owned by one app: the media keys,
/// the seeks from the media widgets and the system audio changes. A backend (on whatever
/// thread its callback runs) holds a clone; the window's pass drains it. A clone is another
/// handle on the SAME queues.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MediaKeyQueues {
    keys: SharedQueue<VirtualKeyCode>,
    controls: SharedQueue<MediaControlRequest>,
    audio_changes: SharedQueue<SystemAudioChange>,
}

// A queue operation hands items over or says whether the item was taken: a caller that only
// wants the queue emptied, or the event parked, rightly ignores the answer.
#[allow(clippy::must_use_candidate)]
impl MediaKeyQueues {
    /// New queues of their own, shared with nobody.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Park a media key delivered by a platform backend, from any thread.
    ///
    /// # A key already pending is DROPPED, and that is what makes two transports
    /// safe
    ///
    /// On Windows both `WM_APPCOMMAND` and the SMTC `ButtonPressed` event can
    /// report the same physical press, and which of the two fires depends on
    /// whether an app has registered a media session - a question that cannot be
    /// answered from here, and answering it wrong either doubles every press or
    /// loses all of them. Collapsing a key that is ALREADY WAITING in this batch
    /// removes the question: one press produces one key however many transports
    /// saw it.
    ///
    /// This is not a time window and needs no constant. The queue is drained once
    /// per event pass, so "already pending" means "reported since the last frame";
    /// a person cannot press play twice inside one frame, and a genuine double
    /// press lands in different batches and survives. Same argument the haptic
    /// queue's adjacent-dedup makes.
    pub fn push_media_key(&self, key: VirtualKeyCode) {
        let _ = self.keys.push_unique_up_to(key, MAX_PENDING);
    }

    /// Drain the parked media keys, in arrival order.
    pub fn drain_media_keys(&self) -> Vec<VirtualKeyCode> {
        self.keys.drain()
    }

    /// Queue one seek request, queued like the keys above and for the same
    /// reason: the D-Bus / SMTC / remote command callbacks run on their own
    /// threads, and the window drains them at the top of its pass (9h-i-a-i-a).
    /// Unlike keys, seeks are NOT de-duplicated: two `Seek(+5s)` in one batch
    /// mean ten seconds, and a `SetPosition` after a `Seek` is a different
    /// destination.
    pub fn push_media_control(&self, request: MediaControlRequest) {
        self.controls.push(request);
    }

    /// Take every queued seek, oldest first.
    pub fn drain_media_controls(&self) -> Vec<MediaControlRequest> {
        self.controls.drain()
    }

    /// Queue one system audio change (9h-i-a-i-d-i) - an iOS interruption
    /// notification, an Android focus change - arriving on its own thread, drained at
    /// the top of the pass. NOT de-duplicated: `Interrupted` then `Resumed` in one
    /// batch is two things the app must do.
    pub fn push_system_audio_change(&self, change: SystemAudioChange) {
        self.audio_changes.push(change);
    }

    /// Take every queued change, oldest first.
    pub fn drain_system_audio_changes(&self) -> Vec<SystemAudioChange> {
        self.audio_changes.drain()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_drain_in_arrival_order_and_empty_the_queue() {
        let queues = MediaKeyQueues::new();
        queues.push_media_key(VirtualKeyCode::PlayPause);
        queues.push_media_key(VirtualKeyCode::NextTrack);
        let got = queues.drain_media_keys();
        assert_eq!(
            got,
            vec![VirtualKeyCode::PlayPause, VirtualKeyCode::NextTrack]
        );
        assert!(queues.drain_media_keys().is_empty());
    }

    /// A stuck sender must not grow the queue without bound.
    /// TWO TRANSPORTS, ONE PRESS. Windows can report the same media key
    /// through `WM_APPCOMMAND` and through SMTC's `ButtonPressed`; without
    /// this, a single play press would toggle playback twice and land back
    /// where it started.
    #[test]
    fn the_same_key_reported_twice_in_one_batch_collapses() {
        let queues = MediaKeyQueues::new();
        queues.push_media_key(VirtualKeyCode::PlayPause);
        queues.push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(queues.drain_media_keys(), vec![VirtualKeyCode::PlayPause]);

        // DIFFERENT keys in one batch are two real presses and both survive.
        queues.push_media_key(VirtualKeyCode::NextTrack);
        queues.push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(
            queues.drain_media_keys(),
            vec![VirtualKeyCode::NextTrack, VirtualKeyCode::PlayPause]
        );

        // And a genuine double press in SEPARATE batches is not collapsed -
        // the whole point of keying on "still pending" rather than on a timer.
        queues.push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(queues.drain_media_keys(), vec![VirtualKeyCode::PlayPause]);
        queues.push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(queues.drain_media_keys(), vec![VirtualKeyCode::PlayPause]);
    }

    #[test]
    fn a_stuck_sender_cannot_grow_the_queue() {
        let queues = MediaKeyQueues::new();
        // A STUCK SENDER CANNOT GROW THE QUEUE AT ALL any more, which is a
        // stronger statement than the old `MAX_PENDING` cap: the dedup bounds
        // the queue by the number of DISTINCT keys, and there are four media
        // keys. `MAX_PENDING` stays as a belt-and-braces guard for a future
        // key set, so this asserts the real invariant rather than a cap it can
        // no longer reach.
        let keys = [
            VirtualKeyCode::PlayPause,
            VirtualKeyCode::NextTrack,
            VirtualKeyCode::PrevTrack,
            VirtualKeyCode::MediaStop,
        ];
        for i in 0..(MAX_PENDING + 50) {
            queues.push_media_key(keys[i % keys.len()]);
        }
        let drained = queues.drain_media_keys();
        assert_eq!(
            drained.len(),
            keys.len(),
            "a sender repeating four keys forever must leave exactly four, got {drained:?}"
        );
        assert!(drained.len() <= MAX_PENDING);
    }

    /// The queues belong to the app: another app's backends never feed these.
    #[test]
    fn the_queues_of_one_app_are_not_another_apps() {
        let ours = MediaKeyQueues::new();
        let theirs = MediaKeyQueues::new();
        ours.push_media_key(VirtualKeyCode::PlayPause);
        assert!(theirs.drain_media_keys().is_empty());
        let sink = ours.clone();
        sink.push_media_key(VirtualKeyCode::NextTrack);
        assert_eq!(
            ours.drain_media_keys(),
            vec![VirtualKeyCode::PlayPause, VirtualKeyCode::NextTrack],
            "a clone feeds the same queues"
        );
    }
}

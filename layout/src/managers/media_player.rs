//! Media playback state machine (ledger 11c).
//!
//! # Why this module exists
//!
//! `EventType::{Play, Pause, Ended, TimeUpdate, VolumeChange, MediaError}`
//! shipped in `azul_core::events` from the beginning, but there was nothing
//! behind them. `dll/src/unified/` has a decoder (`decode_mp4_h264`), an
//! encoder (`VideoEncoder`), an audio sink (`AudioSink::play`) and a screen
//! recorder — and **no player**: no transport, no `is_playing`, no position,
//! no duration, no volume. Six events described state changes in something
//! that did not exist, so they stayed pinned as *unmapped* in
//! `core/src/events_test.rs` and dispatched to nothing.
//!
//! This is the missing state machine: the smallest thing that can honestly
//! raise those six events. It is deliberately **not** a media stack. There is
//! no buffering, no playback rate, no track selection, no seeking semantics
//! beyond "the position moved", and no connection to the decoder or the sink.
//! It owns four numbers and a bool per node, and it says when they change.
//!
//! # Who drives the clock
//!
//! Nothing in azul decodes on a schedule, so nothing here can invent a wall
//! clock. [`MediaPlayerManager::advance`] is called by the app — from a
//! `Timer`, or from the decoder thread's write-back — through
//! `CallbackInfo::media_advance`. That is the honest wiring: the app owns the
//! media clock, the engine owns the state transitions and the events.
//!
//! # Event queueing
//!
//! Same shape as [`crate::managers::sensors`]: transitions push onto
//! `pending`, the `EventProvider` impl reads it non-destructively (it takes
//! `&self`), and the event pass calls
//! [`clear_pending_event`](MediaPlayerManager::clear_pending_event) once it
//! has collected them. A provider that never clears re-emits its whole queue
//! on every pass for the life of the window.

use alloc::{collections::BTreeMap, vec::Vec};

use azul_core::{
    dom::{DomId, DomNodeId},
    events::{
        EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent,
    },
    task::Instant,
};
pub use azul_core::media_player::{OptionPlaybackState, PlaybackState};

use super::{NodeIdMap, NodeIdRemap};

/// How much media time must elapse between two `TimeUpdate` events.
///
/// The web fires `timeupdate` about four times a second — deliberately far
/// below the frame rate, because a progress bar does not need 60 updates a
/// second and an event per frame would flood the event pass for the whole
/// duration of a playing video. 250 ms is that same 4/s budget.
///
/// The throttle is measured in MEDIA time (the position), not wall time: it
/// makes the rule deterministic — the same `advance` sequence always produces
/// the same events, which is what lets a test pin it.
pub const TIME_UPDATE_INTERVAL_S: f32 = 0.25;

/// One player: the observable [`PlaybackState`] plus the throttle
/// bookkeeping, which is deliberately NOT part of the public struct — an app
/// reading its media state has no business seeing when the last `TimeUpdate`
/// went out.
#[derive(Debug, Copy, Clone, PartialEq, Default)]
struct Player {
    state: PlaybackState,
    /// The position at which the last `TimeUpdate` was emitted.
    last_time_update_s: f32,
}

/// Per-node playback state plus the transport that mutates it.
///
/// Keyed by [`DomNodeId`], so it is NODE-keyed state and takes part in
/// [`NodeIdRemap`] — a rebuild renumbers the arena and unremapped state would
/// silently re-attach to a different element (see `managers/mod.rs`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct MediaPlayerManager {
    /// One entry per node the app has ever driven. Created lazily by the
    /// first transport call on that node.
    players: BTreeMap<DomNodeId, Player>,
    /// Transitions this pass has not yet collected, in the order they
    /// happened. Drained by `clear_pending_event`.
    pub pending: Vec<(DomNodeId, EventType)>,
}

/// Clamp that also answers for NaN (which every `f32::clamp` refuses to).
/// A NaN volume or position is a caller bug; the defensible reading is the
/// low bound, never a poisoned state that compares unequal to itself.
fn sanitize(v: f32, lo: f32, hi: f32) -> f32 {
    if v.is_nan() || v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// One transport verb, so a callback can name an operation without holding a
/// `&mut MediaPlayerManager` (see `CallbackChange::MediaTransport`).
///
/// Data-carrying variants are TUPLE variants on purpose — the codegen the
/// repo shares refuses struct variants with anything but exactly one field,
/// and a habit that only holds in the FFI enums is a habit that breaks the
/// first time one of these is exposed.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum MediaTransportOp {
    /// Start playback (`play`).
    Play,
    /// Stop playback where it stands (`pause`).
    Pause,
    /// Play if paused, pause if playing (`toggle`).
    Toggle,
    /// Move the position, in seconds (`seek`).
    Seek(f32),
    /// Set the output gain, `0.0..=1.0` (`set_volume`).
    SetVolume(f32),
    /// Mute or unmute (`set_muted`).
    SetMuted(bool),
    /// Record the media's length in seconds (`set_duration`).
    SetDuration(f32),
    /// Advance the media clock by this many seconds (`advance`).
    Advance(f32),
    /// The pipeline failed (`report_error`).
    ReportError,
}

impl MediaPlayerManager {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Apply one transport verb. The single seam the `CallbackChange` path
    /// uses, so a new verb cannot be added to the enum without being routed.
    pub fn apply(&mut self, node: DomNodeId, op: MediaTransportOp) {
        match op {
            MediaTransportOp::Play => self.play(node),
            MediaTransportOp::Pause => self.pause(node),
            MediaTransportOp::Toggle => self.toggle(node),
            MediaTransportOp::Seek(p) => self.seek(node, p),
            MediaTransportOp::SetVolume(v) => self.set_volume(node, v),
            MediaTransportOp::SetMuted(m) => self.set_muted(node, m),
            MediaTransportOp::SetDuration(d) => self.set_duration(node, d),
            MediaTransportOp::Advance(dt) => self.advance(node, dt),
            MediaTransportOp::ReportError => self.report_error(node),
        }
    }

    /// The current state of `node`, or `None` if no transport call has ever
    /// named it — "this is not a media node" is a real answer, distinct from
    /// a default state, and the app can act on the difference.
    #[must_use]
    pub fn state(&self, node: DomNodeId) -> Option<PlaybackState> {
        self.players.get(&node).map(|p| p.state)
    }

    /// The player for `node`, creating the default entry if this is its first
    /// transport call.
    fn entry(&mut self, node: DomNodeId) -> &mut Player {
        self.players.entry(node).or_default()
    }

    /// Start playback. Emits `Play` only on the transition — calling `play`
    /// on an already-playing node is a no-op, not a second event.
    pub fn play(&mut self, node: DomNodeId) {
        let p = self.entry(node);
        if p.state.playing {
            return;
        }
        // Playing from the end rewinds. Otherwise the very next `advance`
        // would re-reach the end and `Ended` again, and the transport would
        // be stuck at the tail forever.
        if p.state.duration_s > 0.0 && p.state.position_s >= p.state.duration_s {
            p.state.position_s = 0.0;
            p.last_time_update_s = 0.0;
        }
        p.state.playing = true;
        self.pending.push((node, EventType::Play));
    }

    /// Stop playback where it stands. Emits `Pause` only on the transition.
    pub fn pause(&mut self, node: DomNodeId) {
        let p = self.entry(node);
        if !p.state.playing {
            return;
        }
        p.state.playing = false;
        self.pending.push((node, EventType::Pause));
    }

    /// Play if paused, pause if playing. An unknown node starts playing (its
    /// default state is paused).
    pub fn toggle(&mut self, node: DomNodeId) {
        if self.players.get(&node).is_some_and(|p| p.state.playing) {
            self.pause(node);
        } else {
            self.play(node);
        }
    }

    /// Move the position. Clamped into `0..=duration` when a duration is
    /// known, `>= 0` otherwise.
    ///
    /// A seek is a discontinuity in the media clock, so it reports
    /// `TimeUpdate` immediately and resets the throttle rather than waiting
    /// for the next 250 ms boundary. There is no `seeking`/`seeked`
    /// `EventType` in azul, so `TimeUpdate` is the whole story. A seek that
    /// does not move the position emits nothing.
    pub fn seek(&mut self, node: DomNodeId, position_s: f32) {
        let p = self.entry(node);
        let hi = if p.state.duration_s > 0.0 {
            p.state.duration_s
        } else {
            f32::MAX
        };
        let target = sanitize(position_s, 0.0, hi);
        if p.state.position_s == target {
            return;
        }
        p.state.position_s = target;
        p.last_time_update_s = target;
        self.pending.push((node, EventType::TimeUpdate));
    }

    /// Set the output gain, clamped to `0.0..=1.0`. Emits `VolumeChange`
    /// only when the value actually moves.
    pub fn set_volume(&mut self, node: DomNodeId, volume: f32) {
        let p = self.entry(node);
        let target = sanitize(volume, 0.0, 1.0);
        if p.state.volume == target {
            return;
        }
        p.state.volume = target;
        self.pending.push((node, EventType::VolumeChange));
    }

    /// Mute or unmute. Reported as `VolumeChange` — the web spells a mute
    /// change the same way, and azul has no separate event for it.
    pub fn set_muted(&mut self, node: DomNodeId, muted: bool) {
        let p = self.entry(node);
        if p.state.muted == muted {
            return;
        }
        p.state.muted = muted;
        self.pending.push((node, EventType::VolumeChange));
    }

    /// Record the media's length once the app knows it (metadata, or a
    /// decoder's header parse). Negative and NaN lengths read as "unknown".
    ///
    /// Emits nothing: there is no `durationchange` `EventType`, and learning
    /// how long a file is is not playback reaching its end — so a position
    /// past the new duration is clamped WITHOUT raising `Ended`.
    pub fn set_duration(&mut self, node: DomNodeId, duration_s: f32) {
        let p = self.entry(node);
        p.state.duration_s = sanitize(duration_s, 0.0, f32::MAX);
        if p.state.duration_s > 0.0 && p.state.position_s > p.state.duration_s {
            p.state.position_s = p.state.duration_s;
        }
    }

    /// Advance the media clock by `dt_s` seconds of wall time.
    ///
    /// Does nothing while paused, on an unknown node, or for a non-positive
    /// or NaN `dt_s`. Emits at most one `TimeUpdate` (throttled to
    /// [`TIME_UPDATE_INTERVAL_S`]) and, on reaching a known duration, exactly
    /// one `Ended`: reaching the end also clears `playing`, so a second
    /// `advance` returns immediately and cannot emit `Ended` twice.
    // `!(dt_s > 0.0)` below is the NaN filter; `dt_s <= 0.0` would let NaN through.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    pub fn advance(&mut self, node: DomNodeId, dt_s: f32) {
        let Some(p) = self.players.get_mut(&node) else {
            return;
        };
        // `!(dt > 0.0)` rather than `dt <= 0.0` so NaN is rejected too.
        if !p.state.playing || !(dt_s > 0.0) {
            return;
        }
        p.state.position_s += dt_s;
        let ended = if p.state.duration_s > 0.0 && p.state.position_s >= p.state.duration_s {
            p.state.position_s = p.state.duration_s;
            p.state.playing = false;
            true
        } else {
            false
        };
        let due = (p.state.position_s - p.last_time_update_s).abs() >= TIME_UPDATE_INTERVAL_S;
        if due {
            p.last_time_update_s = p.state.position_s;
        }
        if due {
            self.pending.push((node, EventType::TimeUpdate));
        }
        if ended {
            self.pending.push((node, EventType::Ended));
        }
    }

    /// The app's decoder/sink failed. Stops the transport and emits
    /// `MediaError` — every call emits, because two failures are two errors
    /// (there is no state here to transition).
    pub fn report_error(&mut self, node: DomNodeId) {
        let p = self.entry(node);
        p.state.playing = false;
        self.pending.push((node, EventType::MediaError));
    }

    /// Forget everything about `node` (the app tore its player down).
    pub fn remove(&mut self, node: DomNodeId) {
        self.players.remove(&node);
        self.pending.retain(|(n, _)| *n != node);
    }

    /// Drop the collected transitions. Called by the event pass right after
    /// determination, exactly like `SensorManager::clear_pending_event`.
    pub fn clear_pending_event(&mut self) {
        self.pending.clear();
    }

    /// Whether anything is queued (used by tests and by the e2e fingerprint).
    #[must_use]
    pub const fn has_pending(&self) -> bool {
        !self.pending.is_empty()
    }
}

impl EventProvider for MediaPlayerManager {
    /// One event per queued transition, targeted at the media node so the
    /// callback can ask which player moved.
    ///
    /// `EventSource::User` like every other capability manager — nothing in
    /// dispatch reads the source, and a transport call is the user pressing
    /// play as far as the app is concerned.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        self.pending
            .iter()
            .map(|(node, ty)| {
                SyntheticEvent::new(
                    *ty,
                    CoreEventSource::User,
                    *node,
                    timestamp.clone(),
                    EventData::None,
                )
            })
            .collect()
    }
}

impl NodeIdRemap for MediaPlayerManager {
    fn remap_node_ids(&mut self, dom: DomId, map: &NodeIdMap) {
        let old = core::mem::take(&mut self.players);
        for (k, v) in old {
            if let Some(nk) = map.resolve_dom_node_id(dom, k) {
                self.players.insert(nk, v);
            }
        }
        let queued = core::mem::take(&mut self.pending);
        self.pending = queued
            .into_iter()
            .filter_map(|(n, ty)| map.resolve_dom_node_id(dom, n).map(|nn| (nn, ty)))
            .collect();
    }
}

#[cfg(test)]
mod tests {
    use azul_core::{
        id::NodeId,
        styled_dom::NodeHierarchyItemId,
        task::{Instant, SystemTick},
    };

    use super::*;

    fn node(idx: usize) -> DomNodeId {
        DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(Some(NodeId::new(idx))),
        }
    }

    fn kinds(mgr: &MediaPlayerManager) -> Vec<EventType> {
        mgr.pending.iter().map(|(_, ty)| *ty).collect()
    }

    #[test]
    fn play_then_pause_emits_play_then_pause() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(3);
        mgr.play(n);
        mgr.pause(n);
        assert_eq!(kinds(&mgr), vec![EventType::Play, EventType::Pause]);
        assert!(!mgr.state(n).unwrap().playing);
    }

    #[test]
    fn repeated_transport_calls_emit_only_on_the_transition() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(1);
        mgr.play(n);
        mgr.play(n); // already playing — not a second Play
        mgr.pause(n);
        mgr.pause(n); // already paused — not a second Pause
        assert_eq!(kinds(&mgr), vec![EventType::Play, EventType::Pause]);
    }

    #[test]
    fn toggle_alternates_play_and_pause() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(2);
        mgr.toggle(n);
        mgr.toggle(n);
        mgr.toggle(n);
        assert_eq!(kinds(&mgr), vec![
            EventType::Play,
            EventType::Pause,
            EventType::Play
        ]);
    }

    #[test]
    fn advance_past_duration_ends_exactly_once() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(4);
        mgr.set_duration(n, 1.0);
        mgr.play(n);
        mgr.clear_pending_event();

        mgr.advance(n, 5.0);
        assert!(
            kinds(&mgr).contains(&EventType::Ended),
            "reaching the end must raise Ended: {:?}",
            kinds(&mgr)
        );
        assert_eq!(
            kinds(&mgr).iter().filter(|t| **t == EventType::Ended).count(),
            1
        );
        let st = mgr.state(n).unwrap();
        assert!(!st.playing, "the end stops the transport");
        assert_eq!(st.position_s, 1.0, "the position clamps at the duration");

        // A second advance on the ended player must not raise Ended again —
        // this is the "exactly once" half of the rule.
        mgr.clear_pending_event();
        mgr.advance(n, 5.0);
        assert!(kinds(&mgr).is_empty(), "{:?}", kinds(&mgr));
    }

    #[test]
    fn playing_again_after_the_end_rewinds_and_can_end_again() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(5);
        mgr.set_duration(n, 2.0);
        mgr.play(n);
        mgr.advance(n, 10.0);
        mgr.clear_pending_event();

        mgr.play(n);
        assert_eq!(mgr.state(n).unwrap().position_s, 0.0, "play rewinds");
        mgr.advance(n, 10.0);
        assert_eq!(
            kinds(&mgr).iter().filter(|t| **t == EventType::Ended).count(),
            1
        );
    }

    #[test]
    fn an_unknown_duration_never_ends() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(6);
        mgr.play(n); // duration stays 0.0 = unknown (a live stream)
        mgr.clear_pending_event();
        mgr.advance(n, 3600.0);
        assert!(!kinds(&mgr).contains(&EventType::Ended));
        assert!(mgr.state(n).unwrap().playing);
    }

    #[test]
    fn time_update_is_throttled_to_the_250ms_budget() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(7);
        mgr.play(n);
        mgr.clear_pending_event();

        // A 64 Hz tick, not 60: 1/64 is exact in binary, and with 1/60 the
        // accumulated f32 error puts the last boundary one ULP under the
        // threshold, which would make the count below flaky rather than
        // wrong. The throttle rule is what is under test, not float error.
        let frame = 1.0 / 64.0;

        // Sixteen frames = exactly 0.25 s of media time. Unthrottled that
        // would be 16 events; the budget allows one.
        for _ in 0..16 {
            mgr.advance(n, frame);
        }
        assert_eq!(
            kinds(&mgr),
            vec![EventType::TimeUpdate],
            "one TimeUpdate per {TIME_UPDATE_INTERVAL_S}s of media time, not one per frame"
        );

        // A full second more is exactly four more boundaries — the web's
        // ~4/s budget — out of 64 advance calls.
        mgr.clear_pending_event();
        for _ in 0..64 {
            mgr.advance(n, frame);
        }
        assert_eq!(kinds(&mgr).len(), 4, "{:?}", kinds(&mgr));
    }

    #[test]
    fn seek_reports_a_time_update_and_resets_the_throttle() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(8);
        mgr.set_duration(n, 10.0);
        mgr.clear_pending_event();

        mgr.seek(n, 4.0);
        assert_eq!(kinds(&mgr), vec![EventType::TimeUpdate]);
        assert_eq!(mgr.state(n).unwrap().position_s, 4.0);

        // A seek to where we already are is not a change.
        mgr.clear_pending_event();
        mgr.seek(n, 4.0);
        assert!(kinds(&mgr).is_empty());

        // Out of range clamps rather than corrupting the state.
        mgr.seek(n, 99.0);
        assert_eq!(mgr.state(n).unwrap().position_s, 10.0);
        mgr.seek(n, -5.0);
        assert_eq!(mgr.state(n).unwrap().position_s, 0.0);
        mgr.seek(n, f32::NAN);
        assert_eq!(mgr.state(n).unwrap().position_s, 0.0, "NaN reads as 0");
    }

    #[test]
    fn set_volume_emits_only_on_an_actual_change() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(9);
        mgr.set_volume(n, 1.0); // the default — not a change
        assert!(kinds(&mgr).is_empty());

        mgr.set_volume(n, 0.4);
        assert_eq!(kinds(&mgr), vec![EventType::VolumeChange]);

        mgr.clear_pending_event();
        mgr.set_volume(n, 0.4); // same value again
        assert!(kinds(&mgr).is_empty());

        // Clamping: 2.0 and 1.0 are the same volume, so only the first moves.
        mgr.set_volume(n, 2.0);
        assert_eq!(mgr.state(n).unwrap().volume, 1.0);
        mgr.clear_pending_event();
        mgr.set_volume(n, 5.0);
        assert!(kinds(&mgr).is_empty(), "already clamped to 1.0");
    }

    #[test]
    fn muting_is_reported_as_a_volume_change_and_spares_the_level() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(10);
        mgr.set_volume(n, 0.7);
        mgr.clear_pending_event();

        mgr.set_muted(n, true);
        assert_eq!(kinds(&mgr), vec![EventType::VolumeChange]);
        assert_eq!(mgr.state(n).unwrap().volume, 0.7, "unmuting restores it");

        mgr.clear_pending_event();
        mgr.set_muted(n, true); // no change
        assert!(kinds(&mgr).is_empty());
    }

    #[test]
    fn report_error_stops_the_transport_and_emits() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(11);
        mgr.play(n);
        mgr.clear_pending_event();

        mgr.report_error(n);
        assert_eq!(kinds(&mgr), vec![EventType::MediaError]);
        assert!(!mgr.state(n).unwrap().playing);
    }

    #[test]
    fn set_duration_clamps_without_ending() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(12);
        mgr.play(n);
        mgr.advance(n, 30.0);
        mgr.clear_pending_event();

        // Metadata arrives late and says the file is shorter than where we are.
        mgr.set_duration(n, 5.0);
        assert_eq!(mgr.state(n).unwrap().position_s, 5.0);
        assert!(
            kinds(&mgr).is_empty(),
            "learning the length is not reaching the end: {:?}",
            kinds(&mgr)
        );
    }

    #[test]
    fn the_provider_yields_one_event_per_queued_transition() {
        let mut mgr = MediaPlayerManager::new();
        let n = node(13);
        mgr.play(n);
        mgr.set_volume(n, 0.5);
        let ts = Instant::Tick(SystemTick::new(0));

        let evs = mgr.get_pending_events(ts.clone());
        assert_eq!(evs.len(), 2);
        assert_eq!(evs[0].event_type, EventType::Play);
        assert_eq!(evs[1].event_type, EventType::VolumeChange);
        assert_eq!(evs[0].target, n, "the event names the media node");

        // Reading is NOT draining — only clear_pending_event retires them,
        // which is the contract every EventProvider in the tree shares.
        assert_eq!(mgr.get_pending_events(ts.clone()).len(), 2);
        mgr.clear_pending_event();
        assert!(mgr.get_pending_events(ts).is_empty());
    }

    #[test]
    fn every_transport_op_reaches_its_verb() {
        // The `CallbackChange` path goes through `apply`, so a verb that is
        // not routed here is a `CallbackInfo` method that silently does
        // nothing — the exact dead-wiring shape this item exists to close.
        let mut mgr = MediaPlayerManager::new();
        let n = node(20);
        mgr.apply(n, MediaTransportOp::SetDuration(10.0));
        mgr.apply(n, MediaTransportOp::Play);
        mgr.apply(n, MediaTransportOp::Advance(1.0));
        mgr.apply(n, MediaTransportOp::Seek(2.0));
        mgr.apply(n, MediaTransportOp::SetVolume(0.25));
        mgr.apply(n, MediaTransportOp::SetMuted(true));
        mgr.apply(n, MediaTransportOp::Toggle);
        mgr.apply(n, MediaTransportOp::ReportError);
        let st = mgr.state(n).unwrap();
        assert_eq!(st.duration_s, 10.0);
        assert_eq!(st.position_s, 2.0);
        assert_eq!(st.volume, 0.25);
        assert!(st.muted);
        assert!(!st.playing);
        assert_eq!(kinds(&mgr), vec![
            EventType::Play,
            EventType::TimeUpdate, // the 1 s advance
            EventType::TimeUpdate, // the seek
            EventType::VolumeChange,
            EventType::VolumeChange, // the mute
            EventType::Pause,        // toggle, from playing
            EventType::MediaError,
        ]);
    }

    #[test]
    fn remap_follows_a_moved_node_and_drops_an_unmounted_one() {
        let mut mgr = MediaPlayerManager::new();
        let moved = node(4);
        let gone = node(9);
        mgr.play(moved);
        mgr.play(gone);

        // Node 4 became node 2; node 9 was unmounted (absent from the map).
        let map = NodeIdMap::from_pairs([(NodeId::new(4), NodeId::new(2))]);
        mgr.remap_node_ids(DomId::ROOT_ID, &map);

        assert!(mgr.state(node(2)).is_some_and(|s| s.playing));
        assert!(mgr.state(moved).is_none(), "the old index must not survive");
        assert!(mgr.state(gone).is_none(), "unmounted state is dropped");
        assert_eq!(mgr.pending.len(), 1, "its queued Play went with it");
        assert_eq!(mgr.pending[0].0, node(2));
    }
}

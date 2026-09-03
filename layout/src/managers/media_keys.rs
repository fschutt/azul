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

use azul_core::window::VirtualKeyCode;

static PENDING: std::sync::Mutex<Vec<VirtualKeyCode>> = std::sync::Mutex::new(Vec::new());

/// A person cannot press play more often than this between frames; anything
/// beyond it is a stuck sender, and an unbounded queue would grow for the life
/// of the process.
const MAX_PENDING: usize = 64;

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
pub fn push_media_key(key: VirtualKeyCode) {
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if q.len() >= MAX_PENDING {
        return;
    }
    if q.contains(&key) {
        return;
    }
    q.push(key);
}

/// Drain the parked media keys, in arrival order.
pub fn drain_media_keys() -> Vec<VirtualKeyCode> {
    let mut q = PENDING
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    core::mem::take(&mut *q)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `PENDING` is a process-global, and the test harness runs these in
    /// parallel - so without this one test drains another's keys and the
    /// failure looks like a bug in the queue. Serialising is the standard fix
    /// and is cheaper than making the queue per-test.
    static SERIAL: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn exclusive() -> std::sync::MutexGuard<'static, ()> {
        let g = SERIAL
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let _ = drain_media_keys();
        g
    }

    #[test]
    fn keys_drain_in_arrival_order_and_empty_the_queue() {
        let _guard = exclusive();
        push_media_key(VirtualKeyCode::PlayPause);
        push_media_key(VirtualKeyCode::NextTrack);
        let got = drain_media_keys();
        assert_eq!(got, vec![VirtualKeyCode::PlayPause, VirtualKeyCode::NextTrack]);
        assert!(drain_media_keys().is_empty());
    }

    /// A stuck sender must not grow the queue without bound.
    /// TWO TRANSPORTS, ONE PRESS. Windows can report the same media key
    /// through `WM_APPCOMMAND` and through SMTC's `ButtonPressed`; without
    /// this, a single play press would toggle playback twice and land back
    /// where it started.
    #[test]
    fn the_same_key_reported_twice_in_one_batch_collapses() {
        let _guard = exclusive();
        push_media_key(VirtualKeyCode::PlayPause);
        push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(drain_media_keys(), vec![VirtualKeyCode::PlayPause]);

        // DIFFERENT keys in one batch are two real presses and both survive.
        push_media_key(VirtualKeyCode::NextTrack);
        push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(
            drain_media_keys(),
            vec![VirtualKeyCode::NextTrack, VirtualKeyCode::PlayPause]
        );

        // And a genuine double press in SEPARATE batches is not collapsed -
        // the whole point of keying on "still pending" rather than on a timer.
        push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(drain_media_keys(), vec![VirtualKeyCode::PlayPause]);
        push_media_key(VirtualKeyCode::PlayPause);
        assert_eq!(drain_media_keys(), vec![VirtualKeyCode::PlayPause]);
    }

    #[test]
    fn a_stuck_sender_cannot_grow_the_queue() {
        let _guard = exclusive();
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
            push_media_key(keys[i % keys.len()]);
        }
        let drained = drain_media_keys();
        assert_eq!(
            drained.len(),
            keys.len(),
            "a sender repeating four keys forever must leave exactly four, got {drained:?}"
        );
        assert!(drained.len() <= MAX_PENDING);
    }
}

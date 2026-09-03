//! The system MEDIA SESSION - what the desktop's media widget shows.
//!
//! This is the second output in the input subsystem, next to [`crate::haptics`]:
//! nothing is being reported, the app is telling the platform something. It
//! lives here because the transport it feeds is the same one the media KEYS
//! arrive on, and because on every platform the two are one object - an app
//! becomes eligible to receive `Play`/`Next` precisely by declaring what it is
//! playing.
//!
//! # Why the app pushes this instead of the engine deriving it
//!
//! Azul has no playback state machine (11c is blocked on exactly that), and it
//! never will have one that covers the interesting cases: an app playing audio
//! through `rodio`, through a system framework, or over the network knows what
//! it is playing and the toolkit cannot see it. So the app pushes, and the
//! engine's only job is to fan that out to the platform session APIs.
//!
//! # Where it goes
//!
//! | platform | sink | notes |
//! |---|---|---|
//! | Linux | MPRIS `org.mpris.MediaPlayer2.Player` | `Metadata`, `PlaybackStatus`, `Position` |
//! | macOS | `MPNowPlayingInfoCenter` | artwork needs image bytes, not a URL - dropped |
//! | Windows | - | needs an SMTC backend that does not exist yet |
//! | iOS / Android | - | both have an equivalent; no backend yet |
//!
//! Publishing on a platform with no sink is a no-op, not an error: the same
//! call site has to be correct everywhere.

use azul_css::AzString;

/// What the player is doing right now.
///
/// Deliberately three states and not four: MPRIS has exactly these, and
/// macOS's `MPNowPlayingPlaybackState` adds `unknown` and `interrupted` which
/// no app can meaningfully assert about itself - `interrupted` is something the
/// SYSTEM does to you (a phone call), not something you declare.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum MediaPlaybackState {
    /// Nothing is loaded, or playback finished. The desktop widget shows no
    /// track.
    Stopped = 0,
    /// Advancing. A desktop extrapolates the position from here, which is why
    /// this must be honest even when the position is not being republished.
    Playing = 1,
    /// Loaded and holding position.
    Paused = 2,
}

impl Default for MediaPlaybackState {
    fn default() -> Self {
        MediaPlaybackState::Stopped
    }
}

/// What the app is playing, as the system media widget should show it.
///
/// Every field is optional in the sense that an empty string or a zero is a
/// valid "I do not know" - there is no `Option` here because a media widget
/// treats an absent title and an empty title identically, and the FFI cost of
/// six `Option<AzString>`s would buy nothing.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
#[repr(C)]
pub struct NowPlayingInfo {
    /// What the player is doing. This is the field that decides whether the
    /// desktop shows a play or a pause button.
    pub state: MediaPlaybackState,
    /// Track title. Empty means unknown.
    pub title: AzString,
    /// Performer. Empty means unknown.
    ///
    /// ONE artist, not a list, even though MPRIS's `xesam:artist` is an array
    /// of strings: every widget that displays it joins the array back into one
    /// line, and an app with several artists can join them itself with the
    /// separator its locale wants. The MPRIS backend wraps this in a
    /// single-element array because the spec's type demands it.
    pub artist: AzString,
    /// Album title. Empty means unknown.
    pub album: AzString,
    /// Cover art, as a URI - `file://` or `http(s)://`.
    ///
    /// A URI and not bytes because that is what MPRIS wants and because the
    /// alternative makes every publish copy an image. macOS is the platform
    /// that pays for this: `MPMediaItemArtwork` needs a decoded image, so the
    /// macOS backend drops this field rather than fetching a URL from inside a
    /// UI toolkit.
    pub artwork_url: AzString,
    /// Track length in MILLISECONDS. `0` means unknown, which is correct for a
    /// live stream.
    ///
    /// 64-bit because 32-bit milliseconds overflow at 49.7 days but, more to
    /// the point, because a `u32` of *microseconds* - the unit MPRIS actually
    /// wants - overflows at 71 minutes, which is an ordinary audiobook chapter.
    /// The conversion to microseconds happens in the backend.
    pub duration_ms: u64,
    /// How far in, in MILLISECONDS.
    ///
    /// SEE [`MediaSessionManager::set`]: this field deliberately does NOT
    /// trigger a change announcement, because MPRIS forbids announcing it.
    pub position_ms: u64,
}

impl NowPlayingInfo {
    /// A stopped player with no track - what an app that has published nothing
    /// looks like.
    pub fn empty() -> Self {
        Self::default()
    }

    /// True when two publishes differ in a way the platform must be TOLD about,
    /// as opposed to one it will read for itself.
    ///
    /// Everything except the position. See [`MediaSessionManager::set`].
    pub fn differs_in_announced_fields(&self, other: &Self) -> bool {
        self.state != other.state
            || self.title != other.title
            || self.artist != other.artist
            || self.album != other.album
            || self.artwork_url != other.artwork_url
            || self.duration_ms != other.duration_ms
    }

    /// True when this is a DIFFERENT TRACK, not merely a different state of
    /// the same one.
    ///
    /// MPRIS identifies a track by `mpris:trackid`, and a desktop keys its
    /// progress bar and its "song changed" notification on that id. Minting a
    /// new one when the user merely hit pause would reset the progress bar and
    /// pop a notification for a track that never changed - so pausing, seeking
    /// and cover-art changes are all the SAME track.
    ///
    /// The duration counts as identity because two tracks with the same title
    /// and artist but different lengths are a live version and a studio one.
    pub fn is_different_track(&self, other: &Self) -> bool {
        self.title != other.title
            || self.artist != other.artist
            || self.album != other.album
            || self.duration_ms != other.duration_ms
    }

    /// The track length in MICROSECONDS, which is the unit MPRIS's
    /// `mpris:length` and `Position` both use.
    ///
    /// Saturating and `i64`, because D-Bus types this signed: a nonsense
    /// duration from an app must clamp rather than wrap into a negative
    /// length, which some clients render as a progress bar running backwards.
    pub fn duration_us(&self) -> i64 {
        Self::ms_to_us(self.duration_ms)
    }

    /// The playback position in MICROSECONDS. See [`Self::duration_us`].
    pub fn position_us(&self) -> i64 {
        Self::ms_to_us(self.position_ms)
    }

    fn ms_to_us(ms: u64) -> i64 {
        i64::try_from(ms.saturating_mul(1000)).unwrap_or(i64::MAX)
    }
}

/// Holds the current session state and remembers whether the platform has been
/// told about it.
///
/// Mirrors [`crate::haptics::HapticManager`]: the callback thread writes, the
/// shell drains once per pass. The difference is that this is a LATEST-WINS
/// value rather than a queue - a media widget wants the current track, not
/// every track the app has ever played.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct MediaSessionManager {
    info: NowPlayingInfo,
    /// Set when [`Self::set`] changes a field the platform must be told about.
    needs_publish: bool,
}

impl MediaSessionManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record what the app is playing.
    ///
    /// # Why a position-only change does not mark this dirty
    ///
    /// A player calls this once per frame with an advancing position. If that
    /// marked the session dirty, every frame would put a `PropertiesChanged`
    /// signal on the session bus - 60 D-Bus broadcasts a second, woken up in
    /// every listening process on the desktop.
    ///
    /// It would also be WRONG rather than merely wasteful: the MPRIS spec says
    /// `Position` must not appear in `PropertiesChanged` at all, precisely
    /// because it changes continuously. Clients extrapolate it from
    /// `PlaybackStatus` and `Rate` and read the property when they need a
    /// precise value, so the stored position is served on demand instead.
    pub fn set(&mut self, info: NowPlayingInfo) {
        if self.info.differs_in_announced_fields(&info) {
            self.needs_publish = true;
        }
        self.info = info;
    }

    /// What the app last published, whether or not the platform has been told.
    ///
    /// This is what a property GETTER answers with, which is why it is
    /// unconditional: a desktop reading `Position` must get the current value
    /// even though no announcement was made for it.
    pub fn current(&self) -> &NowPlayingInfo {
        &self.info
    }

    /// The session to announce, or `None` when nothing announceable changed.
    pub fn take_if_dirty(&mut self) -> Option<NowPlayingInfo> {
        if core::mem::take(&mut self.needs_publish) {
            Some(self.info.clone())
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(title: &str) -> NowPlayingInfo {
        NowPlayingInfo {
            state: MediaPlaybackState::Playing,
            title: title.into(),
            ..NowPlayingInfo::empty()
        }
    }

    #[test]
    fn a_fresh_manager_has_nothing_to_announce() {
        let mut m = MediaSessionManager::new();
        assert!(m.take_if_dirty().is_none());
        assert_eq!(*m.current(), NowPlayingInfo::empty());
    }

    #[test]
    fn a_new_track_is_announced_once() {
        let mut m = MediaSessionManager::new();
        m.set(track("Aja"));
        assert_eq!(m.take_if_dirty().map(|i| i.title), Some("Aja".into()));
        // Draining twice must not announce the same track again: a desktop
        // that redraws its widget on every signal would flicker.
        assert!(m.take_if_dirty().is_none());
    }

    /// THE POINT OF THE WHOLE DIRTY SPLIT. A player pushing an advancing
    /// position must not put a signal on the bus per frame.
    #[test]
    fn a_position_only_change_is_not_announced() {
        let mut m = MediaSessionManager::new();
        m.set(track("Aja"));
        let _ = m.take_if_dirty();

        for pos in 1..=120u64 {
            let mut t = track("Aja");
            t.position_ms = pos * 16;
            m.set(t);
            assert!(
                m.take_if_dirty().is_none(),
                "a position-only change announced itself at frame {pos}"
            );
        }

        // ...but it IS stored, because the property getter answers from here.
        assert_eq!(m.current().position_ms, 120 * 16);
    }

    /// The other half: a real change while the position is also moving still
    /// gets announced. Comparing the whole struct would have made this pass
    /// for the wrong reason, so it changes the state AND the position.
    #[test]
    fn a_real_change_is_announced_even_while_the_position_moves() {
        let mut m = MediaSessionManager::new();
        m.set(track("Aja"));
        let _ = m.take_if_dirty();

        let mut paused = track("Aja");
        paused.state = MediaPlaybackState::Paused;
        paused.position_ms = 4_000;
        m.set(paused);
        assert_eq!(
            m.take_if_dirty().map(|i| i.state),
            Some(MediaPlaybackState::Paused)
        );
    }

    #[test]
    fn every_announced_field_actually_announces() {
        let base = track("Aja");
        let mut cases: alloc::vec::Vec<(&str, NowPlayingInfo)> = alloc::vec::Vec::new();

        let mut c = base.clone();
        c.state = MediaPlaybackState::Stopped;
        cases.push(("state", c));
        let mut c = base.clone();
        c.title = "Peg".into();
        cases.push(("title", c));
        let mut c = base.clone();
        c.artist = "Steely Dan".into();
        cases.push(("artist", c));
        let mut c = base.clone();
        c.album = "Aja".into();
        cases.push(("album", c));
        let mut c = base.clone();
        c.artwork_url = "file:///cover.png".into();
        cases.push(("artwork_url", c));
        let mut c = base.clone();
        c.duration_ms = 480_000;
        cases.push(("duration_ms", c));

        for (field, changed) in cases {
            let mut m = MediaSessionManager::new();
            m.set(base.clone());
            let _ = m.take_if_dirty();
            m.set(changed);
            assert!(
                m.take_if_dirty().is_some(),
                "changing `{field}` did not announce itself"
            );
        }

        // And the one field that must NOT, stated as a case in the same list
        // so that adding a field to the struct forces a decision about it.
        let mut m = MediaSessionManager::new();
        m.set(base.clone());
        let _ = m.take_if_dirty();
        let mut moved = base.clone();
        moved.position_ms = 9_999;
        m.set(moved);
        assert!(m.take_if_dirty().is_none(), "position_ms announced itself");
    }

    /// A podcast is longer than a `u32` of microseconds can hold. The struct
    /// stores milliseconds and the backend converts, so this pins that the
    /// conversion has room - and that a nonsense value clamps instead of
    /// wrapping negative, which a client would render as a backwards bar.
    #[test]
    fn the_microsecond_conversion_has_room_and_clamps() {
        let mut i = NowPlayingInfo::empty();

        i.duration_ms = 3 * 60 * 60 * 1000;
        assert_eq!(i.duration_us(), 10_800_000_000);
        assert!(i.duration_us() > i64::from(u32::MAX));

        i.position_ms = 1_500;
        assert_eq!(i.position_us(), 1_500_000);

        i.duration_ms = u64::MAX;
        assert_eq!(i.duration_us(), i64::MAX, "an absurd duration must clamp");
        assert!(i.duration_us() > 0, "and must never come out negative");
    }

    /// `mpris:trackid` is what a desktop keys its progress bar on, so a pause
    /// must not look like a new track.
    #[test]
    fn pausing_is_the_same_track_but_a_new_title_is_not() {
        let playing = track("Aja");
        let mut paused = playing.clone();
        paused.state = MediaPlaybackState::Paused;
        paused.position_ms = 30_000;
        assert!(!playing.is_different_track(&paused));

        // Late-arriving cover art is also still the same track.
        let mut with_art = playing.clone();
        with_art.artwork_url = "file:///cover.png".into();
        assert!(!playing.is_different_track(&with_art));

        let next = track("Peg");
        assert!(playing.is_different_track(&next));

        // A live version shares a title and artist but not a length.
        let mut live = playing.clone();
        live.duration_ms = 480_000;
        assert!(playing.is_different_track(&live));
    }
}

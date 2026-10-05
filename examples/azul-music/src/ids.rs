//! AzMusic's DOM ids, classes and markers, each name defined ONCE, every one carrying the app's
//! `__azmusic_` prefix (user ruling 2026-10-02, like the widgets' `__azul_`).

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azmusic_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azmusic_", $value));)*
    };
}

names! {
    /// The sidebar's library entries.
    NAV_SONGS = "nav-songs";
    NAV_ALBUMS = "nav-albums";
    NAV_ARTISTS = "nav-artists";
    /// "New playlist" in the sidebar.
    NAV_NEW_PLAYLIST = "nav-new-playlist";
    /// The songs table (the DataTable's id).
    SONGS = "songs";
    /// The album and artist lists.
    ALBUMS = "albums";
    ARTISTS = "artists";
    /// A playlist's list.
    PLAYLIST = "playlist";
    /// The empty state (no library yet).
    EMPTY = "empty";
    /// The now-playing bar and its parts (markers: moved in place by the playback timer).
    NOW_PLAYING = "now-playing";
    NOW_TITLE = "now-title";
    NOW_ARTIST = "now-artist";
    CONTROLS = "controls";
    SEEK = "seek";
    LEVEL = "level";
    /// The settings section's "Scan the music folder" button and the folder field.
    SCAN = "scan";
    FOLDER = "folder";
    /// The status line under the content.
    STATUS = "status";
}

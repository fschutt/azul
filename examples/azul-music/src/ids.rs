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
    /// The tool bar - the window's title bar, its drag region: back, forward, the search field,
    /// the status.
    TOOLBAR = "toolbar";
    BACK = "back";
    FORWARD = "forward";
    /// The search field.
    SEARCH = "search";
    /// The status line (in the tool bar).
    STATUS = "status";
    /// The sidebar's list and its entries.
    SIDEBAR = "sidebar";
    NAV_QUEUE = "nav-queue";
    NAV_RECENT = "nav-recent";
    NAV_ARTISTS = "nav-artists";
    NAV_ALBUMS = "nav-albums";
    NAV_SONGS = "nav-songs";
    NAV_GENRES = "nav-genres";
    /// The class of a playlist's entry in the sidebar.
    NAV_PLAYLIST = "nav-playlist";
    /// "New Playlist" under the sidebar's list.
    NAV_NEW_PLAYLIST = "nav-new-playlist";
    /// The cover of the song that plays, at the foot of the sidebar.
    SIDEBAR_COVER = "sidebar-cover";
    /// The page's pane.
    MAIN = "main";
    /// The page (a VirtualView): its class, its marker (re-rendered in place when the pointer
    /// moves to another song or card), and its id per page.
    PAGE = "page";
    CONTENT = "content";
    RECENT = "recent";
    ARTISTS = "artists";
    ALBUMS = "albums";
    SONGS = "songs";
    GENRES = "genres";
    SEARCH_RESULTS = "search-results";
    QUEUE = "queue";
    /// The class of a page of one album, artist, genre or playlist (their ids carry an index:
    /// `__azmusic_album-3`, `__azmusic_playlist-<id>`).
    PAGE_ALBUM = "page-album";
    PAGE_ARTIST = "page-artist";
    PAGE_GENRE = "page-genre";
    PAGE_PLAYLIST = "page-playlist";
    /// The page header's Play and Shuffle buttons.
    PAGE_PLAY = "page-play";
    PAGE_SHUFFLE = "page-shuffle";
    /// The class of a song's row, of a card, of the Play button on a card under the pointer and
    /// of the play icon of a song under the pointer.
    TRACK = "track";
    CARD = "card";
    CARD_PLAY = "card-play";
    ROW_PLAY = "row-play";
    /// The empty state (no library yet).
    EMPTY = "empty";
    /// The now-playing bar and its parts (the seek bar, the meter and the time played are
    /// markers too: the playback timer moves them in place).
    NOW_PLAYING = "now-playing";
    NOW_TITLE = "now-title";
    NOW_ARTIST = "now-artist";
    CONTROLS = "controls";
    PREVIOUS = "previous";
    PLAY = "play";
    NEXT = "next";
    SHUFFLE = "shuffle";
    REPEAT = "repeat";
    SEEK = "seek";
    ELAPSED = "elapsed";
    TOTAL = "total";
    QUEUE_BUTTON = "queue-button";
    MUTE = "mute";
    VOLUME = "volume";
    LEVEL = "level";
    /// The settings section's "Scan the music folder" button and the folder field.
    SCAN = "scan";
    FOLDER = "folder";
}

/// The name `stem` with the prefix, made at run time.
#[must_use]
pub fn named(stem: &str) -> AzString {
    AzString::from(format!("{PREFIX}{stem}"))
}

/// `<stem>-<n>` with the prefix: `numbered("album", 3)` is `__azmusic_album-3`.
#[must_use]
pub fn numbered(stem: &str, n: usize) -> AzString {
    AzString::from(format!("{PREFIX}{stem}-{n}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_prefix() {
        for name in [SEARCH, SONGS, ALBUMS, PAGE_PLAY, NOW_TITLE, SEEK, SCAN] {
            assert!(name.as_str().starts_with(PREFIX), "{}", name.as_str());
        }
        assert_eq!(numbered("album", 3).as_str(), "__azmusic_album-3");
        assert_eq!(named("playlist-x").as_str(), "__azmusic_playlist-x");
    }
}

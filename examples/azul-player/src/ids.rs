//! AzPlayer's DOM ids and markers, each name defined ONCE, every one carrying the app's
//! `__azplayer_` prefix (user ruling 2026-10-02, like the widgets' `__azul_`).

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azplayer_";

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azplayer_", $value));)*
    };
}

names! {
    /// The stage the video fills (black, the chrome over it).
    STAGE = "stage";
    /// The video widget.
    VIDEO = "video";
    /// The on-screen display (volume, seek): id and marker of its box, shown and hidden in place.
    OSD = "osd";
    /// The OSD's text node (marker: rewritten in place).
    OSD_TEXT = "osd-text";
    /// The library screen: the recent files and "Open".
    LIBRARY = "library";
    OPEN = "open";
    /// One recent file's tile in the library (class).
    TILE = "tile";
    /// The chrome over the video, shown and hidden in place (id and marker): the top strip (back,
    /// the title, fullscreen) and the bottom strip (the seek row and the transport).
    TOP = "top";
    BAR = "bar";
    /// The title of the file playing.
    TITLE = "title";
    /// The transport cluster (stop, back, play / pause, forward) and the volume buttons.
    CONTROLS = "controls";
    PLAY = "play";
    STOP = "stop";
    RESTART = "restart";
    REWIND = "rewind";
    FORWARD = "forward";
    MUTE = "mute";
    VOLUME_DOWN = "volume-down";
    VOLUME_UP = "volume-up";
    /// Back to the library.
    BACK = "back";
    /// The seek bar (moved in place: a marker).
    SEEK = "seek";
    /// The time played (its text node is a marker, rewritten in place) and the length.
    ELAPSED = "elapsed";
    TOTAL = "total";
    FULLSCREEN = "fullscreen";
    /// A note over the video (loading, why it cannot play).
    NOTE = "note";
}

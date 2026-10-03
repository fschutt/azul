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
    /// The stage the video fills (black, the OSD over it).
    STAGE = "stage";
    /// The video widget.
    VIDEO = "video";
    /// The on-screen display (volume, seek).
    OSD = "osd";
    /// The library screen: the recent files and "Open file".
    LIBRARY = "library";
    OPEN = "open";
    /// The controls bar and its parts (the seek bar is moved in place: a marker).
    BAR = "bar";
    TITLE = "title";
    CONTROLS = "controls";
    SEEK = "seek";
    FULLSCREEN = "fullscreen";
    /// A note under the video (loading, why it cannot play).
    NOTE = "note";
}

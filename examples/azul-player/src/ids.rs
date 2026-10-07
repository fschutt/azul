//! AzPlayer's DOM ids and markers, each name defined ONCE, every one carrying the app's
//! `__azplayer_` prefix (user ruling 2026-10-02, like the widgets' `__azul_`). The ids are also
//! what the engine recognises a node by from one build of the window to the next (a page, a
//! category of the strip, a tile): what keeps a moved tile's slide, a page's entrance and exit.

use azul::str::String as AzString;

/// The prefix of every name.
pub const PREFIX: &str = "__azplayer_";

/// An id made at runtime: the prefix and `name` (`id("tile-3")`).
#[must_use]
pub fn id(name: &str) -> AzString {
    AzString::from(format!("{PREFIX}{name}"))
}

macro_rules! names {
    ($($(#[$doc:meta])* $name:ident = $value:literal;)*) => {
        $($(#[$doc])* pub const $name: AzString = AzString::from_const_str(concat!("__azplayer_", $value));)*
    };
}

names! {
    /// The window's root: black, the menus and the stage over it.
    ROOT = "root";
    /// The menus: the ground, the corner pieces and the page (faded to black before a video).
    MENUS = "menus";
    /// The ground and its light: the base, the bloom, the fall-off, the two sets of shafts.
    GROUND = "ground";
    BLOOM = "bloom";
    FALLOFF = "falloff";
    RAYS_NEAR = "rays-near";
    RAYS_FAR = "rays-far";
    /// The Media Center orb and name, top left; the back button beside it (shown when the pointer
    /// moves: id and marker).
    LOGO = "logo";
    BACK = "back";
    CORNER = "corner";
    /// The clock, top right (its text node is a marker, rewritten in place).
    CLOCK = "clock";
    CLOCK_TEXT = "clock-text";
    /// The start strip: its page, the moving column of categories.
    START = "start";
    STRIP = "strip";
    /// A library's page: the views row, the gallery, its moving sheet of tiles, the status line.
    VIEWS = "views";
    GALLERY = "gallery";
    SHEET = "sheet";
    STATUS = "status";
    /// An empty library's sentence.
    EMPTY = "empty";
    /// One tile of a gallery or the strip (class).
    TILE = "tile";
    /// The "open a file" tile of the recent page.
    OPEN = "open";
    /// What plays: its page, and the small inset over the other pages.
    NOW_PLAYING = "now-playing";
    INSET = "inset";
    /// The search page and its field.
    SEARCH = "search";
    SEARCH_FIELD = "search-field";
    /// The address page: its field, its play button, the sample.
    ADDRESS_FIELD = "address-field";
    ADDRESS_PLAY = "address-play";
    ADDRESS_SAMPLE = "address-sample";
    /// The picture viewer and slide show.
    PICTURE = "picture";
    PICTURE_CAPTION = "picture-caption";
    /// The stage the video fills (black, the chrome over it).
    STAGE = "stage";
    /// The video widget.
    VIDEO = "video";
    /// The on-screen display (volume, seek): id and marker of its box, shown and hidden in place.
    OSD = "osd";
    /// The OSD's text node (marker: rewritten in place).
    OSD_TEXT = "osd-text";
    /// The chrome over the video or the now-playing page, shown and hidden in place (id and
    /// marker): the top strip (back, the title, fullscreen) and the bottom strip (the seek row
    /// and the transport).
    TOP = "top";
    BAR = "bar";
    /// The title of what plays.
    TITLE = "title";
    /// The transport (stop, back, play / pause, forward) and the volume buttons.
    CONTROLS = "controls";
    PLAY = "play";
    STOP = "stop";
    RESTART = "restart";
    PREVIOUS = "previous";
    NEXT = "next";
    REWIND = "rewind";
    FORWARD = "forward";
    MUTE = "mute";
    VOLUME_DOWN = "volume-down";
    VOLUME_UP = "volume-up";
    SHUFFLE = "shuffle";
    MUSIC = "music";
    FULLSCREEN = "fullscreen";
    /// The seek bar (moved in place: a marker).
    SEEK = "seek";
    /// The time played (its text node is a marker, rewritten in place) and the length.
    ELAPSED = "elapsed";
    TOTAL = "total";
    /// A note over the video (opening, why it cannot play).
    NOTE = "note";
}

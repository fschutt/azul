//! AzMusic's colours, per mode, in the look of Spotify's 2010 desktop player: charcoal panes
//! edge to edge, glossy grey bars and round buttons, striped song tables under a gradient header,
//! the lime of the old logo for what plays and for Play. Dark is the default (AzMusic starts dark
//! until the user picks a mode); the light look keeps the same structure in light greys. The
//! widgets (the seek bar, the volume slider, the meter, the settings page) follow the app theme
//! themselves; the search field and the empty library's block are flat in every theme (flora
//! writes them in Garamond, and the player's hand is a sans - `ui.rs`, `HAND`).

/// One mode's colours, as CSS values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Look {
    pub dark: bool,
    /// The tool bar (the window's title bar) and the now-playing bar: a glossy gradient.
    pub bar: &'static str,
    /// The sidebar, its current entry (a raised gradient) and an entry under the pointer.
    pub sidebar: &'static str,
    pub sidebar_current: &'static str,
    pub sidebar_hover: &'static str,
    /// The page, every other row of a song table, a row under the pointer, the selected row.
    pub page: &'static str,
    pub stripe: &'static str,
    pub hover: &'static str,
    pub selected: &'static str,
    /// The top of a page's header (it fades into the page).
    pub header_top: &'static str,
    /// The column titles' bar.
    pub columns: &'static str,
    pub text: &'static str,
    pub muted: &'static str,
    /// Hairlines between the panes and the columns.
    pub line: &'static str,
    /// The lime: the song that plays, a toggle that is on.
    pub accent: &'static str,
    /// The green Play button's gloss, under the pointer, and its ink.
    pub play: &'static str,
    pub play_hover: &'static str,
    pub on_play: &'static str,
    /// A glossy grey button (back, forward, the transport, Shuffle), under the pointer, its rim
    /// and its ink.
    pub button: &'static str,
    pub button_hover: &'static str,
    pub button_rim: &'static str,
    pub button_text: &'static str,
    /// The shadow under a cover.
    pub shadow: &'static str,
}

/// The default: Spotify 2010's charcoal.
pub const DARK: Look = Look {
    dark: true,
    bar: "linear-gradient(to bottom, #4a4a4a, #2c2c2c)",
    sidebar: "#2f2f2f",
    sidebar_current: "linear-gradient(to bottom, #5c5c5c, #454545)",
    sidebar_hover: "#3a3a3a",
    page: "#232323",
    stripe: "#292929",
    hover: "#343434",
    selected: "#4b4b4b",
    header_top: "#3c3c3c",
    columns: "linear-gradient(to bottom, #474747, #353535)",
    text: "#e6e6e6",
    muted: "#9b9b9b",
    line: "#161616",
    accent: "#84bd00",
    play: "linear-gradient(to bottom, #a8da2f, #6c9a00)",
    play_hover: "linear-gradient(to bottom, #bce64e, #7cb000)",
    on_play: "#142000",
    button: "linear-gradient(to bottom, #606060, #2e2e2e)",
    button_hover: "linear-gradient(to bottom, #747474, #3c3c3c)",
    button_rim: "#141414",
    button_text: "#eeeeee",
    shadow: "0px 2px 8px rgba(0, 0, 0, 0.7)",
};

/// The light look: the same panes in light greys.
pub const LIGHT: Look = Look {
    dark: false,
    bar: "linear-gradient(to bottom, #f4f4f4, #cfcfcf)",
    sidebar: "#e4e7eb",
    sidebar_current: "linear-gradient(to bottom, #cbd5e0, #b5c1ce)",
    sidebar_hover: "#d8dde3",
    page: "#ffffff",
    stripe: "#f3f5f8",
    hover: "#e6edf5",
    selected: "#cbd7e4",
    header_top: "#e4e4e4",
    columns: "linear-gradient(to bottom, #f8f8f8, #dfdfdf)",
    text: "#1e1e1e",
    muted: "#686868",
    line: "#b7b7b7",
    accent: "#4f7a00",
    play: "linear-gradient(to bottom, #a8da2f, #6c9a00)",
    play_hover: "linear-gradient(to bottom, #bce64e, #7cb000)",
    on_play: "#142000",
    button: "linear-gradient(to bottom, #ffffff, #d2d2d2)",
    button_hover: "linear-gradient(to bottom, #ffffff, #e4e4e4)",
    button_rim: "#9a9a9a",
    button_text: "#333333",
    shadow: "0px 2px 6px rgba(0, 0, 0, 0.25)",
};

/// The look of a mode.
#[must_use]
pub fn of(dark: bool) -> &'static Look {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_mode_has_its_look_and_both_keep_the_lime_play_button() {
        assert!(of(true).dark);
        assert!(!of(false).dark);
        assert_eq!(of(true).play, of(false).play);
        assert_ne!(of(true).page, of(false).page);
    }
}

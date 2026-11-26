//! The look: AzReview's dark theme and its pen colours, as the colours the screens use. A phone that is a remote
//! control for a desk with a pen tablet stays dark (it sits beside a bright page and a lit room): no light variant yet.
//! Colours are CSS strings so a screen writes `background: {SURFACE}` and a test can read them back.

/// The window behind everything.
pub const BG: &str = "#1b1b1d";
/// Cards, the pad, the keys.
pub const SURFACE: &str = "#2a2a2e";
/// A key that is pressed, a field.
pub const SURFACE_HI: &str = "#38383d";
/// The hairlines.
pub const SEPARATOR: &str = "#3f3f45";
pub const TEXT: &str = "#f2f2f5";
pub const TEXT_DIM: &str = "#a3a3ad";
/// Text on the accent.
pub const ON_ACCENT: &str = "#ffffff";
/// The selection blue of AzReview (the open file in its tree).
pub const ACCENT: &str = "#0a84ff";
/// A warning that is not an error.
pub const WARN: &str = "#e5a800";
pub const DANGER: &str = "#ff453a";

/// An agent is working (blue), a suggestion waits (yellow), done (green): the same three as AzReview's bottom bar.
pub const STATE_WORKING: &str = "#5aafff";
pub const STATE_SUGGESTED: &str = "#e5a800";
pub const STATE_DONE: &str = "#2fa84f";

/// The five pens of AzReview: scope (pink), issue (red), question (blue), duplicate (purple), praise (green).
pub const PENS: [&str; 5] = ["#ff4098", "#d62d20", "#205cd6", "#963cc8", "#188c46"];

/// The colour of the pen on the pad: the scope pen's pink.
pub const PEN_DOT: &str = "#ff4098";

/// The colour each of the four quick buttons carries on its edge (the first four pens').
#[must_use]
pub fn button_colour(index: usize) -> &'static str {
    PENS[index % 4]
}

/// `colour` (`#rrggbb`) at `alpha` (0 to 1) as an `rgba()` the CSS takes.
#[must_use]
pub fn tint(colour: &str, alpha: f32) -> String {
    let hex = colour.trim_start_matches('#');
    let channel = |i: usize| hex.get(i * 2..i * 2 + 2).and_then(|h| u8::from_str_radix(h, 16).ok()).unwrap_or(0);
    format!("rgba({}, {}, {}, {:.2})", channel(0), channel(1), channel(2), alpha.clamp(0.0, 1.0))
}

#[cfg(test)]
#[path = "theme_test.rs"]
mod theme_test;

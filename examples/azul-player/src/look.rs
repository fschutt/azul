//! The look and the motion of Windows Media Center (7), in azul's CSS.
//!
//! THE GROUND is Media Center's deep blue, lit from the upper left by flora's LIGHT SHAFTS - the
//! sunken stone's rig (`doc/templates/flora.css`, "THE STONE RIG": "A clip-path wedge anchored at
//! the upper-left corner holds a set of hard-edged bands raked 114deg", "THE SOURCE. The bloom
//! just off the upper-left corner"). azul draws no clip-path wedge, mask or blur cheaply, so the
//! shafts are soft-edged bands in a linear gradient on a layer of their own (two layers, each
//! fainter as the light spreads), the bloom and the far corner's fall-off are gradients on
//! layers of their own (one node per layer: the paint order of siblings is certain).
//!
//! THE MOTION has two speeds. The ground is SLOW, as flora's shafts are ("opacity over 1.8s,
//! travel over 2.4s"): the light drifts and brightens when the page or the category changes -
//! a transition, never an endless loop, so a window at rest costs nothing on the CPU renderer.
//! The foreground is FAST: pages, tiles and the focus move on springs (a few hundred ms; the
//! strip's own movement is the engine's spring for a moved node). Only `transform` and `opacity`
//! are animated: no frame of any animation lays the window out again.
//!
//! THE FOCUS is Media Center's glow: the focused tile scales up a little and lights up with a
//! soft blue-white glow and a bright rim (a layer of its own whose opacity fades), never the
//! browser's blue outline (the engine's focus ring is switched off for AzPlayer: the app draws
//! its own focus everywhere).

use crate::curtain::{self, Stage};

// ==== Colours ====

/// The text: white, and white dimmed for what is not focused.
pub const INK: &str = "#ffffff";
pub const INK_DIM: &str = "rgba(255, 255, 255, 0.55)";
pub const INK_FAINT: &str = "rgba(214, 232, 255, 0.38)";
/// Media Center's light blue (a progress bar, a selected word).
pub const ACCENT: &str = "#62b8ff";

// ==== The ground ====

/// The ground's base: Media Center's deep blue, lighter at the top.
pub const GROUND_BASE: &str = "background: linear-gradient(to bottom, #1b5aa6 0%, #10427f 30%, \
     #0a2e63 64%, #061b40 100%);";
/// The bloom just off the upper-left corner (flora's `::after` source), cool on blue.
pub const GROUND_BLOOM: &str = "background: linear-gradient(140deg, rgba(196, 228, 255, 0.36) \
     0%, rgba(196, 228, 255, 0.12) 20%, rgba(196, 228, 255, 0) 42%);";
/// The far corner falling away (flora: "the far corner falling away").
pub const GROUND_FALLOFF: &str = "background: linear-gradient(320deg, rgba(1, 8, 26, 0.6) 0%, \
     rgba(1, 8, 26, 0.22) 24%, rgba(1, 8, 26, 0) 46%);";
/// The first set of shafts: flora's three bands at 114deg, each dimmer than the last as the light
/// spreads, the hard edges softened in the gradient itself (no blur layer).
pub const RAYS_NEAR: &str = "background: linear-gradient(114deg, rgba(224, 240, 255, 0) 3%, \
     rgba(224, 240, 255, 0.5) 9%, rgba(224, 240, 255, 0.46) 15%, rgba(224, 240, 255, 0) 23%, \
     rgba(224, 240, 255, 0) 32%, rgba(224, 240, 255, 0.3) 37%, rgba(224, 240, 255, 0.26) 42%, \
     rgba(224, 240, 255, 0) 48%, rgba(224, 240, 255, 0) 56%, rgba(224, 240, 255, 0.18) 60%, \
     rgba(224, 240, 255, 0.14) 63%, rgba(224, 240, 255, 0) 68%);";
/// The second, wider and fainter set, raked a little steeper: the light's spread.
pub const RAYS_FAR: &str = "background: linear-gradient(121deg, rgba(150, 205, 255, 0) 10%, \
     rgba(150, 205, 255, 0.24) 19%, rgba(150, 205, 255, 0) 31%, rgba(150, 205, 255, 0) 45%, \
     rgba(150, 205, 255, 0.15) 52%, rgba(150, 205, 255, 0) 60%, rgba(150, 205, 255, 0) 71%, \
     rgba(150, 205, 255, 0.08) 76%, rgba(150, 205, 255, 0) 82%);";

/// Where the light stands for a page: the shafts' offset (logical px) and their opacity. Every
/// page and every category of the start strip has its own, so the light drifts as the user moves
/// - slowly (see [`RAYS_MOTION`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Light {
    pub near_x: f32,
    pub near_y: f32,
    pub near_opacity: f32,
    pub far_x: f32,
    pub far_opacity: f32,
}

impl Light {
    /// The light for `seed` (a page, a category): a small, steady offset per seed - the same
    /// page always has the same light.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn for_seed(seed: usize) -> Light {
        // Five positions the light visits, in an order that never jumps far.
        const STEPS: [(f32, f32, f32); 5] = [
            (0.0, 0.0, 0.34),
            (9.0, 4.0, 0.42),
            (16.0, 7.0, 0.38),
            (7.0, 10.0, 0.46),
            (-6.0, 5.0, 0.36),
        ];
        let (x, y, o) = STEPS[seed % STEPS.len()];
        Light {
            near_x: x,
            near_y: y,
            near_opacity: o,
            far_x: -x * 0.6,
            far_opacity: 0.5 + (seed % 3) as f32 * 0.12,
        }
    }
}

/// The shafts' motion: flora's sunken stone - travel over 2.4 s, opacity over 1.8 s, both eased
/// out (flora's `--fl-ease-out`).
pub const RAYS_MOTION: &str = "animation: transform 2400ms cubic-bezier(0.22, 0.61, 0.36, 1), \
     opacity 1800ms ease-in-out;";

// ==== The curtain: the fade to black before a video ====

/// A text node's part of the curtain: shown, or faded out FIRST.
#[must_use]
pub fn text_fade(stage: Stage) -> String {
    format!(
        "opacity: {}; animation: opacity {}ms ease-in, color 180ms ease-out;",
        if stage.menus { 1 } else { 0 },
        curtain::TEXT_FADE_MS
    )
}

/// An icon's part of the curtain: shown, or faded out SECOND (after the text has begun).
#[must_use]
pub fn icon_fade(stage: Stage) -> String {
    format!(
        "opacity: {}; animation: opacity {}ms {}ms ease-in;",
        if stage.menus { 1 } else { 0 },
        curtain::ICON_FADE_MS,
        curtain::ICON_DELAY_MS
    )
}

/// The ground's part of the curtain: the blue and its light fade to black LAST.
#[must_use]
pub fn ground_fade(stage: Stage) -> String {
    format!(
        "opacity: {}; animation: opacity {}ms {}ms ease-in-out;",
        if stage.menus { 1 } else { 0 },
        curtain::GROUND_FADE_MS,
        curtain::GROUND_DELAY_MS
    )
}

/// The picture's part of the curtain: hidden (black) until the menus are gone, then faded in.
#[must_use]
pub fn picture_fade(stage: Stage) -> String {
    format!(
        "opacity: {}; animation: opacity {}ms ease-out;",
        if stage.picture { 1 } else { 0 },
        curtain::FADE_IN_MS
    )
}

// ==== The focus ====

/// A tile's face: scaled up when focused, on a snappy spring.
#[must_use]
pub fn face(focused: bool) -> String {
    format!(
        "transform: scale({}); animation: transform 280ms spring-snappy;",
        if focused { "1.07" } else { "1" }
    )
}

/// The glow over a focused face: the bright rim and the soft blue-white light around it, a
/// layer of its own whose opacity fades in fast and out a little slower.
#[must_use]
pub fn glow(focused: bool, radius: f32) -> String {
    format!(
        "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px; border-radius: \
         {radius}px; box-shadow: 0px 0px 18px 3px rgba(118, 196, 255, 0.85), 0px 0px 3px 0px \
         rgba(255, 255, 255, 0.9); border: 2px solid rgba(236, 246, 255, 0.95); opacity: {}; \
         animation: opacity {}ms ease-out;",
        if focused { 1 } else { 0 },
        if focused { 140 } else { 220 }
    )
}

/// A caption's ink: bright when its tile is focused.
#[must_use]
pub fn caption_ink(focused: bool) -> &'static str {
    if focused {
        INK
    } else {
        INK_DIM
    }
}

// ==== The pages' entrances and exits ====

/// The keyframes every page, tile and slide uses; attached to the window's root on every build
/// (an exit is resolved in the OLD tree's sheet, an entrance in the new one's).
pub const KEYFRAMES: &str = "\
@keyframes azp-page-in { from { opacity: 0; transform: scale(1.04); } to { opacity: 1; transform: scale(1); } }
@keyframes azp-page-out { from { opacity: 1; transform: scale(1); } to { opacity: 0; transform: scale(0.96); } }
@keyframes azp-tile-in { from { opacity: 0; transform: translateX(28px); } to { opacity: 1; transform: translateX(0px); } }
@keyframes azp-tile-out { from { opacity: 1; } to { opacity: 0; } }
@keyframes azp-rise-in { from { opacity: 0; transform: translateY(14px); } to { opacity: 1; transform: translateY(0px); } }
@keyframes azp-fade-in { from { opacity: 0; } to { opacity: 1; } }
@keyframes azp-fade-out { from { opacity: 1; } to { opacity: 0; } }
@keyframes azp-burns-0 { 0% { opacity: 0; transform: scale(1); } 12% { opacity: 1; } 100% { opacity: 1; transform: scale(1.09); } }
@keyframes azp-burns-1 { 0% { opacity: 0; transform: scale(1.09); } 12% { opacity: 1; } 100% { opacity: 1; transform: scale(1); } }
@keyframes azp-burns-2 { 0% { opacity: 0; transform: scale(1.07) translateX(-2%); } 12% { opacity: 1; } 100% { opacity: 1; transform: scale(1.07) translateX(2%); } }
@keyframes azp-burns-3 { 0% { opacity: 0; transform: scale(1.07) translateY(2%); } 12% { opacity: 1; } 100% { opacity: 1; transform: scale(1.07) translateY(-2%); } }
";

/// A page's entrance and exit: in on a spring (it leaves at speed, scales down into place), out
/// quickly. `exits`: whether it fades out when it goes (not while a video's curtain is down -
/// the black it left would cover the picture fading in).
#[must_use]
pub fn page_motion(exits: bool) -> &'static str {
    if exits {
        "-azul-animation-in: azp-page-in 360ms spring; -azul-animation-out: azp-page-out 200ms \
         ease-in;"
    } else {
        "-azul-animation-in: azp-page-in 360ms spring;"
    }
}

/// A tile's entrance (staggered by its place in the row: `order`) and exit.
#[must_use]
pub fn tile_motion(order: usize) -> String {
    format!(
        "-azul-animation-in: azp-tile-in 300ms {}ms spring; -azul-animation-out: azp-tile-out \
         140ms ease-in;",
        (order.min(8) * 28)
    )
}

/// A slide of the slide show: Ken Burns' slow pan and zoom (one of four, by the slide's number),
/// the fade in at its start; the fade out when the next one comes is its exit.
#[must_use]
pub fn slide_motion(index: usize, seconds: u64) -> String {
    format!(
        "-azul-animation-in: azp-burns-{} {}ms linear; -azul-animation-out: azp-fade-out 900ms \
         ease-in-out no-clip;",
        index % 4,
        seconds * 1000 + 900
    )
}

// ==== Pieces ====

/// Absolutely over the whole of its container.
pub const FILL: &str =
    "position: absolute; left: 0px; top: 0px; right: 0px; bottom: 0px;";

/// A page's big lower-case title (Media Center's "music").
pub const PAGE_TITLE: &str = "font-size: 50px; font-weight: 300; letter-spacing: -1px; \
     margin: 0px; white-space: nowrap;";

/// A round glass button's face (on its icon): a white rim, a faint shine, the blue glow under
/// the pointer (`:hover`) and while it has the keyboard focus (`:focus`, on the button box).
pub const ROUND: &str = "display: flex; align-items: center; justify-content: center; \
     box-sizing: border-box; flex-shrink: 0; color: #ffffff; border: 2px solid rgba(255, 255, \
     255, 0.8); background: linear-gradient(to bottom, rgba(255, 255, 255, 0.28), rgba(255, \
     255, 255, 0.04)); :hover { border: 2px solid #ffffff; background: linear-gradient(to \
     bottom, #7cc8ff, #1d6fd0); box-shadow: 0px 0px 12px rgba(95, 180, 255, 0.9); }";

/// The green orb (Media Center's start button).
pub const ORB: &str = "display: flex; align-items: center; justify-content: center; \
     flex-shrink: 0; border: 2px solid rgba(255, 255, 255, 0.85); background: \
     linear-gradient(to bottom, #8fe063, #2f9a1c); box-shadow: 0px 0px 10px rgba(120, 220, \
     100, 0.55);";

/// A panel over the picture (the OSD, a note).
pub const PANEL: &str = "position: absolute; margin: 0px; padding: 10px 18px; border-radius: \
     6px; font-size: 16px; color: #ffffff; background: rgba(4, 18, 43, 0.82); border: 1px solid \
     rgba(255, 255, 255, 0.35);";

/// A tile's colour for something without a picture: one of Media Center's blues and teals,
/// steady per `seed`.
#[must_use]
pub fn tile_colour(seed: &str) -> &'static str {
    const COLOURS: [&str; 6] = [
        "linear-gradient(to bottom, #2f78c4, #133f7c)",
        "linear-gradient(to bottom, #2a8f9c, #0f4f5c)",
        "linear-gradient(to bottom, #4d6fc0, #20357a)",
        "linear-gradient(to bottom, #3a8ad0, #164b86)",
        "linear-gradient(to bottom, #5a5fb8, #262a6e)",
        "linear-gradient(to bottom, #2b9b78, #0f5440)",
    ];
    let hash = seed
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    COLOURS[(hash as usize) % COLOURS.len()]
}

/// The initials a tile without a picture shows ("The Long Film" -> "TL").
#[must_use]
pub fn initials(label: &str) -> String {
    label
        .split_whitespace()
        .filter_map(|w| w.chars().find(|c| c.is_alphanumeric()))
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::curtain::Curtain;

    #[test]
    fn the_curtain_fades_text_then_icons_then_the_ground_and_shows_the_picture_last() {
        let closed = Curtain::Closed.stage();
        assert!(text_fade(closed).starts_with("opacity: 1;"));
        assert!(picture_fade(closed).starts_with("opacity: 0;"));
        let mut c = Curtain::preroll(0);
        c.picture_ready();
        c.sound_ready();
        let _ = c.step(1);
        let fading = c.stage();
        assert!(text_fade(fading).starts_with("opacity: 0;"));
        assert!(icon_fade(fading).contains("ms 140ms"), "{}", icon_fade(fading));
        assert!(ground_fade(fading).contains("ms 300ms"), "{}", ground_fade(fading));
        assert!(picture_fade(fading).starts_with("opacity: 0;"), "black between the two");
    }

    #[test]
    fn the_light_differs_per_page_and_stays_the_same_for_one() {
        assert_eq!(Light::for_seed(3), Light::for_seed(3));
        assert_ne!(Light::for_seed(1), Light::for_seed(2));
        assert_eq!(Light::for_seed(0), Light::for_seed(5), "five places, then round again");
    }

    #[test]
    fn a_tile_without_a_picture_has_steady_initials_and_colour() {
        assert_eq!(initials("The Long Film"), "TL");
        assert_eq!(initials("  ok "), "O");
        assert_eq!(initials(""), "");
        assert_eq!(tile_colour("abc"), tile_colour("abc"));
        assert!(tile_motion(2).contains("56ms"));
        assert!(tile_motion(99).contains("224ms"), "the stagger has an end");
        assert!(slide_motion(5, 6).contains("azp-burns-1 6900ms"));
    }
}

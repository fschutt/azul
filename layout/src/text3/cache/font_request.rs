//! The font REQUEST: everything a style asks of a face, in one exact, hashable value.
//!
//! A [`FontRequest`] holds the weight, the stretch, the style and the angle of an oblique one, the
//! optical size and the explicit `font-variation-settings` of a node, none of them rounded to a
//! bucket. It is carried by every [`FontSelector`] and by every [`FontChainKey`], so two requests
//! that differ in any coordinate never share a chain, a shaped run or a laid-out paragraph.
//!
//! The request is face-independent. What a face does with it is decided where the chain is
//! resolved: a STATIC face is chosen by the bucket of the weight ([`FontRequest::fc_weight`], as
//! rust-fontconfig ranks the files of a family), and a VARIABLE face is replaced by its static
//! instance at the coordinates the request asks of its axes
//! ([`crate::solver3::getters::select_variable_instances`]). Shaping, glyph decode and the PDF
//! writer therefore only ever see static faces.

use std::sync::Arc;

use rust_fontconfig::FcWeight;

// `FontSelector`, `FontChainKey`, `FontStyle`, `FourCc` and `optical_size_for`, which the sibling
// modules of the text cache share.
#[allow(clippy::wildcard_imports)]
use super::*;

/// A font coordinate - a weight, a stretch in percent, an angle in degrees - in steps of 1/64.
///
/// Exact for every value a stylesheet writes with up to six binary fraction digits (350, 650,
/// 87.5, 12.25), and a plain integer, so that it can sit in a hash key and be ordered. Two values
/// closer than 1/64 are the same coordinate: no face is drawn at a finer step.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct FontCoord(i32);

impl FontCoord {
    /// How many steps make one unit (one weight, one percent, one degree).
    pub const STEPS: i32 = 64;

    /// `value` rounded to the nearest step (half away from zero); NaN is 0 and a value out of the
    /// range of the steps saturates.
    #[must_use]
    pub const fn from_f32(value: f32) -> Self {
        let scaled = value * 64.0;
        // `as` truncates toward zero, saturates, and makes NaN 0.
        let whole = scaled as i32;
        let rest = scaled - whole as f32;
        let step = if rest >= 0.5 {
            1
        } else if rest <= -0.5 {
            -1
        } else {
            0
        };
        Self(whole.saturating_add(step))
    }

    /// The coordinate of a whole number of units (saturating for a huge one).
    #[must_use]
    pub const fn from_units(units: i32) -> Self {
        Self(units.saturating_mul(Self::STEPS))
    }

    /// The coordinate as a number of units.
    #[must_use]
    pub const fn to_f32(self) -> f32 {
        self.0 as f32 / 64.0
    }

    /// The coordinate in steps of 1/64.
    #[must_use]
    pub const fn steps(self) -> i32 {
        self.0
    }
}

/// What a style asks of a face. Face-independent, exact, hashable.
///
/// The default is the initial value of every property: weight 400, stretch 100%, upright, the
/// face's own optical size, no settings.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct FontRequest {
    /// `font-weight`: a CSS number from 1 to 1000 (400 is `normal`, 700 `bold`).
    pub weight: FontCoord,
    /// `font-stretch` in percent (100 is `normal`, 75 `condensed`, 125 `expanded`).
    pub stretch: FontCoord,
    /// `font-style`.
    pub style: FontStyle,
    /// The angle of `font-style: oblique <angle>` in degrees (a bare `oblique` is 14, positive
    /// leans to the right), 0 for every other style: the angle of an upright or an italic request
    /// is not part of it.
    pub oblique_degrees: FontCoord,
    /// The optical size a variable face with an `opsz` axis is drawn at: the used font size in
    /// whole CSS px (`font-optical-sizing: auto`, as Chrome and CoreText do), 0 for the face's
    /// own (`font-optical-sizing: none`, or a request that was never told a size). macOS draws
    /// its UI in ONE such face (`SFNS.ttf`, opsz 17 to 96, default 28): 13px text is "SF Pro
    /// Text" (opsz 17), not the default's wider-set "Display" design.
    pub optical_size: u16,
    /// `font-variation-settings`: `(tag, value)` sorted by tag, one per tag (the last one written
    /// wins). `None` is `normal`. Explicit: it wins over every coordinate the properties above
    /// derive for the same axis.
    pub settings: Option<Arc<[(FourCc, FontCoord)]>>,
}

impl Default for FontRequest {
    fn default() -> Self {
        Self {
            weight: FontCoord::from_units(400),
            stretch: FontCoord::from_units(100),
            style: FontStyle::Normal,
            oblique_degrees: FontCoord::default(),
            optical_size: 0,
            settings: None,
        }
    }
}

// The builders take `self` (a request owns an `Arc`), which no `const fn` may drop.
#[allow(clippy::missing_const_for_fn)]
impl FontRequest {
    /// The angle of a bare `oblique`, in degrees (CSS Fonts 4, 2.3).
    pub const BARE_OBLIQUE_DEGREES: f32 = 14.0;

    /// The most axes a request sets explicitly. A font has a handful of axes (the widest in use
    /// has a dozen) and a setting for a tag the face lacks is ignored, so a stylesheet that
    /// names more than this many distinct tags says nothing a face could use: the tags written
    /// first are kept, a later value of a kept tag still wins, the rest are dropped. It bounds
    /// the work of a hostile list, which would otherwise be quadratic.
    pub const MAX_SETTINGS: usize = 64;

    /// The bucket of the weight, the weight rust-fontconfig ranks the static files of a family
    /// by. A static face is chosen by it alone; a variable one is drawn at the exact weight.
    #[must_use]
    pub fn fc_weight(&self) -> FcWeight {
        // The bucket NEAREST to the weight, a weight halfway between two going to the heavier
        // (the buckets of `FcWeight::from_u16`, at the steps of the coordinate: a weight of 149.9
        // is still `Thin`).
        let from = |units: i32| self.weight_at_least(units);
        if from(850) {
            FcWeight::Black
        } else if from(750) {
            FcWeight::ExtraBold
        } else if from(650) {
            FcWeight::Bold
        } else if from(550) {
            FcWeight::SemiBold
        } else if from(450) {
            FcWeight::Medium
        } else if from(350) {
            FcWeight::Normal
        } else if from(250) {
            FcWeight::Light
        } else if from(150) {
            FcWeight::ExtraLight
        } else {
            FcWeight::Thin
        }
    }

    /// Whether the request is for an italic face.
    #[must_use]
    pub fn is_italic(&self) -> bool {
        self.style == FontStyle::Italic
    }

    /// Whether the request is for an oblique face.
    #[must_use]
    pub fn is_oblique(&self) -> bool {
        self.style == FontStyle::Oblique
    }

    /// Whether the weight is at least `units` (a CSS weight): exact, no bucket.
    #[must_use]
    pub fn weight_at_least(&self, units: i32) -> bool {
        self.weight >= FontCoord::from_units(units)
    }

    /// This request with the weight `weight` (any number; a stylesheet's is 1 to 1000).
    #[must_use]
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = FontCoord::from_f32(weight);
        self
    }

    /// This request with the weight of the bucket `weight`: 100 for `Thin` to 900 for `Black`.
    #[must_use]
    pub fn with_weight_bucket(mut self, weight: FcWeight) -> Self {
        self.weight = FontCoord::from_units(i32::from(weight as u16));
        self
    }

    /// This request with the stretch `percent` (100 is `normal`).
    #[must_use]
    pub fn with_stretch(mut self, percent: f32) -> Self {
        self.stretch = FontCoord::from_f32(percent);
        self
    }

    /// This request with `font-style: oblique <degrees>`.
    #[must_use]
    pub fn with_oblique(mut self, degrees: f32) -> Self {
        self.style = FontStyle::Oblique;
        self.oblique_degrees = FontCoord::from_f32(degrees);
        self
    }

    /// This request with the style `style`: a bare `oblique` leans by
    /// [`Self::BARE_OBLIQUE_DEGREES`], the other two have no angle.
    #[must_use]
    pub fn with_style(mut self, style: FontStyle) -> Self {
        self.style = style;
        self.oblique_degrees = if style == FontStyle::Oblique {
            FontCoord::from_f32(Self::BARE_OBLIQUE_DEGREES)
        } else {
            FontCoord::default()
        };
        self
    }

    /// This request at the optical size of text of `font_size_px`
    /// ([`crate::text3::cache::optical_size_for`]): the font size rounded to whole CSS px.
    #[must_use]
    pub fn at_font_size(mut self, font_size_px: f32) -> Self {
        self.optical_size = optical_size_for(font_size_px);
        self
    }

    /// This request with the explicit settings `settings` of `font-variation-settings`: sorted
    /// by tag, a tag written twice keeps its last value, none at all is `normal`, and no more
    /// than [`Self::MAX_SETTINGS`] distinct tags are kept.
    #[must_use]
    pub fn with_settings(mut self, settings: &[(FourCc, f32)]) -> Self {
        let mut sorted: Vec<(FourCc, FontCoord)> =
            Vec::with_capacity(settings.len().min(Self::MAX_SETTINGS));
        for (tag, value) in settings {
            let value = FontCoord::from_f32(*value);
            match sorted.iter_mut().find(|(known, _)| known == tag) {
                Some(slot) => slot.1 = value,
                None if sorted.len() < Self::MAX_SETTINGS => sorted.push((*tag, value)),
                None => {}
            }
        }
        sorted.sort_by_key(|(tag, _)| *tag);
        self.settings = if sorted.is_empty() {
            None
        } else {
            Some(Arc::from(sorted))
        };
        self
    }

    /// This request with the value `value` of the axis `tag` set explicitly, as one more entry of
    /// `font-variation-settings` (it replaces an entry of the same tag).
    #[must_use]
    pub fn with_setting(self, tag: FourCc, value: f32) -> Self {
        let mut all: Vec<(FourCc, f32)> = self
            .settings
            .as_deref()
            .map(|known| known.iter().map(|(known, v)| (*known, v.to_f32())).collect())
            .unwrap_or_default();
        all.push((tag, value));
        self.with_settings(&all)
    }

    /// The explicit value of the axis `tag` in `font-variation-settings`, if there is one.
    #[must_use]
    pub fn setting(&self, tag: FourCc) -> Option<FontCoord> {
        self.settings
            .as_deref()?
            .iter()
            .find(|(known, _)| *known == tag)
            .map(|(_, value)| *value)
    }
}

#[cfg(test)]
#[path = "font_request_test.rs"]
mod font_request_test;

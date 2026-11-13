//! The deck themes the Design tab and File > New offer: Office (white) and
//! the five Flora stone families (deep blue "Stone", Leaf, Plum, Clay, Slate),
//! each in three colour variants (light paper, the deep tone, the accent
//! ground), with any of the font schemes. A deck stores the colours it was
//! made with, so a later change of these presets does not change old decks.
//!
//! The stone ramps are flora's accent families (`azul::shells::ShellThemeAccent`):
//! the accent, its deep tone, its soft wash and its glow, read from
//! `ShellThemeAccent::colors`, and flora's paper ink (`on_accent`).

use azul::shells::ShellThemeAccent;

use crate::model::{Color, ColorScheme, FontScheme, Theme};

/// One stone family: accent, deep, soft, glow.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stone {
    pub name: &'static str,
    pub accent: Color,
    pub deep: Color,
    pub soft: Color,
    pub glow: Color,
}

/// The ink of a light variant.
pub const INK: Color = Color::rgb(0x2B, 0x2A, 0x27);

/// The five stone families, in flora's order: the name a deck stores and
/// the shell accent family it is.
pub const FAMILIES: [(&str, ShellThemeAccent); 5] = [
    ("Stone", ShellThemeAccent::Blue),
    ("Leaf", ShellThemeAccent::Leaf),
    ("Plum", ShellThemeAccent::Plum),
    ("Clay", ShellThemeAccent::Clay),
    ("Slate", ShellThemeAccent::Slate),
];

fn color(c: azul::css::ColorU) -> Color {
    Color::rgba(c.r, c.g, c.b, c.a)
}

/// Flora's paper ink, the ground of a light variant and the ink on a dark one.
#[must_use]
pub fn paper() -> Color {
    color(ShellThemeAccent::Blue.colors(false).on_accent)
}

/// Stone family `i` of [`FAMILIES`]: its light-mode ramp.
#[must_use]
pub fn stone(i: usize) -> Option<Stone> {
    let (name, family) = *FAMILIES.get(i)?;
    let c = family.colors(false);
    Some(Stone {
        name,
        accent: color(c.accent),
        deep: color(c.deep),
        soft: color(c.soft),
        glow: color(c.glow),
    })
}

/// The shell accent that goes with theme `index` of [`names`]: the stone's
/// family, Blue for Office.
#[must_use]
pub fn accent(index: usize) -> ShellThemeAccent {
    index
        .checked_sub(1)
        .and_then(|i| FAMILIES.get(i))
        .map_or(ShellThemeAccent::Blue, |(_, family)| *family)
}

/// The colour variants of a stone.
pub const VARIANTS: [&str; 3] = ["Paper", "Deep", "Accent"];

/// The theme names File > New and the Design tab show: Office, then the stones.
#[must_use]
pub fn names() -> Vec<&'static str> {
    let mut out = vec!["Office"];
    out.extend(FAMILIES.iter().map(|(name, _)| *name));
    out
}

/// Theme `index` of [`names`] in colour variant `variant` with font scheme
/// `fonts` (of [`FontScheme::all`]).
#[must_use]
pub fn theme(index: usize, variant: usize, fonts: usize) -> Theme {
    let font_schemes = FontScheme::all();
    let fonts = font_schemes
        .get(fonts)
        .cloned()
        .unwrap_or_else(|| font_schemes[0].clone());
    let Some(stone) = index.checked_sub(1).and_then(stone) else {
        let mut office = Theme::office();
        office.fonts = fonts;
        return office;
    };
    let variant = variant.min(VARIANTS.len() - 1);
    let colors = match variant {
        0 => ColorScheme {
            name: format!("{} {}", stone.name, VARIANTS[0]),
            background: paper(),
            text: INK,
            title: stone.deep,
            accent: stone.accent,
            accent2: stone.glow,
            accent3: stone.soft,
        },
        1 => ColorScheme {
            name: format!("{} {}", stone.name, VARIANTS[1]),
            background: stone.deep,
            text: paper(),
            title: Color::rgb(0xFF, 0xFF, 0xFF),
            accent: stone.glow,
            accent2: stone.soft,
            accent3: stone.accent,
        },
        _ => ColorScheme {
            name: format!("{} {}", stone.name, VARIANTS[2]),
            background: stone.accent,
            text: paper(),
            title: Color::rgb(0xFF, 0xFF, 0xFF),
            accent: stone.glow,
            accent2: stone.soft,
            accent3: stone.deep,
        },
    };
    Theme {
        name: stone.name.to_string(),
        colors,
        fonts,
    }
}

/// The index of [`names`] a deck's theme came from (by name), 0 if none.
#[must_use]
pub fn index_of(theme: &Theme) -> usize {
    names().iter().position(|n| *n == theme.name).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn office_comes_first_then_the_five_stones() {
        assert_eq!(names(), vec!["Office", "Stone", "Leaf", "Plum", "Clay", "Slate"]);
        assert_eq!(theme(0, 2, 0).colors, Theme::office().colors);
    }

    #[test]
    fn every_variant_keeps_its_ink_readable_on_its_ground() {
        for index in 1..names().len() {
            for variant in 0..VARIANTS.len() {
                let t = theme(index, variant, 0);
                let (bg, ink) = (t.colors.background.luminance(), t.colors.text.luminance());
                let contrast = (bg.max(ink) + 0.05) / (bg.min(ink) + 0.05);
                assert!(contrast >= 4.5, "{} {}: {contrast}", t.name, VARIANTS[variant]);
                assert_eq!(index_of(&t), index);
            }
        }
    }

    /// The stones are the shell's accent families, not a copy of them
    /// (DEDUP_OFFICE D1): flora's blue, its paper ink.
    #[test]
    fn the_stones_are_the_shells_accent_families() {
        let blue = stone(0).expect("the first stone");
        assert_eq!(blue.name, "Stone");
        assert_eq!(blue.accent, Color::rgb(0x2F, 0x4A, 0x85));
        assert_eq!(blue.deep, Color::rgb(0x1E, 0x32, 0x60));
        assert_eq!(stone(4).expect("Slate").glow, Color::rgb(0x8A, 0xA0, 0xB0));
        assert_eq!(paper(), Color::rgb(0xF4, 0xF2, 0xEA));
        assert!(stone(5).is_none());
    }

    #[test]
    fn the_font_scheme_is_the_one_asked_for() {
        let schemes = FontScheme::all();
        assert_eq!(theme(3, 0, 2).fonts, schemes[2]);
        assert_eq!(theme(0, 0, 1).fonts, schemes[1]);
    }
}

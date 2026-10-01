//! The deck themes the Design tab and File > New offer: Office (white) and
//! the five Flora stone families (deep blue "Stone", Leaf, Plum, Clay, Slate),
//! each in three colour variants (light paper, the deep tone, the accent
//! ground), with any of the font schemes. A deck stores the colours it was
//! made with, so a later change of these presets does not change old decks.
//!
//! The stone ramps are flora's (`azul::shells::ShellThemeAccent`): the
//! accent, its deep tone, its soft wash and its glow. They are spelled out
//! here because `ShellThemeAccent::colors` is not in the public API yet (see
//! the report); once it is, these four colours come from there.

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

/// Flora's paper ink, the ground of a light variant and the ink on a dark one.
pub const PAPER: Color = Color::rgb(0xF4, 0xF2, 0xEA);
/// The ink of a light variant.
pub const INK: Color = Color::rgb(0x2B, 0x2A, 0x27);

/// The five stone families, in flora's order.
pub const STONES: [Stone; 5] = [
    Stone {
        name: "Stone",
        accent: Color::rgb(0x2F, 0x4A, 0x85),
        deep: Color::rgb(0x1E, 0x32, 0x60),
        soft: Color::rgb(0xE0, 0xE4, 0xEE),
        glow: Color::rgb(0x7A, 0x93, 0xC6),
    },
    Stone {
        name: "Leaf",
        accent: Color::rgb(0x44, 0x68, 0x4F),
        deep: Color::rgb(0x2F, 0x4C, 0x39),
        soft: Color::rgb(0xE1, 0xE6, 0xE1),
        glow: Color::rgb(0x7F, 0xA9, 0x8C),
    },
    Stone {
        name: "Plum",
        accent: Color::rgb(0x57, 0x4A, 0x66),
        deep: Color::rgb(0x3E, 0x34, 0x4B),
        soft: Color::rgb(0xE5, 0xE1, 0xEA),
        glow: Color::rgb(0x8E, 0x80, 0xA2),
    },
    Stone {
        name: "Clay",
        accent: Color::rgb(0x7E, 0x4A, 0x42),
        deep: Color::rgb(0x5E, 0x33, 0x2D),
        soft: Color::rgb(0xEA, 0xE0, 0xDD),
        glow: Color::rgb(0xB3, 0x83, 0x7A),
    },
    Stone {
        name: "Slate",
        accent: Color::rgb(0x4A, 0x5C, 0x6B),
        deep: Color::rgb(0x35, 0x45, 0x51),
        soft: Color::rgb(0xDE, 0xE3, 0xE7),
        glow: Color::rgb(0x8A, 0xA0, 0xB0),
    },
];

/// The colour variants of a stone.
pub const VARIANTS: [&str; 3] = ["Paper", "Deep", "Accent"];

/// The theme names File > New and the Design tab show: Office, then the stones.
#[must_use]
pub fn names() -> Vec<&'static str> {
    let mut out = vec!["Office"];
    out.extend(STONES.iter().map(|s| s.name));
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
    let Some(stone) = index.checked_sub(1).and_then(|i| STONES.get(i)) else {
        let mut office = Theme::office();
        office.fonts = fonts;
        return office;
    };
    let variant = variant.min(VARIANTS.len() - 1);
    let colors = match variant {
        0 => ColorScheme {
            name: format!("{} {}", stone.name, VARIANTS[0]),
            background: PAPER,
            text: INK,
            title: stone.deep,
            accent: stone.accent,
            accent2: stone.glow,
            accent3: stone.soft,
        },
        1 => ColorScheme {
            name: format!("{} {}", stone.name, VARIANTS[1]),
            background: stone.deep,
            text: PAPER,
            title: Color::rgb(0xFF, 0xFF, 0xFF),
            accent: stone.glow,
            accent2: stone.soft,
            accent3: stone.accent,
        },
        _ => ColorScheme {
            name: format!("{} {}", stone.name, VARIANTS[2]),
            background: stone.accent,
            text: PAPER,
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

    #[test]
    fn the_font_scheme_is_the_one_asked_for() {
        let schemes = FontScheme::all();
        assert_eq!(theme(3, 0, 2).fonts, schemes[2]);
        assert_eq!(theme(0, 0, 1).fonts, schemes[1]);
    }
}

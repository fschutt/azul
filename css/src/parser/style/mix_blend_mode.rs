use core::fmt;

use crate::{css_properties::*, parser::*};

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleMixBlendMode {
    Normal,
    Multiply,
    Screen,
    Overlay,
    Darken,
    Lighten,
    ColorDodge,
    ColorBurn,
    HardLight,
    SoftLight,
    Difference,
    Exclusion,
    Hue,
    Saturation,
    Color,
    Luminosity,
}

impl Default for StyleMixBlendMode {
    fn default() -> StyleMixBlendMode {
        StyleMixBlendMode::Normal
    }
}

impl fmt::Display for StyleMixBlendMode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        use self::StyleMixBlendMode::*;
        write!(
            f,
            "{}",
            match self {
                Normal => "normal",
                Multiply => "multiply",
                Screen => "screen",
                Overlay => "overlay",
                Darken => "darken",
                Lighten => "lighten",
                ColorDodge => "color-dodge",
                ColorBurn => "color-burn",
                HardLight => "hard-light",
                SoftLight => "soft-light",
                Difference => "difference",
                Exclusion => "exclusion",
                Hue => "hue",
                Saturation => "saturation",
                Color => "color",
                Luminosity => "luminosity",
            }
        )
    }
}

multi_type_parser!(
    parse_style_mix_blend_mode,
    StyleMixBlendMode,
    ["normal", Normal],
    ["multiply", Multiply],
    ["screen", Screen],
    ["overlay", Overlay],
    ["darken", Darken],
    ["lighten", Lighten],
    ["color-dodge", ColorDodge],
    ["color-burn", ColorBurn],
    ["hard-light", HardLight],
    ["soft-light", SoftLight],
    ["difference", Difference],
    ["exclusion", Exclusion],
    ["hue", Hue],
    ["saturation", Saturation],
    ["color", Color],
    ["luminosity", Luminosity]
);

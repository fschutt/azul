//! CSS color value types, parsing, and formatting

use crate::error::{CssColorParseError, CssParsingError};
use crate::props::formatter::FormatAsCssValue;
use alloc::{format, string::String};
use core::fmt;

/// u8-based color, range 0 to 255 (similar to webrenders ColorU)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct ColorU {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

/// f32-based color, range 0.0 to 1.0 (similar to webrenders ColorF)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ColorF {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

/// Component type for CSS color parsing (used in rgb/hsl functions)
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub enum CssColorComponent {
    /// Percentage value (0-100%)
    Percentage(f32),
    /// Integer value (0-255 for rgb, 0-360 for hue, etc.)
    Integer(u16),
    /// Float value
    Float(f32),
}

impl Default for ColorU {
    fn default() -> Self {
        ColorU::BLACK
    }
}

impl fmt::Display for ColorU {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "rgba({}, {}, {}, {})",
            self.r,
            self.g,
            self.b,
            self.a as f32 / 255.0
        )
    }
}

impl ColorU {
    pub const ALPHA_TRANSPARENT: u8 = 0;
    pub const ALPHA_OPAQUE: u8 = 255;

    pub const RED: ColorU = ColorU {
        r: 255,
        g: 0,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };

    pub const GREEN: ColorU = ColorU {
        r: 0,
        g: 255,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };

    pub const BLUE: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 255,
        a: Self::ALPHA_OPAQUE,
    };

    pub const WHITE: ColorU = ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: Self::ALPHA_OPAQUE,
    };

    pub const BLACK: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: Self::ALPHA_OPAQUE,
    };

    pub const TRANSPARENT: ColorU = ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: Self::ALPHA_TRANSPARENT,
    };

    pub const fn new_rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub fn interpolate(&self, other: &Self, t: f32) -> Self {
        Self {
            r: libm::roundf(self.r as f32 + (other.r as f32 - self.r as f32) * t) as u8,
            g: libm::roundf(self.g as f32 + (other.g as f32 - self.g as f32) * t) as u8,
            b: libm::roundf(self.b as f32 + (other.b as f32 - self.b as f32) * t) as u8,
            a: libm::roundf(self.a as f32 + (other.a as f32 - self.a as f32) * t) as u8,
        }
    }

    pub const fn has_alpha(&self) -> bool {
        self.a != Self::ALPHA_OPAQUE
    }

    pub fn to_hash(&self) -> String {
        format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
    }

    pub fn write_hash(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "#{:02x}{:02x}{:02x}{:02x}",
            self.r, self.g, self.b, self.a
        )
    }
}

impl Default for ColorF {
    fn default() -> Self {
        ColorF::BLACK
    }
}

impl fmt::Display for ColorF {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "rgba({}, {}, {}, {})",
            self.r * 255.0,
            self.g * 255.0,
            self.b * 255.0,
            self.a
        )
    }
}

impl ColorF {
    pub const ALPHA_TRANSPARENT: f32 = 0.0;
    pub const ALPHA_OPAQUE: f32 = 1.0;

    pub const WHITE: ColorF = ColorF {
        r: 1.0,
        g: 1.0,
        b: 1.0,
        a: Self::ALPHA_OPAQUE,
    };

    pub const BLACK: ColorF = ColorF {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Self::ALPHA_OPAQUE,
    };

    pub const TRANSPARENT: ColorF = ColorF {
        r: 0.0,
        g: 0.0,
        b: 0.0,
        a: Self::ALPHA_TRANSPARENT,
    };
}

impl From<ColorU> for ColorF {
    fn from(input: ColorU) -> ColorF {
        ColorF {
            r: (input.r as f32) / 255.0,
            g: (input.g as f32) / 255.0,
            b: (input.b as f32) / 255.0,
            a: (input.a as f32) / 255.0,
        }
    }
}

impl From<ColorF> for ColorU {
    fn from(input: ColorF) -> ColorU {
        ColorU {
            r: (input.r.min(1.0) * 255.0) as u8,
            g: (input.g.min(1.0) * 255.0) as u8,
            b: (input.b.min(1.0) * 255.0) as u8,
            a: (input.a.min(1.0) * 255.0) as u8,
        }
    }
}

impl FormatAsCssValue for ColorU {
    fn format_as_css_value(&self) -> String {
        if self.a == 255 {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, self.a)
        }
    }
}

impl FormatAsCssValue for ColorF {
    fn format_as_css_value(&self) -> String {
        if self.a == 1.0 {
            format!(
                "rgb({}, {}, {})",
                (self.r * 255.0) as u8,
                (self.g * 255.0) as u8,
                (self.b * 255.0) as u8
            )
        } else {
            format!(
                "rgba({}, {}, {}, {})",
                (self.r * 255.0) as u8,
                (self.g * 255.0) as u8,
                (self.b * 255.0) as u8,
                self.a
            )
        }
    }
}

// Color parsing functions
pub fn parse_css_color<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    if input.starts_with('#') {
        parse_color_no_hash(&input[1..])
    } else {
        // Try parsing as function notation (rgb, rgba, hsl, hsla)
        if let Some(open_paren) = input.find('(') {
            let function_name = &input[..open_paren].trim();
            let close_paren = input
                .rfind(')')
                .ok_or(CssColorParseError::MissingParentheses(input))?;
            let inner_value = &input[open_paren + 1..close_paren].trim();

            match *function_name {
                "rgba" => parse_color_rgb(inner_value, true),
                "rgb" => parse_color_rgb(inner_value, false),
                "hsla" => parse_color_hsl(inner_value, true),
                "hsl" => parse_color_hsl(inner_value, false),
                _ => Err(CssColorParseError::InvalidColorName(input)),
            }
        } else {
            // Try parsing as named color
            parse_color_builtin(input)
        }
    }
}

/// Parse a built-in background color (CSS color names)
pub fn parse_color_builtin<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    let (r, g, b, a) = match input {
        "AliceBlue" | "aliceblue" => (240, 248, 255, 255),
        "AntiqueWhite" | "antiquewhite" => (250, 235, 215, 255),
        "Aqua" | "aqua" => (0, 255, 255, 255),
        "Aquamarine" | "aquamarine" => (127, 255, 212, 255),
        "Azure" | "azure" => (240, 255, 255, 255),
        "Beige" | "beige" => (245, 245, 220, 255),
        "Bisque" | "bisque" => (255, 228, 196, 255),
        "Black" | "black" => (0, 0, 0, 255),
        "BlanchedAlmond" | "blanchedalmond" => (255, 235, 205, 255),
        "Blue" | "blue" => (0, 0, 255, 255),
        "BlueViolet" | "blueviolet" => (138, 43, 226, 255),
        "Brown" | "brown" => (165, 42, 42, 255),
        "BurlyWood" | "burlywood" => (222, 184, 135, 255),
        "CadetBlue" | "cadetblue" => (95, 158, 160, 255),
        "Chartreuse" | "chartreuse" => (127, 255, 0, 255),
        "Chocolate" | "chocolate" => (210, 105, 30, 255),
        "Coral" | "coral" => (255, 127, 80, 255),
        "CornflowerBlue" | "cornflowerblue" => (100, 149, 237, 255),
        "Cornsilk" | "cornsilk" => (255, 248, 220, 255),
        "Crimson" | "crimson" => (220, 20, 60, 255),
        "Cyan" | "cyan" => (0, 255, 255, 255),
        "DarkBlue" | "darkblue" => (0, 0, 139, 255),
        "DarkCyan" | "darkcyan" => (0, 139, 139, 255),
        "DarkGoldenRod" | "darkgoldenrod" => (184, 134, 11, 255),
        "DarkGray" | "darkgray" => (169, 169, 169, 255),
        "DarkGrey" | "darkgrey" => (169, 169, 169, 255),
        "DarkGreen" | "darkgreen" => (0, 100, 0, 255),
        "DarkKhaki" | "darkkhaki" => (189, 183, 107, 255),
        "DarkMagenta" | "darkmagenta" => (139, 0, 139, 255),
        "DarkOliveGreen" | "darkolivegreen" => (85, 107, 47, 255),
        "DarkOrange" | "darkorange" => (255, 140, 0, 255),
        "DarkOrchid" | "darkorchid" => (153, 50, 204, 255),
        "DarkRed" | "darkred" => (139, 0, 0, 255),
        "DarkSalmon" | "darksalmon" => (233, 150, 122, 255),
        "DarkSeaGreen" | "darkseagreen" => (143, 188, 143, 255),
        "DarkSlateBlue" | "darkslateblue" => (72, 61, 139, 255),
        "DarkSlateGray" | "darkslategray" => (47, 79, 79, 255),
        "DarkSlateGrey" | "darkslategrey" => (47, 79, 79, 255),
        "DarkTurquoise" | "darkturquoise" => (0, 206, 209, 255),
        "DarkViolet" | "darkviolet" => (148, 0, 211, 255),
        "DeepPink" | "deeppink" => (255, 20, 147, 255),
        "DeepSkyBlue" | "deepskyblue" => (0, 191, 255, 255),
        "DimGray" | "dimgray" => (105, 105, 105, 255),
        "DimGrey" | "dimgrey" => (105, 105, 105, 255),
        "DodgerBlue" | "dodgerblue" => (30, 144, 255, 255),
        "FireBrick" | "firebrick" => (178, 34, 34, 255),
        "FloralWhite" | "floralwhite" => (255, 250, 240, 255),
        "ForestGreen" | "forestgreen" => (34, 139, 34, 255),
        "Fuchsia" | "fuchsia" => (255, 0, 255, 255),
        "Gainsboro" | "gainsboro" => (220, 220, 220, 255),
        "GhostWhite" | "ghostwhite" => (248, 248, 255, 255),
        "Gold" | "gold" => (255, 215, 0, 255),
        "GoldenRod" | "goldenrod" => (218, 165, 32, 255),
        "Gray" | "gray" | "Grey" | "grey" => (128, 128, 128, 255),
        "Green" | "green" => (0, 128, 0, 255),
        "GreenYellow" | "greenyellow" => (173, 255, 47, 255),
        "HoneyDew" | "honeydew" => (240, 255, 240, 255),
        "HotPink" | "hotpink" => (255, 105, 180, 255),
        "IndianRed" | "indianred" => (205, 92, 92, 255),
        "Indigo" | "indigo" => (75, 0, 130, 255),
        "Ivory" | "ivory" => (255, 255, 240, 255),
        "Khaki" | "khaki" => (240, 230, 140, 255),
        "Lavender" | "lavender" => (230, 230, 250, 255),
        "LavenderBlush" | "lavenderblush" => (255, 240, 245, 255),
        "LawnGreen" | "lawngreen" => (124, 252, 0, 255),
        "LemonChiffon" | "lemonchiffon" => (255, 250, 205, 255),
        "LightBlue" | "lightblue" => (173, 216, 230, 255),
        "LightCoral" | "lightcoral" => (240, 128, 128, 255),
        "LightCyan" | "lightcyan" => (224, 255, 255, 255),
        "LightGoldenRodYellow" | "lightgoldenrodyellow" => (250, 250, 210, 255),
        "LightGray" | "lightgray" | "LightGrey" | "lightgrey" => (211, 211, 211, 255),
        "LightGreen" | "lightgreen" => (144, 238, 144, 255),
        "LightPink" | "lightpink" => (255, 182, 193, 255),
        "LightSalmon" | "lightsalmon" => (255, 160, 122, 255),
        "LightSeaGreen" | "lightseagreen" => (32, 178, 170, 255),
        "LightSkyBlue" | "lightskyblue" => (135, 206, 250, 255),
        "LightSlateGray" | "lightslategray" | "LightSlateGrey" | "lightslategrey" => {
            (119, 136, 153, 255)
        }
        "LightSteelBlue" | "lightsteelblue" => (176, 196, 222, 255),
        "LightYellow" | "lightyellow" => (255, 255, 224, 255),
        "Lime" | "lime" => (0, 255, 0, 255),
        "LimeGreen" | "limegreen" => (50, 205, 50, 255),
        "Linen" | "linen" => (250, 240, 230, 255),
        "Magenta" | "magenta" => (255, 0, 255, 255),
        "Maroon" | "maroon" => (128, 0, 0, 255),
        "MediumAquaMarine" | "mediumaquamarine" => (102, 205, 170, 255),
        "MediumBlue" | "mediumblue" => (0, 0, 205, 255),
        "MediumOrchid" | "mediumorchid" => (186, 85, 211, 255),
        "MediumPurple" | "mediumpurple" => (147, 112, 219, 255),
        "MediumSeaGreen" | "mediumseagreen" => (60, 179, 113, 255),
        "MediumSlateBlue" | "mediumslateblue" => (123, 104, 238, 255),
        "MediumSpringGreen" | "mediumspringgreen" => (0, 250, 154, 255),
        "MediumTurquoise" | "mediumturquoise" => (72, 209, 204, 255),
        "MediumVioletRed" | "mediumvioletred" => (199, 21, 133, 255),
        "MidnightBlue" | "midnightblue" => (25, 25, 112, 255),
        "MintCream" | "mintcream" => (245, 255, 250, 255),
        "MistyRose" | "mistyrose" => (255, 228, 225, 255),
        "Moccasin" | "moccasin" => (255, 228, 181, 255),
        "NavajoWhite" | "navajowhite" => (255, 222, 173, 255),
        "Navy" | "navy" => (0, 0, 128, 255),
        "OldLace" | "oldlace" => (253, 245, 230, 255),
        "Olive" | "olive" => (128, 128, 0, 255),
        "OliveDrab" | "olivedrab" => (107, 142, 35, 255),
        "Orange" | "orange" => (255, 165, 0, 255),
        "OrangeRed" | "orangered" => (255, 69, 0, 255),
        "Orchid" | "orchid" => (218, 112, 214, 255),
        "PaleGoldenRod" | "palegoldenrod" => (238, 232, 170, 255),
        "PaleGreen" | "palegreen" => (152, 251, 152, 255),
        "PaleTurquoise" | "paleturquoise" => (175, 238, 238, 255),
        "PaleVioletRed" | "palevioletred" => (219, 112, 147, 255),
        "PapayaWhip" | "papayawhip" => (255, 239, 213, 255),
        "PeachPuff" | "peachpuff" => (255, 218, 185, 255),
        "Peru" | "peru" => (205, 133, 63, 255),
        "Pink" | "pink" => (255, 192, 203, 255),
        "Plum" | "plum" => (221, 160, 221, 255),
        "PowderBlue" | "powderblue" => (176, 224, 230, 255),
        "Purple" | "purple" => (128, 0, 128, 255),
        "Red" | "red" => (255, 0, 0, 255),
        "RosyBrown" | "rosybrown" => (188, 143, 143, 255),
        "RoyalBlue" | "royalblue" => (65, 105, 225, 255),
        "SaddleBrown" | "saddlebrown" => (139, 69, 19, 255),
        "Salmon" | "salmon" => (250, 128, 114, 255),
        "SandyBrown" | "sandybrown" => (244, 164, 96, 255),
        "SeaGreen" | "seagreen" => (46, 139, 87, 255),
        "SeaShell" | "seashell" => (255, 245, 238, 255),
        "Sienna" | "sienna" => (160, 82, 45, 255),
        "Silver" | "silver" => (192, 192, 192, 255),
        "SkyBlue" | "skyblue" => (135, 206, 235, 255),
        "SlateBlue" | "slateblue" => (106, 90, 205, 255),
        "SlateGray" | "slategray" | "SlateGrey" | "slategrey" => (112, 128, 144, 255),
        "Snow" | "snow" => (255, 250, 250, 255),
        "SpringGreen" | "springgreen" => (0, 255, 127, 255),
        "SteelBlue" | "steelblue" => (70, 130, 180, 255),
        "Tan" | "tan" => (210, 180, 140, 255),
        "Teal" | "teal" => (0, 128, 128, 255),
        "Thistle" | "thistle" => (216, 191, 216, 255),
        "Tomato" | "tomato" => (255, 99, 71, 255),
        "Turquoise" | "turquoise" => (64, 224, 208, 255),
        "Violet" | "violet" => (238, 130, 238, 255),
        "Wheat" | "wheat" => (245, 222, 179, 255),
        "White" | "white" => (255, 255, 255, 255),
        "WhiteSmoke" | "whitesmoke" => (245, 245, 245, 255),
        "Yellow" | "yellow" => (255, 255, 0, 255),
        "YellowGreen" | "yellowgreen" => (154, 205, 50, 255),
        "transparent" => (0, 0, 0, 0),
        _ => return Err(CssColorParseError::InvalidColorName(input)),
    };

    Ok(ColorU { r, g, b, a })
}

/// Parse RGB/RGBA color function
pub fn parse_color_rgb<'a>(
    input: &'a str,
    has_alpha: bool,
) -> Result<ColorU, CssColorParseError<'a>> {
    let components: Vec<&str> = input.split(',').map(|s| s.trim()).collect();
    let expected_components = if has_alpha { 4 } else { 3 };

    if components.len() != expected_components {
        return Err(CssColorParseError::InvalidArgumentCount(input));
    }

    let r = parse_color_component(components[0])? as u8;
    let g = parse_color_component(components[1])? as u8;
    let b = parse_color_component(components[2])? as u8;
    let a = if has_alpha {
        let alpha_val = components[3]
            .parse::<f32>()
            .map_err(|_| CssColorParseError::InvalidRgbColor(input))?;
        (alpha_val.clamp(0.0, 1.0) * 255.0) as u8
    } else {
        255
    };

    Ok(ColorU { r, g, b, a })
}

/// Parse HSL/HSLA color function
pub fn parse_color_hsl<'a>(
    input: &'a str,
    has_alpha: bool,
) -> Result<ColorU, CssColorParseError<'a>> {
    let components: Vec<&str> = input.split(',').map(|s| s.trim()).collect();
    let expected_components = if has_alpha { 4 } else { 3 };

    if components.len() != expected_components {
        return Err(CssColorParseError::InvalidArgumentCount(input));
    }

    let h = components[0]
        .trim_end_matches("deg")
        .parse::<f32>()
        .map_err(|_| CssColorParseError::InvalidHslColor(input))?;
    let s = components[1]
        .trim_end_matches('%')
        .parse::<f32>()
        .map_err(|_| CssColorParseError::InvalidHslColor(input))?
        / 100.0;
    let l = components[2]
        .trim_end_matches('%')
        .parse::<f32>()
        .map_err(|_| CssColorParseError::InvalidHslColor(input))?
        / 100.0;
    let a = if has_alpha {
        components[3]
            .parse::<f32>()
            .map_err(|_| CssColorParseError::InvalidHslColor(input))?
            .clamp(0.0, 1.0)
    } else {
        1.0
    };

    let (r, g, b) = hsl_to_rgb(h / 360.0, s, l);
    Ok(ColorU {
        r: (r * 255.0) as u8,
        g: (g * 255.0) as u8,
        b: (b * 255.0) as u8,
        a: (a * 255.0) as u8,
    })
}

/// Parse hex color without the # prefix
pub fn parse_color_no_hash<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    let len = input.len();
    match len {
        3 => {
            // RGB format: "abc" -> "aabbcc"
            let r = u8::from_str_radix(&input[0..1].repeat(2), 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let g = u8::from_str_radix(&input[1..2].repeat(2), 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let b = u8::from_str_radix(&input[2..3].repeat(2), 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            Ok(ColorU { r, g, b, a: 255 })
        }
        6 => {
            // RRGGBB format
            let r = u8::from_str_radix(&input[0..2], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let g = u8::from_str_radix(&input[2..4], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let b = u8::from_str_radix(&input[4..6], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            Ok(ColorU { r, g, b, a: 255 })
        }
        8 => {
            // RRGGBBAA format
            let r = u8::from_str_radix(&input[0..2], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let g = u8::from_str_radix(&input[2..4], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let b = u8::from_str_radix(&input[4..6], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            let a = u8::from_str_radix(&input[6..8], 16)
                .map_err(|_| CssColorParseError::InvalidHexColor(input))?;
            Ok(ColorU { r, g, b, a })
        }
        _ => Err(CssColorParseError::InvalidHexColor(input)),
    }
}

// Helper functions

fn parse_color_component(component: &str) -> Result<f32, CssColorParseError<'_>> {
    if component.ends_with('%') {
        let val = component
            .trim_end_matches('%')
            .parse::<f32>()
            .map_err(|_| CssColorParseError::InvalidRgbColor(component))?;
        Ok(val * 2.55) // Convert percentage to 0-255 range
    } else {
        component
            .parse::<f32>()
            .map_err(|_| CssColorParseError::InvalidRgbColor(component))
    }
}

fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r, g, b) = match (h * 6.0) as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    (r + m, g + m, b + m)
}

/// Formats a ColorU in hex format
pub fn css_color_to_string(color: ColorU, prefix_hash: bool) -> String {
    let prefix = if prefix_hash { "#" } else { "" };
    let alpha = if color.a == 255 {
        String::new()
    } else {
        format!("{:02x}", color.a)
    };
    format!(
        "{}{:02x}{:02x}{:02x}{}",
        prefix, color.r, color.g, color.b, alpha
    )
}

//! CSS property types for color.

use alloc::string::{String, ToString};
use core::{
    fmt,
    num::{ParseFloatError, ParseIntError},
};

use crate::{
    impl_option,
    props::basic::{
        direction::{
            parse_direction, CssDirectionParseError, CssDirectionParseErrorOwned, Direction,
        },
        length::{PercentageParseError, PercentageValue},
    },
};

/// u8-based color, range 0 to 255 (similar to webrenders ColorU)
#[derive(Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash)]
#[repr(C)]
pub struct ColorU {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl_option!(
    ColorU,
    OptionColorU,
    [Debug, Copy, Clone, PartialEq, Ord, PartialOrd, Eq, Hash]
);

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

    pub const fn new(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
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
}

/// f32-based color, range 0.0 to 1.0 (similar to webrenders ColorF)
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd)]
pub struct ColorF {
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
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

// --- PARSER ---

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum CssColorComponent {
    Red,
    Green,
    Blue,
    Hue,
    Saturation,
    Lightness,
    Alpha,
}

#[derive(Clone, PartialEq)]
pub enum CssColorParseError<'a> {
    InvalidColor(&'a str),
    InvalidFunctionName(&'a str),
    InvalidColorComponent(u8),
    IntValueParseErr(ParseIntError),
    FloatValueParseErr(ParseFloatError),
    FloatValueOutOfRange(f32),
    MissingColorComponent(CssColorComponent),
    ExtraArguments(&'a str),
    UnclosedColor(&'a str),
    EmptyInput,
    DirectionParseError(CssDirectionParseError<'a>),
    UnsupportedDirection(&'a str),
    InvalidPercentage(PercentageParseError),
}

impl_debug_as_display!(CssColorParseError<'a>);
impl_display! {CssColorParseError<'a>, {
    InvalidColor(i) => format!("Invalid CSS color: \"{}\"", i),
    InvalidFunctionName(i) => format!("Invalid function name, expected one of: \"rgb\", \"rgba\", \"hsl\", \"hsla\" got: \"{}\"", i),
    InvalidColorComponent(i) => format!("Invalid color component when parsing CSS color: \"{}\"", i),
    IntValueParseErr(e) => format!("CSS color component: Value not in range between 00 - FF: \"{}\"", e),
    FloatValueParseErr(e) => format!("CSS color component: Value cannot be parsed as floating point number: \"{}\"", e),
    FloatValueOutOfRange(v) => format!("CSS color component: Value not in range between 0.0 - 1.0: \"{}\"", v),
    MissingColorComponent(c) => format!("CSS color is missing {:?} component", c),
    ExtraArguments(a) => format!("Extra argument to CSS color: \"{}\"", a),
    EmptyInput => format!("Empty color string."),
    UnclosedColor(i) => format!("Unclosed color: \"{}\"", i),
    DirectionParseError(e) => format!("Could not parse direction argument for CSS color: \"{}\"", e),
    UnsupportedDirection(d) => format!("Unsupported direction type for CSS color: \"{}\"", d),
    InvalidPercentage(p) => format!("Invalid percentage when parsing CSS color: \"{}\"", p),
}}

impl<'a> From<ParseIntError> for CssColorParseError<'a> {
    fn from(e: ParseIntError) -> Self {
        CssColorParseError::IntValueParseErr(e)
    }
}
impl<'a> From<ParseFloatError> for CssColorParseError<'a> {
    fn from(e: ParseFloatError) -> Self {
        CssColorParseError::FloatValueParseErr(e)
    }
}
impl_from!(
    CssDirectionParseError<'a>,
    CssColorParseError::DirectionParseError
);

#[derive(Debug, Clone, PartialEq)]
pub enum CssColorParseErrorOwned {
    InvalidColor(String),
    InvalidFunctionName(String),
    InvalidColorComponent(u8),
    IntValueParseErr(ParseIntError),
    FloatValueParseErr(ParseFloatError),
    FloatValueOutOfRange(f32),
    MissingColorComponent(CssColorComponent),
    ExtraArguments(String),
    UnclosedColor(String),
    EmptyInput,
    DirectionParseError(CssDirectionParseErrorOwned),
    UnsupportedDirection(String),
    InvalidPercentage(PercentageParseError),
}

impl<'a> CssColorParseError<'a> {
    pub fn to_contained(&self) -> CssColorParseErrorOwned {
        match self {
            CssColorParseError::InvalidColor(s) => {
                CssColorParseErrorOwned::InvalidColor(s.to_string())
            }
            CssColorParseError::InvalidFunctionName(s) => {
                CssColorParseErrorOwned::InvalidFunctionName(s.to_string())
            }
            CssColorParseError::InvalidColorComponent(n) => {
                CssColorParseErrorOwned::InvalidColorComponent(*n)
            }
            CssColorParseError::IntValueParseErr(e) => {
                CssColorParseErrorOwned::IntValueParseErr(e.clone())
            }
            CssColorParseError::FloatValueParseErr(e) => {
                CssColorParseErrorOwned::FloatValueParseErr(e.clone())
            }
            CssColorParseError::FloatValueOutOfRange(n) => {
                CssColorParseErrorOwned::FloatValueOutOfRange(*n)
            }
            CssColorParseError::MissingColorComponent(c) => {
                CssColorParseErrorOwned::MissingColorComponent(*c)
            }
            CssColorParseError::ExtraArguments(s) => {
                CssColorParseErrorOwned::ExtraArguments(s.to_string())
            }
            CssColorParseError::UnclosedColor(s) => {
                CssColorParseErrorOwned::UnclosedColor(s.to_string())
            }
            CssColorParseError::EmptyInput => CssColorParseErrorOwned::EmptyInput,
            CssColorParseError::DirectionParseError(e) => {
                CssColorParseErrorOwned::DirectionParseError(e.to_contained())
            }
            CssColorParseError::UnsupportedDirection(s) => {
                CssColorParseErrorOwned::UnsupportedDirection(s.to_string())
            }
            CssColorParseError::InvalidPercentage(e) => {
                CssColorParseErrorOwned::InvalidPercentage(e.clone())
            }
        }
    }
}

impl CssColorParseErrorOwned {
    pub fn to_shared<'a>(&'a self) -> CssColorParseError<'a> {
        match self {
            CssColorParseErrorOwned::InvalidColor(s) => CssColorParseError::InvalidColor(s),
            CssColorParseErrorOwned::InvalidFunctionName(s) => {
                CssColorParseError::InvalidFunctionName(s)
            }
            CssColorParseErrorOwned::InvalidColorComponent(n) => {
                CssColorParseError::InvalidColorComponent(*n)
            }
            CssColorParseErrorOwned::IntValueParseErr(e) => {
                CssColorParseError::IntValueParseErr(e.clone())
            }
            CssColorParseErrorOwned::FloatValueParseErr(e) => {
                CssColorParseError::FloatValueParseErr(e.clone())
            }
            CssColorParseErrorOwned::FloatValueOutOfRange(n) => {
                CssColorParseError::FloatValueOutOfRange(*n)
            }
            CssColorParseErrorOwned::MissingColorComponent(c) => {
                CssColorParseError::MissingColorComponent(*c)
            }
            CssColorParseErrorOwned::ExtraArguments(s) => CssColorParseError::ExtraArguments(s),
            CssColorParseErrorOwned::UnclosedColor(s) => CssColorParseError::UnclosedColor(s),
            CssColorParseErrorOwned::EmptyInput => CssColorParseError::EmptyInput,
            CssColorParseErrorOwned::DirectionParseError(e) => {
                CssColorParseError::DirectionParseError(e.to_shared())
            }
            CssColorParseErrorOwned::UnsupportedDirection(s) => {
                CssColorParseError::UnsupportedDirection(s)
            }
            CssColorParseErrorOwned::InvalidPercentage(e) => {
                CssColorParseError::InvalidPercentage(e.clone())
            }
        }
    }
}

#[cfg(feature = "parser")]
pub fn parse_css_color<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    let input = input.trim();
    if input.starts_with('#') {
        parse_color_no_hash(&input[1..])
    } else {
        use crate::props::basic::parse::{parse_parentheses, ParenthesisParseError};
        match parse_parentheses(input, &["rgba", "rgb", "hsla", "hsl"]) {
            Ok((stopword, inner_value)) => match stopword {
                "rgba" => parse_color_rgb(inner_value, true),
                "rgb" => parse_color_rgb(inner_value, false),
                "hsla" => parse_color_hsl(inner_value, true),
                "hsl" => parse_color_hsl(inner_value, false),
                _ => unreachable!(),
            },
            Err(e) => match e {
                ParenthesisParseError::UnclosedBraces => {
                    Err(CssColorParseError::UnclosedColor(input))
                }
                ParenthesisParseError::EmptyInput => Err(CssColorParseError::EmptyInput),
                ParenthesisParseError::StopWordNotFound(stopword) => {
                    Err(CssColorParseError::InvalidFunctionName(stopword))
                }
                ParenthesisParseError::NoClosingBraceFound => {
                    Err(CssColorParseError::UnclosedColor(input))
                }
                ParenthesisParseError::NoOpeningBraceFound => parse_color_builtin(input),
            },
        }
    }
}

#[cfg(feature = "parser")]
fn parse_color_no_hash<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn from_hex<'a>(c: u8) -> Result<u8, CssColorParseError<'a>> {
        match c {
            b'0'..=b'9' => Ok(c - b'0'),
            b'a'..=b'f' => Ok(c - b'a' + 10),
            b'A'..=b'F' => Ok(c - b'A' + 10),
            _ => Err(CssColorParseError::InvalidColorComponent(c)),
        }
    }

    match input.len() {
        3 => {
            let mut bytes = input.bytes();
            let r = bytes.next().unwrap();
            let g = bytes.next().unwrap();
            let b = bytes.next().unwrap();
            Ok(ColorU::new_rgb(
                from_hex(r)? * 17,
                from_hex(g)? * 17,
                from_hex(b)? * 17,
            ))
        }
        4 => {
            let mut bytes = input.bytes();
            let r = bytes.next().unwrap();
            let g = bytes.next().unwrap();
            let b = bytes.next().unwrap();
            let a = bytes.next().unwrap();
            Ok(ColorU::new(
                from_hex(r)? * 17,
                from_hex(g)? * 17,
                from_hex(b)? * 17,
                from_hex(a)? * 17,
            ))
        }
        6 => {
            let val = u32::from_str_radix(input, 16)?;
            Ok(ColorU::new_rgb(
                ((val >> 16) & 0xFF) as u8,
                ((val >> 8) & 0xFF) as u8,
                (val & 0xFF) as u8,
            ))
        }
        8 => {
            let val = u32::from_str_radix(input, 16)?;
            Ok(ColorU::new(
                ((val >> 24) & 0xFF) as u8,
                ((val >> 16) & 0xFF) as u8,
                ((val >> 8) & 0xFF) as u8,
                (val & 0xFF) as u8,
            ))
        }
        _ => Err(CssColorParseError::InvalidColor(input)),
    }
}

#[cfg(feature = "parser")]
fn parse_color_rgb<'a>(
    input: &'a str,
    parse_alpha: bool,
) -> Result<ColorU, CssColorParseError<'a>> {
    let mut components = input.split(',').map(|c| c.trim());
    let rgb_color = parse_color_rgb_components(&mut components)?;
    let a = if parse_alpha {
        parse_alpha_component(&mut components)?
    } else {
        255
    };
    if let Some(arg) = components.next() {
        return Err(CssColorParseError::ExtraArguments(arg));
    }
    Ok(ColorU { a, ..rgb_color })
}

#[cfg(feature = "parser")]
fn parse_color_rgb_components<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn component_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<u8, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        Ok(c.parse::<u8>()?)
    }
    Ok(ColorU {
        r: component_from_str(components, CssColorComponent::Red)?,
        g: component_from_str(components, CssColorComponent::Green)?,
        b: component_from_str(components, CssColorComponent::Blue)?,
        a: 255,
    })
}

#[cfg(feature = "parser")]
fn parse_color_hsl<'a>(
    input: &'a str,
    parse_alpha: bool,
) -> Result<ColorU, CssColorParseError<'a>> {
    let mut components = input.split(',').map(|c| c.trim());
    let rgb_color = parse_color_hsl_components(&mut components)?;
    let a = if parse_alpha {
        parse_alpha_component(&mut components)?
    } else {
        255
    };
    if let Some(arg) = components.next() {
        return Err(CssColorParseError::ExtraArguments(arg));
    }
    Ok(ColorU { a, ..rgb_color })
}

#[cfg(feature = "parser")]
fn parse_color_hsl_components<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<ColorU, CssColorParseError<'a>> {
    #[inline]
    fn angle_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<f32, CssColorParseError<'a>> {
        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }
        let dir = parse_direction(c)?;
        match dir {
            Direction::Angle(deg) => Ok(deg.to_degrees()),
            Direction::FromTo(_) => Err(CssColorParseError::UnsupportedDirection(c)),
        }
    }

    #[inline]
    fn percent_from_str<'a>(
        components: &mut dyn Iterator<Item = &'a str>,
        which: CssColorComponent,
    ) -> Result<f32, CssColorParseError<'a>> {
        use crate::props::basic::parse_percentage_value;

        let c = components
            .next()
            .ok_or(CssColorParseError::MissingColorComponent(which))?;
        if c.is_empty() {
            return Err(CssColorParseError::MissingColorComponent(which));
        }

        // Modern CSS allows both percentage and unitless values for HSL
        Ok(parse_percentage_value(c)
            .map_err(CssColorParseError::InvalidPercentage)?
            .normalized()
            * 100.0)
    }

    #[inline]
    fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
        let s = s / 100.0;
        let l = l / 100.0;
        let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
        let h_prime = h / 60.0;
        let x = c * (1.0 - ((h_prime % 2.0) - 1.0).abs());
        let (r1, g1, b1) = if h_prime >= 0.0 && h_prime < 1.0 {
            (c, x, 0.0)
        } else if h_prime >= 1.0 && h_prime < 2.0 {
            (x, c, 0.0)
        } else if h_prime >= 2.0 && h_prime < 3.0 {
            (0.0, c, x)
        } else if h_prime >= 3.0 && h_prime < 4.0 {
            (0.0, x, c)
        } else if h_prime >= 4.0 && h_prime < 5.0 {
            (x, 0.0, c)
        } else {
            (c, 0.0, x)
        };
        let m = l - c / 2.0;
        (
            ((r1 + m) * 255.0) as u8,
            ((g1 + m) * 255.0) as u8,
            ((b1 + m) * 255.0) as u8,
        )
    }

    let (h, s, l) = (
        angle_from_str(components, CssColorComponent::Hue)?,
        percent_from_str(components, CssColorComponent::Saturation)?,
        percent_from_str(components, CssColorComponent::Lightness)?,
    );

    let (r, g, b) = hsl_to_rgb(h, s, l);
    Ok(ColorU { r, g, b, a: 255 })
}

#[cfg(feature = "parser")]
fn parse_alpha_component<'a>(
    components: &mut dyn Iterator<Item = &'a str>,
) -> Result<u8, CssColorParseError<'a>> {
    let a_str = components
        .next()
        .ok_or(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha,
        ))?;
    if a_str.is_empty() {
        return Err(CssColorParseError::MissingColorComponent(
            CssColorComponent::Alpha,
        ));
    }
    let a = a_str.parse::<f32>()?;
    if a < 0.0 || a > 1.0 {
        return Err(CssColorParseError::FloatValueOutOfRange(a));
    }
    Ok((a * 255.0).round() as u8)
}

#[cfg(feature = "parser")]
fn parse_color_builtin<'a>(input: &'a str) -> Result<ColorU, CssColorParseError<'a>> {
    let (r, g, b, a) = match input.to_lowercase().as_str() {
        "aliceblue" => (240, 248, 255, 255),
        "antiquewhite" => (250, 235, 215, 255),
        "aqua" => (0, 255, 255, 255),
        "aquamarine" => (127, 255, 212, 255),
        "azure" => (240, 255, 255, 255),
        "beige" => (245, 245, 220, 255),
        "bisque" => (255, 228, 196, 255),
        "black" => (0, 0, 0, 255),
        "blanchedalmond" => (255, 235, 205, 255),
        "blue" => (0, 0, 255, 255),
        "blueviolet" => (138, 43, 226, 255),
        "brown" => (165, 42, 42, 255),
        "burlywood" => (222, 184, 135, 255),
        "cadetblue" => (95, 158, 160, 255),
        "chartreuse" => (127, 255, 0, 255),
        "chocolate" => (210, 105, 30, 255),
        "coral" => (255, 127, 80, 255),
        "cornflowerblue" => (100, 149, 237, 255),
        "cornsilk" => (255, 248, 220, 255),
        "crimson" => (220, 20, 60, 255),
        "cyan" => (0, 255, 255, 255),
        "darkblue" => (0, 0, 139, 255),
        "darkcyan" => (0, 139, 139, 255),
        "darkgoldenrod" => (184, 134, 11, 255),
        "darkgray" | "darkgrey" => (169, 169, 169, 255),
        "darkgreen" => (0, 100, 0, 255),
        "darkkhaki" => (189, 183, 107, 255),
        "darkmagenta" => (139, 0, 139, 255),
        "darkolivegreen" => (85, 107, 47, 255),
        "darkorange" => (255, 140, 0, 255),
        "darkorchid" => (153, 50, 204, 255),
        "darkred" => (139, 0, 0, 255),
        "darksalmon" => (233, 150, 122, 255),
        "darkseagreen" => (143, 188, 143, 255),
        "darkslateblue" => (72, 61, 139, 255),
        "darkslategray" | "darkslategrey" => (47, 79, 79, 255),
        "darkturquoise" => (0, 206, 209, 255),
        "darkviolet" => (148, 0, 211, 255),
        "deeppink" => (255, 20, 147, 255),
        "deepskyblue" => (0, 191, 255, 255),
        "dimgray" | "dimgrey" => (105, 105, 105, 255),
        "dodgerblue" => (30, 144, 255, 255),
        "firebrick" => (178, 34, 34, 255),
        "floralwhite" => (255, 250, 240, 255),
        "forestgreen" => (34, 139, 34, 255),
        "fuchsia" => (255, 0, 255, 255),
        "gainsboro" => (220, 220, 220, 255),
        "ghostwhite" => (248, 248, 255, 255),
        "gold" => (255, 215, 0, 255),
        "goldenrod" => (218, 165, 32, 255),
        "gray" | "grey" => (128, 128, 128, 255),
        "green" => (0, 128, 0, 255),
        "greenyellow" => (173, 255, 47, 255),
        "honeydew" => (240, 255, 240, 255),
        "hotpink" => (255, 105, 180, 255),
        "indianred" => (205, 92, 92, 255),
        "indigo" => (75, 0, 130, 255),
        "ivory" => (255, 255, 240, 255),
        "khaki" => (240, 230, 140, 255),
        "lavender" => (230, 230, 250, 255),
        "lavenderblush" => (255, 240, 245, 255),
        "lawngreen" => (124, 252, 0, 255),
        "lemonchiffon" => (255, 250, 205, 255),
        "lightblue" => (173, 216, 230, 255),
        "lightcoral" => (240, 128, 128, 255),
        "lightcyan" => (224, 255, 255, 255),
        "lightgoldenrodyellow" => (250, 250, 210, 255),
        "lightgray" | "lightgrey" => (211, 211, 211, 255),
        "lightgreen" => (144, 238, 144, 255),
        "lightpink" => (255, 182, 193, 255),
        "lightsalmon" => (255, 160, 122, 255),
        "lightseagreen" => (32, 178, 170, 255),
        "lightskyblue" => (135, 206, 250, 255),
        "lightslategray" | "lightslategrey" => (119, 136, 153, 255),
        "lightsteelblue" => (176, 196, 222, 255),
        "lightyellow" => (255, 255, 224, 255),
        "lime" => (0, 255, 0, 255),
        "limegreen" => (50, 205, 50, 255),
        "linen" => (250, 240, 230, 255),
        "magenta" => (255, 0, 255, 255),
        "maroon" => (128, 0, 0, 255),
        "mediumaquamarine" => (102, 205, 170, 255),
        "mediumblue" => (0, 0, 205, 255),
        "mediumorchid" => (186, 85, 211, 255),
        "mediumpurple" => (147, 112, 219, 255),
        "mediumseagreen" => (60, 179, 113, 255),
        "mediumslateblue" => (123, 104, 238, 255),
        "mediumspringgreen" => (0, 250, 154, 255),
        "mediumturquoise" => (72, 209, 204, 255),
        "mediumvioletred" => (199, 21, 133, 255),
        "midnightblue" => (25, 25, 112, 255),
        "mintcream" => (245, 255, 250, 255),
        "mistyrose" => (255, 228, 225, 255),
        "moccasin" => (255, 228, 181, 255),
        "navajowhite" => (255, 222, 173, 255),
        "navy" => (0, 0, 128, 255),
        "oldlace" => (253, 245, 230, 255),
        "olive" => (128, 128, 0, 255),
        "olivedrab" => (107, 142, 35, 255),
        "orange" => (255, 165, 0, 255),
        "orangered" => (255, 69, 0, 255),
        "orchid" => (218, 112, 214, 255),
        "palegoldenrod" => (238, 232, 170, 255),
        "palegreen" => (152, 251, 152, 255),
        "paleturquoise" => (175, 238, 238, 255),
        "palevioletred" => (219, 112, 147, 255),
        "papayawhip" => (255, 239, 213, 255),
        "peachpuff" => (255, 218, 185, 255),
        "peru" => (205, 133, 63, 255),
        "pink" => (255, 192, 203, 255),
        "plum" => (221, 160, 221, 255),
        "powderblue" => (176, 224, 230, 255),
        "purple" => (128, 0, 128, 255),
        "rebeccapurple" => (102, 51, 153, 255),
        "red" => (255, 0, 0, 255),
        "rosybrown" => (188, 143, 143, 255),
        "royalblue" => (65, 105, 225, 255),
        "saddlebrown" => (139, 69, 19, 255),
        "salmon" => (250, 128, 114, 255),
        "sandybrown" => (244, 164, 96, 255),
        "seagreen" => (46, 139, 87, 255),
        "seashell" => (255, 245, 238, 255),
        "sienna" => (160, 82, 45, 255),
        "silver" => (192, 192, 192, 255),
        "skyblue" => (135, 206, 235, 255),
        "slateblue" => (106, 90, 205, 255),
        "slategray" | "slategrey" => (112, 128, 144, 255),
        "snow" => (255, 250, 250, 255),
        "springgreen" => (0, 255, 127, 255),
        "steelblue" => (70, 130, 180, 255),
        "tan" => (210, 180, 140, 255),
        "teal" => (0, 128, 128, 255),
        "thistle" => (216, 191, 216, 255),
        "tomato" => (255, 99, 71, 255),
        "transparent" => (0, 0, 0, 0),
        "turquoise" => (64, 224, 208, 255),
        "violet" => (238, 130, 238, 255),
        "wheat" => (245, 222, 179, 255),
        "white" => (255, 255, 255, 255),
        "whitesmoke" => (245, 245, 245, 255),
        "yellow" => (255, 255, 0, 255),
        "yellowgreen" => (154, 205, 50, 255),
        _ => return Err(CssColorParseError::InvalidColor(input)),
    };
    Ok(ColorU { r, g, b, a })
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_keywords() {
        assert_eq!(parse_css_color("red").unwrap(), ColorU::RED);
        assert_eq!(parse_css_color("blue").unwrap(), ColorU::BLUE);
        assert_eq!(parse_css_color("transparent").unwrap(), ColorU::TRANSPARENT);
        assert_eq!(
            parse_css_color("rebeccapurple").unwrap(),
            ColorU::new_rgb(102, 51, 153)
        );
    }

    #[test]
    fn test_parse_color_hex() {
        // 3-digit
        assert_eq!(parse_css_color("#f00").unwrap(), ColorU::RED);
        // 4-digit
        assert_eq!(
            parse_css_color("#f008").unwrap(),
            ColorU::new(255, 0, 0, 136)
        );
        // 6-digit
        assert_eq!(parse_css_color("#00ff00").unwrap(), ColorU::GREEN);
        // 8-digit
        assert_eq!(
            parse_css_color("#0000ff80").unwrap(),
            ColorU::new(0, 0, 255, 128)
        );
        // Uppercase
        assert_eq!(
            parse_css_color("#FFC0CB").unwrap(),
            ColorU::new_rgb(255, 192, 203)
        ); // Pink
    }

    #[test]
    fn test_parse_color_rgb() {
        assert_eq!(parse_css_color("rgb(255, 0, 0)").unwrap(), ColorU::RED);
        assert_eq!(
            parse_css_color("rgba(0, 255, 0, 0.5)").unwrap(),
            ColorU::new(0, 255, 0, 128)
        );
        assert_eq!(
            parse_css_color("rgba(10, 20, 30, 1)").unwrap(),
            ColorU::new_rgb(10, 20, 30)
        );
        assert_eq!(parse_css_color("rgb( 0 , 0 , 0 )").unwrap(), ColorU::BLACK);
    }

    #[test]
    fn test_parse_color_hsl() {
        assert_eq!(parse_css_color("hsl(0, 100%, 50%)").unwrap(), ColorU::RED);
        assert_eq!(
            parse_css_color("hsl(120, 100%, 50%)").unwrap(),
            ColorU::GREEN
        );
        assert_eq!(
            parse_css_color("hsla(240, 100%, 50%, 0.5)").unwrap(),
            ColorU::new(0, 0, 255, 128)
        );
        assert_eq!(parse_css_color("hsl(0, 0%, 0%)").unwrap(), ColorU::BLACK);
    }

    #[test]
    fn test_parse_color_errors() {
        assert!(parse_css_color("redd").is_err());
        assert!(parse_css_color("#12345").is_err()); // Invalid length
        assert!(parse_css_color("#ggg").is_err()); // Invalid hex digit
        assert!(parse_css_color("rgb(255, 0)").is_err()); // Missing component
        assert!(parse_css_color("rgba(255, 0, 0, 2)").is_err()); // Alpha out of range
        assert!(parse_css_color("rgb(256, 0, 0)").is_err()); // Value out of range
                                                             // Modern CSS allows both hsl(0, 100%, 50%) and hsl(0 100 50)
        assert!(parse_css_color("hsl(0, 100, 50%)").is_ok()); // Valid in modern CSS
        assert!(parse_css_color("rgb(255 0 0)").is_err()); // Missing commas (this implementation
                                                           // requires commas)
    }
}

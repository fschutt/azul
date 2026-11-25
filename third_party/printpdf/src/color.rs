//! Colors and color spaces.
//!
//! A PDF has no single "color": every color is given in a *color space* that says how many numbers
//! make up a color and what they mean. The device color spaces are the ones used here. DeviceGray
//! takes one number (0 is black, 1 is white). DeviceRGB takes red, green and blue, for screens.
//! DeviceCMYK takes cyan, magenta, yellow and black, for print. Use RGB for documents that are
//! read on screen, CMYK when a print shop asks for it, and grey for black and white content.
//!
//! A color of this crate is a [`Color`], an enum over [`Rgb`], [`Cmyk`], [`Greyscale`] and
//! [`SpotColor`]. All components are `f32` values from 0.0 to 1.0, never 0 to 255. Out-of-range
//! values are reported as a warning when the document is saved (see [`Color::is_out_of_range`]).
//! Set colors with [`Op::SetFillColor`](crate::Op::SetFillColor) (text and filled shapes) and
//! [`Op::SetOutlineColor`](crate::Op::SetOutlineColor) (strokes).
//!
//! An ICC profile is a data file that ties device numbers to real colors. An [`IccProfile`] is a
//! profile plus the metadata needed to embed it. The `icc_profile` field of a color holds an id,
//! but is ignored when the color is written: colors are always written in the device color spaces.
//! An embedded profile is only used as the output intent that PDF/A conformance requires.
//!
//! # Examples
//!
//! ```
//! use printpdf::*;
//!
//! let red = Color::Rgb(Rgb::new(1.0, 0.0, 0.0, None));
//! let op = Op::SetFillColor { col: red };
//! # let _ = op;
//! ```

use serde_derive::{Deserialize, Serialize};

use crate::IccProfileId;

/// Layout of the pixels of an image, with the PDF color space name it maps to.
///
/// Used by `RawImageFormat::get_color_bits_and_space`. It is not used for the colors of
/// vector graphics, see [`Color`] for those.
#[derive(Debug, Copy, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorSpace {
    /// Red, green, blue (DeviceRGB).
    Rgb,
    /// Red, green, blue and an alpha channel.
    Rgba,
    /// Indexed colors that refer to a palette.
    Palette,
    /// Cyan, magenta, yellow, black (DeviceCMYK).
    Cmyk,
    /// One grey channel (DeviceGray).
    Greyscale,
    /// One grey channel and an alpha channel.
    GreyscaleAlpha,
}

impl ColorSpace {
    /// Name of the PDF color space. The variants with an alpha channel return `"DeviceN"`.
    pub fn as_string(&self) -> &'static str {
        use self::ColorSpace::*;
        match self {
            Rgb => "DeviceRGB",
            Cmyk => "DeviceCMYK",
            Greyscale => "DeviceGray",
            Palette => "Indexed",
            Rgba | GreyscaleAlpha => "DeviceN",
        }
    }
}

#[cfg(feature = "images")]
impl From<image::ColorType> for ColorSpace {
    fn from(color_type: image::ColorType) -> Self {
        use image::ColorType::*;
        match color_type {
            L8 | L16 => ColorSpace::Greyscale,
            La8 | La16 => ColorSpace::GreyscaleAlpha,
            Rgb8 | Rgb16 => ColorSpace::Rgb,
            Rgba8 | Rgba16 => ColorSpace::Rgba,
            _ => ColorSpace::Greyscale,
        }
    }
}

/// Bits per color component of an image.
#[derive(Debug, Copy, PartialEq, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ColorBits {
    /// 1 bit per component (black and white).
    Bit1,
    /// 8 bits per component (0 to 255).
    Bit8,
    /// 16 bits per component (0 to 65535).
    Bit16,
}

impl ColorBits {
    /// Number of bits, as written to the `BitsPerComponent` key of an image.
    pub fn as_integer(&self) -> i64 {
        match self {
            ColorBits::Bit1 => 1,
            ColorBits::Bit8 => 8,
            ColorBits::Bit16 => 16,
        }
    }
}

/// A color in one of the PDF device color spaces. All components are from 0.0 to 1.0, not 0 to 255.
///
/// Serialized to JSON as `{ "type": "rgb", "data": { ... } }` (the variant name in kebab-case).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "type", content = "data")]
pub enum Color {
    /// Red, green, blue (`rg` and `RG` operators).
    Rgb(Rgb),
    /// Cyan, magenta, yellow, black (`k` and `K` operators).
    Cmyk(Cmyk),
    /// One grey level (`g` and `G` operators).
    Greyscale(Greyscale),
    /// Named spot color. It is written as CMYK (`k` and `K` operators), there is no separation
    /// color space.
    SpotColor(SpotColor),
}

impl Color {
    /// Returns `true` if any component is outside 0.0 to 1.0.
    pub fn is_out_of_range(&self) -> bool {
        match self {
            Color::Rgb(rgb) => rgb.is_out_of_range(),
            Color::Cmyk(cmyk) => cmyk.is_out_of_range(),
            Color::Greyscale(greyscale) => greyscale.is_out_of_range(),
            Color::SpotColor(spot_color) => spot_color.is_out_of_range(),
        }
    }

    /// The components in operand order: `[r, g, b]`, `[c, m, y, k]` or `[grey]`.
    pub fn into_vec(&self) -> Vec<f32> {
        match self {
            Color::Rgb(rgb) => {
                vec![rgb.r, rgb.g, rgb.b]
            }
            Color::Cmyk(cmyk) => {
                vec![cmyk.c, cmyk.m, cmyk.y, cmyk.k]
            }
            Color::Greyscale(gs) => {
                vec![gs.percent]
            }
            Color::SpotColor(spot) => {
                vec![spot.c, spot.m, spot.y, spot.k]
            }
        }
    }

    /// The color as a CSS `rgb(r, g, b)` string with values from 0 to 255. CMYK is converted with
    /// the plain formula `(1 - c) * (1 - k)`, without a color profile.
    pub fn get_svg_id(&self) -> String {
        match self {
            Color::Rgb(rgb) => {
                let r = (rgb.r * 255.0).round() as u8;
                let g = (rgb.g * 255.0).round() as u8;
                let b = (rgb.b * 255.0).round() as u8;
                format!("rgb({}, {}, {})", r, g, b)
            }
            Color::Cmyk(cmyk) => {
                let r = (1.0 - cmyk.c) * (1.0 - cmyk.k);
                let g = (1.0 - cmyk.m) * (1.0 - cmyk.k);
                let b = (1.0 - cmyk.y) * (1.0 - cmyk.k);
                let r = (r * 255.0).round() as u8;
                let g = (g * 255.0).round() as u8;
                let b = (b * 255.0).round() as u8;
                format!("rgb({}, {}, {})", r, g, b)
            }
            Color::Greyscale(gs) => {
                let gray = (gs.percent * 255.0).round() as u8;
                format!("rgb({}, {}, {})", gray, gray, gray)
            }
            Color::SpotColor(spot) => {
                let r = (1.0 - spot.c) * (1.0 - spot.k);
                let g = (1.0 - spot.m) * (1.0 - spot.k);
                let b = (1.0 - spot.y) * (1.0 - spot.k);
                let r = (r * 255.0).round() as u8;
                let g = (g * 255.0).round() as u8;
                let b = (b * 255.0).round() as u8;
                format!("rgb({}, {}, {})", r, g, b)
            }
        }
    }

    /// The ICC profile id of the color, or `None` for spot colors, which have no profile field.
    pub fn get_icc_profile(&self) -> Option<&Option<IccProfileId>> {
        match *self {
            Color::Rgb(ref rgb) => Some(&rgb.icc_profile),
            Color::Cmyk(ref cmyk) => Some(&cmyk.icc_profile),
            Color::Greyscale(ref gs) => Some(&gs.icc_profile),
            Color::SpotColor(_) => None,
        }
    }
}

/// A color in DeviceRGB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rgb {
    /// Red, from 0.0 to 1.0.
    pub r: f32,
    /// Green, from 0.0 to 1.0.
    pub g: f32,
    /// Blue, from 0.0 to 1.0.
    pub b: f32,
    /// Id of an ICC profile for this color. Not used when the color is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icc_profile: Option<IccProfileId>,
}

impl Rgb {
    /// Creates an RGB color. Components are from 0.0 to 1.0, not 0 to 255.
    pub fn new(r: f32, g: f32, b: f32, icc_profile: Option<IccProfileId>) -> Self {
        Self {
            r,
            g,
            b,
            icc_profile,
        }
    }

    /// Returns `true` if a component is outside 0.0 to 1.0.
    pub fn is_out_of_range(&self) -> bool {
        self.r < 0.0 || self.r > 1.0 || self.g < 0.0 || self.g > 1.0 || self.b < 0.0 || self.b > 1.0
    }
}

/// A color in DeviceCMYK, as used for print. 0.0 is no ink, 1.0 is full ink.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Cmyk {
    /// Cyan, from 0.0 to 1.0.
    pub c: f32,
    /// Magenta, from 0.0 to 1.0.
    pub m: f32,
    /// Yellow, from 0.0 to 1.0.
    pub y: f32,
    /// Black ("key"), from 0.0 to 1.0.
    pub k: f32,
    /// Id of an ICC profile for this color. Not used when the color is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icc_profile: Option<IccProfileId>,
}

impl Cmyk {
    /// Creates a CMYK color. Components are from 0.0 to 1.0, not 0 to 255.
    pub fn new(c: f32, m: f32, y: f32, k: f32, icc_profile: Option<IccProfileId>) -> Self {
        Self {
            c,
            m,
            y,
            k,
            icc_profile,
        }
    }

    /// Returns `true` if a component is outside 0.0 to 1.0.
    pub fn is_out_of_range(&self) -> bool {
        self.c < 0.0
            || self.c > 1.0
            || self.m < 0.0
            || self.m > 1.0
            || self.y < 0.0
            || self.y > 1.0
            || self.k < 0.0
            || self.k > 1.0
    }
}

/// A grey level in DeviceGray.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Greyscale {
    /// Grey level from 0.0 (black) to 1.0 (white).
    pub percent: f32,
    /// Id of an ICC profile for this color. Not used when the color is written.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icc_profile: Option<IccProfileId>,
}

impl Greyscale {
    /// Creates a grey level from 0.0 (black) to 1.0 (white).
    pub fn new(percent: f32, icc_profile: Option<IccProfileId>) -> Self {
        Self {
            percent,
            icc_profile,
        }
    }

    /// Returns `true` if a component is outside 0.0 to 1.0.
    pub fn is_out_of_range(&self) -> bool {
        self.percent < 0.0 || self.percent > 1.0
    }
}

/// A spot color: in print, a pre-mixed ink with a name. Here it only holds CMYK values and is
/// written like a [`Cmyk`] color.
#[derive(Debug, Copy, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotColor {
    /// Cyan, from 0.0 to 1.0.
    pub c: f32,
    /// Magenta, from 0.0 to 1.0.
    pub m: f32,
    /// Yellow, from 0.0 to 1.0.
    pub y: f32,
    /// Black, from 0.0 to 1.0.
    pub k: f32,
}

impl SpotColor {
    /// Creates a spot color from CMYK components, each from 0.0 to 1.0.
    pub fn new(c: f32, m: f32, y: f32, k: f32) -> Self {
        Self { c, m, y, k }
    }

    /// Returns `true` if a component is outside 0.0 to 1.0.
    pub fn is_out_of_range(&self) -> bool {
        self.c < 0.0
            || self.c > 1.0
            || self.m < 0.0
            || self.m > 1.0
            || self.y < 0.0
            || self.y > 1.0
            || self.k < 0.0
            || self.k > 1.0
    }
}

/// Color model of an ICC profile, which decides the number of components.
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum IccProfileType {
    /// Four components, alternate space DeviceCMYK.
    Cmyk,
    /// Three components, alternate space DeviceRGB.
    Rgb,
    /// One component, alternate space DeviceGray.
    Greyscale,
}

/// An ICC color profile with the settings used to embed it as an `ICCBased` stream.
#[derive(Debug, Clone, PartialEq)]
pub struct IccProfile {
    /// Binary Icc profile
    pub icc: Vec<u8>,
    /// Color model of the profile. Sets the `N` key (number of components) of the stream.
    pub icc_type: IccProfileType,
    /// Write an `Alternate` entry that names the matching device space, for readers that cannot
    /// use the profile.
    pub has_alternate: bool,
    /// Write a `Range` entry of four `0 1` pairs.
    pub has_range: bool,
}

impl IccProfile {
    /// Creates a profile from the raw bytes of an ICC file. `has_alternate` is `true` and
    /// `has_range` is `false`.
    pub fn new(icc: Vec<u8>, icc_type: IccProfileType) -> Self {
        Self {
            icc,
            icc_type,
            has_alternate: true,
            has_range: false,
        }
    }

    /// Sets whether an `Alternate` device space is written.
    #[inline]
    pub fn with_alternate_profile(mut self, has_alternate: bool) -> Self {
        self.has_alternate = has_alternate;
        self
    }

    /// Sets whether a `Range` entry is written.
    #[inline]
    pub fn with_range(mut self, has_range: bool) -> Self {
        self.has_range = has_range;
        self
    }
}

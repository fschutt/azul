//! Provides a reference implementation of a style parser for Azul, capable of parsing CSS
//! stylesheets into their respective `Css` counterparts.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/azul_logo_full_min.svg.png",
    html_favicon_url = "https://raw.githubusercontent.com/maps4print/azul/master/assets/images/favicon.ico",
)]

#![warn(unused_must_use)]
#![deny(unreachable_patterns)]
#![deny(missing_copy_implementations)]
#![allow(unused_variables)]

extern crate azul_css;
extern crate simplecss;
#[cfg(feature = "serde_serialization")]
extern crate serde;

#[macro_use]
mod macros;

mod css_parser;
mod css;
mod hot_reloader;

pub use crate::css::{
    new_from_str,
    parse_css_path,
    CssParseError,
    CssPathParseError,
};

pub use crate::css_parser::*;

pub use crate::hot_reloader::{
    HotReloader,
};


pub use crate::css_color::CssColor;

pub mod css_color {

    use azul_css::{ColorU, ColorF};
    use crate::css_parser::{parse_css_color, CssColorParseError};

    /// CssColor is simply a wrapper around the internal CSS color parsing methods.
    ///
    /// Sometimes you'd want to load and parse a CSS color, but you don't want to
    /// write your own parser for that. Since Azul already has a parser for CSS colors,
    /// this API exposes
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub struct CssColor {
        internal: ColorU,
    }

    impl CssColor {
        /// Can parse a CSS color with or without prefixed hash or capitalization, i.e. `#aabbcc`
        pub fn from_str<'a>(input: &'a str) -> Result<Self, CssColorParseError<'a>> {
            let color = parse_css_color(input)?;
            Ok(Self {
                internal: color,
            })
        }

        /// Returns the internal parsed color, but in a `0.0 - 1.0` range instead of `0 - 255`
        pub fn to_color_f(&self) -> ColorF {
            self.internal.into()
        }

        /// Returns the internal parsed color
        pub fn to_color_u(&self) -> ColorU {
            self.internal
        }

        /// If `prefix_hash` is set to false, you only get the string, without a hash, in lowercase
        ///
        /// If `self.alpha` is `FF`, it will be omitted from the final result (since `FF` is the default for CSS colors)
        pub fn to_string(&self, prefix_hash: bool) -> String {
            let prefix = if prefix_hash { "#" } else { "" };
            let alpha = if self.internal.a == 255 { String::new() } else { format!("{:02x}", self.internal.a) };
            format!("{}{:02x}{:02x}{:02x}{}", prefix, self.internal.r, self.internal.g, self.internal.b, alpha)
        }
    }

    impl From<ColorU> for CssColor {
        fn from(color: ColorU) -> Self {
            CssColor { internal: color }
        }
    }

    impl From<ColorF> for CssColor {
        fn from(color: ColorF) -> Self {
            CssColor { internal: color.into() }
        }
    }

    impl Into<ColorF> for CssColor {
        fn into(self) -> ColorF {
            self.to_color_f()
        }
    }

    impl Into<ColorU> for CssColor {
        fn into(self) -> ColorU {
            self.to_color_u()
        }
    }

    impl Into<String> for CssColor {
        fn into(self) -> String {
            self.to_string(false)
        }
    }

    #[cfg(feature = "serde_serialization")]
    use serde::{de, Serialize, Deserialize, Serializer, Deserializer};

    #[cfg(feature = "serde_serialization")]
    impl Serialize for CssColor {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer,
        {
            let prefix_css_color_with_hash = true;
            serializer.serialize_str(&self.to_string(prefix_css_color_with_hash))
        }
    }

    #[cfg(feature = "serde_serialization")]
    impl<'de> Deserialize<'de> for CssColor {
        fn deserialize<D>(deserializer: D) -> Result<CssColor, D::Error>
        where D: Deserializer<'de>,
        {
            let s = String::deserialize(deserializer)?;
            CssColor::from_str(&s).map_err(de::Error::custom)
        }
    }
}
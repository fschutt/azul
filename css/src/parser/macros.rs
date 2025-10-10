/// A parser that can accept a list of items and mappings
macro_rules! multi_type_parser {
    ($fn:ident, $return_str:expr, $return:ident, $import_str:expr, $([$identifier_string:expr, $enum_type:ident, $parse_str:expr]),+) => {
        #[doc = "Parses a `"]
        #[doc = $return_str]
        #[doc = "` attribute from a `&str`"]
        #[doc = ""]
        #[doc = "# Example"]
        #[doc = ""]
        #[doc = "```rust"]
        #[doc = $import_str]
        $(
            #[doc = $parse_str]
        )+
        #[doc = "```"]
        pub fn $fn<'a>(input: &'a str)
        -> Result<$return, InvalidValueErr<'a>>
        {
            let input = input.trim();
            match input {
                $(
                    $identifier_string => Ok($return::$enum_type),
                )+
                _ => Err(InvalidValueErr(input)),
            }
        }

        impl FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
                match self {
                    $(
                        $return::$enum_type => write!(f, $identifier_string),
                    )+
                }
            }
        }
    };
    ($fn:ident, $return:ident, $([$identifier_string:expr, $enum_type:ident]),+) => {
        multi_type_parser!($fn, stringify!($return), $return,
            concat!(
                "# extern crate azul_css;", "\r\n",
                "# use azul_css::parser::", stringify!($fn), ";", "\r\n",
                "# use azul_css::", stringify!($return), ";"
            ),
            $([
                $identifier_string, $enum_type,
                concat!("assert_eq!(", stringify!($fn), "(\"", $identifier_string, "\"), Ok(", stringify!($return), "::", stringify!($enum_type), "));")
            ]),+
        );
    };
}

macro_rules! typed_pixel_value_parser {
    (
        $fn:ident, $fn_str:expr, $return:ident, $return_str:expr, $import_str:expr, $test_str:expr
    ) => {
        ///Parses a `
        #[doc = $return_str]
        ///` attribute from a `&str`
        ///
        ///# Example
        ///
        ///```rust
        #[doc = $import_str]
        #[doc = $test_str]
        ///```
        pub fn $fn<'a>(input: &'a str) -> Result<$return, CssPixelValueParseError<'a>> {
            parse_pixel_value(input).and_then(|e| Ok($return { inner: e }))
        }

        impl FormatAsCssValue for $return {
            fn format_as_css_value(&self, f: &mut fmt::Formatter) -> fmt::Result {
                self.inner.format_as_css_value(f)
            }
        }
    };
    ($fn:ident, $return:ident) => {
        typed_pixel_value_parser!(
            $fn,
            stringify!($fn),
            $return,
            stringify!($return),
            concat!(
                "# extern crate azul_css;",
                "\r\n",
                "# use azul_css::parser::",
                stringify!($fn),
                ";",
                "\r\n",
                "# use azul_css::{PixelValue, ",
                stringify!($return),
                "};"
            ),
            concat!(
                "assert_eq!(",
                stringify!($fn),
                "(\"5px\"), Ok(",
                stringify!($return),
                " { inner: PixelValue::px(5.0) }));"
            )
        );
    };
}

//! CSS properties for flowing content into regions (`flow-into`,
//! `flow-from`, `region-fragment`).

use alloc::string::{String, ToString};

use crate::{corety::AzString, props::formatter::PrintAsCssValue};

// --- flow-into ---
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
/// CSS `flow-into` property: takes an element, or only its content, out of
/// its parent's flow and into a named flow (CSS Regions 1, 3.1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum FlowInto {
    /// The element stays in its parent's flow (default).
    #[default]
    None,
    /// The element goes into the named flow of this name (`<name>` or
    /// `<name> element`).
    Named(AzString),
    /// The element's content goes into the named flow of this name, the
    /// element itself stays (`<name> content`).
    NamedContent(AzString),
}

impl FlowInto {
    /// The name of the flow the element or its content goes into.
    #[must_use]
    pub fn flow_name(&self) -> Option<&str> {
        match self {
            Self::None => None,
            Self::Named(name) | Self::NamedContent(name) => Some(name.as_str()),
        }
    }
}

impl PrintAsCssValue for FlowInto {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Named(s) => s.to_string(),
            Self::NamedContent(s) => alloc::format!("{} content", s.as_str()),
        }
    }
}

// --- flow-from ---
#[allow(variant_size_differences)]
// repr(C,u8) FFI enum: boxing the large variant would change the C ABI (api.json bindings); size
// disparity accepted
/// CSS `flow-from` property: makes a block container a region of a named
/// flow, which lays the flow's content out in it (CSS Regions 1, 3.2).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
#[derive(Default)]
pub enum FlowFrom {
    /// The element is no region (default).
    #[default]
    None,
    /// The element is a region of the named flow of this name.
    Named(AzString),
}

impl PrintAsCssValue for FlowFrom {
    fn print_as_css_value(&self) -> String {
        match self {
            Self::None => "none".to_string(),
            Self::Named(s) => s.to_string(),
        }
    }
}

// --- region-fragment ---
/// CSS `region-fragment` property: what the last region of a named flow does
/// with the content it has no room for (CSS Regions 1, 6.1).
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[repr(C)]
pub enum RegionFragment {
    /// The content runs on past the region's end, as its overflow (default).
    #[default]
    Auto,
    /// The content breaks at the region's end as if another region followed,
    /// and what would go on there is not shown.
    Break,
}

impl PrintAsCssValue for RegionFragment {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Break => "break",
        })
    }
}

// Formatting to Rust code
#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for FlowInto {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("FlowInto::None"),
            Self::Named(s) => format!(
                "FlowInto::Named(AzString::from_const_str({:?}))",
                s.as_str()
            ),
            Self::NamedContent(s) => format!(
                "FlowInto::NamedContent(AzString::from_const_str({:?}))",
                s.as_str()
            ),
        }
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for FlowFrom {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        match self {
            Self::None => String::from("FlowFrom::None"),
            Self::Named(s) => format!(
                "FlowFrom::Named(AzString::from_const_str({:?}))",
                s.as_str()
            ),
        }
    }
}

#[cfg(feature = "codegen")]
impl crate::codegen::format::FormatAsRustCode for RegionFragment {
    fn format_as_rust_code(&self, _tabs: usize) -> String {
        String::from(match self {
            Self::Auto => "RegionFragment::Auto",
            Self::Break => "RegionFragment::Break",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
pub mod parser {
    #[allow(clippy::wildcard_imports)]
    // parser submodule reuses the parent module's value types
    use super::*;
    use crate::corety::AzString;

    macro_rules! define_flow_parser {
        (
            $fn_name:ident,
            $struct_name:ident,
            $error_name:ident,
            $error_owned_name:ident,
            $prop_name:expr
        ) => {
            #[derive(Clone, PartialEq, Eq)]
            pub enum $error_name<'a> {
                InvalidValue(&'a str),
            }

            impl_debug_as_display!($error_name<'a>);
            impl_display! { $error_name<'a>, {
                InvalidValue(v) => format!("Invalid {} value: \"{}\"", $prop_name, v),
            }}

            #[derive(Debug, Clone, PartialEq, Eq)]
            #[repr(C, u8)]
            pub enum $error_owned_name {
                InvalidValue(AzString),
            }

            impl $error_name<'_> {
                #[must_use]
                pub fn to_contained(&self) -> $error_owned_name {
                    match self {
                        Self::InvalidValue(s) => {
                            $error_owned_name::InvalidValue(s.to_string().into())
                        }
                    }
                }
            }

            impl $error_owned_name {
                #[must_use]
                pub fn to_shared(&self) -> $error_name<'_> {
                    match self {
                        Self::InvalidValue(s) => $error_name::InvalidValue(s.as_str()),
                    }
                }
            }

            /// # Errors
            ///
            /// Returns an error if `input` is not a valid CSS value for this property.
            pub fn $fn_name(input: &str) -> Result<$struct_name, $error_name<'_>> {
                $struct_name::parsed(input).ok_or($error_name::InvalidValue(input))
            }
        };
    }

    define_flow_parser!(
        parse_flow_into,
        FlowInto,
        FlowIntoParseError,
        FlowIntoParseErrorOwned,
        "flow-into"
    );
    define_flow_parser!(
        parse_flow_from,
        FlowFrom,
        FlowFromParseError,
        FlowFromParseErrorOwned,
        "flow-from"
    );
    define_flow_parser!(
        parse_region_fragment,
        RegionFragment,
        RegionFragmentParseError,
        RegionFragmentParseErrorOwned,
        "region-fragment"
    );

    /// Whether `name` can name a flow: a CSS identifier other than the
    /// keywords CSS Regions 1 rules out (`none`, `inherit`, `default`,
    /// `auto`, `initial`).
    fn is_flow_name(name: &str) -> bool {
        let mut chars = name.chars();
        let starts_an_ident = match chars.next() {
            Some('-') => chars
                .next()
                .is_some_and(|c| c == '-' || c == '_' || c.is_alphabetic() || !c.is_ascii()),
            Some(c) => c == '_' || c.is_alphabetic() || !c.is_ascii(),
            None => false,
        };
        starts_an_ident
            && name
                .chars()
                .all(|c| c == '-' || c == '_' || c.is_alphanumeric() || !c.is_ascii())
            && !["none", "inherit", "default", "auto", "initial"]
                .iter()
                .any(|keyword| name.eq_ignore_ascii_case(keyword))
    }

    impl FlowInto {
        /// `flow-into`: `none | <ident> [element | content]?`.
        fn parsed(input: &str) -> Option<Self> {
            let mut words = input.split_whitespace();
            let (Some(first), second, None) = (words.next(), words.next(), words.next()) else {
                return None;
            };
            if second.is_none() && first.eq_ignore_ascii_case("none") {
                return Some(Self::None);
            }
            if !is_flow_name(first) {
                return None;
            }
            let name: AzString = first.to_string().into();
            match second {
                None => Some(Self::Named(name)),
                Some(keyword) if keyword.eq_ignore_ascii_case("element") => Some(Self::Named(name)),
                Some(keyword) if keyword.eq_ignore_ascii_case("content") => {
                    Some(Self::NamedContent(name))
                }
                Some(_) => None,
            }
        }
    }

    impl FlowFrom {
        /// `flow-from`: `<ident> | none`.
        fn parsed(input: &str) -> Option<Self> {
            let trimmed = input.trim();
            if trimmed.eq_ignore_ascii_case("none") {
                Some(Self::None)
            } else if is_flow_name(trimmed) {
                Some(Self::Named(trimmed.to_string().into()))
            } else {
                None
            }
        }
    }

    impl RegionFragment {
        /// `region-fragment`: `auto | break`.
        fn parsed(input: &str) -> Option<Self> {
            let trimmed = input.trim();
            if trimmed.eq_ignore_ascii_case("auto") {
                Some(Self::Auto)
            } else if trimmed.eq_ignore_ascii_case("break") {
                Some(Self::Break)
            } else {
                None
            }
        }
    }
}

#[cfg(feature = "parser")]
pub use parser::*;

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn test_parse_flow_into() {
        assert_eq!(parse_flow_into("none").unwrap(), FlowInto::None);
        assert_eq!(
            parse_flow_into("my-article-flow").unwrap(),
            FlowInto::Named("my-article-flow".into())
        );
        assert!(parse_flow_into("").is_err());
    }

    #[test]
    fn flow_into_takes_a_flow_name_and_says_whether_the_element_or_its_content_goes() {
        let printed = |value: &str| parse_flow_into(value).map(|f| f.print_as_css_value()).ok();
        assert_eq!(printed("art7").as_deref(), Some("art7"));
        assert_eq!(printed(" art7  element ").as_deref(), Some("art7"));
        assert_eq!(printed("art7 content").as_deref(), Some("art7 content"));
        assert_eq!(printed("None").as_deref(), Some("none"));
        assert_eq!(
            parse_flow_into("art7 CONTENT").unwrap(),
            FlowInto::NamedContent("art7".into())
        );
        assert_eq!(parse_flow_into("art7 element").unwrap().flow_name(), Some("art7"));
    }

    #[test]
    fn flow_into_and_flow_from_refuse_what_names_no_flow() {
        for value in [
            "auto",
            "initial",
            "inherit",
            "default",
            "a b",
            "art7 content x",
            "7up",
            "none content",
        ] {
            assert!(parse_flow_into(value).is_err(), "flow-into: {value}");
        }
        for value in ["auto", "a b", "art7 content"] {
            assert!(parse_flow_from(value).is_err(), "flow-from: {value}");
        }
    }

    #[test]
    fn test_parse_flow_from() {
        assert_eq!(parse_flow_from("none").unwrap(), FlowFrom::None);
        assert_eq!(
            parse_flow_from("  main-thread  ").unwrap(),
            FlowFrom::Named("main-thread".into())
        );
        assert!(parse_flow_from("").is_err());
    }

    #[test]
    fn region_fragment_is_auto_or_break() {
        assert_eq!(parse_region_fragment("auto"), Ok(RegionFragment::Auto));
        assert_eq!(parse_region_fragment(" Break "), Ok(RegionFragment::Break));
        assert_eq!(RegionFragment::default(), RegionFragment::Auto);
        for value in [RegionFragment::Auto, RegionFragment::Break] {
            assert_eq!(parse_region_fragment(&value.print_as_css_value()), Ok(value));
        }
        for value in ["", "none", "clip", "break auto"] {
            assert!(parse_region_fragment(value).is_err(), "region-fragment: {value}");
        }
    }
}

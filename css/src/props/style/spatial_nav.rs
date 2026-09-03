//! CSS Spatial Navigation Level 1 - the two per-container overrides.
//!
//! 9a-i-a made an arrow key try focus first and fall back to scrolling, which
//! is the spec's default behaviour and is right almost everywhere. These two
//! properties are how a container opts OUT of that default, and neither is
//! expressible any other way:
//!
//! - [`StyleSpatialNavigationAction`] forces the choice on a scroll container:
//!   always scroll (a map, a canvas, a code editor - places where an arrow
//!   means "pan", never "jump to the next button"), or always move focus.
//! - [`StyleSpatialNavigationContain`] makes an element a spatial navigation
//!   CONTAINER even when it is not a scroll container, so navigation inside a
//!   panel stays inside it.
//!
//! Both are from `css-nav-1`, and both have `auto` as their initial value, so
//! adding them changes nothing until a stylesheet asks.

use crate::{corety::AzString, props::formatter::PrintAsCssValue};

/// `spatial-navigation-action` - what an arrow key does on a scroll container.
///
/// ```css
/// .map     { spatial-navigation-action: scroll; }  /* arrows always pan   */
/// .menu    { spatial-navigation-action: focus; }   /* arrows never scroll */
/// ```
///
/// The default is [`Auto`](Self::Auto), which is the ordered fallback 9a-i-a
/// implements: move focus if there is somewhere to move it, otherwise scroll.
///
/// NOT INHERITED. The property answers "what does an arrow do when THIS
/// element is the scroll container", and a container that pans is routinely
/// full of ordinary focusable controls that must keep behaving normally -
/// inheriting `scroll` into them would make every button inside a map
/// unreachable by keyboard.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleSpatialNavigationAction {
    /// Move focus if a candidate lies in that direction; otherwise scroll.
    #[default]
    Auto,
    /// Always move focus. If there is no candidate the container does NOT
    /// scroll - the search continues outward instead, so an arrow at the edge
    /// of a menu escapes it rather than nudging it.
    Focus,
    /// Always scroll, changing nothing about focus, even when focusable
    /// children are sitting right there. What a map, a canvas or a code
    /// editor wants.
    Scroll,
}

impl PrintAsCssValue for StyleSpatialNavigationAction {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Focus => "focus",
            Self::Scroll => "scroll",
        })
    }
}

/// `spatial-navigation-contain` - whether this element is a spatial
/// navigation container.
///
/// ```css
/// .sidebar { spatial-navigation-contain: contain; }
/// ```
///
/// Under `auto`, only scroll containers (and the viewport) are containers,
/// which is the spec's default. `contain` adds one for an element that does
/// not scroll - a toolbar, a dialog, a sidebar - so that arrow keys resolve
/// among its descendants first and only leave it when nothing inside answers.
///
/// NOT INHERITED, and for a sharper reason than the action property: it marks
/// ONE element as a boundary. Inheriting it would make every descendant a
/// boundary too, which is the same as having none - each nested container
/// would trap navigation one level deeper until an arrow could not move at
/// all.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleSpatialNavigationContain {
    /// A container only if this element is a scroll container.
    #[default]
    Auto,
    /// A container regardless of whether it scrolls.
    Contain,
}

impl PrintAsCssValue for StyleSpatialNavigationContain {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Auto => "auto",
            Self::Contain => "contain",
        })
    }
}

/// `spatial-navigation-action` parse error.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssSpatialNavigationActionParseError<'a> {
    InvalidValue(&'a str),
}

impl core::fmt::Display for CssSpatialNavigationActionParseError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidValue(v) => write!(
                f,
                "Invalid spatial-navigation-action value: \"{v}\" (expected auto, focus or scroll)"
            ),
        }
    }
}

/// Owned mirror of [`CssSpatialNavigationActionParseError`].
// `AzString`, not `String`, and `#[repr(C, u8)]`, not bare `repr(C)`. Both
// are FFI requirements this type cannot opt out of: it is reachable from the
// exposed parse-error surface, the codegen builds its mirror from `AzString`,
// and a payload enum with no repr compiles silently and is undefined across
// the boundary. `CssAppRegionParseErrorOwned` beside it is the same shape for
// the same reasons - api.json's own checker passes either way, so this is
// caught only by building the generated C ABI.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum CssSpatialNavigationActionParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> CssSpatialNavigationActionParseError<'a> {
    #[must_use]
    pub fn to_contained(&self) -> CssSpatialNavigationActionParseErrorOwned {
        match self {
            Self::InvalidValue(v) => {
                CssSpatialNavigationActionParseErrorOwned::InvalidValue((*v).into())
            }
        }
    }
}

impl CssSpatialNavigationActionParseErrorOwned {
    #[must_use]
    pub fn to_shared(&self) -> CssSpatialNavigationActionParseError<'_> {
        match self {
            Self::InvalidValue(v) => CssSpatialNavigationActionParseError::InvalidValue(v.as_str()),
        }
    }
}

/// `spatial-navigation-contain` parse error.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssSpatialNavigationContainParseError<'a> {
    InvalidValue(&'a str),
}

impl core::fmt::Display for CssSpatialNavigationContainParseError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidValue(v) => write!(
                f,
                "Invalid spatial-navigation-contain value: \"{v}\" (expected auto or contain)"
            ),
        }
    }
}

/// Owned mirror of [`CssSpatialNavigationContainParseError`].
// `AzString`, not `String`, and `#[repr(C, u8)]`, not bare `repr(C)`. Both
// are FFI requirements this type cannot opt out of: it is reachable from the
// exposed parse-error surface, the codegen builds its mirror from `AzString`,
// and a payload enum with no repr compiles silently and is undefined across
// the boundary. `CssAppRegionParseErrorOwned` beside it is the same shape for
// the same reasons - api.json's own checker passes either way, so this is
// caught only by building the generated C ABI.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum CssSpatialNavigationContainParseErrorOwned {
    InvalidValue(AzString),
}

impl<'a> CssSpatialNavigationContainParseError<'a> {
    #[must_use]
    pub fn to_contained(&self) -> CssSpatialNavigationContainParseErrorOwned {
        match self {
            Self::InvalidValue(v) => {
                CssSpatialNavigationContainParseErrorOwned::InvalidValue((*v).into())
            }
        }
    }
}

impl CssSpatialNavigationContainParseErrorOwned {
    #[must_use]
    pub fn to_shared(&self) -> CssSpatialNavigationContainParseError<'_> {
        match self {
            Self::InvalidValue(v) => {
                CssSpatialNavigationContainParseError::InvalidValue(v.as_str())
            }
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not `auto`, `focus` or `scroll`.
pub fn parse_style_spatial_navigation_action(
    input: &str,
) -> Result<StyleSpatialNavigationAction, CssSpatialNavigationActionParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleSpatialNavigationAction::Auto),
        "focus" => Ok(StyleSpatialNavigationAction::Focus),
        "scroll" => Ok(StyleSpatialNavigationAction::Scroll),
        _ => Err(CssSpatialNavigationActionParseError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not `auto` or `contain`.
pub fn parse_style_spatial_navigation_contain(
    input: &str,
) -> Result<StyleSpatialNavigationContain, CssSpatialNavigationContainParseError<'_>> {
    match input.trim() {
        "auto" => Ok(StyleSpatialNavigationContain::Auto),
        "contain" => Ok(StyleSpatialNavigationContain::Contain),
        _ => Err(CssSpatialNavigationContainParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    fn the_action_keywords_parse_and_round_trip() {
        for (text, value) in [
            ("auto", StyleSpatialNavigationAction::Auto),
            ("focus", StyleSpatialNavigationAction::Focus),
            ("scroll", StyleSpatialNavigationAction::Scroll),
        ] {
            assert_eq!(parse_style_spatial_navigation_action(text), Ok(value));
            assert_eq!(value.print_as_css_value(), text);
        }
        // Surrounding whitespace survives the tokenizer in some paths.
        assert_eq!(
            parse_style_spatial_navigation_action("  scroll "),
            Ok(StyleSpatialNavigationAction::Scroll)
        );
    }

    #[test]
    fn the_contain_keywords_parse_and_round_trip() {
        for (text, value) in [
            ("auto", StyleSpatialNavigationContain::Auto),
            ("contain", StyleSpatialNavigationContain::Contain),
        ] {
            assert_eq!(parse_style_spatial_navigation_contain(text), Ok(value));
            assert_eq!(value.print_as_css_value(), text);
        }
    }

    /// `none` is NOT a spelling of either. Accepting it would silently turn a
    /// typo into the initial value, which reads as "the property did nothing".
    #[test]
    fn a_wrong_keyword_is_an_error_and_not_the_default() {
        assert!(parse_style_spatial_navigation_action("none").is_err());
        assert!(parse_style_spatial_navigation_action("contain").is_err());
        assert!(parse_style_spatial_navigation_contain("none").is_err());
        assert!(parse_style_spatial_navigation_contain("focus").is_err());
    }

    /// THE PROPERTY NAME HAS TO REACH THE PARSER, and a keyword parser that
    /// works in isolation proves nothing about that: the name table, the
    /// `CssPropertyType` arm and the dispatch all have to agree, and each is
    /// in a different file.
    #[test]
    fn both_properties_parse_from_their_css_name() {
        use crate::props::property::{
            get_css_key_map, parse_css_property, CssProperty, CssPropertyType,
        };

        let map = get_css_key_map();
        let ty = CssPropertyType::from_str("spatial-navigation-action", &map)
            .expect("`spatial-navigation-action` must be a known property name");
        assert_eq!(ty, CssPropertyType::SpatialNavigationAction);
        assert_eq!(
            parse_css_property(ty, "scroll"),
            Ok(CssProperty::SpatialNavigationAction(
                crate::css::CssPropertyValue::Exact(StyleSpatialNavigationAction::Scroll)
            ))
        );

        let ty = CssPropertyType::from_str("spatial-navigation-contain", &map)
            .expect("`spatial-navigation-contain` must be a known property name");
        assert_eq!(ty, CssPropertyType::SpatialNavigationContain);
        assert_eq!(
            parse_css_property(ty, "contain"),
            Ok(CssProperty::SpatialNavigationContain(
                crate::css::CssPropertyValue::Exact(StyleSpatialNavigationContain::Contain)
            ))
        );

        // Neither moves a box nor paints a pixel, so neither may charge a
        // layout pass. The default for an unlisted property is `true`, which
        // is why this is worth pinning.
        assert!(!CssPropertyType::SpatialNavigationAction.can_trigger_relayout());
        assert!(!CssPropertyType::SpatialNavigationContain.can_trigger_relayout());
    }

    /// Both default to `auto`, which is what makes adding them a no-op for
    /// every stylesheet that does not mention them.
    #[test]
    fn both_properties_default_to_auto() {
        assert_eq!(
            StyleSpatialNavigationAction::default(),
            StyleSpatialNavigationAction::Auto
        );
        assert_eq!(
            StyleSpatialNavigationContain::default(),
            StyleSpatialNavigationContain::Auto
        );
    }
}

//! CSS Spatial Navigation Level 1 - the three per-container overrides.
//!
//! 9a-i-a made an arrow key try focus first and fall back to scrolling, which
//! is the spec's default behaviour and is right almost everywhere. These
//! properties are how a container opts OUT of that default, and none of them is
//! expressible any other way:
//!
//! - [`StyleSpatialNavigationAction`] forces the choice on a scroll container: always scroll (a
//!   map, a canvas, a code editor - places where an arrow means "pan", never "jump to the next
//!   button"), or always move focus.
//! - [`StyleSpatialNavigationContain`] makes an element a spatial navigation CONTAINER even when it
//!   is not a scroll container, so navigation inside a panel stays inside it.
//! - [`StyleSpatialNavigationFunction`] picks the candidate-selection rule inside a container: the
//!   spec's distance function, or `grid`, which prefers the candidate lined up with the focus.
//!
//! All three are from `css-nav-1`, and each has an initial value that changes
//! nothing until a stylesheet asks.

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

/// `spatial-navigation-function` - how a spatial navigation container picks
/// the next focus among the candidates in the pressed direction (css-nav-1
/// §9.3).
///
/// ```css
/// .tv-guide { spatial-navigation-contain: contain; spatial-navigation-function: grid; }
/// ```
///
/// Read off the CONTAINER being searched, not off the focused element: the
/// property "applies to spatial navigation containers".
///
/// NOT INHERITED, per the spec. A nested container that wants `grid` says so
/// itself; a grid of cards holding a free-form toolbar must not force its rule
/// onto the toolbar.
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum StyleSpatialNavigationFunction {
    /// The spec's distance function: euclidean distance between the two
    /// closest points, plus a heavy penalty for drift across the axis, minus a
    /// bonus for overlap along it. Picks the "nearest" candidate the way a
    /// person reads a free-form layout.
    #[default]
    Normal,
    /// Prefer the candidate that is ALIGNED with the focus (its projection on
    /// the cross axis overlaps the focus's), nearest along the axis first; only
    /// when nothing is aligned, the nearest along the axis. What a grid of
    /// tiles wants: Down goes to the tile below, never to a nearer one that sits
    /// half a column to the side.
    Grid,
}

impl PrintAsCssValue for StyleSpatialNavigationFunction {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Normal => "normal",
            Self::Grid => "grid",
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

impl CssSpatialNavigationActionParseError<'_> {
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

impl CssSpatialNavigationContainParseError<'_> {
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

/// `spatial-navigation-function` parse error.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssSpatialNavigationFunctionParseError<'a> {
    InvalidValue(&'a str),
}

impl core::fmt::Display for CssSpatialNavigationFunctionParseError<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidValue(v) => write!(
                f,
                "Invalid spatial-navigation-function value: \"{v}\" (expected normal or grid)"
            ),
        }
    }
}

/// Owned mirror of [`CssSpatialNavigationFunctionParseError`].
// `AzString` and `#[repr(C, u8)]` for the same FFI reasons as the two owned
// errors above.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C, u8)]
pub enum CssSpatialNavigationFunctionParseErrorOwned {
    InvalidValue(AzString),
}

impl CssSpatialNavigationFunctionParseError<'_> {
    #[must_use]
    pub fn to_contained(&self) -> CssSpatialNavigationFunctionParseErrorOwned {
        match self {
            Self::InvalidValue(v) => {
                CssSpatialNavigationFunctionParseErrorOwned::InvalidValue((*v).into())
            }
        }
    }
}

impl CssSpatialNavigationFunctionParseErrorOwned {
    #[must_use]
    pub fn to_shared(&self) -> CssSpatialNavigationFunctionParseError<'_> {
        match self {
            Self::InvalidValue(v) => {
                CssSpatialNavigationFunctionParseError::InvalidValue(v.as_str())
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

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not `normal` or `grid`.
pub fn parse_style_spatial_navigation_function(
    input: &str,
) -> Result<StyleSpatialNavigationFunction, CssSpatialNavigationFunctionParseError<'_>> {
    match input.trim() {
        "normal" => Ok(StyleSpatialNavigationFunction::Normal),
        "grid" => Ok(StyleSpatialNavigationFunction::Grid),
        _ => Err(CssSpatialNavigationFunctionParseError::InvalidValue(input)),
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

    #[test]
    fn the_function_keywords_parse_and_round_trip() {
        for (text, value) in [
            ("normal", StyleSpatialNavigationFunction::Normal),
            ("grid", StyleSpatialNavigationFunction::Grid),
        ] {
            assert_eq!(parse_style_spatial_navigation_function(text), Ok(value));
            assert_eq!(value.print_as_css_value(), text);
        }
        assert_eq!(
            parse_style_spatial_navigation_function(" grid "),
            Ok(StyleSpatialNavigationFunction::Grid)
        );
        assert_eq!(
            StyleSpatialNavigationFunction::default(),
            StyleSpatialNavigationFunction::Normal,
            "the initial value is `normal`",
        );
    }

    /// `none` is NOT a spelling of either. Accepting it would silently turn a
    /// typo into the initial value, which reads as "the property did nothing".
    #[test]
    fn a_wrong_keyword_is_an_error_and_not_the_default() {
        assert!(parse_style_spatial_navigation_action("none").is_err());
        assert!(parse_style_spatial_navigation_action("contain").is_err());
        assert!(parse_style_spatial_navigation_contain("none").is_err());
        assert!(parse_style_spatial_navigation_contain("focus").is_err());
        assert!(parse_style_spatial_navigation_function("auto").is_err());
        assert!(parse_style_spatial_navigation_function("flex").is_err());
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

    /// `spatial-navigation-function` (css-nav-1 §9.3) reaches the parser by
    /// its CSS name, prints back to the keyword it was written as, and charges
    /// no layout pass. Written against the name table and `CssProperty::value`
    /// only, so it says the same thing before and after the type exists.
    #[test]
    fn the_function_property_parses_from_its_css_name_and_prints_back() {
        use crate::props::property::{get_css_key_map, parse_css_property, CssPropertyType};

        let map = get_css_key_map();
        let ty = CssPropertyType::from_str("spatial-navigation-function", &map)
            .expect("`spatial-navigation-function` must be a known property name");
        for keyword in ["normal", "grid"] {
            let parsed = parse_css_property(ty, keyword)
                .unwrap_or_else(|e| panic!("`{keyword}` must parse, got {e:?}"));
            assert_eq!(parsed.get_type(), ty);
            assert_eq!(parsed.value(), keyword, "`{keyword}` must print back as itself");
        }
        // `auto` is a keyword of the two sibling properties, not of this one.
        assert!(parse_css_property(ty, "auto").is_err());
        assert!(!ty.can_trigger_relayout());
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

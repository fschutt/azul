//! CSS properties for `display` and `float`.

use alloc::string::{String, ToString};
use crate::corety::AzString;

use crate::props::formatter::PrintAsCssValue;

/// Represents a `display` CSS property value
// +spec:display-property:472a62 - display property controls box generation types per CSS 2.2 §9.2
// +spec:display-property:cf1820 - display type enum defining box generation qualities
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutDisplay {
    // Basic display types
    None,
    // +spec:display-property:7d945d - outer display defaults to block, inner defaults to flow
    #[default]
    Block,
    Inline,
    InlineBlock,

    // Flex layout
    Flex,
    InlineFlex,

    // +spec:display-property:03b26a - Table display types mapping document elements to CSS table model
    // +spec:display-property:d40388 - layout-internal display types set both inner and outer display
    // +spec:display-property:dcf7f5 - table display values (table, inline-table, table-row, etc.) per CSS 2.2 §17
    // +spec:table-layout:7fdc60 - display property maps elements to table roles (CSS 2.2 §17.1)
    // Table layout
    // +spec:display-property:1554ad - Layout-internal display types for table layout
    // +spec:table-layout:6cc828 - <display-internal> and <display-legacy> table display types
    Table,
    InlineTable,
    TableRowGroup,
    TableHeaderGroup,
    TableFooterGroup,
    TableRow,
    TableColumnGroup,
    TableColumn,
    TableCell,
    TableCaption,

    FlowRoot,

    // List layout
    ListItem,

    // Special displays
    RunIn,
    Marker,

    // CSS3 additions
    Grid,
    InlineGrid,

    // display:contents - element generates no box, children promoted to parent
    Contents,
}

impl LayoutDisplay {
    /// Returns true if this display type establishes a block formatting context.
    #[must_use] pub const fn creates_block_context(&self) -> bool {
        matches!(
            self,
            Self::Block
                | Self::FlowRoot
                | Self::Flex
                | Self::Grid
                | Self::Table
                | Self::ListItem
        )
    }

    /// Returns true if this display type establishes a flex formatting context.
    #[must_use] pub const fn creates_flex_context(&self) -> bool {
        matches!(self, Self::Flex | Self::InlineFlex)
    }

    // +spec:display-property:798b4f - table box establishes table formatting context (CSS 2.2 §17.4)
    /// Returns true if this display type establishes a table formatting context.
    #[must_use] pub const fn creates_table_context(&self) -> bool {
        matches!(self, Self::Table | Self::InlineTable)
    }

    /// Returns true for layout-internal display types (CSS Display 3 §2.4):
    /// table-row-group, table-header-group, table-footer-group, table-row,
    /// table-column-group, table-column, table-cell, table-caption.
    #[must_use] pub const fn is_layout_internal(&self) -> bool {
        matches!(
            self,
            Self::TableRowGroup
                | Self::TableHeaderGroup
                | Self::TableFooterGroup
                | Self::TableRow
                | Self::TableColumnGroup
                | Self::TableColumn
                | Self::TableCell
                | Self::TableCaption
        )
    }

    // +spec:display-property:101f27 - inline-level boxes (InlineBlock, InlineFlex, etc.) vs inline boxes (Inline)
    // +spec:display-property:18e77e - inner-only display keywords (flex, grid, table, flow-root) are not inline-level, defaulting outer display to block
    // +spec:display-property:a43e48 - inline-table is inline-level per CSS 2.2 §17.4
    /// Returns true if this display type generates an inline-level box.
    #[must_use] pub const fn is_inline_level(&self) -> bool {
        matches!(
            self,
            Self::Inline
                | Self::InlineBlock
                | Self::InlineFlex
                | Self::InlineTable
                | Self::InlineGrid
        )
    }
}

// +spec:display-property:cabaec - serialization uses short display keywords per CSSOM precedence rules
impl PrintAsCssValue for LayoutDisplay {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::None => "none",
            Self::Block => "block",
            Self::Inline => "inline",
            Self::InlineBlock => "inline-block",
            Self::Flex => "flex",
            Self::InlineFlex => "inline-flex",
            Self::Table => "table",
            Self::InlineTable => "inline-table",
            Self::TableRowGroup => "table-row-group",
            Self::TableHeaderGroup => "table-header-group",
            Self::TableFooterGroup => "table-footer-group",
            Self::TableRow => "table-row",
            Self::TableColumnGroup => "table-column-group",
            Self::TableColumn => "table-column",
            Self::TableCell => "table-cell",
            Self::TableCaption => "table-caption",
            Self::ListItem => "list-item",
            Self::RunIn => "run-in",
            Self::Marker => "marker",
            Self::FlowRoot => "flow-root",
            Self::Grid => "grid",
            Self::InlineGrid => "inline-grid",
            Self::Contents => "contents",
        })
    }
}

/// Represents a `float` attribute
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(C)]
pub enum LayoutFloat {
    Left,
    Right,
    #[default]
    None,
}

impl PrintAsCssValue for LayoutFloat {
    fn print_as_css_value(&self) -> String {
        String::from(match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::None => "none",
        })
    }
}

// --- PARSERS ---

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutDisplayParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutDisplayParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { LayoutDisplayParseError<'a>, {
    InvalidValue(val) => format!("Invalid display value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutDisplayParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutDisplayParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutDisplayParseErrorOwned {
        match self {
            Self::InvalidValue(s) => LayoutDisplayParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutDisplayParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutDisplayParseError<'_> {
        match self {
            Self::InvalidValue(s) => LayoutDisplayParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `display` value.
pub fn parse_layout_display(
    input: &str,
) -> Result<LayoutDisplay, LayoutDisplayParseError<'_>> {
    let input = input.trim();
    match input {
        "none" => Ok(LayoutDisplay::None),
        "block" => Ok(LayoutDisplay::Block),
        "inline" => Ok(LayoutDisplay::Inline),
        // +spec:display-property:f704ef - legacy single-keyword inline-level display values (inline-block, inline-table, inline-flex, inline-grid)
        "inline-block" => Ok(LayoutDisplay::InlineBlock),
        "flex" => Ok(LayoutDisplay::Flex),
        "inline-flex" => Ok(LayoutDisplay::InlineFlex),
        "table" => Ok(LayoutDisplay::Table),
        "inline-table" => Ok(LayoutDisplay::InlineTable),
        "table-row-group" => Ok(LayoutDisplay::TableRowGroup),
        "table-header-group" => Ok(LayoutDisplay::TableHeaderGroup),
        "table-footer-group" => Ok(LayoutDisplay::TableFooterGroup),
        "table-row" => Ok(LayoutDisplay::TableRow),
        "table-column-group" => Ok(LayoutDisplay::TableColumnGroup),
        "table-column" => Ok(LayoutDisplay::TableColumn),
        "table-cell" => Ok(LayoutDisplay::TableCell),
        "table-caption" => Ok(LayoutDisplay::TableCaption),
        "list-item" => Ok(LayoutDisplay::ListItem),
        "run-in" => Ok(LayoutDisplay::RunIn),
        "marker" => Ok(LayoutDisplay::Marker),
        "grid" => Ok(LayoutDisplay::Grid),
        "inline-grid" => Ok(LayoutDisplay::InlineGrid),
        "flow-root" => Ok(LayoutDisplay::FlowRoot),
        "contents" => Ok(LayoutDisplay::Contents),
        _ => Err(LayoutDisplayParseError::InvalidValue(input)),
    }
}

#[cfg(feature = "parser")]
#[derive(Clone, PartialEq, Eq)]
pub enum LayoutFloatParseError<'a> {
    InvalidValue(&'a str),
}

#[cfg(feature = "parser")]
impl_debug_as_display!(LayoutFloatParseError<'a>);

#[cfg(feature = "parser")]
impl_display! { LayoutFloatParseError<'a>, {
    InvalidValue(val) => format!("Invalid float value: \"{}\"", val),
}}

#[cfg(feature = "parser")]
#[derive(Debug, Clone, PartialEq, Eq)]
#[repr(C, u8)]
pub enum LayoutFloatParseErrorOwned {
    InvalidValue(AzString),
}

#[cfg(feature = "parser")]
impl LayoutFloatParseError<'_> {
    #[must_use] pub fn to_contained(&self) -> LayoutFloatParseErrorOwned {
        match self {
            Self::InvalidValue(s) => LayoutFloatParseErrorOwned::InvalidValue((*s).to_string().into()),
        }
    }
}

#[cfg(feature = "parser")]
impl LayoutFloatParseErrorOwned {
    #[must_use] pub fn to_shared(&self) -> LayoutFloatParseError<'_> {
        match self {
            Self::InvalidValue(s) => LayoutFloatParseError::InvalidValue(s.as_str()),
        }
    }
}

#[cfg(feature = "parser")]
/// # Errors
///
/// Returns an error if `input` is not a valid CSS `float` value.
pub fn parse_layout_float(input: &str) -> Result<LayoutFloat, LayoutFloatParseError<'_>> {
    let input = input.trim();
    match input {
        "left" => Ok(LayoutFloat::Left),
        "right" => Ok(LayoutFloat::Right),
        "none" => Ok(LayoutFloat::None),
        _ => Err(LayoutFloatParseError::InvalidValue(input)),
    }
}

#[cfg(all(test, feature = "parser"))]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::cognitive_complexity)] // large but cohesive: single-purpose CSS parser/formatter/dispatch table (one branch per property/variant)
    fn test_parse_layout_display() {
        assert_eq!(parse_layout_display("block").unwrap(), LayoutDisplay::Block);
        assert_eq!(
            parse_layout_display("inline").unwrap(),
            LayoutDisplay::Inline
        );
        assert_eq!(
            parse_layout_display("inline-block").unwrap(),
            LayoutDisplay::InlineBlock
        );
        assert_eq!(parse_layout_display("flex").unwrap(), LayoutDisplay::Flex);
        assert_eq!(
            parse_layout_display("inline-flex").unwrap(),
            LayoutDisplay::InlineFlex
        );
        assert_eq!(parse_layout_display("grid").unwrap(), LayoutDisplay::Grid);
        assert_eq!(
            parse_layout_display("inline-grid").unwrap(),
            LayoutDisplay::InlineGrid
        );
        assert_eq!(parse_layout_display("none").unwrap(), LayoutDisplay::None);
        assert_eq!(
            parse_layout_display("flow-root").unwrap(),
            LayoutDisplay::FlowRoot
        );
        assert_eq!(
            parse_layout_display("list-item").unwrap(),
            LayoutDisplay::ListItem
        );
        // Note: 'inherit' and 'initial' are handled by the CSS cascade system,
        // not as enum variants
        assert!(parse_layout_display("inherit").is_err());
        assert!(parse_layout_display("initial").is_err());

        // Table values
        assert_eq!(parse_layout_display("table").unwrap(), LayoutDisplay::Table);
        assert_eq!(
            parse_layout_display("inline-table").unwrap(),
            LayoutDisplay::InlineTable
        );
        assert_eq!(
            parse_layout_display("table-row").unwrap(),
            LayoutDisplay::TableRow
        );
        assert_eq!(
            parse_layout_display("table-cell").unwrap(),
            LayoutDisplay::TableCell
        );
        assert_eq!(
            parse_layout_display("table-caption").unwrap(),
            LayoutDisplay::TableCaption
        );
        assert_eq!(
            parse_layout_display("table-column-group").unwrap(),
            LayoutDisplay::TableColumnGroup
        );
        assert_eq!(
            parse_layout_display("table-header-group").unwrap(),
            LayoutDisplay::TableHeaderGroup
        );
        assert_eq!(
            parse_layout_display("table-footer-group").unwrap(),
            LayoutDisplay::TableFooterGroup
        );
        assert_eq!(
            parse_layout_display("table-row-group").unwrap(),
            LayoutDisplay::TableRowGroup
        );

        // Whitespace
        assert_eq!(
            parse_layout_display("  inline-flex  ").unwrap(),
            LayoutDisplay::InlineFlex
        );

        // Invalid values
        assert!(parse_layout_display("invalid-value").is_err());
        assert!(parse_layout_display("").is_err());
        assert!(parse_layout_display("display").is_err());
    }

    #[test]
    fn test_parse_layout_float() {
        assert_eq!(parse_layout_float("left").unwrap(), LayoutFloat::Left);
        assert_eq!(parse_layout_float("right").unwrap(), LayoutFloat::Right);
        assert_eq!(parse_layout_float("none").unwrap(), LayoutFloat::None);

        // Whitespace
        assert_eq!(parse_layout_float("  right  ").unwrap(), LayoutFloat::Right);

        // Invalid values
        assert!(parse_layout_float("center").is_err());
        assert!(parse_layout_float("").is_err());
        assert!(parse_layout_float("float-left").is_err());
    }
}

#[cfg(all(test, feature = "parser"))]
mod autotest_generated {
    use super::*;

    /// Every `LayoutDisplay` variant. Kept honest by [`display_variant_index`],
    /// whose exhaustive match stops compiling when a variant is added.
    const ALL_DISPLAY: [LayoutDisplay; 23] = [
        LayoutDisplay::None,
        LayoutDisplay::Block,
        LayoutDisplay::Inline,
        LayoutDisplay::InlineBlock,
        LayoutDisplay::Flex,
        LayoutDisplay::InlineFlex,
        LayoutDisplay::Table,
        LayoutDisplay::InlineTable,
        LayoutDisplay::TableRowGroup,
        LayoutDisplay::TableHeaderGroup,
        LayoutDisplay::TableFooterGroup,
        LayoutDisplay::TableRow,
        LayoutDisplay::TableColumnGroup,
        LayoutDisplay::TableColumn,
        LayoutDisplay::TableCell,
        LayoutDisplay::TableCaption,
        LayoutDisplay::FlowRoot,
        LayoutDisplay::ListItem,
        LayoutDisplay::RunIn,
        LayoutDisplay::Marker,
        LayoutDisplay::Grid,
        LayoutDisplay::InlineGrid,
        LayoutDisplay::Contents,
    ];

    const ALL_FLOAT: [LayoutFloat; 3] = [LayoutFloat::Left, LayoutFloat::Right, LayoutFloat::None];

    /// Position of each variant inside `ALL_DISPLAY`. The match is deliberately
    /// exhaustive (no `_` arm) so a new `LayoutDisplay` variant is a compile
    /// error here rather than a silently untested variant below.
    const fn display_variant_index(d: LayoutDisplay) -> usize {
        match d {
            LayoutDisplay::None => 0,
            LayoutDisplay::Block => 1,
            LayoutDisplay::Inline => 2,
            LayoutDisplay::InlineBlock => 3,
            LayoutDisplay::Flex => 4,
            LayoutDisplay::InlineFlex => 5,
            LayoutDisplay::Table => 6,
            LayoutDisplay::InlineTable => 7,
            LayoutDisplay::TableRowGroup => 8,
            LayoutDisplay::TableHeaderGroup => 9,
            LayoutDisplay::TableFooterGroup => 10,
            LayoutDisplay::TableRow => 11,
            LayoutDisplay::TableColumnGroup => 12,
            LayoutDisplay::TableColumn => 13,
            LayoutDisplay::TableCell => 14,
            LayoutDisplay::TableCaption => 15,
            LayoutDisplay::FlowRoot => 16,
            LayoutDisplay::ListItem => 17,
            LayoutDisplay::RunIn => 18,
            LayoutDisplay::Marker => 19,
            LayoutDisplay::Grid => 20,
            LayoutDisplay::InlineGrid => 21,
            LayoutDisplay::Contents => 22,
        }
    }

    // -----------------------------------------------------------------
    // Coverage guards
    // -----------------------------------------------------------------

    #[test]
    fn all_display_lists_every_variant_exactly_once() {
        for (i, d) in ALL_DISPLAY.iter().enumerate() {
            assert_eq!(
                display_variant_index(*d),
                i,
                "ALL_DISPLAY is out of sync at index {i} ({d:?})"
            );
        }
    }

    // -----------------------------------------------------------------
    // creates_block_context / creates_flex_context / creates_table_context
    // -----------------------------------------------------------------

    #[test]
    fn creates_block_context_matches_an_exact_variant_set() {
        // NOTE (spec deviation, pinned as *current* behaviour): CSS 2.1 §9.4.1
        // also gives inline-blocks, table-cells and table-captions a new block
        // formatting context, and flex/grid containers establish flex/grid
        // formatting contexts rather than block ones. This implementation uses
        // the narrower set below.
        const EXPECTED: [LayoutDisplay; 6] = [
            LayoutDisplay::Block,
            LayoutDisplay::FlowRoot,
            LayoutDisplay::Flex,
            LayoutDisplay::Grid,
            LayoutDisplay::Table,
            LayoutDisplay::ListItem,
        ];
        for d in ALL_DISPLAY {
            assert_eq!(
                d.creates_block_context(),
                EXPECTED.contains(&d),
                "creates_block_context({d:?})"
            );
        }
    }

    #[test]
    fn creates_flex_context_matches_an_exact_variant_set() {
        const EXPECTED: [LayoutDisplay; 2] = [LayoutDisplay::Flex, LayoutDisplay::InlineFlex];
        for d in ALL_DISPLAY {
            assert_eq!(
                d.creates_flex_context(),
                EXPECTED.contains(&d),
                "creates_flex_context({d:?})"
            );
        }
    }

    #[test]
    fn creates_table_context_matches_an_exact_variant_set() {
        // Only the table *wrapper* boxes establish a table formatting context;
        // the layout-internal boxes (rows, cells, ...) participate in one.
        const EXPECTED: [LayoutDisplay; 2] = [LayoutDisplay::Table, LayoutDisplay::InlineTable];
        for d in ALL_DISPLAY {
            assert_eq!(
                d.creates_table_context(),
                EXPECTED.contains(&d),
                "creates_table_context({d:?})"
            );
        }
    }

    // -----------------------------------------------------------------
    // is_layout_internal / is_inline_level
    // -----------------------------------------------------------------

    #[test]
    fn is_layout_internal_matches_css_display_3_table_internals() {
        const EXPECTED: [LayoutDisplay; 8] = [
            LayoutDisplay::TableRowGroup,
            LayoutDisplay::TableHeaderGroup,
            LayoutDisplay::TableFooterGroup,
            LayoutDisplay::TableRow,
            LayoutDisplay::TableColumnGroup,
            LayoutDisplay::TableColumn,
            LayoutDisplay::TableCell,
            LayoutDisplay::TableCaption,
        ];
        for d in ALL_DISPLAY {
            assert_eq!(
                d.is_layout_internal(),
                EXPECTED.contains(&d),
                "is_layout_internal({d:?})"
            );
        }
        // The table wrappers themselves are *not* layout-internal.
        assert!(!LayoutDisplay::Table.is_layout_internal());
        assert!(!LayoutDisplay::InlineTable.is_layout_internal());
    }

    #[test]
    fn is_inline_level_matches_an_exact_variant_set() {
        const EXPECTED: [LayoutDisplay; 5] = [
            LayoutDisplay::Inline,
            LayoutDisplay::InlineBlock,
            LayoutDisplay::InlineFlex,
            LayoutDisplay::InlineTable,
            LayoutDisplay::InlineGrid,
        ];
        for d in ALL_DISPLAY {
            assert_eq!(
                d.is_inline_level(),
                EXPECTED.contains(&d),
                "is_inline_level({d:?})"
            );
        }
        // Inner-only keywords default their outer display to block.
        assert!(!LayoutDisplay::Flex.is_inline_level());
        assert!(!LayoutDisplay::Grid.is_inline_level());
        assert!(!LayoutDisplay::Table.is_inline_level());
        assert!(!LayoutDisplay::FlowRoot.is_inline_level());
    }

    // -----------------------------------------------------------------
    // Cross-predicate invariants
    // -----------------------------------------------------------------

    #[test]
    fn flex_and_table_contexts_are_mutually_exclusive() {
        for d in ALL_DISPLAY {
            assert!(
                !(d.creates_flex_context() && d.creates_table_context()),
                "{d:?} claims to establish both a flex and a table formatting context"
            );
        }
    }

    #[test]
    fn layout_internal_and_inline_level_are_mutually_exclusive() {
        for d in ALL_DISPLAY {
            assert!(
                !(d.is_layout_internal() && d.is_inline_level()),
                "{d:?} is both layout-internal and inline-level"
            );
        }
    }

    #[test]
    fn boxless_displays_establish_and_generate_nothing() {
        // `display: none` generates no box at all; `display: contents` generates
        // no box for itself, so neither may establish any formatting context nor
        // count as an inline-level or layout-internal box.
        for d in [LayoutDisplay::None, LayoutDisplay::Contents] {
            assert!(!d.creates_block_context(), "{d:?}");
            assert!(!d.creates_flex_context(), "{d:?}");
            assert!(!d.creates_table_context(), "{d:?}");
            assert!(!d.is_layout_internal(), "{d:?}");
            assert!(!d.is_inline_level(), "{d:?}");
        }
    }

    #[test]
    fn predicates_on_the_default_instance_do_not_panic() {
        // `Default` is `block`: a block-level box establishing a block context.
        let d = LayoutDisplay::default();
        assert_eq!(d, LayoutDisplay::Block);
        assert!(d.creates_block_context());
        assert!(!d.creates_flex_context());
        assert!(!d.creates_table_context());
        assert!(!d.is_layout_internal());
        assert!(!d.is_inline_level());

        assert_eq!(LayoutFloat::default(), LayoutFloat::None);
    }

    #[test]
    fn predicates_are_pure_and_repeatable() {
        for d in ALL_DISPLAY {
            let snapshot = (
                d.creates_block_context(),
                d.creates_flex_context(),
                d.creates_table_context(),
                d.is_layout_internal(),
                d.is_inline_level(),
            );
            for _ in 0..4 {
                assert_eq!(
                    (
                        d.creates_block_context(),
                        d.creates_flex_context(),
                        d.creates_table_context(),
                        d.is_layout_internal(),
                        d.is_inline_level(),
                    ),
                    snapshot,
                    "predicates are not deterministic for {d:?}"
                );
            }
        }
    }

    #[test]
    fn predicates_are_usable_in_const_context() {
        // All five predicates are `const fn`; regressing that is a breaking
        // change for downstream `const` tables.
        const BLOCK_CTX: bool = LayoutDisplay::FlowRoot.creates_block_context();
        const FLEX_CTX: bool = LayoutDisplay::InlineFlex.creates_flex_context();
        const TABLE_CTX: bool = LayoutDisplay::InlineTable.creates_table_context();
        const INTERNAL: bool = LayoutDisplay::TableCell.is_layout_internal();
        const INLINE: bool = LayoutDisplay::InlineGrid.is_inline_level();
        assert!(BLOCK_CTX && FLEX_CTX && TABLE_CTX && INTERNAL && INLINE);
    }

    // -----------------------------------------------------------------
    // Round-trip: print_as_css_value <-> parse
    // -----------------------------------------------------------------

    #[test]
    fn display_round_trips_through_its_css_serialization() {
        for d in ALL_DISPLAY {
            let printed = d.print_as_css_value();
            assert_eq!(
                parse_layout_display(&printed),
                Ok(d),
                "round-trip failed for {d:?} (printed as {printed:?})"
            );
        }
    }

    #[test]
    fn float_round_trips_through_its_css_serialization() {
        for f in ALL_FLOAT {
            let printed = f.print_as_css_value();
            assert_eq!(
                parse_layout_float(&printed),
                Ok(f),
                "round-trip failed for {f:?} (printed as {printed:?})"
            );
        }
    }

    #[test]
    fn display_serializations_are_unique_bare_idents() {
        for (i, a) in ALL_DISPLAY.iter().enumerate() {
            let printed = a.print_as_css_value();
            assert!(!printed.is_empty(), "{a:?} serializes to the empty string");
            assert_eq!(printed.trim(), printed, "{a:?} serializes with padding");
            assert!(
                printed
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c == '-'),
                "{a:?} serializes to a non-ident {printed:?}"
            );
            // A duplicate keyword would make the round-trip above lossy.
            for b in &ALL_DISPLAY[i + 1..] {
                assert_ne!(
                    printed,
                    b.print_as_css_value(),
                    "{a:?} and {b:?} share a CSS keyword"
                );
            }
        }
    }

    #[test]
    fn parse_then_print_is_idempotent() {
        for keyword in [
            "none",
            "block",
            "inline",
            "inline-block",
            "flex",
            "inline-flex",
            "table",
            "inline-table",
            "table-row-group",
            "table-header-group",
            "table-footer-group",
            "table-row",
            "table-column-group",
            "table-column",
            "table-cell",
            "table-caption",
            "list-item",
            "run-in",
            "marker",
            "grid",
            "inline-grid",
            "flow-root",
            "contents",
        ] {
            let parsed = parse_layout_display(keyword)
                .unwrap_or_else(|e| panic!("{keyword:?} must parse, got {e}"));
            assert_eq!(parsed.print_as_css_value(), keyword);
        }
        for keyword in ["left", "right", "none"] {
            let parsed = parse_layout_float(keyword)
                .unwrap_or_else(|e| panic!("{keyword:?} must parse, got {e}"));
            assert_eq!(parsed.print_as_css_value(), keyword);
        }
    }

    // -----------------------------------------------------------------
    // parse_layout_display / parse_layout_float: malformed input
    // -----------------------------------------------------------------

    #[test]
    fn parsers_reject_empty_and_whitespace_only_input() {
        for blank in ["", " ", "   ", "\t", "\n", "\r\n", "\t\n\r ", "\u{c}", "\u{b}"] {
            assert!(
                parse_layout_display(blank).is_err(),
                "display accepted blank {blank:?}"
            );
            assert!(
                parse_layout_float(blank).is_err(),
                "float accepted blank {blank:?}"
            );
        }
        // The error carries the *trimmed* input, so a blank input reports "".
        assert_eq!(
            parse_layout_display("   "),
            Err(LayoutDisplayParseError::InvalidValue(""))
        );
        assert_eq!(
            parse_layout_float("\t\n"),
            Err(LayoutFloatParseError::InvalidValue(""))
        );
    }

    #[test]
    fn display_parser_rejects_garbage_without_panicking() {
        for bad in [
            "invalid-value",
            "display",
            "display: block",
            "block;",
            "block ;",
            "block!important",
            "block block",
            "inline block",
            "inline_block",
            "inline--block",
            "-inline-block",
            "inline-block-",
            "-webkit-box",
            "block flow",     // CSS Display 3 two-value syntax is not supported
            "inline flow-root",
            "table-column-groups",
            "tablerow",
            "\0",
            "blo\0ck",
            "block\0",
            "inherit",
            "initial",
            "unset",
            "revert",
            "\\62 lock", // CSS ident escape
            "/*block*/",
            "\"block\"",
        ] {
            assert!(
                parse_layout_display(bad).is_err(),
                "display accepted garbage {bad:?}"
            );
        }
    }

    #[test]
    fn float_parser_rejects_garbage_without_panicking() {
        for bad in [
            "center",
            "float-left",
            "left right",
            "leftright",
            "inline-start",
            "inline-end",
            "left;",
            "left!important",
            "\0",
            "le\0ft",
            "inherit",
            "initial",
            "footnote",
        ] {
            assert!(
                parse_layout_float(bad).is_err(),
                "float accepted garbage {bad:?}"
            );
        }
    }

    #[test]
    fn parsers_are_ascii_case_sensitive() {
        // NOTE (spec deviation, pinned as *current* behaviour): CSS keywords are
        // ASCII case-insensitive, so `display: BLOCK` is valid CSS. These
        // parsers match exactly; the requirement asserted here is only that they
        // reject deterministically instead of panicking.
        for bad in ["BLOCK", "Block", "bLoCk", "INLINE-FLEX", "Table-Cell"] {
            assert!(
                parse_layout_display(bad).is_err(),
                "display unexpectedly accepted {bad:?}"
            );
        }
        for bad in ["LEFT", "Left", "RIGHT", "None"] {
            assert!(
                parse_layout_float(bad).is_err(),
                "float unexpectedly accepted {bad:?}"
            );
        }
    }

    #[test]
    fn parsers_trim_leading_and_trailing_whitespace_but_not_junk() {
        assert_eq!(
            parse_layout_display("  inline-flex  "),
            Ok(LayoutDisplay::InlineFlex)
        );
        assert_eq!(
            parse_layout_display("\n\t table-cell \r\n"),
            Ok(LayoutDisplay::TableCell)
        );
        assert_eq!(parse_layout_float("\n right \t"), Ok(LayoutFloat::Right));

        // Interior whitespace and trailing junk are *not* forgiven.
        assert!(parse_layout_display("inline - flex").is_err());
        assert!(parse_layout_display("table-cell;garbage").is_err());
        assert!(parse_layout_float("right;garbage").is_err());
    }

    #[test]
    fn parsers_trim_unicode_whitespace_but_not_zero_width_characters() {
        // `str::trim` uses the Unicode `White_Space` property, a strict superset
        // of CSS whitespace (space, tab, LF, CR, FF). NBSP / ideographic space /
        // line separator therefore pass as padding - pinned as current behaviour.
        assert_eq!(
            parse_layout_display("\u{a0}block\u{a0}"),
            Ok(LayoutDisplay::Block)
        );
        assert_eq!(
            parse_layout_display("\u{3000}flex\u{2028}"),
            Ok(LayoutDisplay::Flex)
        );
        assert_eq!(parse_layout_float("\u{a0}left\u{a0}"), Ok(LayoutFloat::Left));

        // U+200B ZERO WIDTH SPACE and U+FEFF are *not* `White_Space`, so they
        // survive the trim and must make the value invalid.
        assert!(parse_layout_display("\u{200b}block").is_err());
        assert!(parse_layout_display("block\u{feff}").is_err());
        assert!(parse_layout_float("\u{200b}left").is_err());
    }

    #[test]
    fn parsers_survive_non_ascii_and_multibyte_input() {
        for bad in [
            "\u{1F600}",
            "block\u{1F600}",
            "\u{1F3F3}\u{FE0F}\u{200D}\u{1F308}", // ZWJ emoji sequence
            "blocke\u{301}",                      // combining acute accent
            "\u{202E}block",                      // RTL override
            "ｂｌｏｃｋ",                          // fullwidth latin
            "блок",
            "块",
            "\u{0}\u{1}\u{2}",
            "\u{10FFFF}",
        ] {
            assert!(
                parse_layout_display(bad).is_err(),
                "display accepted unicode garbage {bad:?}"
            );
            assert!(
                parse_layout_float(bad).is_err(),
                "float accepted unicode garbage {bad:?}"
            );
        }
    }

    #[test]
    fn parsers_reject_boundary_numeric_strings() {
        let numeric = [
            "0".to_string(),
            "-0".to_string(),
            "+0".to_string(),
            "1".to_string(),
            "-1".to_string(),
            i64::MAX.to_string(),
            i64::MIN.to_string(),
            u64::MAX.to_string(),
            format!("{}", f64::MAX),
            format!("{}", f64::MIN_POSITIVE),
            "NaN".to_string(),
            "nan".to_string(),
            "inf".to_string(),
            "-inf".to_string(),
            "infinity".to_string(),
            "1e400".to_string(),
            "-1e-400".to_string(),
            "0x7fffffffffffffff".to_string(),
            "99999999999999999999999999999999".to_string(),
        ];
        for n in &numeric {
            assert!(
                parse_layout_display(n).is_err(),
                "display accepted number {n:?}"
            );
            assert!(parse_layout_float(n).is_err(), "float accepted number {n:?}");
        }
    }

    #[test]
    fn parsers_do_not_hang_on_extremely_long_input() {
        const LONG: usize = 1_000_000;

        let junk = "x".repeat(LONG);
        assert!(parse_layout_display(&junk).is_err());
        assert!(parse_layout_float(&junk).is_err());

        // A keyword that is *almost* right, one megabyte long.
        let near_miss = format!("block{}", "k".repeat(LONG));
        assert!(parse_layout_display(&near_miss).is_err());

        // Whitespace-only, one megabyte long: trims to "" and must still be Err.
        let blanks = " ".repeat(LONG);
        assert!(parse_layout_display(&blanks).is_err());
        assert!(parse_layout_float(&blanks).is_err());

        // A valid keyword buried in a megabyte of padding on each side.
        let padded = format!("{blanks}table-caption{blanks}");
        assert_eq!(
            parse_layout_display(&padded),
            Ok(LayoutDisplay::TableCaption)
        );
        let padded_float = format!("{blanks}left{blanks}");
        assert_eq!(parse_layout_float(&padded_float), Ok(LayoutFloat::Left));
    }

    #[test]
    fn parsers_do_not_stack_overflow_on_deeply_nested_input() {
        const DEPTH: usize = 10_000;
        let nested = format!("{}{}", "(".repeat(DEPTH), ")".repeat(DEPTH));
        assert!(parse_layout_display(&nested).is_err());
        assert!(parse_layout_float(&nested).is_err());

        let nested_fn = format!("{}block{}", "calc(".repeat(DEPTH), ")".repeat(DEPTH));
        assert!(parse_layout_display(&nested_fn).is_err());
    }

    // -----------------------------------------------------------------
    // Parse errors: payload, formatting, to_contained / to_shared
    // -----------------------------------------------------------------

    #[test]
    fn display_error_reports_the_trimmed_input() {
        assert_eq!(
            parse_layout_display("  bogus  "),
            Err(LayoutDisplayParseError::InvalidValue("bogus"))
        );
        // Unicode padding is trimmed off the reported value as well.
        assert_eq!(
            parse_layout_display("\u{a0}bogus\u{3000}"),
            Err(LayoutDisplayParseError::InvalidValue("bogus"))
        );
    }

    #[test]
    fn float_error_reports_the_trimmed_input() {
        assert_eq!(
            parse_layout_float("  bogus  "),
            Err(LayoutFloatParseError::InvalidValue("bogus"))
        );
    }

    #[test]
    fn parse_errors_display_and_debug_identically() {
        let d = parse_layout_display("bogus").unwrap_err();
        assert_eq!(format!("{d}"), "Invalid display value: \"bogus\"");
        assert_eq!(format!("{d:?}"), format!("{d}"));

        let f = parse_layout_float("bogus").unwrap_err();
        assert_eq!(format!("{f}"), "Invalid float value: \"bogus\"");
        assert_eq!(format!("{f:?}"), format!("{f}"));

        // An empty payload must still format without panicking.
        let empty = parse_layout_display("").unwrap_err();
        assert_eq!(format!("{empty}"), "Invalid display value: \"\"");
    }

    #[test]
    fn display_error_to_contained_to_shared_round_trips() {
        let long = "x".repeat(4096);
        let payloads: [&str; 8] = [
            "",
            " ",
            "bogus",
            "\u{1F600}",
            "a\0b",
            "block block",
            "\"quoted\"",
            &long,
        ];
        for payload in payloads {
            let shared = LayoutDisplayParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                LayoutDisplayParseErrorOwned::InvalidValue(payload.to_string().into()),
                "to_contained lost the payload {payload:?}"
            );
            assert_eq!(
                owned.to_shared(),
                shared,
                "to_shared did not restore the payload {payload:?}"
            );
            // Byte-for-byte, not just "equal enough".
            let LayoutDisplayParseErrorOwned::InvalidValue(s) = &owned;
            assert_eq!(s.as_str(), payload);
        }
    }

    #[test]
    fn float_error_to_contained_to_shared_round_trips() {
        let long = "y".repeat(4096);
        let payloads: [&str; 6] = ["", " ", "bogus", "\u{1F600}", "a\0b", &long];
        for payload in payloads {
            let shared = LayoutFloatParseError::InvalidValue(payload);
            let owned = shared.to_contained();
            assert_eq!(
                owned,
                LayoutFloatParseErrorOwned::InvalidValue(payload.to_string().into()),
                "to_contained lost the payload {payload:?}"
            );
            assert_eq!(
                owned.to_shared(),
                shared,
                "to_shared did not restore the payload {payload:?}"
            );
            let LayoutFloatParseErrorOwned::InvalidValue(s) = &owned;
            assert_eq!(s.as_str(), payload);
        }
    }

    #[test]
    fn error_conversions_survive_a_borrowed_owned_borrowed_cycle() {
        // The owned form must outlive the &str it came from; converting back
        // must not resurrect a dangling borrow.
        let owned = {
            let input = format!("{}-bogus", "very-long-".repeat(64));
            parse_layout_display(&input).unwrap_err().to_contained()
        };
        let shared = owned.to_shared();
        assert_eq!(shared.to_contained(), owned);
        assert!(format!("{shared}").starts_with("Invalid display value: \"very-long-"));
    }

    // -----------------------------------------------------------------
    // Positive controls
    // -----------------------------------------------------------------

    #[test]
    fn valid_minimal_inputs_parse() {
        assert_eq!(parse_layout_display("none"), Ok(LayoutDisplay::None));
        assert_eq!(parse_layout_display("contents"), Ok(LayoutDisplay::Contents));
        assert_eq!(parse_layout_display("run-in"), Ok(LayoutDisplay::RunIn));
        assert_eq!(parse_layout_display("marker"), Ok(LayoutDisplay::Marker));
        assert_eq!(parse_layout_float("none"), Ok(LayoutFloat::None));
        assert_eq!(parse_layout_float("left"), Ok(LayoutFloat::Left));
    }
}

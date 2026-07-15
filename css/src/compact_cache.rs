//! Compact layout property cache — three-tier numeric encoding
//!
//! Replaces BTreeMap-based CSS property lookups with cache-friendly arrays.
//!
//! - **Tier 1**: `Vec<u64>` — ALL 21 enum properties bitpacked (8 B/node)
//! - **Tier 2 hot**: `Vec<CompactNodeProps>` — layout-critical numeric dimensions (68 B/node)
//! - **Tier 2 cold**: `Vec<CompactNodePropsCold>` — paint-only properties (28 B/node)
//! - **Tier 2b**: `Vec<CompactTextProps>` — text/IFC properties (24 B/node)
//!
//! Non-compact properties (background, box-shadow, transform, etc.) are
//! resolved via the slow cascade path in `CssPropertyCache::get_property_slow()`.

// The `*_from_u8` decoders below intentionally give an explicit arm for the byte
// that maps to each enum's default (e.g. `0 => Block`) even though the `_`
// catch-all returns the same default — this keeps the decode table a 1:1 mirror
// of the `*_to_u8` encoders. clippy::match_same_arms flags those explicit arms as
// duplicates of `_`; merging them would drop the encoding documentation, so allow
// it for this codec module (false positive for the intent here).
#![allow(clippy::match_same_arms)]

use crate::props::basic::length::{FloatValue, SizeMetric};
use crate::props::basic::pixel::PixelValue;
use crate::props::layout::{
    display::LayoutDisplay,
    dimensions::{LayoutHeight, LayoutWidth, LayoutMaxHeight, LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth},
    flex::{
        LayoutAlignContent, LayoutAlignItems, LayoutAlignSelf, LayoutFlexDirection, LayoutFlexWrap,
        LayoutJustifyContent,
    },
    grid::{LayoutGridAutoFlow, LayoutJustifySelf, LayoutJustifyItems},
    overflow::LayoutOverflow,
    position::LayoutPosition,
    wrapping::{LayoutClear, LayoutWritingMode},
    table::StyleBorderCollapse,
};
use crate::props::layout::display::LayoutFloat;
use crate::props::layout::dimensions::LayoutBoxSizing;
use crate::props::basic::font::{StyleFontStyle, StyleFontWeight};
use crate::props::basic::color::ColorU;
use crate::props::style::{StyleTextAlign, StyleVerticalAlign, StyleVisibility, StyleWhiteSpace, StyleDirection};
use crate::props::style::border::BorderStyle;
use crate::props::property::{CssProperty, CssPropertyType};
use crate::css::CssPropertyValue;
use alloc::boxed::Box;
use alloc::vec::Vec;

// =============================================================================
// Sentinel Constants
// =============================================================================

/// u16 sentinel values (for resolved-px ×10 and flex ×100)
pub const U16_SENTINEL: u16 = 0xFFFF;
/// Any u16 value >= this threshold is a sentinel, not a real value
pub const U16_SENTINEL_THRESHOLD: u16 = 0xFFF9;

/// i16 sentinel values (for signed resolved-px ×10)
pub const I16_SENTINEL: i16 = 0x7FFF;       // 32767
pub const I16_AUTO: i16 = 0x7FFE;           // 32766
pub const I16_INHERIT: i16 = 0x7FFD;        // 32765
pub const I16_INITIAL: i16 = 0x7FFC;        // 32764
/// Any i16 value >= this threshold is a sentinel
pub const I16_SENTINEL_THRESHOLD: i16 = 0x7FFC; // 32764

/// u32 sentinel values (for dimension properties with unit info)
pub const U32_SENTINEL: u32 = 0xFFFF_FFFF;
pub const U32_AUTO: u32 = 0xFFFF_FFFE;
pub const U32_NONE: u32 = 0xFFFF_FFFD;
pub const U32_INHERIT: u32 = 0xFFFF_FFFC;
pub const U32_INITIAL: u32 = 0xFFFF_FFFB;
pub const U32_MIN_CONTENT: u32 = 0xFFFF_FFFA;
pub const U32_MAX_CONTENT: u32 = 0xFFFF_FFF9;
/// Any u32 value >= this threshold is a sentinel
pub const U32_SENTINEL_THRESHOLD: u32 = 0xFFFF_FFF9;

// =============================================================================
// Tier 1: u64 bitfield — ALL enum properties
// =============================================================================
//
// Bit layout (52 bits used, 12 spare):
//   [4:0]    display          5 bits  (22 variants)
//   [7:5]    position         3 bits  (5 variants)
//   [9:8]    float            2 bits  (3 variants)
//   [12:10]  overflow_x       3 bits  (5 variants)
//   [15:13]  overflow_y       3 bits  (5 variants)
//   [16]     box_sizing       1 bit   (2 variants)
//   [18:17]  flex_direction   2 bits  (4 variants)
//   [20:19]  flex_wrap        2 bits  (3 variants)
//   [23:21]  justify_content  3 bits  (8 variants)
//   [26:24]  align_items      3 bits  (5 variants)
//   [29:27]  align_content    3 bits  (6 variants)
//   [31:30]  writing_mode     2 bits  (3 variants)
//   [33:32]  clear            2 bits  (4 variants)
//   [37:34]  font_weight      4 bits  (11 variants)
//   [39:38]  font_style       2 bits  (3 variants)
//   [42:40]  text_align       3 bits  (6 variants)
//   [44:43]  visibility       2 bits  (3 variants)
//   [47:45]  white_space      3 bits  (6 variants)
//   [48]     direction        1 bit   (2 variants)
//   [51:49]  vertical_align   3 bits  (8 variants)
//   [52]     border_collapse  1 bit   (2 variants)
//   [63:53]  (spare / sentinel flags)

// Bit offsets within u64
pub const DISPLAY_SHIFT: u32 = 0;
pub const POSITION_SHIFT: u32 = 5;
pub const FLOAT_SHIFT: u32 = 8;
pub const OVERFLOW_X_SHIFT: u32 = 10;
pub const OVERFLOW_Y_SHIFT: u32 = 13;
pub const BOX_SIZING_SHIFT: u32 = 16;
pub const FLEX_DIRECTION_SHIFT: u32 = 17;
pub const FLEX_WRAP_SHIFT: u32 = 19;
pub const JUSTIFY_CONTENT_SHIFT: u32 = 21;
pub const ALIGN_ITEMS_SHIFT: u32 = 24;
pub const ALIGN_CONTENT_SHIFT: u32 = 27;
pub const WRITING_MODE_SHIFT: u32 = 30;
pub const CLEAR_SHIFT: u32 = 32;
pub const FONT_WEIGHT_SHIFT: u32 = 34;
pub const FONT_STYLE_SHIFT: u32 = 38;
pub const TEXT_ALIGN_SHIFT: u32 = 40;
pub const VISIBILITY_SHIFT: u32 = 43;
pub const WHITE_SPACE_SHIFT: u32 = 45;
pub const DIRECTION_SHIFT: u32 = 48;
pub const VERTICAL_ALIGN_SHIFT: u32 = 49;
pub const BORDER_COLLAPSE_SHIFT: u32 = 52;

// Bit masks
pub const DISPLAY_MASK: u64 = 0x1F;     // 5 bits
pub const POSITION_MASK: u64 = 0x07;    // 3 bits
pub const FLOAT_MASK: u64 = 0x03;       // 2 bits
pub const OVERFLOW_MASK: u64 = 0x07;    // 3 bits
pub const BOX_SIZING_MASK: u64 = 0x01;  // 1 bit
pub const FLEX_DIR_MASK: u64 = 0x03;    // 2 bits
pub const FLEX_WRAP_MASK: u64 = 0x03;   // 2 bits
pub const JUSTIFY_MASK: u64 = 0x07;     // 3 bits
pub const ALIGN_MASK: u64 = 0x07;       // 3 bits
pub const WRITING_MODE_MASK: u64 = 0x03;// 2 bits
pub const CLEAR_MASK: u64 = 0x03;       // 2 bits
pub const FONT_WEIGHT_MASK: u64 = 0x0F; // 4 bits
pub const FONT_STYLE_MASK: u64 = 0x03;  // 2 bits
pub const TEXT_ALIGN_MASK: u64 = 0x07;  // 3 bits
pub const VISIBILITY_MASK: u64 = 0x03;  // 2 bits
pub const WHITE_SPACE_MASK: u64 = 0x07; // 3 bits
pub const DIRECTION_MASK: u64 = 0x01;   // 1 bit
pub const VERTICAL_ALIGN_MASK: u64 = 0x07; // 3 bits
pub const BORDER_COLLAPSE_MASK: u64 = 0x01; // 1 bit

pub const ALIGN_SELF_SHIFT: u32 = 53;
pub const JUSTIFY_SELF_SHIFT: u32 = 56;
pub const GRID_AUTO_FLOW_SHIFT: u32 = 59;
pub const JUSTIFY_ITEMS_SHIFT: u32 = 61;
pub const ALIGN_SELF_MASK: u64 = 0x07;     // 3 bits
pub const JUSTIFY_SELF_MASK: u64 = 0x07;   // 3 bits
pub const GRID_AUTO_FLOW_MASK: u64 = 0x03; // 2 bits (row/col × dense)
pub const JUSTIFY_ITEMS_MASK: u64 = 0x03;  // 2 bits (start/center/end/stretch)

/// Special value stored in the spare bits [63:51] to indicate this node has
/// NO tier-1 data (i.e., all defaults).
///
/// 0 is a valid all-defaults encoding,
/// so we use bit 63 as a "tier1 populated" flag. If bit 63 is 0 and all other
/// bits are 0, it means "all defaults" (`Display::Block`, `Position::Static`, etc.).
/// We set bit 63 = 1 to mark that the node HAS been populated.
pub const TIER1_POPULATED_BIT: u64 = 1 << 63;

// =============================================================================
// Safe from_u8 conversion functions (no transmute!)
// =============================================================================

/// Decode display from u8. **0 = Block** (most common HTML default).
/// Value 31 (0x1F) = sentinel: look up in slow path for uncommon values.
/// Returns default (Block) on invalid input.
#[inline]
#[must_use] pub const fn layout_display_from_u8(v: u8) -> LayoutDisplay {
    match v {
        0 => LayoutDisplay::Block,        // default when bits are 0
        1 => LayoutDisplay::Inline,
        2 => LayoutDisplay::InlineBlock,
        3 => LayoutDisplay::Flex,
        4 => LayoutDisplay::None,
        5 => LayoutDisplay::InlineFlex,
        6 => LayoutDisplay::Table,
        7 => LayoutDisplay::InlineTable,
        8 => LayoutDisplay::TableRowGroup,
        9 => LayoutDisplay::TableHeaderGroup,
        10 => LayoutDisplay::TableFooterGroup,
        11 => LayoutDisplay::TableRow,
        12 => LayoutDisplay::TableColumnGroup,
        13 => LayoutDisplay::TableColumn,
        14 => LayoutDisplay::TableCell,
        15 => LayoutDisplay::TableCaption,
        16 => LayoutDisplay::FlowRoot,
        17 => LayoutDisplay::ListItem,
        18 => LayoutDisplay::RunIn,
        19 => LayoutDisplay::Marker,
        20 => LayoutDisplay::Grid,
        21 => LayoutDisplay::InlineGrid,
        22 => LayoutDisplay::Contents,
        _ => LayoutDisplay::Block, // fallback + sentinel (31)
    }
}

/// Encode display to u8. **0 = Block** (most common HTML default).
#[inline]
#[must_use] pub const fn layout_display_to_u8(v: LayoutDisplay) -> u8 {
    match v {
        LayoutDisplay::Block => 0,         // 0 = default when bits unset
        LayoutDisplay::Inline => 1,
        LayoutDisplay::InlineBlock => 2,
        LayoutDisplay::Flex => 3,
        LayoutDisplay::None => 4,
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

#[inline]
#[must_use] pub const fn layout_position_from_u8(v: u8) -> LayoutPosition {
    match v {
        0 => LayoutPosition::Static,
        1 => LayoutPosition::Relative,
        2 => LayoutPosition::Absolute,
        3 => LayoutPosition::Fixed,
        4 => LayoutPosition::Sticky,
        _ => LayoutPosition::Static,
    }
}

#[inline]
#[must_use] pub const fn layout_position_to_u8(v: LayoutPosition) -> u8 {
    match v {
        LayoutPosition::Static => 0,
        LayoutPosition::Relative => 1,
        LayoutPosition::Absolute => 2,
        LayoutPosition::Fixed => 3,
        LayoutPosition::Sticky => 4,
    }
}

/// Decode float from u8. **0 = None** (CSS initial value).
#[inline]
#[must_use] pub const fn layout_float_from_u8(v: u8) -> LayoutFloat {
    match v {
        0 => LayoutFloat::None,            // default when bits unset
        1 => LayoutFloat::Left,
        2 => LayoutFloat::Right,
        _ => LayoutFloat::None,
    }
}

/// Encode float to u8. **0 = None** (CSS initial value).
#[inline]
#[must_use] pub const fn layout_float_to_u8(v: LayoutFloat) -> u8 {
    match v {
        LayoutFloat::None => 0,
        LayoutFloat::Left => 1,
        LayoutFloat::Right => 2,
    }
}

/// Decode overflow from u8. **0 = Visible** (CSS initial value).
#[inline]
#[must_use] pub const fn layout_overflow_from_u8(v: u8) -> LayoutOverflow {
    match v {
        0 => LayoutOverflow::Visible,      // default when bits unset
        1 => LayoutOverflow::Hidden,
        2 => LayoutOverflow::Scroll,
        3 => LayoutOverflow::Auto,
        4 => LayoutOverflow::Clip,
        _ => LayoutOverflow::Visible,
    }
}

/// Encode overflow to u8. **0 = Visible** (CSS initial value).
#[inline]
#[must_use] pub const fn layout_overflow_to_u8(v: LayoutOverflow) -> u8 {
    match v {
        LayoutOverflow::Visible => 0,      // 0 = default when bits unset
        LayoutOverflow::Hidden => 1,
        LayoutOverflow::Scroll => 2,
        LayoutOverflow::Auto => 3,
        LayoutOverflow::Clip => 4,
    }
}

#[inline]
#[must_use] pub const fn layout_box_sizing_from_u8(v: u8) -> LayoutBoxSizing {
    match v {
        0 => LayoutBoxSizing::ContentBox,
        1 => LayoutBoxSizing::BorderBox,
        _ => LayoutBoxSizing::ContentBox,
    }
}

#[inline]
#[must_use] pub const fn layout_box_sizing_to_u8(v: LayoutBoxSizing) -> u8 {
    match v {
        LayoutBoxSizing::ContentBox => 0,
        LayoutBoxSizing::BorderBox => 1,
    }
}

#[inline]
#[must_use] pub const fn layout_flex_direction_from_u8(v: u8) -> LayoutFlexDirection {
    match v {
        0 => LayoutFlexDirection::Row,
        1 => LayoutFlexDirection::RowReverse,
        2 => LayoutFlexDirection::Column,
        3 => LayoutFlexDirection::ColumnReverse,
        _ => LayoutFlexDirection::Row,
    }
}

#[inline]
#[must_use] pub const fn layout_flex_direction_to_u8(v: LayoutFlexDirection) -> u8 {
    match v {
        LayoutFlexDirection::Row => 0,
        LayoutFlexDirection::RowReverse => 1,
        LayoutFlexDirection::Column => 2,
        LayoutFlexDirection::ColumnReverse => 3,
    }
}

/// 0 = `NoWrap` (CSS initial value for flex-wrap)
#[inline]
#[must_use] pub const fn layout_flex_wrap_from_u8(v: u8) -> LayoutFlexWrap {
    match v {
        0 => LayoutFlexWrap::NoWrap,       // CSS initial
        1 => LayoutFlexWrap::Wrap,
        2 => LayoutFlexWrap::WrapReverse,
        _ => LayoutFlexWrap::NoWrap,
    }
}

#[inline]
#[must_use] pub const fn layout_flex_wrap_to_u8(v: LayoutFlexWrap) -> u8 {
    match v {
        LayoutFlexWrap::NoWrap => 0,
        LayoutFlexWrap::Wrap => 1,
        LayoutFlexWrap::WrapReverse => 2,
    }
}

#[inline]
#[must_use] pub const fn layout_justify_content_from_u8(v: u8) -> LayoutJustifyContent {
    match v {
        0 => LayoutJustifyContent::FlexStart,
        1 => LayoutJustifyContent::FlexEnd,
        2 => LayoutJustifyContent::Start,
        3 => LayoutJustifyContent::End,
        4 => LayoutJustifyContent::Center,
        5 => LayoutJustifyContent::SpaceBetween,
        6 => LayoutJustifyContent::SpaceAround,
        7 => LayoutJustifyContent::SpaceEvenly,
        _ => LayoutJustifyContent::FlexStart,
    }
}

#[inline]
#[must_use] pub const fn layout_justify_content_to_u8(v: LayoutJustifyContent) -> u8 {
    match v {
        LayoutJustifyContent::FlexStart => 0,
        LayoutJustifyContent::FlexEnd => 1,
        LayoutJustifyContent::Start => 2,
        LayoutJustifyContent::End => 3,
        LayoutJustifyContent::Center => 4,
        LayoutJustifyContent::SpaceBetween => 5,
        LayoutJustifyContent::SpaceAround => 6,
        LayoutJustifyContent::SpaceEvenly => 7,
    }
}

#[inline]
#[must_use] pub const fn layout_align_items_from_u8(v: u8) -> LayoutAlignItems {
    match v {
        0 => LayoutAlignItems::Stretch,
        1 => LayoutAlignItems::Center,
        2 => LayoutAlignItems::Start,
        3 => LayoutAlignItems::End,
        4 => LayoutAlignItems::Baseline,
        _ => LayoutAlignItems::Stretch,
    }
}

#[inline]
#[must_use] pub const fn layout_align_items_to_u8(v: LayoutAlignItems) -> u8 {
    match v {
        LayoutAlignItems::Stretch => 0,
        LayoutAlignItems::Center => 1,
        LayoutAlignItems::Start => 2,
        LayoutAlignItems::End => 3,
        LayoutAlignItems::Baseline => 4,
    }
}

#[inline]
#[must_use] pub const fn layout_align_self_to_u8(v: LayoutAlignSelf) -> u8 {
    match v {
        LayoutAlignSelf::Auto => 0,
        LayoutAlignSelf::Stretch => 1,
        LayoutAlignSelf::Center => 2,
        LayoutAlignSelf::Start => 3,
        LayoutAlignSelf::End => 4,
        LayoutAlignSelf::Baseline => 5,
    }
}

#[inline]
#[must_use] pub const fn layout_align_self_from_u8(v: u8) -> LayoutAlignSelf {
    match v {
        0 => LayoutAlignSelf::Auto,
        1 => LayoutAlignSelf::Stretch,
        2 => LayoutAlignSelf::Center,
        3 => LayoutAlignSelf::Start,
        4 => LayoutAlignSelf::End,
        5 => LayoutAlignSelf::Baseline,
        _ => LayoutAlignSelf::Auto,
    }
}

#[inline]
#[must_use] pub const fn layout_justify_self_to_u8(v: LayoutJustifySelf) -> u8 {
    match v {
        LayoutJustifySelf::Auto => 0,
        LayoutJustifySelf::Start => 1,
        LayoutJustifySelf::End => 2,
        LayoutJustifySelf::Center => 3,
        LayoutJustifySelf::Stretch => 4,
    }
}

#[inline]
#[must_use] pub const fn layout_justify_self_from_u8(v: u8) -> LayoutJustifySelf {
    match v {
        0 => LayoutJustifySelf::Auto,
        1 => LayoutJustifySelf::Start,
        2 => LayoutJustifySelf::End,
        3 => LayoutJustifySelf::Center,
        4 => LayoutJustifySelf::Stretch,
        _ => LayoutJustifySelf::Auto,
    }
}

// Tier1 uses 0 as the "unset" sentinel for every enum. For justify-items
// the CSS default is `normal` which behaves as `stretch` on grid items,
// so 0 must decode to Stretch (not Start). Getting this wrong leaves
// every unset grid container reporting justify-items: Start, which
// forces taffy to content-size items instead of stretching them across
// their column tracks — exactly the calc.c regression.
#[inline]
#[must_use] pub const fn layout_justify_items_to_u8(v: LayoutJustifyItems) -> u8 {
    match v {
        LayoutJustifyItems::Stretch => 0,
        LayoutJustifyItems::Start => 1,
        LayoutJustifyItems::End => 2,
        LayoutJustifyItems::Center => 3,
    }
}

#[inline]
#[must_use] pub const fn layout_justify_items_from_u8(v: u8) -> LayoutJustifyItems {
    match v {
        0 => LayoutJustifyItems::Stretch,
        1 => LayoutJustifyItems::Start,
        2 => LayoutJustifyItems::End,
        3 => LayoutJustifyItems::Center,
        _ => LayoutJustifyItems::Stretch,
    }
}

#[inline]
#[must_use] pub const fn layout_grid_auto_flow_to_u8(v: LayoutGridAutoFlow) -> u8 {
    match v {
        LayoutGridAutoFlow::Row => 0,
        LayoutGridAutoFlow::Column => 1,
        LayoutGridAutoFlow::RowDense => 2,
        LayoutGridAutoFlow::ColumnDense => 3,
    }
}

#[inline]
#[must_use] pub const fn layout_grid_auto_flow_from_u8(v: u8) -> LayoutGridAutoFlow {
    match v {
        0 => LayoutGridAutoFlow::Row,
        1 => LayoutGridAutoFlow::Column,
        2 => LayoutGridAutoFlow::RowDense,
        3 => LayoutGridAutoFlow::ColumnDense,
        _ => LayoutGridAutoFlow::Row,
    }
}

#[inline]
#[must_use] pub const fn layout_align_content_from_u8(v: u8) -> LayoutAlignContent {
    match v {
        0 => LayoutAlignContent::Stretch,
        1 => LayoutAlignContent::Center,
        2 => LayoutAlignContent::Start,
        3 => LayoutAlignContent::End,
        4 => LayoutAlignContent::SpaceBetween,
        5 => LayoutAlignContent::SpaceAround,
        _ => LayoutAlignContent::Stretch,
    }
}

#[inline]
#[must_use] pub const fn layout_align_content_to_u8(v: LayoutAlignContent) -> u8 {
    match v {
        LayoutAlignContent::Stretch => 0,
        LayoutAlignContent::Center => 1,
        LayoutAlignContent::Start => 2,
        LayoutAlignContent::End => 3,
        LayoutAlignContent::SpaceBetween => 4,
        LayoutAlignContent::SpaceAround => 5,
    }
}

#[inline]
#[must_use] pub const fn layout_writing_mode_from_u8(v: u8) -> LayoutWritingMode {
    match v {
        0 => LayoutWritingMode::HorizontalTb,
        1 => LayoutWritingMode::VerticalRl,
        2 => LayoutWritingMode::VerticalLr,
        _ => LayoutWritingMode::HorizontalTb,
    }
}

#[inline]
#[must_use] pub const fn layout_writing_mode_to_u8(v: LayoutWritingMode) -> u8 {
    match v {
        LayoutWritingMode::HorizontalTb => 0,
        LayoutWritingMode::VerticalRl => 1,
        LayoutWritingMode::VerticalLr => 2,
    }
}

#[inline]
#[must_use] pub const fn layout_clear_from_u8(v: u8) -> LayoutClear {
    match v {
        0 => LayoutClear::None,
        1 => LayoutClear::Left,
        2 => LayoutClear::Right,
        3 => LayoutClear::Both,
        _ => LayoutClear::None,
    }
}

#[inline]
#[must_use] pub const fn layout_clear_to_u8(v: LayoutClear) -> u8 {
    match v {
        LayoutClear::None => 0,
        LayoutClear::Left => 1,
        LayoutClear::Right => 2,
        LayoutClear::Both => 3,
    }
}

#[inline]
/// 0 = Normal/400 (CSS initial value for font-weight)
#[must_use] pub const fn style_font_weight_from_u8(v: u8) -> StyleFontWeight {
    match v {
        0 => StyleFontWeight::Normal,     // CSS initial (400)
        1 => StyleFontWeight::W100,
        2 => StyleFontWeight::W200,
        3 => StyleFontWeight::W300,
        4 => StyleFontWeight::W500,
        5 => StyleFontWeight::W600,
        6 => StyleFontWeight::Bold,       // 700
        7 => StyleFontWeight::W800,
        8 => StyleFontWeight::W900,
        9 => StyleFontWeight::Lighter,
        10 => StyleFontWeight::Bolder,
        _ => StyleFontWeight::Normal,
    }
}

#[inline]
/// 0 = Normal/400 (CSS initial value for font-weight)
#[must_use] pub const fn style_font_weight_to_u8(v: StyleFontWeight) -> u8 {
    match v {
        StyleFontWeight::Normal => 0,     // CSS initial (400)
        StyleFontWeight::W100 => 1,
        StyleFontWeight::W200 => 2,
        StyleFontWeight::W300 => 3,
        StyleFontWeight::W500 => 4,
        StyleFontWeight::W600 => 5,
        StyleFontWeight::Bold => 6,       // 700
        StyleFontWeight::W800 => 7,
        StyleFontWeight::W900 => 8,
        StyleFontWeight::Lighter => 9,
        StyleFontWeight::Bolder => 10,
    }
}

#[inline]
#[must_use] pub const fn style_font_style_from_u8(v: u8) -> StyleFontStyle {
    match v {
        0 => StyleFontStyle::Normal,
        1 => StyleFontStyle::Italic,
        2 => StyleFontStyle::Oblique,
        _ => StyleFontStyle::Normal,
    }
}

#[inline]
#[must_use] pub const fn style_font_style_to_u8(v: StyleFontStyle) -> u8 {
    match v {
        StyleFontStyle::Normal => 0,
        StyleFontStyle::Italic => 1,
        StyleFontStyle::Oblique => 2,
    }
}

#[inline]
#[must_use] pub const fn style_text_align_from_u8(v: u8) -> StyleTextAlign {
    match v {
        0 => StyleTextAlign::Left,
        1 => StyleTextAlign::Center,
        2 => StyleTextAlign::Right,
        3 => StyleTextAlign::Justify,
        4 => StyleTextAlign::Start,
        5 => StyleTextAlign::End,
        _ => StyleTextAlign::Left,
    }
}

#[inline]
#[must_use] pub const fn style_text_align_to_u8(v: StyleTextAlign) -> u8 {
    match v {
        StyleTextAlign::Left => 0,
        StyleTextAlign::Center => 1,
        StyleTextAlign::Right => 2,
        StyleTextAlign::Justify => 3,
        StyleTextAlign::Start => 4,
        StyleTextAlign::End => 5,
    }
}

#[inline]
#[must_use] pub const fn style_visibility_from_u8(v: u8) -> StyleVisibility {
    match v {
        0 => StyleVisibility::Visible,
        1 => StyleVisibility::Hidden,
        2 => StyleVisibility::Collapse,
        _ => StyleVisibility::Visible,
    }
}

#[inline]
#[must_use] pub const fn style_visibility_to_u8(v: StyleVisibility) -> u8 {
    match v {
        StyleVisibility::Visible => 0,
        StyleVisibility::Hidden => 1,
        StyleVisibility::Collapse => 2,
    }
}

#[inline]
#[must_use] pub const fn style_white_space_from_u8(v: u8) -> StyleWhiteSpace {
    match v {
        0 => StyleWhiteSpace::Normal,
        1 => StyleWhiteSpace::Pre,
        2 => StyleWhiteSpace::Nowrap,
        3 => StyleWhiteSpace::PreWrap,
        4 => StyleWhiteSpace::PreLine,
        5 => StyleWhiteSpace::BreakSpaces,
        _ => StyleWhiteSpace::Normal,
    }
}

#[inline]
#[must_use] pub const fn style_white_space_to_u8(v: StyleWhiteSpace) -> u8 {
    match v {
        StyleWhiteSpace::Normal => 0,
        StyleWhiteSpace::Pre => 1,
        StyleWhiteSpace::Nowrap => 2,
        StyleWhiteSpace::PreWrap => 3,
        StyleWhiteSpace::PreLine => 4,
        StyleWhiteSpace::BreakSpaces => 5,
    }
}

#[inline]
#[must_use] pub const fn style_direction_from_u8(v: u8) -> StyleDirection {
    match v {
        0 => StyleDirection::Ltr,
        1 => StyleDirection::Rtl,
        _ => StyleDirection::Ltr,
    }
}

#[inline]
#[must_use] pub const fn style_direction_to_u8(v: StyleDirection) -> u8 {
    match v {
        StyleDirection::Ltr => 0,
        StyleDirection::Rtl => 1,
    }
}

#[inline]
#[must_use] pub const fn style_vertical_align_from_u8(v: u8) -> StyleVerticalAlign {
    match v {
        0 => StyleVerticalAlign::Baseline,
        1 => StyleVerticalAlign::Top,
        2 => StyleVerticalAlign::Middle,
        3 => StyleVerticalAlign::Bottom,
        4 => StyleVerticalAlign::Sub,
        5 => StyleVerticalAlign::Superscript,
        6 => StyleVerticalAlign::TextTop,
        7 => StyleVerticalAlign::TextBottom,
        _ => StyleVerticalAlign::Baseline,
    }
}

#[inline]
#[must_use] pub const fn style_vertical_align_to_u8(v: StyleVerticalAlign) -> u8 {
    match v {
        StyleVerticalAlign::Baseline => 0,
        StyleVerticalAlign::Top => 1,
        StyleVerticalAlign::Middle => 2,
        StyleVerticalAlign::Bottom => 3,
        StyleVerticalAlign::Sub => 4,
        StyleVerticalAlign::Superscript => 5,
        StyleVerticalAlign::TextTop => 6,
        StyleVerticalAlign::TextBottom => 7,
        // Percentage/Length cannot be stored in the 3-bit compact cache field;
        // fall back to 0 (Baseline). Callers must use the slow cascade path
        // for vertical-align values with length/percentage units.
        StyleVerticalAlign::Percentage(_) | StyleVerticalAlign::Length(_) => 0,
    }
}

#[inline]
#[must_use] pub const fn border_collapse_from_u8(v: u8) -> StyleBorderCollapse {
    match v {
        0 => StyleBorderCollapse::Separate,
        1 => StyleBorderCollapse::Collapse,
        _ => StyleBorderCollapse::Separate,
    }
}

#[inline]
#[must_use] pub const fn border_collapse_to_u8(v: StyleBorderCollapse) -> u8 {
    match v {
        StyleBorderCollapse::Separate => 0,
        StyleBorderCollapse::Collapse => 1,
    }
}

#[inline]
#[must_use] pub const fn border_style_from_u8(v: u8) -> BorderStyle {
    match v {
        0 => BorderStyle::None,
        1 => BorderStyle::Solid,
        2 => BorderStyle::Double,
        3 => BorderStyle::Dotted,
        4 => BorderStyle::Dashed,
        5 => BorderStyle::Hidden,
        6 => BorderStyle::Groove,
        7 => BorderStyle::Ridge,
        8 => BorderStyle::Inset,
        9 => BorderStyle::Outset,
        _ => BorderStyle::None,
    }
}

#[inline]
#[must_use] pub const fn border_style_to_u8(v: BorderStyle) -> u8 {
    match v {
        BorderStyle::None => 0,
        BorderStyle::Solid => 1,
        BorderStyle::Double => 2,
        BorderStyle::Dotted => 3,
        BorderStyle::Dashed => 4,
        BorderStyle::Hidden => 5,
        BorderStyle::Groove => 6,
        BorderStyle::Ridge => 7,
        BorderStyle::Inset => 8,
        BorderStyle::Outset => 9,
    }
}

/// Encode 4 border styles into a u16: [3:0]=top, [7:4]=right, [11:8]=bottom, [15:12]=left
#[inline]
#[must_use] pub const fn encode_border_styles_packed(top: BorderStyle, right: BorderStyle, bottom: BorderStyle, left: BorderStyle) -> u16 {
    (border_style_to_u8(top) as u16)
        | ((border_style_to_u8(right) as u16) << 4)
        | ((border_style_to_u8(bottom) as u16) << 8)
        | ((border_style_to_u8(left) as u16) << 12)
}

/// Decode border-top-style from packed u16
#[inline]
#[must_use] pub const fn decode_border_top_style(packed: u16) -> BorderStyle {
    border_style_from_u8((packed & 0x0F) as u8)
}

/// Decode border-right-style from packed u16
#[inline]
#[must_use] pub const fn decode_border_right_style(packed: u16) -> BorderStyle {
    border_style_from_u8(((packed >> 4) & 0x0F) as u8)
}

/// Decode border-bottom-style from packed u16
#[inline]
#[must_use] pub const fn decode_border_bottom_style(packed: u16) -> BorderStyle {
    border_style_from_u8(((packed >> 8) & 0x0F) as u8)
}

/// Decode border-left-style from packed u16
#[inline]
#[must_use] pub const fn decode_border_left_style(packed: u16) -> BorderStyle {
    border_style_from_u8(((packed >> 12) & 0x0F) as u8)
}

/// Encode a `ColorU` as u32 (0xRRGGBBAA). Returns 0 for sentinel/unset.
#[inline]
#[must_use] pub const fn encode_color_u32(c: &ColorU) -> u32 {
    ((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | (c.a as u32)
}

/// Decode a u32 back to `ColorU`. Returns `None` if sentinel (`0x00000000`).
///
/// **Limitation:** `rgba(0,0,0,0)` (fully transparent black) also encodes as
/// `0x00000000` and will be decoded as `None` (unset). This is acceptable
/// because fully transparent black is visually indistinguishable from unset.
#[inline]
#[must_use] pub const fn decode_color_u32(v: u32) -> Option<ColorU> {
    if v == 0 { return None; }
    Some(ColorU {
        r: ((v >> 24) & 0xFF) as u8,
        g: ((v >> 16) & 0xFF) as u8,
        b: ((v >> 8) & 0xFF) as u8,
        a: (v & 0xFF) as u8,
    })
}

// =============================================================================
// Tier 1: Encode / Decode
// =============================================================================

/// Pack all 21 enum properties into a single u64.
#[inline]
#[must_use] pub const fn encode_tier1(
    display: LayoutDisplay,
    position: LayoutPosition,
    float: LayoutFloat,
    overflow_x: LayoutOverflow,
    overflow_y: LayoutOverflow,
    box_sizing: LayoutBoxSizing,
    flex_direction: LayoutFlexDirection,
    flex_wrap: LayoutFlexWrap,
    justify_content: LayoutJustifyContent,
    align_items: LayoutAlignItems,
    align_content: LayoutAlignContent,
    writing_mode: LayoutWritingMode,
    clear: LayoutClear,
    font_weight: StyleFontWeight,
    font_style: StyleFontStyle,
    text_align: StyleTextAlign,
    visibility: StyleVisibility,
    white_space: StyleWhiteSpace,
    direction: StyleDirection,
    vertical_align: StyleVerticalAlign,
    border_collapse: StyleBorderCollapse,
) -> u64 {
    let mut v: u64 = TIER1_POPULATED_BIT;
    v |= (layout_display_to_u8(display) as u64) << DISPLAY_SHIFT;
    v |= (layout_position_to_u8(position) as u64) << POSITION_SHIFT;
    v |= (layout_float_to_u8(float) as u64) << FLOAT_SHIFT;
    v |= (layout_overflow_to_u8(overflow_x) as u64) << OVERFLOW_X_SHIFT;
    v |= (layout_overflow_to_u8(overflow_y) as u64) << OVERFLOW_Y_SHIFT;
    v |= (layout_box_sizing_to_u8(box_sizing) as u64) << BOX_SIZING_SHIFT;
    v |= (layout_flex_direction_to_u8(flex_direction) as u64) << FLEX_DIRECTION_SHIFT;
    v |= (layout_flex_wrap_to_u8(flex_wrap) as u64) << FLEX_WRAP_SHIFT;
    v |= (layout_justify_content_to_u8(justify_content) as u64) << JUSTIFY_CONTENT_SHIFT;
    v |= (layout_align_items_to_u8(align_items) as u64) << ALIGN_ITEMS_SHIFT;
    v |= (layout_align_content_to_u8(align_content) as u64) << ALIGN_CONTENT_SHIFT;
    v |= (layout_writing_mode_to_u8(writing_mode) as u64) << WRITING_MODE_SHIFT;
    v |= (layout_clear_to_u8(clear) as u64) << CLEAR_SHIFT;
    v |= (style_font_weight_to_u8(font_weight) as u64) << FONT_WEIGHT_SHIFT;
    v |= (style_font_style_to_u8(font_style) as u64) << FONT_STYLE_SHIFT;
    v |= (style_text_align_to_u8(text_align) as u64) << TEXT_ALIGN_SHIFT;
    v |= (style_visibility_to_u8(visibility) as u64) << VISIBILITY_SHIFT;
    v |= (style_white_space_to_u8(white_space) as u64) << WHITE_SPACE_SHIFT;
    v |= (style_direction_to_u8(direction) as u64) << DIRECTION_SHIFT;
    v |= (style_vertical_align_to_u8(vertical_align) as u64) << VERTICAL_ALIGN_SHIFT;
    v |= (border_collapse_to_u8(border_collapse) as u64) << BORDER_COLLAPSE_SHIFT;
    v
}

// Decode individual enum properties from a Tier 1 u64.
// Each function is `#[inline]` for zero-cost extraction.

#[inline]
#[must_use] pub const fn decode_display(t1: u64) -> LayoutDisplay {
    layout_display_from_u8(((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_position(t1: u64) -> LayoutPosition {
    layout_position_from_u8(((t1 >> POSITION_SHIFT) & POSITION_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_float(t1: u64) -> LayoutFloat {
    layout_float_from_u8(((t1 >> FLOAT_SHIFT) & FLOAT_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_overflow_x(t1: u64) -> LayoutOverflow {
    layout_overflow_from_u8(((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_overflow_y(t1: u64) -> LayoutOverflow {
    layout_overflow_from_u8(((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_box_sizing(t1: u64) -> LayoutBoxSizing {
    layout_box_sizing_from_u8(((t1 >> BOX_SIZING_SHIFT) & BOX_SIZING_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_flex_direction(t1: u64) -> LayoutFlexDirection {
    layout_flex_direction_from_u8(((t1 >> FLEX_DIRECTION_SHIFT) & FLEX_DIR_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_flex_wrap(t1: u64) -> LayoutFlexWrap {
    layout_flex_wrap_from_u8(((t1 >> FLEX_WRAP_SHIFT) & FLEX_WRAP_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_justify_content(t1: u64) -> LayoutJustifyContent {
    layout_justify_content_from_u8(((t1 >> JUSTIFY_CONTENT_SHIFT) & JUSTIFY_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_align_items(t1: u64) -> LayoutAlignItems {
    layout_align_items_from_u8(((t1 >> ALIGN_ITEMS_SHIFT) & ALIGN_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_align_content(t1: u64) -> LayoutAlignContent {
    layout_align_content_from_u8(((t1 >> ALIGN_CONTENT_SHIFT) & ALIGN_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_writing_mode(t1: u64) -> LayoutWritingMode {
    layout_writing_mode_from_u8(((t1 >> WRITING_MODE_SHIFT) & WRITING_MODE_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_clear(t1: u64) -> LayoutClear {
    layout_clear_from_u8(((t1 >> CLEAR_SHIFT) & CLEAR_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_font_weight(t1: u64) -> StyleFontWeight {
    style_font_weight_from_u8(((t1 >> FONT_WEIGHT_SHIFT) & FONT_WEIGHT_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_font_style(t1: u64) -> StyleFontStyle {
    style_font_style_from_u8(((t1 >> FONT_STYLE_SHIFT) & FONT_STYLE_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_text_align(t1: u64) -> StyleTextAlign {
    style_text_align_from_u8(((t1 >> TEXT_ALIGN_SHIFT) & TEXT_ALIGN_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_visibility(t1: u64) -> StyleVisibility {
    style_visibility_from_u8(((t1 >> VISIBILITY_SHIFT) & VISIBILITY_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_white_space(t1: u64) -> StyleWhiteSpace {
    style_white_space_from_u8(((t1 >> WHITE_SPACE_SHIFT) & WHITE_SPACE_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_direction(t1: u64) -> StyleDirection {
    style_direction_from_u8(((t1 >> DIRECTION_SHIFT) & DIRECTION_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_vertical_align(t1: u64) -> StyleVerticalAlign {
    style_vertical_align_from_u8(((t1 >> VERTICAL_ALIGN_SHIFT) & VERTICAL_ALIGN_MASK) as u8)
}

#[inline]
#[must_use] pub const fn decode_border_collapse(t1: u64) -> StyleBorderCollapse {
    border_collapse_from_u8(((t1 >> BORDER_COLLAPSE_SHIFT) & BORDER_COLLAPSE_MASK) as u8)
}

/// Returns true if the tier1 u64 was actually populated by `encode_tier1`.
#[inline]
#[cfg(test)]
#[must_use] pub const fn tier1_is_populated(t1: u64) -> bool {
    (t1 & TIER1_POPULATED_BIT) != 0
}

// =============================================================================
// Tier 2: CompactNodeProps — numeric dimensions (64 bytes/node)
// =============================================================================

/// u32 encoding for dimension properties (width, height, min-*, max-*, flex-basis, font-size).
///
/// Layout: `[3:0] SizeMetric (4 bits) | [31:4] signed fixed-point ×1000 (28 bits)`
///
/// This matches `FloatValue`'s internal representation (isize × 1000).
/// Range: ±134,217.727 at 0.001 precision (28-bit signed = ±2^27 = ±134,217,728 / 1000).
///
/// Sentinel values use the top of the u32 range (0xFFFFFFF9..0xFFFFFFFF).
///
/// Encode a `PixelValue` into u32 with `SizeMetric`. Returns `U32_SENTINEL` if out of range.
#[inline]
#[must_use] pub fn encode_pixel_value_u32(pv: &PixelValue) -> u32 {
    let metric = u32::from(size_metric_to_u8(pv.metric));
    let raw = pv.number.number; // already × 1000 (FloatValue internal repr)
    // 28-bit signed range: -134_217_728 ..= +134_217_727
    if !(-134_217_728..=134_217_727).contains(&raw) {
        return U32_SENTINEL; // overflow → tier 3
    }
    // Pack: low 4 bits = metric, upper 28 bits = value (as unsigned offset)
    // raw is range-checked to 28 bits above; reinterpret its low 32 bits for packing.
    let value_bits = i32::try_from(raw).unwrap_or(0).cast_unsigned() << 4;
    let packed = value_bits | metric;
    // A legitimate small NEGATIVE value with a high metric nibble (e.g. -1 in
    // vh/vmin/vmax packs to 0xFFFF_FFF9/FA/FB) lands in the reserved sentinel band
    // [U32_SENTINEL_THRESHOLD, U32_SENTINEL] and decode would misread it as an unset
    // sentinel. Escape to tier 3 so the real value is stored losslessly instead.
    if packed >= U32_SENTINEL_THRESHOLD {
        return U32_SENTINEL;
    }
    packed
}

/// Decode a u32 back to `PixelValue`. Returns None for sentinel values.
#[inline]
#[must_use] pub const fn decode_pixel_value_u32(encoded: u32) -> Option<PixelValue> {
    if encoded >= U32_SENTINEL_THRESHOLD {
        return None; // sentinel
    }
    let metric = size_metric_from_u8((encoded & 0xF) as u8);
    // Cast to i32 FIRST, then arithmetic right-shift to preserve sign bit
    let value_bits = encoded.cast_signed() >> 4;
    let raw = value_bits as isize; // × 1000
    Some(PixelValue {
        metric,
        number: FloatValue { number: raw },
    })
}

/// Encode an i16 resolved px value (×10). Returns `I16_SENTINEL` if out of range.
/// Range: -3276.8 ..= +3276.3 px at 0.1px precision.
#[inline]
#[must_use] pub fn encode_resolved_px_i16(px: f32) -> i16 {
    let scaled = crate::cast::f32_to_i32((px * 10.0).round());
    if scaled < -32768 || scaled > i32::from(I16_SENTINEL_THRESHOLD) - 1 {
        return I16_SENTINEL; // overflow or too large → tier 3
    }
    i16::try_from(scaled).unwrap_or(I16_SENTINEL)
}

/// Decode an i16 back to resolved px. Returns None for sentinel values.
#[inline]
#[must_use] pub fn decode_resolved_px_i16(v: i16) -> Option<f32> {
    if v >= I16_SENTINEL_THRESHOLD {
        return None;
    }
    Some(f32::from(v) / 10.0)
}

/// Encode a u16 flex value (×100). Returns `U16_SENTINEL` if out of range.
/// Range: 0.00 ..= 655.27 at 0.01 precision.
#[inline]
#[must_use] pub fn encode_flex_u16(value: f32) -> u16 {
    let scaled = crate::cast::f32_to_i32((value * 100.0).round());
    if scaled < 0 || scaled >= i32::from(U16_SENTINEL_THRESHOLD) {
        return U16_SENTINEL;
    }
    u16::try_from(scaled).unwrap_or(U16_SENTINEL)
}

/// Decode a u16 flex value back to f32. Returns None for sentinel values.
#[inline]
#[must_use] pub fn decode_flex_u16(v: u16) -> Option<f32> {
    if v >= U16_SENTINEL_THRESHOLD {
        return None;
    }
    Some(f32::from(v) / 100.0)
}

/// `SizeMetric` → u8 (4 bits, 12 variants)
#[inline]
#[must_use] pub const fn size_metric_to_u8(m: SizeMetric) -> u8 {
    match m {
        SizeMetric::Px => 0,
        SizeMetric::Pt => 1,
        SizeMetric::Em => 2,
        SizeMetric::Rem => 3,
        SizeMetric::In => 4,
        SizeMetric::Cm => 5,
        SizeMetric::Mm => 6,
        SizeMetric::Percent => 7,
        SizeMetric::Vw => 8,
        SizeMetric::Vh => 9,
        SizeMetric::Vmin => 10,
        SizeMetric::Vmax => 11,
    }
}

/// u8 → `SizeMetric`
#[inline]
#[must_use] pub const fn size_metric_from_u8(v: u8) -> SizeMetric {
    match v {
        0 => SizeMetric::Px,
        1 => SizeMetric::Pt,
        2 => SizeMetric::Em,
        3 => SizeMetric::Rem,
        4 => SizeMetric::In,
        5 => SizeMetric::Cm,
        6 => SizeMetric::Mm,
        7 => SizeMetric::Percent,
        8 => SizeMetric::Vw,
        9 => SizeMetric::Vh,
        10 => SizeMetric::Vmin,
        11 => SizeMetric::Vmax,
        _ => SizeMetric::Px,
    }
}

/// Layout-hot compact numeric properties for a single node (68 bytes).
/// Only fields accessed during the constraint-solving loop.
/// All dimensions use MSB-sentinel encoding.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CompactNodeProps {
    // --- Dimensions needing unit (u32 MSB-sentinel) ---
    pub width: u32,
    pub height: u32,
    pub min_width: u32,
    pub max_width: u32,
    pub min_height: u32,
    pub max_height: u32,
    pub flex_basis: u32,
    pub font_size: u32,

    // --- Resolved px values (i16 MSB-sentinel, ×10) ---
    pub padding_top: i16,
    pub padding_right: i16,
    pub padding_bottom: i16,
    pub padding_left: i16,
    pub margin_top: i16,
    pub margin_right: i16,
    pub margin_bottom: i16,
    pub margin_left: i16,
    pub border_top_width: i16,
    pub border_right_width: i16,
    pub border_bottom_width: i16,
    pub border_left_width: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
    pub left: i16,

    // --- Flex (u16 MSB-sentinel, ×100) ---
    pub flex_grow: u16,
    pub flex_shrink: u16,

    // --- Gap (i16 px×10, 0 = default) ---
    pub row_gap: i16,
    pub column_gap: i16,
}

/// Paint-cold compact properties for a single node.
/// Only accessed during display list generation, table layout, or text shaping.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CompactNodePropsCold {
    // --- Border colors (u32 RGBA as 0xRRGGBBAA, 0 = unset sentinel) ---
    pub border_top_color: u32,
    pub border_right_color: u32,
    pub border_bottom_color: u32,
    pub border_left_color: u32,

    // --- Border radii (i16 px × 10, I16_SENTINEL = unset/default = 0) ---
    pub border_top_left_radius: i16,
    pub border_top_right_radius: i16,
    pub border_bottom_left_radius: i16,
    pub border_bottom_right_radius: i16,

    // --- Other ---
    pub z_index: i16,   // range ±32764, sentinel = 0x7FFF
    /// Border styles packed: [3:0]=top, [7:4]=right, [11:8]=bottom, [15:12]=left
    pub border_styles_packed: u16,
    pub border_spacing_h: i16,
    pub border_spacing_v: i16,
    pub tab_size: i16,
    /// Grid column start (`I16_AUTO` = auto, positive = line number, negative = span)
    pub grid_col_start: i16,
    /// Grid column end
    pub grid_col_end: i16,
    /// Grid row start
    pub grid_row_start: i16,
    /// Grid row end
    pub grid_row_end: i16,

    // --- GPU / hot paint props ---
    /// Opacity × 254 (0 = fully transparent, 254 = opaque). 255 = unset/default (= 1.0).
    pub opacity: u8,
    /// Bitflags for properties that are usually unset. Lets the getter
    /// short-circuit without a cascade walk when the value is the default.
    ///
    /// bit 0: `has_transform`                (slow-walk only when set)
    /// bit 1: `has_transform_origin`
    /// bit 2: `has_box_shadow`
    /// bit 3: `has_text_decoration`          (slow-walk only when set)
    /// bits 4-5: `scrollbar_gutter` (0 = auto default, 1 = stable, 2 = both-edges, 3 = mirror)
    /// bit 6: `has_background`                (slow-walk only when set; ≈ negative fast path)
    /// bit 7: `has_clip_path`                 (slow-walk only when set)
    pub hot_flags: u8,
    /// Second byte of flags for rarely-set properties.
    ///
    /// bit 0: `has_any_scrollbar_css`
    ///        OR of all -azul-scrollbar-* / scrollbar-color / scrollbar-width props.
    ///        When clear, `get_scrollbar_style` can skip 8 cascade walks and use
    ///        the UA-default result.
    /// bit 1: `has_counter`      (counter-reset OR counter-increment)
    /// bit 2: `has_break`        (break-before OR break-after)
    /// bit 3: `has_text_orientation`
    /// bit 4: `has_text_shadow`
    /// bit 5: `has_backdrop_filter`
    /// bit 6: `has_filter`
    /// bit 7: `has_mix_blend_mode`
    pub extra_flags: u8,
}

pub const OPACITY_SENTINEL: u8 = 255;
pub const HOT_FLAG_HAS_TRANSFORM: u8 = 1 << 0;
pub const HOT_FLAG_HAS_TRANSFORM_ORIGIN: u8 = 1 << 1;
pub const HOT_FLAG_HAS_BOX_SHADOW: u8 = 1 << 2;
pub const HOT_FLAG_HAS_TEXT_DECORATION: u8 = 1 << 3;
pub const HOT_FLAG_SCROLLBAR_GUTTER_SHIFT: u8 = 4;
pub const HOT_FLAG_SCROLLBAR_GUTTER_MASK: u8 = 0b0011_0000;
pub const HOT_FLAG_HAS_BACKGROUND: u8 = 1 << 6;
pub const HOT_FLAG_HAS_CLIP_PATH: u8 = 1 << 7;
pub const EXTRA_FLAG_HAS_SCROLLBAR_CSS: u8 = 1 << 0;
pub const EXTRA_FLAG_HAS_COUNTER: u8 = 1 << 1;
pub const EXTRA_FLAG_HAS_BREAK: u8 = 1 << 2;
pub const EXTRA_FLAG_HAS_TEXT_ORIENTATION: u8 = 1 << 3;
pub const EXTRA_FLAG_HAS_TEXT_SHADOW: u8 = 1 << 4;
pub const EXTRA_FLAG_HAS_BACKDROP_FILTER: u8 = 1 << 5;
pub const EXTRA_FLAG_HAS_FILTER: u8 = 1 << 6;
pub const EXTRA_FLAG_HAS_MIX_BLEND_MODE: u8 = 1 << 7;

// ---- DOM-level rare text prop flags (stored on CompactLayoutCache) ----
// Each bit = "some node in this DOM declared this property".
// When clear, cascade walks for that prop anywhere in the DOM
// necessarily return None → callers can skip the walk and use
// the default value. Eliminates ~N × IFC-count walks per layout
// in typical pages where these props are never declared.
pub const DOM_HAS_SHAPE_INSIDE: u32 = 1 << 0;
pub const DOM_HAS_SHAPE_OUTSIDE: u32 = 1 << 1;
pub const DOM_HAS_TEXT_JUSTIFY: u32 = 1 << 2;
pub const DOM_HAS_TEXT_INDENT: u32 = 1 << 3;
pub const DOM_HAS_COLUMN_COUNT: u32 = 1 << 4;
pub const DOM_HAS_COLUMN_GAP: u32 = 1 << 5;
pub const DOM_HAS_INITIAL_LETTER: u32 = 1 << 6;
pub const DOM_HAS_INITIAL_LETTER_ALIGN: u32 = 1 << 7;
pub const DOM_HAS_LINE_CLAMP: u32 = 1 << 8;
pub const DOM_HAS_HANGING_PUNCTUATION: u32 = 1 << 9;
pub const DOM_HAS_TEXT_COMBINE_UPRIGHT: u32 = 1 << 10;
pub const DOM_HAS_EXCLUSION_MARGIN: u32 = 1 << 11;
pub const DOM_HAS_HYPHENATION_LANGUAGE: u32 = 1 << 12;
pub const DOM_HAS_UNICODE_BIDI: u32 = 1 << 13;
pub const DOM_HAS_TEXT_BOX_TRIM: u32 = 1 << 14;
pub const DOM_HAS_HYPHENS: u32 = 1 << 15;
pub const DOM_HAS_WORD_BREAK: u32 = 1 << 16;
pub const DOM_HAS_OVERFLOW_WRAP: u32 = 1 << 17;
pub const DOM_HAS_LINE_BREAK: u32 = 1 << 18;
pub const DOM_HAS_TEXT_ALIGN_LAST: u32 = 1 << 19;
pub const DOM_HAS_LINE_HEIGHT: u32 = 1 << 20;
pub const DOM_HAS_COLUMN_WIDTH: u32 = 1 << 21;
pub const DOM_HAS_SHAPE_MARGIN: u32 = 1 << 22;
pub const SCROLLBAR_GUTTER_AUTO: u8 = 0;
pub const SCROLLBAR_GUTTER_STABLE: u8 = 1;
pub const SCROLLBAR_GUTTER_BOTH_EDGES: u8 = 2;
pub const SCROLLBAR_GUTTER_MIRROR: u8 = 3;

impl Default for CompactNodeProps {
    fn default() -> Self {
        Self {
            // All dimensions default to Auto
            width: U32_AUTO,
            height: U32_AUTO,
            min_width: U32_AUTO,
            max_width: U32_NONE,
            min_height: U32_AUTO,
            max_height: U32_NONE,
            flex_basis: U32_AUTO,
            font_size: U32_INITIAL,
            // All resolved px default to 0
            padding_top: 0,
            padding_right: 0,
            padding_bottom: 0,
            padding_left: 0,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            border_top_width: 0,
            border_right_width: 0,
            border_bottom_width: 0,
            border_left_width: 0,
            top: I16_AUTO,
            right: I16_AUTO,
            bottom: I16_AUTO,
            left: I16_AUTO,
            // Flex defaults
            flex_grow: 0,
            flex_shrink: encode_flex_u16(1.0), // CSS default: flex-shrink: 1

            // Gap defaults
            row_gap: 0,
            column_gap: 0,
        }
    }
}

impl Default for CompactNodePropsCold {
    fn default() -> Self {
        Self {
            // Border colors default to 0 (sentinel/unset)
            border_top_color: 0,
            border_right_color: 0,
            border_bottom_color: 0,
            border_left_color: 0,
            // Border radii: I16_SENTINEL means "no rounded corner" (skip slow walk)
            border_top_left_radius: I16_SENTINEL,
            border_top_right_radius: I16_SENTINEL,
            border_bottom_left_radius: I16_SENTINEL,
            border_bottom_right_radius: I16_SENTINEL,
            // Other
            z_index: I16_AUTO,
            border_styles_packed: 0, // all BorderStyle::None
            border_spacing_h: 0,
            border_spacing_v: 0,
            tab_size: I16_SENTINEL, // default is 8em, needs resolution → sentinel
            grid_col_start: I16_AUTO,
            grid_col_end: I16_AUTO,
            grid_row_start: I16_AUTO,
            grid_row_end: I16_AUTO,
            opacity: OPACITY_SENTINEL,
            hot_flags: 0,
            extra_flags: 0,
        }
    }
}

// =============================================================================
// Tier 2b: CompactTextProps — IFC/text properties (24 bytes/node)
// =============================================================================

/// Compact text/IFC properties for a single node (24 bytes).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[repr(C)]
pub struct CompactTextProps {
    pub text_color: u32,       // RGBA as 0xRRGGBBAA (0 = transparent/unset)
    pub font_family_hash: u64, // FxHash of font-family list (0 = sentinel/unset)
    pub line_height: i16,      // px × 10, sentinel = I16_SENTINEL
    pub letter_spacing: i16,   // px × 10
    pub word_spacing: i16,     // px × 10
    pub text_indent: i16,      // px × 10
}

impl Default for CompactTextProps {
    fn default() -> Self {
        Self {
            text_color: 0,
            font_family_hash: 0,
            line_height: I16_SENTINEL, // "normal" → sentinel
            letter_spacing: 0,
            word_spacing: 0,
            text_indent: 0,
        }
    }
}

// =============================================================================
// Tier 3: Overflow map — rare/complex properties
// =============================================================================

// Overflow properties that couldn't fit in Tier 1/2 encoding.
// Contains the original `CssProperty` values for properties that:
// - Have `calc()` expressions
// - Exceed the numeric range of compact encoding
// - Are rare CSS properties (grid, transforms, etc.)
// =============================================================================
// CompactLayoutCache — the top-level container
// =============================================================================

/// Three-tier compact layout property cache.
///
/// Allocated once per restyle, indexed by node index (same as `NodeId`).
/// Provides O(1) array-indexed access to all layout properties.
///
/// Non-compact properties (background, box-shadow, transform, etc.) are
/// resolved via the slow cascade path in `CssPropertyCache::get_property_slow()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactLayoutCache {
    /// Tier 1: ALL enum properties bitpacked into u64 per node (8 B/node)
    pub tier1_enums: Vec<u64>,
    /// Tier 2 hot: Layout-critical numeric dimensions per node (68 B/node)
    pub tier2_dims: Vec<CompactNodeProps>,
    /// Tier 2 cold: Paint-only properties per node (28 B/node)
    pub tier2_cold: Vec<CompactNodePropsCold>,
    /// Tier 2b: Text/IFC properties per node (24 B/node)
    pub tier2b_text: Vec<CompactTextProps>,
    /// Indices of nodes whose `font_family_hash` changed since the last frame.
    ///
    /// Enables **per-node** font dirty tracking instead of the global all-or-nothing
    /// `font_stacks_hash` XOR approach. When this list is non-empty, only the
    /// font chains for these specific nodes need to be re-resolved, avoiding O(N)
    /// re-resolution when a single node's `font-family` changes.
    ///
    /// Populated during `build_compact_cache()` by comparing each node's
    /// `font_family_hash` against `prev_font_hashes`.
    pub font_dirty_nodes: Vec<usize>,
    /// Previous frame's per-node `font_family_hash` values.
    ///
    /// Stored after each compact cache build so that the next build can detect
    /// which specific nodes' font-family changed (rather than relying on a
    /// collision-prone global XOR hash).
    pub prev_font_hashes: Vec<u64>,
    /// Reverse map: `font_family_hash` (u64) → actual `StyleFontFamilyVec`.
    ///
    /// Populated during `build_compact_cache()` as a byproduct of hash computation.
    /// Consumers use this to look up font family names from the compact cache hash
    /// without going through `get_property_slow()` (which fails for inherited values
    /// on text nodes).
    pub font_hash_to_families: alloc::collections::BTreeMap<u64, crate::props::basic::font::StyleFontFamilyVec>,
    /// Bitfield tracking which rare text props are declared *anywhere* in the DOM.
    /// Built once during `build_compact_cache_with_inheritance`. When a bit is
    /// clear, callers (e.g. `translate_to_text3_constraints`) can skip the
    /// cascade walk for that property — its slow path would always return
    /// `None` and fall back to the default. See `DOM_HAS_*` constants.
    pub dom_declared_flags: u32,
}

impl CompactLayoutCache {
    /// Create an empty cache (no nodes).
    #[must_use] pub const fn empty() -> Self {
        Self {
            tier1_enums: Vec::new(),
            tier2_dims: Vec::new(),
            tier2_cold: Vec::new(),
            tier2b_text: Vec::new(),
            font_dirty_nodes: Vec::new(),
            prev_font_hashes: Vec::new(),
            font_hash_to_families: alloc::collections::BTreeMap::new(),
            dom_declared_flags: 0,
        }
    }

    /// Create a cache pre-allocated for `node_count` nodes, filled with defaults.
    #[must_use] pub fn with_capacity(node_count: usize) -> Self {
        Self {
            tier1_enums: vec![0u64; node_count],
            tier2_dims: vec![CompactNodeProps::default(); node_count],
            tier2_cold: vec![CompactNodePropsCold::default(); node_count],
            tier2b_text: vec![CompactTextProps::default(); node_count],
            font_dirty_nodes: Vec::new(),
            prev_font_hashes: vec![0u64; node_count],
            font_hash_to_families: alloc::collections::BTreeMap::new(),
            dom_declared_flags: 0,
        }
    }

    /// Number of nodes in this cache.
    #[inline]
    #[must_use] pub const fn node_count(&self) -> usize {
        self.tier1_enums.len()
    }

    // -- Tier 1 getters (enum properties) --

    #[inline]
    #[must_use] pub fn get_display(&self, node_idx: usize) -> LayoutDisplay {
        decode_display(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_position(&self, node_idx: usize) -> LayoutPosition {
        decode_position(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_float(&self, node_idx: usize) -> LayoutFloat {
        decode_float(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_overflow_x(&self, node_idx: usize) -> LayoutOverflow {
        decode_overflow_x(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_overflow_y(&self, node_idx: usize) -> LayoutOverflow {
        decode_overflow_y(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_box_sizing(&self, node_idx: usize) -> LayoutBoxSizing {
        decode_box_sizing(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_flex_direction(&self, node_idx: usize) -> LayoutFlexDirection {
        decode_flex_direction(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_flex_wrap(&self, node_idx: usize) -> LayoutFlexWrap {
        decode_flex_wrap(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_justify_content(&self, node_idx: usize) -> LayoutJustifyContent {
        decode_justify_content(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_align_items(&self, node_idx: usize) -> LayoutAlignItems {
        decode_align_items(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_align_content(&self, node_idx: usize) -> LayoutAlignContent {
        decode_align_content(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_writing_mode(&self, node_idx: usize) -> LayoutWritingMode {
        decode_writing_mode(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_clear(&self, node_idx: usize) -> LayoutClear {
        decode_clear(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_font_weight(&self, node_idx: usize) -> StyleFontWeight {
        decode_font_weight(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_font_style(&self, node_idx: usize) -> StyleFontStyle {
        decode_font_style(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_text_align(&self, node_idx: usize) -> StyleTextAlign {
        decode_text_align(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_visibility(&self, node_idx: usize) -> StyleVisibility {
        decode_visibility(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_white_space(&self, node_idx: usize) -> StyleWhiteSpace {
        decode_white_space(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_direction(&self, node_idx: usize) -> StyleDirection {
        decode_direction(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_vertical_align(&self, node_idx: usize) -> StyleVerticalAlign {
        decode_vertical_align(self.tier1_enums[node_idx])
    }

    #[inline]
    #[must_use] pub fn get_border_collapse(&self, node_idx: usize) -> StyleBorderCollapse {
        decode_border_collapse(self.tier1_enums[node_idx])
    }

    // -- Tier 2 getters (numeric dimensions) --

    /// Get width as encoded u32 (use `decode_pixel_value_u32` or check sentinel).
    #[inline]
    #[must_use] pub fn get_width_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].width
    }

    #[inline]
    #[must_use] pub fn get_height_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].height
    }

    #[inline]
    #[must_use] pub fn get_min_width_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].min_width
    }

    #[inline]
    #[must_use] pub fn get_max_width_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].max_width
    }

    #[inline]
    #[must_use] pub fn get_min_height_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].min_height
    }

    #[inline]
    #[must_use] pub fn get_max_height_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].max_height
    }

    #[inline]
    #[must_use] pub fn get_font_size_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].font_size
    }

    #[inline]
    #[must_use] pub fn get_flex_basis_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].flex_basis
    }

    /// Get padding-top as resolved px. Returns None if sentinel (needs slow path).
    #[inline]
    #[must_use] pub fn get_padding_top(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_top)
    }

    #[inline]
    #[must_use] pub fn get_padding_right(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_right)
    }

    #[inline]
    #[must_use] pub fn get_padding_bottom(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_bottom)
    }

    #[inline]
    #[must_use] pub fn get_padding_left(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_left)
    }

    #[inline]
    #[must_use] pub fn get_margin_top(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_top;
        if v == I16_AUTO { return None; } // Auto for margin is special
        decode_resolved_px_i16(v)
    }

    #[inline]
    #[must_use] pub fn get_margin_right(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_right;
        if v == I16_AUTO { return None; }
        decode_resolved_px_i16(v)
    }

    #[inline]
    #[must_use] pub fn get_margin_bottom(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_bottom;
        if v == I16_AUTO { return None; }
        decode_resolved_px_i16(v)
    }

    #[inline]
    #[must_use] pub fn get_margin_left(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_left;
        if v == I16_AUTO { return None; }
        decode_resolved_px_i16(v)
    }

    /// Check if margin is Auto (important for centering logic).
    #[inline]
    #[must_use] pub fn is_margin_top_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_top == I16_AUTO
    }

    #[inline]
    #[must_use] pub fn is_margin_right_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_right == I16_AUTO
    }

    #[inline]
    #[must_use] pub fn is_margin_bottom_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_bottom == I16_AUTO
    }

    #[inline]
    #[must_use] pub fn is_margin_left_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_left == I16_AUTO
    }

    #[inline]
    #[must_use] pub fn get_border_top_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_top_width)
    }

    #[inline]
    #[must_use] pub fn get_border_right_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_right_width)
    }

    #[inline]
    #[must_use] pub fn get_border_bottom_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_bottom_width)
    }

    #[inline]
    #[must_use] pub fn get_border_left_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_left_width)
    }

    // -- Raw i16 getters for macro fast paths --

    #[inline]
    #[must_use] pub fn get_padding_top_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_top
    }

    #[inline]
    #[must_use] pub fn get_padding_right_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_right
    }

    #[inline]
    #[must_use] pub fn get_padding_bottom_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_bottom
    }

    #[inline]
    #[must_use] pub fn get_padding_left_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_left
    }

    #[inline]
    #[must_use] pub fn get_margin_top_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_top
    }

    #[inline]
    #[must_use] pub fn get_margin_right_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_right
    }

    #[inline]
    #[must_use] pub fn get_margin_bottom_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_bottom
    }

    #[inline]
    #[must_use] pub fn get_margin_left_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_left
    }

    #[inline]
    #[must_use] pub fn get_border_top_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_top_width
    }

    #[inline]
    #[must_use] pub fn get_border_right_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_right_width
    }

    #[inline]
    #[must_use] pub fn get_border_bottom_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_bottom_width
    }

    #[inline]
    #[must_use] pub fn get_border_left_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_left_width
    }

    #[inline]
    #[must_use] pub fn get_top(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].top
    }

    #[inline]
    #[must_use] pub fn get_right(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].right
    }

    #[inline]
    #[must_use] pub fn get_bottom(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].bottom
    }

    #[inline]
    #[must_use] pub fn get_left(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].left
    }

    #[inline]
    #[must_use] pub fn get_flex_grow(&self, node_idx: usize) -> Option<f32> {
        decode_flex_u16(self.tier2_dims[node_idx].flex_grow)
    }

    #[inline]
    #[must_use] pub fn get_flex_shrink(&self, node_idx: usize) -> Option<f32> {
        decode_flex_u16(self.tier2_dims[node_idx].flex_shrink)
    }

    #[inline]
    #[must_use] pub fn get_z_index(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].z_index
    }

    // -- Border colors (u32 RGBA) — cold tier --

    #[inline]
    #[must_use] pub fn get_border_top_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_cold[node_idx].border_top_color
    }

    #[inline]
    #[must_use] pub fn get_border_right_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_cold[node_idx].border_right_color
    }

    #[inline]
    #[must_use] pub fn get_border_bottom_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_cold[node_idx].border_bottom_color
    }

    #[inline]
    #[must_use] pub fn get_border_left_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_cold[node_idx].border_left_color
    }

    // -- Border styles (packed u16) — cold tier --

    #[inline]
    #[must_use] pub fn get_border_styles_packed(&self, node_idx: usize) -> u16 {
        self.tier2_cold[node_idx].border_styles_packed
    }

    #[inline]
    #[must_use] pub fn get_border_top_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_top_style(self.tier2_cold[node_idx].border_styles_packed)
    }

    #[inline]
    #[must_use] pub fn get_border_right_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_right_style(self.tier2_cold[node_idx].border_styles_packed)
    }

    #[inline]
    #[must_use] pub fn get_border_bottom_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_bottom_style(self.tier2_cold[node_idx].border_styles_packed)
    }

    #[inline]
    #[must_use] pub fn get_border_left_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_left_style(self.tier2_cold[node_idx].border_styles_packed)
    }

    // -- Border spacing — cold tier --

    #[inline]
    #[must_use] pub fn get_border_spacing_h_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].border_spacing_h
    }

    #[inline]
    #[must_use] pub fn get_border_spacing_v_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].border_spacing_v
    }

    // -- Tab size — cold tier --

    #[inline]
    #[must_use] pub fn get_tab_size_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].tab_size
    }

    // -- Border radii — cold tier (i16 px × 10, I16_SENTINEL = unset = 0) --

    #[inline]
    #[must_use] pub fn get_border_top_left_radius_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].border_top_left_radius
    }

    #[inline]
    #[must_use] pub fn get_border_top_right_radius_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].border_top_right_radius
    }

    #[inline]
    #[must_use] pub fn get_border_bottom_left_radius_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].border_bottom_left_radius
    }

    #[inline]
    #[must_use] pub fn get_border_bottom_right_radius_raw(&self, node_idx: usize) -> i16 {
        self.tier2_cold[node_idx].border_bottom_right_radius
    }

    // -- Opacity / transform / hot flags --

    /// Raw opacity byte. `OPACITY_SENTINEL` (255) = unset (default = 1.0).
    /// Otherwise value / 254.0 yields the opacity in [0.0, 1.0].
    #[inline]
    #[must_use] pub fn get_opacity_raw(&self, node_idx: usize) -> u8 {
        self.tier2_cold[node_idx].opacity
    }

    #[inline]
    #[must_use] pub fn get_hot_flags(&self, node_idx: usize) -> u8 {
        self.tier2_cold[node_idx].hot_flags
    }

    #[inline]
    #[must_use] pub fn has_transform(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].hot_flags & HOT_FLAG_HAS_TRANSFORM != 0
    }

    #[inline]
    #[must_use] pub fn has_transform_origin(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].hot_flags & HOT_FLAG_HAS_TRANSFORM_ORIGIN != 0
    }

    #[inline]
    #[must_use] pub fn has_box_shadow(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].hot_flags & HOT_FLAG_HAS_BOX_SHADOW != 0
    }

    #[inline]
    #[must_use] pub fn has_text_decoration(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].hot_flags & HOT_FLAG_HAS_TEXT_DECORATION != 0
    }

    #[inline]
    #[must_use] pub fn has_background(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].hot_flags & HOT_FLAG_HAS_BACKGROUND != 0
    }

    #[inline]
    #[must_use] pub fn has_clip_path(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].hot_flags & HOT_FLAG_HAS_CLIP_PATH != 0
    }

    #[inline]
    #[must_use] pub fn has_scrollbar_css(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_SCROLLBAR_CSS != 0
    }

    #[inline]
    #[must_use] pub fn has_counter(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_COUNTER != 0
    }

    #[inline]
    #[must_use] pub fn has_break(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_BREAK != 0
    }

    #[inline]
    #[must_use] pub fn has_text_orientation(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_TEXT_ORIENTATION != 0
    }

    #[inline]
    #[must_use] pub fn has_text_shadow(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_TEXT_SHADOW != 0
    }

    #[inline]
    #[must_use] pub fn has_backdrop_filter(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_BACKDROP_FILTER != 0
    }

    #[inline]
    #[must_use] pub fn has_filter(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_FILTER != 0
    }

    #[inline]
    #[must_use] pub fn has_mix_blend_mode(&self, node_idx: usize) -> bool {
        self.tier2_cold[node_idx].extra_flags & EXTRA_FLAG_HAS_MIX_BLEND_MODE != 0
    }

    /// DOM-level fast-path check: returns `true` if the given flag bit is set
    /// (some node in this DOM declared the corresponding property).
    #[inline]
    #[must_use] pub const fn dom_declared(&self, flag: u32) -> bool {
        self.dom_declared_flags & flag != 0
    }

    /// Scrollbar-gutter: 0 = auto (default), 1 = stable, 2 = both-edges, 3 = mirror.
    #[inline]
    #[must_use] pub fn get_scrollbar_gutter_bits(&self, node_idx: usize) -> u8 {
        (self.tier2_cold[node_idx].hot_flags & HOT_FLAG_SCROLLBAR_GUTTER_MASK)
            >> HOT_FLAG_SCROLLBAR_GUTTER_SHIFT
    }

    // -- Tier 2b getters (text props) --

    #[inline]
    #[must_use] pub fn get_text_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2b_text[node_idx].text_color
    }

    #[inline]
    #[must_use] pub fn get_font_family_hash(&self, node_idx: usize) -> u64 {
        self.tier2b_text[node_idx].font_family_hash
    }

    #[inline]
    #[must_use] pub fn get_line_height(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].line_height)
    }

    #[inline]
    #[must_use] pub fn get_letter_spacing(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].letter_spacing)
    }

    #[inline]
    #[must_use] pub fn get_word_spacing(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].word_spacing)
    }

    #[inline]
    #[must_use] pub fn get_text_indent(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].text_indent)
    }

}

// =============================================================================
// Helper: encode a CssPropertyValue<PixelValue> into i16 resolved-px
// =============================================================================

/// Resolve a `CssPropertyValue`<PixelValue> to an i16 ×10 encoding.
///
/// Only handles `Exact(px(...))` values. Everything else → sentinel.
/// For the compact cache builder, we only pre-resolve absolute pixel values.
/// Relative units (em, %, etc.) get sentinel and fall back to the slow path.
#[inline]
#[must_use] pub fn encode_css_pixel_as_i16(prop: &CssPropertyValue<PixelValue>) -> i16 {
    match prop {
        CssPropertyValue::Exact(pv) => {
            if pv.metric == SizeMetric::Px {
                encode_resolved_px_i16(pv.number.get())
            } else {
                I16_SENTINEL // non-px units need resolution context → slow path
            }
        }
        CssPropertyValue::Auto => I16_AUTO,
        CssPropertyValue::Initial => I16_INITIAL,
        CssPropertyValue::Inherit => I16_INHERIT,
        _ => I16_SENTINEL,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier1_roundtrip() {
        let t1 = encode_tier1(
            LayoutDisplay::Flex,
            LayoutPosition::Relative,
            LayoutFloat::Left,
            LayoutOverflow::Hidden,
            LayoutOverflow::Scroll,
            LayoutBoxSizing::BorderBox,
            LayoutFlexDirection::Column,
            LayoutFlexWrap::Wrap,
            LayoutJustifyContent::SpaceBetween,
            LayoutAlignItems::Center,
            LayoutAlignContent::End,
            LayoutWritingMode::VerticalRl,
            LayoutClear::Both,
            StyleFontWeight::Bold,
            StyleFontStyle::Italic,
            StyleTextAlign::Center,
            StyleVisibility::Hidden,
            StyleWhiteSpace::Pre,
            StyleDirection::Rtl,
            StyleVerticalAlign::Middle,
            StyleBorderCollapse::Collapse,
        );

        assert!(tier1_is_populated(t1));
        assert_eq!(decode_display(t1), LayoutDisplay::Flex);
        assert_eq!(decode_position(t1), LayoutPosition::Relative);
        assert_eq!(decode_float(t1), LayoutFloat::Left);
        assert_eq!(decode_overflow_x(t1), LayoutOverflow::Hidden);
        assert_eq!(decode_overflow_y(t1), LayoutOverflow::Scroll);
        assert_eq!(decode_box_sizing(t1), LayoutBoxSizing::BorderBox);
        assert_eq!(decode_flex_direction(t1), LayoutFlexDirection::Column);
        assert_eq!(decode_flex_wrap(t1), LayoutFlexWrap::Wrap);
        assert_eq!(decode_justify_content(t1), LayoutJustifyContent::SpaceBetween);
        assert_eq!(decode_align_items(t1), LayoutAlignItems::Center);
        assert_eq!(decode_align_content(t1), LayoutAlignContent::End);
        assert_eq!(decode_writing_mode(t1), LayoutWritingMode::VerticalRl);
        assert_eq!(decode_clear(t1), LayoutClear::Both);
        assert_eq!(decode_font_weight(t1), StyleFontWeight::Bold);
        assert_eq!(decode_font_style(t1), StyleFontStyle::Italic);
        assert_eq!(decode_text_align(t1), StyleTextAlign::Center);
        assert_eq!(decode_visibility(t1), StyleVisibility::Hidden);
        assert_eq!(decode_white_space(t1), StyleWhiteSpace::Pre);
        assert_eq!(decode_direction(t1), StyleDirection::Rtl);
        assert_eq!(decode_vertical_align(t1), StyleVerticalAlign::Middle);
        assert_eq!(decode_border_collapse(t1), StyleBorderCollapse::Collapse);
    }

    #[test]
    fn test_tier1_defaults() {
        let t1 = encode_tier1(
            LayoutDisplay::Block,
            LayoutPosition::Static,
            LayoutFloat::None,
            LayoutOverflow::Visible,
            LayoutOverflow::Visible,
            LayoutBoxSizing::ContentBox,
            LayoutFlexDirection::Row,
            LayoutFlexWrap::NoWrap,
            LayoutJustifyContent::FlexStart,
            LayoutAlignItems::Stretch,
            LayoutAlignContent::Stretch,
            LayoutWritingMode::HorizontalTb,
            LayoutClear::None,
            StyleFontWeight::Normal,
            StyleFontStyle::Normal,
            StyleTextAlign::Left,
            StyleVisibility::Visible,
            StyleWhiteSpace::Normal,
            StyleDirection::Ltr,
            StyleVerticalAlign::Baseline,
            StyleBorderCollapse::Separate,
        );

        assert!(tier1_is_populated(t1));
        assert_eq!(decode_display(t1), LayoutDisplay::Block);
        assert_eq!(decode_position(t1), LayoutPosition::Static);
    }

    #[test]
    fn test_pixel_value_u32_roundtrip() {
        let pv = PixelValue::px(123.456);
        let encoded = encode_pixel_value_u32(&pv);
        assert!(encoded < U32_SENTINEL_THRESHOLD);
        let decoded = decode_pixel_value_u32(encoded).unwrap();
        assert_eq!(decoded.metric, SizeMetric::Px);
        // Check within precision (×1000)
        assert!((decoded.number.get() - 123.456).abs() < 0.002);
    }

    #[test]
    fn test_pixel_value_u32_percent() {
        let pv = PixelValue {
            metric: SizeMetric::Percent,
            number: FloatValue::new(50.0),
        };
        let encoded = encode_pixel_value_u32(&pv);
        let decoded = decode_pixel_value_u32(encoded).unwrap();
        assert_eq!(decoded.metric, SizeMetric::Percent);
        assert!((decoded.number.get() - 50.0).abs() < 0.002);
    }

    #[test]
    fn test_sentinel_values() {
        assert_eq!(decode_pixel_value_u32(U32_SENTINEL), None);
        assert_eq!(decode_pixel_value_u32(U32_AUTO), None);
        assert_eq!(decode_pixel_value_u32(U32_MIN_CONTENT), None);
        assert_eq!(decode_resolved_px_i16(I16_SENTINEL), None);
        assert_eq!(decode_resolved_px_i16(I16_AUTO), None);
    }

    #[test]
    fn test_resolved_px_i16_roundtrip() {
        let px = 123.4f32;
        let encoded = encode_resolved_px_i16(px);
        let decoded = decode_resolved_px_i16(encoded).unwrap();
        assert!((decoded - 123.4).abs() < 0.11);

        // Negative values
        let px = -50.7f32;
        let encoded = encode_resolved_px_i16(px);
        let decoded = decode_resolved_px_i16(encoded).unwrap();
        assert!((decoded - (-50.7)).abs() < 0.11);
    }

    #[test]
    fn test_flex_u16_roundtrip() {
        let v = 2.5f32;
        let encoded = encode_flex_u16(v);
        let decoded = decode_flex_u16(encoded).unwrap();
        assert!((decoded - 2.5).abs() < 0.011);
    }

    #[test]
    fn test_compact_node_props_size() {
        // 72B hot props: 8×u32 dimensions (32B) + 16×i16 box model (32B)
        // + 2×u16 flex (4B) + 1×i16 order + align/pos tier1 bits (4B).
        assert_eq!(size_of::<CompactNodeProps>(), 72);
    }

    #[test]
    fn test_compact_node_props_cold_size() {
        // 48B cold props: 4×u32 border colors (16B) + 4×i16 border radii (8B)
        // + 1×i16 z_index + 1×u16 border_styles_packed + 2×i16 border_spacing
        // + 1×i16 tab_size + 4×i16 grid placement (8B) + 3×u8 (opacity,
        // hot_flags, extra_flags) = 45B, padded to 48B for u32 alignment.
        assert_eq!(size_of::<CompactNodePropsCold>(), 48);
    }

    #[test]
    fn test_compact_text_props_size() {
        assert_eq!(size_of::<CompactTextProps>(), 24);
    }

    // ========================================================================
    // Tier1 enum 0-sentinel contract
    //
    // Tier1 packs 21 enums into a single u64. A bit run that is all zeros
    // must decode to the CSS initial value of that property, because an
    // unpopulated tier1 field is all zeros. If any encoder shifts so that
    // `0 -> something-other-than-initial`, every node that didn't explicitly
    // set the property silently gets the wrong default — which is exactly
    // how the calc.c grid stretch regression shipped (Start encoded as 0,
    // so every grid container reported justify-items: Start instead of
    // the CSS default Stretch-for-grid, collapsing the calc button grid).
    //
    // Test invariant: decoding a u8 of 0 for every enum yields the CSS
    // initial value of that property.
    // ========================================================================

    #[test]
    fn test_justify_items_zero_is_stretch() {
        // CSS initial for justify-items is `normal`, which on a grid item
        // behaves as `stretch`. The tier1 bit pattern 0 must round-trip to
        // Stretch so unset grid containers don't collapse their items.
        assert_eq!(layout_justify_items_from_u8(0), LayoutJustifyItems::Stretch);
        assert_eq!(layout_justify_items_to_u8(LayoutJustifyItems::Stretch), 0);
    }

    #[test]
    fn test_tier1_enum_zero_sentinel_is_css_initial() {
        assert_eq!(layout_display_from_u8(0), LayoutDisplay::Block);
        assert_eq!(layout_position_from_u8(0), LayoutPosition::Static);
        assert_eq!(layout_float_from_u8(0), LayoutFloat::None);
        assert_eq!(layout_overflow_from_u8(0), LayoutOverflow::Visible);
        assert_eq!(layout_box_sizing_from_u8(0), LayoutBoxSizing::ContentBox);
        assert_eq!(layout_flex_direction_from_u8(0), LayoutFlexDirection::Row);
        assert_eq!(layout_flex_wrap_from_u8(0), LayoutFlexWrap::NoWrap);
        assert_eq!(layout_justify_content_from_u8(0), LayoutJustifyContent::FlexStart);
        assert_eq!(layout_align_items_from_u8(0), LayoutAlignItems::Stretch);
        assert_eq!(layout_align_content_from_u8(0), LayoutAlignContent::Stretch);
        assert_eq!(layout_align_self_from_u8(0), LayoutAlignSelf::Auto);
        assert_eq!(layout_justify_self_from_u8(0), LayoutJustifySelf::Auto);
        assert_eq!(layout_justify_items_from_u8(0), LayoutJustifyItems::Stretch);
        assert_eq!(layout_grid_auto_flow_from_u8(0), LayoutGridAutoFlow::Row);
        assert_eq!(layout_writing_mode_from_u8(0), LayoutWritingMode::HorizontalTb);
        assert_eq!(layout_clear_from_u8(0), LayoutClear::None);
        assert_eq!(style_font_weight_from_u8(0), StyleFontWeight::Normal);
        assert_eq!(style_font_style_from_u8(0), StyleFontStyle::Normal);
        // text-align initial is `start`; we collapse `start` → `left` on
        // LTR runs at encode time, so the 0 slot decodes to Left.
        assert_eq!(style_text_align_from_u8(0), StyleTextAlign::Left);
        assert_eq!(style_visibility_from_u8(0), StyleVisibility::Visible);
        assert_eq!(style_white_space_from_u8(0), StyleWhiteSpace::Normal);
        assert_eq!(style_direction_from_u8(0), StyleDirection::Ltr);
        assert_eq!(style_vertical_align_from_u8(0), StyleVerticalAlign::Baseline);
        assert_eq!(border_collapse_from_u8(0), StyleBorderCollapse::Separate);
    }

    #[test]
    fn test_tier1_enum_initial_encodes_to_zero() {
        // Mirror of the above — encoding the CSS initial value must
        // produce 0, otherwise an `all-zeros` tier1 bit run would encode
        // a non-initial value and nodes without explicit properties would
        // silently inherit the wrong default.
        assert_eq!(layout_display_to_u8(LayoutDisplay::Block), 0);
        assert_eq!(layout_position_to_u8(LayoutPosition::Static), 0);
        assert_eq!(layout_float_to_u8(LayoutFloat::None), 0);
        assert_eq!(layout_overflow_to_u8(LayoutOverflow::Visible), 0);
        assert_eq!(layout_box_sizing_to_u8(LayoutBoxSizing::ContentBox), 0);
        assert_eq!(layout_flex_direction_to_u8(LayoutFlexDirection::Row), 0);
        assert_eq!(layout_flex_wrap_to_u8(LayoutFlexWrap::NoWrap), 0);
        assert_eq!(layout_justify_content_to_u8(LayoutJustifyContent::FlexStart), 0);
        assert_eq!(layout_align_items_to_u8(LayoutAlignItems::Stretch), 0);
        assert_eq!(layout_align_content_to_u8(LayoutAlignContent::Stretch), 0);
        assert_eq!(layout_align_self_to_u8(LayoutAlignSelf::Auto), 0);
        assert_eq!(layout_justify_self_to_u8(LayoutJustifySelf::Auto), 0);
        assert_eq!(layout_justify_items_to_u8(LayoutJustifyItems::Stretch), 0);
        assert_eq!(layout_grid_auto_flow_to_u8(LayoutGridAutoFlow::Row), 0);
        assert_eq!(layout_writing_mode_to_u8(LayoutWritingMode::HorizontalTb), 0);
        assert_eq!(layout_clear_to_u8(LayoutClear::None), 0);
        assert_eq!(style_font_weight_to_u8(StyleFontWeight::Normal), 0);
        assert_eq!(style_font_style_to_u8(StyleFontStyle::Normal), 0);
        assert_eq!(style_text_align_to_u8(StyleTextAlign::Left), 0);
        assert_eq!(style_visibility_to_u8(StyleVisibility::Visible), 0);
        assert_eq!(style_white_space_to_u8(StyleWhiteSpace::Normal), 0);
        assert_eq!(style_direction_to_u8(StyleDirection::Ltr), 0);
        assert_eq!(style_vertical_align_to_u8(StyleVerticalAlign::Baseline), 0);
        assert_eq!(border_collapse_to_u8(StyleBorderCollapse::Separate), 0);
    }

    // ========================================================================
    // Exhaustive round-trip: every variant of every enum must survive
    // encode → decode unchanged. Catches any reordering that maps two
    // different variants to the same u8, or any mask-width mismatch.
    // ========================================================================

    macro_rules! roundtrip_all {
        ($name:ident, $to:ident, $from:ident, [$($variant:expr),+ $(,)?]) => {
            #[test]
            fn $name() {
                for v in [$($variant),+] {
                    let u = $to(v);
                    let decoded = $from(u);
                    assert_eq!(decoded, v, "{:?} != {:?} (via u8 = {})", decoded, v, u);
                }
            }
        };
    }

    roundtrip_all!(rt_display, layout_display_to_u8, layout_display_from_u8, [
        LayoutDisplay::Block, LayoutDisplay::Inline, LayoutDisplay::InlineBlock,
        LayoutDisplay::Flex, LayoutDisplay::None, LayoutDisplay::InlineFlex,
        LayoutDisplay::Table, LayoutDisplay::InlineTable, LayoutDisplay::TableRowGroup,
        LayoutDisplay::TableHeaderGroup, LayoutDisplay::TableFooterGroup,
        LayoutDisplay::TableRow, LayoutDisplay::TableColumnGroup,
        LayoutDisplay::TableColumn, LayoutDisplay::TableCell,
        LayoutDisplay::TableCaption, LayoutDisplay::FlowRoot,
        LayoutDisplay::ListItem, LayoutDisplay::RunIn, LayoutDisplay::Marker,
        LayoutDisplay::Grid, LayoutDisplay::InlineGrid, LayoutDisplay::Contents,
    ]);

    roundtrip_all!(rt_position, layout_position_to_u8, layout_position_from_u8, [
        LayoutPosition::Static, LayoutPosition::Relative, LayoutPosition::Absolute,
        LayoutPosition::Fixed, LayoutPosition::Sticky,
    ]);

    roundtrip_all!(rt_float, layout_float_to_u8, layout_float_from_u8, [
        LayoutFloat::None, LayoutFloat::Left, LayoutFloat::Right,
    ]);

    roundtrip_all!(rt_overflow, layout_overflow_to_u8, layout_overflow_from_u8, [
        LayoutOverflow::Visible, LayoutOverflow::Hidden, LayoutOverflow::Scroll,
        LayoutOverflow::Auto, LayoutOverflow::Clip,
    ]);

    roundtrip_all!(rt_box_sizing, layout_box_sizing_to_u8, layout_box_sizing_from_u8, [
        LayoutBoxSizing::ContentBox, LayoutBoxSizing::BorderBox,
    ]);

    roundtrip_all!(rt_flex_direction, layout_flex_direction_to_u8, layout_flex_direction_from_u8, [
        LayoutFlexDirection::Row, LayoutFlexDirection::RowReverse,
        LayoutFlexDirection::Column, LayoutFlexDirection::ColumnReverse,
    ]);

    roundtrip_all!(rt_flex_wrap, layout_flex_wrap_to_u8, layout_flex_wrap_from_u8, [
        LayoutFlexWrap::NoWrap, LayoutFlexWrap::Wrap, LayoutFlexWrap::WrapReverse,
    ]);

    roundtrip_all!(rt_justify_content, layout_justify_content_to_u8, layout_justify_content_from_u8, [
        LayoutJustifyContent::FlexStart, LayoutJustifyContent::FlexEnd,
        LayoutJustifyContent::Start, LayoutJustifyContent::End,
        LayoutJustifyContent::Center, LayoutJustifyContent::SpaceBetween,
        LayoutJustifyContent::SpaceAround, LayoutJustifyContent::SpaceEvenly,
    ]);

    roundtrip_all!(rt_align_items, layout_align_items_to_u8, layout_align_items_from_u8, [
        LayoutAlignItems::Stretch, LayoutAlignItems::Center, LayoutAlignItems::Start,
        LayoutAlignItems::End, LayoutAlignItems::Baseline,
    ]);

    roundtrip_all!(rt_align_self, layout_align_self_to_u8, layout_align_self_from_u8, [
        LayoutAlignSelf::Auto, LayoutAlignSelf::Stretch, LayoutAlignSelf::Center,
        LayoutAlignSelf::Start, LayoutAlignSelf::End, LayoutAlignSelf::Baseline,
    ]);

    roundtrip_all!(rt_justify_self, layout_justify_self_to_u8, layout_justify_self_from_u8, [
        LayoutJustifySelf::Auto, LayoutJustifySelf::Start, LayoutJustifySelf::End,
        LayoutJustifySelf::Center, LayoutJustifySelf::Stretch,
    ]);

    roundtrip_all!(rt_justify_items, layout_justify_items_to_u8, layout_justify_items_from_u8, [
        LayoutJustifyItems::Stretch, LayoutJustifyItems::Start,
        LayoutJustifyItems::End, LayoutJustifyItems::Center,
    ]);

    roundtrip_all!(rt_grid_auto_flow, layout_grid_auto_flow_to_u8, layout_grid_auto_flow_from_u8, [
        LayoutGridAutoFlow::Row, LayoutGridAutoFlow::Column,
        LayoutGridAutoFlow::RowDense, LayoutGridAutoFlow::ColumnDense,
    ]);

    roundtrip_all!(rt_align_content, layout_align_content_to_u8, layout_align_content_from_u8, [
        LayoutAlignContent::Stretch, LayoutAlignContent::Center,
        LayoutAlignContent::Start, LayoutAlignContent::End,
        LayoutAlignContent::SpaceBetween, LayoutAlignContent::SpaceAround,
    ]);

    roundtrip_all!(rt_writing_mode, layout_writing_mode_to_u8, layout_writing_mode_from_u8, [
        LayoutWritingMode::HorizontalTb, LayoutWritingMode::VerticalRl,
        LayoutWritingMode::VerticalLr,
    ]);

    roundtrip_all!(rt_clear, layout_clear_to_u8, layout_clear_from_u8, [
        LayoutClear::None, LayoutClear::Left, LayoutClear::Right, LayoutClear::Both,
    ]);

    roundtrip_all!(rt_font_weight, style_font_weight_to_u8, style_font_weight_from_u8, [
        StyleFontWeight::Normal, StyleFontWeight::W100, StyleFontWeight::W200,
        StyleFontWeight::W300, StyleFontWeight::W500, StyleFontWeight::W600,
        StyleFontWeight::Bold, StyleFontWeight::W800, StyleFontWeight::W900,
        StyleFontWeight::Lighter, StyleFontWeight::Bolder,
    ]);

    roundtrip_all!(rt_font_style, style_font_style_to_u8, style_font_style_from_u8, [
        StyleFontStyle::Normal, StyleFontStyle::Italic, StyleFontStyle::Oblique,
    ]);

    roundtrip_all!(rt_text_align, style_text_align_to_u8, style_text_align_from_u8, [
        StyleTextAlign::Left, StyleTextAlign::Center, StyleTextAlign::Right,
        StyleTextAlign::Justify, StyleTextAlign::Start, StyleTextAlign::End,
    ]);

    roundtrip_all!(rt_visibility, style_visibility_to_u8, style_visibility_from_u8, [
        StyleVisibility::Visible, StyleVisibility::Hidden, StyleVisibility::Collapse,
    ]);

    roundtrip_all!(rt_white_space, style_white_space_to_u8, style_white_space_from_u8, [
        StyleWhiteSpace::Normal, StyleWhiteSpace::Pre, StyleWhiteSpace::Nowrap,
        StyleWhiteSpace::PreWrap, StyleWhiteSpace::PreLine, StyleWhiteSpace::BreakSpaces,
    ]);

    roundtrip_all!(rt_direction, style_direction_to_u8, style_direction_from_u8, [
        StyleDirection::Ltr, StyleDirection::Rtl,
    ]);

    roundtrip_all!(rt_vertical_align, style_vertical_align_to_u8, style_vertical_align_from_u8, [
        StyleVerticalAlign::Baseline, StyleVerticalAlign::Top, StyleVerticalAlign::Middle,
        StyleVerticalAlign::Bottom, StyleVerticalAlign::Sub, StyleVerticalAlign::Superscript,
        StyleVerticalAlign::TextTop, StyleVerticalAlign::TextBottom,
    ]);

    roundtrip_all!(rt_border_collapse, border_collapse_to_u8, border_collapse_from_u8, [
        StyleBorderCollapse::Separate, StyleBorderCollapse::Collapse,
    ]);

    // ========================================================================
    // Bit-layout safety: every encoder must produce a u8 whose bits all
    // fit inside the mask allocated for that property in the tier1 u64.
    // If an enum grows a new variant that overflows its mask, the encoded
    // bits would leak into the next property's slot and silently corrupt
    // unrelated state.
    // ========================================================================

    #[test]
    fn test_encoded_u8_fits_in_tier1_mask() {
        fn assert_fits(name: &str, val: u8, mask: u64) {
            assert!(
                u64::from(val) & !mask == 0,
                "{name}: encoded u8 {val} overflows mask {mask:b}",
            );
        }

        assert_fits("display", layout_display_to_u8(LayoutDisplay::Contents), DISPLAY_MASK);
        assert_fits("position", layout_position_to_u8(LayoutPosition::Sticky), POSITION_MASK);
        assert_fits("float", layout_float_to_u8(LayoutFloat::Right), FLOAT_MASK);
        assert_fits("overflow", layout_overflow_to_u8(LayoutOverflow::Clip), OVERFLOW_MASK);
        assert_fits("box_sizing", layout_box_sizing_to_u8(LayoutBoxSizing::BorderBox), BOX_SIZING_MASK);
        assert_fits("flex_direction", layout_flex_direction_to_u8(LayoutFlexDirection::ColumnReverse), FLEX_DIR_MASK);
        assert_fits("flex_wrap", layout_flex_wrap_to_u8(LayoutFlexWrap::WrapReverse), FLEX_WRAP_MASK);
        assert_fits("justify_content", layout_justify_content_to_u8(LayoutJustifyContent::SpaceEvenly), JUSTIFY_MASK);
        assert_fits("align_items", layout_align_items_to_u8(LayoutAlignItems::Baseline), ALIGN_MASK);
        assert_fits("align_self", layout_align_self_to_u8(LayoutAlignSelf::Baseline), ALIGN_SELF_MASK);
        assert_fits("justify_self", layout_justify_self_to_u8(LayoutJustifySelf::Stretch), JUSTIFY_SELF_MASK);
        assert_fits("justify_items", layout_justify_items_to_u8(LayoutJustifyItems::Center), JUSTIFY_ITEMS_MASK);
        assert_fits("grid_auto_flow", layout_grid_auto_flow_to_u8(LayoutGridAutoFlow::ColumnDense), GRID_AUTO_FLOW_MASK);
        assert_fits("align_content", layout_align_content_to_u8(LayoutAlignContent::SpaceAround), ALIGN_MASK);
        assert_fits("writing_mode", layout_writing_mode_to_u8(LayoutWritingMode::VerticalLr), WRITING_MODE_MASK);
        assert_fits("clear", layout_clear_to_u8(LayoutClear::Both), CLEAR_MASK);
        assert_fits("font_weight", style_font_weight_to_u8(StyleFontWeight::Bolder), FONT_WEIGHT_MASK);
        assert_fits("font_style", style_font_style_to_u8(StyleFontStyle::Oblique), FONT_STYLE_MASK);
        assert_fits("text_align", style_text_align_to_u8(StyleTextAlign::End), TEXT_ALIGN_MASK);
        assert_fits("visibility", style_visibility_to_u8(StyleVisibility::Collapse), VISIBILITY_MASK);
        assert_fits("white_space", style_white_space_to_u8(StyleWhiteSpace::BreakSpaces), WHITE_SPACE_MASK);
        assert_fits("direction", style_direction_to_u8(StyleDirection::Rtl), DIRECTION_MASK);
        assert_fits("vertical_align", style_vertical_align_to_u8(StyleVerticalAlign::TextBottom), VERTICAL_ALIGN_MASK);
        assert_fits("border_collapse", border_collapse_to_u8(StyleBorderCollapse::Collapse), BORDER_COLLAPSE_MASK);
    }

    // ========================================================================
    // Empty tier1 decodes to all-initial — this is the core contract that
    // was violated by the pre-fix justify_items encoding. An empty u64 with
    // only TIER1_POPULATED_BIT set must decode every property to its CSS
    // initial value; this is how `build_compact_cache` can leave unspecified
    // properties at 0 and still produce the correct cascade result.
    // ========================================================================

    #[test]
    fn test_empty_tier1_decodes_to_initial_values() {
        let t1 = TIER1_POPULATED_BIT; // populated, but zero content
        assert!(tier1_is_populated(t1));
        assert_eq!(decode_display(t1), LayoutDisplay::Block);
        assert_eq!(decode_position(t1), LayoutPosition::Static);
        assert_eq!(decode_float(t1), LayoutFloat::None);
        assert_eq!(decode_overflow_x(t1), LayoutOverflow::Visible);
        assert_eq!(decode_overflow_y(t1), LayoutOverflow::Visible);
        assert_eq!(decode_box_sizing(t1), LayoutBoxSizing::ContentBox);
        assert_eq!(decode_flex_direction(t1), LayoutFlexDirection::Row);
        assert_eq!(decode_flex_wrap(t1), LayoutFlexWrap::NoWrap);
        assert_eq!(decode_justify_content(t1), LayoutJustifyContent::FlexStart);
        assert_eq!(decode_align_items(t1), LayoutAlignItems::Stretch);
        assert_eq!(decode_align_content(t1), LayoutAlignContent::Stretch);
        assert_eq!(decode_writing_mode(t1), LayoutWritingMode::HorizontalTb);
        assert_eq!(decode_clear(t1), LayoutClear::None);
        assert_eq!(decode_font_weight(t1), StyleFontWeight::Normal);
        assert_eq!(decode_font_style(t1), StyleFontStyle::Normal);
        assert_eq!(decode_text_align(t1), StyleTextAlign::Left);
        assert_eq!(decode_visibility(t1), StyleVisibility::Visible);
        assert_eq!(decode_white_space(t1), StyleWhiteSpace::Normal);
        assert_eq!(decode_direction(t1), StyleDirection::Ltr);
        assert_eq!(decode_vertical_align(t1), StyleVerticalAlign::Baseline);
        assert_eq!(decode_border_collapse(t1), StyleBorderCollapse::Separate);
    }
}

#[cfg(test)]
#[allow(
    clippy::float_cmp,
    clippy::unreadable_literal,
    clippy::too_many_lines,
    clippy::cast_precision_loss
)]
mod autotest_generated {
    use super::*;
    use crate::props::basic::length::PercentageValue;

    // =========================================================================
    // Shared fixtures
    // =========================================================================

    const ALL_DISPLAY: [LayoutDisplay; 23] = [
        LayoutDisplay::Block,
        LayoutDisplay::Inline,
        LayoutDisplay::InlineBlock,
        LayoutDisplay::Flex,
        LayoutDisplay::None,
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

    const ALL_BORDER_STYLE: [BorderStyle; 10] = [
        BorderStyle::None,
        BorderStyle::Solid,
        BorderStyle::Double,
        BorderStyle::Dotted,
        BorderStyle::Dashed,
        BorderStyle::Hidden,
        BorderStyle::Groove,
        BorderStyle::Ridge,
        BorderStyle::Inset,
        BorderStyle::Outset,
    ];

    const ALL_SIZE_METRIC: [SizeMetric; 12] = [
        SizeMetric::Px,
        SizeMetric::Pt,
        SizeMetric::Em,
        SizeMetric::Rem,
        SizeMetric::In,
        SizeMetric::Cm,
        SizeMetric::Mm,
        SizeMetric::Percent,
        SizeMetric::Vw,
        SizeMetric::Vh,
        SizeMetric::Vmin,
        SizeMetric::Vmax,
    ];

    /// Build a `PixelValue` straight from the raw fixed-point isize (×1000),
    /// bypassing the f32 constructor so boundary rows are exact.
    fn pv_raw(metric: SizeMetric, raw: isize) -> PixelValue {
        PixelValue {
            metric,
            number: FloatValue { number: raw },
        }
    }

    /// All 21 tier-1 enum properties as one struct, so a decode can be checked
    /// field-by-field against exactly what was encoded.
    #[derive(Debug, Copy, Clone, PartialEq, Eq)]
    struct T1 {
        display: LayoutDisplay,
        position: LayoutPosition,
        float: LayoutFloat,
        overflow_x: LayoutOverflow,
        overflow_y: LayoutOverflow,
        box_sizing: LayoutBoxSizing,
        flex_direction: LayoutFlexDirection,
        flex_wrap: LayoutFlexWrap,
        justify_content: LayoutJustifyContent,
        align_items: LayoutAlignItems,
        align_content: LayoutAlignContent,
        writing_mode: LayoutWritingMode,
        clear: LayoutClear,
        font_weight: StyleFontWeight,
        font_style: StyleFontStyle,
        text_align: StyleTextAlign,
        visibility: StyleVisibility,
        white_space: StyleWhiteSpace,
        direction: StyleDirection,
        vertical_align: StyleVerticalAlign,
        border_collapse: StyleBorderCollapse,
    }

    impl T1 {
        /// Every field at its CSS initial value (the u8-0 slot).
        const fn initial() -> Self {
            Self {
                display: LayoutDisplay::Block,
                position: LayoutPosition::Static,
                float: LayoutFloat::None,
                overflow_x: LayoutOverflow::Visible,
                overflow_y: LayoutOverflow::Visible,
                box_sizing: LayoutBoxSizing::ContentBox,
                flex_direction: LayoutFlexDirection::Row,
                flex_wrap: LayoutFlexWrap::NoWrap,
                justify_content: LayoutJustifyContent::FlexStart,
                align_items: LayoutAlignItems::Stretch,
                align_content: LayoutAlignContent::Stretch,
                writing_mode: LayoutWritingMode::HorizontalTb,
                clear: LayoutClear::None,
                font_weight: StyleFontWeight::Normal,
                font_style: StyleFontStyle::Normal,
                text_align: StyleTextAlign::Left,
                visibility: StyleVisibility::Visible,
                white_space: StyleWhiteSpace::Normal,
                direction: StyleDirection::Ltr,
                vertical_align: StyleVerticalAlign::Baseline,
                border_collapse: StyleBorderCollapse::Separate,
            }
        }

        /// Every field at its highest-numbered variant — the worst case for a
        /// mask that is one bit too narrow.
        const fn saturated() -> Self {
            Self {
                display: LayoutDisplay::Contents,
                position: LayoutPosition::Sticky,
                float: LayoutFloat::Right,
                overflow_x: LayoutOverflow::Clip,
                overflow_y: LayoutOverflow::Clip,
                box_sizing: LayoutBoxSizing::BorderBox,
                flex_direction: LayoutFlexDirection::ColumnReverse,
                flex_wrap: LayoutFlexWrap::WrapReverse,
                justify_content: LayoutJustifyContent::SpaceEvenly,
                align_items: LayoutAlignItems::Baseline,
                align_content: LayoutAlignContent::SpaceAround,
                writing_mode: LayoutWritingMode::VerticalLr,
                clear: LayoutClear::Both,
                font_weight: StyleFontWeight::Bolder,
                font_style: StyleFontStyle::Oblique,
                text_align: StyleTextAlign::End,
                visibility: StyleVisibility::Collapse,
                white_space: StyleWhiteSpace::BreakSpaces,
                direction: StyleDirection::Rtl,
                vertical_align: StyleVerticalAlign::TextBottom,
                border_collapse: StyleBorderCollapse::Collapse,
            }
        }

        fn encode(self) -> u64 {
            encode_tier1(
                self.display,
                self.position,
                self.float,
                self.overflow_x,
                self.overflow_y,
                self.box_sizing,
                self.flex_direction,
                self.flex_wrap,
                self.justify_content,
                self.align_items,
                self.align_content,
                self.writing_mode,
                self.clear,
                self.font_weight,
                self.font_style,
                self.text_align,
                self.visibility,
                self.white_space,
                self.direction,
                self.vertical_align,
                self.border_collapse,
            )
        }

        fn decode(t1: u64) -> Self {
            Self {
                display: decode_display(t1),
                position: decode_position(t1),
                float: decode_float(t1),
                overflow_x: decode_overflow_x(t1),
                overflow_y: decode_overflow_y(t1),
                box_sizing: decode_box_sizing(t1),
                flex_direction: decode_flex_direction(t1),
                flex_wrap: decode_flex_wrap(t1),
                justify_content: decode_justify_content(t1),
                align_items: decode_align_items(t1),
                align_content: decode_align_content(t1),
                writing_mode: decode_writing_mode(t1),
                clear: decode_clear(t1),
                font_weight: decode_font_weight(t1),
                font_style: decode_font_style(t1),
                text_align: decode_text_align(t1),
                visibility: decode_visibility(t1),
                white_space: decode_white_space(t1),
                direction: decode_direction(t1),
                vertical_align: decode_vertical_align(t1),
                border_collapse: decode_border_collapse(t1),
            }
        }
    }

    // =========================================================================
    // u8 decoders — every byte outside the variant range must fall back to the
    // CSS initial value (the u8-0 slot), never panic, never alias a real variant
    // =========================================================================

    /// `first_invalid` = the first u8 that has no variant. Every byte from there
    /// to `u8::MAX` must decode to `initial`.
    fn assert_u8_fallback<T: PartialEq + core::fmt::Debug + Copy>(
        name: &str,
        decode: fn(u8) -> T,
        first_invalid: u8,
        initial: T,
    ) {
        for v in first_invalid..=u8::MAX {
            assert_eq!(
                decode(v),
                initial,
                "{name}: out-of-range byte {v} must fall back to the CSS initial value",
            );
        }
    }

    #[test]
    fn out_of_range_u8_falls_back_to_css_initial_for_every_enum() {
        assert_u8_fallback("display", layout_display_from_u8, 23, LayoutDisplay::Block);
        assert_u8_fallback("position", layout_position_from_u8, 5, LayoutPosition::Static);
        assert_u8_fallback("float", layout_float_from_u8, 3, LayoutFloat::None);
        assert_u8_fallback("overflow", layout_overflow_from_u8, 5, LayoutOverflow::Visible);
        assert_u8_fallback(
            "box_sizing",
            layout_box_sizing_from_u8,
            2,
            LayoutBoxSizing::ContentBox,
        );
        assert_u8_fallback(
            "flex_direction",
            layout_flex_direction_from_u8,
            4,
            LayoutFlexDirection::Row,
        );
        assert_u8_fallback("flex_wrap", layout_flex_wrap_from_u8, 3, LayoutFlexWrap::NoWrap);
        assert_u8_fallback(
            "justify_content",
            layout_justify_content_from_u8,
            8,
            LayoutJustifyContent::FlexStart,
        );
        assert_u8_fallback(
            "align_items",
            layout_align_items_from_u8,
            5,
            LayoutAlignItems::Stretch,
        );
        assert_u8_fallback("align_self", layout_align_self_from_u8, 6, LayoutAlignSelf::Auto);
        assert_u8_fallback(
            "justify_self",
            layout_justify_self_from_u8,
            5,
            LayoutJustifySelf::Auto,
        );
        assert_u8_fallback(
            "justify_items",
            layout_justify_items_from_u8,
            4,
            LayoutJustifyItems::Stretch,
        );
        assert_u8_fallback(
            "grid_auto_flow",
            layout_grid_auto_flow_from_u8,
            4,
            LayoutGridAutoFlow::Row,
        );
        assert_u8_fallback(
            "align_content",
            layout_align_content_from_u8,
            6,
            LayoutAlignContent::Stretch,
        );
        assert_u8_fallback(
            "writing_mode",
            layout_writing_mode_from_u8,
            3,
            LayoutWritingMode::HorizontalTb,
        );
        assert_u8_fallback("clear", layout_clear_from_u8, 4, LayoutClear::None);
        assert_u8_fallback(
            "font_weight",
            style_font_weight_from_u8,
            11,
            StyleFontWeight::Normal,
        );
        assert_u8_fallback("font_style", style_font_style_from_u8, 3, StyleFontStyle::Normal);
        assert_u8_fallback("text_align", style_text_align_from_u8, 6, StyleTextAlign::Left);
        assert_u8_fallback(
            "visibility",
            style_visibility_from_u8,
            3,
            StyleVisibility::Visible,
        );
        assert_u8_fallback(
            "white_space",
            style_white_space_from_u8,
            6,
            StyleWhiteSpace::Normal,
        );
        assert_u8_fallback("direction", style_direction_from_u8, 2, StyleDirection::Ltr);
        assert_u8_fallback(
            "vertical_align",
            style_vertical_align_from_u8,
            8,
            StyleVerticalAlign::Baseline,
        );
        assert_u8_fallback(
            "border_collapse",
            border_collapse_from_u8,
            2,
            StyleBorderCollapse::Separate,
        );
        assert_u8_fallback("border_style", border_style_from_u8, 10, BorderStyle::None);
        assert_u8_fallback("size_metric", size_metric_from_u8, 12, SizeMetric::Px);
    }

    #[test]
    fn display_documented_sentinel_31_decodes_to_block() {
        // The module doc promises 0x1F is the "look it up in the slow path"
        // sentinel and that decoding it yields the default rather than a
        // garbage variant. 31 is also DISPLAY_MASK, so a saturated tier1 u64
        // lands exactly here.
        assert_eq!(layout_display_from_u8(31), LayoutDisplay::Block);
        assert_eq!(layout_display_from_u8(u8::MAX), LayoutDisplay::Block);
    }

    #[test]
    fn every_variant_of_every_enum_fits_its_tier1_mask() {
        // The existing suite checks only the highest variant per enum. If a new
        // variant is inserted in the middle with a hand-written u8 above the
        // mask width, that check still passes while the encoding silently
        // corrupts the neighbouring bit run. Check the whole variant set.
        fn fits(name: &str, val: u8, mask: u64) {
            assert!(
                u64::from(val) & !mask == 0,
                "{name}: encoded u8 {val} does not fit mask {mask:#b}",
            );
        }

        for v in ALL_DISPLAY {
            fits("display", layout_display_to_u8(v), DISPLAY_MASK);
        }
        for v in [
            LayoutPosition::Static,
            LayoutPosition::Relative,
            LayoutPosition::Absolute,
            LayoutPosition::Fixed,
            LayoutPosition::Sticky,
        ] {
            fits("position", layout_position_to_u8(v), POSITION_MASK);
        }
        for v in [LayoutFloat::None, LayoutFloat::Left, LayoutFloat::Right] {
            fits("float", layout_float_to_u8(v), FLOAT_MASK);
        }
        for v in [
            LayoutOverflow::Visible,
            LayoutOverflow::Hidden,
            LayoutOverflow::Scroll,
            LayoutOverflow::Auto,
            LayoutOverflow::Clip,
        ] {
            fits("overflow", layout_overflow_to_u8(v), OVERFLOW_MASK);
        }
        for v in [LayoutBoxSizing::ContentBox, LayoutBoxSizing::BorderBox] {
            fits("box_sizing", layout_box_sizing_to_u8(v), BOX_SIZING_MASK);
        }
        for v in [
            LayoutFlexDirection::Row,
            LayoutFlexDirection::RowReverse,
            LayoutFlexDirection::Column,
            LayoutFlexDirection::ColumnReverse,
        ] {
            fits("flex_direction", layout_flex_direction_to_u8(v), FLEX_DIR_MASK);
        }
        for v in [
            LayoutFlexWrap::NoWrap,
            LayoutFlexWrap::Wrap,
            LayoutFlexWrap::WrapReverse,
        ] {
            fits("flex_wrap", layout_flex_wrap_to_u8(v), FLEX_WRAP_MASK);
        }
        for v in [
            LayoutJustifyContent::FlexStart,
            LayoutJustifyContent::FlexEnd,
            LayoutJustifyContent::Start,
            LayoutJustifyContent::End,
            LayoutJustifyContent::Center,
            LayoutJustifyContent::SpaceBetween,
            LayoutJustifyContent::SpaceAround,
            LayoutJustifyContent::SpaceEvenly,
        ] {
            fits("justify_content", layout_justify_content_to_u8(v), JUSTIFY_MASK);
        }
        for v in [
            LayoutAlignItems::Stretch,
            LayoutAlignItems::Center,
            LayoutAlignItems::Start,
            LayoutAlignItems::End,
            LayoutAlignItems::Baseline,
        ] {
            fits("align_items", layout_align_items_to_u8(v), ALIGN_MASK);
        }
        for v in [
            LayoutAlignSelf::Auto,
            LayoutAlignSelf::Stretch,
            LayoutAlignSelf::Center,
            LayoutAlignSelf::Start,
            LayoutAlignSelf::End,
            LayoutAlignSelf::Baseline,
        ] {
            fits("align_self", layout_align_self_to_u8(v), ALIGN_SELF_MASK);
        }
        for v in [
            LayoutJustifySelf::Auto,
            LayoutJustifySelf::Start,
            LayoutJustifySelf::End,
            LayoutJustifySelf::Center,
            LayoutJustifySelf::Stretch,
        ] {
            fits("justify_self", layout_justify_self_to_u8(v), JUSTIFY_SELF_MASK);
        }
        for v in [
            LayoutJustifyItems::Stretch,
            LayoutJustifyItems::Start,
            LayoutJustifyItems::End,
            LayoutJustifyItems::Center,
        ] {
            fits("justify_items", layout_justify_items_to_u8(v), JUSTIFY_ITEMS_MASK);
        }
        for v in [
            LayoutGridAutoFlow::Row,
            LayoutGridAutoFlow::Column,
            LayoutGridAutoFlow::RowDense,
            LayoutGridAutoFlow::ColumnDense,
        ] {
            fits("grid_auto_flow", layout_grid_auto_flow_to_u8(v), GRID_AUTO_FLOW_MASK);
        }
        for v in [
            LayoutAlignContent::Stretch,
            LayoutAlignContent::Center,
            LayoutAlignContent::Start,
            LayoutAlignContent::End,
            LayoutAlignContent::SpaceBetween,
            LayoutAlignContent::SpaceAround,
        ] {
            fits("align_content", layout_align_content_to_u8(v), ALIGN_MASK);
        }
        for v in [
            LayoutWritingMode::HorizontalTb,
            LayoutWritingMode::VerticalRl,
            LayoutWritingMode::VerticalLr,
        ] {
            fits("writing_mode", layout_writing_mode_to_u8(v), WRITING_MODE_MASK);
        }
        for v in [
            LayoutClear::None,
            LayoutClear::Left,
            LayoutClear::Right,
            LayoutClear::Both,
        ] {
            fits("clear", layout_clear_to_u8(v), CLEAR_MASK);
        }
        for v in [
            StyleFontWeight::Normal,
            StyleFontWeight::W100,
            StyleFontWeight::W200,
            StyleFontWeight::W300,
            StyleFontWeight::W500,
            StyleFontWeight::W600,
            StyleFontWeight::Bold,
            StyleFontWeight::W800,
            StyleFontWeight::W900,
            StyleFontWeight::Lighter,
            StyleFontWeight::Bolder,
        ] {
            fits("font_weight", style_font_weight_to_u8(v), FONT_WEIGHT_MASK);
        }
        for v in [
            StyleFontStyle::Normal,
            StyleFontStyle::Italic,
            StyleFontStyle::Oblique,
        ] {
            fits("font_style", style_font_style_to_u8(v), FONT_STYLE_MASK);
        }
        for v in [
            StyleTextAlign::Left,
            StyleTextAlign::Center,
            StyleTextAlign::Right,
            StyleTextAlign::Justify,
            StyleTextAlign::Start,
            StyleTextAlign::End,
        ] {
            fits("text_align", style_text_align_to_u8(v), TEXT_ALIGN_MASK);
        }
        for v in [
            StyleVisibility::Visible,
            StyleVisibility::Hidden,
            StyleVisibility::Collapse,
        ] {
            fits("visibility", style_visibility_to_u8(v), VISIBILITY_MASK);
        }
        for v in [
            StyleWhiteSpace::Normal,
            StyleWhiteSpace::Pre,
            StyleWhiteSpace::Nowrap,
            StyleWhiteSpace::PreWrap,
            StyleWhiteSpace::PreLine,
            StyleWhiteSpace::BreakSpaces,
        ] {
            fits("white_space", style_white_space_to_u8(v), WHITE_SPACE_MASK);
        }
        for v in [StyleDirection::Ltr, StyleDirection::Rtl] {
            fits("direction", style_direction_to_u8(v), DIRECTION_MASK);
        }
        for v in [
            StyleVerticalAlign::Baseline,
            StyleVerticalAlign::Top,
            StyleVerticalAlign::Middle,
            StyleVerticalAlign::Bottom,
            StyleVerticalAlign::Sub,
            StyleVerticalAlign::Superscript,
            StyleVerticalAlign::TextTop,
            StyleVerticalAlign::TextBottom,
            StyleVerticalAlign::Percentage(PercentageValue::new(50.0)),
            StyleVerticalAlign::Length(PixelValue::px(4.0)),
        ] {
            fits("vertical_align", style_vertical_align_to_u8(v), VERTICAL_ALIGN_MASK);
        }
        for v in [StyleBorderCollapse::Separate, StyleBorderCollapse::Collapse] {
            fits("border_collapse", border_collapse_to_u8(v), BORDER_COLLAPSE_MASK);
        }
        // border-style gets a 4-bit nibble inside the packed u16, not a tier1 mask.
        for v in ALL_BORDER_STYLE {
            fits("border_style", border_style_to_u8(v), 0x0F);
        }
        // SizeMetric gets the low 4 bits of the pixel-value u32.
        for v in ALL_SIZE_METRIC {
            fits("size_metric", size_metric_to_u8(v), 0x0F);
        }
    }

    #[test]
    fn vertical_align_percentage_and_length_collapse_to_baseline() {
        // Documented lossy fallback: the 3-bit tier1 slot cannot carry a
        // length/percentage payload, so these must encode as 0 (Baseline) and
        // the caller is expected to take the slow path. What must NOT happen is
        // an out-of-range u8 leaking into the border-collapse bit next door.
        for v in [
            StyleVerticalAlign::Percentage(PercentageValue::new(0.0)),
            StyleVerticalAlign::Percentage(PercentageValue::new(-100.0)),
            StyleVerticalAlign::Percentage(PercentageValue::new(1e9)),
            StyleVerticalAlign::Length(PixelValue::px(0.0)),
            StyleVerticalAlign::Length(PixelValue::px(-1e9)),
        ] {
            assert_eq!(style_vertical_align_to_u8(v), 0);
            assert_eq!(
                style_vertical_align_from_u8(style_vertical_align_to_u8(v)),
                StyleVerticalAlign::Baseline,
            );
        }

        // …and the collapse must not disturb the neighbouring border-collapse bit.
        let mut t = T1::initial();
        t.vertical_align = StyleVerticalAlign::Percentage(PercentageValue::new(150.0));
        t.border_collapse = StyleBorderCollapse::Collapse;
        let encoded = t.encode();
        assert_eq!(decode_vertical_align(encoded), StyleVerticalAlign::Baseline);
        assert_eq!(decode_border_collapse(encoded), StyleBorderCollapse::Collapse);
    }

    // =========================================================================
    // Tier1 u64 packing — field isolation, saturation, arbitrary input
    // =========================================================================

    #[test]
    fn tier1_each_field_at_max_does_not_leak_into_any_other_field() {
        // One field at its highest variant, all others at CSS initial. If a
        // shift/mask pair is wrong, the extra bits land in a neighbour and that
        // neighbour decodes to something other than its initial value.
        let sat = T1::saturated();
        let mut cases: Vec<(&str, T1)> = Vec::new();

        macro_rules! case {
            ($field:ident) => {{
                let mut t = T1::initial();
                t.$field = sat.$field;
                cases.push((stringify!($field), t));
            }};
        }

        case!(display);
        case!(position);
        case!(float);
        case!(overflow_x);
        case!(overflow_y);
        case!(box_sizing);
        case!(flex_direction);
        case!(flex_wrap);
        case!(justify_content);
        case!(align_items);
        case!(align_content);
        case!(writing_mode);
        case!(clear);
        case!(font_weight);
        case!(font_style);
        case!(text_align);
        case!(visibility);
        case!(white_space);
        case!(direction);
        case!(vertical_align);
        case!(border_collapse);

        assert_eq!(cases.len(), 21, "every tier1 field must be covered");

        for (name, expected) in cases {
            let decoded = T1::decode(expected.encode());
            assert_eq!(
                decoded, expected,
                "{name} at its max variant leaked into another tier1 field",
            );
        }
    }

    #[test]
    fn tier1_all_fields_saturated_roundtrips() {
        let sat = T1::saturated();
        assert_eq!(T1::decode(sat.encode()), sat);
        assert!(tier1_is_populated(sat.encode()));
    }

    #[test]
    fn tier1_encode_never_touches_the_grid_bits_or_bits_above_63() {
        // encode_tier1 owns bits [52:0] plus the populated flag at bit 63.
        // Bits [62:53] belong to align-self / justify-self / grid-auto-flow /
        // justify-items, which the cache builder ORs in separately. If
        // encode_tier1 ever spills into that window it silently rewrites a grid
        // property that it never received as an argument.
        const GRID_WINDOW: u64 = 0x3FF << 53; // bits 53..=62

        for t in [T1::initial(), T1::saturated()] {
            let encoded = t.encode();
            assert_eq!(
                encoded & GRID_WINDOW,
                0,
                "encode_tier1 wrote into the grid bit window [62:53]",
            );
            assert_eq!(encoded & TIER1_POPULATED_BIT, TIER1_POPULATED_BIT);
        }

        // The grid fields must survive being ORed on top of a saturated tier1.
        let base = T1::saturated().encode();
        let with_grid = base
            | ((u64::from(layout_align_self_to_u8(LayoutAlignSelf::Baseline))) << ALIGN_SELF_SHIFT)
            | ((u64::from(layout_justify_self_to_u8(LayoutJustifySelf::Stretch)))
                << JUSTIFY_SELF_SHIFT)
            | ((u64::from(layout_grid_auto_flow_to_u8(LayoutGridAutoFlow::ColumnDense)))
                << GRID_AUTO_FLOW_SHIFT)
            | ((u64::from(layout_justify_items_to_u8(LayoutJustifyItems::Center)))
                << JUSTIFY_ITEMS_SHIFT);

        assert_eq!(T1::decode(with_grid), T1::saturated());
        assert_eq!(
            layout_align_self_from_u8(((with_grid >> ALIGN_SELF_SHIFT) & ALIGN_SELF_MASK) as u8),
            LayoutAlignSelf::Baseline,
        );
        assert_eq!(
            layout_justify_self_from_u8(
                ((with_grid >> JUSTIFY_SELF_SHIFT) & JUSTIFY_SELF_MASK) as u8
            ),
            LayoutJustifySelf::Stretch,
        );
        assert_eq!(
            layout_grid_auto_flow_from_u8(
                ((with_grid >> GRID_AUTO_FLOW_SHIFT) & GRID_AUTO_FLOW_MASK) as u8
            ),
            LayoutGridAutoFlow::ColumnDense,
        );
        assert_eq!(
            layout_justify_items_from_u8(
                ((with_grid >> JUSTIFY_ITEMS_SHIFT) & JUSTIFY_ITEMS_MASK) as u8
            ),
            LayoutJustifyItems::Center,
        );
    }

    #[test]
    fn tier1_zero_is_unpopulated_but_still_decodes_to_css_initial() {
        // A `with_capacity`-allocated cache holds 0 for every node until the
        // builder fills it in. Reading such a node must be safe and yield the
        // CSS initial value, not a garbage variant.
        assert!(!tier1_is_populated(0));
        assert_eq!(T1::decode(0), T1::initial());
    }

    #[test]
    fn tier1_decodes_arbitrary_u64_deterministically_without_panic() {
        // u64::MAX means every mask reads all-ones. Each decoder must clamp to a
        // real variant (usually the initial value via the `_` arm) rather than
        // panic or produce an out-of-range discriminant.
        let m = u64::MAX;
        assert!(tier1_is_populated(m));
        assert_eq!(
            T1::decode(m),
            T1 {
                display: LayoutDisplay::Block,          // 31 → fallback
                position: LayoutPosition::Static,       // 7  → fallback
                float: LayoutFloat::None,               // 3  → fallback
                overflow_x: LayoutOverflow::Visible,    // 7  → fallback
                overflow_y: LayoutOverflow::Visible,    // 7  → fallback
                box_sizing: LayoutBoxSizing::BorderBox, // 1  → real variant
                flex_direction: LayoutFlexDirection::ColumnReverse, // 3 → real
                flex_wrap: LayoutFlexWrap::NoWrap,      // 3  → fallback
                justify_content: LayoutJustifyContent::SpaceEvenly, // 7 → real
                align_items: LayoutAlignItems::Stretch, // 7  → fallback
                align_content: LayoutAlignContent::Stretch, // 7 → fallback
                writing_mode: LayoutWritingMode::HorizontalTb, // 3 → fallback
                clear: LayoutClear::Both,               // 3  → real variant
                font_weight: StyleFontWeight::Normal,   // 15 → fallback
                font_style: StyleFontStyle::Normal,     // 3  → fallback
                text_align: StyleTextAlign::Left,       // 7  → fallback
                visibility: StyleVisibility::Visible,   // 3  → fallback
                white_space: StyleWhiteSpace::Normal,   // 7  → fallback
                direction: StyleDirection::Rtl,         // 1  → real variant
                vertical_align: StyleVerticalAlign::TextBottom, // 7 → real
                border_collapse: StyleBorderCollapse::Collapse, // 1 → real
            },
        );

        // A deterministic sweep of adversarial bit patterns: none may panic.
        let mut x: u64 = 0x9E37_79B9_7F4A_7C15;
        for _ in 0..4096 {
            let _ = T1::decode(x);
            let _ = tier1_is_populated(x);
            x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        }
        for x in [0u64, 1, u64::MAX, 0xAAAA_AAAA_AAAA_AAAA, 0x5555_5555_5555_5555] {
            let _ = T1::decode(x);
        }
    }

    // =========================================================================
    // Packed border styles (u16, four nibbles)
    // =========================================================================

    #[test]
    fn border_styles_packed_roundtrip_for_all_10000_combinations() {
        for top in ALL_BORDER_STYLE {
            for right in ALL_BORDER_STYLE {
                for bottom in ALL_BORDER_STYLE {
                    for left in ALL_BORDER_STYLE {
                        let packed = encode_border_styles_packed(top, right, bottom, left);
                        assert_eq!(decode_border_top_style(packed), top);
                        assert_eq!(decode_border_right_style(packed), right);
                        assert_eq!(decode_border_bottom_style(packed), bottom);
                        assert_eq!(decode_border_left_style(packed), left);
                    }
                }
            }
        }
    }

    #[test]
    fn border_styles_packed_nibbles_do_not_alias() {
        // Only the top nibble set — the other three must read as None (0),
        // not pick up bits from their neighbours.
        let packed = encode_border_styles_packed(
            BorderStyle::Outset, // 9 — the widest valid nibble
            BorderStyle::None,
            BorderStyle::None,
            BorderStyle::None,
        );
        assert_eq!(packed, 0x0009);
        assert_eq!(decode_border_top_style(packed), BorderStyle::Outset);
        assert_eq!(decode_border_right_style(packed), BorderStyle::None);
        assert_eq!(decode_border_bottom_style(packed), BorderStyle::None);
        assert_eq!(decode_border_left_style(packed), BorderStyle::None);

        let packed = encode_border_styles_packed(
            BorderStyle::None,
            BorderStyle::None,
            BorderStyle::None,
            BorderStyle::Outset,
        );
        assert_eq!(packed, 0x9000);
        assert_eq!(decode_border_left_style(packed), BorderStyle::Outset);
        assert_eq!(decode_border_top_style(packed), BorderStyle::None);
    }

    #[test]
    fn border_styles_packed_decodes_garbage_u16_without_panic() {
        // Nibbles 10..=15 have no variant. `0xFFFF` (an all-ones cold-tier row,
        // e.g. from a misinitialised buffer) must decode to None everywhere.
        for packed in [0u16, 0xFFFF, 0xAAAA, 0x5555, u16::MAX / 2] {
            let _ = decode_border_top_style(packed);
            let _ = decode_border_right_style(packed);
            let _ = decode_border_bottom_style(packed);
            let _ = decode_border_left_style(packed);
        }
        assert_eq!(decode_border_top_style(0xFFFF), BorderStyle::None);
        assert_eq!(decode_border_right_style(0xFFFF), BorderStyle::None);
        assert_eq!(decode_border_bottom_style(0xFFFF), BorderStyle::None);
        assert_eq!(decode_border_left_style(0xFFFF), BorderStyle::None);
        // Exhaustive: no u16 may panic any of the four decoders.
        for packed in 0..=u16::MAX {
            let _ = decode_border_top_style(packed);
            let _ = decode_border_left_style(packed);
        }
    }

    // =========================================================================
    // Colors (u32 0xRRGGBBAA)
    // =========================================================================

    #[test]
    fn color_u32_channel_order_is_rrggbbaa() {
        let c = ColorU {
            r: 0x12,
            g: 0x34,
            b: 0x56,
            a: 0x78,
        };
        assert_eq!(encode_color_u32(&c), 0x1234_5678);
        assert_eq!(decode_color_u32(0x1234_5678), Some(c));
    }

    #[test]
    fn color_u32_roundtrips_every_boundary_channel_combination() {
        for r in [0u8, 1, 127, 254, 255] {
            for g in [0u8, 1, 127, 254, 255] {
                for b in [0u8, 1, 127, 254, 255] {
                    for a in [0u8, 1, 127, 254, 255] {
                        let c = ColorU { r, g, b, a };
                        let encoded = encode_color_u32(&c);
                        if encoded == 0 {
                            // Only fully-transparent black hits the unset sentinel.
                            assert_eq!((r, g, b, a), (0, 0, 0, 0));
                            assert_eq!(decode_color_u32(encoded), None);
                        } else {
                            assert_eq!(decode_color_u32(encoded), Some(c));
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn color_u32_transparent_black_is_the_documented_unset_collision() {
        // Documented limitation, pinned so it cannot regress silently:
        // rgba(0,0,0,0) is indistinguishable from "property never set".
        let transparent_black = ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
        assert_eq!(encode_color_u32(&transparent_black), 0);
        assert_eq!(decode_color_u32(0), None);

        // Every other alpha-0 color must still survive the round-trip.
        let transparent_red = ColorU {
            r: 255,
            g: 0,
            b: 0,
            a: 0,
        };
        assert_eq!(encode_color_u32(&transparent_red), 0xFF00_0000);
        assert_eq!(decode_color_u32(0xFF00_0000), Some(transparent_red));

        // …including "black but only just" (alpha 1).
        let almost = ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 1,
        };
        assert_eq!(decode_color_u32(encode_color_u32(&almost)), Some(almost));
    }

    #[test]
    fn color_u32_max_decodes_to_opaque_white() {
        assert_eq!(
            decode_color_u32(u32::MAX),
            Some(ColorU {
                r: 255,
                g: 255,
                b: 255,
                a: 255
            }),
        );
    }

    // =========================================================================
    // PixelValue u32 (4-bit metric + 28-bit signed fixed-point ×1000)
    // =========================================================================

    #[test]
    fn pixel_value_u32_sentinels_all_decode_to_none_and_are_distinct() {
        let sentinels = [
            U32_SENTINEL,
            U32_AUTO,
            U32_NONE,
            U32_INHERIT,
            U32_INITIAL,
            U32_MIN_CONTENT,
            U32_MAX_CONTENT,
        ];
        for (i, a) in sentinels.iter().enumerate() {
            assert!(
                *a >= U32_SENTINEL_THRESHOLD,
                "sentinel {a:#X} sits below the threshold and would decode as a value",
            );
            assert_eq!(decode_pixel_value_u32(*a), None);
            for b in &sentinels[i + 1..] {
                assert_ne!(a, b, "two u32 sentinels share a bit pattern");
            }
        }
        // The threshold itself is the lowest reserved value.
        assert_eq!(decode_pixel_value_u32(U32_SENTINEL_THRESHOLD), None);
        // One below the threshold is still a real (negative) value.
        assert!(decode_pixel_value_u32(U32_SENTINEL_THRESHOLD - 1).is_some());
    }

    #[test]
    fn pixel_value_u32_roundtrips_at_the_28_bit_boundaries_for_every_metric() {
        // ±2^27 is the documented edge of the 28-bit signed fixed-point field.
        for metric in ALL_SIZE_METRIC {
            for raw in [0isize, 1, -1, 1000, -1000, 134_217_727, -134_217_728] {
                let pv = pv_raw(metric, raw);
                let encoded = encode_pixel_value_u32(&pv);

                // Raw -1 with a high metric nibble collides with the sentinel
                // band; that is asserted separately in the bug test below.
                if encoded >= U32_SENTINEL_THRESHOLD {
                    continue;
                }

                let decoded = decode_pixel_value_u32(encoded)
                    .unwrap_or_else(|| panic!("{metric:?} raw {raw} decoded as a sentinel"));
                assert_eq!(decoded.metric, metric, "metric nibble lost for raw {raw}");
                assert_eq!(
                    decoded.number.number(),
                    raw,
                    "{metric:?}: raw {raw} did not survive the round-trip",
                );
            }
        }
    }

    #[test]
    fn pixel_value_u32_out_of_28_bit_range_returns_sentinel() {
        for metric in ALL_SIZE_METRIC {
            for raw in [
                134_217_728isize,
                -134_217_729,
                1_000_000_000,
                -1_000_000_000,
                isize::MAX,
                isize::MIN,
            ] {
                assert_eq!(
                    encode_pixel_value_u32(&pv_raw(metric, raw)),
                    U32_SENTINEL,
                    "{metric:?}: raw {raw} is outside 28 bits and must escape to tier 3",
                );
            }
        }
    }

    #[test]
    fn pixel_value_u32_decodes_every_low_bit_pattern_without_panic() {
        // Metric nibbles 12..=15 have no SizeMetric — they must clamp to Px.
        for nibble in 12u32..16 {
            let encoded = (1u32 << 4) | nibble;
            let decoded = decode_pixel_value_u32(encoded).unwrap();
            assert_eq!(decoded.metric, SizeMetric::Px);
            assert_eq!(decoded.number.number(), 1);
        }
        // Sign extension: the top bit of the 28-bit field must arithmetic-shift.
        let neg = decode_pixel_value_u32(0x8000_0000).unwrap();
        assert_eq!(neg.metric, SizeMetric::Px);
        assert_eq!(neg.number.number(), -134_217_728);
    }

    #[test]
    fn pixel_value_u32_negative_0_001_in_vh_vmin_vmax_collides_with_sentinels() {
        // BUG (encode_pixel_value_u32 / decode_pixel_value_u32):
        //
        // raw == -1 (i.e. -0.001 of a unit) sign-extends to 0xFFFF_FFFF, and
        // `<< 4` leaves 0xFFFF_FFF0. ORing in a metric nibble >= 9 pushes the
        // word into the reserved sentinel band (>= 0xFFFF_FFF9):
        //
        //   -0.001vh   → 0xFFFF_FFF9 == U32_MAX_CONTENT
        //   -0.001vmin → 0xFFFF_FFFA == U32_MIN_CONTENT
        //   -0.001vmax → 0xFFFF_FFFB == U32_INITIAL
        //
        // So a legal (if tiny) negative viewport-relative length is written into
        // the cache as `max-content` / `min-content` / `initial`, and the decoder
        // reports None (unset) instead of the value. The encoder's range check
        // guards the 28-bit magnitude but not the sentinel band it lands in.
        //
        // Fix would be to reject any encoding that lands >= U32_SENTINEL_THRESHOLD
        // and escape to tier 3 instead.
        // FIXED (as this test's own comment prescribed): a raw -1 with a high metric
        // nibble packs into the reserved sentinel band, so the encoder now ESCAPES it to
        // U32_SENTINEL (tier 3) rather than emitting a value that decodes as a wrong,
        // aliased sentinel. decode() therefore returns None ("not in the fast cache —
        // look in tier 3"), which is the safe, non-aliasing outcome.
        for metric in [SizeMetric::Vh, SizeMetric::Vmin, SizeMetric::Vmax] {
            let pv = pv_raw(metric, -1);
            let encoded = encode_pixel_value_u32(&pv);
            assert_eq!(encoded, U32_SENTINEL, "{metric:?}: raw -1 must escape to tier 3");
            assert_eq!(
                decode_pixel_value_u32(encoded),
                None,
                "{metric:?}: an escaped value decodes as None, never an aliased sentinel",
            );
        }
    }

    // =========================================================================
    // Resolved px i16 (×10)
    // =========================================================================

    #[test]
    fn resolved_px_i16_nan_and_infinity_are_defined_and_do_not_panic() {
        // `f32 as i32` saturates: NaN → 0, +inf → i32::MAX, -inf → i32::MIN.
        assert_eq!(encode_resolved_px_i16(f32::NAN), 0);
        assert_eq!(encode_resolved_px_i16(-f32::NAN), 0);
        assert_eq!(encode_resolved_px_i16(f32::INFINITY), I16_SENTINEL);
        assert_eq!(encode_resolved_px_i16(f32::NEG_INFINITY), I16_SENTINEL);
        assert_eq!(encode_resolved_px_i16(f32::MAX), I16_SENTINEL);
        assert_eq!(encode_resolved_px_i16(f32::MIN), I16_SENTINEL);
        // Subnormals round to zero rather than blowing up.
        assert_eq!(encode_resolved_px_i16(f32::MIN_POSITIVE), 0);
        assert_eq!(encode_resolved_px_i16(-0.0), 0);
    }

    #[test]
    fn resolved_px_i16_saturates_exactly_at_the_documented_range() {
        // Doc: -3276.8 ..= +3276.3 px at 0.1px precision.
        assert_eq!(encode_resolved_px_i16(3276.3), 32763);
        assert_eq!(encode_resolved_px_i16(-3276.8), -32768);

        // One tick outside in either direction escapes to tier 3.
        assert_eq!(encode_resolved_px_i16(3276.4), I16_SENTINEL);
        assert_eq!(encode_resolved_px_i16(-3276.9), I16_SENTINEL);
        assert_eq!(encode_resolved_px_i16(1e9), I16_SENTINEL);
        assert_eq!(encode_resolved_px_i16(-1e9), I16_SENTINEL);
    }

    #[test]
    fn resolved_px_i16_sentinels_decode_to_none_and_are_distinct() {
        let sentinels = [I16_SENTINEL, I16_AUTO, I16_INHERIT, I16_INITIAL];
        for (i, a) in sentinels.iter().enumerate() {
            assert!(*a >= I16_SENTINEL_THRESHOLD);
            assert_eq!(decode_resolved_px_i16(*a), None);
            for b in &sentinels[i + 1..] {
                assert_ne!(a, b, "two i16 sentinels share a bit pattern");
            }
        }
        assert_eq!(decode_resolved_px_i16(I16_SENTINEL_THRESHOLD), None);
        assert_eq!(decode_resolved_px_i16(I16_SENTINEL_THRESHOLD - 1), Some(3276.3));
    }

    #[test]
    fn resolved_px_i16_every_non_sentinel_value_roundtrips() {
        // Exhaustive over the whole non-sentinel i16 domain: decode → encode must
        // be the identity, or a value written by one frame reads back shifted on
        // the next.
        for v in i16::MIN..I16_SENTINEL_THRESHOLD {
            let px = decode_resolved_px_i16(v)
                .unwrap_or_else(|| panic!("{v} is below the threshold but decoded as a sentinel"));
            assert_eq!(
                encode_resolved_px_i16(px),
                v,
                "i16 {v} decoded to {px} px which re-encodes to a different i16",
            );
        }
    }

    // =========================================================================
    // Flex u16 (×100)
    // =========================================================================

    #[test]
    fn flex_u16_nan_infinity_and_negatives_are_defined_and_do_not_panic() {
        assert_eq!(encode_flex_u16(f32::NAN), 0);
        assert_eq!(encode_flex_u16(f32::INFINITY), U16_SENTINEL);
        assert_eq!(encode_flex_u16(f32::NEG_INFINITY), U16_SENTINEL);
        assert_eq!(encode_flex_u16(f32::MAX), U16_SENTINEL);
        // flex-grow/shrink are non-negative; a negative escapes to tier 3.
        assert_eq!(encode_flex_u16(-1.0), U16_SENTINEL);
        assert_eq!(encode_flex_u16(-0.01), U16_SENTINEL);
        // …but -0.0 and values that round to zero clamp to 0, not to a sentinel.
        assert_eq!(encode_flex_u16(-0.0), 0);
        assert_eq!(encode_flex_u16(0.0), 0);
    }

    #[test]
    fn flex_u16_saturates_exactly_at_the_documented_range() {
        // Doc: 0.00 ..= 655.27 at 0.01 precision.
        assert_eq!(encode_flex_u16(655.27), 65527);
        assert_eq!(decode_flex_u16(65527), Some(655.27));
        // 65528 is representable but 65529 is the threshold.
        assert_eq!(encode_flex_u16(655.28), 65528);
        assert_eq!(encode_flex_u16(655.29), U16_SENTINEL);
        assert_eq!(encode_flex_u16(1e9), U16_SENTINEL);
    }

    #[test]
    fn flex_u16_sentinel_band_decodes_to_none() {
        for v in U16_SENTINEL_THRESHOLD..=u16::MAX {
            assert_eq!(decode_flex_u16(v), None, "u16 {v} is reserved and must decode as None");
        }
        assert_eq!(U16_SENTINEL, u16::MAX);
        assert!(decode_flex_u16(U16_SENTINEL_THRESHOLD - 1).is_some());
    }

    #[test]
    fn flex_u16_every_non_sentinel_value_roundtrips() {
        for v in 0..U16_SENTINEL_THRESHOLD {
            let f = decode_flex_u16(v).unwrap_or_else(|| panic!("{v} decoded as a sentinel"));
            assert_eq!(encode_flex_u16(f), v, "u16 {v} → {f} → re-encoded differently");
        }
    }

    // =========================================================================
    // encode_css_pixel_as_i16 — CssPropertyValue → i16 keyword sentinels
    // =========================================================================

    #[test]
    fn css_pixel_as_i16_maps_every_keyword_to_its_own_sentinel() {
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::Auto),
            I16_AUTO,
        );
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::Initial),
            I16_INITIAL,
        );
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::Inherit),
            I16_INHERIT,
        );
        // None / Revert / Unset have no dedicated slot → generic sentinel (slow path).
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::<PixelValue>::None),
            I16_SENTINEL,
        );
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::<PixelValue>::Revert),
            I16_SENTINEL,
        );
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::<PixelValue>::Unset),
            I16_SENTINEL,
        );
    }

    #[test]
    fn css_pixel_as_i16_only_takes_the_fast_path_for_absolute_px() {
        // Only SizeMetric::Px can be pre-resolved without layout context;
        // every relative unit must escape to the cascade.
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::Exact(PixelValue::px(12.5))),
            125,
        );
        for metric in ALL_SIZE_METRIC {
            if metric == SizeMetric::Px {
                continue;
            }
            assert_eq!(
                encode_css_pixel_as_i16(&CssPropertyValue::Exact(pv_raw(metric, 12_500))),
                I16_SENTINEL,
                "{metric:?} needs resolution context and must not be pre-resolved",
            );
        }
    }

    #[test]
    fn css_pixel_as_i16_out_of_range_px_escapes_to_the_sentinel() {
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::Exact(PixelValue::px(1e6))),
            I16_SENTINEL,
        );
        assert_eq!(
            encode_css_pixel_as_i16(&CssPropertyValue::Exact(PixelValue::px(-1e6))),
            I16_SENTINEL,
        );
        // A px value that happens to land on a keyword sentinel would be
        // misread as `auto`/`inherit`; the range check must exclude the band.
        for raw in [I16_SENTINEL, I16_AUTO, I16_INHERIT, I16_INITIAL] {
            let px = f32::from(raw) / 10.0;
            assert_eq!(
                encode_css_pixel_as_i16(&CssPropertyValue::Exact(PixelValue::px(px))),
                I16_SENTINEL,
                "{px} px must not silently encode as the keyword sentinel {raw}",
            );
        }
    }

    // =========================================================================
    // CompactLayoutCache — constructor invariants, bounds, predicates
    // =========================================================================

    #[test]
    fn empty_cache_is_the_neutral_element() {
        let c = CompactLayoutCache::empty();
        assert_eq!(c.node_count(), 0);
        assert!(c.tier1_enums.is_empty());
        assert!(c.tier2_dims.is_empty());
        assert!(c.tier2_cold.is_empty());
        assert!(c.tier2b_text.is_empty());
        assert!(c.font_dirty_nodes.is_empty());
        assert!(c.prev_font_hashes.is_empty());
        assert!(c.font_hash_to_families.is_empty());
        assert_eq!(c.dom_declared_flags, 0);
        // with_capacity(0) must produce exactly the same thing.
        assert_eq!(CompactLayoutCache::with_capacity(0), c);
    }

    #[test]
    fn with_capacity_keeps_every_tier_the_same_length() {
        for n in [0usize, 1, 2, 3, 17, 1024] {
            let c = CompactLayoutCache::with_capacity(n);
            assert_eq!(c.node_count(), n);
            assert_eq!(c.tier1_enums.len(), n);
            assert_eq!(c.tier2_dims.len(), n);
            assert_eq!(c.tier2_cold.len(), n);
            assert_eq!(c.tier2b_text.len(), n);
            assert_eq!(c.prev_font_hashes.len(), n);
            // Dirty list starts empty regardless of node count.
            assert!(c.font_dirty_nodes.is_empty());
            assert_eq!(c.dom_declared_flags, 0);
        }
    }

    #[test]
    fn freshly_allocated_node_reads_back_every_css_initial_value() {
        // `with_capacity` zeroes tier1 and default-fills tier2. A node the
        // builder never touched must still answer every getter with the CSS
        // initial value — this is what lets the builder skip unset properties.
        let c = CompactLayoutCache::with_capacity(3);
        for i in 0..3 {
            assert_eq!(T1::decode(c.tier1_enums[i]), T1::initial());
            assert_eq!(c.get_display(i), LayoutDisplay::Block);
            assert_eq!(c.get_position(i), LayoutPosition::Static);
            assert_eq!(c.get_float(i), LayoutFloat::None);
            assert_eq!(c.get_overflow_x(i), LayoutOverflow::Visible);
            assert_eq!(c.get_overflow_y(i), LayoutOverflow::Visible);
            assert_eq!(c.get_box_sizing(i), LayoutBoxSizing::ContentBox);
            assert_eq!(c.get_flex_direction(i), LayoutFlexDirection::Row);
            assert_eq!(c.get_flex_wrap(i), LayoutFlexWrap::NoWrap);
            assert_eq!(c.get_justify_content(i), LayoutJustifyContent::FlexStart);
            assert_eq!(c.get_align_items(i), LayoutAlignItems::Stretch);
            assert_eq!(c.get_align_content(i), LayoutAlignContent::Stretch);
            assert_eq!(c.get_writing_mode(i), LayoutWritingMode::HorizontalTb);
            assert_eq!(c.get_clear(i), LayoutClear::None);
            assert_eq!(c.get_font_weight(i), StyleFontWeight::Normal);
            assert_eq!(c.get_font_style(i), StyleFontStyle::Normal);
            assert_eq!(c.get_text_align(i), StyleTextAlign::Left);
            assert_eq!(c.get_visibility(i), StyleVisibility::Visible);
            assert_eq!(c.get_white_space(i), StyleWhiteSpace::Normal);
            assert_eq!(c.get_direction(i), StyleDirection::Ltr);
            assert_eq!(c.get_vertical_align(i), StyleVerticalAlign::Baseline);
            assert_eq!(c.get_border_collapse(i), StyleBorderCollapse::Separate);

            // Dimensions: auto / none, i.e. no decodable pixel value.
            assert_eq!(c.get_width_raw(i), U32_AUTO);
            assert_eq!(c.get_height_raw(i), U32_AUTO);
            assert_eq!(c.get_min_width_raw(i), U32_AUTO);
            assert_eq!(c.get_min_height_raw(i), U32_AUTO);
            assert_eq!(c.get_max_width_raw(i), U32_NONE);
            assert_eq!(c.get_max_height_raw(i), U32_NONE);
            assert_eq!(c.get_flex_basis_raw(i), U32_AUTO);
            assert_eq!(c.get_font_size_raw(i), U32_INITIAL);
            assert_eq!(decode_pixel_value_u32(c.get_width_raw(i)), None);
            assert_eq!(decode_pixel_value_u32(c.get_font_size_raw(i)), None);

            // Box model: zeros are real values (Some), offsets are auto (raw sentinel).
            assert_eq!(c.get_padding_top(i), Some(0.0));
            assert_eq!(c.get_padding_right(i), Some(0.0));
            assert_eq!(c.get_padding_bottom(i), Some(0.0));
            assert_eq!(c.get_padding_left(i), Some(0.0));
            assert_eq!(c.get_border_top_width(i), Some(0.0));
            assert_eq!(c.get_border_left_width(i), Some(0.0));
            // margin defaults to 0, NOT auto — centering must not kick in for free.
            assert_eq!(c.get_margin_top(i), Some(0.0));
            assert_eq!(c.get_margin_left(i), Some(0.0));
            assert!(!c.is_margin_top_auto(i));
            assert!(!c.is_margin_right_auto(i));
            assert!(!c.is_margin_bottom_auto(i));
            assert!(!c.is_margin_left_auto(i));
            // …but the inset properties DO default to auto.
            assert_eq!(c.get_top(i), I16_AUTO);
            assert_eq!(c.get_right(i), I16_AUTO);
            assert_eq!(c.get_bottom(i), I16_AUTO);
            assert_eq!(c.get_left(i), I16_AUTO);

            // Flex: grow 0, shrink 1 (the CSS defaults).
            assert_eq!(c.get_flex_grow(i), Some(0.0));
            assert_eq!(c.get_flex_shrink(i), Some(1.0));

            // Cold tier.
            assert_eq!(c.get_z_index(i), I16_AUTO);
            assert_eq!(c.get_border_styles_packed(i), 0);
            assert_eq!(c.get_border_top_style(i), BorderStyle::None);
            assert_eq!(c.get_border_right_style(i), BorderStyle::None);
            assert_eq!(c.get_border_bottom_style(i), BorderStyle::None);
            assert_eq!(c.get_border_left_style(i), BorderStyle::None);
            assert_eq!(c.get_border_top_color_raw(i), 0);
            assert_eq!(decode_color_u32(c.get_border_top_color_raw(i)), None);
            assert_eq!(c.get_border_top_left_radius_raw(i), I16_SENTINEL);
            assert_eq!(c.get_tab_size_raw(i), I16_SENTINEL);
            assert_eq!(c.get_border_spacing_h_raw(i), 0);
            assert_eq!(c.get_border_spacing_v_raw(i), 0);
            assert_eq!(c.get_opacity_raw(i), OPACITY_SENTINEL);
            assert_eq!(c.get_hot_flags(i), 0);
            assert_eq!(c.get_scrollbar_gutter_bits(i), SCROLLBAR_GUTTER_AUTO);

            // Every "has this rare prop" predicate must be false on a fresh node,
            // otherwise the fast path would take a cascade walk for every node.
            assert!(!c.has_transform(i));
            assert!(!c.has_transform_origin(i));
            assert!(!c.has_box_shadow(i));
            assert!(!c.has_text_decoration(i));
            assert!(!c.has_background(i));
            assert!(!c.has_clip_path(i));
            assert!(!c.has_scrollbar_css(i));
            assert!(!c.has_counter(i));
            assert!(!c.has_break(i));
            assert!(!c.has_text_orientation(i));
            assert!(!c.has_text_shadow(i));
            assert!(!c.has_backdrop_filter(i));
            assert!(!c.has_filter(i));
            assert!(!c.has_mix_blend_mode(i));

            // Text tier.
            assert_eq!(c.get_text_color_raw(i), 0);
            assert_eq!(c.get_font_family_hash(i), 0);
            assert_eq!(c.get_line_height(i), None); // "normal" → slow path
            assert_eq!(c.get_letter_spacing(i), Some(0.0));
            assert_eq!(c.get_word_spacing(i), Some(0.0));
            assert_eq!(c.get_text_indent(i), Some(0.0));
        }
    }

    #[test]
    fn hot_flag_predicates_read_only_their_own_bit() {
        let flags = [
            ("transform", HOT_FLAG_HAS_TRANSFORM),
            ("transform_origin", HOT_FLAG_HAS_TRANSFORM_ORIGIN),
            ("box_shadow", HOT_FLAG_HAS_BOX_SHADOW),
            ("text_decoration", HOT_FLAG_HAS_TEXT_DECORATION),
            ("background", HOT_FLAG_HAS_BACKGROUND),
            ("clip_path", HOT_FLAG_HAS_CLIP_PATH),
        ];

        for (name, bit) in flags {
            let mut c = CompactLayoutCache::with_capacity(1);
            c.tier2_cold[0].hot_flags = bit;

            let observed = [
                ("transform", c.has_transform(0)),
                ("transform_origin", c.has_transform_origin(0)),
                ("box_shadow", c.has_box_shadow(0)),
                ("text_decoration", c.has_text_decoration(0)),
                ("background", c.has_background(0)),
                ("clip_path", c.has_clip_path(0)),
            ];
            for (other, is_set) in observed {
                assert_eq!(
                    is_set,
                    other == name,
                    "hot_flags = {bit:#010b}: has_{other}() should be {}",
                    other == name,
                );
            }
            // The gutter field lives in bits 4-5 and must be unaffected.
            assert_eq!(c.get_scrollbar_gutter_bits(0), SCROLLBAR_GUTTER_AUTO);
        }

        // No two hot flags may share a bit, and none may overlap the gutter field.
        let mut seen = 0u8;
        for (_, bit) in flags {
            assert_eq!(seen & bit, 0, "two hot flags share bit {bit:#010b}");
            assert_eq!(
                bit & HOT_FLAG_SCROLLBAR_GUTTER_MASK,
                0,
                "hot flag {bit:#010b} overlaps the scrollbar-gutter field",
            );
            seen |= bit;
        }
    }

    #[test]
    fn scrollbar_gutter_bits_survive_a_fully_set_hot_flags_byte() {
        for gutter in [
            SCROLLBAR_GUTTER_AUTO,
            SCROLLBAR_GUTTER_STABLE,
            SCROLLBAR_GUTTER_BOTH_EDGES,
            SCROLLBAR_GUTTER_MIRROR,
        ] {
            let mut c = CompactLayoutCache::with_capacity(1);
            // Every boolean flag set *and* a gutter value: the gutter must still
            // read back cleanly out of the middle of the byte.
            c.tier2_cold[0].hot_flags = HOT_FLAG_HAS_TRANSFORM
                | HOT_FLAG_HAS_TRANSFORM_ORIGIN
                | HOT_FLAG_HAS_BOX_SHADOW
                | HOT_FLAG_HAS_TEXT_DECORATION
                | HOT_FLAG_HAS_BACKGROUND
                | HOT_FLAG_HAS_CLIP_PATH
                | (gutter << HOT_FLAG_SCROLLBAR_GUTTER_SHIFT);

            assert_eq!(c.get_scrollbar_gutter_bits(0), gutter);
            assert!(c.has_transform(0));
            assert!(c.has_clip_path(0));
        }

        // An all-ones byte reads the max gutter value, never something out of range.
        let mut c = CompactLayoutCache::with_capacity(1);
        c.tier2_cold[0].hot_flags = u8::MAX;
        assert_eq!(c.get_scrollbar_gutter_bits(0), SCROLLBAR_GUTTER_MIRROR);
        assert!(c.get_scrollbar_gutter_bits(0) <= 3);
    }

    #[test]
    fn extra_flag_predicates_read_only_their_own_bit() {
        let flags = [
            ("scrollbar_css", EXTRA_FLAG_HAS_SCROLLBAR_CSS),
            ("counter", EXTRA_FLAG_HAS_COUNTER),
            ("break", EXTRA_FLAG_HAS_BREAK),
            ("text_orientation", EXTRA_FLAG_HAS_TEXT_ORIENTATION),
            ("text_shadow", EXTRA_FLAG_HAS_TEXT_SHADOW),
            ("backdrop_filter", EXTRA_FLAG_HAS_BACKDROP_FILTER),
            ("filter", EXTRA_FLAG_HAS_FILTER),
            ("mix_blend_mode", EXTRA_FLAG_HAS_MIX_BLEND_MODE),
        ];

        // All 8 bits must be distinct and together cover the whole byte.
        let mut seen = 0u8;
        for (_, bit) in flags {
            assert_eq!(seen & bit, 0, "two extra flags share bit {bit:#010b}");
            seen |= bit;
        }
        assert_eq!(seen, u8::MAX);

        for (name, bit) in flags {
            let mut c = CompactLayoutCache::with_capacity(1);
            c.tier2_cold[0].extra_flags = bit;
            let observed = [
                ("scrollbar_css", c.has_scrollbar_css(0)),
                ("counter", c.has_counter(0)),
                ("break", c.has_break(0)),
                ("text_orientation", c.has_text_orientation(0)),
                ("text_shadow", c.has_text_shadow(0)),
                ("backdrop_filter", c.has_backdrop_filter(0)),
                ("filter", c.has_filter(0)),
                ("mix_blend_mode", c.has_mix_blend_mode(0)),
            ];
            for (other, is_set) in observed {
                assert_eq!(
                    is_set,
                    other == name,
                    "extra_flags = {bit:#010b}: has_{other}() should be {}",
                    other == name,
                );
            }
            // Setting an extra flag must not make any hot-flag predicate fire.
            assert!(!c.has_transform(0));
            assert!(!c.has_background(0));
        }
    }

    #[test]
    fn dom_declared_flags_are_distinct_and_queryable() {
        let flags = [
            DOM_HAS_SHAPE_INSIDE,
            DOM_HAS_SHAPE_OUTSIDE,
            DOM_HAS_TEXT_JUSTIFY,
            DOM_HAS_TEXT_INDENT,
            DOM_HAS_COLUMN_COUNT,
            DOM_HAS_COLUMN_GAP,
            DOM_HAS_INITIAL_LETTER,
            DOM_HAS_INITIAL_LETTER_ALIGN,
            DOM_HAS_LINE_CLAMP,
            DOM_HAS_HANGING_PUNCTUATION,
            DOM_HAS_TEXT_COMBINE_UPRIGHT,
            DOM_HAS_EXCLUSION_MARGIN,
            DOM_HAS_HYPHENATION_LANGUAGE,
            DOM_HAS_UNICODE_BIDI,
            DOM_HAS_TEXT_BOX_TRIM,
            DOM_HAS_HYPHENS,
            DOM_HAS_WORD_BREAK,
            DOM_HAS_OVERFLOW_WRAP,
            DOM_HAS_LINE_BREAK,
            DOM_HAS_TEXT_ALIGN_LAST,
            DOM_HAS_LINE_HEIGHT,
            DOM_HAS_COLUMN_WIDTH,
            DOM_HAS_SHAPE_MARGIN,
        ];

        let mut seen = 0u32;
        for f in flags {
            assert_eq!(f.count_ones(), 1, "{f:#X} is not a single-bit flag");
            assert_eq!(seen & f, 0, "two DOM_HAS_* flags share bit {f:#X}");
            seen |= f;
        }

        let mut c = CompactLayoutCache::empty();
        // Nothing declared: every query is false, including the degenerate ones.
        for f in flags {
            assert!(!c.dom_declared(f));
        }
        assert!(!c.dom_declared(0));
        assert!(!c.dom_declared(u32::MAX));

        // One flag declared: only that query is true.
        c.dom_declared_flags = DOM_HAS_LINE_HEIGHT;
        for f in flags {
            assert_eq!(c.dom_declared(f), f == DOM_HAS_LINE_HEIGHT);
        }
        assert!(!c.dom_declared(0), "an empty flag query must never report declared");
        assert!(c.dom_declared(u32::MAX));

        // Everything declared: every query is true.
        c.dom_declared_flags = u32::MAX;
        for f in flags {
            assert!(c.dom_declared(f));
        }
    }

    #[test]
    fn getters_at_the_last_valid_index_do_not_panic() {
        let c = CompactLayoutCache::with_capacity(4);
        let last = c.node_count() - 1;
        let _ = c.get_display(last);
        let _ = c.get_width_raw(last);
        let _ = c.get_padding_top(last);
        let _ = c.get_z_index(last);
        let _ = c.get_border_top_style(last);
        let _ = c.get_line_height(last);
        let _ = c.has_transform(last);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn tier1_getter_on_an_empty_cache_panics_rather_than_reading_oob() {
        let c = CompactLayoutCache::empty();
        let _ = c.get_display(0);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn tier2_getter_past_the_end_panics_rather_than_reading_oob() {
        let c = CompactLayoutCache::with_capacity(2);
        let _ = c.get_padding_top(2);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn cold_tier_getter_at_usize_max_panics_rather_than_wrapping() {
        // usize::MAX would wrap to a valid offset if the index were ever used in
        // pointer arithmetic without a bounds check.
        let c = CompactLayoutCache::with_capacity(1);
        let _ = c.get_z_index(usize::MAX);
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn text_tier_getter_past_the_end_panics_rather_than_reading_oob() {
        let c = CompactLayoutCache::with_capacity(1);
        let _ = c.get_font_family_hash(1);
    }

    // =========================================================================
    // Struct layout — the compact cache's whole point is its byte budget
    // =========================================================================

    #[test]
    fn compact_structs_stay_within_their_documented_byte_budget() {
        // These sizes are load-bearing: the cache is sized as N × these, and the
        // module header quotes them. A field added without updating the header
        // silently doubles the per-node memory cost.
        assert_eq!(size_of::<CompactNodeProps>(), 72);
        assert_eq!(size_of::<CompactNodePropsCold>(), 48);
        assert_eq!(size_of::<CompactTextProps>(), 24);
        // Tier 1 is exactly one u64 per node.
        assert_eq!(size_of::<u64>(), 8);
        // No padding surprises from #[repr(C)] reordering.
        assert_eq!(align_of::<CompactNodeProps>(), 4);
        assert_eq!(align_of::<CompactNodePropsCold>(), 4);
        assert_eq!(align_of::<CompactTextProps>(), 8);
    }
}

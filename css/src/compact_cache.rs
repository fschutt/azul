//! Compact layout property cache — three-tier numeric encoding
//!
//! Replaces BTreeMap-based CSS property lookups with cache-friendly arrays.
//! See `scripts/COMPACT_CACHE_PLAN.md` for design rationale.
//!
//! - **Tier 1**: `Vec<u64>` — ALL 21 enum properties bitpacked (8 B/node)
//! - **Tier 2**: `Vec<CompactNodeProps>` — numeric dimensions + border colors/styles (96 B/node)
//! - **Tier 2b**: `Vec<CompactTextProps>` — text/IFC properties (24 B/node)
//!
//! Non-compact properties (background, box-shadow, transform, etc.) are
//! resolved via the slow cascade path in `CssPropertyCache::get_property_slow()`.

use crate::props::basic::length::{FloatValue, SizeMetric};
use crate::props::basic::pixel::PixelValue;
use crate::props::layout::{
    display::LayoutDisplay,
    dimensions::{LayoutHeight, LayoutWidth, LayoutMaxHeight, LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth},
    flex::{
        LayoutAlignContent, LayoutAlignItems, LayoutFlexDirection, LayoutFlexWrap,
        LayoutJustifyContent,
    },
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
pub const U16_AUTO: u16 = 0xFFFE;
pub const U16_NONE: u16 = 0xFFFD;
pub const U16_INHERIT: u16 = 0xFFFC;
pub const U16_INITIAL: u16 = 0xFFFB;
pub const U16_MIN_CONTENT: u16 = 0xFFFA;
pub const U16_MAX_CONTENT: u16 = 0xFFF9;
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
pub const U32_SENTINEL: u32 = 0xFFFFFFFF;
pub const U32_AUTO: u32 = 0xFFFFFFFE;
pub const U32_NONE: u32 = 0xFFFFFFFD;
pub const U32_INHERIT: u32 = 0xFFFFFFFC;
pub const U32_INITIAL: u32 = 0xFFFFFFFB;
pub const U32_MIN_CONTENT: u32 = 0xFFFFFFFA;
pub const U32_MAX_CONTENT: u32 = 0xFFFFFFF9;
/// Any u32 value >= this threshold is a sentinel
pub const U32_SENTINEL_THRESHOLD: u32 = 0xFFFFFFF9;

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
const DISPLAY_SHIFT: u32 = 0;
const POSITION_SHIFT: u32 = 5;
const FLOAT_SHIFT: u32 = 8;
const OVERFLOW_X_SHIFT: u32 = 10;
const OVERFLOW_Y_SHIFT: u32 = 13;
const BOX_SIZING_SHIFT: u32 = 16;
const FLEX_DIRECTION_SHIFT: u32 = 17;
const FLEX_WRAP_SHIFT: u32 = 19;
const JUSTIFY_CONTENT_SHIFT: u32 = 21;
const ALIGN_ITEMS_SHIFT: u32 = 24;
const ALIGN_CONTENT_SHIFT: u32 = 27;
const WRITING_MODE_SHIFT: u32 = 30;
const CLEAR_SHIFT: u32 = 32;
const FONT_WEIGHT_SHIFT: u32 = 34;
const FONT_STYLE_SHIFT: u32 = 38;
const TEXT_ALIGN_SHIFT: u32 = 40;
const VISIBILITY_SHIFT: u32 = 43;
const WHITE_SPACE_SHIFT: u32 = 45;
const DIRECTION_SHIFT: u32 = 48;
const VERTICAL_ALIGN_SHIFT: u32 = 49;
const BORDER_COLLAPSE_SHIFT: u32 = 52;

// Bit masks
const DISPLAY_MASK: u64 = 0x1F;     // 5 bits
const POSITION_MASK: u64 = 0x07;    // 3 bits
const FLOAT_MASK: u64 = 0x03;       // 2 bits
const OVERFLOW_MASK: u64 = 0x07;    // 3 bits
const BOX_SIZING_MASK: u64 = 0x01;  // 1 bit
const FLEX_DIR_MASK: u64 = 0x03;    // 2 bits
const FLEX_WRAP_MASK: u64 = 0x03;   // 2 bits
const JUSTIFY_MASK: u64 = 0x07;     // 3 bits
const ALIGN_MASK: u64 = 0x07;       // 3 bits
const WRITING_MODE_MASK: u64 = 0x03;// 2 bits
const CLEAR_MASK: u64 = 0x03;       // 2 bits
const FONT_WEIGHT_MASK: u64 = 0x0F; // 4 bits
const FONT_STYLE_MASK: u64 = 0x03;  // 2 bits
const TEXT_ALIGN_MASK: u64 = 0x07;  // 3 bits
const VISIBILITY_MASK: u64 = 0x03;  // 2 bits
const WHITE_SPACE_MASK: u64 = 0x07; // 3 bits
const DIRECTION_MASK: u64 = 0x01;   // 1 bit
const VERTICAL_ALIGN_MASK: u64 = 0x07; // 3 bits
const BORDER_COLLAPSE_MASK: u64 = 0x01; // 1 bit

/// Special value stored in the spare bits [63:51] to indicate this node has
/// NO tier-1 data (i.e., all defaults). 0 is a valid all-defaults encoding,
/// so we use bit 63 as a "tier1 populated" flag. If bit 63 is 0 and all other
/// bits are 0, it means "all defaults" (Display::Block, Position::Static, etc.).
/// We set bit 63 = 1 to mark that the node HAS been populated.
const TIER1_POPULATED_BIT: u64 = 1 << 63;

// =============================================================================
// Safe from_u8 conversion functions (no transmute!)
// =============================================================================

/// Convert raw bits back to LayoutDisplay. Returns default on invalid input.
#[inline(always)]
pub fn layout_display_from_u8(v: u8) -> LayoutDisplay {
    match v {
        0 => LayoutDisplay::None,
        1 => LayoutDisplay::Block,
        2 => LayoutDisplay::Inline,
        3 => LayoutDisplay::InlineBlock,
        4 => LayoutDisplay::Flex,
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
        _ => LayoutDisplay::Block, // safe fallback
    }
}

/// Convert LayoutDisplay to its discriminant value (matching #[repr(C)] order).
#[inline(always)]
pub fn layout_display_to_u8(v: LayoutDisplay) -> u8 {
    match v {
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
    }
}

#[inline(always)]
pub fn layout_position_from_u8(v: u8) -> LayoutPosition {
    match v {
        0 => LayoutPosition::Static,
        1 => LayoutPosition::Relative,
        2 => LayoutPosition::Absolute,
        3 => LayoutPosition::Fixed,
        4 => LayoutPosition::Sticky,
        _ => LayoutPosition::Static,
    }
}

#[inline(always)]
pub fn layout_position_to_u8(v: LayoutPosition) -> u8 {
    match v {
        LayoutPosition::Static => 0,
        LayoutPosition::Relative => 1,
        LayoutPosition::Absolute => 2,
        LayoutPosition::Fixed => 3,
        LayoutPosition::Sticky => 4,
    }
}

#[inline(always)]
pub fn layout_float_from_u8(v: u8) -> LayoutFloat {
    match v {
        0 => LayoutFloat::Left,
        1 => LayoutFloat::Right,
        2 => LayoutFloat::None,
        _ => LayoutFloat::None,
    }
}

#[inline(always)]
pub fn layout_float_to_u8(v: LayoutFloat) -> u8 {
    match v {
        LayoutFloat::Left => 0,
        LayoutFloat::Right => 1,
        LayoutFloat::None => 2,
    }
}

#[inline(always)]
pub fn layout_overflow_from_u8(v: u8) -> LayoutOverflow {
    match v {
        0 => LayoutOverflow::Scroll,
        1 => LayoutOverflow::Auto,
        2 => LayoutOverflow::Hidden,
        3 => LayoutOverflow::Visible,
        4 => LayoutOverflow::Clip,
        _ => LayoutOverflow::Visible,
    }
}

#[inline(always)]
pub fn layout_overflow_to_u8(v: LayoutOverflow) -> u8 {
    match v {
        LayoutOverflow::Scroll => 0,
        LayoutOverflow::Auto => 1,
        LayoutOverflow::Hidden => 2,
        LayoutOverflow::Visible => 3,
        LayoutOverflow::Clip => 4,
    }
}

#[inline(always)]
pub fn layout_box_sizing_from_u8(v: u8) -> LayoutBoxSizing {
    match v {
        0 => LayoutBoxSizing::ContentBox,
        1 => LayoutBoxSizing::BorderBox,
        _ => LayoutBoxSizing::ContentBox,
    }
}

#[inline(always)]
pub fn layout_box_sizing_to_u8(v: LayoutBoxSizing) -> u8 {
    match v {
        LayoutBoxSizing::ContentBox => 0,
        LayoutBoxSizing::BorderBox => 1,
    }
}

#[inline(always)]
pub fn layout_flex_direction_from_u8(v: u8) -> LayoutFlexDirection {
    match v {
        0 => LayoutFlexDirection::Row,
        1 => LayoutFlexDirection::RowReverse,
        2 => LayoutFlexDirection::Column,
        3 => LayoutFlexDirection::ColumnReverse,
        _ => LayoutFlexDirection::Row,
    }
}

#[inline(always)]
pub fn layout_flex_direction_to_u8(v: LayoutFlexDirection) -> u8 {
    match v {
        LayoutFlexDirection::Row => 0,
        LayoutFlexDirection::RowReverse => 1,
        LayoutFlexDirection::Column => 2,
        LayoutFlexDirection::ColumnReverse => 3,
    }
}

#[inline(always)]
pub fn layout_flex_wrap_from_u8(v: u8) -> LayoutFlexWrap {
    match v {
        0 => LayoutFlexWrap::Wrap,
        1 => LayoutFlexWrap::NoWrap,
        2 => LayoutFlexWrap::WrapReverse,
        _ => LayoutFlexWrap::NoWrap,
    }
}

#[inline(always)]
pub fn layout_flex_wrap_to_u8(v: LayoutFlexWrap) -> u8 {
    match v {
        LayoutFlexWrap::Wrap => 0,
        LayoutFlexWrap::NoWrap => 1,
        LayoutFlexWrap::WrapReverse => 2,
    }
}

#[inline(always)]
pub fn layout_justify_content_from_u8(v: u8) -> LayoutJustifyContent {
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

#[inline(always)]
pub fn layout_justify_content_to_u8(v: LayoutJustifyContent) -> u8 {
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

#[inline(always)]
pub fn layout_align_items_from_u8(v: u8) -> LayoutAlignItems {
    match v {
        0 => LayoutAlignItems::Stretch,
        1 => LayoutAlignItems::Center,
        2 => LayoutAlignItems::Start,
        3 => LayoutAlignItems::End,
        4 => LayoutAlignItems::Baseline,
        _ => LayoutAlignItems::Stretch,
    }
}

#[inline(always)]
pub fn layout_align_items_to_u8(v: LayoutAlignItems) -> u8 {
    match v {
        LayoutAlignItems::Stretch => 0,
        LayoutAlignItems::Center => 1,
        LayoutAlignItems::Start => 2,
        LayoutAlignItems::End => 3,
        LayoutAlignItems::Baseline => 4,
    }
}

#[inline(always)]
pub fn layout_align_content_from_u8(v: u8) -> LayoutAlignContent {
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

#[inline(always)]
pub fn layout_align_content_to_u8(v: LayoutAlignContent) -> u8 {
    match v {
        LayoutAlignContent::Stretch => 0,
        LayoutAlignContent::Center => 1,
        LayoutAlignContent::Start => 2,
        LayoutAlignContent::End => 3,
        LayoutAlignContent::SpaceBetween => 4,
        LayoutAlignContent::SpaceAround => 5,
    }
}

#[inline(always)]
pub fn layout_writing_mode_from_u8(v: u8) -> LayoutWritingMode {
    match v {
        0 => LayoutWritingMode::HorizontalTb,
        1 => LayoutWritingMode::VerticalRl,
        2 => LayoutWritingMode::VerticalLr,
        _ => LayoutWritingMode::HorizontalTb,
    }
}

#[inline(always)]
pub fn layout_writing_mode_to_u8(v: LayoutWritingMode) -> u8 {
    match v {
        LayoutWritingMode::HorizontalTb => 0,
        LayoutWritingMode::VerticalRl => 1,
        LayoutWritingMode::VerticalLr => 2,
    }
}

#[inline(always)]
pub fn layout_clear_from_u8(v: u8) -> LayoutClear {
    match v {
        0 => LayoutClear::None,
        1 => LayoutClear::Left,
        2 => LayoutClear::Right,
        3 => LayoutClear::Both,
        _ => LayoutClear::None,
    }
}

#[inline(always)]
pub fn layout_clear_to_u8(v: LayoutClear) -> u8 {
    match v {
        LayoutClear::None => 0,
        LayoutClear::Left => 1,
        LayoutClear::Right => 2,
        LayoutClear::Both => 3,
    }
}

#[inline(always)]
pub fn style_font_weight_from_u8(v: u8) -> StyleFontWeight {
    match v {
        0 => StyleFontWeight::Lighter,
        1 => StyleFontWeight::W100,
        2 => StyleFontWeight::W200,
        3 => StyleFontWeight::W300,
        4 => StyleFontWeight::Normal,
        5 => StyleFontWeight::W500,
        6 => StyleFontWeight::W600,
        7 => StyleFontWeight::Bold,
        8 => StyleFontWeight::W800,
        9 => StyleFontWeight::W900,
        10 => StyleFontWeight::Bolder,
        _ => StyleFontWeight::Normal,
    }
}

#[inline(always)]
pub fn style_font_weight_to_u8(v: StyleFontWeight) -> u8 {
    match v {
        StyleFontWeight::Lighter => 0,
        StyleFontWeight::W100 => 1,
        StyleFontWeight::W200 => 2,
        StyleFontWeight::W300 => 3,
        StyleFontWeight::Normal => 4,
        StyleFontWeight::W500 => 5,
        StyleFontWeight::W600 => 6,
        StyleFontWeight::Bold => 7,
        StyleFontWeight::W800 => 8,
        StyleFontWeight::W900 => 9,
        StyleFontWeight::Bolder => 10,
    }
}

#[inline(always)]
pub fn style_font_style_from_u8(v: u8) -> StyleFontStyle {
    match v {
        0 => StyleFontStyle::Normal,
        1 => StyleFontStyle::Italic,
        2 => StyleFontStyle::Oblique,
        _ => StyleFontStyle::Normal,
    }
}

#[inline(always)]
pub fn style_font_style_to_u8(v: StyleFontStyle) -> u8 {
    match v {
        StyleFontStyle::Normal => 0,
        StyleFontStyle::Italic => 1,
        StyleFontStyle::Oblique => 2,
    }
}

#[inline(always)]
pub fn style_text_align_from_u8(v: u8) -> StyleTextAlign {
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

#[inline(always)]
pub fn style_text_align_to_u8(v: StyleTextAlign) -> u8 {
    match v {
        StyleTextAlign::Left => 0,
        StyleTextAlign::Center => 1,
        StyleTextAlign::Right => 2,
        StyleTextAlign::Justify => 3,
        StyleTextAlign::Start => 4,
        StyleTextAlign::End => 5,
    }
}

#[inline(always)]
pub fn style_visibility_from_u8(v: u8) -> StyleVisibility {
    match v {
        0 => StyleVisibility::Visible,
        1 => StyleVisibility::Hidden,
        2 => StyleVisibility::Collapse,
        _ => StyleVisibility::Visible,
    }
}

#[inline(always)]
pub fn style_visibility_to_u8(v: StyleVisibility) -> u8 {
    match v {
        StyleVisibility::Visible => 0,
        StyleVisibility::Hidden => 1,
        StyleVisibility::Collapse => 2,
    }
}

#[inline(always)]
pub fn style_white_space_from_u8(v: u8) -> StyleWhiteSpace {
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

#[inline(always)]
pub fn style_white_space_to_u8(v: StyleWhiteSpace) -> u8 {
    match v {
        StyleWhiteSpace::Normal => 0,
        StyleWhiteSpace::Pre => 1,
        StyleWhiteSpace::Nowrap => 2,
        StyleWhiteSpace::PreWrap => 3,
        StyleWhiteSpace::PreLine => 4,
        StyleWhiteSpace::BreakSpaces => 5,
    }
}

#[inline(always)]
pub fn style_direction_from_u8(v: u8) -> StyleDirection {
    match v {
        0 => StyleDirection::Ltr,
        1 => StyleDirection::Rtl,
        _ => StyleDirection::Ltr,
    }
}

#[inline(always)]
pub fn style_direction_to_u8(v: StyleDirection) -> u8 {
    match v {
        StyleDirection::Ltr => 0,
        StyleDirection::Rtl => 1,
    }
}

#[inline(always)]
pub fn style_vertical_align_from_u8(v: u8) -> StyleVerticalAlign {
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

#[inline(always)]
pub fn style_vertical_align_to_u8(v: StyleVerticalAlign) -> u8 {
    match v {
        StyleVerticalAlign::Baseline => 0,
        StyleVerticalAlign::Top => 1,
        StyleVerticalAlign::Middle => 2,
        StyleVerticalAlign::Bottom => 3,
        StyleVerticalAlign::Sub => 4,
        StyleVerticalAlign::Superscript => 5,
        StyleVerticalAlign::TextTop => 6,
        StyleVerticalAlign::TextBottom => 7,
    }
}

#[inline(always)]
pub fn border_collapse_from_u8(v: u8) -> StyleBorderCollapse {
    match v {
        0 => StyleBorderCollapse::Separate,
        1 => StyleBorderCollapse::Collapse,
        _ => StyleBorderCollapse::Separate,
    }
}

#[inline(always)]
pub fn border_collapse_to_u8(v: StyleBorderCollapse) -> u8 {
    match v {
        StyleBorderCollapse::Separate => 0,
        StyleBorderCollapse::Collapse => 1,
    }
}

#[inline(always)]
pub fn border_style_from_u8(v: u8) -> BorderStyle {
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

#[inline(always)]
pub fn border_style_to_u8(v: BorderStyle) -> u8 {
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
pub fn encode_border_styles_packed(top: BorderStyle, right: BorderStyle, bottom: BorderStyle, left: BorderStyle) -> u16 {
    (border_style_to_u8(top) as u16)
        | ((border_style_to_u8(right) as u16) << 4)
        | ((border_style_to_u8(bottom) as u16) << 8)
        | ((border_style_to_u8(left) as u16) << 12)
}

/// Decode border-top-style from packed u16
#[inline(always)]
pub fn decode_border_top_style(packed: u16) -> BorderStyle {
    border_style_from_u8((packed & 0x0F) as u8)
}

/// Decode border-right-style from packed u16
#[inline(always)]
pub fn decode_border_right_style(packed: u16) -> BorderStyle {
    border_style_from_u8(((packed >> 4) & 0x0F) as u8)
}

/// Decode border-bottom-style from packed u16
#[inline(always)]
pub fn decode_border_bottom_style(packed: u16) -> BorderStyle {
    border_style_from_u8(((packed >> 8) & 0x0F) as u8)
}

/// Decode border-left-style from packed u16
#[inline(always)]
pub fn decode_border_left_style(packed: u16) -> BorderStyle {
    border_style_from_u8(((packed >> 12) & 0x0F) as u8)
}

/// Encode a ColorU as u32 (0xRRGGBBAA). Returns 0 for sentinel/unset.
#[inline(always)]
pub fn encode_color_u32(c: &ColorU) -> u32 {
    ((c.r as u32) << 24) | ((c.g as u32) << 16) | ((c.b as u32) << 8) | (c.a as u32)
}

/// Decode a u32 back to ColorU. Returns None if sentinel (0x00000000).
#[inline(always)]
pub fn decode_color_u32(v: u32) -> Option<ColorU> {
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
pub fn encode_tier1(
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

/// Decode individual enum properties from a Tier 1 u64.
/// Each function is `#[inline(always)]` for zero-cost extraction.

#[inline(always)]
pub fn decode_display(t1: u64) -> LayoutDisplay {
    layout_display_from_u8(((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8)
}

#[inline(always)]
pub fn decode_position(t1: u64) -> LayoutPosition {
    layout_position_from_u8(((t1 >> POSITION_SHIFT) & POSITION_MASK) as u8)
}

#[inline(always)]
pub fn decode_float(t1: u64) -> LayoutFloat {
    layout_float_from_u8(((t1 >> FLOAT_SHIFT) & FLOAT_MASK) as u8)
}

#[inline(always)]
pub fn decode_overflow_x(t1: u64) -> LayoutOverflow {
    layout_overflow_from_u8(((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8)
}

#[inline(always)]
pub fn decode_overflow_y(t1: u64) -> LayoutOverflow {
    layout_overflow_from_u8(((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8)
}

#[inline(always)]
pub fn decode_box_sizing(t1: u64) -> LayoutBoxSizing {
    layout_box_sizing_from_u8(((t1 >> BOX_SIZING_SHIFT) & BOX_SIZING_MASK) as u8)
}

#[inline(always)]
pub fn decode_flex_direction(t1: u64) -> LayoutFlexDirection {
    layout_flex_direction_from_u8(((t1 >> FLEX_DIRECTION_SHIFT) & FLEX_DIR_MASK) as u8)
}

#[inline(always)]
pub fn decode_flex_wrap(t1: u64) -> LayoutFlexWrap {
    layout_flex_wrap_from_u8(((t1 >> FLEX_WRAP_SHIFT) & FLEX_WRAP_MASK) as u8)
}

#[inline(always)]
pub fn decode_justify_content(t1: u64) -> LayoutJustifyContent {
    layout_justify_content_from_u8(((t1 >> JUSTIFY_CONTENT_SHIFT) & JUSTIFY_MASK) as u8)
}

#[inline(always)]
pub fn decode_align_items(t1: u64) -> LayoutAlignItems {
    layout_align_items_from_u8(((t1 >> ALIGN_ITEMS_SHIFT) & ALIGN_MASK) as u8)
}

#[inline(always)]
pub fn decode_align_content(t1: u64) -> LayoutAlignContent {
    layout_align_content_from_u8(((t1 >> ALIGN_CONTENT_SHIFT) & ALIGN_MASK) as u8)
}

#[inline(always)]
pub fn decode_writing_mode(t1: u64) -> LayoutWritingMode {
    layout_writing_mode_from_u8(((t1 >> WRITING_MODE_SHIFT) & WRITING_MODE_MASK) as u8)
}

#[inline(always)]
pub fn decode_clear(t1: u64) -> LayoutClear {
    layout_clear_from_u8(((t1 >> CLEAR_SHIFT) & CLEAR_MASK) as u8)
}

#[inline(always)]
pub fn decode_font_weight(t1: u64) -> StyleFontWeight {
    style_font_weight_from_u8(((t1 >> FONT_WEIGHT_SHIFT) & FONT_WEIGHT_MASK) as u8)
}

#[inline(always)]
pub fn decode_font_style(t1: u64) -> StyleFontStyle {
    style_font_style_from_u8(((t1 >> FONT_STYLE_SHIFT) & FONT_STYLE_MASK) as u8)
}

#[inline(always)]
pub fn decode_text_align(t1: u64) -> StyleTextAlign {
    style_text_align_from_u8(((t1 >> TEXT_ALIGN_SHIFT) & TEXT_ALIGN_MASK) as u8)
}

#[inline(always)]
pub fn decode_visibility(t1: u64) -> StyleVisibility {
    style_visibility_from_u8(((t1 >> VISIBILITY_SHIFT) & VISIBILITY_MASK) as u8)
}

#[inline(always)]
pub fn decode_white_space(t1: u64) -> StyleWhiteSpace {
    style_white_space_from_u8(((t1 >> WHITE_SPACE_SHIFT) & WHITE_SPACE_MASK) as u8)
}

#[inline(always)]
pub fn decode_direction(t1: u64) -> StyleDirection {
    style_direction_from_u8(((t1 >> DIRECTION_SHIFT) & DIRECTION_MASK) as u8)
}

#[inline(always)]
pub fn decode_vertical_align(t1: u64) -> StyleVerticalAlign {
    style_vertical_align_from_u8(((t1 >> VERTICAL_ALIGN_SHIFT) & VERTICAL_ALIGN_MASK) as u8)
}

#[inline(always)]
pub fn decode_border_collapse(t1: u64) -> StyleBorderCollapse {
    border_collapse_from_u8(((t1 >> BORDER_COLLAPSE_SHIFT) & BORDER_COLLAPSE_MASK) as u8)
}

/// Returns true if the tier1 u64 was actually populated by `encode_tier1`.
#[inline(always)]
pub fn tier1_is_populated(t1: u64) -> bool {
    (t1 & TIER1_POPULATED_BIT) != 0
}

// =============================================================================
// Tier 2: CompactNodeProps — numeric dimensions (64 bytes/node)
// =============================================================================

/// u32 encoding for dimension properties (width, height, min-*, max-*, flex-basis, font-size).
///
/// Layout: `[3:0] SizeMetric (4 bits) | [31:4] signed fixed-point ×1000 (28 bits)`
///
/// This matches FloatValue's internal representation (isize × 1000).
/// Range: ±134,217.727 at 0.001 precision (28-bit signed = ±2^27 = ±134,217,728 / 1000).
///
/// Sentinel values use the top of the u32 range (0xFFFFFFF9..0xFFFFFFFF).

/// Encode a PixelValue into u32 with SizeMetric. Returns U32_SENTINEL if out of range.
#[inline]
pub fn encode_pixel_value_u32(pv: &PixelValue) -> u32 {
    let metric = size_metric_to_u8(pv.metric) as u32;
    let raw = pv.number.number; // already × 1000 (FloatValue internal repr)
    // 28-bit signed range: -134_217_728 ..= +134_217_727
    if raw < -134_217_728 || raw > 134_217_727 {
        return U32_SENTINEL; // overflow → tier 3
    }
    // Pack: low 4 bits = metric, upper 28 bits = value (as unsigned offset)
    let value_bits = ((raw as i32) as u32) << 4;
    value_bits | metric
}

/// Decode a u32 back to PixelValue. Returns None for sentinel values.
#[inline]
pub fn decode_pixel_value_u32(encoded: u32) -> Option<PixelValue> {
    if encoded >= U32_SENTINEL_THRESHOLD {
        return None; // sentinel
    }
    let metric = size_metric_from_u8((encoded & 0xF) as u8);
    // Cast to i32 FIRST, then arithmetic right-shift to preserve sign bit
    let value_bits = (encoded as i32) >> 4;
    let raw = value_bits as isize; // × 1000
    Some(PixelValue {
        metric,
        number: FloatValue { number: raw },
    })
}

/// Encode an i16 resolved px value (×10). Returns I16_SENTINEL if out of range.
/// Range: -3276.8 ..= +3276.3 px at 0.1px precision.
#[inline]
pub fn encode_resolved_px_i16(px: f32) -> i16 {
    let scaled = (px * 10.0).round() as i32;
    if scaled < -32768 || scaled > I16_SENTINEL_THRESHOLD as i32 - 1 {
        return I16_SENTINEL; // overflow or too large → tier 3
    }
    scaled as i16
}

/// Decode an i16 back to resolved px. Returns None for sentinel values.
#[inline(always)]
pub fn decode_resolved_px_i16(v: i16) -> Option<f32> {
    if v >= I16_SENTINEL_THRESHOLD {
        return None;
    }
    Some(v as f32 / 10.0)
}

/// Encode a u16 flex value (×100). Returns U16_SENTINEL if out of range.
/// Range: 0.00 ..= 655.27 at 0.01 precision.
#[inline]
pub fn encode_flex_u16(value: f32) -> u16 {
    let scaled = (value * 100.0).round() as i32;
    if scaled < 0 || scaled >= U16_SENTINEL_THRESHOLD as i32 {
        return U16_SENTINEL;
    }
    scaled as u16
}

/// Decode a u16 flex value back to f32. Returns None for sentinel values.
#[inline(always)]
pub fn decode_flex_u16(v: u16) -> Option<f32> {
    if v >= U16_SENTINEL_THRESHOLD {
        return None;
    }
    Some(v as f32 / 100.0)
}

/// SizeMetric → u8 (4 bits, 12 variants)
#[inline(always)]
pub fn size_metric_to_u8(m: SizeMetric) -> u8 {
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

/// u8 → SizeMetric
#[inline(always)]
pub fn size_metric_from_u8(v: u8) -> SizeMetric {
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

/// Compact numeric properties for a single node (96 bytes).
/// All dimensions use MSB-sentinel encoding.
#[derive(Debug, Copy, Clone, PartialEq)]
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

    // --- Border colors (u32 RGBA as 0xRRGGBBAA, 0 = unset sentinel) ---
    pub border_top_color: u32,
    pub border_right_color: u32,
    pub border_bottom_color: u32,
    pub border_left_color: u32,

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
    pub border_spacing_h: i16,
    pub border_spacing_v: i16,
    pub tab_size: i16,

    // --- Flex (u16 MSB-sentinel, ×100) ---
    pub flex_grow: u16,
    pub flex_shrink: u16,

    // --- Other ---
    pub z_index: i16,   // range ±32764, sentinel = 0x7FFF
    /// Border styles packed: [3:0]=top, [7:4]=right, [11:8]=bottom, [15:12]=left
    pub border_styles_packed: u16,
    pub _pad: [u8; 2],
}

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
            // Border colors default to 0 (sentinel/unset)
            border_top_color: 0,
            border_right_color: 0,
            border_bottom_color: 0,
            border_left_color: 0,
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
            border_spacing_h: 0,
            border_spacing_v: 0,
            tab_size: I16_SENTINEL, // default is 8em, needs resolution → sentinel
            // Flex defaults
            flex_grow: 0,
            flex_shrink: encode_flex_u16(1.0), // CSS default: flex-shrink: 1
            // Other
            z_index: I16_AUTO,
            border_styles_packed: 0, // all BorderStyle::None
            _pad: [0; 2],
        }
    }
}

// =============================================================================
// Tier 2b: CompactTextProps — IFC/text properties (24 bytes/node)
// =============================================================================

/// Compact text/IFC properties for a single node (24 bytes).
#[derive(Debug, Copy, Clone, PartialEq)]
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

/// Overflow properties that couldn't fit in Tier 1/2 encoding.
/// Contains the original `CssProperty` values for properties that:
/// - Have calc() expressions
/// - Exceed the numeric range of compact encoding
/// - Are rare CSS properties (grid, transforms, etc.)
// =============================================================================
// CompactLayoutCache — the top-level container
// =============================================================================

/// Three-tier compact layout property cache.
///
/// Allocated once per restyle, indexed by node index (same as NodeId).
/// Provides O(1) array-indexed access to all layout properties.
///
/// Non-compact properties (background, box-shadow, transform, etc.) are
/// resolved via the slow cascade path in `CssPropertyCache::get_property_slow()`.
#[derive(Debug, Clone, PartialEq)]
pub struct CompactLayoutCache {
    /// Tier 1: ALL enum properties bitpacked into u64 per node (8 B/node)
    pub tier1_enums: Vec<u64>,
    /// Tier 2: Numeric dimensions per node (64 B/node)
    pub tier2_dims: Vec<CompactNodeProps>,
    /// Tier 2b: Text/IFC properties per node (24 B/node)
    pub tier2b_text: Vec<CompactTextProps>,
}

impl CompactLayoutCache {
    /// Create an empty cache (no nodes).
    pub fn empty() -> Self {
        Self {
            tier1_enums: Vec::new(),
            tier2_dims: Vec::new(),
            tier2b_text: Vec::new(),
        }
    }

    /// Create a cache pre-allocated for `node_count` nodes, filled with defaults.
    pub fn with_capacity(node_count: usize) -> Self {
        Self {
            tier1_enums: vec![0u64; node_count],
            tier2_dims: vec![CompactNodeProps::default(); node_count],
            tier2b_text: vec![CompactTextProps::default(); node_count],
        }
    }

    /// Number of nodes in this cache.
    #[inline]
    pub fn node_count(&self) -> usize {
        self.tier1_enums.len()
    }

    // -- Tier 1 getters (enum properties) --

    #[inline(always)]
    pub fn get_display(&self, node_idx: usize) -> LayoutDisplay {
        decode_display(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_position(&self, node_idx: usize) -> LayoutPosition {
        decode_position(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_float(&self, node_idx: usize) -> LayoutFloat {
        decode_float(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_overflow_x(&self, node_idx: usize) -> LayoutOverflow {
        decode_overflow_x(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_overflow_y(&self, node_idx: usize) -> LayoutOverflow {
        decode_overflow_y(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_box_sizing(&self, node_idx: usize) -> LayoutBoxSizing {
        decode_box_sizing(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_flex_direction(&self, node_idx: usize) -> LayoutFlexDirection {
        decode_flex_direction(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_flex_wrap(&self, node_idx: usize) -> LayoutFlexWrap {
        decode_flex_wrap(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_justify_content(&self, node_idx: usize) -> LayoutJustifyContent {
        decode_justify_content(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_align_items(&self, node_idx: usize) -> LayoutAlignItems {
        decode_align_items(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_align_content(&self, node_idx: usize) -> LayoutAlignContent {
        decode_align_content(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_writing_mode(&self, node_idx: usize) -> LayoutWritingMode {
        decode_writing_mode(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_clear(&self, node_idx: usize) -> LayoutClear {
        decode_clear(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_font_weight(&self, node_idx: usize) -> StyleFontWeight {
        decode_font_weight(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_font_style(&self, node_idx: usize) -> StyleFontStyle {
        decode_font_style(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_text_align(&self, node_idx: usize) -> StyleTextAlign {
        decode_text_align(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_visibility(&self, node_idx: usize) -> StyleVisibility {
        decode_visibility(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_white_space(&self, node_idx: usize) -> StyleWhiteSpace {
        decode_white_space(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_direction(&self, node_idx: usize) -> StyleDirection {
        decode_direction(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_vertical_align(&self, node_idx: usize) -> StyleVerticalAlign {
        decode_vertical_align(self.tier1_enums[node_idx])
    }

    #[inline(always)]
    pub fn get_border_collapse(&self, node_idx: usize) -> StyleBorderCollapse {
        decode_border_collapse(self.tier1_enums[node_idx])
    }

    // -- Tier 2 getters (numeric dimensions) --

    /// Get width as encoded u32 (use `decode_pixel_value_u32` or check sentinel).
    #[inline(always)]
    pub fn get_width_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].width
    }

    #[inline(always)]
    pub fn get_height_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].height
    }

    #[inline(always)]
    pub fn get_min_width_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].min_width
    }

    #[inline(always)]
    pub fn get_max_width_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].max_width
    }

    #[inline(always)]
    pub fn get_min_height_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].min_height
    }

    #[inline(always)]
    pub fn get_max_height_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].max_height
    }

    #[inline(always)]
    pub fn get_font_size_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].font_size
    }

    #[inline(always)]
    pub fn get_flex_basis_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].flex_basis
    }

    /// Get padding-top as resolved px. Returns None if sentinel (needs slow path).
    #[inline(always)]
    pub fn get_padding_top(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_top)
    }

    #[inline(always)]
    pub fn get_padding_right(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_right)
    }

    #[inline(always)]
    pub fn get_padding_bottom(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_bottom)
    }

    #[inline(always)]
    pub fn get_padding_left(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].padding_left)
    }

    #[inline(always)]
    pub fn get_margin_top(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_top;
        if v == I16_AUTO { return None; } // Auto for margin is special
        decode_resolved_px_i16(v)
    }

    #[inline(always)]
    pub fn get_margin_right(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_right;
        if v == I16_AUTO { return None; }
        decode_resolved_px_i16(v)
    }

    #[inline(always)]
    pub fn get_margin_bottom(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_bottom;
        if v == I16_AUTO { return None; }
        decode_resolved_px_i16(v)
    }

    #[inline(always)]
    pub fn get_margin_left(&self, node_idx: usize) -> Option<f32> {
        let v = self.tier2_dims[node_idx].margin_left;
        if v == I16_AUTO { return None; }
        decode_resolved_px_i16(v)
    }

    /// Check if margin is Auto (important for centering logic).
    #[inline(always)]
    pub fn is_margin_top_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_top == I16_AUTO
    }

    #[inline(always)]
    pub fn is_margin_right_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_right == I16_AUTO
    }

    #[inline(always)]
    pub fn is_margin_bottom_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_bottom == I16_AUTO
    }

    #[inline(always)]
    pub fn is_margin_left_auto(&self, node_idx: usize) -> bool {
        self.tier2_dims[node_idx].margin_left == I16_AUTO
    }

    #[inline(always)]
    pub fn get_border_top_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_top_width)
    }

    #[inline(always)]
    pub fn get_border_right_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_right_width)
    }

    #[inline(always)]
    pub fn get_border_bottom_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_bottom_width)
    }

    #[inline(always)]
    pub fn get_border_left_width(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2_dims[node_idx].border_left_width)
    }

    // -- Raw i16 getters for macro fast paths --

    #[inline(always)]
    pub fn get_padding_top_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_top
    }

    #[inline(always)]
    pub fn get_padding_right_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_right
    }

    #[inline(always)]
    pub fn get_padding_bottom_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_bottom
    }

    #[inline(always)]
    pub fn get_padding_left_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].padding_left
    }

    #[inline(always)]
    pub fn get_margin_top_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_top
    }

    #[inline(always)]
    pub fn get_margin_right_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_right
    }

    #[inline(always)]
    pub fn get_margin_bottom_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_bottom
    }

    #[inline(always)]
    pub fn get_margin_left_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].margin_left
    }

    #[inline(always)]
    pub fn get_border_top_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_top_width
    }

    #[inline(always)]
    pub fn get_border_right_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_right_width
    }

    #[inline(always)]
    pub fn get_border_bottom_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_bottom_width
    }

    #[inline(always)]
    pub fn get_border_left_width_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_left_width
    }

    #[inline(always)]
    pub fn get_top(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].top
    }

    #[inline(always)]
    pub fn get_right(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].right
    }

    #[inline(always)]
    pub fn get_bottom(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].bottom
    }

    #[inline(always)]
    pub fn get_left(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].left
    }

    #[inline(always)]
    pub fn get_flex_grow(&self, node_idx: usize) -> Option<f32> {
        decode_flex_u16(self.tier2_dims[node_idx].flex_grow)
    }

    #[inline(always)]
    pub fn get_flex_shrink(&self, node_idx: usize) -> Option<f32> {
        decode_flex_u16(self.tier2_dims[node_idx].flex_shrink)
    }

    #[inline(always)]
    pub fn get_z_index(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].z_index
    }

    // -- Border colors (u32 RGBA) --

    #[inline(always)]
    pub fn get_border_top_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].border_top_color
    }

    #[inline(always)]
    pub fn get_border_right_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].border_right_color
    }

    #[inline(always)]
    pub fn get_border_bottom_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].border_bottom_color
    }

    #[inline(always)]
    pub fn get_border_left_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2_dims[node_idx].border_left_color
    }

    // -- Border styles (packed u16) --

    #[inline(always)]
    pub fn get_border_styles_packed(&self, node_idx: usize) -> u16 {
        self.tier2_dims[node_idx].border_styles_packed
    }

    #[inline(always)]
    pub fn get_border_top_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_top_style(self.tier2_dims[node_idx].border_styles_packed)
    }

    #[inline(always)]
    pub fn get_border_right_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_right_style(self.tier2_dims[node_idx].border_styles_packed)
    }

    #[inline(always)]
    pub fn get_border_bottom_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_bottom_style(self.tier2_dims[node_idx].border_styles_packed)
    }

    #[inline(always)]
    pub fn get_border_left_style(&self, node_idx: usize) -> BorderStyle {
        decode_border_left_style(self.tier2_dims[node_idx].border_styles_packed)
    }

    // -- Border spacing --

    #[inline(always)]
    pub fn get_border_spacing_h_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_spacing_h
    }

    #[inline(always)]
    pub fn get_border_spacing_v_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].border_spacing_v
    }

    // -- Tab size --

    #[inline(always)]
    pub fn get_tab_size_raw(&self, node_idx: usize) -> i16 {
        self.tier2_dims[node_idx].tab_size
    }

    // -- Tier 2b getters (text props) --

    #[inline(always)]
    pub fn get_text_color_raw(&self, node_idx: usize) -> u32 {
        self.tier2b_text[node_idx].text_color
    }

    #[inline(always)]
    pub fn get_font_family_hash(&self, node_idx: usize) -> u64 {
        self.tier2b_text[node_idx].font_family_hash
    }

    #[inline(always)]
    pub fn get_line_height(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].line_height)
    }

    #[inline(always)]
    pub fn get_letter_spacing(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].letter_spacing)
    }

    #[inline(always)]
    pub fn get_word_spacing(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].word_spacing)
    }

    #[inline(always)]
    pub fn get_text_indent(&self, node_idx: usize) -> Option<f32> {
        decode_resolved_px_i16(self.tier2b_text[node_idx].text_indent)
    }

}

// =============================================================================
// Helper: encode a CssPropertyValue<PixelValue> into i16 resolved-px
// =============================================================================

/// Resolve a CssPropertyValue<PixelValue> to an i16 ×10 encoding.
/// Only handles `Exact(px(...))` values. Everything else → sentinel.
/// For the compact cache builder, we only pre-resolve absolute pixel values.
/// Relative units (em, %, etc.) get sentinel and fall back to the slow path.
#[inline]
pub fn encode_css_pixel_as_i16(prop: &CssPropertyValue<PixelValue>) -> i16 {
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
        assert_eq!(core::mem::size_of::<CompactNodeProps>(), 96);
    }

    #[test]
    fn test_compact_text_props_size() {
        assert_eq!(core::mem::size_of::<CompactTextProps>(), 24);
    }
}

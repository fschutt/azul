//! CSS property cache for efficient style resolution and animation.
//!
//! This module implements a cache layer between the raw CSS stylesheet and the rendered DOM.
//! It resolves CSS properties for each node, handling:
//!
//! - **Cascade resolution**: Computes final values from CSS rules, inline styles, and inheritance
//! - **Pseudo-class states**: Caches styles for `:hover`, `:active`, `:focus`, etc.
//! - **Animation support**: Tracks animating properties for smooth interpolation
//! - **Performance**: Avoids re-parsing and re-resolving unchanged properties
//!
//! # Architecture
//!
//! The cache is organized per-node and per-property-type. Each property has a dedicated
//! getter method that:
//!
//! 1. Checks if the property is cached
//! 2. If not, resolves it from CSS rules + inline styles
//! 3. Caches the result for subsequent frames
//!
//! # Thread Safety
//!
//! Not thread-safe. Each window has its own cache instance.

extern crate alloc;

use alloc::{boxed::Box, string::String, vec::Vec};
use core::fmt::Write;
use core::mem::ManuallyDrop;

use crate::dom::NodeType;

/// Tracks the origin of a CSS property value.
/// Used to correctly implement the CSS cascade and inheritance rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CssPropertyOrigin {
    /// Property was inherited from parent node (only for inheritable properties)
    Inherited,
    /// Property is the node's own value (from UA CSS, CSS file, inline style, or user override)
    Own,
}

/// A CSS property with its origin tracking.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CssPropertyWithOrigin {
    pub property: CssProperty,
    pub origin: CssPropertyOrigin,
}

use azul_css::{
    css::{Css, CssPath},
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        layout::{LayoutDisplay, LayoutHeight, LayoutWidth},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, CaretWidthValue, ClipPathValue, ColumnCountValue, ColumnFillValue,
            ColumnRuleColorValue, ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue,
            ColumnWidthValue, ContentValue, CounterIncrementValue, CounterResetValue, CssProperty,
            CssPropertyType, FlowFromValue, FlowIntoValue, LayoutAlignContentValue,
            LayoutAlignItemsValue, LayoutAlignSelfValue, LayoutBorderBottomWidthValue,
            LayoutBorderLeftWidthValue, LayoutBorderRightWidthValue, LayoutBorderSpacingValue,
            LayoutBorderTopWidthValue, LayoutBoxSizingValue, LayoutClearValue,
            LayoutColumnGapValue, LayoutDisplayValue, LayoutFlexBasisValue,
            LayoutFlexDirectionValue, LayoutFlexGrowValue, LayoutFlexShrinkValue,
            LayoutFlexWrapValue, LayoutFloatValue, LayoutGapValue, LayoutGridAutoColumnsValue,
            LayoutGridAutoFlowValue, LayoutGridAutoRowsValue, LayoutGridColumnValue,
            LayoutGridRowValue, LayoutGridTemplateColumnsValue, LayoutGridTemplateRowsValue,
            LayoutHeightValue, LayoutInsetBottomValue, LayoutJustifyContentValue,
            LayoutJustifyItemsValue, LayoutJustifySelfValue, LayoutLeftValue,
            LayoutMarginBottomValue, LayoutMarginLeftValue, LayoutMarginRightValue,
            LayoutMarginTopValue, LayoutMaxHeightValue, LayoutMaxWidthValue, LayoutMinHeightValue,
            LayoutMinWidthValue, LayoutOverflowValue, LayoutPaddingBottomValue,
            LayoutPaddingLeftValue, LayoutPaddingRightValue, LayoutPaddingTopValue,
            LayoutPositionValue, LayoutRightValue, LayoutRowGapValue, LayoutScrollbarWidthValue,
            LayoutTableLayoutValue, LayoutTextJustifyValue, LayoutTopValue, LayoutWidthValue,
            LayoutWritingModeValue, LayoutZIndexValue, OrphansValue, PageBreakValue,
            StyleBackgroundContentValue, ScrollbarFadeDelayValue, ScrollbarFadeDurationValue,
            ScrollbarVisibilityModeValue, SelectionBackgroundColorValue, SelectionColorValue,
            SelectionRadiusValue, ShapeImageThresholdValue, ShapeInsideValue, ShapeMarginValue,
            ShapeOutsideValue, StringSetValue, StyleBackfaceVisibilityValue,
            StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
            StyleBackgroundRepeatVecValue, StyleBackgroundSizeVecValue,
            StyleBorderBottomColorValue, StyleBorderBottomLeftRadiusValue,
            StyleBorderBottomRightRadiusValue, StyleBorderBottomStyleValue,
            StyleBorderCollapseValue, StyleBorderLeftColorValue, StyleBorderLeftStyleValue,
            StyleBorderRightColorValue, StyleBorderRightStyleValue, StyleBorderTopColorValue,
            StyleBorderTopLeftRadiusValue, StyleBorderTopRightRadiusValue,
            StyleBorderTopStyleValue, StyleBoxShadowValue, StyleCaptionSideValue, StyleCursorValue,
            StyleDirectionValue, StyleEmptyCellsValue, StyleExclusionMarginValue,
            StyleFilterVecValue, StyleFontFamilyVecValue, StyleFontSizeValue, StyleFontStyleValue,
            StyleFontValue, StyleFontWeightValue, StyleHangingPunctuationValue,
            StyleHyphenationLanguageValue, StyleHyphensValue, StyleInitialLetterValue,
            StyleLetterSpacingValue, StyleLineBreakValue, StyleLineClampValue, StyleLineHeightValue,
            StyleListStylePositionValue, StyleListStyleTypeValue, StyleMixBlendModeValue,
            StyleAspectRatioValue, StyleObjectFitValue, StyleObjectPositionValue,
            StyleOpacityValue, StylePerspectiveOriginValue,
            StyleScrollbarColorValue, StyleOverflowWrapValue, StyleTabSizeValue,
            StyleTextAlignLastValue, StyleTextOrientationValue,
            StyleTextAlignValue, StyleTextColorValue,
            StyleTextCombineUprightValue, StyleUnicodeBidiValue,
            StyleTextBoxTrimValue, StyleTextBoxEdgeValue,
            StyleDominantBaselineValue, StyleAlignmentBaselineValue,
            StyleInitialLetterAlignValue, StyleInitialLetterWrapValue,
            StyleScrollbarGutterValue, StyleOverflowClipMarginValue, StyleClipRectValue,
            StyleTextDecorationValue, StyleTextIndentValue,
            StyleTransformOriginValue, StyleTransformVecValue, StyleUserSelectValue,
            StyleVerticalAlignValue, StyleVisibilityValue, StyleWhiteSpaceValue,
            StyleWordBreakValue, StyleWordSpacingValue, WidowsValue,
        },
        style::{StyleCursor, StyleTextColor, StyleTransformOrigin},
    },
    AzString,
};

use crate::{
    dom::{NodeData, NodeId, TabIndex, TagId},
    id::{NodeDataContainer, NodeDataContainerRef},
    style::CascadeInfo,
    styled_dom::{
        NodeHierarchyItem, NodeHierarchyItemId, NodeHierarchyItemVec, ParentWithNodeDepth,
        ParentWithNodeDepthVec, StyledNodeState, TagIdToNodeIdMapping,
    },
};

use azul_css::dynamic_selector::{
    CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelectorContext,
};

#[cfg(feature = "std")]
std::thread_local! {
    static PROP_COUNTS: core::cell::RefCell<
        std::collections::HashMap<&'static str, usize>
    > = core::cell::RefCell::new(std::collections::HashMap::new());
}

/// Drain the per-thread CSS cascade-walk counter populated by
/// [`CssPropertyCache::get_property`] when `AZ_PROP_COUNT=1` is set
/// in the environment.
///
/// Returns `(property_label, count)` pairs
/// sorted by count descending. Layout-side instrumentation calls
/// this after each `layout_document` to print which properties
/// drove the most cascade walks.
#[cfg(feature = "std")]
#[must_use] pub fn drain_css_prop_counts() -> Vec<(&'static str, usize)> {
    // try_with: no real TLS in the lifted-to-wasm web backend (see the
    // get_property recording site) — return empty rather than panic.
    PROP_COUNTS
        .try_with(|c| {
            let map = core::mem::take(&mut *c.borrow_mut());
            let mut v: Vec<_> = map.into_iter().collect();
            v.sort_by(|a, b| b.1.cmp(&a.1));
            v
        })
        .unwrap_or_default()
}

// Unit conversion constants (CSS absolute units → pixels)
const PT_TO_PX: f32 = 1.333_333;
const IN_TO_PX: f32 = 96.0;
const CM_TO_PX: f32 = 37.795_277;
const MM_TO_PX: f32 = 3.779_527_7;

/// Match on any `CssProperty` variant and access the inner `CssPropertyValue`<T>.
#[allow(unused_macros)]
macro_rules! match_property_value {
    ($property:expr, $value:ident, $expr:expr) => {
        match $property {
            CssProperty::CaretColor($value) => $expr,
            CssProperty::CaretAnimationDuration($value) => $expr,
            CssProperty::SelectionBackgroundColor($value) => $expr,
            CssProperty::SelectionColor($value) => $expr,
            CssProperty::SelectionRadius($value) => $expr,
            CssProperty::TextColor($value) => $expr,
            CssProperty::FontSize($value) => $expr,
            CssProperty::FontFamily($value) => $expr,
            CssProperty::FontWeight($value) => $expr,
            CssProperty::FontStyle($value) => $expr,
            CssProperty::TextAlign($value) => $expr,
            CssProperty::TextJustify($value) => $expr,
            CssProperty::VerticalAlign($value) => $expr,
            CssProperty::LetterSpacing($value) => $expr,
            CssProperty::TextIndent($value) => $expr,
            CssProperty::InitialLetter($value) => $expr,
            CssProperty::LineClamp($value) => $expr,
            CssProperty::HangingPunctuation($value) => $expr,
            CssProperty::TextCombineUpright($value) => $expr,
            CssProperty::UnicodeBidi($value) => $expr,
            CssProperty::TextBoxTrim($value) => $expr,
            CssProperty::TextBoxEdge($value) => $expr,
            CssProperty::DominantBaseline($value) => $expr,
            CssProperty::AlignmentBaseline($value) => $expr,
            CssProperty::InitialLetterAlign($value) => $expr,
            CssProperty::InitialLetterWrap($value) => $expr,
            CssProperty::ScrollbarGutter($value) => $expr,
            CssProperty::OverflowClipMargin($value) => $expr,
            CssProperty::Clip($value) => $expr,
            CssProperty::ExclusionMargin($value) => $expr,
            CssProperty::HyphenationLanguage($value) => $expr,
            CssProperty::LineHeight($value) => $expr,
            CssProperty::WordSpacing($value) => $expr,
            CssProperty::TabSize($value) => $expr,
            CssProperty::WhiteSpace($value) => $expr,
            CssProperty::Hyphens($value) => $expr,
            CssProperty::Direction($value) => $expr,
            CssProperty::UserSelect($value) => $expr,
            CssProperty::TextDecoration($value) => $expr,
            CssProperty::Cursor($value) => $expr,
            CssProperty::Display($value) => $expr,
            CssProperty::Float($value) => $expr,
            CssProperty::BoxSizing($value) => $expr,
            CssProperty::Width($value) => $expr,
            CssProperty::Height($value) => $expr,
            CssProperty::MinWidth($value) => $expr,
            CssProperty::MinHeight($value) => $expr,
            CssProperty::MaxWidth($value) => $expr,
            CssProperty::MaxHeight($value) => $expr,
            CssProperty::Position($value) => $expr,
            CssProperty::Top($value) => $expr,
            CssProperty::Right($value) => $expr,
            CssProperty::Left($value) => $expr,
            CssProperty::Bottom($value) => $expr,
            CssProperty::ZIndex($value) => $expr,
            CssProperty::FlexWrap($value) => $expr,
            CssProperty::FlexDirection($value) => $expr,
            CssProperty::FlexGrow($value) => $expr,
            CssProperty::FlexShrink($value) => $expr,
            CssProperty::FlexBasis($value) => $expr,
            CssProperty::JustifyContent($value) => $expr,
            CssProperty::AlignItems($value) => $expr,
            CssProperty::AlignContent($value) => $expr,
            CssProperty::AlignSelf($value) => $expr,
            CssProperty::JustifyItems($value) => $expr,
            CssProperty::JustifySelf($value) => $expr,
            CssProperty::BackgroundContent($value) => $expr,
            CssProperty::BackgroundPosition($value) => $expr,
            CssProperty::BackgroundSize($value) => $expr,
            CssProperty::BackgroundRepeat($value) => $expr,
            CssProperty::OverflowX($value) => $expr,
            CssProperty::OverflowY($value) => $expr,
            CssProperty::OverflowBlock($value) => $expr,
            CssProperty::OverflowInline($value) => $expr,
            CssProperty::PaddingTop($value) => $expr,
            CssProperty::PaddingLeft($value) => $expr,
            CssProperty::PaddingRight($value) => $expr,
            CssProperty::PaddingBottom($value) => $expr,
            CssProperty::MarginTop($value) => $expr,
            CssProperty::MarginLeft($value) => $expr,
            CssProperty::MarginRight($value) => $expr,
            CssProperty::MarginBottom($value) => $expr,
            CssProperty::BorderTopLeftRadius($value) => $expr,
            CssProperty::BorderTopRightRadius($value) => $expr,
            CssProperty::BorderBottomLeftRadius($value) => $expr,
            CssProperty::BorderBottomRightRadius($value) => $expr,
            CssProperty::BorderTopColor($value) => $expr,
            CssProperty::BorderRightColor($value) => $expr,
            CssProperty::BorderLeftColor($value) => $expr,
            CssProperty::BorderBottomColor($value) => $expr,
            CssProperty::BorderTopStyle($value) => $expr,
            CssProperty::BorderRightStyle($value) => $expr,
            CssProperty::BorderLeftStyle($value) => $expr,
            CssProperty::BorderBottomStyle($value) => $expr,
            CssProperty::BorderTopWidth($value) => $expr,
            CssProperty::BorderRightWidth($value) => $expr,
            CssProperty::BorderLeftWidth($value) => $expr,
            CssProperty::BorderBottomWidth($value) => $expr,
            CssProperty::BoxShadow($value) => $expr,
            CssProperty::Opacity($value) => $expr,
            CssProperty::Transform($value) => $expr,
            CssProperty::TransformOrigin($value) => $expr,
            CssProperty::PerspectiveOrigin($value) => $expr,
            CssProperty::BackfaceVisibility($value) => $expr,
            CssProperty::MixBlendMode($value) => $expr,
            CssProperty::Filter($value) => $expr,
            CssProperty::Visibility($value) => $expr,
            CssProperty::WritingMode($value) => $expr,
            CssProperty::GridTemplateColumns($value) => $expr,
            CssProperty::GridTemplateRows($value) => $expr,
            CssProperty::GridAutoColumns($value) => $expr,
            CssProperty::GridAutoRows($value) => $expr,
            CssProperty::GridAutoFlow($value) => $expr,
            CssProperty::GridColumn($value) => $expr,
            CssProperty::GridRow($value) => $expr,
            CssProperty::GridTemplateAreas($value) => $expr,
            CssProperty::Gap($value) => $expr,
            CssProperty::ColumnGap($value) => $expr,
            CssProperty::RowGap($value) => $expr,
            CssProperty::Clear($value) => $expr,
            CssProperty::ScrollbarTrack($value) => $expr,
            CssProperty::ScrollbarThumb($value) => $expr,
            CssProperty::ScrollbarButton($value) => $expr,
            CssProperty::ScrollbarCorner($value) => $expr,
            CssProperty::ScrollbarResizer($value) => $expr,
            CssProperty::ScrollbarWidth($value) => $expr,
            CssProperty::ScrollbarColor($value) => $expr,
            CssProperty::ListStyleType($value) => $expr,
            CssProperty::ListStylePosition($value) => $expr,
            CssProperty::Font($value) => $expr,
            CssProperty::ColumnCount($value) => $expr,
            CssProperty::ColumnWidth($value) => $expr,
            CssProperty::ColumnSpan($value) => $expr,
            CssProperty::ColumnFill($value) => $expr,
            CssProperty::ColumnRuleStyle($value) => $expr,
            CssProperty::ColumnRuleWidth($value) => $expr,
            CssProperty::ColumnRuleColor($value) => $expr,
            CssProperty::FlowInto($value) => $expr,
            CssProperty::FlowFrom($value) => $expr,
            CssProperty::ShapeOutside($value) => $expr,
            CssProperty::ShapeInside($value) => $expr,
            CssProperty::ShapeImageThreshold($value) => $expr,
            CssProperty::ShapeMargin($value) => $expr,
            CssProperty::ClipPath($value) => $expr,
            CssProperty::Content($value) => $expr,
            CssProperty::CounterIncrement($value) => $expr,
            CssProperty::CounterReset($value) => $expr,
            CssProperty::StringSet($value) => $expr,
            CssProperty::Orphans($value) => $expr,
            CssProperty::Widows($value) => $expr,
            CssProperty::PageBreakBefore($value) => $expr,
            CssProperty::PageBreakAfter($value) => $expr,
            CssProperty::PageBreakInside($value) => $expr,
            CssProperty::BreakInside($value) => $expr,
            CssProperty::BoxDecorationBreak($value) => $expr,
            CssProperty::TableLayout($value) => $expr,
            CssProperty::BorderCollapse($value) => $expr,
            CssProperty::BorderSpacing($value) => $expr,
            CssProperty::CaptionSide($value) => $expr,
            CssProperty::EmptyCells($value) => $expr,
        }
    };
}

/// A CSS property tagged with its pseudo-state and property type.
///
/// Replaces the per-pseudo-state `BTreeMap` approach: instead of 6 `BTreeMaps`
/// per node (Normal/Hover/Active/Focus/Dragging/DragOver), we store one Vec
/// per node and tag each property with its state. Lookups use `.iter().find()`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatefulCssProperty {
    pub state: azul_css::dynamic_selector::PseudoStateType,
    pub prop_type: CssPropertyType,
    pub property: CssProperty,
}

// =============================================================================
// FlatVecVec: Cache-friendly replacement for Vec<Vec<T>>
// =============================================================================

/// A flat, cache-friendly replacement for `Vec<Vec<T>>`.
///
/// During the **build phase**, items are pushed into per-node inner Vecs
/// (same as before). After building is complete, `flatten()` compacts all
/// inner Vecs into a single contiguous `Vec<T>` with a `(start, len)` offset
/// table per node. All subsequent reads use the flat layout, eliminating
/// N heap allocations and pointer chasing.
///
/// ## Lifecycle
///
/// ```text
/// new(n) → push_to(idx, item)* → sort_each_and_flatten(key_fn) → get_slice(idx)*
///          ── build phase ──       ── transition ──                ── read phase ──
/// ```
#[derive(Debug, Clone)]
pub struct FlatVecVec<T> {
    /// Per-node inner Vecs (used during build phase, empty after flatten).
    build: Vec<Vec<T>>,
    /// Flat contiguous storage (populated after flatten).
    data: Vec<T>,
    /// `(start, len)` offsets into `data` for each node (populated after flatten).
    offsets: Vec<(u32, u32)>,
}

impl<T: PartialEq> PartialEq for FlatVecVec<T> {
    fn eq(&self, other: &Self) -> bool {
        let self_in_build = !self.build.is_empty() && self.offsets.is_empty();
        let other_in_build = !other.build.is_empty() && other.offsets.is_empty();
        debug_assert!(
            self_in_build == other_in_build,
            "FlatVecVec::eq called across phases (one build, one flattened)"
        );
        if self_in_build || other_in_build {
            self.build == other.build
        } else {
            self.data == other.data && self.offsets == other.offsets
        }
    }
}

impl<T> Default for FlatVecVec<T> {
    fn default() -> Self {
        Self {
            build: Vec::new(),
            data: Vec::new(),
            offsets: Vec::new(),
        }
    }
}

impl<T> FlatVecVec<T> {
    /// Approximate heap bytes retained. Sums capacity of the
    /// flattened `data` + `offsets` tables and the per-node build
    /// Vecs (in case `sort_each_and_flatten` hasn't been called
    /// yet). `per_element_size` should be `size_of::<T>()`.
    #[must_use] pub fn heap_bytes(&self, per_element_size: usize) -> usize {
        let data_bytes = self.data.capacity() * per_element_size;
        let offsets_bytes =
            self.offsets.capacity() * size_of::<(u32, u32)>();
        let mut build_bytes = self.build.capacity() * size_of::<Vec<T>>();
        for v in &self.build {
            build_bytes += v.capacity() * per_element_size;
        }
        data_bytes + offsets_bytes + build_bytes
    }

    /// Create a new `FlatVecVec` with `node_count` empty slots (build phase).
    #[must_use] pub fn new(node_count: usize) -> Self {
        let mut build = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            build.push(Vec::new());
        }
        Self {
            build,
            data: Vec::new(),
            offsets: Vec::new(),
        }
    }

    /// Push an item to the inner Vec at `node_index` (build phase).
    ///
    /// # Panics
    /// Panics if already flattened or if `node_index >= len()`.
    #[inline]
    pub fn push_to(&mut self, node_index: usize, item: T) {
        self.build[node_index].push(item);
    }

    /// Get a mutable reference to the inner Vec at `node_index` (build phase).
    #[inline]
    pub fn build_mut(&mut self, node_index: usize) -> &mut Vec<T> {
        &mut self.build[node_index]
    }

    /// Iterate mutably over all inner Vecs (build phase, e.g. for clearing).
    #[inline]
    pub fn build_iter_mut(&mut self) -> core::slice::IterMut<'_, Vec<T>> {
        self.build.iter_mut()
    }

    /// Get a reference to the inner Vec at `node_index` during build phase.
    /// During read phase, returns None (use `get_slice` instead).
    #[inline]
    #[must_use] pub fn build_get(&self, node_index: usize) -> Option<&Vec<T>> {
        self.build.get(node_index)
    }

    /// Number of node slots.
    #[inline]
    #[must_use] pub const fn len(&self) -> usize {
        if self.offsets.is_empty() {
            self.build.len()
        } else {
            self.offsets.len()
        }
    }

    /// Returns `true` if there are no node slots.
    #[inline]
    #[must_use] pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if this is in read (flattened) mode.
    #[inline]
    #[must_use] pub const fn is_flattened(&self) -> bool {
        !self.offsets.is_empty() || self.build.is_empty()
    }

    /// Get a slice for the node at `node_index` (read phase).
    /// Returns empty slice if index is out of bounds or not yet flattened
    /// (falls back to build-phase data if not yet flattened).
    #[inline]
    #[must_use] pub fn get_slice(&self, node_index: usize) -> &[T] {
        if self.offsets.is_empty() {
            // Build phase fallback: use inner Vecs
            self.build.get(node_index).map_or(&[], std::vec::Vec::as_slice)
        } else {
            // Read phase: use flat data
            if let Some(&(start, len)) = self.offsets.get(node_index) {
                let s = start as usize;
                let l = len as usize;
                &self.data[s..s + l]
            } else {
                &[]
            }
        }
    }

    /// Flatten: sort each inner Vec by key, deduplicate by keeping the last
    /// occurrence of each key (CSS cascade: later source order wins among
    /// equal specificity), then compact into flat storage.
    /// Drains all build-phase Vecs. After this call, only `get_slice()` works.
    pub fn sort_each_and_flatten<K: Ord + Eq>(&mut self, key_fn: impl Fn(&T) -> K) {
        let node_count = self.build.len();
        let total: usize = self.build.iter().map(std::vec::Vec::len).sum();

        let mut flat_data = Vec::with_capacity(total);
        let mut offsets = Vec::with_capacity(node_count);

        for inner in &mut self.build {
            inner.sort_by_key(|a| key_fn(a));

            // Deduplicate: keep last of each consecutive-key group (CSS cascade).
            let n = inner.len();
            let mut keep = vec![false; n];
            for i in 0..n {
                if i + 1 >= n || key_fn(&inner[i]) != key_fn(&inner[i + 1]) {
                    keep[i] = true;
                }
            }

            let start = flat_data.len() as u32;
            // Drain inner and push only kept items
            for (i, item) in inner.drain(..).enumerate() {
                if keep[i] {
                    flat_data.push(item);
                }
            }

            let len = (flat_data.len() as u32) - start;
            offsets.push((start, len));
        }

        flat_data.shrink_to_fit();
        self.data = flat_data;
        self.offsets = offsets;
        self.build = Vec::new();
    }

    /// Flatten without sorting (for data that's already sorted).
    pub fn flatten(&mut self) {
        let node_count = self.build.len();
        let total: usize = self.build.iter().map(std::vec::Vec::len).sum();

        let mut flat_data = Vec::with_capacity(total);
        let mut offsets = Vec::with_capacity(node_count);

        for inner in &mut self.build {
            let start = flat_data.len() as u32;
            let len = inner.len() as u32;
            offsets.push((start, len));
            flat_data.append(inner);
        }

        self.data = flat_data;
        self.offsets = offsets;
        self.build = Vec::new();
    }

    /// Rebuild flat storage, keeping only items matching `predicate`.
    /// Must be called after flatten. Preserves per-node ordering.
    pub fn retain(&mut self, predicate: impl Fn(&T) -> bool) where T: Clone {
        if self.offsets.is_empty() { return; }
        let node_count = self.offsets.len();
        let mut new_data = Vec::new();
        let mut new_offsets = Vec::with_capacity(node_count);
        for &(start, len) in &self.offsets {
            let s = start as usize;
            let l = len as usize;
            let new_start = new_data.len() as u32;
            let slice = &self.data[s..s + l];
            let mut kept = 0u32;
            for item in slice {
                if predicate(item) {
                    new_data.push((*item).clone());
                    kept += 1;
                }
            }
            new_offsets.push((new_start, kept));
        }
        new_data.shrink_to_fit();
        self.data = new_data;
        self.offsets = new_offsets;
    }

    /// Like `retain`, but passes each item's owning node index to the predicate.
    /// Must be called after flatten. Preserves per-node ordering.
    pub fn retain_with_node_index(
        &mut self,
        predicate: impl Fn(usize, &T) -> bool,
    ) where T: Clone {
        if self.offsets.is_empty() { return; }
        let node_count = self.offsets.len();
        let mut new_data = Vec::new();
        let mut new_offsets = Vec::with_capacity(node_count);
        for (node_idx, &(start, len)) in self.offsets.iter().enumerate() {
            let s = start as usize;
            let l = len as usize;
            let new_start = new_data.len() as u32;
            let slice = &self.data[s..s + l];
            let mut kept = 0u32;
            for item in slice {
                if predicate(node_idx, item) {
                    new_data.push((*item).clone());
                    kept += 1;
                }
            }
            new_offsets.push((new_start, kept));
        }
        new_data.shrink_to_fit();
        self.data = new_data;
        self.offsets = new_offsets;
    }

    /// Iterate over all nodes, yielding (`node_index`, &[T]) for each.
    /// Works in both build and flattened phases.
    pub(crate) const fn iter_node_slices(&self) -> FlatVecVecIter<'_, T> {
        FlatVecVecIter {
            fvv: self,
            idx: 0,
            count: self.len(),
        }
    }

    /// Extend this `FlatVecVec` with all nodes from `other` (append for DOM merge).
    /// Both must be in build phase, or both must be flattened.
    pub fn extend_from(&mut self, other: &mut Self) {
        if !self.offsets.is_empty() && !other.offsets.is_empty() {
            // Both flattened: extend flat data with offset adjustment
            let base = self.data.len() as u32;
            self.data.append(&mut other.data);
            self.offsets.extend(other.offsets.drain(..).map(|(s, l)| (s + base, l)));
        } else {
            // At least one in build phase: extend build vecs
            self.build.append(&mut other.build);
            // Invalidate flat data if it existed
            self.data.clear();
            self.offsets.clear();
        }
    }
}

/// Iterator over (`node_index`, &[T]) pairs from a `FlatVecVec`.
pub(crate) struct FlatVecVecIter<'a, T> {
    fvv: &'a FlatVecVec<T>,
    idx: usize,
    count: usize,
}

impl<'a, T> Iterator for FlatVecVecIter<'a, T> {
    type Item = (usize, &'a [T]);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if self.idx >= self.count {
            return None;
        }
        let i = self.idx;
        self.idx += 1;
        Some((i, self.fvv.get_slice(i)))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let rem = self.count - self.idx;
        (rem, Some(rem))
    }
}

impl<T> ExactSizeIterator for FlatVecVecIter<'_, T> {}

// NOTE: To avoid large memory allocations, this is a "cache" that stores all the CSS properties
// found in the DOM. This cache exists on a per-DOM basis, so it scales independent of how many
// nodes are in the DOM.
//
// If each node would carry its own CSS properties, that would unnecessarily consume memory
// because most nodes use the default properties or override only one or two properties.
//
// The cache can compute the property of any node at any given time, given the current node
// state (hover, active, focused, normal). This way we don't have to duplicate the CSS properties
// onto every single node and exchange them when the style changes. Two caches can be appended
// to each other by simply merging their NodeIds.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct CssPropertyCache {
    // number of nodes in the current DOM
    pub node_count: usize,

    // properties that were overridden in callbacks (not specific to any node state)
    pub user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>,

    // non-default CSS properties that were cascaded from the parent,
    // unified across all pseudo-states (Normal, Hover, Active, Focus, Dragging, DragOver).
    // Stored in a flat cache-friendly layout after sort_and_flatten().
    pub cascaded_props: FlatVecVec<StatefulCssProperty>,

    // non-default CSS properties that were set via a CSS file,
    // unified across all pseudo-states.
    pub css_props: FlatVecVec<StatefulCssProperty>,

    // Pre-resolved inherited properties (sorted Vec per node, keyed by CssPropertyType)
    pub computed_values: Vec<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,

    // Compact layout cache: three-tier numeric encoding for O(1) layout lookups.
    // Built once after restyle + apply_ua_css + compute_inherited_values.
    // Non-compact properties (background, shadow, transform) use get_property_slow().
    pub compact_cache: Option<azul_css::compact_cache::CompactLayoutCache>,

    // Global CSS properties from `*` rules — shared across all nodes.
    // Applied during build_compact_cache_with_inheritance instead of being
    // cloned into each node's css_props (saves 50K×N clones).
    pub global_css_props: Vec<CssProperty>,

    /// Per-node resolved font-size, in pixels, for the `Normal`
    /// pseudo-state. Populated lazily on first call to
    /// [`crate::styled_dom::StyledDom::resolved_font_size_px`] via a
    /// single bottom-up DOM walk; subsequent reads are O(1) Vec
    /// index by `NodeId::index()`.
    ///
    /// Motivation: `get_font_size` is called ~730× per node per
    /// layout pass (see `AZ_PROP_COUNT=1` report — 329 629
    /// cascade walks on excel.html alone). Each resolution
    /// recursively reads the parent's font-size (for `em`) plus
    /// the root's font-size (for `rem`), multiplying the walk
    /// count. Caching the pre-resolved pixel value collapses that
    /// to a single `Vec<f32>` indexed lookup.
    pub resolved_font_sizes_px: crate::sync::OnceLock<Vec<f32>>,
}

/// Heap-size breakdown of a `CssPropertyCache`, produced by
/// [`CssPropertyCache::memory_breakdown`]. All values in bytes.
///
/// Primarily a diagnostic — the numbers are capacity-based and
/// don't chase into property-variant payloads (e.g. the `Vec`
/// inside a `FontFamily(...)`). Intended for "which subfield is
/// eating RSS" triage, not for precise accounting.
#[derive(Debug, Clone, Copy, Default)]
pub struct CssPropertyCacheBreakdown {
    pub node_count: usize,
    pub cascaded_props_bytes: usize,
    pub css_props_bytes: usize,
    pub computed_values_bytes: usize,
    pub user_overridden_bytes: usize,
    pub global_css_props_bytes: usize,
    pub compact_cache_bytes: usize,
    pub resolved_font_sizes_bytes: usize,
}

impl CssPropertyCacheBreakdown {
    /// Sum of all subfields.
    #[must_use] pub const fn total_bytes(&self) -> usize {
        self.cascaded_props_bytes
            + self.css_props_bytes
            + self.computed_values_bytes
            + self.user_overridden_bytes
            + self.global_css_props_bytes
            + self.compact_cache_bytes
            + self.resolved_font_sizes_bytes
    }
}

impl CssPropertyCache {
    /// Approximate heap bytes retained by this cache, broken out by
    /// subfield. Used by `StyledDom::memory_breakdown` + the
    /// `AZ_MEM_BREAKDOWN=1` reporter. Sums capacity × element size
    /// for each Vec and adds a coarse allowance for the inner Vec
    /// headers inside `computed_values`.
    ///
    /// This is a measurement helper, not a tight bound — it doesn't
    /// chase into the `CssProperty` enum variants that carry their
    /// own `Vec`/`String` allocations (notably `FontFamily` →
    /// `StyleFontFamilyVec` → `Vec<StyleFontFamily>`), so the real
    /// heap footprint for a property-rich DOM can be 2-3× these
    /// numbers. Still useful for spotting gross duplication between
    /// the pre-compact and compact caches.
    pub fn memory_breakdown(&self) -> CssPropertyCacheBreakdown {
        let stateful_sz = size_of::<StatefulCssProperty>();
        let computed_entry_sz =
            size_of::<(CssPropertyType, CssPropertyWithOrigin)>();
        let outer_vec_sz = size_of::<Vec<(CssPropertyType, CssPropertyWithOrigin)>>();

        let cascaded_bytes = self.cascaded_props.heap_bytes(stateful_sz);
        let css_bytes = self.css_props.heap_bytes(stateful_sz);

        let mut computed_bytes = self.computed_values.capacity() * outer_vec_sz;
        for v in &self.computed_values {
            computed_bytes += v.capacity() * computed_entry_sz;
        }

        let user_overridden_bytes = {
            let mut b = self.user_overridden_properties.capacity() * outer_vec_sz;
            for v in &self.user_overridden_properties {
                b += v.capacity()
                    * size_of::<(CssPropertyType, CssProperty)>();
            }
            b
        };

        let global_bytes = self.global_css_props.capacity()
            * size_of::<CssProperty>();

        let compact_bytes = self
            .compact_cache
            .as_ref()
            .map_or(0, |c| {
                c.tier1_enums.capacity() * 8
                    + c.tier2_dims.capacity() * 68
                    + c.tier2_cold.capacity() * 28
                    + c.tier2b_text.capacity() * 24
                    + c.prev_font_hashes.capacity() * 8
                    + c.font_dirty_nodes.capacity() * 8
            });

        let resolved_font_sizes_bytes = self
            .resolved_font_sizes_px
            .get()
            .map_or(0, |v| v.capacity() * size_of::<f32>());

        CssPropertyCacheBreakdown {
            node_count: self.node_count,
            cascaded_props_bytes: cascaded_bytes,
            css_props_bytes: css_bytes,
            computed_values_bytes: computed_bytes,
            user_overridden_bytes,
            global_css_props_bytes: global_bytes,
            compact_cache_bytes: compact_bytes,
            resolved_font_sizes_bytes,
        }
    }

    /// Drop Normal-state properties that have compact encodings from
    /// `css_props` and `cascaded_props`. After `build_compact_cache_with_inheritance`,
    /// these are redundant — the compact cache is the source of truth for layout.
    /// Non-Normal entries (hover/active/focus) and non-compact properties
    /// (background, box-shadow, transform, etc.) are kept for `get_property_slow`.
    pub fn prune_compact_normal_props(&mut self) {
        use azul_css::dynamic_selector::PseudoStateType;

        #[cfg(feature = "std")]
        {
        static PRUNE_DBG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let dbg = *PRUNE_DBG.get_or_init(crate::profile::memory_enabled);
        if dbg {
            let mut normal_compact = 0usize;
            let mut normal_noncompact = 0usize;
            let mut nonnormal = 0usize;
            for i in 0..self.css_props.len() {
                for p in self.css_props.get_slice(i) {
                    if p.state != PseudoStateType::Normal {
                        nonnormal += 1;
                    } else if p.prop_type.has_compact_encoding() {
                        normal_compact += 1;
                    } else {
                        normal_noncompact += 1;
                    }
                }
            }
            let ssp_sz = size_of::<StatefulCssProperty>();
            let mut casc_normal_compact = 0usize;
            let mut casc_total = 0usize;
            for i in 0..self.cascaded_props.len() {
                for p in self.cascaded_props.get_slice(i) {
                    casc_total += 1;
                    if p.state == PseudoStateType::Normal && p.prop_type.has_compact_encoding() {
                        casc_normal_compact += 1;
                    }
                }
            }
            eprintln!("[PRUNE] css_props: norm+compact={normal_compact} norm+other={normal_noncompact} nonnorm={nonnormal} SSP={ssp_sz}B | cascaded: total={casc_total} norm+compact={casc_normal_compact}");
        }
        }

        // The compact cache stores SENTINEL for pixel-valued properties whose inner
        // value is Exact with a non-px metric (vh, vw, %, em, rem, calc(), ...).
        // Those need the slow `css_props` walk at layout time because the compact
        // cache has nothing usable. We must keep them here or the slow path falls
        // back to UA CSS and silently clobbers the author's rule.
        let keep = |p: &StatefulCssProperty| -> bool {
            if p.state != PseudoStateType::Normal {
                return true;
            }
            if !p.prop_type.has_compact_encoding() {
                return true;
            }
            // Compact-encoded AND Normal: drop only if the compact cache fully
            // captured the value (px metric, or Auto/Initial/Inherit/None).
            if property_needs_slow_path_after_compact(&p.property) {
                return true;
            }
            false
        };
        // DO NOT prune css_props: regenerate_layout calls
        // recompute_inheritance_and_compact_cache() every frame, which REBUILDS the
        // compact cache from css_props (build_compact_cache_with_inheritance reads
        // css_props in its per-node Step 3). If we drop compact-encoded Normal props
        // here, that rebuild reads pruned css_props and resets those props to their
        // CSS-initial value — e.g. white-space:pre-wrap on a node regressed to Normal
        // on the 2nd (recompute) build, collapsing \n in pre-wrap text into one line
        // (#8, intermittently — depends on whether the recompute ran). The doc's
        // premise ("the compact cache is the source of truth", implying permanence)
        // is false given that per-frame recompute. cascaded_props is NOT read by the
        // rebuild (Step 1 inherits from the parent's COMPACT value, not cascaded_props),
        // so pruning it remains safe. TODO: re-enable css_props pruning once recompute
        // becomes incremental (preserve directly-set compact values instead of rebuilding).
        if !self.cascaded_props.is_flattened() {
            self.cascaded_props.sort_each_and_flatten(|p| (p.state, p.prop_type));
        }
        self.cascaded_props.retain(keep);
    }

    /// Look up a CSS property for a specific pseudo-state in a stateful property vec.
    /// Requires the vec to be sorted by (state, `prop_type`).
    #[inline]
    // prop_cache threads &NodeId/&CssPropertyType uniformly through its hot cascade
    // lookup API (40+ such params); flipping only clippy's few flags to by-value
    // would force ref/deref juggling at every boundary with the by-ref majority,
    // for no measurable hot-path gain — keep the uniform by-ref convention.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    fn find_in_stateful<'a>(
        props: &'a [StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
        prop_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        let key = (state, *prop_type);
        props.binary_search_by_key(&key, |p| (p.state, p.prop_type))
            .ok()
            .map(|idx| &props[idx].property)
    }

    /// Check if any properties exist for a specific pseudo-state in a stateful property vec.
    /// Requires the vec to be sorted by (state, `prop_type`).
    #[inline]
    fn has_state_props(
        props: &[StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
    ) -> bool {
        // All entries with the same state are contiguous. Use partition_point
        // to find the first entry >= state, then check if it matches.
        let i = props.partition_point(|p| p.state < state);
        i < props.len() && props[i].state == state
    }

    /// Collect all property types for a specific pseudo-state.
    pub(crate) fn prop_types_for_state(
        props: &[StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
    ) -> impl Iterator<Item = &CssPropertyType> + '_ {
        props.iter().filter(move |p| p.state == state).map(|p| &p.prop_type)
    }
}

/// Returns true if `prop`'s value cannot be fully represented in the compact
/// cache and therefore needs to survive `prune_compact_normal_props` so the
/// slow `css_props` walk can still find it at layout time.
///
/// Pixel-valued properties (margin, padding, width, height, ...) are the only
/// case: `Exact(pv)` with `pv.metric != Px` (vh, vw, %, em, rem, ...) encodes
/// to the compact cache's SENTINEL slot, which loses the value. All other
/// compact-encoded types (tier1 enums, colors, hashes, etc.) always round-trip
/// through the compact encoding.
fn property_needs_slow_path_after_compact(prop: &CssProperty) -> bool {
    use azul_css::css::CssPropertyValue;
    use azul_css::props::{
        basic::length::SizeMetric,
        layout::{
            dimensions::{LayoutHeight, LayoutWidth},
            flex::LayoutFlexBasis,
        },
    };

    // `inner: PixelValue` wrapper types — check metric directly.
    macro_rules! check_plain {
        ($v:expr) => {{
            if let CssPropertyValue::Exact(ref inner) = $v {
                return inner.inner.metric != SizeMetric::Px;
            }
            false
        }};
    }

    match prop {
        // LayoutWidth / LayoutHeight: enum with `Px(PixelValue)` variant.
        // Non-pixel variants (Auto / MinContent / MaxContent / FitContent / Calc)
        // are already handled by the tier1 fast path or don't exist as i16 dims.
        CssProperty::Width(v) => {
            if let CssPropertyValue::Exact(LayoutWidth::Px(pv)) = v {
                return pv.metric != SizeMetric::Px;
            }
            false
        }
        CssProperty::Height(v) => {
            if let CssPropertyValue::Exact(LayoutHeight::Px(pv)) = v {
                return pv.metric != SizeMetric::Px;
            }
            false
        }

        // LayoutFlexBasis: enum with `Exact(PixelValue)` variant.
        CssProperty::FlexBasis(v) => {
            if let CssPropertyValue::Exact(LayoutFlexBasis::Exact(pv)) = v {
                return pv.metric != SizeMetric::Px;
            }
            false
        }

        // `inner: PixelValue` wrappers
        CssProperty::MinWidth(v) => check_plain!(v),
        CssProperty::MaxWidth(v) => check_plain!(v),
        CssProperty::MinHeight(v) => check_plain!(v),
        CssProperty::MaxHeight(v) => check_plain!(v),
        CssProperty::FontSize(v) => check_plain!(v),
        CssProperty::PaddingTop(v) => check_plain!(v),
        CssProperty::PaddingRight(v) => check_plain!(v),
        CssProperty::PaddingBottom(v) => check_plain!(v),
        CssProperty::PaddingLeft(v) => check_plain!(v),
        CssProperty::MarginTop(v) => check_plain!(v),
        CssProperty::MarginRight(v) => check_plain!(v),
        CssProperty::MarginBottom(v) => check_plain!(v),
        CssProperty::MarginLeft(v) => check_plain!(v),
        CssProperty::BorderTopWidth(v) => check_plain!(v),
        CssProperty::BorderRightWidth(v) => check_plain!(v),
        CssProperty::BorderBottomWidth(v) => check_plain!(v),
        CssProperty::BorderLeftWidth(v) => check_plain!(v),
        CssProperty::Top(v) => check_plain!(v),
        CssProperty::Right(v) => check_plain!(v),
        CssProperty::Bottom(v) => check_plain!(v),
        CssProperty::Left(v) => check_plain!(v),
        CssProperty::ColumnGap(v) => check_plain!(v),
        CssProperty::RowGap(v) => check_plain!(v),
        CssProperty::LetterSpacing(v) => check_plain!(v),
        CssProperty::WordSpacing(v) => check_plain!(v),
        CssProperty::TextIndent(v) => check_plain!(v),
        CssProperty::TabSize(v) => check_plain!(v),

        // All other compact-encoded types round-trip through the compact cache.
        _ => false,
    }
}

/// Clone a `CssProperty` WITHOUT going through its derived `Clone`. The derived clone
/// is a ~179-arm `match self { V(x) => V(x.clone()) }` that LLVM lowers to an indirect
/// HALFWORD jump table (`ldrh`-indexed). The web (remill→wasm) backend mis-lifts that
/// table, so for HEAP/Vec-bearing variants (gradients, font-family, shadows, filters,
/// transforms) the mis-dispatched clone reads wrong-sized data and the cascade traps
/// with "memory access out of bounds" (restyle → inherit → clone). Here every
/// heap-bearing variant is dispatched via single-variant `if let` — a direct
/// discriminant compare, NO jump table — and each inner `v.clone()` is the value
/// type's own clone, which lifts correctly. POD variants fall through to the derived
/// clone: correct on native, and harmless on web (a mis-dispatched discriminant 0 is
/// `CaretColor`, a `Copy` value with no heap pointer to deref). On native this function
/// is byte-for-byte equivalent to `p.clone()`.
fn clone_inheritable_property(
    p: &CssProperty,
) -> CssProperty {
    use azul_css::props::property::CssProperty;
    if let CssProperty::FontFamily(v) = p { return CssProperty::FontFamily(v.clone()); }
    if let CssProperty::BackgroundContent(v) = p { return CssProperty::BackgroundContent(v.clone()); }
    if let CssProperty::BackgroundPosition(v) = p { return CssProperty::BackgroundPosition(v.clone()); }
    if let CssProperty::BackgroundSize(v) = p { return CssProperty::BackgroundSize(v.clone()); }
    if let CssProperty::BackgroundRepeat(v) = p { return CssProperty::BackgroundRepeat(v.clone()); }
    if let CssProperty::BoxShadowLeft(v) = p { return CssProperty::BoxShadowLeft(v.clone()); }
    if let CssProperty::BoxShadowRight(v) = p { return CssProperty::BoxShadowRight(v.clone()); }
    if let CssProperty::BoxShadowTop(v) = p { return CssProperty::BoxShadowTop(v.clone()); }
    if let CssProperty::BoxShadowBottom(v) = p { return CssProperty::BoxShadowBottom(v.clone()); }
    if let CssProperty::TextShadow(v) = p { return CssProperty::TextShadow(v.clone()); }
    if let CssProperty::ScrollbarTrack(v) = p { return CssProperty::ScrollbarTrack(v.clone()); }
    if let CssProperty::ScrollbarThumb(v) = p { return CssProperty::ScrollbarThumb(v.clone()); }
    if let CssProperty::ScrollbarButton(v) = p { return CssProperty::ScrollbarButton(v.clone()); }
    if let CssProperty::ScrollbarCorner(v) = p { return CssProperty::ScrollbarCorner(v.clone()); }
    if let CssProperty::ScrollbarResizer(v) = p { return CssProperty::ScrollbarResizer(v.clone()); }
    if let CssProperty::Transform(v) = p { return CssProperty::Transform(v.clone()); }
    if let CssProperty::Filter(v) = p { return CssProperty::Filter(v.clone()); }
    if let CssProperty::BackdropFilter(v) = p { return CssProperty::BackdropFilter(v.clone()); }
    if let CssProperty::Content(v) = p { return CssProperty::Content(v.clone()); }
    if let CssProperty::HyphenationLanguage(v) = p { return CssProperty::HyphenationLanguage(v.clone()); }
    if let CssProperty::Cursor(v) = p { return CssProperty::Cursor(*v); }
    p.clone()
}

impl CssPropertyCache {
    /// Match CSS selectors to nodes and populate `css_props`.
    /// Returns tag IDs for hit-testing. If `compact_cache` is available,
    /// uses it for fast display/overflow checks; otherwise falls back to slow path.
    #[must_use]
    pub fn restyle(
        &mut self,
        css: &mut Css,
        node_data: &NodeDataContainerRef<NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
        html_tree: &NodeDataContainerRef<CascadeInfo>,
    ) -> Vec<TagIdToNodeIdMapping> {
        use azul_css::{
            css::{CssDeclaration, CssPathPseudoSelector::{Hover, Active, Focus, Dragging, DragOver}, CssPathSelector, CssRuleBlock},
            dynamic_selector::{DynamicSelector, PseudoStateType},
            props::layout::LayoutDisplay,
        };

        let css_is_empty = css.is_empty();

        if !css_is_empty {
            css.sort_by_specificity();

            // Separate CSS rules into "global only" (just `*`) vs "has specific selector".
            // Global-only rules apply to ALL nodes — push directly into css_props
            // without per-node selector matching (avoids m×n for these rules).
            // Specific rules still go through matches_html_element per-node.
            let mut global_only_rules: Vec<&CssRuleBlock> = Vec::new();
            let mut specific_rules: Vec<&CssRuleBlock> = Vec::new();

            for rule in css.rules() {
                let selectors = rule.path.selectors.as_ref();
                let is_global_only = selectors.len() == 1
                    && matches!(selectors.first(), Some(CssPathSelector::Global));
                if is_global_only {
                    global_only_rules.push(rule);
                } else {
                    specific_rules.push(rule);
                }
            }

            // Clear all css_props before assigning
            for entry in self.css_props.build_iter_mut() { entry.clear(); }

            // Collect global-only rule declarations ONCE (not per-node).
            // These are stored in self.global_css_props and applied during
            // build_compact_cache_with_inheritance for each node, avoiding
            // 50K × N clones into per-node css_props Vecs.
            self.global_css_props.clear();
            for rule in &global_only_rules {
                if crate::style::rule_ends_with(&rule.path, None) {
                    for d in &rule.declarations {
                        if let CssDeclaration::Static(s) = d {
                            self.global_css_props.push(s.clone());
                        }
                    }
                }
            }

            // Phase 2: Match specific rules per-node (only non-global rules)
            if !specific_rules.is_empty() {

            // Per-node "which declarations match" lists are built as
            // `(rule_idx, decl_idx)` pairs — 4 bytes per entry instead of
            // cloning a 140-byte `CssProperty`. The clone only happens at
            // the final push_to step, so the transient peak is ~35× smaller.
            //
            // rule_idx indexes into `specific_rules` (Vec<&CssRuleBlock>),
            // decl_idx indexes into `rule.declarations.as_slice()`. Both
            // fit in u16 since real stylesheets have far fewer than 65k
            // rules and declarations per rule.
            macro_rules! filter_rules {($expected_pseudo_selector:expr, $node_id:expr) => {{
                let mut out: Vec<(u16, u16)> = Vec::new();
                for (rule_idx, rule_block) in specific_rules.iter().enumerate() {
                    if !crate::style::rule_ends_with(&rule_block.path, $expected_pseudo_selector) {
                        continue;
                    }
                    if !crate::style::matches_html_element(
                        &rule_block.path,
                        $node_id,
                        &node_hierarchy.as_container(),
                        &node_data,
                        &html_tree,
                        $expected_pseudo_selector,
                    ) {
                        continue;
                    }
                    for (decl_idx, decl) in rule_block.declarations.as_slice().iter().enumerate() {
                        if matches!(decl, CssDeclaration::Static(_)) {
                            out.push((rule_idx as u16, decl_idx as u16));
                        }
                    }
                }
                out
            }};}

            // Pre-check which pseudo-states have any matching rules at all.
            // This avoids iterating 50K nodes for pseudo-states with zero rules
            // (common: most stylesheets have no :hover/:focus/:active rules).
            let has_normal = specific_rules.iter().any(|r| crate::style::rule_ends_with(&r.path, None));
            let has_hover = specific_rules.iter().any(|r| crate::style::rule_ends_with(&r.path, Some(Hover)));
            let has_active = specific_rules.iter().any(|r| crate::style::rule_ends_with(&r.path, Some(Active)));
            let has_focus = specific_rules.iter().any(|r| crate::style::rule_ends_with(&r.path, Some(Focus)));
            let has_dragging = specific_rules.iter().any(|r| crate::style::rule_ends_with(&r.path, Some(Dragging)));
            let has_drag_over = specific_rules.iter().any(|r| crate::style::rule_ends_with(&r.path, Some(DragOver)));

            macro_rules! collect_and_assign {
                ($pseudo:expr, $state:expr, $has_any:expr) => {
                    if $has_any {
                        let indices: NodeDataContainer<(NodeId, Vec<(u16, u16)>)> = node_data
                            .transform_nodeid_optional(|node_id| {
                                let r = filter_rules!($pseudo, node_id);
                                if r.is_empty() { None } else { Some((node_id, r)) }
                            });
                        for (n, pairs) in indices.internal.into_iter() {
                            for (rule_idx, decl_idx) in pairs {
                                let decl = &specific_rules[rule_idx as usize]
                                    .declarations
                                    .as_slice()[decl_idx as usize];
                                if let CssDeclaration::Static(prop) = decl {
                                    self.css_props.push_to(n.index(), StatefulCssProperty {
                                        state: $state,
                                        prop_type: prop.get_type(),
                                        property: prop.clone(),
                                    });
                                }
                            }
                        }
                    }
                };
            }

            collect_and_assign!(None, PseudoStateType::Normal, has_normal);
            collect_and_assign!(Some(Hover), PseudoStateType::Hover, has_hover);
            collect_and_assign!(Some(Active), PseudoStateType::Active, has_active);
            collect_and_assign!(Some(Focus), PseudoStateType::Focus, has_focus);
            collect_and_assign!(Some(Dragging), PseudoStateType::Dragging, has_dragging);
            collect_and_assign!(Some(DragOver), PseudoStateType::DragOver, has_drag_over);

            } // end if !specific_rules.is_empty()
        }

        // Inheritance: Inherit all values of the parent to the children, but
        // only if the property is inheritable and isn't yet set
        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes {
            let Some(parent_id) = node_id.into_crate_internal() else {
                continue;
            };

            let all_states = [
                PseudoStateType::Normal,
                PseudoStateType::Hover,
                PseudoStateType::Active,
                PseudoStateType::Focus,
                PseudoStateType::Dragging,
                PseudoStateType::DragOver,
            ];

            for &state in &all_states {
                // 1. Inherit inline CSS properties from parent for this pseudo-state
                let parent_inheritable_inline: Vec<(CssPropertyType, CssProperty)> = node_data[parent_id]
                    .style
                    .iter_inline_properties()
                    .filter(|(_prop, conds)| {
                        let conditions = conds.as_slice();
                        if conditions.is_empty() {
                            state == PseudoStateType::Normal
                        } else {
                            conditions.iter().all(|c| {
                                matches!(c, DynamicSelector::PseudoState(s) if *s == state)
                            })
                        }
                    })
                    .map(|(prop, _)| prop)
                    .filter(|prop| prop.get_type().is_inheritable())
                    .map(|p| (p.get_type(), clone_inheritable_property(p)))
                    .collect();

                // 2. Inherit CSS stylesheet properties from parent for this pseudo-state
                let parent_inheritable_css: Vec<(CssPropertyType, CssProperty)> = if css_is_empty {
                    Vec::new()
                } else {
                    self.css_props.get_slice(parent_id.index())
                        .iter()
                        .filter(|p| p.state == state && p.prop_type.is_inheritable())
                        .map(|p| (p.prop_type, clone_inheritable_property(&p.property)))
                        .collect()
                };

                // 3. Inherit cascaded properties from parent for this pseudo-state
                let parent_inheritable_cascaded: Vec<(CssPropertyType, CssProperty)> =
                    self.cascaded_props.get_slice(parent_id.index())
                        .iter()
                        .filter(|p| p.state == state && p.prop_type.is_inheritable())
                        .map(|p| (p.prop_type, clone_inheritable_property(&p.property)))
                        .collect();

                // Combine all inheritable props (inline first = strongest, cascaded last)
                // Only insert if child doesn't already have that (state, prop_type) combo
                if parent_inheritable_inline.is_empty()
                    && parent_inheritable_css.is_empty()
                    && parent_inheritable_cascaded.is_empty()
                {
                    continue;
                }

                for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                    let child_vec = self.cascaded_props.build_mut(child_id.index());
                    for (prop_type, prop_value) in parent_inheritable_inline
                        .iter()
                        .chain(parent_inheritable_css.iter())
                        .chain(parent_inheritable_cascaded.iter())
                    {
                        // or_insert: only insert if child doesn't already have this (state, prop_type)
                        if !child_vec.iter().any(|p| p.state == state && p.prop_type == *prop_type) {
                            child_vec.push(StatefulCssProperty {
                                state,
                                prop_type: *prop_type,
                                property: prop_value.clone(),
                            });
                        }
                    }
                }
            }
        }

        // Sort css_props by (state, prop_type) for binary search lookups,
        // then flatten into contiguous memory for cache-friendly reads.
        self.css_props.sort_each_and_flatten(|p| (p.state, p.prop_type));

        self.generate_tag_ids(node_data, node_hierarchy)
    }

    /// Generate hit-test tag IDs for nodes that need event handling.
    /// Uses compact cache (if available) for fast display/overflow reads.
    /// Can be called separately after `build_compact_cache_with_inheritance`.
    pub fn generate_tag_ids(
        &self,
        node_data: &NodeDataContainerRef<NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
    ) -> Vec<TagIdToNodeIdMapping> {

        // Tag ID generation: determine which nodes need hit-test tags for
        // hover/click/scroll events. Uses compact cache for display/overflow
        // checks instead of get_property_slow (which searches 6 data structures).
        use azul_css::compact_cache::{
            DISPLAY_SHIFT, DISPLAY_MASK,
            OVERFLOW_X_SHIFT, OVERFLOW_Y_SHIFT, OVERFLOW_MASK,
        };

        let compact_cache = self.compact_cache.as_ref();
        let node_data_container = &node_data.internal;

        let tag_ids = node_data
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_idx, node_data)| {
                let node_id = NodeId::new(node_idx);

                let should_auto_insert_tabindex = node_data
                    .get_callbacks()
                    .iter()
                    .any(|cb| cb.event.is_focus_callback());

                let tab_index = match node_data.get_tab_index() {
                    Some(s) => Some(s),
                    None => {
                        if should_auto_insert_tabindex {
                            Some(TabIndex::Auto)
                        } else {
                            None
                        }
                    }
                };

                let mut need_tag = false;

                // Single-pass guard block: each check `break`s out early once it
                // decides `need_tag`. Labeled block (not `loop`) makes the
                // never-iterating control flow explicit (clippy::never_loop).
                'compute_need_tag: {
                    // display:none check — read directly from compact tier1 (fast u64 read)
                    if let Some(cc) = compact_cache.as_ref() {
                        let t1 = cc.tier1_enums[node_idx];
                        let display_val = ((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8;
                        if display_val == 4 { break 'compute_need_tag; } // 4 = LayoutDisplay::None (new encoding)
                    }

                    if node_data.has_context_menu() || node_data.get_context_menu().is_some() {
                        need_tag = true; break 'compute_need_tag;
                    }
                    if tab_index.is_some() { need_tag = true; break 'compute_need_tag; }

                    // Pseudo-state property checks (hover/active/focus/dragging/drag-over)
                    {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        let has_pseudo = |state: PseudoStateType| -> bool {
                            node_data.style.iter_inline_properties().any(|(_p, conds)| {
                                conds.as_slice().iter().any(|c|
                                    matches!(c, DynamicSelector::PseudoState(s) if *s == state)
                                )
                            }) || Self::has_state_props(self.css_props.get_slice(node_idx), state)
                        };

                        if has_pseudo(PseudoStateType::Hover)
                            || has_pseudo(PseudoStateType::Active)
                            || has_pseudo(PseudoStateType::Focus)
                            || has_pseudo(PseudoStateType::Dragging)
                            || has_pseudo(PseudoStateType::DragOver)
                        {
                            need_tag = true; break 'compute_need_tag;
                        }
                    }

                    // Non-window callbacks
                    let has_non_window_cb = !node_data.get_callbacks().is_empty()
                        && !node_data.get_callbacks().iter().all(|cb| cb.event.is_window_callback());
                    if has_non_window_cb { need_tag = true; break 'compute_need_tag; }

                    // Cursor check — read from cached css_props or inline style.
                    if self.css_props.get_slice(node_idx).iter().any(|p|
                        p.state == azul_css::dynamic_selector::PseudoStateType::Normal
                        && p.prop_type == CssPropertyType::Cursor
                    ) || node_data.style.iter_inline_properties().any(|(p, _)|
                        p.get_type() == CssPropertyType::Cursor
                    ) {
                        need_tag = true; break 'compute_need_tag;
                    }

                    // Overflow scroll check — read from compact tier1
                    if let Some(cc) = compact_cache.as_ref() {
                        let t1 = cc.tier1_enums[node_idx];
                        let ox = ((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8;
                        let oy = ((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8;
                        // 2 = Scroll, 3 = Auto in layout_overflow_to_u8 (new encoding)
                        if ox == 2 || ox == 3 || oy == 2 || oy == 3 {
                            need_tag = true; break 'compute_need_tag;
                        }
                    }

                    // Selectable text check
                    {
                        use crate::dom::NodeType;
                        let hier = node_hierarchy.as_container()[node_id];
                        let mut has_text = false;
                        if let Some(first_child) = hier.first_child_id(node_id) {
                            let mut child_id = Some(first_child);
                            while let Some(cid) = child_id {
                                if matches!(node_data_container[cid.index()].get_node_type(), NodeType::Text(_)) {
                                    has_text = true; break;
                                }
                                child_id = node_hierarchy.as_container()[cid].next_sibling_id();
                            }
                        }
                        if has_text { need_tag = true; break 'compute_need_tag; }
                    }

                    break 'compute_need_tag;
                }

                if need_tag {
                    Some(TagIdToNodeIdMapping {
                        tag_id: TagId::from_crate_internal(TagId::unique()),
                        node_id: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                        tab_index: tab_index.into(),
                    })
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        tag_ids
    }

    pub fn get_computed_css_style_string(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> String {
        let mut s = String::new();
        if let Some(p) = self.get_background_content(node_data, node_id, node_state) {
            let _ = write!(s,"background: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_position(node_data, node_id, node_state) {
            let _ = write!(s,"background-position: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_size(node_data, node_id, node_state) {
            let _ = write!(s,"background-size: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_repeat(node_data, node_id, node_state) {
            let _ = write!(s,"background-repeat: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_font_size(node_data, node_id, node_state) {
            let _ = write!(s,"font-size: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_font_family(node_data, node_id, node_state) {
            let _ = write!(s,"font-family: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_color(node_data, node_id, node_state) {
            let _ = write!(s,"color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_align(node_data, node_id, node_state) {
            let _ = write!(s,"text-align: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_line_height(node_data, node_id, node_state) {
            let _ = write!(s,"line-height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_letter_spacing(node_data, node_id, node_state) {
            let _ = write!(s,"letter-spacing: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_word_spacing(node_data, node_id, node_state) {
            let _ = write!(s,"word-spacing: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_tab_size(node_data, node_id, node_state) {
            let _ = write!(s,"tab-size: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_cursor(node_data, node_id, node_state) {
            let _ = write!(s,"cursor: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_shadow_left(node_data, node_id, node_state) {
            let _ = write!(s,
                "-azul-box-shadow-left: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_box_shadow_right(node_data, node_id, node_state) {
            let _ = write!(s,
                "-azul-box-shadow-right: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_box_shadow_top(node_data, node_id, node_state) {
            let _ = write!(s,"-azul-box-shadow-top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_shadow_bottom(node_data, node_id, node_state) {
            let _ = write!(s,
                "-azul-box-shadow-bottom: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_border_top_color(node_data, node_id, node_state) {
            let _ = write!(s,"border-top-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_left_color(node_data, node_id, node_state) {
            let _ = write!(s,"border-left-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_right_color(node_data, node_id, node_state) {
            let _ = write!(s,"border-right-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_color(node_data, node_id, node_state) {
            let _ = write!(s,"border-bottom-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_style(node_data, node_id, node_state) {
            let _ = write!(s,"border-top-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_left_style(node_data, node_id, node_state) {
            let _ = write!(s,"border-left-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_right_style(node_data, node_id, node_state) {
            let _ = write!(s,"border-right-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_style(node_data, node_id, node_state) {
            let _ = write!(s,"border-bottom-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_left_radius(node_data, node_id, node_state) {
            let _ = write!(s,
                "border-top-left-radius: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_border_top_right_radius(node_data, node_id, node_state) {
            let _ = write!(s,
                "border-top-right-radius: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_border_bottom_left_radius(node_data, node_id, node_state) {
            let _ = write!(s,
                "border-bottom-left-radius: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_border_bottom_right_radius(node_data, node_id, node_state) {
            let _ = write!(s,
                "border-bottom-right-radius: {};",
                p.get_css_value_fmt()
            );
        }
        if let Some(p) = self.get_opacity(node_data, node_id, node_state) {
            let _ = write!(s,"opacity: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_transform(node_data, node_id, node_state) {
            let _ = write!(s,"transform: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_transform_origin(node_data, node_id, node_state) {
            let _ = write!(s,"transform-origin: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_perspective_origin(node_data, node_id, node_state) {
            let _ = write!(s,"perspective-origin: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_backface_visibility(node_data, node_id, node_state) {
            let _ = write!(s,"backface-visibility: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_hyphens(node_data, node_id, node_state) {
            let _ = write!(s,"hyphens: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_direction(node_data, node_id, node_state) {
            let _ = write!(s,"direction: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_unicode_bidi(node_data, node_id, node_state) {
            let _ = write!(s,"unicode-bidi: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_box_trim(node_data, node_id, node_state) {
            let _ = write!(s,"text-box-trim: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_box_edge(node_data, node_id, node_state) {
            let _ = write!(s,"text-box-edge: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_dominant_baseline(node_data, node_id, node_state) {
            let _ = write!(s,"dominant-baseline: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_alignment_baseline(node_data, node_id, node_state) {
            let _ = write!(s,"alignment-baseline: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_initial_letter_align(node_data, node_id, node_state) {
            let _ = write!(s,"initial-letter-align: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_initial_letter_wrap(node_data, node_id, node_state) {
            let _ = write!(s,"initial-letter-wrap: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_scrollbar_gutter(node_data, node_id, node_state) {
            let _ = write!(s,"scrollbar-gutter: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_overflow_clip_margin(node_data, node_id, node_state) {
            let _ = write!(s,"overflow-clip-margin: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_clip(node_data, node_id, node_state) {
            let _ = write!(s,"clip: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_white_space(node_data, node_id, node_state) {
            let _ = write!(s,"white-space: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_display(node_data, node_id, node_state) {
            let _ = write!(s,"display: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_float(node_data, node_id, node_state) {
            let _ = write!(s,"float: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_sizing(node_data, node_id, node_state) {
            let _ = write!(s,"box-sizing: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_width(node_data, node_id, node_state) {
            let _ = write!(s,"width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_height(node_data, node_id, node_state) {
            let _ = write!(s,"height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_min_width(node_data, node_id, node_state) {
            let _ = write!(s,"min-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_min_height(node_data, node_id, node_state) {
            let _ = write!(s,"min-height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_max_width(node_data, node_id, node_state) {
            let _ = write!(s,"max-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_max_height(node_data, node_id, node_state) {
            let _ = write!(s,"max-height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_position(node_data, node_id, node_state) {
            let _ = write!(s,"position: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_top(node_data, node_id, node_state) {
            let _ = write!(s,"top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_bottom(node_data, node_id, node_state) {
            let _ = write!(s,"bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_right(node_data, node_id, node_state) {
            let _ = write!(s,"right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_left(node_data, node_id, node_state) {
            let _ = write!(s,"left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_top(node_data, node_id, node_state) {
            let _ = write!(s,"padding-top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_bottom(node_data, node_id, node_state) {
            let _ = write!(s,"padding-bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_left(node_data, node_id, node_state) {
            let _ = write!(s,"padding-left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_right(node_data, node_id, node_state) {
            let _ = write!(s,"padding-right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_top(node_data, node_id, node_state) {
            let _ = write!(s,"margin-top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_bottom(node_data, node_id, node_state) {
            let _ = write!(s,"margin-bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_left(node_data, node_id, node_state) {
            let _ = write!(s,"margin-left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_right(node_data, node_id, node_state) {
            let _ = write!(s,"margin-right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_width(node_data, node_id, node_state) {
            let _ = write!(s,"border-top-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_left_width(node_data, node_id, node_state) {
            let _ = write!(s,"border-left-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_right_width(node_data, node_id, node_state) {
            let _ = write!(s,"border-right-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_width(node_data, node_id, node_state) {
            let _ = write!(s,"border-bottom-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_overflow_x(node_data, node_id, node_state) {
            let _ = write!(s,"overflow-x: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_overflow_y(node_data, node_id, node_state) {
            let _ = write!(s,"overflow-y: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_direction(node_data, node_id, node_state) {
            let _ = write!(s,"flex-direction: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_wrap(node_data, node_id, node_state) {
            let _ = write!(s,"flex-wrap: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_grow(node_data, node_id, node_state) {
            let _ = write!(s,"flex-grow: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_shrink(node_data, node_id, node_state) {
            let _ = write!(s,"flex-shrink: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_justify_content(node_data, node_id, node_state) {
            let _ = write!(s,"justify-content: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_align_items(node_data, node_id, node_state) {
            let _ = write!(s,"align-items: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_align_content(node_data, node_id, node_state) {
            let _ = write!(s,"align-content: {};", p.get_css_value_fmt());
        }
        s
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Clone)]
pub struct CssPropertyCachePtr {
    // `ManuallyDrop` so the owned `Box` is freed ONLY by our `Drop` (gated on
    // `run_destructor`), never by drop-glue. The codegen Az wrapper (AzStyledDom)
    // nests an AzCssPropertyCachePtr field whose own `Drop` re-runs
    // `_delete` -> `drop_in_place::<CssPropertyCachePtr>` on the SAME bytes; with a
    // bare `Box` the glue freed it a second time -> double free. Layout is
    // unchanged (one pointer), so the AzCssPropertyCachePtr<->CssPropertyCachePtr
    // FFI transmute stays valid. Matches the GlContextPtr / InstantPtr convention.
    pub ptr: ManuallyDrop<Box<CssPropertyCache>>,
    pub run_destructor: bool,
}

impl CssPropertyCachePtr {
    pub fn new(cache: CssPropertyCache) -> Self {
        Self {
            ptr: ManuallyDrop::new(Box::new(cache)),
            run_destructor: true,
        }
    }
    pub fn downcast_mut(&mut self) -> &mut CssPropertyCache {
        &mut self.ptr
    }
}

impl Drop for CssPropertyCachePtr {
    fn drop(&mut self) {
        // First drop (run_destructor still true) frees the Box and clears the flag in
        // the shared bytes; the codegen's redundant second drop sees false -> no-op.
        if self.run_destructor {
            self.run_destructor = false;
            unsafe {
                ManuallyDrop::drop(&mut self.ptr);
            }
        }
    }
}

/// Generates a mechanical `get_<name>` CSS-property accessor: resolve the property
/// for `(node_data, node_id, node_state)` via `get_property`, then downcast it with
/// the given `as_*` method. Covers the long run of one-line accessors below.
macro_rules! impl_get_prop {
    ($name:ident, $value_ty:ty, $variant:ident, $as_method:ident) => {
        pub fn $name<'a>(
            &'a self,
            node_data: &'a NodeData,
            node_id: &NodeId,
            node_state: &StyledNodeState,
        ) -> Option<&'a $value_ty> {
            self.get_property(node_data, node_id, node_state, &CssPropertyType::$variant)
                .and_then(|p| p.$as_method())
        }
    };
}

impl CssPropertyCache {
    #[must_use] pub fn empty(node_count: usize) -> Self {
        Self {
            node_count,
            user_overridden_properties: Vec::new(),

            cascaded_props: FlatVecVec::new(node_count),
            css_props: FlatVecVec::new(node_count),

            computed_values: Vec::new(),
            compact_cache: None,
            global_css_props: Vec::new(),
            resolved_font_sizes_px: crate::sync::OnceLock::new(),
        }
    }

    /// Clear the lazily-populated font-size cache. Call after any
    /// mutation that could change resolved font-sizes (restyle,
    /// DOM mutation, `append`, etc.). The next
    /// [`crate::styled_dom::StyledDom::resolved_font_size_px`] call
    /// repopulates via a single bottom-up tree walk.
    pub fn invalidate_resolved_font_sizes(&mut self) {
        self.resolved_font_sizes_px = crate::sync::OnceLock::new();
    }

    pub fn append(&mut self, other: &mut Self) {
        self.user_overridden_properties.append(&mut other.user_overridden_properties);
        self.cascaded_props.extend_from(&mut other.cascaded_props);
        self.css_props.extend_from(&mut other.css_props);
        self.computed_values.append(&mut other.computed_values);

        self.node_count += other.node_count;
        // Indices shifted — invalidate the font-size cache too.
        self.resolved_font_sizes_px = crate::sync::OnceLock::new();

        // Invalidate compact cache since node IDs shifted
        self.compact_cache = None;
    }

    pub fn is_horizontal_overflow_visible(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_x(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_visible()
    }

    pub fn is_vertical_overflow_visible(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_y(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_visible()
    }

    pub fn is_horizontal_overflow_hidden(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_x(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_hidden()
    }

    pub fn is_vertical_overflow_hidden(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_overflow_y(node_data, node_id, node_state)
            .and_then(|p| p.get_property_or_default())
            .unwrap_or_default()
            .is_overflow_hidden()
    }

    pub fn get_text_color_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleTextColor {
        use azul_css::defaults::DEFAULT_TEXT_COLOR;
        self.get_text_color(node_data, node_id, node_state)
            .and_then(|fs| fs.get_property().copied())
            .unwrap_or(DEFAULT_TEXT_COLOR)
    }

    /// Returns the font family of the node, or the default font family if none is set.
    pub fn get_font_id_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleFontFamilyVec {
        use azul_css::defaults::DEFAULT_FONT_ID;
        let default_font_id = vec![StyleFontFamily::System(AzString::from_const_str(
            DEFAULT_FONT_ID,
        ))]
        .into();
        let font_family_opt = self.get_font_family(node_data, node_id, node_state);

        font_family_opt
            .as_ref()
            .and_then(|family| Some(family.get_property()?.clone()))
            .unwrap_or(default_font_id)
    }

    pub fn get_font_size_or_default(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> StyleFontSize {
        use azul_css::defaults::DEFAULT_FONT_SIZE;
        self.get_font_size(node_data, node_id, node_state)
            .and_then(|fs| fs.get_property().copied())
            .unwrap_or(DEFAULT_FONT_SIZE)
    }

    pub fn has_border(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_border_left_width(node_data, node_id, node_state)
            .is_some()
            || self
                .get_border_right_width(node_data, node_id, node_state)
                .is_some()
            || self
                .get_border_top_width(node_data, node_id, node_state)
                .is_some()
            || self
                .get_border_bottom_width(node_data, node_id, node_state)
                .is_some()
    }

    pub fn has_box_shadow(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> bool {
        self.get_box_shadow_left(node_data, node_id, node_state)
            .is_some()
            || self
                .get_box_shadow_right(node_data, node_id, node_state)
                .is_some()
            || self
                .get_box_shadow_top(node_data, node_id, node_state)
                .is_some()
            || self
                .get_box_shadow_bottom(node_data, node_id, node_state)
                .is_some()
    }

    pub fn get_property<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {
        // Thread-local counter of cascade walks, broken down by
        // property type. Drain with `drain_css_prop_counts` (free
        // fn below) when `AZ_PROP_COUNT=1` is set to see which
        // properties dominate the cold layout path.
        //
        // Env check is read ONCE at process start and cached in a
        // `OnceLock<bool>`. Before this, the env check ran per
        // `get_property` call — and the function fires 710k+ times
        // per cold layout on excel.html. `std::env::var_os` takes
        // ~100 ns per call on macOS (env lock + hashmap lookup), so
        // the naive check added ~70 ms of pure noise to every
        // single layout, regardless of whether the env var was set.
        // Using a one-time cached bool removes that overhead.
        //
        // `no_std` builds have no thread-locals / env, so the profiling
        // counter is compiled out entirely.
        #[cfg(feature = "std")]
        {
            static PROP_COUNT_ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
            let enabled = *PROP_COUNT_ENABLED.get_or_init(crate::profile::cascade_enabled);
            if enabled {
                // `try_with` (not `with`): the lifted-to-wasm web backend has no
                // real TLS, so `with` would hit `panic_access_error` (the layout
                // path reads CSS props via these getters → would trap). `try_with`
                // returns Err and we skip the profiling-only increment (and its
                // inner Mutex-guarded label table). Desktop behaviour unchanged —
                // when the env var is unset the whole block is gated off anyway.
                let _ = PROP_COUNTS.try_with(|c| {
                    *c.borrow_mut()
                        .entry(Self::css_prop_type_label(css_property_type))
                        .or_insert(0) += 1;
                });
            }
        }

        // Always use full cascade resolution.
        // Tier 1/2/2b handle layout-hot properties via direct typed getters.
        // This path is only used for paint-time reads (background, shadow, etc.)
        self.get_property_slow(node_data, node_id, node_state, css_property_type)
    }

    #[cfg(feature = "std")]
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    fn css_prop_type_label(t: &CssPropertyType) -> &'static str {
        // Intern Debug-format labels under a mutex-guarded map so
        // we leak at most one `&'static str` per distinct
        // `CssPropertyType` variant (bounded at ≤ 178 total). Only
        // triggered when `AZ_PROP_COUNT=1`, so zero cost normally.
        use std::sync::{Mutex, OnceLock};
        static TABLE: OnceLock<Mutex<std::collections::HashMap<CssPropertyType, &'static str>>> =
            OnceLock::new();
        let m = TABLE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
        let mut g = m.lock().expect("AZ_PROP_COUNT label table poisoned");
        if let Some(s) = g.get(t) {
            return s;
        }
        let s: String = std::format!("{t:?}");
        let leaked: &'static str = std::boxed::Box::leak(s.into_boxed_str());
        g.insert(*t, leaked);
        leaked
    }

    /// Full cascade resolution for any CSS property type.
    /// Walks all cascade layers: user overrides → inline → stylesheet → cascaded → computed → UA.
    /// Also used by restyle functions that need state-aware lookups.
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    pub(crate) fn get_property_slow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {

        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

        // Helper: do these conditions identify a rule that applies in `state`?
        // Empty conditions = Normal-only. Otherwise all conditions must be
        // PseudoState(state).
        fn matches_pseudo_state(
            conds: &azul_css::dynamic_selector::DynamicSelectorVec,
            state: PseudoStateType,
        ) -> bool {
            let conditions = conds.as_slice();
            if conditions.is_empty() {
                state == PseudoStateType::Normal
            } else {
                conditions
                    .iter()
                    .all(|c| matches!(c, DynamicSelector::PseudoState(s) if *s == state))
            }
        }

        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                return Some(&v[idx].1);
            }
        }

        // If that fails, see if there is an inline CSS property that matches
        // :focus > :active > :hover > normal (fallback)
        if node_state.focused {
            // PRIORITY 1: Inline CSS properties (highest priority per CSS spec)
            if let Some(p) = node_data.style.iter_inline_properties().find_map(|(prop, conds)| {
                if matches_pseudo_state(conds,PseudoStateType::Focus)
                    && prop.get_type() == *css_property_type
                {
                    Some(prop)
                } else {
                    None
                }
            }) {
                return Some(p);
            }

            // PRIORITY 2: CSS stylesheet properties
            if let Some(p) = Self::find_in_stateful(
                self.css_props.get_slice(node_id.index()),
                PseudoStateType::Focus,
                css_property_type,
            ) {
                return Some(p);
            }

            // PRIORITY 3: Cascaded/inherited properties
            if let Some(p) = Self::find_in_stateful(
                self.cascaded_props.get_slice(node_id.index()),
                PseudoStateType::Focus,
                css_property_type,
            ) {
                return Some(p);
            }
        }

        if node_state.active {
            // PRIORITY 1: Inline CSS properties (highest priority per CSS spec)
            if let Some(p) = node_data.style.iter_inline_properties().find_map(|(prop, conds)| {
                if matches_pseudo_state(conds,PseudoStateType::Active)
                    && prop.get_type() == *css_property_type
                {
                    Some(prop)
                } else {
                    None
                }
            }) {
                return Some(p);
            }

            // PRIORITY 2: CSS stylesheet properties
            if let Some(p) = Self::find_in_stateful(
                self.css_props.get_slice(node_id.index()),
                PseudoStateType::Active,
                css_property_type,
            ) {
                return Some(p);
            }

            // PRIORITY 3: Cascaded/inherited properties
            if let Some(p) = Self::find_in_stateful(
                self.cascaded_props.get_slice(node_id.index()),
                PseudoStateType::Active,
                css_property_type,
            ) {
                return Some(p);
            }
        }

        // :dragging pseudo-state (higher priority than :hover)
        if node_state.dragging {
            if let Some(p) = node_data.style.iter_inline_properties().find_map(|(prop, conds)| {
                if matches_pseudo_state(conds,PseudoStateType::Dragging)
                    && prop.get_type() == *css_property_type
                {
                    Some(prop)
                } else {
                    None
                }
            }) {
                return Some(p);
            }

            if let Some(p) = Self::find_in_stateful(
                self.css_props.get_slice(node_id.index()),
                PseudoStateType::Dragging,
                css_property_type,
            ) {
                return Some(p);
            }

            if let Some(p) = Self::find_in_stateful(
                self.cascaded_props.get_slice(node_id.index()),
                PseudoStateType::Dragging,
                css_property_type,
            ) {
                return Some(p);
            }
        }

        // :drag-over pseudo-state (higher priority than :hover)
        if node_state.drag_over {
            if let Some(p) = node_data.style.iter_inline_properties().find_map(|(prop, conds)| {
                if matches_pseudo_state(conds,PseudoStateType::DragOver)
                    && prop.get_type() == *css_property_type
                {
                    Some(prop)
                } else {
                    None
                }
            }) {
                return Some(p);
            }

            if let Some(p) = Self::find_in_stateful(
                self.css_props.get_slice(node_id.index()),
                PseudoStateType::DragOver,
                css_property_type,
            ) {
                return Some(p);
            }

            if let Some(p) = Self::find_in_stateful(
                self.cascaded_props.get_slice(node_id.index()),
                PseudoStateType::DragOver,
                css_property_type,
            ) {
                return Some(p);
            }
        }

        if node_state.hover {
            // PRIORITY 1: Inline CSS properties (highest priority per CSS spec)
            if let Some(p) = node_data.style.iter_inline_properties().find_map(|(prop, conds)| {
                if matches_pseudo_state(conds,PseudoStateType::Hover)
                    && prop.get_type() == *css_property_type
                {
                    Some(prop)
                } else {
                    None
                }
            }) {
                return Some(p);
            }

            // PRIORITY 2: CSS stylesheet properties
            if let Some(p) = Self::find_in_stateful(
                self.css_props.get_slice(node_id.index()),
                PseudoStateType::Hover,
                css_property_type,
            ) {
                return Some(p);
            }

            // PRIORITY 3: Cascaded/inherited properties
            if let Some(p) = Self::find_in_stateful(
                self.cascaded_props.get_slice(node_id.index()),
                PseudoStateType::Hover,
                css_property_type,
            ) {
                return Some(p);
            }
        }

        // Normal/fallback properties - always apply as base layer
        // PRIORITY 1: Inline CSS properties (highest priority per CSS spec)
        if let Some(p) = node_data.style.iter_inline_properties().find_map(|(prop, conds)| {
            if matches_pseudo_state(conds, PseudoStateType::Normal)
                && prop.get_type() == *css_property_type
            {
                Some(prop)
            } else {
                None
            }
        }) {
            return Some(p);
        }

        // PRIORITY 2: CSS stylesheet properties
        if let Some(p) = Self::find_in_stateful(
            self.css_props.get_slice(node_id.index()),
            PseudoStateType::Normal,
            css_property_type,
        ) {
            return Some(p);
        }

        // PRIORITY 2b: Global `*` selector properties (specificity 0,0,0)
        // These are collected once during restyle and apply to all nodes.
        // Lower priority than per-node rules but higher than inheritance/UA.
        if let Some(p) = self.global_css_props.iter().find(|p| p.get_type() == *css_property_type) {
            return Some(p);
        }

        // PRIORITY 3: Cascaded/inherited properties
        if let Some(p) = Self::find_in_stateful(
            self.cascaded_props.get_slice(node_id.index()),
            PseudoStateType::Normal,
            css_property_type,
        ) {
            return Some(p);
        }

        // Check computed values cache for inherited properties
        // Sorted Vec with binary search
        if css_property_type.is_inheritable() {
            if let Some(vec) = self.computed_values.get(node_id.index()) {
                if let Ok(idx) = vec.binary_search_by_key(css_property_type, |(k, _)| *k) {
                    return Some(&vec[idx].1.property);
                }
            }
        }

        // User-agent stylesheet fallback (lowest precedence)
        // Check if the node type has a default value for this property
        crate::ua_css::get_ua_property(&node_data.node_type, *css_property_type)
    }

    /// Get a CSS property using `DynamicSelectorContext` for evaluation.
    ///
    /// This is the new API that supports @media queries, @container queries,
    /// OS-specific styles, and all pseudo-states via `CssPropertyWithConditions`.
    ///
    /// The evaluation follows "last wins" semantics - properties are evaluated
    /// in reverse order and the first matching property wins.
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    pub(crate) fn get_property_with_context<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        context: &DynamicSelectorContext,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {
        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                return Some(&v[idx].1);
            }
        }

        // Check inline CSS properties with DynamicSelectorContext evaluation.
        // Iterate in REVERSE order across the flat (prop, conds) view —
        // "last found wins" semantics, replacing the old Focus > Active >
        // Hover > Normal priority chain.
        let inline_props_rev: Vec<_> = node_data
            .style
            .iter_inline_properties()
            .collect::<Vec<_>>();
        if let Some(prop) = inline_props_rev.into_iter().rev().find_map(|(prop, conds)| {
            let conditions_match = conds.as_slice().iter().all(|c| c.matches(context));
            if prop.get_type() == *css_property_type && conditions_match {
                Some(prop)
            } else {
                None
            }
        }) {
            return Some(prop);
        }

        // Fall back to CSS file and cascaded properties
        let legacy_state = StyledNodeState::from_pseudo_state_flags(&context.pseudo_state);
        if let Some(p) = self.get_property(node_data, node_id, &legacy_state, css_property_type) {
            return Some(p);
        }

        None
    }

    /// Check if any properties with conditions would change between two contexts.
    /// This is used for re-layout detection on viewport/container resize.
    pub(crate) fn check_properties_changed(
        node_data: &NodeData,
        old_context: &DynamicSelectorContext,
        new_context: &DynamicSelectorContext,
    ) -> bool {
        for (_prop, conds) in node_data.style.iter_inline_properties() {
            let was_active = conds.as_slice().iter().all(|c| c.matches(old_context));
            let is_active = conds.as_slice().iter().all(|c| c.matches(new_context));
            if was_active != is_active {
                return true;
            }
        }
        false
    }

    /// Check if any layout-affecting properties would change between two contexts.
    /// This is a more targeted check for re-layout detection.
    pub(crate) fn check_layout_properties_changed(
        node_data: &NodeData,
        old_context: &DynamicSelectorContext,
        new_context: &DynamicSelectorContext,
    ) -> bool {
        for (prop, conds) in node_data.style.iter_inline_properties() {
            // Skip non-layout-affecting properties
            if !prop.get_type().can_trigger_relayout() {
                continue;
            }

            let was_active = conds.as_slice().iter().all(|c| c.matches(old_context));
            let is_active = conds.as_slice().iter().all(|c| c.matches(new_context));
            if was_active != is_active {
                return true;
            }
        }
        false
    }

    impl_get_prop!(get_background_content, StyleBackgroundContentVecValue, BackgroundContent, as_background_content);

    impl_get_prop!(get_hyphens, StyleHyphensValue, Hyphens, as_hyphens);

    impl_get_prop!(get_word_break, StyleWordBreakValue, WordBreak, as_word_break);

    impl_get_prop!(get_overflow_wrap, StyleOverflowWrapValue, OverflowWrap, as_overflow_wrap);

    impl_get_prop!(get_line_break, StyleLineBreakValue, LineBreak, as_line_break);

    impl_get_prop!(get_text_align_last, StyleTextAlignLastValue, TextAlignLast, as_text_align_last);

    impl_get_prop!(get_object_fit, StyleObjectFitValue, ObjectFit, as_object_fit);

    impl_get_prop!(get_text_orientation, StyleTextOrientationValue, TextOrientation, as_text_orientation);

    impl_get_prop!(get_object_position, StyleObjectPositionValue, ObjectPosition, as_object_position);

    impl_get_prop!(get_aspect_ratio, StyleAspectRatioValue, AspectRatio, as_aspect_ratio);

    impl_get_prop!(get_direction, StyleDirectionValue, Direction, as_direction);

    impl_get_prop!(get_unicode_bidi, StyleUnicodeBidiValue, UnicodeBidi, as_unicode_bidi);

    impl_get_prop!(get_text_box_trim, StyleTextBoxTrimValue, TextBoxTrim, as_text_box_trim);

    impl_get_prop!(get_text_box_edge, StyleTextBoxEdgeValue, TextBoxEdge, as_text_box_edge);

    impl_get_prop!(get_dominant_baseline, StyleDominantBaselineValue, DominantBaseline, as_dominant_baseline);

    impl_get_prop!(get_alignment_baseline, StyleAlignmentBaselineValue, AlignmentBaseline, as_alignment_baseline);

    impl_get_prop!(get_initial_letter_align, StyleInitialLetterAlignValue, InitialLetterAlign, as_initial_letter_align);

    impl_get_prop!(get_initial_letter_wrap, StyleInitialLetterWrapValue, InitialLetterWrap, as_initial_letter_wrap);

    impl_get_prop!(get_scrollbar_gutter, StyleScrollbarGutterValue, ScrollbarGutter, as_scrollbar_gutter);

    impl_get_prop!(get_overflow_clip_margin, StyleOverflowClipMarginValue, OverflowClipMargin, as_overflow_clip_margin);

    impl_get_prop!(get_clip, StyleClipRectValue, Clip, as_clip);

    impl_get_prop!(get_white_space, StyleWhiteSpaceValue, WhiteSpace, as_white_space);
    impl_get_prop!(get_background_position, StyleBackgroundPositionVecValue, BackgroundPosition, as_background_position);
    impl_get_prop!(get_background_size, StyleBackgroundSizeVecValue, BackgroundSize, as_background_size);
    impl_get_prop!(get_background_repeat, StyleBackgroundRepeatVecValue, BackgroundRepeat, as_background_repeat);
    impl_get_prop!(get_font_size, StyleFontSizeValue, FontSize, as_font_size);
    impl_get_prop!(get_font_family, StyleFontFamilyVecValue, FontFamily, as_font_family);
    impl_get_prop!(get_font_weight, StyleFontWeightValue, FontWeight, as_font_weight);
    impl_get_prop!(get_font_style, StyleFontStyleValue, FontStyle, as_font_style);
    impl_get_prop!(get_text_color, StyleTextColorValue, TextColor, as_text_color);
    impl_get_prop!(get_text_indent, StyleTextIndentValue, TextIndent, as_text_indent);
    impl_get_prop!(get_initial_letter, StyleInitialLetterValue, InitialLetter, as_initial_letter);
    impl_get_prop!(get_line_clamp, StyleLineClampValue, LineClamp, as_line_clamp);
    impl_get_prop!(get_hanging_punctuation, StyleHangingPunctuationValue, HangingPunctuation, as_hanging_punctuation);
    impl_get_prop!(get_text_combine_upright, StyleTextCombineUprightValue, TextCombineUpright, as_text_combine_upright);
    impl_get_prop!(get_exclusion_margin, StyleExclusionMarginValue, ExclusionMargin, as_exclusion_margin);
    impl_get_prop!(get_hyphenation_language, StyleHyphenationLanguageValue, HyphenationLanguage, as_hyphenation_language);
    impl_get_prop!(get_caret_color, CaretColorValue, CaretColor, as_caret_color);

    impl_get_prop!(get_caret_width, CaretWidthValue, CaretWidth, as_caret_width);

    impl_get_prop!(get_caret_animation_duration, CaretAnimationDurationValue, CaretAnimationDuration, as_caret_animation_duration);

    impl_get_prop!(get_selection_background_color, SelectionBackgroundColorValue, SelectionBackgroundColor, as_selection_background_color);

    impl_get_prop!(get_selection_color, SelectionColorValue, SelectionColor, as_selection_color);

    impl_get_prop!(get_selection_radius, SelectionRadiusValue, SelectionRadius, as_selection_radius);

    impl_get_prop!(get_text_justify, LayoutTextJustifyValue, TextJustify, as_text_justify);

    impl_get_prop!(get_z_index, LayoutZIndexValue, ZIndex, as_z_index);

    impl_get_prop!(get_flex_basis, LayoutFlexBasisValue, FlexBasis, as_flex_basis);

    impl_get_prop!(get_column_gap, LayoutColumnGapValue, ColumnGap, as_column_gap);

    impl_get_prop!(get_row_gap, LayoutRowGapValue, RowGap, as_row_gap);

    impl_get_prop!(get_grid_template_columns, LayoutGridTemplateColumnsValue, GridTemplateColumns, as_grid_template_columns);

    impl_get_prop!(get_grid_template_rows, LayoutGridTemplateRowsValue, GridTemplateRows, as_grid_template_rows);

    impl_get_prop!(get_grid_auto_columns, LayoutGridAutoColumnsValue, GridAutoColumns, as_grid_auto_columns);

    impl_get_prop!(get_grid_auto_rows, LayoutGridAutoRowsValue, GridAutoRows, as_grid_auto_rows);

    impl_get_prop!(get_grid_column, LayoutGridColumnValue, GridColumn, as_grid_column);

    impl_get_prop!(get_grid_row, LayoutGridRowValue, GridRow, as_grid_row);

    impl_get_prop!(get_grid_auto_flow, LayoutGridAutoFlowValue, GridAutoFlow, as_grid_auto_flow);

    impl_get_prop!(get_justify_self, LayoutJustifySelfValue, JustifySelf, as_justify_self);

    impl_get_prop!(get_justify_items, LayoutJustifyItemsValue, JustifyItems, as_justify_items);

    impl_get_prop!(get_gap, LayoutGapValue, Gap, as_gap);

    /// Method for getting grid-gap property
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    pub(crate) fn get_grid_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridGap)
            .and_then(|p| p.as_grid_gap())
    }

    impl_get_prop!(get_align_self, LayoutAlignSelfValue, AlignSelf, as_align_self);

    impl_get_prop!(get_font, StyleFontValue, Font, as_font);

    impl_get_prop!(get_writing_mode, LayoutWritingModeValue, WritingMode, as_writing_mode);

    impl_get_prop!(get_clear, LayoutClearValue, Clear, as_clear);

    impl_get_prop!(get_shape_outside, ShapeOutsideValue, ShapeOutside, as_shape_outside);

    impl_get_prop!(get_shape_inside, ShapeInsideValue, ShapeInside, as_shape_inside);

    impl_get_prop!(get_clip_path, ClipPathValue, ClipPath, as_clip_path);

    /// Method for getting scrollbar track background
    pub fn get_scrollbar_track<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ScrollbarTrack)
            .and_then(|p| p.as_scrollbar_track())
    }

    /// Method for getting scrollbar thumb background
    pub fn get_scrollbar_thumb<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ScrollbarThumb)
            .and_then(|p| p.as_scrollbar_thumb())
    }

    /// Method for getting scrollbar button background
    pub fn get_scrollbar_button<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ScrollbarButton)
            .and_then(|p| p.as_scrollbar_button())
    }

    /// Method for getting scrollbar corner background
    pub fn get_scrollbar_corner<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ScrollbarCorner)
            .and_then(|p| p.as_scrollbar_corner())
    }

    /// Method for getting scrollbar resizer background
    pub fn get_scrollbar_resizer<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ScrollbarResizer)
            .and_then(|p| p.as_scrollbar_resizer())
    }

    impl_get_prop!(get_scrollbar_width, LayoutScrollbarWidthValue, ScrollbarWidth, as_scrollbar_width);

    impl_get_prop!(get_scrollbar_color, StyleScrollbarColorValue, ScrollbarColor, as_scrollbar_color);

    impl_get_prop!(get_scrollbar_visibility, ScrollbarVisibilityModeValue, ScrollbarVisibility, as_scrollbar_visibility);

    impl_get_prop!(get_scrollbar_fade_delay, ScrollbarFadeDelayValue, ScrollbarFadeDelay, as_scrollbar_fade_delay);

    impl_get_prop!(get_scrollbar_fade_duration, ScrollbarFadeDurationValue, ScrollbarFadeDuration, as_scrollbar_fade_duration);

    impl_get_prop!(get_visibility, StyleVisibilityValue, Visibility, as_visibility);

    impl_get_prop!(get_break_before, PageBreakValue, BreakBefore, as_break_before);

    impl_get_prop!(get_break_after, PageBreakValue, BreakAfter, as_break_after);

    impl_get_prop!(get_break_inside, BreakInsideValue, BreakInside, as_break_inside);

    impl_get_prop!(get_orphans, OrphansValue, Orphans, as_orphans);

    impl_get_prop!(get_widows, WidowsValue, Widows, as_widows);

    impl_get_prop!(get_box_decoration_break, BoxDecorationBreakValue, BoxDecorationBreak, as_box_decoration_break);

    impl_get_prop!(get_column_count, ColumnCountValue, ColumnCount, as_column_count);

    impl_get_prop!(get_column_width, ColumnWidthValue, ColumnWidth, as_column_width);

    impl_get_prop!(get_column_span, ColumnSpanValue, ColumnSpan, as_column_span);

    impl_get_prop!(get_column_fill, ColumnFillValue, ColumnFill, as_column_fill);

    impl_get_prop!(get_column_rule_width, ColumnRuleWidthValue, ColumnRuleWidth, as_column_rule_width);

    impl_get_prop!(get_column_rule_style, ColumnRuleStyleValue, ColumnRuleStyle, as_column_rule_style);

    impl_get_prop!(get_column_rule_color, ColumnRuleColorValue, ColumnRuleColor, as_column_rule_color);

    impl_get_prop!(get_flow_into, FlowIntoValue, FlowInto, as_flow_into);

    impl_get_prop!(get_flow_from, FlowFromValue, FlowFrom, as_flow_from);

    impl_get_prop!(get_shape_margin, ShapeMarginValue, ShapeMargin, as_shape_margin);

    impl_get_prop!(get_shape_image_threshold, ShapeImageThresholdValue, ShapeImageThreshold, as_shape_image_threshold);

    impl_get_prop!(get_content, ContentValue, Content, as_content);

    impl_get_prop!(get_counter_reset, CounterResetValue, CounterReset, as_counter_reset);

    impl_get_prop!(get_counter_increment, CounterIncrementValue, CounterIncrement, as_counter_increment);

    impl_get_prop!(get_string_set, StringSetValue, StringSet, as_string_set);
    impl_get_prop!(get_text_align, StyleTextAlignValue, TextAlign, as_text_align);
    impl_get_prop!(get_user_select, StyleUserSelectValue, UserSelect, as_user_select);
    impl_get_prop!(get_text_decoration, StyleTextDecorationValue, TextDecoration, as_text_decoration);
    impl_get_prop!(get_vertical_align, StyleVerticalAlignValue, VerticalAlign, as_vertical_align);
    impl_get_prop!(get_line_height, StyleLineHeightValue, LineHeight, as_line_height);
    impl_get_prop!(get_letter_spacing, StyleLetterSpacingValue, LetterSpacing, as_letter_spacing);
    impl_get_prop!(get_word_spacing, StyleWordSpacingValue, WordSpacing, as_word_spacing);
    impl_get_prop!(get_tab_size, StyleTabSizeValue, TabSize, as_tab_size);
    impl_get_prop!(get_cursor, StyleCursorValue, Cursor, as_cursor);
    impl_get_prop!(get_box_shadow_left, StyleBoxShadowValue, BoxShadowLeft, as_box_shadow_left);
    impl_get_prop!(get_box_shadow_right, StyleBoxShadowValue, BoxShadowRight, as_box_shadow_right);
    impl_get_prop!(get_box_shadow_top, StyleBoxShadowValue, BoxShadowTop, as_box_shadow_top);
    impl_get_prop!(get_box_shadow_bottom, StyleBoxShadowValue, BoxShadowBottom, as_box_shadow_bottom);
    impl_get_prop!(get_border_top_color, StyleBorderTopColorValue, BorderTopColor, as_border_top_color);
    impl_get_prop!(get_border_left_color, StyleBorderLeftColorValue, BorderLeftColor, as_border_left_color);
    impl_get_prop!(get_border_right_color, StyleBorderRightColorValue, BorderRightColor, as_border_right_color);
    impl_get_prop!(get_border_bottom_color, StyleBorderBottomColorValue, BorderBottomColor, as_border_bottom_color);
    impl_get_prop!(get_border_top_style, StyleBorderTopStyleValue, BorderTopStyle, as_border_top_style);
    impl_get_prop!(get_border_left_style, StyleBorderLeftStyleValue, BorderLeftStyle, as_border_left_style);
    impl_get_prop!(get_border_right_style, StyleBorderRightStyleValue, BorderRightStyle, as_border_right_style);
    impl_get_prop!(get_border_bottom_style, StyleBorderBottomStyleValue, BorderBottomStyle, as_border_bottom_style);
    impl_get_prop!(get_border_top_left_radius, StyleBorderTopLeftRadiusValue, BorderTopLeftRadius, as_border_top_left_radius);
    impl_get_prop!(get_border_top_right_radius, StyleBorderTopRightRadiusValue, BorderTopRightRadius, as_border_top_right_radius);
    impl_get_prop!(get_border_bottom_left_radius, StyleBorderBottomLeftRadiusValue, BorderBottomLeftRadius, as_border_bottom_left_radius);
    impl_get_prop!(get_border_bottom_right_radius, StyleBorderBottomRightRadiusValue, BorderBottomRightRadius, as_border_bottom_right_radius);
    impl_get_prop!(get_opacity, StyleOpacityValue, Opacity, as_opacity);
    impl_get_prop!(get_transform, StyleTransformVecValue, Transform, as_transform);
    impl_get_prop!(get_transform_origin, StyleTransformOriginValue, TransformOrigin, as_transform_origin);
    impl_get_prop!(get_perspective_origin, StylePerspectiveOriginValue, PerspectiveOrigin, as_perspective_origin);
    impl_get_prop!(get_backface_visibility, StyleBackfaceVisibilityValue, BackfaceVisibility, as_backface_visibility);
    impl_get_prop!(get_display, LayoutDisplayValue, Display, as_display);
    impl_get_prop!(get_float, LayoutFloatValue, Float, as_float);
    impl_get_prop!(get_box_sizing, LayoutBoxSizingValue, BoxSizing, as_box_sizing);
    impl_get_prop!(get_width, LayoutWidthValue, Width, as_width);
    impl_get_prop!(get_height, LayoutHeightValue, Height, as_height);
    impl_get_prop!(get_min_width, LayoutMinWidthValue, MinWidth, as_min_width);
    impl_get_prop!(get_min_height, LayoutMinHeightValue, MinHeight, as_min_height);
    impl_get_prop!(get_max_width, LayoutMaxWidthValue, MaxWidth, as_max_width);
    impl_get_prop!(get_max_height, LayoutMaxHeightValue, MaxHeight, as_max_height);
    impl_get_prop!(get_position, LayoutPositionValue, Position, as_position);
    impl_get_prop!(get_top, LayoutTopValue, Top, as_top);
    impl_get_prop!(get_bottom, LayoutInsetBottomValue, Bottom, as_bottom);
    impl_get_prop!(get_right, LayoutRightValue, Right, as_right);
    impl_get_prop!(get_left, LayoutLeftValue, Left, as_left);
    impl_get_prop!(get_padding_top, LayoutPaddingTopValue, PaddingTop, as_padding_top);
    impl_get_prop!(get_padding_bottom, LayoutPaddingBottomValue, PaddingBottom, as_padding_bottom);
    impl_get_prop!(get_padding_left, LayoutPaddingLeftValue, PaddingLeft, as_padding_left);
    impl_get_prop!(get_padding_right, LayoutPaddingRightValue, PaddingRight, as_padding_right);
    impl_get_prop!(get_margin_top, LayoutMarginTopValue, MarginTop, as_margin_top);
    impl_get_prop!(get_margin_bottom, LayoutMarginBottomValue, MarginBottom, as_margin_bottom);
    impl_get_prop!(get_margin_left, LayoutMarginLeftValue, MarginLeft, as_margin_left);
    impl_get_prop!(get_margin_right, LayoutMarginRightValue, MarginRight, as_margin_right);
    impl_get_prop!(get_border_top_width, LayoutBorderTopWidthValue, BorderTopWidth, as_border_top_width);
    impl_get_prop!(get_border_left_width, LayoutBorderLeftWidthValue, BorderLeftWidth, as_border_left_width);
    impl_get_prop!(get_border_right_width, LayoutBorderRightWidthValue, BorderRightWidth, as_border_right_width);
    impl_get_prop!(get_border_bottom_width, LayoutBorderBottomWidthValue, BorderBottomWidth, as_border_bottom_width);
    impl_get_prop!(get_overflow_x, LayoutOverflowValue, OverflowX, as_overflow_x);
    impl_get_prop!(get_overflow_y, LayoutOverflowValue, OverflowY, as_overflow_y);
    impl_get_prop!(get_overflow_block, LayoutOverflowValue, OverflowBlock, as_overflow_block);
    impl_get_prop!(get_overflow_inline, LayoutOverflowValue, OverflowInline, as_overflow_inline);
    impl_get_prop!(get_flex_direction, LayoutFlexDirectionValue, FlexDirection, as_flex_direction);
    impl_get_prop!(get_flex_wrap, LayoutFlexWrapValue, FlexWrap, as_flex_wrap);
    impl_get_prop!(get_flex_grow, LayoutFlexGrowValue, FlexGrow, as_flex_grow);
    impl_get_prop!(get_flex_shrink, LayoutFlexShrinkValue, FlexShrink, as_flex_shrink);
    impl_get_prop!(get_justify_content, LayoutJustifyContentValue, JustifyContent, as_justify_content);
    impl_get_prop!(get_align_items, LayoutAlignItemsValue, AlignItems, as_align_items);
    impl_get_prop!(get_align_content, LayoutAlignContentValue, AlignContent, as_align_content);
    impl_get_prop!(get_mix_blend_mode, StyleMixBlendModeValue, MixBlendMode, as_mix_blend_mode);
    impl_get_prop!(get_filter, StyleFilterVecValue, Filter, as_filter);
    impl_get_prop!(get_backdrop_filter, StyleFilterVecValue, BackdropFilter, as_backdrop_filter);
    impl_get_prop!(get_text_shadow, StyleBoxShadowValue, TextShadow, as_text_shadow);
    impl_get_prop!(get_list_style_type, StyleListStyleTypeValue, ListStyleType, as_list_style_type);
    impl_get_prop!(get_list_style_position, StyleListStylePositionValue, ListStylePosition, as_list_style_position);
    impl_get_prop!(get_table_layout, LayoutTableLayoutValue, TableLayout, as_table_layout);
    impl_get_prop!(get_border_collapse, StyleBorderCollapseValue, BorderCollapse, as_border_collapse);
    impl_get_prop!(get_border_spacing, LayoutBorderSpacingValue, BorderSpacing, as_border_spacing);
    impl_get_prop!(get_caption_side, StyleCaptionSideValue, CaptionSide, as_caption_side);
    impl_get_prop!(get_empty_cells, StyleEmptyCellsValue, EmptyCells, as_empty_cells);

    // Width calculation methods
    pub fn calc_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_width(node_data, node_id, styled_node_state)
            .and_then(|w| match w.get_property()? {
                LayoutWidth::Px(px) => Some(px.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                )),
                _ => Some(0.0), // min-content/max-content not resolved here
            })
            .unwrap_or(0.0)
    }

    pub fn calc_min_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_min_width(node_data, node_id, styled_node_state)
            .and_then(|w| {
                Some(w.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_max_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_max_width(node_data, node_id, styled_node_state)
            .and_then(|w| {
                Some(w.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    // Height calculation methods
    pub fn calc_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_height(node_data, node_id, styled_node_state)
            .and_then(|h| match h.get_property()? {
                LayoutHeight::Px(px) => Some(px.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                )),
                _ => Some(0.0), // min-content/max-content not resolved here
            })
            .unwrap_or(0.0)
    }

    pub fn calc_min_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_min_height(node_data, node_id, styled_node_state)
            .and_then(|h| {
                Some(h.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_max_height(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_max_height(node_data, node_id, styled_node_state)
            .and_then(|h| {
                Some(h.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    // Position calculation methods
    pub fn calc_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_left(node_data, node_id, styled_node_state)
            .and_then(|l| {
                Some(l.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    pub fn calc_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> Option<f32> {
        self.get_right(node_data, node_id, styled_node_state)
            .and_then(|r| {
                Some(r.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    pub fn calc_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_top(node_data, node_id, styled_node_state)
            .and_then(|t| {
                Some(t.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    pub fn calc_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> Option<f32> {
        self.get_bottom(node_data, node_id, styled_node_state)
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
    }

    // Border calculation methods
    pub fn calc_border_left_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_border_left_width(node_data, node_id, styled_node_state)
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_border_right_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_border_right_width(node_data, node_id, styled_node_state)
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_border_top_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_border_top_width(node_data, node_id, styled_node_state)
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_border_bottom_width(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_border_bottom_width(node_data, node_id, styled_node_state)
            .and_then(|b| {
                Some(b.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    // Padding calculation methods
    pub fn calc_padding_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_padding_left(node_data, node_id, styled_node_state)
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_padding_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_padding_right(node_data, node_id, styled_node_state)
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_padding_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_padding_top(node_data, node_id, styled_node_state)
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_padding_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_padding_bottom(node_data, node_id, styled_node_state)
            .and_then(|p| {
                Some(p.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    // Margin calculation methods
    pub fn calc_margin_left(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_margin_left(node_data, node_id, styled_node_state)
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_margin_right(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_width: f32,
    ) -> f32 {
        self.get_margin_right(node_data, node_id, styled_node_state)
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_width,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_margin_top(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_margin_top(node_data, node_id, styled_node_state)
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    pub fn calc_margin_bottom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        styled_node_state: &StyledNodeState,
        reference_height: f32,
    ) -> f32 {
        self.get_margin_bottom(node_data, node_id, styled_node_state)
            .and_then(|m| {
                Some(m.get_property()?.inner.to_pixels_internal(
                    reference_height,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ))
            })
            .unwrap_or(0.0)
    }

    fn resolve_property_dependency(
        target_property: &CssProperty,
        reference_property: &CssProperty,
    ) -> Option<CssProperty> {
        use azul_css::{
            css::CssPropertyValue,
            props::{
                basic::{font::StyleFontSize, length::SizeMetric, pixel::PixelValue},
                layout::*,
                style::{SelectionRadius, StyleLetterSpacing, StyleWordSpacing},
            },
        };

        // Extract PixelValue from various property types (returns owned value)
        let get_pixel_value = |prop: &CssProperty| -> Option<PixelValue> {
            match prop {
                CssProperty::FontSize(val) => val.get_property().map(|v| v.inner),
                CssProperty::LetterSpacing(val) => val.get_property().map(|v| v.inner),
                CssProperty::WordSpacing(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingLeft(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingRight(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingTop(val) => val.get_property().map(|v| v.inner),
                CssProperty::PaddingBottom(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginLeft(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginRight(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginTop(val) => val.get_property().map(|v| v.inner),
                CssProperty::MarginBottom(val) => val.get_property().map(|v| v.inner),
                CssProperty::MinWidth(val) => val.get_property().map(|v| v.inner),
                CssProperty::MinHeight(val) => val.get_property().map(|v| v.inner),
                CssProperty::MaxWidth(val) => val.get_property().map(|v| v.inner),
                CssProperty::MaxHeight(val) => val.get_property().map(|v| v.inner),
                CssProperty::SelectionRadius(val) => val.get_property().map(|v| v.inner),
                _ => None,
            }
        };

        let target_pixel_value = get_pixel_value(target_property)?;
        let reference_pixel_value = get_pixel_value(reference_property)?;

        // Convert reference to absolute pixels first
        let reference_px = match reference_pixel_value.metric {
            SizeMetric::Px => reference_pixel_value.number.get(),
            SizeMetric::Pt => reference_pixel_value.number.get() * PT_TO_PX,
            SizeMetric::In => reference_pixel_value.number.get() * IN_TO_PX,
            SizeMetric::Cm => reference_pixel_value.number.get() * CM_TO_PX,
            SizeMetric::Mm => reference_pixel_value.number.get() * MM_TO_PX,
            // Reference can't be relative (em/rem/%) or viewport-relative.
            SizeMetric::Em
            | SizeMetric::Rem
            | SizeMetric::Percent
            | SizeMetric::Vw
            | SizeMetric::Vh
            | SizeMetric::Vmin
            | SizeMetric::Vmax => return None,
        };

        // Resolve target based on reference
        let resolved_px = match target_pixel_value.metric {
            SizeMetric::Px => target_pixel_value.number.get(),
            SizeMetric::Pt => target_pixel_value.number.get() * PT_TO_PX,
            SizeMetric::In => target_pixel_value.number.get() * IN_TO_PX,
            SizeMetric::Cm => target_pixel_value.number.get() * CM_TO_PX,
            SizeMetric::Mm => target_pixel_value.number.get() * MM_TO_PX,
            // em/rem both scale by reference (rem uses reference as root font-size).
            SizeMetric::Em | SizeMetric::Rem => target_pixel_value.number.get() * reference_px,
            SizeMetric::Percent => target_pixel_value.number.get() / 100.0 * reference_px,
            // Need viewport context
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => return None,
        };

        // Create a new property with the resolved value
        let resolved_pixel_value = PixelValue::px(resolved_px);

        match target_property {
            CssProperty::FontSize(_) => Some(CssProperty::FontSize(CssPropertyValue::Exact(
                StyleFontSize {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::LetterSpacing(_) => Some(CssProperty::LetterSpacing(
                CssPropertyValue::Exact(StyleLetterSpacing {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::WordSpacing(_) => Some(CssProperty::WordSpacing(CssPropertyValue::Exact(
                StyleWordSpacing {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::PaddingLeft(_) => Some(CssProperty::PaddingLeft(CssPropertyValue::Exact(
                LayoutPaddingLeft {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::PaddingRight(_) => Some(CssProperty::PaddingRight(
                CssPropertyValue::Exact(LayoutPaddingRight {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::PaddingTop(_) => Some(CssProperty::PaddingTop(CssPropertyValue::Exact(
                LayoutPaddingTop {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::PaddingBottom(_) => Some(CssProperty::PaddingBottom(
                CssPropertyValue::Exact(LayoutPaddingBottom {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::MarginLeft(_) => Some(CssProperty::MarginLeft(CssPropertyValue::Exact(
                LayoutMarginLeft {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MarginRight(_) => Some(CssProperty::MarginRight(CssPropertyValue::Exact(
                LayoutMarginRight {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MarginTop(_) => Some(CssProperty::MarginTop(CssPropertyValue::Exact(
                LayoutMarginTop {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MarginBottom(_) => Some(CssProperty::MarginBottom(
                CssPropertyValue::Exact(LayoutMarginBottom {
                    inner: resolved_pixel_value,
                }),
            )),
            CssProperty::MinWidth(_) => Some(CssProperty::MinWidth(CssPropertyValue::Exact(
                LayoutMinWidth {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MinHeight(_) => Some(CssProperty::MinHeight(CssPropertyValue::Exact(
                LayoutMinHeight {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MaxWidth(_) => Some(CssProperty::MaxWidth(CssPropertyValue::Exact(
                LayoutMaxWidth {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::MaxHeight(_) => Some(CssProperty::MaxHeight(CssPropertyValue::Exact(
                LayoutMaxHeight {
                    inner: resolved_pixel_value,
                },
            ))),
            CssProperty::SelectionRadius(_) => Some(CssProperty::SelectionRadius(
                CssPropertyValue::Exact(SelectionRadius {
                    inner: resolved_pixel_value,
                }),
            )),
            _ => None,
        }
    }

    /// Applies user-agent (UA) CSS properties to the cascade before inheritance.
    ///
    /// UA CSS has the lowest priority in the cascade, so it should only be applied
    /// if the node doesn't already have the property from inline styles or author CSS.
    ///
    /// This is critical for text nodes: UA CSS properties (like font-weight: bold for H1)
    /// must be in the cascade maps so they can be inherited by child text nodes.
    ///
    /// Uses a bitset per node to avoid O(n²) scanning of property vecs.
    pub fn apply_ua_css(&mut self, node_data: &[NodeData]) {
        use azul_css::props::property::CssPropertyType;
        use azul_css::dynamic_selector::PseudoStateType;

        let node_count = node_data.len();
        if node_count == 0 {
            return;
        }

        // Build a bitset per node: which CssPropertyType values are already set (Normal state).
        // CssPropertyType has ~178 variants, so we need [u128; 2] per node (256 bits).
        let mut prop_set: Vec<[u128; 2]> = vec![[0u128; 2]; node_count];

        // Mark properties from css_props (author CSS, Normal state)
        for (node_idx, props) in self.css_props.iter_node_slices() {
            for p in props {
                if p.state == PseudoStateType::Normal {
                    let d = p.prop_type as u16 as usize;
                    if d < 128 {
                        prop_set[node_idx][0] |= 1u128 << d;
                    } else {
                        prop_set[node_idx][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }

        // Mark properties from cascaded_props (Normal state)
        for (node_idx, props) in self.cascaded_props.iter_node_slices() {
            for p in props {
                if p.state == PseudoStateType::Normal {
                    let d = p.prop_type as u16 as usize;
                    if d < 128 {
                        prop_set[node_idx][0] |= 1u128 << d;
                    } else {
                        prop_set[node_idx][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }

        // Mark properties from inline CSS (NodeData.style, unconditional = Normal)
        for (node_idx, node) in node_data.iter().enumerate() {
            for (prop, conds) in node.style.iter_inline_properties() {
                let is_normal = conds.as_slice().is_empty();
                if is_normal {
                    let d = prop.get_type() as u16 as usize;
                    if d < 128 {
                        prop_set[node_idx][0] |= 1u128 << d;
                    } else {
                        prop_set[node_idx][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }

        // All UA property types that get_ua_property() may return Some for
        let property_types = [
            CssPropertyType::Display,
            CssPropertyType::Width,
            CssPropertyType::Height,
            CssPropertyType::FontSize,
            CssPropertyType::FontWeight,
            CssPropertyType::FontFamily,
            CssPropertyType::MarginTop,
            CssPropertyType::MarginBottom,
            CssPropertyType::MarginLeft,
            CssPropertyType::MarginRight,
            CssPropertyType::PaddingTop,
            CssPropertyType::PaddingBottom,
            CssPropertyType::PaddingLeft,
            CssPropertyType::PaddingRight,
            CssPropertyType::BorderTopStyle,
            CssPropertyType::BorderTopWidth,
            CssPropertyType::BorderTopColor,
            CssPropertyType::BreakInside,
            CssPropertyType::BreakAfter,
            CssPropertyType::ListStyleType,
            CssPropertyType::CounterReset,
            CssPropertyType::TextDecoration,
            CssPropertyType::TextAlign,
            CssPropertyType::VerticalAlign,
            CssPropertyType::Cursor,
        ];

        // Apply UA CSS: only insert for property types not yet set (bitset check = O(1))
        for (node_index, node) in node_data.iter().enumerate() {
            let node_type = &node.node_type;

            for prop_type in &property_types {
                // Check bitset: if already set, skip entirely
                let d = *prop_type as u16 as usize;
                let has_prop = if d < 128 {
                    (prop_set[node_index][0] & (1u128 << d)) != 0
                } else {
                    (prop_set[node_index][1] & (1u128 << (d - 128))) != 0
                };

                if has_prop {
                    continue;
                }

                // Check if UA CSS defines this property for this node type
                if let Some(ua_prop) = crate::ua_css::get_ua_property(node_type, *prop_type) {
                    self.cascaded_props.push_to(node_index, StatefulCssProperty {
                        state: PseudoStateType::Normal,
                        prop_type: *prop_type,
                        property: ua_prop.clone(),
                    });

                    // Mark as set in the bitset (prevent duplicate insertion for same node)
                    if d < 128 {
                        prop_set[node_index][0] |= 1u128 << d;
                    } else {
                        prop_set[node_index][1] |= 1u128 << (d - 128);
                    }
                }
            }
        }
    }

    /// Sort `cascaded_props` by (state, `prop_type`) and flatten into contiguous memory.
    /// Must be called after `apply_ua_css()` which adds entries to `cascaded_props`.
    pub fn sort_cascaded_props(&mut self) {
        self.cascaded_props.sort_each_and_flatten(|p| (p.state, p.prop_type));
    }

    /// Compute inherited values for all nodes in the DOM tree.
    ///
    /// Implements CSS inheritance: walk tree depth-first, apply cascade priority
    /// (inherited → cascaded → css → inline → user), create dependency chains for
    /// relative values. Call `apply_ua_css()` before this function.
    pub fn compute_inherited_values(
        &mut self,
        node_hierarchy: &[NodeHierarchyItem],
        node_data: &[NodeData],
    ) -> Vec<NodeId> {
        if self.computed_values.len() < node_hierarchy.len() {
            self.computed_values.resize(node_hierarchy.len(), Vec::new());
        }
        node_hierarchy
            .iter()
            .enumerate()
            .filter_map(|(node_index, hierarchy_item)| {
                let node_id = NodeId::new(node_index);
                let parent_id = hierarchy_item.parent_id();
                let parent_computed: Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>> =
                    parent_id.and_then(|pid| self.computed_values.get(pid.index()).cloned());

                let mut ctx = InheritanceContext {
                    node_id,
                    parent_id,
                    computed_values: Vec::new(),
                };

                // Step 1: Inherit from parent
                if let Some(ref parent_values) = parent_computed {
                    self.inherit_from_parent(&mut ctx, parent_values);
                }

                // Steps 2-5: Apply cascade in priority order
                self.apply_cascade_properties(
                    &mut ctx,
                    node_id,
                    &parent_computed,
                    node_data,
                    node_index,
                );

                // Check for changes and store
                let changed = self.store_if_changed(&ctx);
                changed.then_some(node_id)
            })
            .collect()
    }

    /// Inherit inheritable properties from parent node
    fn inherit_from_parent(
        &self,
        ctx: &mut InheritanceContext,
        parent_values: &[(CssPropertyType, CssPropertyWithOrigin)],
    ) {
        for (prop_type, prop_with_origin) in
            parent_values.iter().filter(|(pt, _)| pt.is_inheritable())
        {
            let entry = (*prop_type, CssPropertyWithOrigin {
                property: prop_with_origin.property.clone(),
                origin: CssPropertyOrigin::Inherited,
            });
            // Insert into sorted vec
            match ctx.computed_values.binary_search_by_key(prop_type, |(k, _)| *k) {
                Ok(idx) => ctx.computed_values[idx] = entry,
                Err(idx) => ctx.computed_values.insert(idx, entry),
            }
        }
    }

    /// Apply all cascade properties in priority order
    fn apply_cascade_properties(
        &self,
        ctx: &mut InheritanceContext,
        node_id: NodeId,
        parent_computed: &Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
        node_data: &[NodeData],
        node_index: usize,
    ) {
        // Step 2: Cascaded properties (UA CSS)
        {
            let cascaded_slice = self.cascaded_props.get_slice(node_id.index());
            for p in cascaded_slice {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal
                    && self.should_apply_cascaded(&ctx.computed_values, p.prop_type, &p.property) {
                        self.process_property(ctx, &p.property, parent_computed);
                    }
            }
        }

        // Step 3: CSS properties (stylesheets)
        {
            let css_slice = self.css_props.get_slice(node_id.index());
            for p in css_slice {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal {
                    self.process_property(ctx, &p.property, parent_computed);
                }
            }
        }

        // Step 4: Inline CSS properties
        for (prop, conds) in node_data[node_index].style.iter_inline_properties() {
            // Only apply unconditional (normal) properties
            if conds.as_slice().is_empty() {
                self.process_property(ctx, prop, parent_computed);
            }
        }

        // Step 5: User-overridden properties
        if let Some(user_props) = self.user_overridden_properties.get(node_id.index()) {
            for (_, prop) in user_props {
                self.process_property(ctx, prop, parent_computed);
            }
        }
    }

    /// Check if a cascaded property should be applied
    fn should_apply_cascaded(
        &self,
        computed: &[(CssPropertyType, CssPropertyWithOrigin)],
        prop_type: CssPropertyType,
        prop: &CssProperty,
    ) -> bool {
        // Skip relative font-size if we already have inherited resolved value
        if prop_type == CssPropertyType::FontSize {
            if let Ok(idx) = computed.binary_search_by_key(&prop_type, |(k, _)| *k) {
                if computed[idx].1.origin == CssPropertyOrigin::Inherited
                    && Self::has_relative_font_size_unit(prop)
                {
                    return false;
                }
            }
        }

        computed.binary_search_by_key(&prop_type, |(k, _)| *k).map_or(true, |idx| computed[idx].1.origin == CssPropertyOrigin::Inherited)
    }

    /// Process a single property: resolve and store
    fn process_property(
        &self,
        ctx: &mut InheritanceContext,
        prop: &CssProperty,
        parent_computed: &Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) {
        let prop_type = prop.get_type();

        let resolved = if prop_type == CssPropertyType::FontSize {
            self.resolve_font_size_property(prop, parent_computed)
        } else {
            self.resolve_other_property(prop, &ctx.computed_values)
        };

        let entry = (prop_type, CssPropertyWithOrigin {
            property: resolved,
            origin: CssPropertyOrigin::Own,
        });
        match ctx.computed_values.binary_search_by_key(&prop_type, |(k, _)| *k) {
            Ok(idx) => ctx.computed_values[idx] = entry,
            Err(idx) => ctx.computed_values.insert(idx, entry),
        }
    }

    /// Resolve font-size property (uses parent's font-size as reference)
    fn resolve_font_size_property(
        &self,
        prop: &CssProperty,
        parent_computed: &Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) -> CssProperty {
        let parent_font_size = parent_computed
            .as_ref()
            .and_then(|p| {
                p.binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k)
                    .ok()
                    .map(|idx| &p[idx].1)
            });

        match parent_font_size {
            Some(pfs) => Self::resolve_property_dependency(prop, &pfs.property).unwrap_or_else(
                || {
                    Self::resolve_font_size_to_pixels(
                        prop,
                        azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    )
                },
            ),
            None => Self::resolve_font_size_to_pixels(
                prop,
                azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
            ),
        }
    }

    /// Resolve other properties (uses current node's font-size as reference)
    fn resolve_other_property(
        &self,
        prop: &CssProperty,
        computed: &[(CssPropertyType, CssPropertyWithOrigin)],
    ) -> CssProperty {
        computed
            .binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k)
            .ok()
            .and_then(|idx| Self::resolve_property_dependency(prop, &computed[idx].1.property))
            .unwrap_or_else(|| prop.clone())
    }

    /// Convert font-size to absolute pixels
    fn resolve_font_size_to_pixels(prop: &CssProperty, reference_px: f32) -> CssProperty {
        use azul_css::{
            css::CssPropertyValue,
            props::basic::{font::StyleFontSize, length::SizeMetric, pixel::PixelValue},
        };

        let CssProperty::FontSize(css_val) = prop else {
            return prop.clone();
        };

        let Some(font_size) = css_val.get_property() else {
            return prop.clone();
        };

        let resolved_px = match font_size.inner.metric {
            SizeMetric::Px => font_size.inner.number.get(),
            SizeMetric::Pt => font_size.inner.number.get() * PT_TO_PX,
            SizeMetric::In => font_size.inner.number.get() * IN_TO_PX,
            SizeMetric::Cm => font_size.inner.number.get() * CM_TO_PX,
            SizeMetric::Mm => font_size.inner.number.get() * MM_TO_PX,
            SizeMetric::Em => font_size.inner.number.get() * reference_px,
            SizeMetric::Rem => {
                font_size.inner.number.get() * azul_css::props::basic::pixel::DEFAULT_FONT_SIZE
            }
            SizeMetric::Percent => font_size.inner.number.get() / 100.0 * reference_px,
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => {
                return prop.clone();
            }
        };

        CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
            inner: PixelValue::px(resolved_px),
        }))
    }

    /// Check if font-size has relative unit (em, rem, %)
    fn has_relative_font_size_unit(prop: &CssProperty) -> bool {
        use azul_css::props::basic::length::SizeMetric;

        let CssProperty::FontSize(css_val) = prop else {
            return false;
        };

        css_val
            .get_property()
            .is_some_and(|fs| {
                matches!(
                    fs.inner.metric,
                    SizeMetric::Em | SizeMetric::Rem | SizeMetric::Percent
                )
            })
    }

    /// Store computed values if changed, returns true if values were updated
    fn store_if_changed(&mut self, ctx: &InheritanceContext) -> bool {
        let values_changed = self
            .computed_values
            .get(ctx.node_id.index()) != Some(&ctx.computed_values);

        self.computed_values[ctx.node_id.index()] = ctx.computed_values.clone();

        values_changed
    }
}

/// Context for computing inherited values for a single node
struct InheritanceContext {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    computed_values: Vec<(CssPropertyType, CssPropertyWithOrigin)>,
}

impl CssPropertyCache {

    /// Clear the entire compact cache. Call after major DOM changes.
    pub(crate) fn invalidate_resolved_cache(&mut self) {
        self.compact_cache = None;
    }
}

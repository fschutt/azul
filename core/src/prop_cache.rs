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
#[derive(Debug, Clone, PartialEq)]
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
/// [`CssPropertyCache::get_property`] when `AZUL_PROP_COUNT=1` is set
/// in the environment. Returns `(property_label, count)` pairs
/// sorted by count descending. Layout-side instrumentation calls
/// this after each `layout_document` to print which properties
/// drove the most cascade walks.
#[cfg(feature = "std")]
pub fn drain_css_prop_counts() -> Vec<(&'static str, usize)> {
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
const PT_TO_PX: f32 = 1.333333;
const IN_TO_PX: f32 = 96.0;
const CM_TO_PX: f32 = 37.7952755906;
const MM_TO_PX: f32 = 3.7795275591;

/// Match on any CssProperty variant and access the inner CssPropertyValue<T>.
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
/// Replaces the per-pseudo-state BTreeMap approach: instead of 6 BTreeMaps
/// per node (Normal/Hover/Active/Focus/Dragging/DragOver), we store one Vec
/// per node and tag each property with its state. Lookups use `.iter().find()`.
#[derive(Debug, Clone, PartialEq)]
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
    pub fn heap_bytes(&self, per_element_size: usize) -> usize {
        let data_bytes = self.data.capacity() * per_element_size;
        let offsets_bytes =
            self.offsets.capacity() * core::mem::size_of::<(u32, u32)>();
        let mut build_bytes = self.build.capacity() * core::mem::size_of::<Vec<T>>();
        for v in &self.build {
            build_bytes += v.capacity() * per_element_size;
        }
        data_bytes + offsets_bytes + build_bytes
    }

    /// Create a new `FlatVecVec` with `node_count` empty slots (build phase).
    pub fn new(node_count: usize) -> Self {
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
    pub fn build_get(&self, node_index: usize) -> Option<&Vec<T>> {
        self.build.get(node_index)
    }

    /// Number of node slots.
    #[inline]
    pub fn len(&self) -> usize {
        if !self.offsets.is_empty() {
            self.offsets.len()
        } else {
            self.build.len()
        }
    }

    /// Returns true if this is in read (flattened) mode.
    #[inline]
    pub fn is_flattened(&self) -> bool {
        !self.offsets.is_empty() || self.build.is_empty()
    }

    /// Get a slice for the node at `node_index` (read phase).
    /// Returns empty slice if index is out of bounds or not yet flattened
    /// (falls back to build-phase data if not yet flattened).
    #[inline]
    pub fn get_slice(&self, node_index: usize) -> &[T] {
        if !self.offsets.is_empty() {
            // Read phase: use flat data
            if let Some(&(start, len)) = self.offsets.get(node_index) {
                let s = start as usize;
                let l = len as usize;
                &self.data[s..s + l]
            } else {
                &[]
            }
        } else {
            // Build phase fallback: use inner Vecs
            self.build.get(node_index).map(|v| v.as_slice()).unwrap_or(&[])
        }
    }

    /// Flatten: sort each inner Vec by key, deduplicate by keeping the last
    /// occurrence of each key (CSS cascade: later source order wins among
    /// equal specificity), then compact into flat storage.
    /// Drains all build-phase Vecs. After this call, only `get_slice()` works.
    pub fn sort_each_and_flatten<K: Ord + Eq>(&mut self, key_fn: impl Fn(&T) -> K) {
        let node_count = self.build.len();
        let total: usize = self.build.iter().map(|v| v.len()).sum();

        let mut flat_data = Vec::with_capacity(total);
        let mut offsets = Vec::with_capacity(node_count);

        for inner in self.build.iter_mut() {
            inner.sort_by(|a, b| key_fn(a).cmp(&key_fn(b)));

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
        let total: usize = self.build.iter().map(|v| v.len()).sum();

        let mut flat_data = Vec::with_capacity(total);
        let mut offsets = Vec::with_capacity(node_count);

        for inner in self.build.iter_mut() {
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

    /// Iterate over all nodes, yielding (node_index, &[T]) for each.
    /// Works in both build and flattened phases.
    pub(crate) fn iter_node_slices(&self) -> FlatVecVecIter<'_, T> {
        FlatVecVecIter {
            fvv: self,
            idx: 0,
            count: self.len(),
        }
    }

    /// Extend this FlatVecVec with all nodes from `other` (append for DOM merge).
    /// Both must be in build phase, or both must be flattened.
    pub fn extend_from(&mut self, other: &mut Self) {
        if !self.offsets.is_empty() && !other.offsets.is_empty() {
            // Both flattened: extend flat data with offset adjustment
            let base = self.data.len() as u32;
            self.data.extend(other.data.drain(..));
            self.offsets.extend(other.offsets.drain(..).map(|(s, l)| (s + base, l)));
        } else {
            // At least one in build phase: extend build vecs
            self.build.extend(other.build.drain(..));
            // Invalidate flat data if it existed
            self.data.clear();
            self.offsets.clear();
        }
    }
}

/// Iterator over (node_index, &[T]) pairs from a `FlatVecVec`.
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

impl<'a, T> ExactSizeIterator for FlatVecVecIter<'a, T> {}

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
    /// layout pass (see `AZUL_PROP_COUNT=1` report — 329 629
    /// cascade walks on excel.html alone). Each resolution
    /// recursively reads the parent's font-size (for `em`) plus
    /// the root's font-size (for `rem`), multiplying the walk
    /// count. Caching the pre-resolved pixel value collapses that
    /// to a single `Vec<f32>` indexed lookup.
    pub resolved_font_sizes_px: std::sync::OnceLock<Vec<f32>>,
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
    pub fn total_bytes(&self) -> usize {
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
    /// `AZUL_MEM_BREAKDOWN=1` reporter. Sums capacity × element size
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
        let stateful_sz = core::mem::size_of::<StatefulCssProperty>();
        let computed_entry_sz =
            core::mem::size_of::<(CssPropertyType, CssPropertyWithOrigin)>();
        let outer_vec_sz = core::mem::size_of::<Vec<(CssPropertyType, CssPropertyWithOrigin)>>();

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
                    * core::mem::size_of::<(CssPropertyType, CssProperty)>();
            }
            b
        };

        let global_bytes = self.global_css_props.capacity()
            * core::mem::size_of::<CssProperty>();

        let compact_bytes = self
            .compact_cache
            .as_ref()
            .map(|c| {
                c.tier1_enums.capacity() * 8
                    + c.tier2_dims.capacity() * 68
                    + c.tier2_cold.capacity() * 28
                    + c.tier2b_text.capacity() * 24
                    + c.prev_font_hashes.capacity() * 8
                    + c.font_dirty_nodes.capacity() * 8
            })
            .unwrap_or(0);

        let resolved_font_sizes_bytes = self
            .resolved_font_sizes_px
            .get()
            .map(|v| v.capacity() * core::mem::size_of::<f32>())
            .unwrap_or(0);

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
            let ssp_sz = core::mem::size_of::<StatefulCssProperty>();
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
            eprintln!("[PRUNE] css_props: norm+compact={} norm+other={} nonnorm={} SSP={}B | cascaded: total={} norm+compact={}",
                normal_compact, normal_noncompact, nonnormal, ssp_sz, casc_total, casc_normal_compact);
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
        self.css_props.retain(keep);
        if !self.cascaded_props.is_flattened() {
            self.cascaded_props.sort_each_and_flatten(|p| (p.state, p.prop_type));
        }
        self.cascaded_props.retain(keep);
    }

    /// Look up a CSS property for a specific pseudo-state in a stateful property vec.
    /// Requires the vec to be sorted by (state, prop_type).
    #[inline]
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
    /// Requires the vec to be sorted by (state, prop_type).
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
    pub(crate) fn prop_types_for_state<'a>(
        props: &'a [StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
    ) -> impl Iterator<Item = &'a CssPropertyType> + 'a {
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

impl CssPropertyCache {
    /// Match CSS selectors to nodes and populate css_props.
    /// Returns tag IDs for hit-testing. If compact_cache is available,
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
            css::{CssDeclaration, CssPathPseudoSelector::*},
            props::layout::LayoutDisplay,
        };

        let css_is_empty = css.is_empty();

        if !css_is_empty {
            css.sort_by_specificity();

            // Separate CSS rules into "global only" (just `*`) vs "has specific selector".
            // Global-only rules apply to ALL nodes — push directly into css_props
            // without per-node selector matching (avoids m×n for these rules).
            // Specific rules still go through matches_html_element per-node.
            use azul_css::css::{CssPathSelector, CssRuleBlock};

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

            use azul_css::dynamic_selector::PseudoStateType;

            // Collect global-only rule declarations ONCE (not per-node).
            // These are stored in self.global_css_props and applied during
            // build_compact_cache_with_inheritance for each node, avoiding
            // 50K × N clones into per-node css_props Vecs.
            self.global_css_props.clear();
            for rule in &global_only_rules {
                if crate::style::rule_ends_with(&rule.path, None) {
                    for d in rule.declarations.iter() {
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
        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes.iter() {
            let parent_id = match node_id.into_crate_internal() {
                Some(s) => s,
                None => continue,
            };

            use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

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
                    .map(|p| (p.get_type(), p.clone()))
                    .collect();

                // 2. Inherit CSS stylesheet properties from parent for this pseudo-state
                let parent_inheritable_css: Vec<(CssPropertyType, CssProperty)> = if !css_is_empty {
                    self.css_props.get_slice(parent_id.index())
                        .iter()
                        .filter(|p| p.state == state && p.prop_type.is_inheritable())
                        .map(|p| (p.prop_type, p.property.clone()))
                        .collect()
                } else {
                    Vec::new()
                };

                // 3. Inherit cascaded properties from parent for this pseudo-state
                let parent_inheritable_cascaded: Vec<(CssPropertyType, CssProperty)> =
                    self.cascaded_props.get_slice(parent_id.index())
                        .iter()
                        .filter(|p| p.state == state && p.prop_type.is_inheritable())
                        .map(|p| (p.prop_type, p.property.clone()))
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
    /// Can be called separately after build_compact_cache_with_inheritance.
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

                loop {
                    // display:none check — read directly from compact tier1 (fast u64 read)
                    if let Some(cc) = compact_cache.as_ref() {
                        let t1 = cc.tier1_enums[node_idx];
                        let display_val = ((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8;
                        if display_val == 4 { break; } // 4 = LayoutDisplay::None (new encoding)
                    }

                    if node_data.has_context_menu() || node_data.get_context_menu().is_some() {
                        need_tag = true; break;
                    }
                    if tab_index.is_some() { need_tag = true; break; }

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
                            need_tag = true; break;
                        }
                    }

                    // Non-window callbacks
                    let has_non_window_cb = !node_data.get_callbacks().is_empty()
                        && !node_data.get_callbacks().iter().all(|cb| cb.event.is_window_callback());
                    if has_non_window_cb { need_tag = true; break; }

                    // Cursor check — read from cached css_props or inline style.
                    if self.css_props.get_slice(node_idx).iter().any(|p|
                        p.state == azul_css::dynamic_selector::PseudoStateType::Normal
                        && p.prop_type == azul_css::props::property::CssPropertyType::Cursor
                    ) || node_data.style.iter_inline_properties().any(|(p, _)|
                        p.get_type() == azul_css::props::property::CssPropertyType::Cursor
                    ) {
                        need_tag = true; break;
                    }

                    // Overflow scroll check — read from compact tier1
                    if let Some(cc) = compact_cache.as_ref() {
                        let t1 = cc.tier1_enums[node_idx];
                        let ox = ((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8;
                        let oy = ((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8;
                        // 2 = Scroll, 3 = Auto in layout_overflow_to_u8 (new encoding)
                        if ox == 2 || ox == 3 || oy == 2 || oy == 3 {
                            need_tag = true; break;
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
                        if has_text { need_tag = true; break; }
                    }

                    break;
                }

                if !need_tag {
                    None
                } else {
                    Some(TagIdToNodeIdMapping {
                        tag_id: TagId::from_crate_internal(TagId::unique()),
                        node_id: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                        tab_index: tab_index.into(),
                    })
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
        if let Some(p) = self.get_background_content(&node_data, node_id, node_state) {
            s.push_str(&format!("background: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_position(&node_data, node_id, node_state) {
            s.push_str(&format!("background-position: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_size(&node_data, node_id, node_state) {
            s.push_str(&format!("background-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_background_repeat(&node_data, node_id, node_state) {
            s.push_str(&format!("background-repeat: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_font_size(&node_data, node_id, node_state) {
            s.push_str(&format!("font-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_font_family(&node_data, node_id, node_state) {
            s.push_str(&format!("font-family: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_color(&node_data, node_id, node_state) {
            s.push_str(&format!("color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_align(&node_data, node_id, node_state) {
            s.push_str(&format!("text-align: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_line_height(&node_data, node_id, node_state) {
            s.push_str(&format!("line-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_letter_spacing(&node_data, node_id, node_state) {
            s.push_str(&format!("letter-spacing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_word_spacing(&node_data, node_id, node_state) {
            s.push_str(&format!("word-spacing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_tab_size(&node_data, node_id, node_state) {
            s.push_str(&format!("tab-size: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_cursor(&node_data, node_id, node_state) {
            s.push_str(&format!("cursor: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_shadow_left(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-left: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_box_shadow_right(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-right: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_box_shadow_top(&node_data, node_id, node_state) {
            s.push_str(&format!("-azul-box-shadow-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_shadow_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "-azul-box-shadow-bottom: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_top_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_color(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-color: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_style(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-style: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_left_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-top-left-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_top_right_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-top-right-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_bottom_left_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-bottom-left-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_border_bottom_right_radius(&node_data, node_id, node_state) {
            s.push_str(&format!(
                "border-bottom-right-radius: {};",
                p.get_css_value_fmt()
            ));
        }
        if let Some(p) = self.get_opacity(&node_data, node_id, node_state) {
            s.push_str(&format!("opacity: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_transform(&node_data, node_id, node_state) {
            s.push_str(&format!("transform: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_transform_origin(&node_data, node_id, node_state) {
            s.push_str(&format!("transform-origin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_perspective_origin(&node_data, node_id, node_state) {
            s.push_str(&format!("perspective-origin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_backface_visibility(&node_data, node_id, node_state) {
            s.push_str(&format!("backface-visibility: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_hyphens(&node_data, node_id, node_state) {
            s.push_str(&format!("hyphens: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_direction(&node_data, node_id, node_state) {
            s.push_str(&format!("direction: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_unicode_bidi(&node_data, node_id, node_state) {
            s.push_str(&format!("unicode-bidi: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_box_trim(&node_data, node_id, node_state) {
            s.push_str(&format!("text-box-trim: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_text_box_edge(&node_data, node_id, node_state) {
            s.push_str(&format!("text-box-edge: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_dominant_baseline(&node_data, node_id, node_state) {
            s.push_str(&format!("dominant-baseline: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_alignment_baseline(&node_data, node_id, node_state) {
            s.push_str(&format!("alignment-baseline: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_initial_letter_align(&node_data, node_id, node_state) {
            s.push_str(&format!("initial-letter-align: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_initial_letter_wrap(&node_data, node_id, node_state) {
            s.push_str(&format!("initial-letter-wrap: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_scrollbar_gutter(&node_data, node_id, node_state) {
            s.push_str(&format!("scrollbar-gutter: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_clip_margin(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-clip-margin: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_clip(&node_data, node_id, node_state) {
            s.push_str(&format!("clip: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_white_space(&node_data, node_id, node_state) {
            s.push_str(&format!("white-space: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_display(&node_data, node_id, node_state) {
            s.push_str(&format!("display: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_float(&node_data, node_id, node_state) {
            s.push_str(&format!("float: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_box_sizing(&node_data, node_id, node_state) {
            s.push_str(&format!("box-sizing: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_width(&node_data, node_id, node_state) {
            s.push_str(&format!("width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_height(&node_data, node_id, node_state) {
            s.push_str(&format!("height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_min_width(&node_data, node_id, node_state) {
            s.push_str(&format!("min-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_min_height(&node_data, node_id, node_state) {
            s.push_str(&format!("min-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_max_width(&node_data, node_id, node_state) {
            s.push_str(&format!("max-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_max_height(&node_data, node_id, node_state) {
            s.push_str(&format!("max-height: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_position(&node_data, node_id, node_state) {
            s.push_str(&format!("position: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_top(&node_data, node_id, node_state) {
            s.push_str(&format!("top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_right(&node_data, node_id, node_state) {
            s.push_str(&format!("right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_left(&node_data, node_id, node_state) {
            s.push_str(&format!("left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_top(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_left(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_padding_right(&node_data, node_id, node_state) {
            s.push_str(&format!("padding-right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_top(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-top: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_bottom(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-bottom: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_left(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-left: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_margin_right(&node_data, node_id, node_state) {
            s.push_str(&format!("margin-right: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_top_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-top-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_left_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-left-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_right_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-right-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_border_bottom_width(&node_data, node_id, node_state) {
            s.push_str(&format!("border-bottom-width: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_x(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-x: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_overflow_y(&node_data, node_id, node_state) {
            s.push_str(&format!("overflow-y: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_direction(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-direction: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_wrap(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-wrap: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_grow(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-grow: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_flex_shrink(&node_data, node_id, node_state) {
            s.push_str(&format!("flex-shrink: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_justify_content(&node_data, node_id, node_state) {
            s.push_str(&format!("justify-content: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_align_items(&node_data, node_id, node_state) {
            s.push_str(&format!("align-items: {};", p.get_css_value_fmt()));
        }
        if let Some(p) = self.get_align_content(&node_data, node_id, node_state) {
            s.push_str(&format!("align-content: {};", p.get_css_value_fmt()));
        }
        s
    }
}

#[repr(C)]
#[derive(Debug, PartialEq, Clone)]
pub struct CssPropertyCachePtr {
    pub ptr: Box<CssPropertyCache>,
    pub run_destructor: bool,
}

impl CssPropertyCachePtr {
    pub fn new(cache: CssPropertyCache) -> Self {
        Self {
            ptr: Box::new(cache),
            run_destructor: true,
        }
    }
    pub fn downcast_mut<'a>(&'a mut self) -> &'a mut CssPropertyCache {
        &mut *self.ptr
    }
}

impl Drop for CssPropertyCachePtr {
    fn drop(&mut self) {
        self.run_destructor = false;
    }
}

impl CssPropertyCache {
    pub fn empty(node_count: usize) -> Self {
        Self {
            node_count,
            user_overridden_properties: Vec::new(),

            cascaded_props: FlatVecVec::new(node_count),
            css_props: FlatVecVec::new(node_count),

            computed_values: Vec::new(),
            compact_cache: None,
            global_css_props: Vec::new(),
            resolved_font_sizes_px: std::sync::OnceLock::new(),
        }
    }

    /// Clear the lazily-populated font-size cache. Call after any
    /// mutation that could change resolved font-sizes (restyle,
    /// DOM mutation, `append`, etc.). The next
    /// [`crate::styled_dom::StyledDom::resolved_font_size_px`] call
    /// repopulates via a single bottom-up tree walk.
    pub fn invalidate_resolved_font_sizes(&mut self) {
        self.resolved_font_sizes_px = std::sync::OnceLock::new();
    }

    pub fn append(&mut self, other: &mut Self) {
        self.user_overridden_properties.extend(other.user_overridden_properties.drain(..));
        self.cascaded_props.extend_from(&mut other.cascaded_props);
        self.css_props.extend_from(&mut other.css_props);
        self.computed_values.extend(other.computed_values.drain(..));

        self.node_count += other.node_count;
        // Indices shifted — invalidate the font-size cache too.
        self.resolved_font_sizes_px = std::sync::OnceLock::new();

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
            .and_then(|fs| fs.get_property().cloned())
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
            .and_then(|fs| fs.get_property().cloned())
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
        // fn below) when `AZUL_PROP_COUNT=1` is set to see which
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

        // Always use full cascade resolution.
        // Tier 1/2/2b handle layout-hot properties via direct typed getters.
        // This path is only used for paint-time reads (background, shadow, etc.)
        self.get_property_slow(node_data, node_id, node_state, css_property_type)
    }

    fn css_prop_type_label(t: &CssPropertyType) -> &'static str {
        // Intern Debug-format labels under a mutex-guarded map so
        // we leak at most one `&'static str` per distinct
        // `CssPropertyType` variant (bounded at ≤ 178 total). Only
        // triggered when `AZUL_PROP_COUNT=1`, so zero cost normally.
        use std::sync::{Mutex, OnceLock};
        static TABLE: OnceLock<Mutex<std::collections::HashMap<CssPropertyType, &'static str>>> =
            OnceLock::new();
        let m = TABLE.get_or_init(|| Mutex::new(std::collections::HashMap::new()));
        let mut g = m.lock().expect("AZUL_PROP_COUNT label table poisoned");
        if let Some(s) = g.get(t) {
            return *s;
        }
        let s: String = std::format!("{:?}", t);
        let leaked: &'static str = std::boxed::Box::leak(s.into_boxed_str());
        g.insert(*t, leaked);
        leaked
    }

    /// Full cascade resolution for any CSS property type.
    /// Walks all cascade layers: user overrides → inline → stylesheet → cascaded → computed → UA.
    /// Also used by restyle functions that need state-aware lookups.
    pub(crate) fn get_property_slow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {

        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                return Some(&v[idx].1);
            }
        }

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

    /// Get a CSS property using DynamicSelectorContext for evaluation.
    ///
    /// This is the new API that supports @media queries, @container queries,
    /// OS-specific styles, and all pseudo-states via `CssPropertyWithConditions`.
    ///
    /// The evaluation follows "last wins" semantics - properties are evaluated
    /// in reverse order and the first matching property wins.
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

    pub fn get_background_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundContent,
        )
        .and_then(|p| p.as_background_content())
    }

    /// Method for getting hyphens property
    pub fn get_hyphens<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHyphensValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Hyphens)
            .and_then(|p| p.as_hyphens())
    }

    /// Method for getting word-break property
    pub fn get_word_break<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWordBreakValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::WordBreak)
            .and_then(|p| p.as_word_break())
    }

    /// Method for getting overflow-wrap property
    pub fn get_overflow_wrap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleOverflowWrapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowWrap)
            .and_then(|p| p.as_overflow_wrap())
    }

    /// Method for getting line-break property
    pub fn get_line_break<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLineBreakValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::LineBreak)
            .and_then(|p| p.as_line_break())
    }

    /// Method for getting text-align-last property
    pub fn get_text_align_last<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextAlignLastValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextAlignLast)
            .and_then(|p| p.as_text_align_last())
    }

    /// Method for getting object-fit property
    pub fn get_object_fit<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleObjectFitValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ObjectFit)
            .and_then(|p| p.as_object_fit())
    }

    /// Method for getting text-orientation property
    pub fn get_text_orientation<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextOrientationValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextOrientation)
            .and_then(|p| p.as_text_orientation())
    }

    /// Method for getting object-position property
    pub fn get_object_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleObjectPositionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ObjectPosition)
            .and_then(|p| p.as_object_position())
    }

    /// Method for getting aspect-ratio property
    pub fn get_aspect_ratio<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleAspectRatioValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AspectRatio)
            .and_then(|p| p.as_aspect_ratio())
    }

    /// Method for getting direction property
    pub fn get_direction<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleDirectionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Direction)
            .and_then(|p| p.as_direction())
    }

    pub fn get_unicode_bidi<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleUnicodeBidiValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::UnicodeBidi)
            .and_then(|p| p.as_unicode_bidi())
    }

    pub fn get_text_box_trim<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextBoxTrimValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextBoxTrim)
            .and_then(|p| p.as_text_box_trim())
    }

    pub fn get_text_box_edge<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextBoxEdgeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextBoxEdge)
            .and_then(|p| p.as_text_box_edge())
    }

    pub fn get_dominant_baseline<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleDominantBaselineValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::DominantBaseline)
            .and_then(|p| p.as_dominant_baseline())
    }

    pub fn get_alignment_baseline<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleAlignmentBaselineValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignmentBaseline)
            .and_then(|p| p.as_alignment_baseline())
    }

    pub fn get_initial_letter_align<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleInitialLetterAlignValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::InitialLetterAlign)
            .and_then(|p| p.as_initial_letter_align())
    }

    pub fn get_initial_letter_wrap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleInitialLetterWrapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::InitialLetterWrap)
            .and_then(|p| p.as_initial_letter_wrap())
    }

    pub fn get_scrollbar_gutter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleScrollbarGutterValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ScrollbarGutter)
            .and_then(|p| p.as_scrollbar_gutter())
    }

    pub fn get_overflow_clip_margin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleOverflowClipMarginValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowClipMargin)
            .and_then(|p| p.as_overflow_clip_margin())
    }

    pub fn get_clip<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleClipRectValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Clip)
            .and_then(|p| p.as_clip())
    }

    /// Method for getting white-space property
    pub fn get_white_space<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWhiteSpaceValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::WhiteSpace)
            .and_then(|p| p.as_white_space())
    }
    pub fn get_background_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundPositionVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundPosition,
        )
        .and_then(|p| p.as_background_position())
    }
    pub fn get_background_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundSizeVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundSize,
        )
        .and_then(|p| p.as_background_size())
    }
    pub fn get_background_repeat<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundRepeatVecValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackgroundRepeat,
        )
        .and_then(|p| p.as_background_repeat())
    }
    pub fn get_font_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontSizeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontSize)
            .and_then(|p| p.as_font_size())
    }
    pub fn get_font_family<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontFamilyVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontFamily)
            .and_then(|p| p.as_font_family())
    }
    pub fn get_font_weight<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontWeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontWeight)
            .and_then(|p| p.as_font_weight())
    }
    pub fn get_font_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontStyleValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FontStyle)
            .and_then(|p| p.as_font_style())
    }
    pub fn get_text_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextColorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextColor)
            .and_then(|p| p.as_text_color())
    }
    /// Method for getting text-indent property
    pub fn get_text_indent<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextIndentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextIndent)
            .and_then(|p| p.as_text_indent())
    }
    /// Method for getting initial-letter property
    pub fn get_initial_letter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleInitialLetterValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::InitialLetter,
        )
        .and_then(|p| p.as_initial_letter())
    }
    /// Method for getting line-clamp property
    pub fn get_line_clamp<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLineClampValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::LineClamp)
            .and_then(|p| p.as_line_clamp())
    }
    /// Method for getting hanging-punctuation property
    pub fn get_hanging_punctuation<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHangingPunctuationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::HangingPunctuation,
        )
        .and_then(|p| p.as_hanging_punctuation())
    }
    /// Method for getting text-combine-upright property
    pub fn get_text_combine_upright<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextCombineUprightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextCombineUpright,
        )
        .and_then(|p| p.as_text_combine_upright())
    }
    /// Method for getting -azul-exclusion-margin property
    pub fn get_exclusion_margin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleExclusionMarginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ExclusionMargin,
        )
        .and_then(|p| p.as_exclusion_margin())
    }
    /// Method for getting -azul-hyphenation-language property
    pub fn get_hyphenation_language<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleHyphenationLanguageValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::HyphenationLanguage,
        )
        .and_then(|p| p.as_hyphenation_language())
    }
    /// Method for getting caret-color property
    pub fn get_caret_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretColorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::CaretColor)
            .and_then(|p| p.as_caret_color())
    }

    /// Method for getting -azul-caret-width property
    pub fn get_caret_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::CaretWidth)
            .and_then(|p| p.as_caret_width())
    }

    /// Method for getting caret-animation-duration property
    pub fn get_caret_animation_duration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CaretAnimationDurationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CaretAnimationDuration,
        )
        .and_then(|p| p.as_caret_animation_duration())
    }

    /// Method for getting selection-background-color property
    pub fn get_selection_background_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionBackgroundColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionBackgroundColor,
        )
        .and_then(|p| p.as_selection_background_color())
    }

    /// Method for getting selection-color property
    pub fn get_selection_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionColor,
        )
        .and_then(|p| p.as_selection_color())
    }

    /// Method for getting -azul-selection-radius property
    pub fn get_selection_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a SelectionRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::SelectionRadius,
        )
        .and_then(|p| p.as_selection_radius())
    }

    /// Method for getting text-justify property
    pub fn get_text_justify<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTextJustifyValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextJustify,
        )
        .and_then(|p| p.as_text_justify())
    }

    /// Method for getting z-index property
    pub fn get_z_index<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutZIndexValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ZIndex)
            .and_then(|p| p.as_z_index())
    }

    /// Method for getting flex-basis property
    pub fn get_flex_basis<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexBasisValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexBasis)
            .and_then(|p| p.as_flex_basis())
    }

    /// Method for getting column-gap property
    pub fn get_column_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutColumnGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnGap)
            .and_then(|p| p.as_column_gap())
    }

    /// Method for getting row-gap property
    pub fn get_row_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutRowGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::RowGap)
            .and_then(|p| p.as_row_gap())
    }

    /// Method for getting grid-template-columns property
    pub fn get_grid_template_columns<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridTemplateColumnsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridTemplateColumns,
        )
        .and_then(|p| p.as_grid_template_columns())
    }

    /// Method for getting grid-template-rows property
    pub fn get_grid_template_rows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridTemplateRowsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridTemplateRows,
        )
        .and_then(|p| p.as_grid_template_rows())
    }

    /// Method for getting grid-auto-columns property
    pub fn get_grid_auto_columns<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoColumnsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoColumns,
        )
        .and_then(|p| p.as_grid_auto_columns())
    }

    /// Method for getting grid-auto-rows property
    pub fn get_grid_auto_rows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoRowsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoRows,
        )
        .and_then(|p| p.as_grid_auto_rows())
    }

    /// Method for getting grid-column property
    pub fn get_grid_column<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridColumnValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridColumn)
            .and_then(|p| p.as_grid_column())
    }

    /// Method for getting grid-row property
    pub fn get_grid_row<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridRowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridRow)
            .and_then(|p| p.as_grid_row())
    }

    /// Method for getting grid-auto-flow property
    pub fn get_grid_auto_flow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGridAutoFlowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::GridAutoFlow,
        )
        .and_then(|p| p.as_grid_auto_flow())
    }

    /// Method for getting justify-self property
    pub fn get_justify_self<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifySelfValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifySelf,
        )
        .and_then(|p| p.as_justify_self())
    }

    /// Method for getting justify-items property
    pub fn get_justify_items<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifyItemsValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifyItems,
        )
        .and_then(|p| p.as_justify_items())
    }

    /// Method for getting gap property
    pub fn get_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Gap)
            .and_then(|p| p.as_gap())
    }

    /// Method for getting grid-gap property
    pub(crate) fn get_grid_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridGap)
            .and_then(|p| p.as_grid_gap())
    }

    /// Method for getting align-self property
    pub fn get_align_self<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignSelfValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignSelf)
            .and_then(|p| p.as_align_self())
    }

    /// Method for getting font property
    pub fn get_font<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFontValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Font)
            .and_then(|p| p.as_font())
    }

    /// Method for getting writing-mode property
    pub fn get_writing_mode<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutWritingModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::WritingMode,
        )
        .and_then(|p| p.as_writing_mode())
    }

    /// Method for getting clear property
    pub fn get_clear<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutClearValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Clear)
            .and_then(|p| p.as_clear())
    }

    /// Method for getting shape-outside property
    pub fn get_shape_outside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeOutsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeOutside,
        )
        .and_then(|p| p.as_shape_outside())
    }

    /// Method for getting shape-inside property
    pub fn get_shape_inside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeInsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeInside,
        )
        .and_then(|p| p.as_shape_inside())
    }

    /// Method for getting clip-path property
    pub fn get_clip_path<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ClipPathValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ClipPath)
            .and_then(|p| p.as_clip_path())
    }

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

    /// Method for getting scrollbar-width property
    pub fn get_scrollbar_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutScrollbarWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarWidth,
        )
        .and_then(|p| p.as_scrollbar_width())
    }

    /// Method for getting scrollbar-color property
    pub fn get_scrollbar_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleScrollbarColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarColor,
        )
        .and_then(|p| p.as_scrollbar_color())
    }

    /// Method for getting -azul-scrollbar-visibility property
    pub fn get_scrollbar_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarVisibilityModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarVisibility,
        )
        .and_then(|p| p.as_scrollbar_visibility())
    }

    /// Method for getting -azul-scrollbar-fade-delay property
    pub fn get_scrollbar_fade_delay<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarFadeDelayValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarFadeDelay,
        )
        .and_then(|p| p.as_scrollbar_fade_delay())
    }

    /// Method for getting -azul-scrollbar-fade-duration property
    pub fn get_scrollbar_fade_duration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ScrollbarFadeDurationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarFadeDuration,
        )
        .and_then(|p| p.as_scrollbar_fade_duration())
    }

    /// Method for getting visibility property
    pub fn get_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleVisibilityValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Visibility)
            .and_then(|p| p.as_visibility())
    }

    /// Method for getting break-before property
    pub fn get_break_before<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a PageBreakValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BreakBefore,
        )
        .and_then(|p| p.as_break_before())
    }

    /// Method for getting break-after property
    pub fn get_break_after<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a PageBreakValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BreakAfter)
            .and_then(|p| p.as_break_after())
    }

    /// Method for getting break-inside property
    pub fn get_break_inside<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a BreakInsideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BreakInside,
        )
        .and_then(|p| p.as_break_inside())
    }

    /// Method for getting orphans property
    pub fn get_orphans<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a OrphansValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Orphans)
            .and_then(|p| p.as_orphans())
    }

    /// Method for getting widows property
    pub fn get_widows<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a WidowsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Widows)
            .and_then(|p| p.as_widows())
    }

    /// Method for getting box-decoration-break property
    pub fn get_box_decoration_break<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a BoxDecorationBreakValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxDecorationBreak,
        )
        .and_then(|p| p.as_box_decoration_break())
    }

    /// Method for getting column-count property
    pub fn get_column_count<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnCountValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnCount,
        )
        .and_then(|p| p.as_column_count())
    }

    /// Method for getting column-width property
    pub fn get_column_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnWidth,
        )
        .and_then(|p| p.as_column_width())
    }

    /// Method for getting column-span property
    pub fn get_column_span<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnSpanValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnSpan)
            .and_then(|p| p.as_column_span())
    }

    /// Method for getting column-fill property
    pub fn get_column_fill<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnFillValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::ColumnFill)
            .and_then(|p| p.as_column_fill())
    }

    /// Method for getting column-rule-width property
    pub fn get_column_rule_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleWidth,
        )
        .and_then(|p| p.as_column_rule_width())
    }

    /// Method for getting column-rule-style property
    pub fn get_column_rule_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleStyle,
        )
        .and_then(|p| p.as_column_rule_style())
    }

    /// Method for getting column-rule-color property
    pub fn get_column_rule_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ColumnRuleColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ColumnRuleColor,
        )
        .and_then(|p| p.as_column_rule_color())
    }

    /// Method for getting flow-into property
    pub fn get_flow_into<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a FlowIntoValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlowInto)
            .and_then(|p| p.as_flow_into())
    }

    /// Method for getting flow-from property
    pub fn get_flow_from<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a FlowFromValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlowFrom)
            .and_then(|p| p.as_flow_from())
    }

    /// Method for getting shape-margin property
    pub fn get_shape_margin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeMarginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeMargin,
        )
        .and_then(|p| p.as_shape_margin())
    }

    /// Method for getting shape-image-threshold property
    pub fn get_shape_image_threshold<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ShapeImageThresholdValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ShapeImageThreshold,
        )
        .and_then(|p| p.as_shape_image_threshold())
    }

    /// Method for getting content property
    pub fn get_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a ContentValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Content)
            .and_then(|p| p.as_content())
    }

    /// Method for getting counter-reset property
    pub fn get_counter_reset<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CounterResetValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CounterReset,
        )
        .and_then(|p| p.as_counter_reset())
    }

    /// Method for getting counter-increment property
    pub fn get_counter_increment<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a CounterIncrementValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CounterIncrement,
        )
        .and_then(|p| p.as_counter_increment())
    }

    /// Method for getting string-set property
    pub fn get_string_set<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StringSetValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::StringSet)
            .and_then(|p| p.as_string_set())
    }
    pub fn get_text_align<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextAlignValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextAlign)
            .and_then(|p| p.as_text_align())
    }
    pub fn get_user_select<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleUserSelectValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::UserSelect)
            .and_then(|p| p.as_user_select())
    }
    pub fn get_text_decoration<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTextDecorationValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TextDecoration,
        )
        .and_then(|p| p.as_text_decoration())
    }
    pub fn get_vertical_align<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleVerticalAlignValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::VerticalAlign,
        )
        .and_then(|p| p.as_vertical_align())
    }
    pub fn get_line_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLineHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::LineHeight)
            .and_then(|p| p.as_line_height())
    }
    pub fn get_letter_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleLetterSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::LetterSpacing,
        )
        .and_then(|p| p.as_letter_spacing())
    }
    pub fn get_word_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleWordSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::WordSpacing,
        )
        .and_then(|p| p.as_word_spacing())
    }
    pub fn get_tab_size<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTabSizeValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TabSize)
            .and_then(|p| p.as_tab_size())
    }
    pub fn get_cursor<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleCursorValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Cursor)
            .and_then(|p| p.as_cursor())
    }
    pub fn get_box_shadow_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowLeft,
        )
        .and_then(|p| p.as_box_shadow_left())
    }
    pub fn get_box_shadow_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowRight,
        )
        .and_then(|p| p.as_box_shadow_right())
    }
    pub fn get_box_shadow_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowTop,
        )
        .and_then(|p| p.as_box_shadow_top())
    }
    pub fn get_box_shadow_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BoxShadowBottom,
        )
        .and_then(|p| p.as_box_shadow_bottom())
    }
    pub fn get_border_top_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopColor,
        )
        .and_then(|p| p.as_border_top_color())
    }
    pub fn get_border_left_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderLeftColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftColor,
        )
        .and_then(|p| p.as_border_left_color())
    }
    pub fn get_border_right_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderRightColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightColor,
        )
        .and_then(|p| p.as_border_right_color())
    }
    pub fn get_border_bottom_color<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomColorValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomColor,
        )
        .and_then(|p| p.as_border_bottom_color())
    }
    pub fn get_border_top_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopStyle,
        )
        .and_then(|p| p.as_border_top_style())
    }
    pub fn get_border_left_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderLeftStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftStyle,
        )
        .and_then(|p| p.as_border_left_style())
    }
    pub fn get_border_right_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderRightStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightStyle,
        )
        .and_then(|p| p.as_border_right_style())
    }
    pub fn get_border_bottom_style<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomStyleValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomStyle,
        )
        .and_then(|p| p.as_border_bottom_style())
    }
    pub fn get_border_top_left_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopLeftRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopLeftRadius,
        )
        .and_then(|p| p.as_border_top_left_radius())
    }
    pub fn get_border_top_right_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderTopRightRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopRightRadius,
        )
        .and_then(|p| p.as_border_top_right_radius())
    }
    pub fn get_border_bottom_left_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomLeftRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomLeftRadius,
        )
        .and_then(|p| p.as_border_bottom_left_radius())
    }
    pub fn get_border_bottom_right_radius<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderBottomRightRadiusValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomRightRadius,
        )
        .and_then(|p| p.as_border_bottom_right_radius())
    }
    pub fn get_opacity<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleOpacityValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Opacity)
            .and_then(|p| p.as_opacity())
    }
    pub fn get_transform<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTransformVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Transform)
            .and_then(|p| p.as_transform())
    }
    pub fn get_transform_origin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleTransformOriginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TransformOrigin,
        )
        .and_then(|p| p.as_transform_origin())
    }
    pub fn get_perspective_origin<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StylePerspectiveOriginValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PerspectiveOrigin,
        )
        .and_then(|p| p.as_perspective_origin())
    }
    pub fn get_backface_visibility<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackfaceVisibilityValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BackfaceVisibility,
        )
        .and_then(|p| p.as_backface_visibility())
    }
    pub fn get_display<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutDisplayValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Display)
            .and_then(|p| p.as_display())
    }
    pub fn get_float<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFloatValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Float)
            .and_then(|p| p.as_float())
    }
    pub fn get_box_sizing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBoxSizingValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BoxSizing)
            .and_then(|p| p.as_box_sizing())
    }
    pub fn get_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Width)
            .and_then(|p| p.as_width())
    }
    pub fn get_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Height)
            .and_then(|p| p.as_height())
    }
    pub fn get_min_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMinWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MinWidth)
            .and_then(|p| p.as_min_width())
    }
    pub fn get_min_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMinHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MinHeight)
            .and_then(|p| p.as_min_height())
    }
    pub fn get_max_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMaxWidthValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MaxWidth)
            .and_then(|p| p.as_max_width())
    }
    pub fn get_max_height<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMaxHeightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MaxHeight)
            .and_then(|p| p.as_max_height())
    }
    pub fn get_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPositionValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Position)
            .and_then(|p| p.as_position())
    }
    pub fn get_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Top)
            .and_then(|p| p.as_top())
    }
    pub fn get_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutInsetBottomValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Bottom)
            .and_then(|p| p.as_bottom())
    }
    pub fn get_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutRightValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Right)
            .and_then(|p| p.as_right())
    }
    pub fn get_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutLeftValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Left)
            .and_then(|p| p.as_left())
    }
    pub fn get_padding_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::PaddingTop)
            .and_then(|p| p.as_padding_top())
    }
    pub fn get_padding_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingBottomValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingBottom,
        )
        .and_then(|p| p.as_padding_bottom())
    }
    pub fn get_padding_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingLeftValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingLeft,
        )
        .and_then(|p| p.as_padding_left())
    }
    pub fn get_padding_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutPaddingRightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::PaddingRight,
        )
        .and_then(|p| p.as_padding_right())
    }
    pub fn get_margin_top<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginTopValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MarginTop)
            .and_then(|p| p.as_margin_top())
    }
    pub fn get_margin_bottom<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginBottomValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MarginBottom,
        )
        .and_then(|p| p.as_margin_bottom())
    }
    pub fn get_margin_left<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginLeftValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::MarginLeft)
            .and_then(|p| p.as_margin_left())
    }
    pub fn get_margin_right<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutMarginRightValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MarginRight,
        )
        .and_then(|p| p.as_margin_right())
    }
    pub fn get_border_top_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderTopWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderTopWidth,
        )
        .and_then(|p| p.as_border_top_width())
    }
    pub fn get_border_left_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderLeftWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderLeftWidth,
        )
        .and_then(|p| p.as_border_left_width())
    }
    pub fn get_border_right_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderRightWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderRightWidth,
        )
        .and_then(|p| p.as_border_right_width())
    }
    pub fn get_border_bottom_width<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderBottomWidthValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderBottomWidth,
        )
        .and_then(|p| p.as_border_bottom_width())
    }
    pub fn get_overflow_x<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowX)
            .and_then(|p| p.as_overflow_x())
    }
    pub fn get_overflow_y<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowY)
            .and_then(|p| p.as_overflow_y())
    }
    pub fn get_overflow_block<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowBlock)
            .and_then(|p| p.as_overflow_block())
    }
    pub fn get_overflow_inline<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutOverflowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::OverflowInline)
            .and_then(|p| p.as_overflow_inline())
    }
    pub fn get_flex_direction<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexDirectionValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::FlexDirection,
        )
        .and_then(|p| p.as_flex_direction())
    }
    pub fn get_flex_wrap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexWrapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexWrap)
            .and_then(|p| p.as_flex_wrap())
    }
    pub fn get_flex_grow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexGrowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexGrow)
            .and_then(|p| p.as_flex_grow())
    }
    pub fn get_flex_shrink<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutFlexShrinkValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::FlexShrink)
            .and_then(|p| p.as_flex_shrink())
    }
    pub fn get_justify_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutJustifyContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::JustifyContent,
        )
        .and_then(|p| p.as_justify_content())
    }
    pub fn get_align_items<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignItemsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::AlignItems)
            .and_then(|p| p.as_align_items())
    }
    pub fn get_align_content<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutAlignContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::AlignContent,
        )
        .and_then(|p| p.as_align_content())
    }
    pub fn get_mix_blend_mode<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleMixBlendModeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::MixBlendMode,
        )
        .and_then(|p| p.as_mix_blend_mode())
    }
    pub fn get_filter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFilterVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::Filter)
            .and_then(|p| p.as_filter())
    }
    pub fn get_backdrop_filter<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleFilterVecValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::BackdropFilter)
            .and_then(|p| p.as_backdrop_filter())
    }
    pub fn get_text_shadow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBoxShadowValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::TextShadow)
            .and_then(|p| p.as_text_shadow())
    }
    pub fn get_list_style_type<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleListStyleTypeValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ListStyleType,
        )
        .and_then(|p| p.as_list_style_type())
    }
    pub fn get_list_style_position<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleListStylePositionValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ListStylePosition,
        )
        .and_then(|p| p.as_list_style_position())
    }
    pub fn get_table_layout<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutTableLayoutValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::TableLayout,
        )
        .and_then(|p| p.as_table_layout())
    }
    pub fn get_border_collapse<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBorderCollapseValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderCollapse,
        )
        .and_then(|p| p.as_border_collapse())
    }
    pub fn get_border_spacing<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutBorderSpacingValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::BorderSpacing,
        )
        .and_then(|p| p.as_border_spacing())
    }
    pub fn get_caption_side<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleCaptionSideValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::CaptionSide,
        )
        .and_then(|p| p.as_caption_side())
    }
    pub fn get_empty_cells<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleEmptyCellsValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::EmptyCells)
            .and_then(|p| p.as_empty_cells())
    }

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
            SizeMetric::Em => return None, // Reference can't be relative
            SizeMetric::Rem => return None, // Reference can't be relative
            SizeMetric::Percent => return None, // Reference can't be relative
            // Reference can't be viewport-relative
            SizeMetric::Vw | SizeMetric::Vh | SizeMetric::Vmin | SizeMetric::Vmax => return None,
        };

        // Resolve target based on reference
        let resolved_px = match target_pixel_value.metric {
            SizeMetric::Px => target_pixel_value.number.get(),
            SizeMetric::Pt => target_pixel_value.number.get() * PT_TO_PX,
            SizeMetric::In => target_pixel_value.number.get() * IN_TO_PX,
            SizeMetric::Cm => target_pixel_value.number.get() * CM_TO_PX,
            SizeMetric::Mm => target_pixel_value.number.get() * MM_TO_PX,
            SizeMetric::Em => target_pixel_value.number.get() * reference_px,
            // Use reference as root font-size
            SizeMetric::Rem => target_pixel_value.number.get() * reference_px,
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
            for p in props.iter() {
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
            for p in props.iter() {
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

    /// Sort cascaded_props by (state, prop_type) and flatten into contiguous memory.
    /// Must be called after apply_ua_css() which adds entries to cascaded_props.
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
            for p in cascaded_slice.iter() {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal {
                    if self.should_apply_cascaded(&ctx.computed_values, p.prop_type, &p.property) {
                        self.process_property(ctx, &p.property, parent_computed);
                    }
                }
            }
        }

        // Step 3: CSS properties (stylesheets)
        {
            let css_slice = self.css_props.get_slice(node_id.index());
            for p in css_slice.iter() {
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
            for (_, prop) in user_props.iter() {
                self.process_property(ctx, &prop, parent_computed);
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

        match computed.binary_search_by_key(&prop_type, |(k, _)| *k) {
            Err(_) => true,
            Ok(idx) => computed[idx].1.origin == CssPropertyOrigin::Inherited,
        }
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
            .map(|fs| {
                matches!(
                    fs.inner.metric,
                    SizeMetric::Em | SizeMetric::Rem | SizeMetric::Percent
                )
            })
            .unwrap_or(false)
    }

    /// Store computed values if changed, returns true if values were updated
    fn store_if_changed(&mut self, ctx: &InheritanceContext) -> bool {
        let values_changed = self
            .computed_values
            .get(ctx.node_id.index())
            .map(|old| old != &ctx.computed_values)
            .unwrap_or(true);

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

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
use core::{fmt::Write, mem::ManuallyDrop};

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

/// What the root of a HOSTED DOM - a `VirtualView`'s content - inherits from
/// the node that hosts it.
///
/// The host's value of every inheritable property it has one for, sorted by
/// type ([`CssPropertyCache::inherited_from_host`], built by
/// [`CssPropertyCache::inherited_values_for_hosted_dom`]). Empty for a document
/// of its own.
pub type InheritedFromHost = Vec<(CssPropertyType, CssPropertyWithOrigin)>;

use azul_css::{
    css::{Css, CssPath},
    dynamic_selector::{
        CssPropertyWithConditions, CssPropertyWithConditionsVec, DynamicSelectorContext,
    },
    props::{
        basic::{StyleFontFamily, StyleFontFamilyVec, StyleFontSize},
        layout::{LayoutDisplay, LayoutHeight, LayoutWidth},
        property::{
            BoxDecorationBreakValue, BreakInsideValue, CaretAnimationDurationValue,
            CaretColorValue, CaretWidthValue, ClipPathValue, ColumnCountValue, ColumnFillValue,
            ColumnRuleColorValue, ColumnRuleStyleValue, ColumnRuleWidthValue, ColumnSpanValue,
            ColumnWidthValue, ContentValue, CounterIncrementValue, CounterResetValue, CounterSetValue,
            CssProperty, StyleBackgroundOriginVecValue, StyleQuotesValue,
            CssPropertyType, FlowFromValue, FlowIntoValue, RegionFragmentValue,
            WrapFlowValue, WrapThroughValue,
            LayoutAlignContentValue,
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
            LayoutWritingModeValue, LayoutZIndexValue, OrphansValue, OverscrollBehaviorValue,
            PageBreakValue, ScrollbarFadeDelayValue, ScrollbarFadeDurationValue,
            ScrollbarVisibilityModeValue, SelectionBackgroundColorValue, SelectionColorValue,
            SelectionRadiusValue, ShapeImageThresholdValue, ShapeInsideValue, ShapeMarginValue,
            ShapeOutsideValue, StringSetValue, StyleAlignmentBaselineValue, StyleAppRegionValue,
            StyleAspectRatioValue, StyleBackfaceVisibilityValue, StyleBackgroundContentValue,
            StyleBackgroundContentVecValue, StyleBackgroundPositionVecValue,
            StyleBackgroundRepeatVecValue, StyleBackgroundSizeVecValue, StyleBaselineSourceValue,
            StyleBorderBottomColorValue, StyleBorderBottomLeftRadiusValue,
            StyleBorderBottomRightRadiusValue, StyleBorderBottomStyleValue,
            StyleBorderCollapseValue, StyleBorderLeftColorValue, StyleBorderLeftStyleValue,
            StyleBorderRightColorValue, StyleBorderRightStyleValue, StyleBorderTopColorValue,
            StyleBorderTopLeftRadiusValue, StyleBorderTopRightRadiusValue,
            StyleBorderTopStyleValue, StyleBoxShadowValue, StyleCaptionSideValue,
            StyleClipRectValue, StyleCursorValue, StyleDirectionValue, StyleDominantBaselineValue,
            StyleEmptyCellsValue, StyleExclusionMarginValue, StyleFilterVecValue,
            StyleFontFamilyVecValue, StyleFontSizeValue, StyleFontStyleValue, StyleFontValue,
            StyleFontWeightValue, StyleHangingPunctuationValue, StyleHyphenationLanguageValue,
            StyleHyphensValue, StyleInitialLetterAlignValue, StyleInitialLetterValue,
            StyleInitialLetterWrapValue, StyleLetterSpacingValue, StyleLineBreakValue,
            StyleLineClampValue, StyleLineFitEdgeValue, StyleLineHeightValue,
            StyleListStylePositionValue, StyleListStyleTypeValue, StyleMixBlendModeValue,
            StyleObjectFitValue, StyleObjectPositionValue, StyleOpacityValue,
            StyleOverflowClipMarginValue, StyleOverflowWrapValue, StylePerspectiveOriginValue,
            StyleScrollbarColorValue, StyleScrollbarGutterValue, StyleSpatialNavigationActionValue,
            StyleSpatialNavigationContainValue, StyleSpatialNavigationFunctionValue,
            StyleTabSizeValue, StyleTextAlignLastValue,
            StyleTextAlignValue, StyleTextBoxEdgeValue, StyleTextBoxTrimValue, StyleTextColorValue,
            StyleTextCombineUprightValue, StyleTextDecorationValue, StyleTextIndentValue,
            StyleTextOrientationValue, StyleTextOverflowValue, StyleTextTransformValue,
            StyleTransformOriginValue, StyleTransformVecValue, StyleUnicodeBidiValue,
            StyleUserSelectValue, StyleVerticalAlignValue, StyleVisibilityValue,
            StyleWhiteSpaceValue, StyleWordBreakValue, StyleWordSpacingValue, StyleZoomValue, StyleBackgroundClipVecValue,
            StyleFontFeatureSettingsValue, StyleFontVariantCapsValue,
            StyleFontVariantNumericValue, WidowsValue,
        },
        style::{StyleCursor, StyleTextColor, StyleTransformOrigin},
    },
    AzString,
};

use crate::{
    dom::{NodeData, NodeId, TabIndex, TagId},
    id::NodeDataContainerRef,
    style::CascadeInfo,
    styled_dom::{
        NodeHierarchyItem, NodeHierarchyItemId, NodeHierarchyItemVec, ParentWithNodeDepth,
        ParentWithNodeDepthVec, StyledNodeState, TagIdToNodeIdMapping,
    },
    work_count::WorkCount,
};
// The tests build their trees with it (they import this module's names).
#[cfg(test)]
use crate::id::NodeDataContainer;

/// How often each property type was looked up through [`CssPropertyCache::get_property`].
///
/// A cascade walk is counted only under `AZ_PROFILE=cascade`: the narrow diagnostic that says which
/// properties drove the cascade walks of a layout pass. It is no part of what the cache is: two
/// caches that differ in nothing else are equal. [`CssPropertyCache::drain_css_prop_counts`] hands
/// the counts out.
#[derive(Debug, Default)]
pub struct PropWalks {
    #[cfg(feature = "std")]
    counts: std::sync::Mutex<alloc::collections::BTreeMap<CssPropertyType, usize>>,
}

impl Clone for PropWalks {
    fn clone(&self) -> Self {
        Self {
            #[cfg(feature = "std")]
            counts: std::sync::Mutex::new(
                self.counts.lock().map(|counts| counts.clone()).unwrap_or_default(),
            ),
        }
    }
}

impl PartialEq for PropWalks {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl PropWalks {
    /// One more walk for `ty`.
    #[cfg(feature = "std")]
    fn count(&self, ty: CssPropertyType) {
        if let Ok(mut counts) = self.counts.lock() {
            *counts.entry(ty).or_insert(0) += 1;
        }
    }

    /// Without `std` there is no lock to count behind, and no environment that asks for it.
    #[cfg(not(feature = "std"))]
    fn count(&self, _ty: CssPropertyType) {}

    /// Takes the counts, sorted by count descending.
    #[cfg(feature = "std")]
    fn drain(&self) -> Vec<(CssPropertyType, usize)> {
        let taken = self
            .counts
            .lock()
            .map(|mut counts| core::mem::take(&mut *counts))
            .unwrap_or_default();
        let mut counts: Vec<_> = taken.into_iter().collect();
        counts.sort_by(|a, b| b.1.cmp(&a.1));
        counts
    }

    #[cfg(not(feature = "std"))]
    fn drain(&self) -> Vec<(CssPropertyType, usize)> {
        Vec::new()
    }
}

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
            CssProperty::BaselineSource($value) => $expr,
            CssProperty::LineFitEdge($value) => $expr,
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
            CssProperty::WrapThrough($value) => $expr,
            CssProperty::WrapFlow($value) => $expr,
            CssProperty::RegionFragment($value) => $expr,
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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatefulCssProperty {
    pub state: azul_css::dynamic_selector::PseudoStateType,
    pub prop_type: CssPropertyType,
    pub property: CssProperty,
    /// `true` when this entry was pushed by [`CssPropertyCache::apply_ua_css`]
    /// (a user-agent default), `false` for everything the author cascade and
    /// the inheritance walk produced.
    ///
    /// The UA defaults are answered from the window's THEME (`<hr>` rule,
    /// native button borders, the root text colour), so a theme flip has to
    /// replace them. `apply_ua_css` guards duplicates with a per-node bitset
    /// built from the entries already present — which made a second
    /// application a no-op: the stale light entry marked the bit, the dark
    /// twin was skipped, and the flip never reached `cascaded_props`. The
    /// origin tag is what lets a re-application strip exactly its own
    /// previous entries first (and nothing else).
    pub ua_origin: bool,
    /// The `rule_priority` of the rule that declared this value: a value for
    /// one state beats one for a stronger state, or the resting value beats
    /// a state's, only from a higher priority (a rice at `priority: app`
    /// over the app's `:hover` rule); at equal priority the stronger state
    /// wins. `rule_priority::PRESENTATIONAL` for a presentational hint,
    /// [`ENGINE_PRIORITY`] for what the engine decides itself, and 0 for
    /// what the node inherited and for a UA default, which are never
    /// compared this way.
    pub priority: u8,
}

/// Where the declaration that won a property's cascade sits, comparable
/// across two properties: the stronger rank is the one the cascade would
/// have taken had both declared the same property.
///
/// By its source first (a runtime override, then the `::placeholder`'s own
/// values, then what the node declares, then a `*` rule, then what it
/// inherited or the UA default), then by `rule_priority`, then by the
/// strength of the state it was declared for, then - for the node's own
/// declarations - by their order (the later one wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct CascadeRank {
    source: u8,
    priority: u8,
    state: u8,
    order: u32,
}

impl CascadeRank {
    pub const USER_OVERRIDE: Self = Self::of_source(4);
    pub const PLACEHOLDER: Self = Self::of_source(3);
    pub const GLOBAL_RULE: Self = Self::of_source(1);
    pub const INHERITED: Self = Self::of_source(0);

    const fn of_source(source: u8) -> Self {
        Self {
            source,
            priority: 0,
            state: 0,
            order: 0,
        }
    }

    /// A declaration of the node at `priority`, for a state of `state`
    /// strength (the resting state the weakest).
    const fn declared(priority: u8, state: u8) -> Self {
        Self {
            source: 2,
            priority,
            state,
            order: 0,
        }
    }
}

/// The [`StatefulCssProperty::priority`] of what the engine decides itself
/// (a generated node whose `content` makes nothing has no box): above every
/// rule and the node's own style.
pub const ENGINE_PRIORITY: u8 = u8::MAX;

/// One bit per `CssPropertyType` (fewer than 256 variants): "this node has
/// a Normal-state value of this type from a layer that beats the UA sheet".
type PropTypeBits = [u128; 2];

#[inline]
const fn prop_type_bit_set(bits: &mut PropTypeBits, prop_type: CssPropertyType) {
    let d = prop_type as u16 as usize;
    if d < 128 {
        bits[0] |= 1u128 << d;
    } else {
        bits[1] |= 1u128 << (d - 128);
    }
}

#[inline]
const fn prop_type_bit_test(bits: &PropTypeBits, prop_type: CssPropertyType) -> bool {
    let d = prop_type as u16 as usize;
    if d < 128 {
        (bits[0] & (1u128 << d)) != 0
    } else {
        (bits[1] & (1u128 << (d - 128))) != 0
    }
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
    /// How many entries of `data` the rows [`Self::replace_rows`] replaced since the store
    /// was last compacted pointed at (an upper bound of what no node points at any more).
    orphaned: usize,
    /// How many recorded runs `dedup_runs` compared a node's run with: what the size sweep of
    /// the tests reads to say nodes of one rule set do not each read the table of runs.
    run_compares: WorkCount,
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
            orphaned: 0,
            run_compares: WorkCount::new(),
        }
    }
}

impl<T> FlatVecVec<T> {
    /// Approximate heap bytes retained. Sums capacity of the
    /// flattened `data` + `offsets` tables and the per-node build
    /// Vecs (in case `sort_each_and_flatten` hasn't been called
    /// yet). `per_element_size` should be `size_of::<T>()`.
    #[must_use]
    pub fn heap_bytes(&self, per_element_size: usize) -> usize {
        let data_bytes = self.data.capacity() * per_element_size;
        let offsets_bytes = self.offsets.capacity() * size_of::<(u32, u32)>();
        let mut build_bytes = self.build.capacity() * size_of::<Vec<T>>();
        for v in &self.build {
            build_bytes += v.capacity() * per_element_size;
        }
        data_bytes + offsets_bytes + build_bytes
    }

    /// Create a new `FlatVecVec` with `node_count` empty slots (build phase).
    #[must_use]
    pub fn new(node_count: usize) -> Self {
        let mut build = Vec::with_capacity(node_count);
        for _ in 0..node_count {
            build.push(Vec::new());
        }
        Self {
            build,
            data: Vec::new(),
            offsets: Vec::new(),
            orphaned: 0,
            run_compares: WorkCount::new(),
        }
    }

    /// Gives the nodes of `rows` the runs they come with and leaves every other node's run
    /// where it lies: what a cascade over a few nodes of a DOM writes
    /// ([`CssPropertyCache::recascade_nodes`]). In the read phase a row is appended to the flat
    /// storage and its node pointed at it; the run the node had stays in the storage until
    /// more than half of it is such runs, then the storage is compacted (`dedup_runs`). In
    /// the build phase the node's inner `Vec` is replaced. A node outside the store is skipped.
    pub fn replace_rows(&mut self, rows: impl Iterator<Item = (usize, Vec<T>)>)
    where
        T: Clone + PartialEq + core::hash::Hash,
    {
        if self.offsets.is_empty() {
            for (node, row) in rows {
                if let Some(slot) = self.build.get_mut(node) {
                    *slot = row;
                }
            }
            return;
        }
        for (node, row) in rows {
            let Some(slot) = self.offsets.get_mut(node) else {
                continue;
            };
            self.orphaned += slot.1 as usize;
            if row.is_empty() {
                *slot = (0, 0);
                continue;
            }
            let start = u32::try_from(self.data.len()).unwrap_or(u32::MAX);
            let len = u32::try_from(row.len()).unwrap_or(u32::MAX);
            self.data.extend(row);
            *slot = (start, len);
        }
        if self.orphaned > 256 && self.orphaned > self.data.len() / 2 {
            self.dedup_runs();
        }
    }

    /// The store of ANOTHER ARENA: node `n` of it has the run node `source[n]` has here, and
    /// an empty run where `source[n]` is `None` or names no node of this store. What a
    /// rebuild that changed the tree starts its cascade from
    /// ([`CssPropertyCache::spliced`]): the nodes that are what they were keep their runs
    /// under their new numbers.
    ///
    /// No run is copied node by node: the flat storage is cloned once (it holds each distinct
    /// run once, `dedup_runs`) and the new nodes point into it, as the nodes of one rule set
    /// share a run here. The runs no new node points at stay in the storage until they are
    /// more than half of it, then it is compacted - the rule of [`Self::replace_rows`].
    /// `None` in the build phase: there are no runs to point at.
    #[must_use]
    pub fn gathered(&self, source: &[Option<NodeId>]) -> Option<Self>
    where
        T: Clone + PartialEq + core::hash::Hash,
    {
        if self.offsets.is_empty() && !self.build.is_empty() {
            return None;
        }
        let offsets: Vec<(u32, u32)> = source
            .iter()
            .map(|from| {
                from.and_then(|old| self.offsets.get(old.index()).copied())
                    .unwrap_or((0, 0))
            })
            .collect();
        // What the new nodes point at, each distinct run once: runs are equal or disjoint.
        let mut live_runs: Vec<(u32, u32)> =
            offsets.iter().copied().filter(|(_, len)| *len != 0).collect();
        live_runs.sort_unstable();
        live_runs.dedup();
        let live: usize = live_runs.iter().map(|(_, len)| *len as usize).sum();
        let mut store = Self {
            build: Vec::new(),
            data: self.data.clone(),
            offsets,
            orphaned: 0,
            run_compares: self.run_compares.clone(),
        };
        store.orphaned = store.data.len().saturating_sub(live);
        if store.orphaned > 256 && store.orphaned > store.data.len() / 2 {
            store.dedup_runs();
        }
        Some(store)
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
    #[must_use]
    pub fn build_get(&self, node_index: usize) -> Option<&Vec<T>> {
        self.build.get(node_index)
    }

    /// Number of node slots.
    #[inline]
    #[must_use]
    pub const fn len(&self) -> usize {
        if self.offsets.is_empty() {
            self.build.len()
        } else {
            self.offsets.len()
        }
    }

    /// Returns `true` if there are no node slots.
    #[inline]
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns true if this is in read (flattened) mode.
    #[inline]
    #[must_use]
    pub const fn is_flattened(&self) -> bool {
        !self.offsets.is_empty() || self.build.is_empty()
    }

    /// Get a slice for the node at `node_index` (read phase).
    /// Returns empty slice if index is out of bounds or not yet flattened
    /// (falls back to build-phase data if not yet flattened).
    #[inline]
    #[must_use]
    pub fn get_slice(&self, node_index: usize) -> &[T] {
        if self.offsets.is_empty() {
            // Build phase fallback: use inner Vecs
            self.build
                .get(node_index)
                .map_or(&[], alloc::vec::Vec::as_slice)
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

    /// Collapse IDENTICAL per-node runs so nodes with the same property set
    /// share ONE copy of it.
    ///
    /// A stylesheet's declarations repeat across every node they match, and the
    /// flat store held a private copy for each: on the 50k-node XHTML bench,
    /// `css_props` was 165 120 entries carrying EIGHT distinct values, at 144
    /// bytes per entry — 23.8 MB of the same handful of properties written out
    /// over and over.
    ///
    /// Nothing is dropped and no API changes: `get_slice` still returns a real
    /// contiguous slice, several nodes simply point at the same range. That is
    /// sound because no caller can obtain a `&mut` to a node's run — the store
    /// is rebuilt wholesale by `sort_each_and_flatten` / `retain`, never patched
    /// in place.
    ///
    /// The run table is CAPPED: distinct runs are few in practice (a
    /// stylesheet has far fewer rule combinations than nodes), and beyond the
    /// cap new runs are stored without being recorded.
    ///
    /// A node's run is looked for in two steps, neither of which reads the
    /// table: the run the node before it had (nodes of one rule set come one
    /// after the other: the labels of a text style), then the recorded runs
    /// filed under a key of the run's length and its first and last entry.
    /// The table used to be read from its start for every node: nodes x
    /// distinct runs when the rule sets alternate (the cells of a table with
    /// a class per column, a list whose rows cycle through styles). Which run
    /// is found does not depend on the order they are tried in: a run is
    /// recorded once, so the one that equals is the same.
    // the sequence reads in the order the work happens: an `if` expression would hide the early path
    #[allow(clippy::useless_let_if_seq)]
    fn dedup_runs(&mut self)
    where
        T: Clone + PartialEq + core::hash::Hash,
    {
        /// Beyond this many distinct runs, stop recording new ones. Existing
        /// runs are still reused, so the result is partial sharing, never wrong.
        const MAX_RUNS: usize = 1024;

        /// Is the recorded run `(start, len)` of `data` the run `run`? (`compares` counts the
        /// asks.)
        fn same<T: PartialEq>(
            data: &[T],
            (rs, rl): (u32, u32),
            run: &[T],
            compares: &WorkCount,
        ) -> bool {
            compares.bump();
            rl as usize == run.len() && data[rs as usize..(rs + rl) as usize] == *run
        }

        /// What a run is filed under: its length, its first and its last entry.
        fn key_of<T: core::hash::Hash>(run: &[T]) -> u64 {
            use core::hash::{Hash, Hasher};
            let mut h = crate::hash::FastHasher::new();
            run.len().hash(&mut h);
            run.first().hash(&mut h);
            run.last().hash(&mut h);
            h.finish()
        }

        if self.offsets.is_empty() {
            return;
        }
        let mut new_data: Vec<T> = Vec::new();
        let mut runs: Vec<(u32, u32)> = Vec::new();
        // The recorded runs (their indices in `runs`) by `key_of`.
        let mut by_key: alloc::collections::BTreeMap<u64, Vec<u32>> =
            alloc::collections::BTreeMap::new();
        // The recorded run the node before this one had.
        let mut last: Option<usize> = None;
        let mut new_offsets = Vec::with_capacity(self.offsets.len());

        for &(start, len) in &self.offsets {
            if len == 0 {
                new_offsets.push((0, 0));
                continue;
            }
            let s = start as usize;
            let run = &self.data[s..s + len as usize];
            let mut existing =
                last.filter(|at| same(&new_data, runs[*at], run, &self.run_compares));
            let mut key = None;
            if existing.is_none() {
                let k = key_of(run);
                existing = by_key.get(&k).and_then(|filed| {
                    filed
                        .iter()
                        .map(|at| *at as usize)
                        .find(|at| {
                            Some(*at) != last
                                && same(&new_data, runs[*at], run, &self.run_compares)
                        })
                });
                key = Some(k);
            }
            if let Some(at) = existing {
                new_offsets.push(runs[at]);
                last = Some(at);
                continue;
            }
            let ns = u32::try_from(new_data.len()).unwrap_or(u32::MAX);
            new_data.extend_from_slice(run);
            last = None;
            if runs.len() < MAX_RUNS {
                if let Some(k) = key {
                    by_key
                        .entry(k)
                        .or_default()
                        .push(u32::try_from(runs.len()).unwrap_or(u32::MAX));
                }
                last = Some(runs.len());
                runs.push((ns, len));
            }
            new_offsets.push((ns, len));
        }

        self.data = new_data;
        self.offsets = new_offsets;
        self.orphaned = 0;
    }

    /// Flatten: sort each inner Vec by key, deduplicate by keeping the last
    /// occurrence of each key (CSS cascade: later source order wins among
    /// equal specificity), then compact into flat storage.
    ///
    /// Drains all build-phase Vecs. After this call, only `get_slice()` works.
    pub fn sort_each_and_flatten<K: Ord + Eq>(&mut self, key_fn: impl Fn(&T) -> K)
    where
        T: Clone + PartialEq + core::hash::Hash,
    {
        let node_count = self.build.len();
        let total: usize = self.build.iter().map(alloc::vec::Vec::len).sum();

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

            let start = u32::try_from(flat_data.len()).unwrap_or(u32::MAX);
            // Drain inner and push only kept items
            for (i, item) in inner.drain(..).enumerate() {
                if keep[i] {
                    flat_data.push(item);
                }
            }

            let len = u32::try_from(flat_data.len()).unwrap_or(u32::MAX) - start;
            offsets.push((start, len));
        }

        flat_data.shrink_to_fit();
        self.data = flat_data;
        self.offsets = offsets;
        self.build = Vec::new();
        self.dedup_runs();
    }

    /// Flatten without sorting (for data that's already sorted).
    pub fn flatten(&mut self) {
        let node_count = self.build.len();
        let total: usize = self.build.iter().map(alloc::vec::Vec::len).sum();

        let mut flat_data = Vec::with_capacity(total);
        let mut offsets = Vec::with_capacity(node_count);

        for inner in &mut self.build {
            let start = u32::try_from(flat_data.len()).unwrap_or(u32::MAX);
            let len = u32::try_from(inner.len()).unwrap_or(u32::MAX);
            offsets.push((start, len));
            flat_data.append(inner);
        }

        self.data = flat_data;
        self.offsets = offsets;
        self.build = Vec::new();
    }

    /// Rebuild flat storage, keeping only items matching `predicate`.
    /// Must be called after flatten. Preserves per-node ordering.
    pub fn retain(&mut self, predicate: impl Fn(&T) -> bool)
    where
        T: Clone + PartialEq + core::hash::Hash,
    {
        if self.offsets.is_empty() {
            return;
        }
        let node_count = self.offsets.len();
        let mut new_data = Vec::new();
        let mut new_offsets = Vec::with_capacity(node_count);
        for &(start, len) in &self.offsets {
            let s = start as usize;
            let l = len as usize;
            let new_start = u32::try_from(new_data.len()).unwrap_or(u32::MAX);
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
        self.dedup_runs();
    }

    /// Return to build phase from read (flattened) phase, preserving all data, so
    /// `push_to` / `build_mut` work again. No-op if already in build phase.
    ///
    /// The build → flatten → read progression is otherwise one-way, but `restyle()`
    /// legitimately runs more than once on the same cache (`StyledDom::create` does one
    /// internal pass, then the public API may do more), and building the compact cache
    /// flattens these vecs in between. Without re-entering build phase, the next
    /// restyle's `push_to` / `build_mut` would index an emptied `build` and panic.
    pub fn ensure_build_phase(&mut self)
    where
        T: Clone,
    {
        if self.offsets.is_empty() {
            return; // already in build phase (or wholly empty)
        }
        let mut build = Vec::with_capacity(self.offsets.len());
        for &(start, len) in &self.offsets {
            let s = start as usize;
            let l = len as usize;
            build.push(self.data[s..s + l].to_vec());
        }
        self.build = build;
        self.data = Vec::new();
        self.offsets = Vec::new();
    }

    /// Like `retain`, but passes each item's owning node index to the predicate.
    /// Must be called after flatten. Preserves per-node ordering.
    pub fn retain_with_node_index(&mut self, predicate: impl Fn(usize, &T) -> bool)
    where
        T: Clone,
    {
        if self.offsets.is_empty() {
            return;
        }
        let node_count = self.offsets.len();
        let mut new_data = Vec::new();
        let mut new_offsets = Vec::with_capacity(node_count);
        for (node_idx, &(start, len)) in self.offsets.iter().enumerate() {
            let s = start as usize;
            let l = len as usize;
            let new_start = u32::try_from(new_data.len()).unwrap_or(u32::MAX);
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
            let base = u32::try_from(self.data.len()).unwrap_or(u32::MAX);
            self.data.append(&mut other.data);
            self.offsets
                .extend(other.offsets.drain(..).map(|(s, l)| (s + base, l)));
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

/// The slow path's inherited-value store, TRANSPOSED: one copy of each distinct
/// value plus the nodes that resolve to it.
///
/// The per-node shape (`Vec<Vec<(CssPropertyType, CssPropertyWithOrigin)>>`)
/// paid for the VALUE once per node, and `CssProperty` is a 136-byte enum held
/// inline. Inherited values are shared down a subtree by definition, so that is
/// the worst possible layout for them: on the 50k-node XHTML bench the store
/// held 53 476 entries carrying just EIGHT distinct values — `cursor` alone was
/// 29 391 entries of one value, re-paid in every descendant of the node that
/// set it.
///
/// Transposed, the same content is 215 KB instead of 8.1 MB (38x). The lookup
/// stays cheap because the bucket count is tiny: a linear scan over ~6 property
/// types beats hashing, and the node list is sorted (nodes are cascaded in
/// increasing index order, so it is built by `push`) for a binary search.
///
/// INVARIANT: every bucket's `prop_type.is_inheritable()` is true — this store
/// is only ever written through [`CssPropertyCache::store_if_changed`], which
/// filters. Non-inherited properties live in `cascaded_props` / `css_props` /
/// the compact cache.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct InheritedValues {
    buckets: Vec<InheritedBucket>,
    /// Node count this store was sized for; `Vec<Vec<_>>::len()` used to answer
    /// this and several callers (and tests) still ask.
    node_count: usize,
    /// The buckets by the hash of their (type, value) ([`Self::bucket_hash`]),
    /// in the order they were made: where a value's bucket is looked for
    /// ([`Self::bucket_of`]). A map view has a bucket per text style and
    /// inherited property; reading all of them for every value of every
    /// node was nodes times distinct values.
    by_hash: alloc::collections::BTreeMap<u64, Vec<u32>>,
    /// The buckets of each property type, in the order they were made: what
    /// [`Self::get`] searches. It used to pass over every bucket of the
    /// store to find the ones of its type - 150 to 200 buckets on a map with
    /// 26 text styles, for a property (`user-select`, `direction`) that has
    /// one or two.
    by_type: alloc::collections::BTreeMap<CssPropertyType, Vec<u32>>,
    /// How many buckets this store looked at to read or place values (`values_for`: all of
    /// them; placing a value: its parent's, else the ones under its hash): what the size sweep
    /// of the tests reads to say the inheritance pass does not look at every distinct value of
    /// the DOM for every node.
    pub(crate) bucket_reads: WorkCount,
    /// How many buckets [`Self::get`] searched.
    pub(crate) get_reads: WorkCount,
}

#[derive(Debug, Clone, PartialEq)]
struct InheritedBucket {
    prop_type: CssPropertyType,
    value: CssPropertyWithOrigin,
    /// Node indices with this value, ascending.
    nodes: Vec<u32>,
}

impl InheritedValues {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            buckets: Vec::new(),
            node_count: 0,
            by_hash: alloc::collections::BTreeMap::new(),
            by_type: alloc::collections::BTreeMap::new(),
            bucket_reads: WorkCount::new(),
            get_reads: WorkCount::new(),
        }
    }

    /// Takes over the work `previous` (the store this one replaces) had counted: the cascade
    /// starts each pass with an empty store, and what a pass read stays counted with the DOM.
    fn carry_work_from(&self, previous: &Self) {
        self.bucket_reads.add(previous.bucket_reads.get());
        self.get_reads.add(previous.get_reads.get());
    }

    /// The key a (type, value) is kept under in `by_hash`.
    fn bucket_hash(prop_type: CssPropertyType, value: &CssPropertyWithOrigin) -> u64 {
        use core::hash::{Hash, Hasher};
        let mut hasher = crate::hash::FastHasher::new();
        prop_type.hash(&mut hasher);
        value.property.hash(&mut hasher);
        value.origin.hash(&mut hasher);
        hasher.finish()
    }

    /// The bucket of (type, value), if there is one (there is at most one).
    fn bucket_of(&self, prop_type: CssPropertyType, value: &CssPropertyWithOrigin) -> Option<usize> {
        let candidates = self.by_hash.get(&Self::bucket_hash(prop_type, value))?;
        candidates.iter().map(|b| *b as usize).find(|b| {
            self.bucket_reads.bump();
            self.buckets
                .get(*b)
                .is_some_and(|bucket| bucket.prop_type == prop_type && bucket.value == *value)
        })
    }

    /// Adds `bucket` as the newest one and answers its index.
    fn push_bucket(&mut self, bucket: InheritedBucket) -> usize {
        let index = self.buckets.len();
        self.by_hash
            .entry(Self::bucket_hash(bucket.prop_type, &bucket.value))
            .or_default()
            .push(u32::try_from(index).unwrap_or(u32::MAX));
        self.by_type
            .entry(bucket.prop_type)
            .or_default()
            .push(u32::try_from(index).unwrap_or(u32::MAX));
        self.buckets.push(bucket);
        index
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.node_count
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.node_count == 0
    }

    /// Number of distinct (type, value) buckets — the figure the scan cost is
    /// linear in. Exposed for the memory/coverage tests.
    #[must_use]
    pub const fn bucket_count(&self) -> usize {
        self.buckets.len()
    }

    /// Every distinct `(type, value)` of the store with the nodes that resolve to it
    /// (ascending): the store as it lies. For a reader that compares the values of many
    /// nodes of two stores: [`Self::values_for`] reads every bucket for ONE node, so asking
    /// it per node is nodes times distinct values.
    pub fn buckets(
        &self,
    ) -> impl Iterator<Item = (CssPropertyType, &CssPropertyWithOrigin, &[u32])> + '_ {
        self.buckets
            .iter()
            .map(|b| (b.prop_type, &b.value, b.nodes.as_slice()))
    }

    /// Total (node, property) pairs — what the old per-node vecs would have
    /// stored one entry each for.
    #[must_use]
    pub fn entry_count(&self) -> usize {
        self.buckets.iter().map(|b| b.nodes.len()).sum()
    }

    pub fn grow_to(&mut self, node_count: usize) {
        self.node_count = self.node_count.max(node_count);
    }

    /// Drop every value, keeping the sizing. Called at the head of a cascade.
    pub fn clear_values(&mut self) {
        self.buckets.clear();
        self.by_hash.clear();
        self.by_type.clear();
    }

    /// The resolved value of `prop_type` for `node`, if any.
    ///
    /// Over the buckets of the type, in the order they were made (the first
    /// that holds the node, as the scan of all buckets found it), then
    /// binary over the node list.
    #[must_use]
    pub fn get(&self, node: usize, prop_type: CssPropertyType) -> Option<&CssPropertyWithOrigin> {
        let n = u32::try_from(node).ok()?;
        self.by_type
            .get(&prop_type)?
            .iter()
            .filter_map(|index| self.buckets.get(*index as usize))
            .find(|b| {
                self.get_reads.bump();
                b.nodes.binary_search(&n).is_ok()
            })
            .map(|b| &b.value)
    }

    /// Every (type, value) this node resolved to, ascending by type — the shape
    /// the per-node `Vec` used to hand out.
    #[must_use]
    pub fn values_for(&self, node: usize) -> Vec<(CssPropertyType, CssPropertyWithOrigin)> {
        let Ok(n) = u32::try_from(node) else {
            return Vec::new();
        };
        self.bucket_reads.add(self.buckets.len() as u64);
        let mut out: Vec<_> = self
            .buckets
            .iter()
            .filter(|b| b.nodes.binary_search(&n).is_ok())
            .map(|b| (b.prop_type, b.value.clone()))
            .collect();
        out.sort_by_key(|(t, _)| *t);
        out
    }

    /// `values_for`, as an `Option` that is `None` when the node resolved to
    /// nothing — the shape `Vec<Vec<_>>::get` had, so `if let Some(..)` call
    /// sites keep their structure.
    ///
    /// `None` iff `node` is outside the store; a node that exists but resolved
    /// no inherited properties yields `Some(vec![])`.
    ///
    /// This used to key off emptiness (`(!v.is_empty()).then_some(v)`), which
    /// made "this node has nothing" and "there is no such node"
    /// indistinguishable. That was harmless while every node held a value, but
    /// the store now keeps only properties a reader can actually reach, so a
    /// plain `<div>` with no inherited CSS legitimately has none - and the
    /// caller could no longer tell that from an out-of-range index.
    #[must_use]
    pub fn values_for_opt(
        &self,
        node: usize,
    ) -> Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>> {
        (node < self.node_count).then(|| self.values_for(node))
    }

    /// Record `entries` as `node`'s resolved set.
    ///
    /// Nodes are cascaded in ascending index order, so each `nodes` list is
    /// built by `push` and stays sorted without an insert.
    pub fn set_node(&mut self, node: usize, entries: &[(CssPropertyType, CssPropertyWithOrigin)]) {
        drop(self.set_node_near(node, entries, None));
    }

    /// [`Self::set_node`], answering the bucket each entry went to (in the
    /// order of `entries`), and looking for an entry's bucket first where
    /// `near` has its type: `near` is another node's entries (ascending by
    /// type) with the buckets they went to - the node's parent's. A value a
    /// node inherits unchanged is in its parent's bucket, so most entries
    /// are placed without reading the other buckets; an entry `near` does
    /// not place is found by its hash ([`Self::bucket_of`]). A (type, value)
    /// has one bucket, so both ways find the same one - the one a scan of
    /// all buckets from the first found.
    pub fn set_node_near(
        &mut self,
        node: usize,
        entries: &[(CssPropertyType, CssPropertyWithOrigin)],
        near: Option<(&[(CssPropertyType, CssPropertyWithOrigin)], &[u32])>,
    ) -> Vec<u32> {
        let Ok(n) = u32::try_from(node) else {
            return Vec::new();
        };
        let mut placed = Vec::with_capacity(entries.len());
        for (prop_type, value) in entries {
            let hinted = near.and_then(|(near_entries, near_buckets)| {
                let at = near_entries
                    .binary_search_by_key(prop_type, |(t, _)| *t)
                    .ok()?;
                let bucket = *near_buckets.get(at)? as usize;
                let b = self.buckets.get(bucket)?;
                self.bucket_reads.bump();
                (b.prop_type == *prop_type && b.value == *value).then_some(bucket)
            });
            let found = hinted.or_else(|| self.bucket_of(*prop_type, value));
            let bucket = match found {
                Some(bucket) => {
                    let b = &mut self.buckets[bucket];
                    if b.nodes.last() != Some(&n) {
                        b.nodes.push(n);
                    }
                    bucket
                }
                None => self.push_bucket(InheritedBucket {
                    prop_type: *prop_type,
                    value: value.clone(),
                    nodes: vec![n],
                }),
            };
            placed.push(u32::try_from(bucket).unwrap_or(u32::MAX));
        }
        placed
    }

    /// Takes the nodes `gone` names (`gone[node]`) out of every value's node list: what a
    /// cascade over a few nodes of a DOM does before it places their new values
    /// ([`Self::place_node`]). The lists stay ascending; a value no node holds any more keeps
    /// its (empty) bucket, which no lookup finds a node in.
    pub fn remove_nodes(&mut self, gone: &[bool]) {
        for bucket in &mut self.buckets {
            bucket
                .nodes
                .retain(|node| !gone.get(*node as usize).copied().unwrap_or(false));
        }
    }

    /// [`Self::set_node_near`] for a node that is placed out of turn - a node of a DOM whose
    /// other nodes keep their values: its index goes into each value's node list at its
    /// place, so the lists stay ascending and every lookup (a binary search) still finds the
    /// nodes behind it. The node must hold no value yet ([`Self::remove_nodes`]).
    pub fn place_node(
        &mut self,
        node: usize,
        entries: &[(CssPropertyType, CssPropertyWithOrigin)],
        near: Option<(&[(CssPropertyType, CssPropertyWithOrigin)], &[u32])>,
    ) -> Vec<u32> {
        let Ok(n) = u32::try_from(node) else {
            return Vec::new();
        };
        let mut placed = Vec::with_capacity(entries.len());
        for (prop_type, value) in entries {
            let hinted = near.and_then(|(near_entries, near_buckets)| {
                let at = near_entries
                    .binary_search_by_key(prop_type, |(t, _)| *t)
                    .ok()?;
                let bucket = *near_buckets.get(at)? as usize;
                let b = self.buckets.get(bucket)?;
                (b.prop_type == *prop_type && b.value == *value).then_some(bucket)
            });
            let found = hinted.or_else(|| self.bucket_of(*prop_type, value));
            let bucket = match found {
                Some(bucket) => {
                    let b = &mut self.buckets[bucket];
                    if let Err(at) = b.nodes.binary_search(&n) {
                        b.nodes.insert(at, n);
                    }
                    bucket
                }
                None => self.push_bucket(InheritedBucket {
                    prop_type: *prop_type,
                    value: value.clone(),
                    nodes: vec![n],
                }),
            };
            placed.push(u32::try_from(bucket).unwrap_or(u32::MAX));
        }
        placed
    }

    /// The store of ANOTHER ARENA of `node_count` nodes: old node `o` is new node
    /// `new_of_old[o]` there, and an old node without a new one (`None`, or past the end of
    /// `new_of_old`) holds no value in it ([`CssPropertyCache::spliced`]). Every value keeps
    /// its bucket and the bucket its index, so the two indexes of the buckets stand as they
    /// are; a bucket whose nodes all went is empty, which no lookup finds a node in. The
    /// node lists are sorted again: the new numbers need not come in the order of the old.
    #[must_use]
    pub fn remapped(&self, new_of_old: &[Option<usize>], node_count: usize) -> Self {
        let buckets = self
            .buckets
            .iter()
            .map(|bucket| {
                let mut nodes: Vec<u32> = bucket
                    .nodes
                    .iter()
                    .filter_map(|old| new_of_old.get(*old as usize).copied().flatten())
                    .filter(|new| *new < node_count)
                    .filter_map(|new| u32::try_from(new).ok())
                    .collect();
                nodes.sort_unstable();
                nodes.dedup();
                InheritedBucket {
                    prop_type: bucket.prop_type,
                    value: bucket.value.clone(),
                    nodes,
                }
            })
            .collect();
        Self {
            buckets,
            node_count,
            by_hash: self.by_hash.clone(),
            by_type: self.by_type.clone(),
            bucket_reads: self.bucket_reads.clone(),
            get_reads: self.get_reads.clone(),
        }
    }

    /// Absorb another store whose nodes sit after this one's (subtree
    /// composition), shifting its node indices by this store's node count.
    pub fn append(&mut self, other: &mut Self) {
        let shift = u32::try_from(self.node_count).unwrap_or(u32::MAX);
        other.by_hash.clear();
        other.by_type.clear();
        for mut ob in core::mem::take(&mut other.buckets) {
            for n in &mut ob.nodes {
                *n = n.saturating_add(shift);
            }
            match self.bucket_of(ob.prop_type, &ob.value) {
                Some(b) => self.buckets[b].nodes.extend_from_slice(&ob.nodes),
                None => {
                    self.push_bucket(ob);
                }
            }
        }
        self.node_count += other.node_count;
        other.node_count = 0;
        self.carry_work_from(other);
    }

    /// Approximate retained heap bytes: one value per bucket plus the node ids.
    #[must_use]
    pub fn heap_bytes(&self) -> usize {
        let entry = size_of::<(CssPropertyType, CssPropertyWithOrigin)>();
        self.buckets.capacity() * size_of::<InheritedBucket>()
            + self.buckets.len() * entry
            + self
                .buckets
                .iter()
                .map(|b| b.nodes.capacity() * size_of::<u32>())
                .sum::<usize>()
    }
}

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
/// Whether the document a [`CssPropertyCache`] styles is being laid out on PAGED media with
/// footnotes in it (`float: footnote`, CSS Generated Content for Paged Media).
///
/// The layout pass says so before it reads any style, and the readers of `position` and `float`
/// treat a footnote as the out-of-flow box it is there and as `float: none` everywhere else.
///
/// It is a fact about the presentation, not about the cascade, and it is
/// the layout that knows it - through a shared reference to the document.
/// Hence the atomic, and why two caches compare equal whatever it says. A
/// document must not be laid out on paged and on continuous media at the
/// same time from two threads: the one flag would be right for one of them.
#[derive(Debug, Default)]
pub struct PagedFootnotes(core::sync::atomic::AtomicBool);

impl PagedFootnotes {
    /// Whether the last layout pass was a paged one of a document with
    /// footnotes.
    #[inline]
    #[must_use]
    pub fn get(&self) -> bool {
        self.0.load(core::sync::atomic::Ordering::Relaxed)
    }

    /// Set by the layout pass, before it reads any style.
    #[inline]
    pub fn set(&self, paged_with_footnotes: bool) {
        self.0
            .store(paged_with_footnotes, core::sync::atomic::Ordering::Relaxed);
    }
}

impl Clone for PagedFootnotes {
    fn clone(&self) -> Self {
        Self(core::sync::atomic::AtomicBool::new(self.get()))
    }
}

impl PartialEq for PagedFootnotes {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Debug, Default, Clone, PartialEq)]
// plain flags, set and read one by one: a state enum would not be simpler
#[allow(clippy::struct_excessive_bools)]
pub struct CssPropertyCache {
    // number of nodes in the current DOM
    pub node_count: usize,

    /// See [`PagedFootnotes`].
    pub paged_footnotes: PagedFootnotes,

    // The author stylesheet this cache was last cascaded with. Retained so nodes
    // inserted at runtime can be re-styled (`StyledDom::restyle_retained`) — the
    // cascade runs once at creation, and without the rules an inserted node could
    // only ever receive UA defaults + inheritance, never its author CSS.
    // (This struct lives behind `CssPropertyCachePtr`, so the field is invisible
    // to the C ABI.)
    pub retained_author_css: Css,

    // properties that were overridden in callbacks (not specific to any node state)
    pub user_overridden_properties: Vec<Vec<(CssPropertyType, CssProperty)>>,
    /// Some node has, or had, a runtime override of `color` in
    /// [`Self::user_overridden_properties`]. That is the one override a
    /// node's DESCENDANTS take through inheritance while the precomputed
    /// tables cannot see it, so a reader of the used colour walks to the
    /// root for it (`inherited_color_override` in the layout crate) - once
    /// ANY override exists, were it only a bound `transform`, unless this
    /// says no node ever had one of `color`. Raised by every writer of the
    /// table (`StyledDom::set_user_property_override_fast`,
    /// `StyledDom::migrate_user_overrides_from`, [`Self::append`]) and never
    /// lowered: `false` means there is none.
    pub text_color_override_seen: bool,
    /// Another styled DOM was appended to this one
    /// (`StyledDom::append_child`) and the inheritance tables and the
    /// compact cache have not been rebuilt for the composed tree since
    /// (`StyledDom::recompute_inheritance_and_compact_cache`): what the
    /// appended subtree inherits and its compact entries are still the ones
    /// of its own, isolated cascade. A DOM that came out of one cascade has
    /// both right and this `false`.
    pub composed_since_recompute: bool,
    /// The window's dynamic-selector context (viewport size, theme, OS,
    /// media type...), provided by the layout funnel before the first
    /// layout. `None` = context UNKNOWN (a freshly created `StyledDom` that no
    /// window has adopted yet): non-pseudo-state conditions then evaluate to
    /// "does not apply", which is the same behaviour they always had before
    /// contexts were wired through. Pseudo-state conditions never depend on
    /// this field.
    pub dynamic_context: Option<Box<DynamicSelectorContext>>,

    /// Whether [`Self::apply_ua_css`] has run on this cache at least once.
    ///
    /// Read by the re-application path: the UA defaults it pushed last time
    /// were answered under the context of THAT time (the theme, above all),
    /// so a second application first strips every `ua_origin` entry and
    /// re-answers them. Also what the paint-time fallbacks assert against: a
    /// cache the UA pass has run on carries the inherited text colour in its
    /// resolved style, so a reader falling back to a constant is a bug there
    /// and merely "no cascade yet" here.
    pub ua_applied: bool,

    /// The CASCADE GENERATION: bumped by everything that changes what a node's
    /// resolved style answers — a restyle, a UA/inheritance re-cascade, a
    /// compact-cache rebuild, a context change (`set_dynamic_selector_context`),
    /// a user override written through `restyle_user_property`, a DOM merge.
    ///
    /// Caches that serve PAINTED output key on it (the display-list cache via
    /// `dl_input_fingerprint`, the per-pass `StyleCache`, the DL patch gate):
    /// colour is deliberately excluded from the relayout hash, so without this
    /// two cascades of one DOM under two themes (or before/after a `color`
    /// override) hashed identically and the cache served the other one's
    /// list. One monotonic counter replaces the three compensations that grew
    /// for that (`paint_defaults_fingerprint`, the `last_dynamic_context`
    /// equality gate, `reset_incremental` on a style change) — theme-chain
    /// analysis 2026-09-12, R4/I5.
    ///
    /// Custom properties ride it too (design §9.1 pitfall 11): every
    /// `var()` is resolved by the restyle, under the context, so a variable
    /// that follows the mode or a rice edit is a new generation like any
    /// other cascade input - no per-variable cache key to forget.
    ///
    /// NOT bumped by `set_user_property_override_fast`: that is the per-tick
    /// animation channel, whose caller patches the display list with the
    /// interpolated pixels itself; bumping there would miss the DL cache on
    /// every tick of every transition. Only equality is ever compared, so
    /// wrapping is fine.
    pub cascade_epoch: u64,

    // non-default CSS properties that were cascaded from the parent,
    // unified across all pseudo-states (Normal, Hover, Active, Focus, Dragging, DragOver).
    // Stored in a flat cache-friendly layout after sort_and_flatten().
    pub cascaded_props: FlatVecVec<StatefulCssProperty>,

    // non-default CSS properties that were set via a CSS file,
    // unified across all pseudo-states.
    pub css_props: FlatVecVec<StatefulCssProperty>,

    /// Pre-resolved INHERITABLE properties, sorted per node and keyed by
    /// `CssPropertyType`.
    ///
    /// INVARIANT: every entry's `prop_type.is_inheritable()` is true — enforced
    /// at the single write site, `store_if_changed`. Non-inherited properties
    /// live in `cascaded_props` / `css_props` / the compact cache; storing them
    /// here too made this 80% unreachable duplicate (see `store_if_changed`).
    pub computed_values: InheritedValues,

    // Compact layout cache: three-tier numeric encoding for O(1) layout lookups.
    // Built once after restyle + apply_ua_css + compute_inherited_values.
    // Non-compact properties (background, shadow, transform) use get_property_slow().
    pub compact_cache: Option<azul_css::compact_cache::CompactLayoutCache>,

    // Global CSS properties from `*` rules — shared across all nodes.
    // Applied during build_compact_cache_with_inheritance instead of being
    // cloned into each node's css_props (saves 50K×N clones).
    pub global_css_props: Vec<CssProperty>,

    /// Every node's custom properties (`--name`) as the last restyle
    /// computed them under the window's context; see
    /// [`crate::custom_property_cascade`]. Empty for a DOM without variables.
    pub custom_property_envs: crate::custom_property_cascade::CustomPropertyEnvs,

    /// Per node (sparse): the node's OWN `var()` / `env()` declarations as the
    /// last restyle resolved them. Every cascade reader of inline style goes
    /// through [`Self::inline_properties`], which reads this.
    pub resolved_inline:
        alloc::collections::BTreeMap<usize, crate::custom_property_cascade::ResolvedInline>,

    /// Whether the last restyle's variables depend on the window context
    /// beyond the author rules' own conditions: a node's own definition under
    /// a condition. `StyledDom::set_dynamic_selector_context` re-runs the
    /// cascade then, even with an empty author stylesheet - as it does for a
    /// node's own `env()` ([`Self::env_readers`]).
    pub variables_depend_on_context: bool,

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
    /// Per-node EFFECTIVE `zoom` (the product of `zoom` on the node and on
    /// every ancestor) for the `Normal` pseudo-state, filled lazily by the
    /// layout (`solver3::getters::get_effective_zoom`) in one top-down walk.
    /// EMPTY when no node declares a zoom other than 1 - every lookup is
    /// then 1.0 without an index. Cleared with the font sizes.
    pub resolved_zooms: crate::sync::OnceLock<Vec<f32>>,

    /// What the ROOT of this DOM inherits from OUTSIDE it: the computed
    /// inheritable values of the node that hosts the DOM - a `VirtualView`'s
    /// host ([`Self::inherited_values_for_hosted_dom`]), sorted by type, each
    /// `CssPropertyOrigin::Inherited`. EMPTY for a document of its own (the
    /// layout callback's DOM, a popup, a measured DOM with no host).
    ///
    /// A view is a virtualized part of the SAME document, not an iframe: its
    /// root inherits from its host as any element inherits from its parent
    /// (CSS Cascade 4 s7). Every cascade stage reads it where it reads a
    /// parent: the inheritance walk of [`Self::restyle`] (the root's
    /// `cascaded_props`, so a text node's UA I-beam yields to a host's
    /// `cursor` as to a parent's), [`Self::compute_inherited_values`] (the
    /// root's parent values) and the compact builder (the root's parent
    /// slots). The document-wide UA defaults of the root
    /// (`ua_css::get_ua_root_property_themed`, the text colour) stand in
    /// for what a document root inherits from nothing, so they yield to a
    /// type the host supplies (`takes_root_ua_default`). The DOM's
    /// own declarations beat all of it, as they beat what any element
    /// inherits. Set before the first cascade
    /// (`StyledDom::create_from_dom_inheriting`) or re-seeded with a restyle
    /// (`StyledDom::set_inherited_from_host`) when the host's values move.
    pub inherited_from_host: Vec<(CssPropertyType, CssPropertyWithOrigin)>,

    /// The states the last restyle's rules name LEFT of their subject
    /// (`.btn:hover .ink`), one `style::state_bit` each. Such a rule holds by
    /// the state its anchor was in when the cascade ran, so a state change
    /// re-runs the cascade - only for a state set here, which keeps every
    /// other hover or focus change free (`StyledDom::restyle_nodes_state`).
    /// A DOM merge takes the union: the merged retained stylesheet holds
    /// both halves' rules (`append`).
    pub left_state_mask: u32,

    /// Per node (sparse): the `(state, property)` pairs a stylesheet rule
    /// ABOVE `rule_priority::INLINE` set for it (a rice at `widgets`, a
    /// `force` sheet). The node's own declarations of such a property lose
    /// to that rule, as a higher cascade origin wins; [`Self::inline_properties`]
    /// leaves them out, so every reader of inline style agrees. A rule for
    /// the resting state overrules the property's inline declarations of
    /// every state, a rule for a state only that state's.
    pub inline_overruled:
        alloc::collections::BTreeMap<usize, Vec<(azul_css::dynamic_selector::PseudoStateType, CssPropertyType)>>,

    /// Per node and property (sparse): the value a CSS-wide keyword the node declares stands
    /// for, where that is not the property's initial value as every reader takes a keyword -
    /// the parent's value for `inherit` (and `unset` of a property that inherits), the user
    /// agent's for `revert`, `inline` for `display: initial`. [`Self::get_property_slow`]
    /// answers it in the keyword's place and the compact cache encodes it. Built in pre-order
    /// by [`Self::compute_inherited_values`]; empty for a DOM without such keywords.
    pub css_wide_values: alloc::collections::BTreeMap<
        (usize, CssPropertyType, azul_css::css::CssWideKeyword),
        CssProperty,
    >,

    /// The nodes the last restyle gave a value from the window's environment
    /// (an `env()`, a `system:` length), so a new environment lays out only
    /// them again.
    pub env_readers: EnvReaders,
    /// How many times `generate_tag_ids` walked this cache's DOM: what a test reads to say
    /// styling a DOM walks it once.
    pub(crate) tag_id_passes: WorkCount,
    /// How many nodes the walks that prepared the `Dom` this cache was made for visited
    /// (`scope_inline_css`, `take_css_from_dom`): what a test reads to say each node is visited
    /// once per walk.
    pub(crate) dom_prepare_steps: WorkCount,
    /// How much selector matching the cascades of this cache did: what a test reads to say a
    /// restyle tries each node against the rules around it only.
    pub(crate) match_work: crate::style::MatchWork,
    /// The cascade walks by property type, counted under `AZ_PROFILE=cascade`
    /// ([`Self::drain_css_prop_counts`]).
    pub(crate) prop_walks: PropWalks,
}

/// The nodes whose style reads the window's environment.
///
/// That is an `env()` or a `system:` length, which the cascade resolves
/// against the window's context. A new value changes what such a node
/// computes while its declarations, its data and its states stay the same,
/// so nothing the layout compares sees it: the window hands these nodes to
/// the layout as restyled (`LayoutWindow`'s css dirt) when the environment
/// moves.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct EnvReaders {
    /// Each node with the widest relayout a new value of its properties asks
    /// for.
    pub nodes: alloc::collections::BTreeMap<usize, azul_css::props::property::RelayoutScope>,
    /// A value reaches further than [`Self::nodes`]: a `*` rule's reaches
    /// every node, an inherited property's the node's descendants.
    pub beyond_nodes: bool,
    /// A value follows the viewport's size (`env(titlebar-area-width)`), so a
    /// resize can move it.
    pub follows_viewport: bool,
}

impl EnvReaders {
    /// Notes that `node` (`None`: every node, a `*` rule) takes
    /// `declaration`'s value, if that comes from the environment.
    pub fn note(&mut self, node: Option<usize>, declaration: &azul_css::css::CssDeclaration) {
        let azul_css::css::CssDeclaration::Dynamic(d) = declaration else {
            return;
        };
        if !d.is_env() {
            return;
        }
        self.follows_viewport |= d.env_follows_viewport();
        let ty = d.default_value.get_type();
        match node {
            Some(node) if !ty.is_inheritable() => {
                let scope = self.nodes.entry(node).or_default();
                *scope = (*scope).max(ty.relayout_scope(true));
            }
            _ => self.beyond_nodes = true,
        }
    }

    /// Takes `other`'s readers, whose nodes come `offset` after this DOM's.
    pub fn append(&mut self, other: &mut Self, offset: usize) {
        self.nodes.extend(
            core::mem::take(&mut other.nodes)
                .into_iter()
                .map(|(node, scope)| (node + offset, scope)),
        );
        self.beyond_nodes |= other.beyond_nodes;
        self.follows_viewport |= other.follows_viewport;
    }
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
    #[must_use]
    pub const fn total_bytes(&self) -> usize {
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
    /// `AZ_PROFILE=memory` reporter. Sums capacity × element size
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
        let computed_entry_sz = size_of::<(CssPropertyType, CssPropertyWithOrigin)>();
        let outer_vec_sz = size_of::<Vec<(CssPropertyType, CssPropertyWithOrigin)>>();

        let cascaded_bytes = self.cascaded_props.heap_bytes(stateful_sz);
        let css_bytes = self.css_props.heap_bytes(stateful_sz);

        let computed_bytes = self.computed_values.heap_bytes();

        let user_overridden_bytes = {
            let mut b = self.user_overridden_properties.capacity() * outer_vec_sz;
            for v in &self.user_overridden_properties {
                b += v.capacity() * size_of::<(CssPropertyType, CssProperty)>();
            }
            b
        };

        let global_bytes = self.global_css_props.capacity() * size_of::<CssProperty>();

        let compact_bytes = self.compact_cache.as_ref().map_or(0, |c| {
            c.tier1_enums.capacity() * 8
                + c.tier2_dims.capacity()
                    * size_of::<azul_css::compact_cache::CompactNodeProps>()
                + c.tier2_cold.capacity() * 28
                + c.tier2b_text.capacity()
                    * size_of::<azul_css::compact_cache::CompactTextProps>()
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
            if crate::profile::memory_enabled() {
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
                        if p.state == PseudoStateType::Normal && p.prop_type.has_compact_encoding()
                        {
                            casc_normal_compact += 1;
                        }
                    }
                }
                eprintln!(
                    "[PRUNE] css_props: norm+compact={normal_compact} \
                     norm+other={normal_noncompact} nonnorm={nonnormal} SSP={ssp_sz}B | cascaded: \
                     total={casc_total} norm+compact={casc_normal_compact}"
                );
            }
        }

        // The compact cache stores SENTINEL for pixel-valued properties whose inner
        // value is Exact with a non-px metric (vh, vw, %, em, rem, calc(), ...).
        // Those need the slow `css_props` walk at layout time because the compact
        // cache has nothing usable. We must keep them here or the slow path falls
        // back to UA CSS and silently clobbers the author's rule.
        let keep = survives_the_prune;
        // css_props is STILL NOT PRUNED, but not for the reason recorded here
        // before. That reason — "regenerate_layout rebuilds the compact cache
        // from css_props every frame" (#8: white-space:pre-wrap collapsing) —
        // is FALSE, and the measurement is in the module docs of
        // `StyledDom::cascade_trace`: a 252-pass scenario produced ONE rebuild
        // and 251 context offers that early-returned unchanged. The rebuild
        // fires once per distinct dynamic context, not per frame, and
        // `css_props` is exactly re-derivable from `retained_author_css`, so
        // re-deriving it around a rebuild is affordable.
        //
        // The REAL blocker is the reader set. `css_props` is also read directly
        // by `transient::extract_subtree_as_dom`, which copies a node's matched
        // author rules — ALL pseudo-states — into the extracted DOM's inline
        // style; pruning drops the Normal compact-encoded ones and a scoped
        // `with_css` width vanishes from the extracted subtree
        // (`resolved_style_travels_with_the_extracted_subtree` catches it).
        // `get_property_slow` cannot stand in: it answers one (node, type) at a
        // time and cannot enumerate a node's pseudo-state rules.
        //
        // And the compact cache cannot back-fill what would be dropped:
        // `FontFamily` has a compact encoding but is stored as a u64 HASH, so a
        // pruned family is unrecoverable for any reader not going through
        // `computed_values`.
        //
        // So the prerequisite is not incremental recompute — it is giving
        // extraction a source that survives the prune (its own retained view,
        // or a lossless `CssPropertyType -> CssProperty` bridge over the compact
        // cache, which does not exist today).
        //
        // MEASURED (`AZ_CASCADE_TRACE=1`, e2e scenario of a click plus 250
        // keystrokes, 252 layout passes):
        //
        //     253 cascade decisions | 1 compact rebuild | 251 context offers
        //     that early-returned unchanged
        //
        // The rebuild is gated behind `set_dynamic_selector_context`, which
        // returns immediately when the context is equal — so it fires ONCE per
        // distinct dynamic context (viewport size, theme, focus), i.e. once at
        // startup, and again per size during a resize. Not per frame.
        //
        // What makes the prune SOUND is not the rarity, though — it is that
        // `css_props` is written only by `restyle()`, wholly from
        // `retained_author_css`, so it is exactly re-derivable. Every rebuild
        // site now runs `restyle_retained()` first (see
        // `StyledDom::set_dynamic_selector_context` and its siblings), which
        // repopulates `css_props` before the rebuild reads it. #8 cannot recur:
        // the rebuild never sees a pruned store.
        if !self.cascaded_props.is_flattened() {
            self.cascaded_props
                .sort_each_and_flatten(|p| (p.state, p.prop_type));
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
        Self::find_entry_in_stateful(props, state, prop_type).map(|p| &p.property)
    }

    /// [`Self::find_in_stateful`], answering the whole entry (its cascade
    /// [`StatefulCssProperty::order`] with it).
    #[inline]
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention
    fn find_entry_in_stateful<'a>(
        props: &'a [StatefulCssProperty],
        state: azul_css::dynamic_selector::PseudoStateType,
        prop_type: &CssPropertyType,
    ) -> Option<&'a StatefulCssProperty> {
        let key = (state, *prop_type);
        props
            .binary_search_by_key(&key, |p| (p.state, p.prop_type))
            .ok()
            .map(|idx| &props[idx])
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
        props
            .iter()
            .filter(move |p| p.state == state)
            .map(|p| &p.prop_type)
    }
}

/// Whether [`CssPropertyCache::prune_compact_normal_props`] keeps the inherited entry `p`:
/// every entry of a state, every property the compact cache has no encoding for, and a
/// compact-encoded resting one only when the compact cache could not hold its value (a
/// length that is not px: the slow path still has to find it).
fn survives_the_prune(p: &StatefulCssProperty) -> bool {
    use azul_css::dynamic_selector::PseudoStateType;
    if p.state != PseudoStateType::Normal {
        return true;
    }
    if !p.prop_type.has_compact_encoding() {
        return true;
    }
    // Compact-encoded AND Normal: drop only if the compact cache fully
    // captured the value (px metric, or Auto/Initial/Inherit/None).
    property_needs_slow_path_after_compact(&p.property)
}

/// Whether a node's own declaration `prop` (under `conditions`) loses to a
/// rule above `rule_priority::INLINE` that set the same property: for the
/// resting state, in every state; for a state, in that state.
fn inline_is_overruled(
    overruled: &[(azul_css::dynamic_selector::PseudoStateType, CssPropertyType)],
    prop: &CssProperty,
    conditions: &azul_css::dynamic_selector::DynamicSelectorVec,
) -> bool {
    use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
    let ty = prop.get_type();
    let state = conditions
        .as_slice()
        .iter()
        .find_map(|c| match c {
            DynamicSelector::PseudoState(s) => Some(*s),
            _ => None,
        })
        .unwrap_or(PseudoStateType::Normal);
    overruled
        .iter()
        .any(|(s, t)| *t == ty && (*s == PseudoStateType::Normal || *s == state))
}

/// Drops from one node's stylesheet values (`css_props`, still in its build
/// phase) what can never show:
///
/// - a value for a state whose rule has a lower priority than the node's
///   resting value of the same property: priority ranks above the state (a
///   rice at `priority: app` keeps its colour under the pointer), so the
///   resting value wins in that state too. `::placeholder` is a
///   pseudo-element the host's rules do not reach, so its values stay.
/// - a presentational hint the `*` rules of the global bucket (`global`, the
///   highest priority each sets a property at) outrank: a hint is the
///   weakest author declaration (`<table bgcolor>` under `* { background }`).
fn drop_outranked_values(
    entries: &mut Vec<StatefulCssProperty>,
    global: &[(CssPropertyType, u8)],
) {
    use azul_css::{css::rule_priority::PRESENTATIONAL, dynamic_selector::PseudoStateType};
    let hint_outranked = |e: &StatefulCssProperty| {
        e.state == PseudoStateType::Normal
            && e.priority == PRESENTATIONAL
            && global
                .iter()
                .any(|(ty, priority)| *ty == e.prop_type && *priority >= PRESENTATIONAL)
    };
    let stateful = entries.iter().any(|e| e.state != PseudoStateType::Normal);
    if !stateful && (global.is_empty() || !entries.iter().any(hint_outranked)) {
        return;
    }
    let mut resting: Vec<(CssPropertyType, u8)> = Vec::new();
    for e in entries.iter().filter(|e| e.state == PseudoStateType::Normal) {
        match resting.iter_mut().find(|(ty, _)| *ty == e.prop_type) {
            Some(slot) => slot.1 = slot.1.max(e.priority),
            None => resting.push((e.prop_type, e.priority)),
        }
    }
    entries.retain(|e| match e.state {
        PseudoStateType::Normal => !hint_outranked(e),
        PseudoStateType::Placeholder => true,
        _ => resting
            .iter()
            .find(|(ty, _)| *ty == e.prop_type)
            .is_none_or(|(_, priority)| e.priority >= *priority),
    });
}

/// Whether an unscoped `*` rule styles `node`. `*` selects elements, so it
/// styles neither a text node, which only inherits, nor generated content,
/// which the rules naming its pseudo-element style and which inherits from
/// its host.
#[must_use]
pub(crate) const fn star_selects(node: &NodeData) -> bool {
    !node.is_text_node() && !node.is_generated_content()
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
    use azul_css::{
        css::CssPropertyValue,
        props::{
            basic::length::SizeMetric,
            layout::{
                dimensions::{LayoutHeight, LayoutWidth},
                flex::LayoutFlexBasis,
            },
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
/// Inheritable properties whose value must be inherited as the parent's already
/// *resolved* value, NOT propagated as a raw declaration through `cascaded_props`.
///
/// `font-size` is the case: a relative parent value (`1.5em`) propagated raw
/// would be re-resolved against the already-resolved parent at every descendant
/// (multiplicative error: 30px -> 45px -> 67.5px down a chain), and a parent
/// whose own `cascaded` font-size is the *grandparent's* absolute value would
/// skip the parent's own size entirely. Both consuming paths already inherit
/// font-size correctly from the parent's resolved value — `inherit_from_parent`
/// for `computed_values`, and the parent's resolved compact slot in
/// `build_compact_cache_with_inheritance` — so font-size must not ride the raw
/// propagation at all.
fn is_resolved_parent_inherited(prop_type: CssPropertyType) -> bool {
    prop_type == CssPropertyType::FontSize
}

/// A declaration the descendants must NOT receive raw through
/// `cascaded_props`, because it computes to a length against the font size
/// of the element that declares it: `line-height` in `em` / `%` (CSS 2.2
/// s10.8.1). They inherit the computed length through `computed_values`
/// instead ([`CssPropertyCache::compute_inherited_values`]) - the raw `1.5em`
/// would be re-resolved against each descendant's own font size. A number,
/// `normal` and every other length are inherited as written. (`font-size`
/// takes the same road for every value: [`is_resolved_parent_inherited`].)
fn inherits_its_computed_length(p: &CssProperty) -> bool {
    match p {
        CssProperty::LineHeight(v) => v
            .get_property()
            .is_some_and(azul_css::props::style::StyleLineHeight::is_font_relative_length),
        // `bolder` / `lighter` compute against the declaring element's
        // PARENT (CSS Fonts 4 s2.2): the raw keyword re-applied at every
        // descendant made the text inside a `<b>` bolder than the `<b>`.
        CssProperty::FontWeight(v) => v.get_property().is_some_and(|w| w.is_relative()),
        _ => false,
    }
}

fn clone_inheritable_property(p: &CssProperty) -> CssProperty {
    use azul_css::props::property::CssProperty;
    if let CssProperty::FontFamily(v) = p {
        return CssProperty::FontFamily(v.clone());
    }
    if let CssProperty::BackgroundContent(v) = p {
        return CssProperty::BackgroundContent(v.clone());
    }
    if let CssProperty::BackgroundPosition(v) = p {
        return CssProperty::BackgroundPosition(v.clone());
    }
    if let CssProperty::BackgroundSize(v) = p {
        return CssProperty::BackgroundSize(v.clone());
    }
    if let CssProperty::BackgroundRepeat(v) = p {
        return CssProperty::BackgroundRepeat(v.clone());
    }
    if let CssProperty::BoxShadowLeft(v) = p {
        return CssProperty::BoxShadowLeft(v.clone());
    }
    if let CssProperty::BoxShadowRight(v) = p {
        return CssProperty::BoxShadowRight(v.clone());
    }
    if let CssProperty::BoxShadowTop(v) = p {
        return CssProperty::BoxShadowTop(v.clone());
    }
    if let CssProperty::BoxShadowBottom(v) = p {
        return CssProperty::BoxShadowBottom(v.clone());
    }
    if let CssProperty::TextShadow(v) = p {
        return CssProperty::TextShadow(v.clone());
    }
    if let CssProperty::ScrollbarTrack(v) = p {
        return CssProperty::ScrollbarTrack(v.clone());
    }
    if let CssProperty::ScrollbarThumb(v) = p {
        return CssProperty::ScrollbarThumb(v.clone());
    }
    if let CssProperty::ScrollbarButton(v) = p {
        return CssProperty::ScrollbarButton(v.clone());
    }
    if let CssProperty::ScrollbarCorner(v) = p {
        return CssProperty::ScrollbarCorner(v.clone());
    }
    if let CssProperty::ScrollbarResizer(v) = p {
        return CssProperty::ScrollbarResizer(v.clone());
    }
    if let CssProperty::Transform(v) = p {
        return CssProperty::Transform(v.clone());
    }
    if let CssProperty::Filter(v) = p {
        return CssProperty::Filter(v.clone());
    }
    if let CssProperty::BackdropFilter(v) = p {
        return CssProperty::BackdropFilter(v.clone());
    }
    if let CssProperty::Content(v) = p {
        return CssProperty::Content(v.clone());
    }
    if let CssProperty::HyphenationLanguage(v) = p {
        return CssProperty::HyphenationLanguage(v.clone());
    }
    if let CssProperty::Cursor(v) = p {
        return CssProperty::Cursor(*v);
    }
    p.clone()
}

impl CssPropertyCache {
    /// [`Self::restyle_cascade`], then the tag ids for hit-testing
    /// ([`Self::generate_tag_ids`]).
    ///
    /// A caller that builds the compact cache next wants the ids made AFTER
    /// it (`generate_tag_ids` reads the compact cache for display and
    /// overflow): it calls `restyle_cascade` and generates them once, then.
    /// Both callers in `StyledDom` took the ids from here, dropped them and
    /// generated them again: two walks of every node per styling.
    #[must_use]
    pub fn restyle(
        &mut self,
        css: &mut Css,
        node_data: &NodeDataContainerRef<'_, NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
        html_tree: &NodeDataContainerRef<'_, CascadeInfo>,
        node_states: &[StyledNodeState],
    ) -> Vec<TagIdToNodeIdMapping> {
        self.restyle_cascade(
            css,
            node_data,
            node_hierarchy,
            non_leaf_nodes,
            html_tree,
            node_states,
        );
        self.generate_tag_ids(node_data, node_hierarchy)
    }

    /// Match CSS selectors to nodes and populate `css_props`: the cascade
    /// itself, without the tag ids ([`Self::restyle`]).
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive:
                                                                   // single-purpose
                                                                   // parser/builder/dispatch (one
                                                                   // branch per input variant)
    pub fn restyle_cascade(
        &mut self,
        css: &mut Css,
        node_data: &NodeDataContainerRef<'_, NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
        html_tree: &NodeDataContainerRef<'_, CascadeInfo>,
        node_states: &[StyledNodeState],
    ) {
        use azul_css::{
            css::{
                CssDeclaration,
                CssPathPseudoSelector::{
                    self, Active, Backdrop, DragOver, Dragging, Focus, FocusVisible, FocusWithin,
                    Hover, Placeholder, SeatFocus,
                },
                CssPathSelector, CssRuleBlock,
            },
            dynamic_selector::{DynamicSelector, PseudoStateType},
            props::layout::LayoutDisplay,
        };

        use crate::custom_property_cascade::{needs_variable_pass, PendingVar, VarStage};

        let css_is_empty = css.is_empty();
        crate::styled_dom::count_nodes_cascaded(node_data.len());

        // Custom properties are CASCADE-level (design
        // RICING_LAYERS_AND_STOPTHEMINGMYAPP_2026_09_29.md §7.3): the rule
        // matching below stages every `--name` definition on the nodes its
        // rule matches and pushes a placeholder for every `var()` reference;
        // the variable pass then resolves them under this context, before
        // the inheritance walk copies values down.
        let variables = needs_variable_pass(css, node_data.internal);
        let mut var_stage = VarStage::new(self.css_props.len(), variables);

        // @-rule conditions (@media width/height, theme, OS...) gate whole
        // rule BLOCKS. Evaluated here against the window's dynamic context —
        // rules whose conditions do not hold are dropped from this cascade
        // exactly as if absent, and `StyledDom::set_dynamic_selector_context`
        // re-runs the cascade when the context changes and the author css
        // has conditional rules. With NO context yet (a StyledDom no window
        // has adopted), conditional rules do not apply - except the app
        // theme's own `@theme(<name>)` blocks (`condition_holds`), the same
        // rule inline conditional properties follow. (Until 2026-08-10
        // these conditions were silently IGNORED: an author
        // `@media (max-width: 720px)` block applied at every viewport.)
        let dyn_ctx = self.dynamic_context.clone();
        let no_context_theme = dyn_ctx.is_none().then(crate::app_theme::current_theme);
        let rule_applies = |conds: &azul_css::dynamic_selector::DynamicSelectorVec| -> bool {
            let cs = conds.as_slice();
            cs.is_empty()
                || cs.iter().all(|sel| {
                    condition_holds(dyn_ctx.as_deref(), no_context_theme.as_ref(), sel)
                })
        };
        // The THEME RANK of a rule or an inline declaration under the
        // window's theme chain (`DynamicSelectorContext::cascade_rank`): the
        // cascade orders by `(priority, rank, specificity, source order)`, a
        // lower rank winning - so a live `@theme(xyz:pink)` block beats
        // `@theme(xyz)` whatever their order. Without a context no theme
        // block applies here, and every rank is the same.
        let rank = |conds: &[DynamicSelector]| {
            rank_of(dyn_ctx.as_deref(), no_context_theme.as_ref(), conds)
        };

        // Re-enter build phase before repopulating. restyle() is not
        // single-shot: StyledDom::create runs one restyle internally, and building
        // the compact cache flattens these vecs — so on a later restyle both are in
        // read phase, where the old reset `build_iter_mut().clear()` silently
        // iterated ZERO entries (flatten empties `build`). The push_to / build_mut
        // below then indexed an emptied Vec and panicked.
        //
        // css_props is rebuilt from scratch each restyle (repopulated below,
        // flattened at the end), so replace it with a fresh build-phase vec.
        //
        // cascaded_props is rebuilt from scratch TOO (2026-08-12): the old
        // preserve-and-or_insert approach LEAKED properties of rules whose
        // @-condition turned OFF — a color inherited under a min-width
        // block survived in every descendant after crossing below it, so
        // wide and narrow styling applied SIMULTANEOUSLY (the
        // media_restyle_cost law pin caught it). Preservation is
        // unnecessary: the inheritance walk is top-down (parents' fresh
        // slices are written before children read them — the same
        // ordering css_props relies on), so a fresh build-phase vec
        // repopulates completely. The historical reason for preserving
        // was a phase-bug in the old clear, not a data dependency.
        //
        // UNCONDITIONALLY, not only for a non-empty stylesheet: the
        // inheritance walk below runs for every restyle and pushes into
        // `cascaded_props` via `build_mut`, so a restyle with an EMPTY sheet
        // on an already-built cache (a theme flip on an inline-styled DOM
        // is exactly that) indexed the emptied build vec and panicked. The
        // UA entries a previous `apply_ua_css` pushed go with the rest; the
        // caller re-applies them.
        let node_count = self.css_props.len();
        self.css_props = FlatVecVec::new(node_count);
        self.cascaded_props = FlatVecVec::new(node_count);
        self.ua_applied = false;
        // Collect global-only rule declarations ONCE (not per-node).
        // These are stored in self.global_css_props and applied during
        // build_compact_cache_with_inheritance for each node, avoiding
        // 50K × N clones into per-node css_props Vecs.
        self.global_css_props.clear();
        self.left_state_mask = 0;
        self.inline_overruled.clear();
        self.env_readers = EnvReaders::default();
        // What the compounds left of a rule's subject read (`.btn:hover
        // .ink`): the nodes' states as they are now, and the window's
        // activation for `:backdrop`.
        let ancestor_states = crate::style::AncestorStates {
            nodes: node_states,
            window_inactive: dyn_ctx.as_deref().is_some_and(|c| !c.window_focused),
            every_state: false,
        };

        // A node's presentational hints (its markup attributes' style,
        // `rule_priority::PRESENTATIONAL`) come FIRST among its stylesheet
        // properties: the last pushed wins, so every matched rule overrides
        // them - and the inline style, read before the stylesheet, does too.
        for (index, nd) in node_data.internal.iter().enumerate() {
            for rule in nd.style.rules.as_ref() {
                let stateful = rule
                    .conditions
                    .as_slice()
                    .iter()
                    .any(|c| matches!(c, DynamicSelector::PseudoState(_)));
                if rule.priority >= azul_css::css::rule_priority::INLINE
                    || stateful
                    || !rule_applies(&rule.conditions)
                {
                    continue;
                }
                for declaration in rule.declarations.as_ref() {
                    self.env_readers.note(Some(index), declaration);
                    if let Some(prop) = declaration.resolve_in_cascade(dyn_ctx.as_deref()) {
                        self.css_props.push_to(
                            index,
                            StatefulCssProperty {
                                state: PseudoStateType::Normal,
                                prop_type: prop.get_type(),
                                property: prop,
                                ua_origin: false,
                                priority: azul_css::css::rule_priority::PRESENTATIONAL,
                            },
                        );
                    }
                }
            }
        }

        // `:backdrop` is a WINDOW state (the window is not the active one):
        // whether this DOM declares anything under it, in a stylesheet rule
        // or inline, decides whether the inheritance walk below carries a
        // backdrop tier at all. A DOM that never mentions it pays nothing.
        let mut any_backdrop = node_data.internal.iter().any(|nd| {
            nd.style.iter_inline_properties().any(|(_, conds)| {
                conds.as_slice().iter().any(|c| {
                    matches!(c, DynamicSelector::PseudoState(PseudoStateType::Backdrop))
                })
            })
        });

        // The priority of each declaration of the global `*` bucket, by type.
        let mut global_priorities: Vec<(CssPropertyType, u8)> = Vec::new();
        if !css_is_empty {
            css.sort_by_specificity();

            // Separate CSS rules into "global only" (just `*`) vs "has specific selector".
            // Global-only rules apply to ALL nodes — push directly into css_props
            // without per-node selector matching (avoids m×n for these rules).
            // Specific rules still go through matches_html_element per-node.
            // A rule that defines or reads a custom property resolves PER
            // NODE (each node sees its own variables), so it cannot ride the
            // one-value-for-every-node global bucket. Nor can a rule above
            // the inline declarations (a `widgets` rice): the bucket is read
            // below them.
            let is_global_only = |rule: &CssRuleBlock| {
                let selectors = rule.path.selectors.as_ref();
                selectors.len() == 1
                    && matches!(selectors.first(), Some(CssPathSelector::Global))
                    && rule.priority <= azul_css::css::rule_priority::INLINE
                    && !rule
                        .declarations
                        .as_ref()
                        .iter()
                        .any(CssDeclaration::uses_custom_properties)
            };
            // Every rule in cascade order (the theme rank on top of the
            // context-free order); both lists below keep it, as they are
            // applied in turn (last wins). The bucket is read below every
            // rule matched node by node, so a `*` rule of a higher priority
            // than one of those (a rice's at `priority: app` over the app's
            // own rules) is matched node by node too, where its priority
            // decides.
            let mut all: Vec<&CssRuleBlock> = css.rules().collect();
            Css::sort_rules_in_cascade_order(&mut all, rank);
            let lowest_matched = all
                .iter()
                .filter(|rule| !is_global_only(rule))
                .map(|rule| rule.priority)
                .min();
            let mut global_only_rules: Vec<&CssRuleBlock> = Vec::new();
            let mut specific_rules: Vec<&CssRuleBlock> = Vec::new();
            for rule in all {
                if is_global_only(rule) && lowest_matched.is_none_or(|p| rule.priority <= p) {
                    global_only_rules.push(rule);
                } else {
                    specific_rules.push(rule);
                }
            }

            for rule in &global_only_rules {
                if !rule_applies(&rule.conditions) {
                    continue;
                }
                if crate::style::rule_ends_with(&rule.path, None) {
                    for d in &rule.declarations {
                        // `Static` as-is; an `env()` declaration resolved
                        // against the same context that gates the rule.
                        self.env_readers.note(None, d);
                        if let Some(s) = d.resolve_in_cascade(dyn_ctx.as_deref()) {
                            global_priorities.push((s.get_type(), rule.priority));
                            self.global_css_props.push(s);
                        }
                    }
                }
            }

            // Phase 2: Match specific rules per-node (only non-global rules)
            if !specific_rules.is_empty() {
                // The passes, one per state a rule's subject can be in: a rule
                // takes part in the pass of the interaction pseudo-class its
                // path ends with, the resting pass takes every other rule. A
                // node's entries come out pass by pass in this order, and rule
                // by rule (cascade order) within a pass.
                let passes: [(Option<CssPathPseudoSelector>, PseudoStateType); 11] = [
                    (None, PseudoStateType::Normal),
                    (Some(Hover), PseudoStateType::Hover),
                    (Some(Active), PseudoStateType::Active),
                    (Some(Focus), PseudoStateType::Focus),
                    (Some(SeatFocus), PseudoStateType::SeatFocus),
                    (Some(FocusVisible), PseudoStateType::FocusVisible),
                    (Some(FocusWithin), PseudoStateType::FocusWithin),
                    (Some(Dragging), PseudoStateType::Dragging),
                    (Some(DragOver), PseudoStateType::DragOver),
                    (Some(Placeholder), PseudoStateType::Placeholder),
                    (Some(Backdrop), PseudoStateType::Backdrop),
                ];
                let backdrop_pass: u16 = 1 << (passes.len() - 1);
                // Per rule, one bit per pass it ends in; and the same bits
                // only where its @-conditions hold under this context. Both
                // belong to the rule, so they are answered once, not per node.
                let ends_in: Vec<u16> = specific_rules
                    .iter()
                    .map(|rule| {
                        passes
                            .iter()
                            .enumerate()
                            .filter(|(_, (pseudo, _))| {
                                crate::style::rule_ends_with(&rule.path, pseudo.clone())
                            })
                            .fold(0u16, |bits, (pass, _)| bits | (1 << pass))
                    })
                    .collect();
                let takes_part: Vec<u16> = specific_rules
                    .iter()
                    .zip(&ends_in)
                    .map(|(rule, &bits)| {
                        if rule_applies(&rule.conditions) {
                            bits
                        } else {
                            0
                        }
                    })
                    .collect();
                // A stylesheet that mentions `:backdrop` at all gives the
                // inheritance walk below its backdrop tier.
                any_backdrop |= ends_in.iter().any(|bits| bits & backdrop_pass != 0);
                let live_passes = takes_part.iter().fold(0u16, |all, bits| all | bits);
                self.left_state_mask = specific_rules
                    .iter()
                    .zip(&takes_part)
                    .filter(|(_, bits)| **bits != 0)
                    .fold(0, |mask, (rule, _)| {
                        mask | crate::style::left_state_bits(&rule.path)
                    });

                // Each node is tried only against the rules that can match it
                // (`SubjectRuleIndex`): the unscoped ones and those of the
                // `with_css` scopes around it, not every widget's sheet on
                // the page - and of the rules that name a class, only those
                // of the classes it carries, not every class of a sheet.
                let hierarchy = node_hierarchy.as_container();
                let mut index = crate::style::SubjectRuleIndex::new(
                    specific_rules.iter().map(|rule| &rule.path),
                    &hierarchy,
                );
                let mut candidates: Vec<usize> = Vec::new();
                // What the `~` combinators of the rules found, for the nodes
                // still to come (the node states and the rules are the same
                // for every node and pass of this cascade).
                let mut sibling_memo = crate::style::GeneralSiblingMemo::default();
                let node_total = if live_passes == 0 { 0 } else { node_data.len() };
                for node_index in 0..node_total {
                    let node_id = NodeId::new(node_index);
                    index.candidates_into(node_index, &node_data[node_id], &mut candidates);
                    for (pass, (pseudo, state)) in passes.iter().enumerate() {
                        let bit = 1u16 << pass;
                        if live_passes & bit == 0 {
                            continue;
                        }
                        let state = *state;
                        // Each pass reads the node's candidates from the start.
                        for &rule_idx in &candidates {
                            if takes_part[rule_idx] & bit == 0 {
                                continue;
                            }
                            let rule_block: &CssRuleBlock = specific_rules[rule_idx];
                            if !crate::style::matches_html_element_metered(
                                &rule_block.path,
                                node_id,
                                &hierarchy,
                                node_data,
                                html_tree,
                                pseudo.clone(),
                                Some(ancestor_states),
                                Some((&mut sibling_memo, rule_idx)),
                                &self.match_work,
                            ) {
                                continue;
                            }
                            if rule_block.priority > azul_css::css::rule_priority::INLINE {
                                let overruled =
                                    self.inline_overruled.entry(node_index).or_default();
                                for ty in rule_block
                                    .declarations
                                    .as_slice()
                                    .iter()
                                    .filter_map(CssDeclaration::get_type)
                                {
                                    if !overruled.contains(&(state, ty)) {
                                        overruled.push((state, ty));
                                    }
                                }
                            }
                            // Every declaration: properties, `env()` / `var()`
                            // references and custom-property definitions.
                            for decl in rule_block.declarations.as_slice() {
                                if let Some(definition) = decl.custom_property() {
                                    var_stage.define(node_index, state, definition);
                                } else if let Some(reference) = decl.var_reference() {
                                    // A placeholder at the position the
                                    // cascade order gives it; the variable
                                    // pass overwrites it.
                                    let slot =
                                        self.css_props.build_get(node_index).map_or(0, Vec::len);
                                    self.css_props.push_to(
                                        node_index,
                                        StatefulCssProperty {
                                            state,
                                            prop_type: reference.default_value.get_type(),
                                            property: reference.default_value.clone(),
                                            ua_origin: false,
                                            priority: rule_block.priority,
                                        },
                                    );
                                    var_stage.pending.push(PendingVar {
                                        node: node_index,
                                        slot,
                                        state,
                                        reference: reference.clone(),
                                    });
                                } else if let Some(prop) =
                                    decl.resolve_in_cascade(dyn_ctx.as_deref())
                                {
                                    self.env_readers.note(Some(node_index), decl);
                                    self.css_props.push_to(
                                        node_index,
                                        StatefulCssProperty {
                                            state,
                                            prop_type: prop.get_type(),
                                            property: prop,
                                            ua_origin: false,
                                            priority: rule_block.priority,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
            } // end if !specific_rules.is_empty()
        }

        // The variable pass: every node's custom properties, then every
        // `var()` / `env()` reference (the stylesheet placeholders above and
        // the nodes' own declarations) resolved under this context - BEFORE
        // the inheritance walk below copies resolved values to children.
        if variables {
            self.run_variable_pass(var_stage, node_data.internal, node_hierarchy, non_leaf_nodes);
        } else {
            self.custom_property_envs = crate::custom_property_cascade::CustomPropertyEnvs::default();
            self.resolved_inline.clear();
            self.variables_depend_on_context = false;
        }

        // A `::before` / `::after` whose `content` makes nothing gets no box,
        // nor a `::marker` whose host is no inline list item.
        self.hide_contentless_generated_nodes(
            node_data.internal,
            node_hierarchy.as_container().internal,
        );

        // A value for a state whose rule has a lower priority than the
        // node's resting value never shows, nor a presentational hint a `*`
        // rule outranks. Dropped here, they stay out of the lookups and out
        // of what the node's children inherit.
        let mut global: Vec<(CssPropertyType, u8)> = Vec::new();
        for (ty, priority) in &global_priorities {
            match global.iter_mut().find(|(t, _)| t == ty) {
                Some(slot) => slot.1 = slot.1.max(*priority),
                None => global.push((*ty, *priority)),
            }
        }
        for (index, entries) in self.css_props.build_iter_mut().enumerate() {
            let star = node_data.internal.get(index).is_some_and(star_selects);
            drop_outranked_values(entries, if star { &global } else { &[] });
        }

        // What the ROOT inherits from OUTSIDE this DOM (`inherited_from_host`:
        // a VirtualView's host) enters the walk where a parent's values would,
        // as Normal-state entries of the root's `cascaded_props` - below the
        // root's own declarations and its UA defaults (pushed after them, by
        // `apply_ua_css`), handed on below like anything the root inherited.
        // A text node's UA I-beam then yields to a host's `cursor` exactly as
        // it yields to a parent's. `font-size` and a font-relative length go
        // through `computed_values` and the compact cache instead, as they do
        // from any parent (`is_resolved_parent_inherited`,
        // `inherits_its_computed_length`).
        if node_count > 0 && !self.inherited_from_host.is_empty() {
            let from_host: Vec<StatefulCssProperty> = self
                .inherited_from_host
                .iter()
                .filter(|(prop_type, value)| {
                    !is_resolved_parent_inherited(*prop_type)
                        && !inherits_its_computed_length(&value.property)
                })
                .map(|(prop_type, value)| StatefulCssProperty {
                    state: PseudoStateType::Normal,
                    prop_type: *prop_type,
                    property: clone_inheritable_property(&value.property),
                    ua_origin: false,
                    priority: 0,
                })
                .collect();
            self.cascaded_props.build_mut(0).extend(from_host);
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
                PseudoStateType::FocusVisible,
                PseudoStateType::FocusWithin,
                PseudoStateType::SeatFocus,
                PseudoStateType::Dragging,
                PseudoStateType::DragOver,
                PseudoStateType::Placeholder,
                // Last, and only when the DOM declares it (`any_backdrop`).
                PseudoStateType::Backdrop,
            ];
            let states = if any_backdrop {
                &all_states[..]
            } else {
                &all_states[..all_states.len() - 1]
            };

            // 1. The parent's own inheritable declarations, per state.
            //
            // A declaration belongs to the pseudo-state named in its
            // conditions (none = Normal), and its OTHER conditions — the
            // theme, the viewport, the OS — must hold under the dynamic
            // context, exactly as `apply_cascade_properties` decides for
            // the node itself. Last match wins per property, the way inline
            // declarations resolve everywhere else. This used to keep only
            // declarations whose every condition was a pseudo-state, so a
            // `dark_theme(color)` twin never qualified and the child got
            // the light value as a cascaded declaration of ITS OWN — which
            // `compute_inherited_values` then preferred over the twin it
            // had just inherited from the parent. Every widget label under
            // a dark window (the button's, the tree view's, the ribbon's)
            // was dark-on-dark on a DOM cascaded under the window's
            // context from the start; a DOM that had the context set
            // afterwards was recascaded after the prune and came out
            // right by accident.
            //
            // The parent's declarations are put in cascade order once for
            // all states: ordering them is the costly part, and it does not
            // depend on the state.
            let mut inline_by_state: Vec<Vec<(CssPropertyType, CssProperty)>> = {
                // The parent's own declarations AS RESOLVED (`var()`
                // substituted), in cascade order (theme rank, then source
                // order), so the last match below is the one that won on
                // the parent.
                let mut parent_inline = Vec::new();
                azul_css::css::inline_in_cascade_order(
                    self.inline_properties(&node_data[parent_id], parent_id.index()),
                    rank,
                    &mut parent_inline,
                );
                states
                    .iter()
                    .map(|&state| {
                        let mut picked: Vec<(CssPropertyType, CssProperty)> = Vec::new();
                        for &(prop, conds) in &parent_inline {
                            let conditions = conds.as_slice();
                            let decl_state = conditions
                                .iter()
                                .find_map(|c| match c {
                                    DynamicSelector::PseudoState(s) => Some(*s),
                                    _ => None,
                                })
                                .unwrap_or(PseudoStateType::Normal);
                            let applies = decl_state == state
                                && conditions.iter().all(|c| match c {
                                    DynamicSelector::PseudoState(s) => *s == state,
                                    other => condition_holds(
                                        dyn_ctx.as_deref(),
                                        no_context_theme.as_ref(),
                                        other,
                                    ),
                                });
                            let prop_type = prop.get_type();
                            if !applies
                                || !prop_type.is_inheritable()
                                || is_resolved_parent_inherited(prop_type)
                            {
                                continue;
                            }
                            let value = clone_inheritable_property(prop);
                            match picked.iter_mut().find(|(t, _)| *t == prop_type) {
                                Some(slot) => slot.1 = value,
                                None => picked.push((prop_type, value)),
                            }
                        }
                        picked
                    })
                    .collect()
            };

            for (state_index, &state) in states.iter().enumerate() {
                let parent_inheritable_inline = core::mem::take(&mut inline_by_state[state_index]);

                // 2. Inherit CSS stylesheet properties from parent for this pseudo-state.
                //
                // `css_props` is still in its BUILD phase here (it is sorted and
                // deduplicated only after this walk), so the parent's slice lists
                // every matching declaration in cascade order - `color: #101828`
                // AND the `@media (prefers-color-scheme: dark) { color: .. }`
                // twin after it. The child must inherit the one that WON on the
                // parent, the LAST per property (what `sort_each_and_flatten`
                // keeps for the parent itself); the `or_insert` below takes the
                // first it is offered, so the list is deduplicated last-wins
                // first. Taking the first one painted the text inside every
                // light/dark pair in its light value (the demo's headings, dark
                // on a dark page) while the element itself resolved right.
                let parent_inheritable_css: Vec<(CssPropertyType, CssProperty)> = if css_is_empty {
                    Vec::new()
                } else {
                    let mut picked: Vec<(CssPropertyType, CssProperty)> = Vec::new();
                    for p in self.css_props.get_slice(parent_id.index()).iter().filter(|p| {
                        p.state == state
                            && p.prop_type.is_inheritable()
                            && !is_resolved_parent_inherited(p.prop_type)
                    }) {
                        let value = clone_inheritable_property(&p.property);
                        match picked.iter_mut().find(|(t, _)| *t == p.prop_type) {
                            Some(slot) => slot.1 = value,
                            None => picked.push((p.prop_type, value)),
                        }
                    }
                    picked
                };

                // 3. Inherit cascaded properties from parent for this pseudo-state
                let parent_inheritable_cascaded: Vec<(CssPropertyType, CssProperty)> = self
                    .cascaded_props
                    .get_slice(parent_id.index())
                    .iter()
                    .filter(|p| {
                        p.state == state
                            && p.prop_type.is_inheritable()
                            && !is_resolved_parent_inherited(p.prop_type)
                    })
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

                // The parent's winning value per type: the first offered wins
                // (inline, then stylesheet, then what it inherited itself). A
                // winner that computes to a length against the PARENT's font
                // size (`line-height: 1.5em`) is dropped - its children inherit
                // the computed length through `computed_values` - and dropping
                // the WINNER, not the entry, keeps a lower-priority value of the
                // same type from standing in for it.
                let mut offered: Vec<&(CssPropertyType, CssProperty)> = Vec::new();
                for entry in parent_inheritable_inline
                    .iter()
                    .chain(parent_inheritable_css.iter())
                    .chain(parent_inheritable_cascaded.iter())
                {
                    if !offered.iter().any(|(t, _)| *t == entry.0) {
                        offered.push(entry);
                    }
                }
                offered.retain(|(_, v)| !inherits_its_computed_length(v));

                for child_id in parent_id.az_children(&node_hierarchy.as_container()) {
                    let child_vec = self.cascaded_props.build_mut(child_id.index());
                    for (prop_type, prop_value) in offered.iter().copied() {
                        // or_insert: only insert if child doesn't already have this (state,
                        // prop_type)
                        if !child_vec
                            .iter()
                            .any(|p| p.state == state && p.prop_type == *prop_type)
                        {
                            child_vec.push(StatefulCssProperty {
                                state,
                                prop_type: *prop_type,
                                property: prop_value.clone(),
                                ua_origin: false,
                                priority: 0,
                            });
                        }
                    }
                }
            }
        }

        // Sort css_props by (state, prop_type) for binary search lookups,
        // then flatten into contiguous memory for cache-friendly reads.
        self.css_props
            .sort_each_and_flatten(|p| (p.state, p.prop_type));

        // Restyling can change font-size properties; the memoized resolved font
        // sizes are now stale and must be recomputed on next access.
        self.invalidate_resolved_font_sizes();
    }

    /// Generate hit-test tag IDs for nodes that need event handling.
    /// Uses compact cache (if available) for fast display/overflow reads.
    /// Can be called separately after `build_compact_cache_with_inheritance`.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose tag-id walk
    pub fn generate_tag_ids(
        &self,
        node_data: &NodeDataContainerRef<'_, NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
    ) -> Vec<TagIdToNodeIdMapping> {
        // Tag ID generation: determine which nodes need hit-test tags for
        // hover/click/scroll events. Uses compact cache for display/overflow
        // checks instead of get_property_slow (which searches 6 data structures).
        use azul_css::compact_cache::{
            DISPLAY_MASK, DISPLAY_SHIFT, OVERFLOW_MASK, OVERFLOW_X_SHIFT, OVERFLOW_Y_SHIFT,
        };

        self.tag_id_passes.bump();
        let compact_cache = self.compact_cache.as_ref();
        let node_data_container = &node_data.internal;

        let tag_ids = node_data
            .internal
            .iter()
            .enumerate()
            .filter_map(|(node_idx, node_data)| {
                let node_id = NodeId::new(node_idx);

                // What of the node itself makes it a target: its handlers,
                // its tab stop, its context menu. One reading, which the
                // node fingerprints compare too.
                let inputs = crate::layout_inputs::HitTestInputs::of(node_data);
                let tab_index = inputs.tab_stop;

                let mut need_tag = false;

                // Single-pass guard block: each check `break`s out early once it
                // decides `need_tag`. Labeled block (not `loop`) makes the
                // never-iterating control flow explicit (clippy::never_loop).
                'compute_need_tag: {
                    // display:none check — read directly from compact tier1 (fast u64 read)
                    if let Some(cc) = compact_cache.as_ref() {
                        let t1 = cc.tier1_enums[node_idx];
                        let display_val = ((t1 >> DISPLAY_SHIFT) & DISPLAY_MASK) as u8;
                        if display_val == 4 {
                            break 'compute_need_tag;
                        } // 4 = LayoutDisplay::None (new encoding)
                    }

                    // A handler of an event that is not the window's, a tab
                    // stop, a context menu.
                    if inputs.make_a_target() {
                        need_tag = true;
                        break 'compute_need_tag;
                    }

                    // Pseudo-state property checks (hover/active/focus/dragging/drag-over)
                    {
                        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
                        let has_pseudo = |state: PseudoStateType| -> bool {
                            self.inline_properties(node_data, node_idx).any(|(_p, conds)| {
                                conds.as_slice().iter().any(
                                    |c| matches!(c, DynamicSelector::PseudoState(s) if *s == state),
                                )
                            }) || Self::has_state_props(self.css_props.get_slice(node_idx), state)
                        };

                        if has_pseudo(PseudoStateType::Hover)
                            || has_pseudo(PseudoStateType::Active)
                            || has_pseudo(PseudoStateType::Focus)
                            || has_pseudo(PseudoStateType::FocusVisible)
                            || has_pseudo(PseudoStateType::SeatFocus)
                            || has_pseudo(PseudoStateType::Dragging)
                            || has_pseudo(PseudoStateType::DragOver)
                        {
                            need_tag = true;
                            break 'compute_need_tag;
                        }
                    }

                    // Cursor check — read from cached css_props or inline style.
                    if self.css_props.get_slice(node_idx).iter().any(|p| {
                        p.state == azul_css::dynamic_selector::PseudoStateType::Normal
                            && p.prop_type == CssPropertyType::Cursor
                    }) || self
                        .inline_properties(node_data, node_id.index())
                        .any(|(p, _)| p.get_type() == CssPropertyType::Cursor)
                    {
                        need_tag = true;
                        break 'compute_need_tag;
                    }

                    // Overflow scroll check — read from compact tier1
                    if let Some(cc) = compact_cache.as_ref() {
                        let t1 = cc.tier1_enums[node_idx];
                        let ox = ((t1 >> OVERFLOW_X_SHIFT) & OVERFLOW_MASK) as u8;
                        let oy = ((t1 >> OVERFLOW_Y_SHIFT) & OVERFLOW_MASK) as u8;
                        // 2 = Scroll, 3 = Auto in layout_overflow_to_u8 (new encoding)
                        if ox == 2 || ox == 3 || oy == 2 || oy == 3 {
                            need_tag = true;
                            break 'compute_need_tag;
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
                                if matches!(
                                    node_data_container[cid.index()].get_node_type(),
                                    NodeType::Text(_)
                                ) {
                                    has_text = true;
                                    break;
                                }
                                child_id = node_hierarchy.as_container()[cid].next_sibling_id();
                            }
                        }
                        if has_text {
                            need_tag = true;
                            break 'compute_need_tag;
                        }
                    }

                    break 'compute_need_tag;
                }

                if need_tag {
                    // DETERMINISTIC tag: a pure function of node identity
                    // (node index + 1; 0 stays "no tag"), NOT a global
                    // counter. Tag values are namespaced by tag TYPE
                    // (`TAG_TYPE_DOM_NODE` vs cursor/scrollbar/... — every
                    // consumer matches `tag.1`) and resolved per-DOM, so
                    // per-node determinism is all that is required.
                    //
                    // The old `TagId::unique()` counter made tag numbers an
                    // ALLOCATION ORDER artifact: rebuilding the SAME UI (any
                    // callback returning RefreshDom) produced a fresh tag
                    // map, while the structural-identity display-list cache
                    // (solver3 Step 1.1 — root subtree hash + viewport)
                    // correctly reused the old display list. Map and display
                    // list then disagreed about every tag, and each lookup
                    // that crosses the two — `get_node_hit_test_bounds`, the
                    // WebRender hit-test translation — silently resolved to
                    // nothing: after a hash-identical rebuild, clicks aimed
                    // by node stopped landing (the E2E `double_click` on the
                    // ribbon tab was the visible case). With tags derived
                    // from node identity, a structurally identical tree gets
                    // identical tags, which is exactly the invariant the
                    // display-list cache assumes.
                    Some(TagIdToNodeIdMapping {
                        tag_id: TagId::from_crate_internal(TagId {
                            inner: (node_idx as u64) + 1,
                        }),
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

    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive:
                                                                   // single-purpose
                                                                   // parser/builder/dispatch (one
                                                                   // branch per input variant)
    pub fn get_computed_css_style_string(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> String {
        let mut s = String::new();
        if let Some(p) = self.get_background_content(node_data, node_id, node_state) {
            let _ = write!(s, "background: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_color(node_data, node_id, node_state) {
            let _ = write!(s, "background-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_position(node_data, node_id, node_state) {
            let _ = write!(s, "background-position: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_size(node_data, node_id, node_state) {
            let _ = write!(s, "background-size: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_background_repeat(node_data, node_id, node_state) {
            let _ = write!(s, "background-repeat: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_font_size(node_data, node_id, node_state) {
            let _ = write!(s, "font-size: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_font_family(node_data, node_id, node_state) {
            let _ = write!(s, "font-family: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_color(node_data, node_id, node_state) {
            let _ = write!(s, "color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_align(node_data, node_id, node_state) {
            let _ = write!(s, "text-align: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_line_height(node_data, node_id, node_state) {
            let _ = write!(s, "line-height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_letter_spacing(node_data, node_id, node_state) {
            let _ = write!(s, "letter-spacing: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_word_spacing(node_data, node_id, node_state) {
            let _ = write!(s, "word-spacing: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_tab_size(node_data, node_id, node_state) {
            let _ = write!(s, "tab-size: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_cursor(node_data, node_id, node_state) {
            let _ = write!(s, "cursor: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_shadow_left(node_data, node_id, node_state) {
            let _ = write!(s, "-azul-box-shadow-left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_shadow_right(node_data, node_id, node_state) {
            let _ = write!(s, "-azul-box-shadow-right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_shadow_top(node_data, node_id, node_state) {
            let _ = write!(s, "-azul-box-shadow-top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_shadow_bottom(node_data, node_id, node_state) {
            let _ = write!(s, "-azul-box-shadow-bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_color(node_data, node_id, node_state) {
            let _ = write!(s, "border-top-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_left_color(node_data, node_id, node_state) {
            let _ = write!(s, "border-left-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_right_color(node_data, node_id, node_state) {
            let _ = write!(s, "border-right-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_color(node_data, node_id, node_state) {
            let _ = write!(s, "border-bottom-color: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_style(node_data, node_id, node_state) {
            let _ = write!(s, "border-top-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_left_style(node_data, node_id, node_state) {
            let _ = write!(s, "border-left-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_right_style(node_data, node_id, node_state) {
            let _ = write!(s, "border-right-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_style(node_data, node_id, node_state) {
            let _ = write!(s, "border-bottom-style: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_left_radius(node_data, node_id, node_state) {
            let _ = write!(s, "border-top-left-radius: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_right_radius(node_data, node_id, node_state) {
            let _ = write!(s, "border-top-right-radius: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_left_radius(node_data, node_id, node_state) {
            let _ = write!(s, "border-bottom-left-radius: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_right_radius(node_data, node_id, node_state) {
            let _ = write!(s, "border-bottom-right-radius: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_opacity(node_data, node_id, node_state) {
            let _ = write!(s, "opacity: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_transform(node_data, node_id, node_state) {
            let _ = write!(s, "transform: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_transform_origin(node_data, node_id, node_state) {
            let _ = write!(s, "transform-origin: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_perspective_origin(node_data, node_id, node_state) {
            let _ = write!(s, "perspective-origin: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_backface_visibility(node_data, node_id, node_state) {
            let _ = write!(s, "backface-visibility: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_hyphens(node_data, node_id, node_state) {
            let _ = write!(s, "hyphens: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_direction(node_data, node_id, node_state) {
            let _ = write!(s, "direction: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_unicode_bidi(node_data, node_id, node_state) {
            let _ = write!(s, "unicode-bidi: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_box_trim(node_data, node_id, node_state) {
            let _ = write!(s, "text-box-trim: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_text_box_edge(node_data, node_id, node_state) {
            let _ = write!(s, "text-box-edge: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_dominant_baseline(node_data, node_id, node_state) {
            let _ = write!(s, "dominant-baseline: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_alignment_baseline(node_data, node_id, node_state) {
            let _ = write!(s, "alignment-baseline: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_baseline_source(node_data, node_id, node_state) {
            let _ = write!(s, "baseline-source: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_line_fit_edge(node_data, node_id, node_state) {
            let _ = write!(s, "line-fit-edge: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_initial_letter_align(node_data, node_id, node_state) {
            let _ = write!(s, "initial-letter-align: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_initial_letter_wrap(node_data, node_id, node_state) {
            let _ = write!(s, "initial-letter-wrap: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_scrollbar_gutter(node_data, node_id, node_state) {
            let _ = write!(s, "scrollbar-gutter: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_overflow_clip_margin(node_data, node_id, node_state) {
            let _ = write!(s, "overflow-clip-margin: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_clip(node_data, node_id, node_state) {
            let _ = write!(s, "clip: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_white_space(node_data, node_id, node_state) {
            let _ = write!(s, "white-space: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_display(node_data, node_id, node_state) {
            let _ = write!(s, "display: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_float(node_data, node_id, node_state) {
            let _ = write!(s, "float: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_box_sizing(node_data, node_id, node_state) {
            let _ = write!(s, "box-sizing: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_width(node_data, node_id, node_state) {
            let _ = write!(s, "width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_height(node_data, node_id, node_state) {
            let _ = write!(s, "height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_min_width(node_data, node_id, node_state) {
            let _ = write!(s, "min-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_min_height(node_data, node_id, node_state) {
            let _ = write!(s, "min-height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_max_width(node_data, node_id, node_state) {
            let _ = write!(s, "max-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_max_height(node_data, node_id, node_state) {
            let _ = write!(s, "max-height: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_position(node_data, node_id, node_state) {
            let _ = write!(s, "position: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_top(node_data, node_id, node_state) {
            let _ = write!(s, "top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_bottom(node_data, node_id, node_state) {
            let _ = write!(s, "bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_right(node_data, node_id, node_state) {
            let _ = write!(s, "right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_left(node_data, node_id, node_state) {
            let _ = write!(s, "left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_top(node_data, node_id, node_state) {
            let _ = write!(s, "padding-top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_bottom(node_data, node_id, node_state) {
            let _ = write!(s, "padding-bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_left(node_data, node_id, node_state) {
            let _ = write!(s, "padding-left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_padding_right(node_data, node_id, node_state) {
            let _ = write!(s, "padding-right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_top(node_data, node_id, node_state) {
            let _ = write!(s, "margin-top: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_bottom(node_data, node_id, node_state) {
            let _ = write!(s, "margin-bottom: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_left(node_data, node_id, node_state) {
            let _ = write!(s, "margin-left: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_margin_right(node_data, node_id, node_state) {
            let _ = write!(s, "margin-right: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_top_width(node_data, node_id, node_state) {
            let _ = write!(s, "border-top-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_left_width(node_data, node_id, node_state) {
            let _ = write!(s, "border-left-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_right_width(node_data, node_id, node_state) {
            let _ = write!(s, "border-right-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_border_bottom_width(node_data, node_id, node_state) {
            let _ = write!(s, "border-bottom-width: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_overflow_x(node_data, node_id, node_state) {
            let _ = write!(s, "overflow-x: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_overflow_y(node_data, node_id, node_state) {
            let _ = write!(s, "overflow-y: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_direction(node_data, node_id, node_state) {
            let _ = write!(s, "flex-direction: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_wrap(node_data, node_id, node_state) {
            let _ = write!(s, "flex-wrap: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_grow(node_data, node_id, node_state) {
            let _ = write!(s, "flex-grow: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_flex_shrink(node_data, node_id, node_state) {
            let _ = write!(s, "flex-shrink: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_justify_content(node_data, node_id, node_state) {
            let _ = write!(s, "justify-content: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_align_items(node_data, node_id, node_state) {
            let _ = write!(s, "align-items: {};", p.get_css_value_fmt());
        }
        if let Some(p) = self.get_align_content(node_data, node_id, node_state) {
            let _ = write!(s, "align-content: {};", p.get_css_value_fmt());
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
    #[must_use]
    pub fn empty(node_count: usize) -> Self {
        Self {
            node_count,
            paged_footnotes: PagedFootnotes::default(),
            retained_author_css: Css::default(),
            user_overridden_properties: Vec::new(),
            text_color_override_seen: false,
            composed_since_recompute: false,
            dynamic_context: None,
            ua_applied: false,
            cascade_epoch: 0,

            cascaded_props: FlatVecVec::new(node_count),
            css_props: FlatVecVec::new(node_count),

            computed_values: InheritedValues::new(),
            compact_cache: None,
            global_css_props: Vec::new(),
            custom_property_envs: crate::custom_property_cascade::CustomPropertyEnvs::default(),
            resolved_inline: alloc::collections::BTreeMap::new(),
            variables_depend_on_context: false,
            resolved_font_sizes_px: crate::sync::OnceLock::new(),
            resolved_zooms: crate::sync::OnceLock::new(),
            inherited_from_host: Vec::new(),
            left_state_mask: 0,
            inline_overruled: alloc::collections::BTreeMap::new(),
            css_wide_values: alloc::collections::BTreeMap::new(),
            env_readers: EnvReaders::default(),
            tag_id_passes: WorkCount::new(),
            dom_prepare_steps: WorkCount::new(),
            match_work: crate::style::MatchWork::default(),
            prop_walks: PropWalks::default(),
        }
    }

    /// Clear the lazily-populated font-size cache. Call after any
    /// mutation that could change resolved font-sizes (restyle,
    /// DOM mutation, `append`, etc.). The next
    /// [`crate::styled_dom::StyledDom::resolved_font_size_px`] call
    /// repopulates via a single bottom-up tree walk.
    pub fn invalidate_resolved_font_sizes(&mut self) {
        self.resolved_font_sizes_px = crate::sync::OnceLock::new();
        // A restyle can change a `zoom` as well (and with it every font size).
        self.resolved_zooms = crate::sync::OnceLock::new();
    }

    pub fn append(&mut self, other: &mut Self) {
        self.user_overridden_properties
            .append(&mut other.user_overridden_properties);
        self.text_color_override_seen |= other.text_color_override_seen;
        // The parent's dynamic context wins; a child subtree styled before
        // composition has no window context of its own worth keeping.
        if self.dynamic_context.is_none() {
            self.dynamic_context = other.dynamic_context.take();
        }
        // The merged store carries UA entries as soon as either half did.
        self.ua_applied |= other.ua_applied;
        self.cascade_epoch = self.cascade_epoch.wrapping_add(1);
        self.cascaded_props.extend_from(&mut other.cascaded_props);
        self.css_props.extend_from(&mut other.css_props);
        self.computed_values.append(&mut other.computed_values);
        // The variable side tables follow their nodes (indices shift by this
        // DOM's node count).
        self.custom_property_envs.append(
            &mut other.custom_property_envs,
            self.node_count,
            other.node_count,
        );
        let offset = self.node_count;
        self.resolved_inline.extend(
            core::mem::take(&mut other.resolved_inline)
                .into_iter()
                .map(|(node, resolved)| (node + offset, resolved)),
        );
        self.variables_depend_on_context |= other.variables_depend_on_context;
        self.env_readers.append(&mut other.env_readers, offset);
        self.inline_overruled.extend(
            core::mem::take(&mut other.inline_overruled)
                .into_iter()
                .map(|(node, overruled)| (node + offset, overruled)),
        );
        self.css_wide_values.extend(
            core::mem::take(&mut other.css_wide_values)
                .into_iter()
                .map(|((node, ty, keyword), value)| ((node + offset, ty, keyword), value)),
        );
        // The other DOM's stylesheet goes with its nodes, so a later cascade
        // over the merged DOM (a state anchor's re-run, a viewport or theme
        // change) styles both halves as their own cascades did.
        if !other.retained_author_css.is_empty() {
            self.adopt_stylesheet_of(other);
        }

        self.node_count += other.node_count;
        // Indices shifted — invalidate the font-size cache too.
        self.resolved_font_sizes_px = crate::sync::OnceLock::new();
        self.resolved_zooms = crate::sync::OnceLock::new();

        // Invalidate compact cache since node IDs shifted
        self.compact_cache = None;
    }

    /// Takes `other`'s retained stylesheet into this one, for `append`
    /// (`other`'s nodes are about to follow this cache's): its rules keep to
    /// `other`'s nodes - their scopes move with the nodes, an unscoped rule
    /// is scoped to them - and when this cache has rules of its own, its
    /// unscoped ones are scoped to its own nodes, so neither sheet reaches
    /// the other half. The rules anchored on a state left of their subject
    /// keep their re-run (`left_state_mask`).
    fn adopt_stylesheet_of(&mut self, other: &mut Self) {
        use azul_css::css::CssPathSelector;
        let offset = self.node_count;
        let scoped = |rule: &azul_css::css::CssRuleBlock| {
            matches!(rule.path.selectors.as_ref().first(), Some(CssPathSelector::Root(_)))
        };
        if !self.retained_author_css.rules.as_ref().is_empty() && offset > 0 {
            for rule in self.retained_author_css.rules.as_mut() {
                if !scoped(rule) {
                    rule.path.push_front_scope_for(0, offset - 1, false);
                }
            }
        }
        let other_last = offset + other.node_count.saturating_sub(1);
        let mut rules = core::mem::take(&mut other.retained_author_css.rules)
            .into_library_owned_vec();
        for rule in &mut rules {
            if scoped(rule) {
                let mut selectors = rule.path.selectors.as_ref().to_vec();
                for selector in &mut selectors {
                    if let CssPathSelector::Root(range) = selector {
                        range.start += offset;
                        range.end += offset;
                    }
                }
                rule.path.selectors = selectors.into();
            } else {
                rule.path.push_front_scope_for(offset, other_last, false);
            }
        }
        let mut merged = core::mem::take(&mut self.retained_author_css.rules)
            .into_library_owned_vec();
        merged.extend(rules);
        self.retained_author_css.rules = merged.into();
        let mut keyframes = core::mem::take(&mut self.retained_author_css.keyframes)
            .into_library_owned_vec();
        keyframes.extend(
            core::mem::take(&mut other.retained_author_css.keyframes).into_library_owned_vec(),
        );
        self.retained_author_css.keyframes = keyframes.into();
        self.left_state_mask |= other.left_state_mask;
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

    /// The node's USER OVERRIDE for a property, if any — the runtime layer a
    /// transition writes. Public so the display-list builder's inheritance
    /// walk can consult ancestors (the precomputed inherited tables cannot
    /// see a runtime override on an ancestor).
    #[must_use]
    pub fn get_user_override(
        &self,
        node_id: &NodeId,
        css_property_type: &CssPropertyType,
    ) -> Option<&CssProperty> {
        let v = self.user_overridden_properties.get(node_id.index())?;
        v.binary_search_by_key(css_property_type, |(k, _)| *k)
            .ok()
            .map(|idx| &v[idx].1)
    }

    /// A node's own (inline) declarations as `(property, conditions)`, in
    /// declaration order: the view every CASCADE reader of inline style uses
    /// (the slow path, the compact builder, both inheritance passes, the
    /// hit-test tagger).
    ///
    /// `Static` values as declared; `var()` / `env()` references as the last
    /// restyle resolved them under the window's context (their declared
    /// fallback before the first restyle, or for a declaration added since);
    /// custom-property definitions skipped - they set no property; and last,
    /// the state variants of resting `var()` declarations (see
    /// [`crate::custom_property_cascade`]). The DECLARED static view is
    /// `Css::iter_inline_properties`.
    pub fn inline_properties<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_index: usize,
    ) -> impl Iterator<
        Item = (
            &'a CssProperty,
            &'a azul_css::dynamic_selector::DynamicSelectorVec,
        ),
    > + 'a {
        use azul_css::css::CssDeclaration;

        use crate::custom_property_cascade::ResolvedInline;

        let resolved = if self.resolved_inline.is_empty() {
            None
        } else {
            self.resolved_inline.get(&node_index)
        };
        let overruled: &[(azul_css::dynamic_selector::PseudoStateType, CssPropertyType)] =
            if self.inline_overruled.is_empty() {
                &[]
            } else {
                self.inline_overruled
                    .get(&node_index)
                    .map_or(&[][..], Vec::as_slice)
            };
        let mut ordinal = 0usize;
        node_data
            .style
            .rules
            .as_ref()
            .iter()
            // Presentational hints are not inline style: they cascade as the
            // first of the node's stylesheet properties (`restyle`).
            .filter(|r| r.priority >= azul_css::css::rule_priority::INLINE)
            .flat_map(|r| {
                let conditions = &r.conditions;
                r.declarations
                    .as_ref()
                    .iter()
                    .map(move |d| (d, conditions))
            })
            .filter_map(move |(d, conditions)| match d {
                CssDeclaration::Static(p) => Some((p, conditions)),
                CssDeclaration::Dynamic(dy) => {
                    let i = ordinal;
                    ordinal += 1;
                    Some((ResolvedInline::value_of(resolved, i, dy), conditions))
                }
                CssDeclaration::CustomProperty(_) => None,
            })
            .chain(resolved.into_iter().flat_map(ResolvedInline::variants))
            .filter(move |(prop, conditions)| {
                overruled.is_empty() || !inline_is_overruled(overruled, prop, conditions)
            })
    }

    /// The value of the custom property `--name` (pass it without `--`) that
    /// `node` sees in `state`, as the last restyle resolved it.
    #[must_use]
    pub fn get_custom_property(
        &self,
        node: NodeId,
        state: azul_css::dynamic_selector::PseudoStateType,
        name: &str,
    ) -> Option<&str> {
        self.custom_property_envs.get(node.index(), state, name)
    }

    /// Does this node DECLARE the property itself (inline style or a matched
    /// stylesheet rule, any pseudo-state)? Inherited values do NOT count —
    /// this is the "inheritance re-roots here" test for the ancestor-override
    /// walk: below a node with its own `color`, an animated ancestor colour
    /// must not leak through.
    #[must_use]
    pub fn has_own_declaration(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        css_property_type: &CssPropertyType,
    ) -> bool {
        if self
            .inline_properties(node_data, node_id.index())
            .any(|(p, _)| p.get_type() == *css_property_type)
        {
            return true;
        }
        self.css_props
            .get_slice(node_id.index())
            .iter()
            .any(|p| p.prop_type == *css_property_type)
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
            .unwrap_or_else(|| {
                // On a cascaded DOM the UA root text colour is in every
                // node's resolved style (root: `cascaded_props`; below it:
                // `computed_values`), so reaching this constant means a
                // reader lost the cascade's answer — the theme-blind seed
                // the theme-chain analysis (2026-09-12, R1) traced the
                // black-on-dark family to. Legitimate only for a cache no UA
                // pass has run on.
                debug_assert!(
                    !self.ua_applied,
                    "get_text_color_or_default: node {} has no `color` in its resolved style \
                     although the UA pass ran — the themed root default did not reach it",
                    node_id.index()
                );
                DEFAULT_TEXT_COLOR
            })
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
    ) -> Option<&'a CssProperty> {
        // Counter of cascade walks, broken down by property type, in the cache that walked.
        // Drain with `drain_css_prop_counts` when `AZ_PROFILE=cascade` is set to see which
        // properties dominate the cold layout path.
        //
        // The flag is `profile::cascade_enabled`, read from the environment ONCE and cached
        // there: this function fires 710k+ times per cold layout on excel.html, and
        // `std::env::var_os` takes ~100 ns per call on macOS (env lock + hashmap lookup), so a
        // per-call check added ~70 ms of pure noise to every single layout, whether or not the
        // variable was set.
        if crate::profile::cascade_enabled() {
            self.prop_walks.count(*css_property_type);
        }

        // Always use full cascade resolution.
        // Tier 1/2/2b handle layout-hot properties via direct typed getters.
        // This path is only used for paint-time reads (background, shadow, etc.)
        self.get_property_slow(node_data, node_id, node_state, css_property_type)
    }

    /// Whether node `index` declares `ty` anywhere the cascade looks for a
    /// declaration, in any state: a user override, its own style, a
    /// stylesheet rule, what it inherited, or a `*` rule. `false` means
    /// [`Self::get_property`] can only answer the computed value or the UA
    /// default for it.
    #[must_use]
    pub fn declares(&self, node_data: &NodeData, index: usize, ty: CssPropertyType) -> bool {
        self.user_overridden_properties
            .get(index)
            .is_some_and(|v| v.binary_search_by_key(&ty, |(k, _)| *k).is_ok())
            || self.inline_properties(node_data, index).any(|(p, _)| p.get_type() == ty)
            || self.css_props.get_slice(index).iter().any(|p| p.prop_type == ty)
            || self.cascaded_props.get_slice(index).iter().any(|p| p.prop_type == ty)
            || (star_selects(node_data) && self.global_css_props.iter().any(|p| p.get_type() == ty))
    }

    /// [`Self::get_property`], answering a type the node declares nowhere
    /// ([`Self::declares`]) straight from its computed value and the UA
    /// default instead of walking every state tier: what a pass that reads
    /// one type of every node of a tree (a rebuild reading each node's
    /// `animation`) wants, where most nodes declare none.
    #[allow(clippy::trivially_copy_pass_by_ref)] // the same signature as `get_property`
    #[must_use]
    pub fn get_property_if_declared<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        let index = node_id.index();
        if self.declares(node_data, index, *css_property_type) {
            return self.get_property(node_data, node_id, node_state, css_property_type);
        }
        // The tail of the full walk (`get_property_slow`), which is all it
        // could have found.
        if css_property_type.is_inheritable() {
            if let Some(v) = self.computed_values.get(index, *css_property_type) {
                return Some(&v.property);
            }
        }
        crate::ua_css::get_ua_default(
            node_data,
            takes_root_ua_default(index, *css_property_type, &self.inherited_from_host),
            *css_property_type,
            self.dynamic_context.as_deref(),
        )
    }

    /// Takes the per-property cascade-walk counts of this cache, populated by
    /// [`Self::get_property`] when `AZ_PROFILE=cascade` is set in the environment.
    ///
    /// Returns `(property type, count)` pairs sorted by count descending. Layout-side
    /// instrumentation calls this on the DOM it just laid out to print which properties drove
    /// the most cascade walks.
    #[must_use]
    pub fn drain_css_prop_counts(&self) -> Vec<(CssPropertyType, usize)> {
        self.prop_walks.drain()
    }

    /// Full cascade resolution for any CSS property type.
    /// Walks all cascade layers: user overrides → inline → stylesheet → cascaded → computed → UA.
    /// Also used by restyle functions that need state-aware lookups.
    /// A CSS-wide keyword that wins is answered by the value it stands for
    /// ([`Self::css_wide_values`]).
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    pub(crate) fn get_property_slow<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        let declared =
            self.get_declared_property(node_data, node_id, node_state, css_property_type)?;
        Some(
            self.css_wide_value_of(node_id.index(), declared)
                .unwrap_or(declared),
        )
    }

    /// [`Self::get_property`] with the [`CascadeRank`] of the declaration that won: what two
    /// properties that paint one thing compare (a `background-color` and the bottom colour of a
    /// layer list).
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    #[must_use]
    pub fn get_ranked_property<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<(CascadeRank, &'a CssProperty)> {
        let (mut rank, declared) =
            self.get_declared_property_ranked(node_data, node_id, node_state, css_property_type)?;
        // One of the node's own declarations: its place among them (1-based)
        // tells two of them at one priority and state apart.
        if rank.source == CascadeRank::declared(0, 0).source
            && rank.priority == azul_css::css::rule_priority::INLINE
        {
            rank.order = self
                .inline_properties(node_data, node_id.index())
                .position(|(p, _)| core::ptr::eq(p, declared))
                .map_or(0, |i| u32::try_from(i + 1).unwrap_or(u32::MAX));
        }
        Some((
            rank,
            self.css_wide_value_of(node_id.index(), declared)
                .unwrap_or(declared),
        ))
    }

    /// The value `declared`, a declaration of node `index`, stands for when it is a CSS-wide
    /// keyword with a value of its own ([`Self::css_wide_values`]); `None` for any other
    /// declaration, and for a keyword the readers take as the property's initial value.
    #[must_use]
    pub fn css_wide_value_of(&self, index: usize, declared: &CssProperty) -> Option<&CssProperty> {
        if self.css_wide_values.is_empty() {
            return None;
        }
        let keyword = declared.css_wide_keyword()?;
        self.css_wide_values
            .get(&(index, declared.get_type(), keyword))
    }

    /// The declaration that wins for `css_property_type` on the node, as written: a CSS-wide
    /// keyword stays a keyword ([`Self::get_property_slow`] answers its value).
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch
                                     // (one branch per input variant)
    fn get_declared_property<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        self.get_declared_property_ranked(node_data, node_id, node_state, css_property_type)
            .map(|(_, p)| p)
    }

    /// [`Self::get_declared_property`] with the [`CascadeRank`] it won at.
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see find_in_stateful)
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch
                                     // (one branch per input variant)
    fn get_declared_property_ranked<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
        css_property_type: &CssPropertyType,
    ) -> Option<(CascadeRank, &'a CssProperty)> {
        /// Takes `property` (declared at `priority` for the state of
        /// strength `state`) over what `found` holds only from a higher
        /// priority: the states come strongest first.
        fn offer<'p>(
            found: &mut Option<(CascadeRank, &'p CssProperty)>,
            priority: u8,
            state: u8,
            property: &'p CssProperty,
        ) {
            if found.is_none_or(|(best, _)| priority > best.priority) {
                *found = Some((CascadeRank::declared(priority, state), property));
            }
        }
        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};

        // Helper: do these conditions identify a rule that applies in `state`
        // under the window's dynamic context? Empty conditions = Normal-only.
        // Otherwise EVERY condition must hold: a pseudo-state condition must
        // equal `state`, and every other condition (viewport/@media, theme,
        // OS, container...) is evaluated against the window-provided
        // `dynamic_context`. With no context yet (a StyledDom no window has
        // adopted), non-pseudo conditions do not apply - the exact behaviour
        // they had before contexts were wired through, so creation-time
        // styling is unchanged.
        let ctx = self.dynamic_context.as_deref();
        let no_context_theme = ctx.is_none().then(crate::app_theme::current_theme);
        // `::placeholder` is a pseudo-ELEMENT, not a state of the host: only
        // a declaration that names it styles the prompt. A declaration
        // conditioned only on the mode or the app theme (every declaration
        // of a widget that follows the app theme sits in an `@theme(..)`
        // block) is the HOST's - it reaches the prompt through the Normal
        // tier, where the prompt declares nothing - and must not outrank
        // the prompt's own declaration by coming later in source order.
        let matches_pseudo_state = |conds: &azul_css::dynamic_selector::DynamicSelectorVec,
                                    state: PseudoStateType|
         -> bool {
            let conditions = conds.as_slice();
            if conditions.is_empty() {
                state == PseudoStateType::Normal
            } else if state == PseudoStateType::Placeholder
                && !conditions.iter().any(|c| {
                    matches!(
                        c,
                        DynamicSelector::PseudoState(PseudoStateType::Placeholder)
                    )
                })
            {
                false
            } else {
                conditions.iter().all(|c| match c {
                    DynamicSelector::PseudoState(s) => *s == state,
                    non_pseudo => condition_holds(ctx, no_context_theme.as_ref(), non_pseudo),
                })
            }
        };
        // Among the inline declarations that apply, the one of the LOWEST
        // theme rank wins (`@theme(xyz:pink)` over `@theme(xyz)` over a
        // declaration outside every block), the LAST in source order among
        // equals - `Css::winning_inline_property`, the same order the compact
        // builder applies them in. With no theme block in play that is the
        // plain last match a widget's merged style relies on.
        let rank = |conds: &[DynamicSelector]| rank_of(ctx, no_context_theme.as_ref(), conds);

        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                return Some((CascadeRank::USER_OVERRIDE, &v[idx].1));
            }
        }

        // The states the node is in, strongest first, and the Normal base
        // layer last. Every state looks in the same places: PRIORITY 1 the
        // node's inline declarations for it, PRIORITY 2 its stylesheet rules
        // for it (and, at rest, the global `*` rules), PRIORITY 3 what it
        // inherited for it. A declaration of a weaker state, or the resting
        // one, wins over one already found only from a higher
        // `rule_priority` (a rice at `priority: app` keeps its resting value
        // under the pointer, and the node's own resting style beats an
        // author's `:hover` rule); at equal priority the stronger state wins.
        // What a state inherited counts only while nothing is declared yet.
        //
        // - `::placeholder` FIRST. It is a pseudo-ELEMENT: the flag is only
        //   ever set for the ONE resolve the engine does to style the prompt
        //   it paints inside an empty editable, and for that resolve the
        //   pseudo-element's own declarations (`on_placeholder(..)`,
        //   `.field::placeholder { .. }`) must outrank every state of the
        //   host. Anything `::placeholder` does not declare keeps falling
        //   through to the host's normal value, which is what makes an
        //   unstyled prompt inherit the field's font.
        // - `:focus-visible` > `:focus` (a ring declared for seen focus wins
        //   over the plain focus face) > `:focus-within` (a frame's ring for
        //   the focus inside it; on a focused node `:focus` says more) >
        //   `:seat-focus` (a non-primary seat's focus) > `:active` >
        //   `:dragging` > `:drag-over` (both above `:hover`) > `:hover`.
        // - `:backdrop` (GTK: the window is not the active one). A WINDOW
        //   state: `StyledDom::sync_backdrop_state` raises the node flag,
        //   while the window is inactive, on exactly the nodes that declare
        //   or inherit a `:backdrop` value. Below every interaction state, so
        //   a control of an inactive window still shows its hover.
        // - Normal always applies, as the base layer. Between its stylesheet
        //   rules and what it inherited sit the global `*` rules.
        let index = node_id.index();
        let tiers = [
            (node_state.focus_visible, PseudoStateType::FocusVisible),
            (node_state.focused, PseudoStateType::Focus),
            (node_state.focus_within, PseudoStateType::FocusWithin),
            (node_state.seat_focused, PseudoStateType::SeatFocus),
            (node_state.active, PseudoStateType::Active),
            (node_state.dragging, PseudoStateType::Dragging),
            (node_state.drag_over, PseudoStateType::DragOver),
            (node_state.hover, PseudoStateType::Hover),
            (node_state.backdrop, PseudoStateType::Backdrop),
            (true, PseudoStateType::Normal),
        ];
        let sheet = self.css_props.get_slice(index);
        let inherited = self.cascaded_props.get_slice(index);

        // The node's own declaration for `state`.
        let own = |state: PseudoStateType| -> Option<&'a CssProperty> {
            if state == PseudoStateType::Backdrop {
                // `:backdrop` reads the DECLARED static view, the last match
                // winning (the titlebar's `background_inactive`) - not the
                // resolved (`var()`), theme-ranked view every other state
                // reads.
                node_data
                    .style
                    .iter_inline_properties()
                    .filter(|&(prop, conds)| {
                        prop.get_type() == *css_property_type && matches_pseudo_state(conds, state)
                    })
                    .last()
                    .map(|(prop, _)| prop)
            } else {
                azul_css::css::winning_inline_in(
                    self.inline_properties(node_data, index),
                    *css_property_type,
                    |conds| matches_pseudo_state(conds, state),
                    rank,
                )
            }
        };

        // The prompt's own values: its declarations, its rules, and what it
        // inherited for `::placeholder`, before anything of the host.
        if node_state.placeholder {
            let placeholder = PseudoStateType::Placeholder;
            if let Some(p) = own(placeholder)
                .or_else(|| Self::find_in_stateful(sheet, placeholder, css_property_type))
                .or_else(|| Self::find_in_stateful(inherited, placeholder, css_property_type))
            {
                return Some((CascadeRank::PLACEHOLDER, p));
            }
        }

        let mut found: Option<(CascadeRank, &'a CssProperty)> = None;
        let tier_count = tiers.len();
        for (tier, (raised, state)) in tiers.into_iter().enumerate() {
            if !raised {
                continue;
            }
            // The strongest state ranks highest.
            let strength = u8::try_from(tier_count - tier).unwrap_or(u8::MAX);
            // PRIORITY 1: inline declarations.
            if let Some(p) = own(state) {
                offer(&mut found, azul_css::css::rule_priority::INLINE, strength, p);
            }
            // PRIORITY 2: stylesheet rules (`.x:hover { .. }`).
            if let Some(p) = Self::find_entry_in_stateful(sheet, state, css_property_type) {
                offer(&mut found, p.priority, strength, &p.property);
            }
            // PRIORITY 2b (Normal only): the global `*` rules (specificity
            // 0,0,0), collected once during restyle for every element. They
            // rank below every rule matched node by node (`restyle` keeps
            // only those in the bucket) and above inheritance and the UA
            // defaults. Collected in cascade order (priority, theme rank,
            // source order), so the LAST of a property wins - as the compact
            // builder, which applies them in turn, has it.
            if state == PseudoStateType::Normal && found.is_none() && star_selects(node_data) {
                if let Some(p) = self
                    .global_css_props
                    .iter()
                    .rev()
                    .find(|p| p.get_type() == *css_property_type)
                {
                    return Some((CascadeRank::GLOBAL_RULE, p));
                }
            }
            // PRIORITY 3: cascaded / inherited properties, while nothing is
            // declared.
            if found.is_none() {
                if let Some(entry) = Self::find_entry_in_stateful(inherited, state, css_property_type)
                {
                    // A resting COPY of the parent's value (not a UA default,
                    // which is the node's own) where the inheritance pass
                    // passed it over: the parent shows the property from a
                    // state (`.row:hover { color }`, its own or one above
                    // it), and the node holds what the parent shows, as an
                    // inherited value (`apply_cascade_properties`). That is
                    // the only way a node with such a copy holds the type
                    // inherited: the pass applies every other copy as the
                    // node's own. The row keeps the copy for the day the
                    // state ends; until then the value is the inherited one.
                    if state == PseudoStateType::Normal
                        && !entry.ua_origin
                        && css_property_type.is_inheritable()
                    {
                        if let Some(shown) = self
                            .computed_values
                            .get(index, *css_property_type)
                            .filter(|value| value.origin == CssPropertyOrigin::Inherited)
                        {
                            return Some((CascadeRank::INHERITED, &shown.property));
                        }
                    }
                    return Some((CascadeRank::INHERITED, &entry.property));
                }
            }
        }
        if found.is_some() {
            return found;
        }

        // Check computed values cache for inherited properties
        // Sorted Vec with binary search
        if css_property_type.is_inheritable() {
            if let Some(v) = self
                .computed_values
                .get(node_id.index(), *css_property_type)
            {
                return Some((CascadeRank::INHERITED, &v.property));
            }
        }

        // User-agent stylesheet fallback (lowest precedence): the SAME themed
        // table `apply_ua_css` and the compact builder consume. On a cascaded
        // DOM every answer here is already in `cascaded_props` /
        // `computed_values` above (the root's document-wide defaults
        // included), so this is reached for caches no UA pass has run on and
        // for the properties the cascade prunes back out
        // (`prune_compact_normal_props`).
        crate::ua_css::get_ua_default(
            node_data,
            takes_root_ua_default(
                node_id.index(),
                *css_property_type,
                &self.inherited_from_host,
            ),
            *css_property_type,
            self.dynamic_context.as_deref(),
        )
        .map(|p| (CascadeRank::INHERITED, p))
    }

    /// Get a CSS property using `DynamicSelectorContext` for evaluation.
    ///
    /// This is the new API that supports @media queries, @container queries,
    /// OS-specific styles, and all pseudo-states via `CssPropertyWithConditions`.
    ///
    /// The evaluation follows "last wins" semantics - properties are evaluated
    /// in reverse order and the first matching property wins.
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see
                                                 // find_in_stateful)
    pub(crate) fn get_property_with_context<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        context: &DynamicSelectorContext,
        css_property_type: &CssPropertyType,
    ) -> Option<&'a CssProperty> {
        // First test if there is some user-defined override for the property
        if let Some(v) = self.user_overridden_properties.get(node_id.index()) {
            if let Ok(idx) = v.binary_search_by_key(css_property_type, |(k, _)| *k) {
                let declared = &v[idx].1;
                return Some(
                    self.css_wide_value_of(node_id.index(), declared)
                        .unwrap_or(declared),
                );
            }
        }

        // Check inline CSS properties with DynamicSelectorContext evaluation:
        // among the declarations whose every condition holds, the lowest
        // theme rank wins, the LAST in source order among equals
        // (`azul_css::css::winning_inline_in`), over the node's inline style
        // as the cascade resolved it (`var()` substituted) - replacing the
        // old Focus > Active > Hover > Normal priority chain.
        if let Some(prop) = azul_css::css::winning_inline_in(
            self.inline_properties(node_data, node_id.index()),
            *css_property_type,
            |conds| conds.as_slice().iter().all(|c| c.matches(context)),
            |conds| context.cascade_rank(conds),
        ) {
            return Some(self.css_wide_value_of(node_id.index(), prop).unwrap_or(prop));
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
        // Every declaration kind counts (a `var()` reference or a
        // custom-property definition under a condition flips too), so this
        // walks the rules, not the static view.
        for rule in node_data.style.rules.as_ref() {
            if rule.declarations.as_ref().is_empty() {
                continue;
            }
            let conds = &rule.conditions;
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
        for rule in node_data.style.rules.as_ref() {
            // Skip rules that set nothing layout-affecting (a custom-property
            // definition can feed a layout property below it: it counts).
            if !rule
                .declarations
                .as_ref()
                .iter()
                .any(azul_css::css::CssDeclaration::can_trigger_relayout)
            {
                continue;
            }
            let conds = &rule.conditions;

            let was_active = conds.as_slice().iter().all(|c| c.matches(old_context));
            let is_active = conds.as_slice().iter().all(|c| c.matches(new_context));
            if was_active != is_active {
                return true;
            }
        }
        false
    }

    impl_get_prop!(
        get_background_content,
        StyleBackgroundContentVecValue,
        BackgroundContent,
        as_background_content
    );

    impl_get_prop!(get_hyphens, StyleHyphensValue, Hyphens, as_hyphens);

    impl_get_prop!(
        get_word_break,
        StyleWordBreakValue,
        WordBreak,
        as_word_break
    );

    impl_get_prop!(
        get_overflow_wrap,
        StyleOverflowWrapValue,
        OverflowWrap,
        as_overflow_wrap
    );

    impl_get_prop!(
        get_line_break,
        StyleLineBreakValue,
        LineBreak,
        as_line_break
    );

    impl_get_prop!(
        get_text_align_last,
        StyleTextAlignLastValue,
        TextAlignLast,
        as_text_align_last
    );

    impl_get_prop!(
        get_text_transform,
        StyleTextTransformValue,
        TextTransform,
        as_text_transform
    );

    impl_get_prop!(
        get_object_fit,
        StyleObjectFitValue,
        ObjectFit,
        as_object_fit
    );

    impl_get_prop!(
        get_text_overflow,
        StyleTextOverflowValue,
        TextOverflow,
        as_text_overflow
    );

    impl_get_prop!(
        get_text_orientation,
        StyleTextOrientationValue,
        TextOrientation,
        as_text_orientation
    );

    impl_get_prop!(
        get_object_position,
        StyleObjectPositionValue,
        ObjectPosition,
        as_object_position
    );

    impl_get_prop!(
        get_aspect_ratio,
        StyleAspectRatioValue,
        AspectRatio,
        as_aspect_ratio
    );

    impl_get_prop!(get_direction, StyleDirectionValue, Direction, as_direction);

    impl_get_prop!(
        get_unicode_bidi,
        StyleUnicodeBidiValue,
        UnicodeBidi,
        as_unicode_bidi
    );

    impl_get_prop!(
        get_text_box_trim,
        StyleTextBoxTrimValue,
        TextBoxTrim,
        as_text_box_trim
    );

    impl_get_prop!(
        get_text_box_edge,
        StyleTextBoxEdgeValue,
        TextBoxEdge,
        as_text_box_edge
    );

    impl_get_prop!(
        get_dominant_baseline,
        StyleDominantBaselineValue,
        DominantBaseline,
        as_dominant_baseline
    );

    impl_get_prop!(
        get_alignment_baseline,
        StyleAlignmentBaselineValue,
        AlignmentBaseline,
        as_alignment_baseline
    );

    impl_get_prop!(
        get_baseline_source,
        StyleBaselineSourceValue,
        BaselineSource,
        as_baseline_source
    );

    impl_get_prop!(
        get_line_fit_edge,
        StyleLineFitEdgeValue,
        LineFitEdge,
        as_line_fit_edge
    );

    impl_get_prop!(
        get_initial_letter_align,
        StyleInitialLetterAlignValue,
        InitialLetterAlign,
        as_initial_letter_align
    );

    impl_get_prop!(
        get_initial_letter_wrap,
        StyleInitialLetterWrapValue,
        InitialLetterWrap,
        as_initial_letter_wrap
    );

    impl_get_prop!(
        get_scrollbar_gutter,
        StyleScrollbarGutterValue,
        ScrollbarGutter,
        as_scrollbar_gutter
    );

    impl_get_prop!(
        get_overflow_clip_margin,
        StyleOverflowClipMarginValue,
        OverflowClipMargin,
        as_overflow_clip_margin
    );

    impl_get_prop!(get_clip, StyleClipRectValue, Clip, as_clip);

    impl_get_prop!(
        get_white_space,
        StyleWhiteSpaceValue,
        WhiteSpace,
        as_white_space
    );
    impl_get_prop!(
        get_background_position,
        StyleBackgroundPositionVecValue,
        BackgroundPosition,
        as_background_position
    );
    impl_get_prop!(
        get_background_size,
        StyleBackgroundSizeVecValue,
        BackgroundSize,
        as_background_size
    );
    impl_get_prop!(
        get_background_repeat,
        StyleBackgroundRepeatVecValue,
        BackgroundRepeat,
        as_background_repeat
    );
    impl_get_prop!(get_font_size, StyleFontSizeValue, FontSize, as_font_size);
    impl_get_prop!(
        get_font_family,
        StyleFontFamilyVecValue,
        FontFamily,
        as_font_family
    );
    impl_get_prop!(
        get_font_weight,
        StyleFontWeightValue,
        FontWeight,
        as_font_weight
    );
    impl_get_prop!(
        get_font_style,
        StyleFontStyleValue,
        FontStyle,
        as_font_style
    );
    impl_get_prop!(
        get_text_color,
        StyleTextColorValue,
        TextColor,
        as_text_color
    );
    impl_get_prop!(
        get_text_indent,
        StyleTextIndentValue,
        TextIndent,
        as_text_indent
    );
    impl_get_prop!(
        get_initial_letter,
        StyleInitialLetterValue,
        InitialLetter,
        as_initial_letter
    );
    impl_get_prop!(
        get_line_clamp,
        StyleLineClampValue,
        LineClamp,
        as_line_clamp
    );
    impl_get_prop!(
        get_hanging_punctuation,
        StyleHangingPunctuationValue,
        HangingPunctuation,
        as_hanging_punctuation
    );
    impl_get_prop!(
        get_text_combine_upright,
        StyleTextCombineUprightValue,
        TextCombineUpright,
        as_text_combine_upright
    );
    impl_get_prop!(
        get_exclusion_margin,
        StyleExclusionMarginValue,
        ExclusionMargin,
        as_exclusion_margin
    );
    impl_get_prop!(
        get_hyphenation_language,
        StyleHyphenationLanguageValue,
        HyphenationLanguage,
        as_hyphenation_language
    );
    impl_get_prop!(get_caret_color, CaretColorValue, CaretColor, as_caret_color);

    impl_get_prop!(get_caret_width, CaretWidthValue, CaretWidth, as_caret_width);

    impl_get_prop!(
        get_caret_animation_duration,
        CaretAnimationDurationValue,
        CaretAnimationDuration,
        as_caret_animation_duration
    );

    impl_get_prop!(
        get_selection_background_color,
        SelectionBackgroundColorValue,
        SelectionBackgroundColor,
        as_selection_background_color
    );

    impl_get_prop!(
        get_selection_color,
        SelectionColorValue,
        SelectionColor,
        as_selection_color
    );

    impl_get_prop!(
        get_selection_radius,
        SelectionRadiusValue,
        SelectionRadius,
        as_selection_radius
    );

    impl_get_prop!(
        get_text_justify,
        LayoutTextJustifyValue,
        TextJustify,
        as_text_justify
    );

    impl_get_prop!(get_z_index, LayoutZIndexValue, ZIndex, as_z_index);

    impl_get_prop!(
        get_flex_basis,
        LayoutFlexBasisValue,
        FlexBasis,
        as_flex_basis
    );

    impl_get_prop!(
        get_column_gap,
        LayoutColumnGapValue,
        ColumnGap,
        as_column_gap
    );

    impl_get_prop!(get_row_gap, LayoutRowGapValue, RowGap, as_row_gap);

    impl_get_prop!(
        get_grid_template_columns,
        LayoutGridTemplateColumnsValue,
        GridTemplateColumns,
        as_grid_template_columns
    );

    impl_get_prop!(
        get_grid_template_rows,
        LayoutGridTemplateRowsValue,
        GridTemplateRows,
        as_grid_template_rows
    );

    impl_get_prop!(
        get_grid_auto_columns,
        LayoutGridAutoColumnsValue,
        GridAutoColumns,
        as_grid_auto_columns
    );

    impl_get_prop!(
        get_grid_auto_rows,
        LayoutGridAutoRowsValue,
        GridAutoRows,
        as_grid_auto_rows
    );

    impl_get_prop!(
        get_grid_column,
        LayoutGridColumnValue,
        GridColumn,
        as_grid_column
    );

    impl_get_prop!(get_grid_row, LayoutGridRowValue, GridRow, as_grid_row);

    impl_get_prop!(
        get_grid_auto_flow,
        LayoutGridAutoFlowValue,
        GridAutoFlow,
        as_grid_auto_flow
    );

    impl_get_prop!(
        get_justify_self,
        LayoutJustifySelfValue,
        JustifySelf,
        as_justify_self
    );

    impl_get_prop!(
        get_justify_items,
        LayoutJustifyItemsValue,
        JustifyItems,
        as_justify_items
    );

    impl_get_prop!(get_gap, LayoutGapValue, Gap, as_gap);

    /// Method for getting grid-gap property
    #[allow(clippy::trivially_copy_pass_by_ref)] // uniform by-ref cascade-API convention (see
                                                 // find_in_stateful)
    pub(crate) fn get_grid_gap<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a LayoutGapValue> {
        self.get_property(node_data, node_id, node_state, &CssPropertyType::GridGap)
            .and_then(|p| p.as_grid_gap())
    }

    impl_get_prop!(
        get_align_self,
        LayoutAlignSelfValue,
        AlignSelf,
        as_align_self
    );

    impl_get_prop!(get_font, StyleFontValue, Font, as_font);

    impl_get_prop!(
        get_writing_mode,
        LayoutWritingModeValue,
        WritingMode,
        as_writing_mode
    );

    impl_get_prop!(get_clear, LayoutClearValue, Clear, as_clear);

    impl_get_prop!(
        get_shape_outside,
        ShapeOutsideValue,
        ShapeOutside,
        as_shape_outside
    );

    impl_get_prop!(
        get_shape_inside,
        ShapeInsideValue,
        ShapeInside,
        as_shape_inside
    );

    impl_get_prop!(get_clip_path, ClipPathValue, ClipPath, as_clip_path);

    /// Method for getting scrollbar track background
    pub fn get_scrollbar_track<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarTrack,
        )
        .and_then(|p| p.as_scrollbar_track())
    }

    /// Method for getting scrollbar thumb background
    pub fn get_scrollbar_thumb<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarThumb,
        )
        .and_then(|p| p.as_scrollbar_thumb())
    }

    /// Method for getting scrollbar button background
    pub fn get_scrollbar_button<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarButton,
        )
        .and_then(|p| p.as_scrollbar_button())
    }

    /// Method for getting scrollbar corner background
    pub fn get_scrollbar_corner<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarCorner,
        )
        .and_then(|p| p.as_scrollbar_corner())
    }

    /// Method for getting scrollbar resizer background
    pub fn get_scrollbar_resizer<'a>(
        &'a self,
        node_data: &'a NodeData,
        node_id: &NodeId,
        node_state: &StyledNodeState,
    ) -> Option<&'a StyleBackgroundContentValue> {
        self.get_property(
            node_data,
            node_id,
            node_state,
            &CssPropertyType::ScrollbarResizer,
        )
        .and_then(|p| p.as_scrollbar_resizer())
    }

    impl_get_prop!(
        get_scrollbar_width,
        LayoutScrollbarWidthValue,
        ScrollbarWidth,
        as_scrollbar_width
    );
    impl_get_prop!(
        get_overscroll_behavior_x,
        OverscrollBehaviorValue,
        OverscrollBehaviorX,
        as_overscroll_behavior_x
    );
    impl_get_prop!(
        get_overscroll_behavior_y,
        OverscrollBehaviorValue,
        OverscrollBehaviorY,
        as_overscroll_behavior_y
    );

    impl_get_prop!(
        get_scrollbar_color,
        StyleScrollbarColorValue,
        ScrollbarColor,
        as_scrollbar_color
    );

    impl_get_prop!(
        get_scrollbar_visibility,
        ScrollbarVisibilityModeValue,
        ScrollbarVisibility,
        as_scrollbar_visibility
    );

    impl_get_prop!(
        get_scrollbar_fade_delay,
        ScrollbarFadeDelayValue,
        ScrollbarFadeDelay,
        as_scrollbar_fade_delay
    );

    impl_get_prop!(
        get_scrollbar_fade_duration,
        ScrollbarFadeDurationValue,
        ScrollbarFadeDuration,
        as_scrollbar_fade_duration
    );

    impl_get_prop!(
        get_visibility,
        StyleVisibilityValue,
        Visibility,
        as_visibility
    );

    impl_get_prop!(
        get_break_before,
        PageBreakValue,
        BreakBefore,
        as_break_before
    );

    impl_get_prop!(get_break_after, PageBreakValue, BreakAfter, as_break_after);

    impl_get_prop!(
        get_break_inside,
        BreakInsideValue,
        BreakInside,
        as_break_inside
    );

    impl_get_prop!(get_orphans, OrphansValue, Orphans, as_orphans);

    impl_get_prop!(get_widows, WidowsValue, Widows, as_widows);

    impl_get_prop!(
        get_box_decoration_break,
        BoxDecorationBreakValue,
        BoxDecorationBreak,
        as_box_decoration_break
    );

    impl_get_prop!(
        get_column_count,
        ColumnCountValue,
        ColumnCount,
        as_column_count
    );

    impl_get_prop!(
        get_column_width,
        ColumnWidthValue,
        ColumnWidth,
        as_column_width
    );

    impl_get_prop!(get_column_span, ColumnSpanValue, ColumnSpan, as_column_span);

    impl_get_prop!(get_column_fill, ColumnFillValue, ColumnFill, as_column_fill);

    impl_get_prop!(
        get_column_rule_width,
        ColumnRuleWidthValue,
        ColumnRuleWidth,
        as_column_rule_width
    );

    impl_get_prop!(
        get_column_rule_style,
        ColumnRuleStyleValue,
        ColumnRuleStyle,
        as_column_rule_style
    );

    impl_get_prop!(
        get_column_rule_color,
        ColumnRuleColorValue,
        ColumnRuleColor,
        as_column_rule_color
    );

    impl_get_prop!(get_flow_into, FlowIntoValue, FlowInto, as_flow_into);

    impl_get_prop!(get_flow_from, FlowFromValue, FlowFrom, as_flow_from);
    impl_get_prop!(get_wrap_through, WrapThroughValue, WrapThrough, as_wrap_through);
    impl_get_prop!(get_wrap_flow, WrapFlowValue, WrapFlow, as_wrap_flow);
    impl_get_prop!(get_region_fragment, RegionFragmentValue, RegionFragment, as_region_fragment);

    impl_get_prop!(
        get_shape_margin,
        ShapeMarginValue,
        ShapeMargin,
        as_shape_margin
    );

    impl_get_prop!(
        get_shape_image_threshold,
        ShapeImageThresholdValue,
        ShapeImageThreshold,
        as_shape_image_threshold
    );

    impl_get_prop!(get_content, ContentValue, Content, as_content);

    impl_get_prop!(
        get_counter_reset,
        CounterResetValue,
        CounterReset,
        as_counter_reset
    );

    impl_get_prop!(
        get_counter_increment,
        CounterIncrementValue,
        CounterIncrement,
        as_counter_increment
    );

    impl_get_prop!(get_counter_set, CounterSetValue, CounterSet, as_counter_set);
    impl_get_prop!(get_quotes, StyleQuotesValue, Quotes, as_quotes);
    impl_get_prop!(
        get_background_origin,
        StyleBackgroundOriginVecValue,
        BackgroundOrigin,
        as_background_origin
    );
    impl_get_prop!(
        get_background_color,
        StyleBackgroundContentValue,
        BackgroundColor,
        as_background_color
    );

    impl_get_prop!(get_string_set, StringSetValue, StringSet, as_string_set);
    impl_get_prop!(
        get_text_align,
        StyleTextAlignValue,
        TextAlign,
        as_text_align
    );
    impl_get_prop!(
        get_user_select,
        StyleUserSelectValue,
        UserSelect,
        as_user_select
    );
    impl_get_prop!(
        get_text_decoration,
        StyleTextDecorationValue,
        TextDecoration,
        as_text_decoration
    );
    impl_get_prop!(
        get_vertical_align,
        StyleVerticalAlignValue,
        VerticalAlign,
        as_vertical_align
    );
    impl_get_prop!(
        get_line_height,
        StyleLineHeightValue,
        LineHeight,
        as_line_height
    );
    impl_get_prop!(
        get_letter_spacing,
        StyleLetterSpacingValue,
        LetterSpacing,
        as_letter_spacing
    );
    impl_get_prop!(
        get_word_spacing,
        StyleWordSpacingValue,
        WordSpacing,
        as_word_spacing
    );
    impl_get_prop!(get_tab_size, StyleTabSizeValue, TabSize, as_tab_size);
    impl_get_prop!(get_cursor, StyleCursorValue, Cursor, as_cursor);
    impl_get_prop!(
        get_box_shadow_left,
        StyleBoxShadowValue,
        BoxShadowLeft,
        as_box_shadow_left
    );
    impl_get_prop!(
        get_box_shadow_right,
        StyleBoxShadowValue,
        BoxShadowRight,
        as_box_shadow_right
    );
    impl_get_prop!(
        get_box_shadow_top,
        StyleBoxShadowValue,
        BoxShadowTop,
        as_box_shadow_top
    );
    impl_get_prop!(
        get_box_shadow_bottom,
        StyleBoxShadowValue,
        BoxShadowBottom,
        as_box_shadow_bottom
    );
    impl_get_prop!(
        get_border_top_color,
        StyleBorderTopColorValue,
        BorderTopColor,
        as_border_top_color
    );
    impl_get_prop!(
        get_border_left_color,
        StyleBorderLeftColorValue,
        BorderLeftColor,
        as_border_left_color
    );
    impl_get_prop!(
        get_border_right_color,
        StyleBorderRightColorValue,
        BorderRightColor,
        as_border_right_color
    );
    impl_get_prop!(
        get_border_bottom_color,
        StyleBorderBottomColorValue,
        BorderBottomColor,
        as_border_bottom_color
    );
    impl_get_prop!(
        get_border_top_style,
        StyleBorderTopStyleValue,
        BorderTopStyle,
        as_border_top_style
    );
    impl_get_prop!(
        get_border_left_style,
        StyleBorderLeftStyleValue,
        BorderLeftStyle,
        as_border_left_style
    );
    impl_get_prop!(
        get_border_right_style,
        StyleBorderRightStyleValue,
        BorderRightStyle,
        as_border_right_style
    );
    impl_get_prop!(
        get_border_bottom_style,
        StyleBorderBottomStyleValue,
        BorderBottomStyle,
        as_border_bottom_style
    );
    impl_get_prop!(
        get_border_top_left_radius,
        StyleBorderTopLeftRadiusValue,
        BorderTopLeftRadius,
        as_border_top_left_radius
    );
    impl_get_prop!(
        get_border_top_right_radius,
        StyleBorderTopRightRadiusValue,
        BorderTopRightRadius,
        as_border_top_right_radius
    );
    impl_get_prop!(
        get_border_bottom_left_radius,
        StyleBorderBottomLeftRadiusValue,
        BorderBottomLeftRadius,
        as_border_bottom_left_radius
    );
    impl_get_prop!(
        get_border_bottom_right_radius,
        StyleBorderBottomRightRadiusValue,
        BorderBottomRightRadius,
        as_border_bottom_right_radius
    );
    impl_get_prop!(get_opacity, StyleOpacityValue, Opacity, as_opacity);
    impl_get_prop!(get_zoom, StyleZoomValue, Zoom, as_zoom);
    impl_get_prop!(
        get_background_clip,
        StyleBackgroundClipVecValue,
        BackgroundClip,
        as_background_clip
    );
    impl_get_prop!(
        get_font_variant_numeric,
        StyleFontVariantNumericValue,
        FontVariantNumeric,
        as_font_variant_numeric
    );
    impl_get_prop!(
        get_font_variant_caps,
        StyleFontVariantCapsValue,
        FontVariantCaps,
        as_font_variant_caps
    );
    impl_get_prop!(
        get_font_feature_settings,
        StyleFontFeatureSettingsValue,
        FontFeatureSettings,
        as_font_feature_settings
    );
    impl_get_prop!(
        get_transform,
        StyleTransformVecValue,
        Transform,
        as_transform
    );
    impl_get_prop!(
        get_transform_origin,
        StyleTransformOriginValue,
        TransformOrigin,
        as_transform_origin
    );
    impl_get_prop!(
        get_perspective_origin,
        StylePerspectiveOriginValue,
        PerspectiveOrigin,
        as_perspective_origin
    );
    impl_get_prop!(
        get_backface_visibility,
        StyleBackfaceVisibilityValue,
        BackfaceVisibility,
        as_backface_visibility
    );
    impl_get_prop!(
        get_app_region,
        StyleAppRegionValue,
        AppRegion,
        as_app_region
    );
    impl_get_prop!(
        get_spatial_navigation_action,
        StyleSpatialNavigationActionValue,
        SpatialNavigationAction,
        as_spatial_navigation_action
    );
    impl_get_prop!(
        get_spatial_navigation_contain,
        StyleSpatialNavigationContainValue,
        SpatialNavigationContain,
        as_spatial_navigation_contain
    );
    impl_get_prop!(
        get_spatial_navigation_function,
        StyleSpatialNavigationFunctionValue,
        SpatialNavigationFunction,
        as_spatial_navigation_function
    );
    impl_get_prop!(get_display, LayoutDisplayValue, Display, as_display);
    impl_get_prop!(get_float, LayoutFloatValue, Float, as_float);
    impl_get_prop!(
        get_box_sizing,
        LayoutBoxSizingValue,
        BoxSizing,
        as_box_sizing
    );
    impl_get_prop!(get_width, LayoutWidthValue, Width, as_width);
    impl_get_prop!(get_height, LayoutHeightValue, Height, as_height);
    impl_get_prop!(get_min_width, LayoutMinWidthValue, MinWidth, as_min_width);
    impl_get_prop!(
        get_min_height,
        LayoutMinHeightValue,
        MinHeight,
        as_min_height
    );
    impl_get_prop!(get_max_width, LayoutMaxWidthValue, MaxWidth, as_max_width);
    impl_get_prop!(
        get_max_height,
        LayoutMaxHeightValue,
        MaxHeight,
        as_max_height
    );
    impl_get_prop!(get_position, LayoutPositionValue, Position, as_position);
    impl_get_prop!(get_top, LayoutTopValue, Top, as_top);
    impl_get_prop!(get_bottom, LayoutInsetBottomValue, Bottom, as_bottom);
    impl_get_prop!(get_right, LayoutRightValue, Right, as_right);
    impl_get_prop!(get_left, LayoutLeftValue, Left, as_left);
    impl_get_prop!(
        get_padding_top,
        LayoutPaddingTopValue,
        PaddingTop,
        as_padding_top
    );
    impl_get_prop!(
        get_padding_bottom,
        LayoutPaddingBottomValue,
        PaddingBottom,
        as_padding_bottom
    );
    impl_get_prop!(
        get_padding_left,
        LayoutPaddingLeftValue,
        PaddingLeft,
        as_padding_left
    );
    impl_get_prop!(
        get_padding_right,
        LayoutPaddingRightValue,
        PaddingRight,
        as_padding_right
    );
    impl_get_prop!(
        get_margin_top,
        LayoutMarginTopValue,
        MarginTop,
        as_margin_top
    );
    impl_get_prop!(
        get_margin_bottom,
        LayoutMarginBottomValue,
        MarginBottom,
        as_margin_bottom
    );
    impl_get_prop!(
        get_margin_left,
        LayoutMarginLeftValue,
        MarginLeft,
        as_margin_left
    );
    impl_get_prop!(
        get_margin_right,
        LayoutMarginRightValue,
        MarginRight,
        as_margin_right
    );
    impl_get_prop!(
        get_border_top_width,
        LayoutBorderTopWidthValue,
        BorderTopWidth,
        as_border_top_width
    );
    impl_get_prop!(
        get_border_left_width,
        LayoutBorderLeftWidthValue,
        BorderLeftWidth,
        as_border_left_width
    );
    impl_get_prop!(
        get_border_right_width,
        LayoutBorderRightWidthValue,
        BorderRightWidth,
        as_border_right_width
    );
    impl_get_prop!(
        get_border_bottom_width,
        LayoutBorderBottomWidthValue,
        BorderBottomWidth,
        as_border_bottom_width
    );
    impl_get_prop!(
        get_overflow_x,
        LayoutOverflowValue,
        OverflowX,
        as_overflow_x
    );
    impl_get_prop!(
        get_overflow_y,
        LayoutOverflowValue,
        OverflowY,
        as_overflow_y
    );
    impl_get_prop!(
        get_overflow_block,
        LayoutOverflowValue,
        OverflowBlock,
        as_overflow_block
    );
    impl_get_prop!(
        get_overflow_inline,
        LayoutOverflowValue,
        OverflowInline,
        as_overflow_inline
    );
    impl_get_prop!(
        get_flex_direction,
        LayoutFlexDirectionValue,
        FlexDirection,
        as_flex_direction
    );
    impl_get_prop!(get_flex_wrap, LayoutFlexWrapValue, FlexWrap, as_flex_wrap);
    impl_get_prop!(get_flex_grow, LayoutFlexGrowValue, FlexGrow, as_flex_grow);
    impl_get_prop!(
        get_flex_shrink,
        LayoutFlexShrinkValue,
        FlexShrink,
        as_flex_shrink
    );
    impl_get_prop!(
        get_justify_content,
        LayoutJustifyContentValue,
        JustifyContent,
        as_justify_content
    );
    impl_get_prop!(
        get_align_items,
        LayoutAlignItemsValue,
        AlignItems,
        as_align_items
    );
    impl_get_prop!(
        get_align_content,
        LayoutAlignContentValue,
        AlignContent,
        as_align_content
    );
    impl_get_prop!(
        get_mix_blend_mode,
        StyleMixBlendModeValue,
        MixBlendMode,
        as_mix_blend_mode
    );
    impl_get_prop!(get_filter, StyleFilterVecValue, Filter, as_filter);
    impl_get_prop!(
        get_backdrop_filter,
        StyleFilterVecValue,
        BackdropFilter,
        as_backdrop_filter
    );
    impl_get_prop!(
        get_text_shadow,
        StyleBoxShadowValue,
        TextShadow,
        as_text_shadow
    );
    impl_get_prop!(
        get_list_style_type,
        StyleListStyleTypeValue,
        ListStyleType,
        as_list_style_type
    );
    impl_get_prop!(
        get_list_style_position,
        StyleListStylePositionValue,
        ListStylePosition,
        as_list_style_position
    );
    impl_get_prop!(
        get_table_layout,
        LayoutTableLayoutValue,
        TableLayout,
        as_table_layout
    );
    impl_get_prop!(
        get_border_collapse,
        StyleBorderCollapseValue,
        BorderCollapse,
        as_border_collapse
    );
    impl_get_prop!(
        get_border_spacing,
        LayoutBorderSpacingValue,
        BorderSpacing,
        as_border_spacing
    );
    impl_get_prop!(
        get_caption_side,
        StyleCaptionSideValue,
        CaptionSide,
        as_caption_side
    );
    impl_get_prop!(
        get_empty_cells,
        StyleEmptyCellsValue,
        EmptyCells,
        as_empty_cells
    );

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

    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose parser/builder/dispatch
                                     // (one branch per input variant)
    fn resolve_property_dependency(
        target_property: &CssProperty,
        reference_property: &CssProperty,
    ) -> Option<CssProperty> {
        // wildcard import: this big property-dispatch match references the full set
        // of layout property types; enumerating them all is unmaintainable.
        #[allow(clippy::wildcard_imports)]
        use azul_css::{
            css::CssPropertyValue,
            props::{
                basic::{font::StyleFontSize, length::SizeMetric, pixel::PixelValue},
                layout::*,
                style::{SelectionRadius, StyleLetterSpacing, StyleWordSpacing},
            },
        };


        let target_pixel_value = pixel_value_of(target_property)?;
        let reference_pixel_value = pixel_value_of(reference_property)?;

        // Convert reference to absolute pixels first, by the one conversion
        // of the absolute units (`PixelValue::absolute_px`, shared with the
        // compact cache). The reference can't be relative (em/rem/%) or
        // viewport-relative: `None` for those.
        let reference_px = reference_pixel_value.absolute_px()?;

        // Resolve target based on reference
        let resolved_px = match target_pixel_value.metric {
            // The absolute units: the same conversion.
            SizeMetric::Px | SizeMetric::Pt | SizeMetric::In | SizeMetric::Cm | SizeMetric::Mm => {
                target_pixel_value.absolute_px()?
            }
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
    ///
    /// Idempotent under a context change: the defaults are answered from
    /// [`Self::dynamic_context`] (the theme picks the `<hr>` rule, the native
    /// button border, the root text colour), and a second call — a theme
    /// flip on a retained DOM — first removes the entries the previous call
    /// pushed (`StatefulCssProperty::ua_origin`) and answers them again.
    /// Leaves `cascaded_props` sorted and flattened.
    ///
    /// A UA default is a DECLARED value (CSS Cascade 4: the user-agent
    /// origin), so on an ELEMENT it beats what the element would inherit: a
    /// link under `<td style="color:#333">` is `#0000EE`, `<code>` under
    /// `font-family: serif` is monospace. Only a TEXT node lets what it
    /// inherited win over its own row (its I-beam cursor yields to a
    /// button's pointer). Without the tree, the descendants of such an
    /// element keep the stale value the inheritance walk handed them; the
    /// cascade calls [`Self::apply_ua_css_in_tree`].
    pub fn apply_ua_css(&mut self, node_data: &[NodeData]) {
        self.apply_ua_css_inner(node_data, None);
    }

    /// [`Self::apply_ua_css`], and then every node below an element whose UA
    /// default overrode an inherited value inherits that default instead
    /// ([`Self::hand_ua_defaults_down`]). What `StyledDom`'s cascade runs.
    pub fn apply_ua_css_in_tree(
        &mut self,
        node_data: &[NodeData],
        node_hierarchy: &[NodeHierarchyItem],
    ) {
        self.apply_ua_css_inner(node_data, Some(node_hierarchy));
    }

    fn apply_ua_css_inner(
        &mut self,
        node_data: &[NodeData],
        node_hierarchy: Option<&[NodeHierarchyItem]>,
    ) {
        use azul_css::dynamic_selector::PseudoStateType;

        let node_count = node_data.len();
        if node_count == 0 {
            return;
        }

        self.strip_ua_entries();
        // `push_to` needs the build phase; a cache that has been through the
        // compact build (or a prune) is flattened.
        self.cascaded_props.ensure_build_phase();

        let prop_set = self.normal_props_present(node_data);
        let ctx = self.dynamic_context.as_deref();

        // Apply UA CSS: only insert for property types not yet set (bitset
        // check = O(1)). The ONE list both cascade passes walk (the compact
        // builder reads the same const): a UA property present in one pass
        // but not the other made the two readers disagree about a node's
        // computed value. The document root additionally carries the
        // document-wide defaults (the inherited text colour) — themed, and
        // IN the cascade from here on, so `computed_values` inherits it down
        // to every text node and no paint-time reader has to guess it. A
        // HOSTED root (`inherited_from_host`) inherits its host's value
        // instead, for every type the host supplies.
        for (node_index, node) in node_data.iter().enumerate() {
            let present = &prop_set[node_index];
            for prop_type in crate::ua_css::UA_PROPERTY_TYPES {
                if prop_type_bit_test(present, *prop_type) {
                    continue;
                }
                let is_root =
                    takes_root_ua_default(node_index, *prop_type, &self.inherited_from_host);
                let Some(ua_prop) =
                    crate::ua_css::get_ua_default(node, is_root, *prop_type, ctx)
                else {
                    continue;
                };
                // No bitset write needed: `UA_PROPERTY_TYPES` has no
                // duplicates (pinned by `one_themed_ua_table` tests), so one
                // node sees each type once. On an element this may sit beside
                // the value the inheritance walk handed it: pushed LAST, the
                // UA default is the one `sort_cascaded_props` keeps.
                self.cascaded_props.push_to(
                    node_index,
                    StatefulCssProperty {
                        state: PseudoStateType::Normal,
                        prop_type: *prop_type,
                        property: ua_prop.clone(),
                        ua_origin: true,
                        priority: 0,
                    },
                );
            }
        }

        if let Some(node_hierarchy) = node_hierarchy {
            self.hand_ua_defaults_down(node_data, node_hierarchy);
        }

        // Back to the read phase the rest of the pipeline expects
        // (`append` merges two flattened stores; a build-phase half would
        // drop the other's data). Sorted, so the slow path's binary search
        // over `cascaded_props` stays valid after a re-application too.
        self.sort_cascaded_props();
        self.ua_applied = true;
        self.cascade_epoch = self.cascade_epoch.wrapping_add(1);
    }

    /// RE-APPLICATION of the UA pass: strip what the previous pass pushed
    /// before answering again.
    ///
    /// The UA defaults depend on the context's theme, and the presence
    /// bitset treats an existing entry of a type as "already set" — so
    /// without this a theme flip found the stale light entry, skipped the
    /// dark twin, and `cascaded_props` never moved (the non-idempotence the
    /// theme-chain analysis of 2026-09-12 traces the whole black-text-on-dark
    /// family back to). Only `ua_origin` entries go; everything the author
    /// cascade and the inheritance walk produced stays, exactly as it was
    /// when the first pass ran after them. A no-op before the first pass.
    fn strip_ua_entries(&mut self) {
        if !self.ua_applied {
            return;
        }
        if self.cascaded_props.is_flattened() {
            self.cascaded_props.retain(|p| !p.ua_origin);
        } else {
            for v in self.cascaded_props.build_iter_mut() {
                v.retain(|p| !p.ua_origin);
            }
        }
    }

    /// The inheritance walk of [`Self::restyle`] runs BEFORE the UA pass: it
    /// handed every node a COPY of its parent's value as the parent stood
    /// then. Where an element's UA default has since shadowed that value on
    /// the element (a link's `color` under `<td style="color:#333">`,
    /// `<code>`'s `font-family` under `font-family: serif`), the copies below
    /// it are STALE: the text of the link painted the wrapper's grey.
    ///
    /// The stale copies are REMOVED, nothing is added: a node without a
    /// cascaded entry inherits its parent's computed value
    /// ([`Self::compute_inherited_values`], the compact builder alike), which
    /// is the element's UA default - so the child INHERITS the colour and
    /// never owns it (a UA `color` is on the root and inherited below; a
    /// child that owned one would block an author colour above it). Only a
    /// copy EQUAL to the shadowed value goes: a copy of something else (the
    /// parent's live conditional inline declaration, which the walk resolves
    /// and the bitset of declared types cannot see) is right and stays.
    ///
    /// One pre-order pass per inheritable UA type, the parent settled before
    /// the child: `stale[p]` is the value the copies under `p` are stale
    /// copies of, set where a UA push shadows the walk copy on `p` and
    /// carried down through every node whose copy was removed; a node that
    /// declares the type ends it (its children copied the declaration).
    fn hand_ua_defaults_down(
        &mut self,
        node_data: &[NodeData],
        node_hierarchy: &[NodeHierarchyItem],
    ) {
        use azul_css::dynamic_selector::PseudoStateType;

        let node_count = node_data
            .len()
            .min(node_hierarchy.len())
            .min(self.cascaded_props.len());
        let types: Vec<CssPropertyType> = crate::ua_css::UA_PROPERTY_TYPES
            .iter()
            .copied()
            .filter(|t| t.is_inheritable() && !is_resolved_parent_inherited(*t))
            .collect();
        let declared = self.declared_normal_props(node_data);
        let mut stale: Vec<Option<CssProperty>> = vec![None; node_count];
        for &ty in &types {
            for s in &mut stale {
                *s = None;
            }
            for node_index in 0..node_count {
                if prop_type_bit_test(&declared[node_index], ty) {
                    continue;
                }
                let is_normal_of = |e: &StatefulCssProperty| {
                    e.state == PseudoStateType::Normal && e.prop_type == ty
                };
                // The walk's copy (never `ua_origin`) and the UA pass's push.
                let (copy, ua) =
                    self.cascaded_props
                        .build_get(node_index)
                        .map_or((None, None), |v| {
                            let copy = v
                                .iter()
                                .rev()
                                .find(|e| is_normal_of(e) && !e.ua_origin)
                                .map(|e| clone_inheritable_property(&e.property));
                            let ua = v
                                .iter()
                                .rev()
                                .find(|e| is_normal_of(e) && e.ua_origin)
                                .map(|e| clone_inheritable_property(&e.property));
                            (copy, ua)
                        });
                // Pre-order arena: the parent is settled before the child.
                let inherited_stale = node_hierarchy[node_index]
                    .parent_id()
                    .map(|p| p.index())
                    .filter(|p| *p < node_index)
                    .and_then(|p| stale[p].clone());
                let Some(copy) = copy else {
                    continue;
                };
                if inherited_stale.as_ref() == Some(&copy) {
                    // A stale copy: out, so this node inherits. Its children
                    // copied the same value.
                    self.cascaded_props
                        .build_mut(node_index)
                        .retain(|e| !is_normal_of(e) || e.ua_origin);
                    stale[node_index] = Some(copy);
                } else if ua.is_some_and(|u| u != copy) {
                    // The UA push shadows the copy on this element (pushed
                    // last, it is the one the sort keeps); the children hold
                    // the copy's value.
                    stale[node_index] = Some(copy);
                }
            }
        }
    }

    /// Per node, which property types already have a Normal-state value from
    /// a layer that beats the UA sheet: author css (`css_props`), unconditional
    /// inline declarations, the global `*` bucket - and, on a TEXT node only,
    /// what it inherited (`cascaded_props`): a text node has no UA sheet of
    /// its own in CSS, and its one row here (the I-beam cursor) must yield to
    /// an inherited value (a button's pointer). On an ELEMENT the UA default
    /// is a declared value and beats the inherited one.
    ///
    /// A `* { margin: 0 }` reset is author CSS and must beat UA defaults on
    /// every ELEMENT (origin beats specificity), but it is stored once
    /// globally rather than per node, so a per-node scan alone never saw it -
    /// the UA body margin (8px) survived the classic reset and every page
    /// using it rendered shifted against the browser reference. Text nodes
    /// are exempt: `*` matches elements only (the compact builder makes the
    /// same distinction), and UA defaults for text nodes must stay.
    fn normal_props_present(&self, node_data: &[NodeData]) -> Vec<PropTypeBits> {
        use azul_css::dynamic_selector::PseudoStateType;

        let mut prop_set = self.declared_normal_props(node_data);
        for (node_idx, props) in self.cascaded_props.iter_node_slices() {
            if !node_data.get(node_idx).is_some_and(NodeData::is_text_node) {
                continue;
            }
            for p in props.iter().filter(|p| p.state == PseudoStateType::Normal) {
                prop_type_bit_set(&mut prop_set[node_idx], p.prop_type);
            }
        }
        prop_set
    }

    /// Per node, which property types it DECLARES in the Normal state: author
    /// css (`css_props`), an unconditional inline declaration, and - on an
    /// element - the global `*` bucket. (What it inherited is not declared.)
    fn declared_normal_props(&self, node_data: &[NodeData]) -> Vec<PropTypeBits> {
        use azul_css::dynamic_selector::PseudoStateType;

        let mut prop_set: Vec<PropTypeBits> = vec![[0u128; 2]; node_data.len()];

        for (node_idx, props) in self.css_props.iter_node_slices() {
            if node_idx >= prop_set.len() {
                continue;
            }
            for p in props.iter().filter(|p| p.state == PseudoStateType::Normal) {
                prop_type_bit_set(&mut prop_set[node_idx], p.prop_type);
            }
        }
        for (node_idx, node) in node_data.iter().enumerate() {
            for (prop, conds) in self.inline_properties(node, node_idx) {
                if conds.as_slice().is_empty() {
                    prop_type_bit_set(&mut prop_set[node_idx], prop.get_type());
                }
            }
        }
        if !self.global_css_props.is_empty() {
            let mut global_bits: PropTypeBits = [0u128; 2];
            for p in &self.global_css_props {
                prop_type_bit_set(&mut global_bits, p.get_type());
            }
            for (node_idx, node) in node_data.iter().enumerate() {
                if star_selects(node) {
                    prop_set[node_idx][0] |= global_bits[0];
                    prop_set[node_idx][1] |= global_bits[1];
                }
            }
        }
        prop_set
    }

    /// Sort `cascaded_props` by (state, `prop_type`) and flatten into contiguous memory.
    /// Must be called after `apply_ua_css()` which adds entries to `cascaded_props`.
    pub fn sort_cascaded_props(&mut self) {
        self.cascaded_props
            .sort_each_and_flatten(|p| (p.state, p.prop_type));
    }

    /// The inheritable values node `index` holds because of a state it is in
    /// (`:hover`, `:focus`, ...) rather than at rest: for each inheritable
    /// property it declares for some pseudo-state, the declaration that wins
    /// for the states it is in. Empty for a node at rest, the common case.
    ///
    /// Inheritance is computed from the resting cascade; this is what a
    /// node's children take from it on top, so a `.bar:hover { color: red }`
    /// reaches the text inside the hovered bar as it does in a browser.
    #[must_use]
    pub fn state_inherited_values(
        &self,
        node_data: &NodeData,
        index: usize,
        state: &StyledNodeState,
    ) -> Vec<CssProperty> {
        use azul_css::dynamic_selector::{DynamicSelector, PseudoStateType};
        if state.is_normal() {
            return Vec::new();
        }
        let mut types: Vec<CssPropertyType> = Vec::new();
        let mut note = |ty: CssPropertyType| {
            if ty.is_inheritable() && !types.contains(&ty) {
                types.push(ty);
            }
        };
        for p in self.css_props.get_slice(index) {
            if p.state != PseudoStateType::Normal {
                note(p.prop_type);
            }
        }
        for p in self.cascaded_props.get_slice(index) {
            if p.state != PseudoStateType::Normal {
                note(p.prop_type);
            }
        }
        for (p, conds) in self.inline_properties(node_data, index) {
            let stateful = conds.as_slice().iter().any(|c| {
                matches!(c, DynamicSelector::PseudoState(s) if *s != PseudoStateType::Normal)
            });
            if stateful {
                note(p.get_type());
            }
        }
        let id = NodeId::new(index);
        types
            .iter()
            .filter_map(|ty| self.get_declared_property(node_data, &id, state, ty))
            .filter(|p| p.css_wide_keyword().is_none())
            .cloned()
            .collect()
    }

    /// Compute inherited values for all nodes in the DOM tree.
    ///
    /// Implements CSS inheritance: walk tree depth-first, apply cascade priority
    /// (inherited → cascaded → css → inline → user), create dependency chains for
    /// relative values. Call `apply_ua_css()` before this function.
    // large but cohesive: one pass over one structure
    #[allow(clippy::too_many_lines)]
    // the sequence reads in the order the work happens: an `if` expression would hide the early path
    #[allow(clippy::useless_let_if_seq)]
    pub fn compute_inherited_values(
        &mut self,
        node_hierarchy: &[NodeHierarchyItem],
        node_data: &[NodeData],
        node_states: &[StyledNodeState],
    ) -> Vec<NodeId> {
        self.computed_values.grow_to(node_hierarchy.len());
        // The cascade rewrites every node, and the transposed store appends to
        // per-value node lists — so it starts empty or the previous pass's
        // nodes would still be listed. Change detection compares against the
        // snapshot taken below, not against the live store.
        let previous = core::mem::replace(&mut self.computed_values, InheritedValues::new());
        self.computed_values.carry_work_from(&previous);
        self.computed_values.grow_to(node_hierarchy.len());
        // The keywords' values are found node by node below, each from its parent's.
        self.css_wide_values.clear();
        // The values a node in a state hands its children on top of its
        // resting ones (Step 5b): few nodes are in a state at a time.
        let mut state_overlays: Vec<(usize, Vec<CssProperty>)> = Vec::new();
        // The inheritable types a node shows its children FROM A STATE: what
        // it holds for a state of its own (`state_overlays`) and what came to
        // it from a state above and it declares nothing against. A child's
        // copy of the resting value does not win over those
        // (`apply_cascade_properties`). Empty while no node in a state hands
        // a value down.
        let mut from_a_state: alloc::collections::BTreeMap<usize, Vec<CssPropertyType>> =
            alloc::collections::BTreeMap::new();
        // The nodes whose children the walk is in, outermost first: each
        // with the values it stored (what `values_for` reads back for it)
        // and the buckets they went to. The arena is in document order, so
        // a node's parent is on top once the subtrees that ended before the
        // node are popped. Reading the parent's values back from the store
        // instead cost every node a probe of every distinct value of the
        // DOM, and placing a node's values a comparison with each of them.
        let mut ancestors: Vec<(usize, Vec<(CssPropertyType, CssPropertyWithOrigin)>, Vec<u32>)> =
            Vec::new();
        node_hierarchy
            .iter()
            .enumerate()
            .filter_map(|(node_index, hierarchy_item)| {
                let node_id = NodeId::new(node_index);
                let parent_id = hierarchy_item.parent_id();
                // Whether the parent's stored values are on top of
                // `ancestors` (not for an arena out of document order: its
                // nodes read the store, as every node did).
                let mut parent_on_top = false;
                if let Some(pid) = parent_id {
                    while ancestors
                        .last()
                        .is_some_and(|(node, _, _)| *node != pid.index())
                    {
                        ancestors.pop();
                    }
                    parent_on_top = !ancestors.is_empty();
                }
                // The parent's values as it shows them in the state it is in
                // (Step 5b), which is what it hands down.
                let overlay = parent_id
                    .and_then(|pid| state_overlays.iter().find(|(i, _)| *i == pid.index()));
                // A copy of the parent's values only where they are not the
                // stored ones on top of `ancestors`: under a state overlay,
                // read back from the store, or a HOSTED root's - its parent
                // values are its host's (`inherited_from_host`): it inherits
                // them, and an `em` of its own resolves against the host's
                // font size.
                let parent_owned: Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>> =
                    match parent_id {
                        Some(_) if parent_on_top && overlay.is_none() => None,
                        Some(pid) => {
                            let mut values = if parent_on_top {
                                ancestors
                                    .last()
                                    .map(|(_, values, _)| values.clone())
                                    .unwrap_or_default()
                            } else {
                                self.computed_values.values_for(pid.index())
                            };
                            if let Some((_, held)) = overlay {
                                for property in held {
                                    let ty = property.get_type();
                                    let entry = (
                                        ty,
                                        CssPropertyWithOrigin {
                                            property: property.clone(),
                                            origin: CssPropertyOrigin::Own,
                                        },
                                    );
                                    match values.binary_search_by_key(&ty, |(k, _)| *k) {
                                        Ok(at) => values[at] = entry,
                                        Err(at) => values.insert(at, entry),
                                    }
                                }
                            }
                            Some(values)
                        }
                        None if !self.inherited_from_host.is_empty() => {
                            Some(self.inherited_from_host.clone())
                        }
                        None => None,
                    };
                let parent_computed: Option<&Vec<(CssPropertyType, CssPropertyWithOrigin)>> =
                    match parent_owned.as_ref() {
                        Some(owned) => Some(owned),
                        None if parent_on_top => ancestors.last().map(|(_, values, _)| values),
                        None => None,
                    };

                let mut ctx = InheritanceContext {
                    node_id,
                    parent_id,
                    computed_values: Vec::new(),
                };

                // What the node's CSS-wide keywords stand for, from its parent's values (all
                // known: pre-order), before its own values are computed from them.
                let keyword_values = self.css_wide_values_of_node(node_data, node_index, parent_id);
                self.css_wide_values.extend(keyword_values);

                // Step 1: Inherit from parent
                if let Some(parent_values) = parent_computed {
                    Self::inherit_from_parent(&mut ctx, parent_values);
                }

                // What the parent shows from a state, its own or one above.
                let held_above: Vec<CssPropertyType> =
                    parent_id.and_then(|pid| from_a_state.get(&pid.index())).map_or_else(Vec::new, Clone::clone);

                // Steps 2-5: Apply cascade in priority order
                self.apply_cascade_properties(
                    &mut ctx,
                    node_id,
                    parent_computed,
                    node_data,
                    node_index,
                    &held_above,
                );
                // Step 5b: what a state the node is in declares, for its
                // children to inherit (`state_inherited_values`). Kept out of
                // the node's own values, which stay its resting ones: the
                // restyle that ends the state compares against those.
                let mut hands_on: Vec<CssPropertyType> = Vec::new();
                if let Some(state) = node_states.get(node_index) {
                    let held = self.state_inherited_values(&node_data[node_index], node_index, state);
                    if !held.is_empty() {
                        hands_on.extend(held.iter().map(CssProperty::get_type));
                        state_overlays.push((node_index, held));
                    }
                }
                // ... and what it took from a state above without declaring
                // the property itself: its children take that from it too.
                for ty in &held_above {
                    if !hands_on.contains(ty) && holds_inherited(&ctx.computed_values, *ty) {
                        hands_on.push(*ty);
                    }
                }
                if !hands_on.is_empty() && hierarchy_item.last_child_id().is_some() {
                    from_a_state.insert(node_index, hands_on);
                }

                // Step 6: a `line-height` in `em` / `%` computes to a length
                // against the node's own font size, final only now.
                Self::compute_font_relative_line_height(&mut ctx);

                // Check for changes and store
                let near = if parent_on_top {
                    ancestors
                        .last()
                        .map(|(_, values, buckets)| (values.as_slice(), buckets.as_slice()))
                } else {
                    None
                };
                let (changed, stored, buckets) = self.store_if_changed(&ctx, &previous, near);
                if hierarchy_item.last_child_id().is_some() {
                    ancestors.push((node_index, stored, buckets));
                }
                changed.then_some(node_id)
            })
            .collect()
    }

    /// The values node `index`'s CSS-wide keyword declarations stand for (see
    /// [`Self::css_wide_values`]), in every tier and pseudo-state, its `parent`'s values
    /// already computed (pre-order).
    fn css_wide_values_of_node(
        &self,
        node_data: &[NodeData],
        index: usize,
        parent: Option<NodeId>,
    ) -> Vec<(
        (usize, CssPropertyType, azul_css::css::CssWideKeyword),
        CssProperty,
    )> {
        let mut declared: Vec<(CssPropertyType, azul_css::css::CssWideKeyword)> = Vec::new();
        let mut note = |p: &CssProperty| {
            if let Some(keyword) = p.css_wide_keyword() {
                let entry = (p.get_type(), keyword);
                if !declared.contains(&entry) {
                    declared.push(entry);
                }
            }
        };
        if let Some(user) = self.user_overridden_properties.get(index) {
            for (_, p) in user {
                note(p);
            }
        }
        for (p, _) in self.inline_properties(&node_data[index], index) {
            note(p);
        }
        for p in self.css_props.get_slice(index) {
            note(&p.property);
        }
        for p in self.cascaded_props.get_slice(index) {
            note(&p.property);
        }
        if star_selects(&node_data[index]) {
            for p in &self.global_css_props {
                note(p);
            }
        }
        declared
            .into_iter()
            .filter_map(|(ty, keyword)| {
                self.css_wide_value_for(node_data, index, parent, ty, keyword)
                    .map(|value| ((index, ty, keyword), value))
            })
            .collect()
    }

    /// The value `keyword` stands for as node `index`'s `ty`; `None` for the property's
    /// initial value where the readers take the keyword for it.
    fn css_wide_value_for(
        &self,
        node_data: &[NodeData],
        index: usize,
        parent: Option<NodeId>,
        ty: CssPropertyType,
        keyword: azul_css::css::CssWideKeyword,
    ) -> Option<CssProperty> {
        use azul_css::css::CssWideKeyword;

        let inherits = ty.is_inheritable();
        let from_parent = match keyword {
            CssWideKeyword::Inherit => true,
            CssWideKeyword::Unset => inherits,
            CssWideKeyword::Initial => false,
            // The author's declarations are rolled back to the user agent's (there is no user
            // style sheet): its value for the element, else as `unset`.
            CssWideKeyword::Revert => {
                let is_root = takes_root_ua_default(index, ty, &self.inherited_from_host);
                if let Some(ua) = crate::ua_css::get_ua_default(
                    &node_data[index],
                    is_root,
                    ty,
                    self.dynamic_context.as_deref(),
                ) {
                    return Some(ua.clone());
                }
                inherits
            }
        };
        if from_parent {
            self.parent_value(node_data, parent, ty)
        } else {
            initial_value_unlike_the_default(ty)
        }
    }

    /// The parent's value of `ty`, as `inherit` takes it: its computed value (a font-relative
    /// length in pixels). `None` where the parent is at the initial value, and for the root of
    /// a document (a hosted root's parent is its host).
    fn parent_value(
        &self,
        node_data: &[NodeData],
        parent: Option<NodeId>,
        ty: CssPropertyType,
    ) -> Option<CssProperty> {
        let value = match parent {
            None => self
                .inherited_from_host
                .iter()
                .find(|(t, _)| *t == ty)
                .map(|(_, v)| v.property.clone())?,
            Some(parent) => self.own_value(node_data, parent, ty)?,
        };
        // A parent at its initial value hands that on.
        value.css_wide_keyword().is_none().then_some(value)
    }

    /// Node `node`'s computed value of `ty`, as a child's `inherit` takes it.
    fn own_value(
        &self,
        node_data: &[NodeData],
        node: NodeId,
        ty: CssPropertyType,
    ) -> Option<CssProperty> {
        if ty.is_inheritable() {
            return self
                .computed_values
                .get(node.index(), ty)
                .map(|v| v.property.clone());
        }
        let declared = self.get_property_slow(
            &node_data[node.index()],
            &node,
            &StyledNodeState::default(),
            &ty,
        )?;
        // A length in `em` computes against the node's own font size.
        let em = pixel_value_of(declared)
            .is_some_and(|v| v.metric == azul_css::props::basic::length::SizeMetric::Em);
        let font_size = self
            .computed_values
            .get(node.index(), CssPropertyType::FontSize)
            .filter(|_| em);
        Some(
            font_size
                .and_then(|fs| Self::resolve_property_dependency(declared, &fs.property))
                .unwrap_or_else(|| declared.clone()),
        )
    }

    /// Inherit inheritable properties from parent node
    fn inherit_from_parent(
        ctx: &mut InheritanceContext,
        parent_values: &[(CssPropertyType, CssPropertyWithOrigin)],
    ) {
        for (prop_type, prop_with_origin) in
            parent_values.iter().filter(|(pt, _)| pt.is_inheritable())
        {
            let entry = (
                *prop_type,
                CssPropertyWithOrigin {
                    property: prop_with_origin.property.clone(),
                    origin: CssPropertyOrigin::Inherited,
                },
            );
            // Insert into sorted vec
            match ctx
                .computed_values
                .binary_search_by_key(prop_type, |(k, _)| *k)
            {
                Ok(idx) => ctx.computed_values[idx] = entry,
                Err(idx) => ctx.computed_values.insert(idx, entry),
            }
        }
    }

    /// Apply all cascade properties in priority order
    ///
    /// `from_a_state`: the inheritable types the node's PARENT shows it from a state - its
    /// own (`.row:hover { color }`) or one above it that it hands on. The node's inherited
    /// row holds a COPY of what its parent holds AT REST for each such type (the walk of
    /// the cascade copies resting values), and Step 2 takes a copy for a declaration of
    /// the node's own: applied, it put the resting colour back over the hover colour the
    /// node had just inherited, on every node below a hovered one whenever the pass ran
    /// over rows that still held their resting copies (a cascade before its prune:
    /// `recascade_nodes`, `restyle`). A copy of such a type is passed over; a UA default
    /// (a link's colour) is the node's own declaration and applies as ever.
    fn apply_cascade_properties(
        &self,
        ctx: &mut InheritanceContext,
        node_id: NodeId,
        parent_computed: Option<&Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
        node_data: &[NodeData],
        node_index: usize,
        from_a_state: &[CssPropertyType],
    ) {
        // Step 2: Cascaded properties (UA CSS)
        {
            let cascaded_slice = self.cascaded_props.get_slice(node_id.index());
            for p in cascaded_slice {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal
                    && (p.ua_origin || !from_a_state.contains(&p.prop_type))
                    && Self::should_apply_cascaded(&ctx.computed_values, p.prop_type, &p.property)
                {
                    Self::process_property(
                        ctx,
                        self.css_wide_value_of(node_index, &p.property)
                            .unwrap_or(&p.property),
                        parent_computed,
                    );
                }
            }
        }

        // Step 2b: the global `*` rules, which rank above the UA sheet and below every rule
        // matched node by node. An element's children inherit what they give it, as a text
        // node (which `*` does not select) inherits its parent's colour.
        if star_selects(&node_data[node_index]) {
            for p in &self.global_css_props {
                Self::process_property(
                    ctx,
                    self.css_wide_value_of(node_index, p).unwrap_or(p),
                    parent_computed,
                );
            }
        }

        // Step 3: CSS properties (stylesheets)
        {
            let css_slice = self.css_props.get_slice(node_id.index());
            for p in css_slice {
                if p.state == azul_css::dynamic_selector::PseudoStateType::Normal {
                    Self::process_property(
                        ctx,
                        self.css_wide_value_of(node_index, &p.property)
                            .unwrap_or(&p.property),
                        parent_computed,
                    );
                }
            }
        }

        // Step 4: Inline CSS properties.
        //
        // Conditional declarations take part in inheritance exactly when they
        // apply to the node's RESTING style under the window's context — the
        // same test `get_property` makes for the node's own value, minus every
        // pseudo-state (hover/focus/active are runtime overlays, never an
        // inherited default). Skipping every conditional declaration, as this
        // did, made a container's `dark_theme(color: ..)` twin invisible to
        // its children: the label under a dark-mode button inherited the light
        // value and painted dark-on-dark. In cascade order (theme rank, then
        // source order), last match wins, like the node's own resolution.
        let dyn_ctx = self.dynamic_context.as_deref();
        let no_context_theme = dyn_ctx.is_none().then(crate::app_theme::current_theme);
        let mut in_order = Vec::new();
        azul_css::css::inline_in_cascade_order(
            self.inline_properties(&node_data[node_index], node_index),
            |conds| rank_of(dyn_ctx, no_context_theme.as_ref(), conds),
            &mut in_order,
        );
        for (prop, conds) in in_order {
            let applies = conds.as_slice().iter().all(|c| match c {
                azul_css::dynamic_selector::DynamicSelector::PseudoState(s) => {
                    *s == azul_css::dynamic_selector::PseudoStateType::Normal
                }
                other => condition_holds(dyn_ctx, no_context_theme.as_ref(), other),
            });
            if applies {
                Self::process_property(
                    ctx,
                    self.css_wide_value_of(node_index, prop).unwrap_or(prop),
                    parent_computed,
                );
            }
        }

        // Step 5: User-overridden properties
        if let Some(user_props) = self.user_overridden_properties.get(node_id.index()) {
            for (_, prop) in user_props {
                Self::process_property(
                    ctx,
                    self.css_wide_value_of(node_index, prop).unwrap_or(prop),
                    parent_computed,
                );
            }
        }
    }

    /// Check if a cascaded property should be applied.
    ///
    /// A cascaded (UA / author-selector) value applies unless the node has
    /// already set its OWN value (`origin == Own`), which wins per the cascade.
    /// An `Inherited` placeholder must NOT block it — including a relative
    /// `font-size`: an earlier version skipped a cascaded relative font-size when
    /// an inherited value existed, which silently dropped `<h1>`'s UA
    /// `font-size: 2em` (and every heading), leaving headings at their parent's
    /// size. That was wrong: `resolve_font_size_property` resolves the `em`
    /// against the *parent's* font-size, not the inherited value, so there is no
    /// double-scaling — the cascaded relative size must apply and overwrite the
    /// inherited entry.
    fn should_apply_cascaded(
        computed: &[(CssPropertyType, CssPropertyWithOrigin)],
        prop_type: CssPropertyType,
        _prop: &CssProperty,
    ) -> bool {
        computed
            .binary_search_by_key(&prop_type, |(k, _)| *k)
            .map_or(true, |idx| {
                computed[idx].1.origin == CssPropertyOrigin::Inherited
            })
    }

    /// Process a single property: resolve and store
    fn process_property(
        ctx: &mut InheritanceContext,
        prop: &CssProperty,
        parent_computed: Option<&Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) {
        let prop_type = prop.get_type();

        let resolved = if prop_type == CssPropertyType::FontSize {
            Self::resolve_font_size_property(prop, parent_computed)
        } else if prop_type == CssPropertyType::FontWeight {
            Self::resolve_font_weight_property(prop, parent_computed)
        } else {
            Self::resolve_other_property(prop, &ctx.computed_values)
        };

        let entry = (
            prop_type,
            CssPropertyWithOrigin {
                property: resolved,
                origin: CssPropertyOrigin::Own,
            },
        );
        match ctx
            .computed_values
            .binary_search_by_key(&prop_type, |(k, _)| *k)
        {
            Ok(idx) => ctx.computed_values[idx] = entry,
            Err(idx) => ctx.computed_values.insert(idx, entry),
        }
    }

    /// A `line-height` in `em` / `%` computes to an absolute length against
    /// the node's OWN font size (CSS 2.2 s10.8.1) - known only once every
    /// cascade tier has run, so a `font-size` declared after it counts - and
    /// that length is what the descendants inherit from here (the raw value
    /// is kept out of their `cascaded_props`, see
    /// `inherits_its_computed_length`).
    fn compute_font_relative_line_height(ctx: &mut InheritanceContext) {
        use azul_css::{css::CssPropertyValue, props::basic::length::SizeMetric};

        let Ok(lh_idx) = ctx
            .computed_values
            .binary_search_by_key(&CssPropertyType::LineHeight, |(k, _)| *k)
        else {
            return;
        };
        let line_height = match &ctx.computed_values[lh_idx].1.property {
            CssProperty::LineHeight(CssPropertyValue::Exact(lh))
                if lh.is_font_relative_length() =>
            {
                *lh
            }
            _ => return,
        };
        let font_size_px = ctx
            .computed_values
            .binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k)
            .ok()
            .and_then(|idx| match &ctx.computed_values[idx].1.property {
                CssProperty::FontSize(v) => v
                    .get_property()
                    .filter(|fs| fs.inner.metric == SizeMetric::Px)
                    .map(|fs| fs.inner.number.get()),
                _ => None,
            })
            .unwrap_or(azul_css::props::basic::pixel::DEFAULT_FONT_SIZE);
        ctx.computed_values[lh_idx].1.property = CssProperty::LineHeight(
            CssPropertyValue::Exact(line_height.computed(font_size_px)),
        );
    }

    /// `font-weight: bolder` / `lighter` compute against the PARENT's
    /// computed weight (CSS Fonts 4 s2.2, `StyleFontWeight::computed`); the
    /// descendants inherit the resulting number. Any other weight is its own
    /// computed value.
    fn resolve_font_weight_property(
        prop: &CssProperty,
        parent_computed: Option<&Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) -> CssProperty {
        use azul_css::{css::CssPropertyValue, props::basic::font::StyleFontWeight};

        let CssProperty::FontWeight(CssPropertyValue::Exact(weight)) = prop else {
            return prop.clone();
        };
        if !weight.is_relative() {
            return prop.clone();
        }
        let parent_weight = parent_computed
            .and_then(|p| {
                p.binary_search_by_key(&CssPropertyType::FontWeight, |(k, _)| *k)
                    .ok()
                    .map(|idx| &p[idx].1.property)
            })
            .and_then(|p| match p {
                CssProperty::FontWeight(v) => v.get_property().copied(),
                _ => None,
            })
            .unwrap_or(StyleFontWeight::Normal);
        CssProperty::FontWeight(CssPropertyValue::Exact(weight.computed(parent_weight)))
    }

    /// Resolve font-size property (uses parent's font-size as reference)
    fn resolve_font_size_property(
        prop: &CssProperty,
        parent_computed: Option<&Vec<(CssPropertyType, CssPropertyWithOrigin)>>,
    ) -> CssProperty {
        let parent_font_size = parent_computed.and_then(|p| {
            p.binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k)
                .ok()
                .map(|idx| &p[idx].1)
        });

        parent_font_size.map_or_else(
            || {
                Self::resolve_font_size_to_pixels(
                    prop,
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                )
            },
            |pfs| {
                Self::resolve_property_dependency(prop, &pfs.property).unwrap_or_else(|| {
                    Self::resolve_font_size_to_pixels(
                        prop,
                        azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                    )
                })
            },
        )
    }

    /// Resolve other properties (uses current node's font-size as reference)
    fn resolve_other_property(
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
            // The absolute units, by the one conversion the compact cache
            // uses too (`PixelValue::absolute_px`): `1in` is 96 px in both.
            SizeMetric::Px | SizeMetric::Pt | SizeMetric::In | SizeMetric::Cm | SizeMetric::Mm => {
                let Some(px) = font_size.inner.absolute_px() else {
                    return prop.clone();
                };
                px
            }
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

        css_val.get_property().is_some_and(|fs| {
            matches!(
                fs.inner.metric,
                SizeMetric::Em | SizeMetric::Rem | SizeMetric::Percent
            )
        })
    }

    /// Store computed values if changed, returns true if values were updated
    /// Store the node's resolved values — INHERITABLE ONLY.
    ///
    /// `computed_values` is documented as "pre-resolved INHERITED properties",
    /// and every consumer treats it that way: `get_property_slow` consults it
    /// only when `css_property_type.is_inheritable()`, `inherit_from_parent`
    /// filters to inheritable before handing values down, and
    /// `transient::extract_subtree_as_dom` filters again on the way out
    /// ("non-inheritable entries must not travel"). But the cascade built the
    /// FULL set here and stored it, so 80% of the store was entries no reader
    /// could reach: on the 50k-node XHTML bench, 226 610 of 284 438 entries
    /// were `display` (one per node), `margin-*`, `padding-*` and
    /// `vertical-align` — none of them inheritable, none of them readable
    /// through the `is_inheritable()` gate.
    ///
    /// Filtering here is safe for the cascade because the only parent value the
    /// cascade reads back is `FontSize` (`process_property` →
    /// `resolve_font_size_property`), which is itself inheritable. Non-inherited
    /// properties are still answered by `cascaded_props` / `css_props` / the
    /// compact cache, which is where the layout path reads them from anyway.
    ///
    /// `near` is the node's parent's stored values and their buckets, when
    /// the caller has them ([`InheritedValues::set_node_near`]). Answers
    /// whether the values differ from `previous`'s, the values stored
    /// (ascending by type) and the bucket each went to.
    fn store_if_changed(
        &mut self,
        ctx: &InheritanceContext,
        previous: &InheritedValues,
        near: Option<(&[(CssPropertyType, CssPropertyWithOrigin)], &[u32])>,
    ) -> (bool, Vec<(CssPropertyType, CssPropertyWithOrigin)>, Vec<u32>) {
        let inheritable: Vec<(CssPropertyType, CssPropertyWithOrigin)> = ctx
            .computed_values
            .iter()
            .filter(|(pt, _)| pt.is_inheritable())
            .cloned()
            .collect();
        let changed = previous.values_for(ctx.node_id.index()) != inheritable;
        let buckets = self
            .computed_values
            .set_node_near(ctx.node_id.index(), &inheritable, near);
        (changed, inheritable, buckets)
    }
}

/// Context for computing inherited values for a single node
struct InheritanceContext {
    node_id: NodeId,
    parent_id: Option<NodeId>,
    computed_values: Vec<(CssPropertyType, CssPropertyWithOrigin)>,
}

/// Whether `computed` (a node's values, ascending by type) holds `ty` as an INHERITED
/// value: the node took it from its parent and declares nothing for it.
fn holds_inherited(
    computed: &[(CssPropertyType, CssPropertyWithOrigin)],
    ty: CssPropertyType,
) -> bool {
    computed
        .binary_search_by_key(&ty, |(k, _)| *k)
        .is_ok_and(|at| computed[at].1.origin == CssPropertyOrigin::Inherited)
}

impl CssPropertyCache {
    /// Clear the entire compact cache. Call after major DOM changes.
    pub(crate) fn invalidate_resolved_cache(&mut self) {
        self.compact_cache = None;
    }

    /// The inheritable types node `node` shows its children FROM A STATE, read from what
    /// the cache holds for it and its ancestors (a node whose rows and values a cascade
    /// over other nodes keeps, [`Self::recascade_nodes`]): what it holds for a state it is
    /// in itself ([`Self::state_inherited_values`]), and what an ancestor in a state hands
    /// down to it through nodes that declare nothing against it - each of them, `node`
    /// included, holds the type as an inherited value. What the inheritance pass keeps per
    /// node while it walks a whole DOM (`compute_inherited_values`), found again for one
    /// node: a walk up its ancestors, which asks a node at rest nothing.
    fn types_shown_from_a_state(
        &self,
        node_data: &[NodeData],
        node_hierarchy: &[NodeHierarchyItem],
        node_states: &[StyledNodeState],
        node: usize,
    ) -> Vec<CssPropertyType> {
        let mut found: Vec<CssPropertyType> = Vec::new();
        // The nodes from `node` up to, not including, the one asked: each has to hand a
        // type on for it to reach the children of `node`.
        let mut below: Vec<usize> = Vec::new();
        let mut at = Some(node);
        while let Some(index) = at {
            let (Some(data), Some(state)) = (node_data.get(index), node_states.get(index)) else {
                break;
            };
            for held in self.state_inherited_values(data, index, state) {
                let ty = held.get_type();
                if found.contains(&ty) {
                    continue;
                }
                let handed_on = below.iter().all(|passed| {
                    self.computed_values
                        .get(*passed, ty)
                        .is_some_and(|value| value.origin == CssPropertyOrigin::Inherited)
                });
                if handed_on {
                    found.push(ty);
                }
            }
            below.push(index);
            // Document order: a parent stands before its child (and a malformed arena
            // ends the walk instead of going round).
            at = node_hierarchy
                .get(index)
                .and_then(NodeHierarchyItem::parent_id)
                .map(|parent| parent.index())
                .filter(|parent| *parent < index);
        }
        found
    }
}

impl CssPropertyCache {
    /// Runs the cascade for the nodes `dirty` names (`dirty[node]`) and leaves every other
    /// node's rows as they are: its matched rules, what it inherited, its computed inherited
    /// values and its compact entries. What a rebuild that changed a few nodes of a DOM where
    /// they stand runs instead of [`Self::restyle_cascade`] and the passes that follow it
    /// (`StyledDom::restyle_changed_nodes`).
    ///
    /// SOUND when `dirty` holds every node whose cascade inputs moved since the rows were
    /// made, the tree, the sheets, the window's context and the nodes' states being what they
    /// were (`StyledDom::nodes_reached_by_changes` for a change of nodes' own data). Then, for
    /// a node that is not named, every stage below reads what it read before:
    /// - its matched rules (`css_props`) are a function of the rules in cascade order, the
    ///   node, its ancestors and the element siblings before it and before them, and the
    ///   states of those nodes (`crate::style::matches_html_element_remembering`);
    /// - what it inherited (`cascaded_props`) is a function of its parent's own style,
    ///   matched rules and inherited row, of its own type (the UA defaults) and of the rows
    ///   of its ancestors (the stale copies the UA pass removes);
    /// - its computed inherited values and its compact entries are functions of its own rows
    ///   and of its parent's values and entries.
    ///   By induction from the root a node whose every ancestor is unnamed keeps every row.
    ///
    /// The named nodes' rows are made as the whole-DOM passes make them, pass by pass, from
    /// copies of those passes' per-node steps. One thing has to be made again for nodes that
    /// are NOT named: the inherited row of each ancestor of a named node as it stood BEFORE
    /// the prune (`prune_compact_normal_props` drops the resting compact values from the kept
    /// rows, and a child copies them from its parent's row). They are rebuilt from the root
    /// down along those chains, used, and dropped: the ancestors keep the rows they have.
    ///
    /// `moved_in_place` (empty: none) names nodes that are NOT cascaded and whose own style
    /// moved in a way no row but their compact entries holds - the values of their insets,
    /// of an exact `transform` / `opacity` (`StyledDom::own_style_moved_in_place`): these
    /// properties do not inherit, a node's own declarations are read from its node data and
    /// not from the matched or inherited rows, and no selector reads a style. Their compact
    /// entries are built again with the named nodes'; their children copy only inheritable
    /// slots from them, which did not move, and keep their entries.
    ///
    /// Answers `false`, having changed nothing, where the whole cascade has to run instead:
    /// a cache that is not complete (no compact cache, rows in their build phase, a composed
    /// DOM), a custom property or `var()` anywhere (the variable pass resolves them DOM-wide),
    /// a generated node among the named ones, `*` rules that are no longer the ones the rows
    /// were made with, or more than half of the nodes named.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // the passes of a cascade, in order
    // the names follow the cascade's own vocabulary
    #[allow(clippy::similar_names)]
    // the sequence reads in the order the work happens: an `if` expression would hide the early path
    #[allow(clippy::useless_let_if_seq)]
    pub fn recascade_nodes(
        &mut self,
        css: &mut Css,
        node_data: &NodeDataContainerRef<'_, NodeData>,
        node_hierarchy: &NodeHierarchyItemVec,
        non_leaf_nodes: &ParentWithNodeDepthVec,
        html_tree: &NodeDataContainerRef<'_, CascadeInfo>,
        node_states: &[StyledNodeState],
        dirty: &[bool],
        moved_in_place: &[bool],
    ) -> bool {
        use azul_css::{
            css::{
                CssDeclaration,
                CssPathPseudoSelector::{
                    self, Active, Backdrop, DragOver, Dragging, Focus, FocusVisible, FocusWithin,
                    Hover, Placeholder, SeatFocus,
                },
                CssPathSelector, CssRuleBlock,
            },
            dynamic_selector::{DynamicSelector, PseudoStateType},
        };

        let node_count = node_data.len();
        let hierarchy = node_hierarchy.as_container();
        let items = hierarchy.internal;
        let named = dirty.iter().filter(|d| **d).count();
        let any_moved = moved_in_place.iter().any(|moved| *moved);
        if dirty.len() != node_count
            || (!moved_in_place.is_empty() && moved_in_place.len() != node_count)
            || items.len() != node_count
            || self.node_count != node_count
            || self.css_props.len() != node_count
            || self.cascaded_props.len() != node_count
            || !self.css_props.is_flattened()
            || !self.cascaded_props.is_flattened()
            || self
                .compact_cache
                .as_ref()
                .is_none_or(|compact| compact.node_count() != node_count)
            || !self.ua_applied
            || self.composed_since_recompute
            || !self.resolved_inline.is_empty()
            || self.variables_depend_on_context
            || self.custom_property_envs
                != crate::custom_property_cascade::CustomPropertyEnvs::default()
            || crate::custom_property_cascade::needs_variable_pass(css, node_data.internal)
            || named * 2 > node_count
            || node_data
                .internal
                .iter()
                .zip(dirty)
                .any(|(node, named)| *named && node.is_generated_content())
        {
            return false;
        }

        let css_is_empty = css.is_empty();
        let dyn_ctx = self.dynamic_context.clone();
        let no_context_theme = dyn_ctx.is_none().then(crate::app_theme::current_theme);
        let rule_applies = |conds: &azul_css::dynamic_selector::DynamicSelectorVec| -> bool {
            let cs = conds.as_slice();
            cs.is_empty()
                || cs.iter().all(|sel| {
                    condition_holds(dyn_ctx.as_deref(), no_context_theme.as_ref(), sel)
                })
        };
        let rank = |conds: &[DynamicSelector]| {
            rank_of(dyn_ctx.as_deref(), no_context_theme.as_ref(), conds)
        };
        let ancestor_states = crate::style::AncestorStates {
            nodes: node_states,
            window_inactive: dyn_ctx.as_deref().is_some_and(|c| !c.window_focused),
            every_state: false,
        };

        // ---- The matched rules of the named nodes (`restyle_cascade`, node by node) ----

        let mut new_css: Vec<Vec<StatefulCssProperty>> = vec![Vec::new(); node_count];
        let mut new_overruled: alloc::collections::BTreeMap<
            usize,
            Vec<(PseudoStateType, CssPropertyType)>,
        > = alloc::collections::BTreeMap::new();
        let mut new_env = EnvReaders::default();

        // Presentational hints first, as the first of a node's stylesheet properties.
        for (index, nd) in node_data.internal.iter().enumerate() {
            if !dirty[index] {
                continue;
            }
            for rule in nd.style.rules.as_ref() {
                let stateful = rule
                    .conditions
                    .as_slice()
                    .iter()
                    .any(|c| matches!(c, DynamicSelector::PseudoState(_)));
                if rule.priority >= azul_css::css::rule_priority::INLINE
                    || stateful
                    || !rule_applies(&rule.conditions)
                {
                    continue;
                }
                for declaration in rule.declarations.as_ref() {
                    new_env.note(Some(index), declaration);
                    if let Some(prop) = declaration.resolve_in_cascade(dyn_ctx.as_deref()) {
                        new_css[index].push(StatefulCssProperty {
                            state: PseudoStateType::Normal,
                            prop_type: prop.get_type(),
                            property: prop,
                            ua_origin: false,
                            priority: azul_css::css::rule_priority::PRESENTATIONAL,
                        });
                    }
                }
            }
        }

        // Whether the inheritance walk carries a `:backdrop` tier, as the whole cascade asks.
        let mut any_backdrop = node_data.internal.iter().any(|nd| {
            nd.style.iter_inline_properties().any(|(_, conds)| {
                conds.as_slice().iter().any(|c| {
                    matches!(c, DynamicSelector::PseudoState(PseudoStateType::Backdrop))
                })
            })
        });

        let mut global_priorities: Vec<(CssPropertyType, u8)> = Vec::new();
        // The states the sheet's live rules name left of their subject, as the whole cascade
        // finds them (`left_state_mask`): the sheet of a spliced cache is another sheet than
        // the one its mask came with (`Self::spliced`); for a kept DOM this is what it holds.
        let mut new_left_mask: u32 = 0;
        if !css_is_empty {
            css.sort_by_specificity();

            let is_global_only = |rule: &CssRuleBlock| {
                let selectors = rule.path.selectors.as_ref();
                selectors.len() == 1
                    && matches!(selectors.first(), Some(CssPathSelector::Global))
                    && rule.priority <= azul_css::css::rule_priority::INLINE
                    && !rule
                        .declarations
                        .as_ref()
                        .iter()
                        .any(CssDeclaration::uses_custom_properties)
            };
            let mut all: Vec<&CssRuleBlock> = css.rules().collect();
            Css::sort_rules_in_cascade_order(&mut all, rank);
            let lowest_matched = all
                .iter()
                .filter(|rule| !is_global_only(rule))
                .map(|rule| rule.priority)
                .min();
            let mut global_only_rules: Vec<&CssRuleBlock> = Vec::new();
            let mut specific_rules: Vec<&CssRuleBlock> = Vec::new();
            for rule in all {
                if is_global_only(rule) && lowest_matched.is_none_or(|p| rule.priority <= p) {
                    global_only_rules.push(rule);
                } else {
                    specific_rules.push(rule);
                }
            }

            // The `*` bucket every node shares: it has to be the one the kept rows were made
            // with, or no row stands.
            let mut global_props: Vec<CssProperty> = Vec::new();
            for rule in &global_only_rules {
                if !rule_applies(&rule.conditions) {
                    continue;
                }
                if crate::style::rule_ends_with(&rule.path, None) {
                    for d in &rule.declarations {
                        if let Some(s) = d.resolve_in_cascade(dyn_ctx.as_deref()) {
                            global_priorities.push((s.get_type(), rule.priority));
                            global_props.push(s);
                        }
                    }
                }
            }
            if global_props != self.global_css_props {
                return false;
            }

            if !specific_rules.is_empty() {
                let passes: [(Option<CssPathPseudoSelector>, PseudoStateType); 11] = [
                    (None, PseudoStateType::Normal),
                    (Some(Hover), PseudoStateType::Hover),
                    (Some(Active), PseudoStateType::Active),
                    (Some(Focus), PseudoStateType::Focus),
                    (Some(SeatFocus), PseudoStateType::SeatFocus),
                    (Some(FocusVisible), PseudoStateType::FocusVisible),
                    (Some(FocusWithin), PseudoStateType::FocusWithin),
                    (Some(Dragging), PseudoStateType::Dragging),
                    (Some(DragOver), PseudoStateType::DragOver),
                    (Some(Placeholder), PseudoStateType::Placeholder),
                    (Some(Backdrop), PseudoStateType::Backdrop),
                ];
                let backdrop_pass: u16 = 1 << (passes.len() - 1);
                let ends_in: Vec<u16> = specific_rules
                    .iter()
                    .map(|rule| {
                        passes
                            .iter()
                            .enumerate()
                            .filter(|(_, (pseudo, _))| {
                                crate::style::rule_ends_with(&rule.path, pseudo.clone())
                            })
                            .fold(0u16, |bits, (pass, _)| bits | (1 << pass))
                    })
                    .collect();
                let takes_part: Vec<u16> = specific_rules
                    .iter()
                    .zip(&ends_in)
                    .map(|(rule, &bits)| {
                        if rule_applies(&rule.conditions) {
                            bits
                        } else {
                            0
                        }
                    })
                    .collect();
                any_backdrop |= ends_in.iter().any(|bits| bits & backdrop_pass != 0);
                let live_passes = takes_part.iter().fold(0u16, |all, bits| all | bits);
                new_left_mask = specific_rules
                    .iter()
                    .zip(&takes_part)
                    .filter(|(_, bits)| **bits != 0)
                    .fold(0, |mask, (rule, _)| {
                        mask | crate::style::left_state_bits(&rule.path)
                    });

                let mut index = crate::style::SubjectRuleIndex::new(
                    specific_rules.iter().map(|rule| &rule.path),
                    &hierarchy,
                );
                let mut candidates: Vec<usize> = Vec::new();
                let mut sibling_memo = crate::style::GeneralSiblingMemo::default();
                let node_total = if live_passes == 0 { 0 } else { node_count };
                // Ascending, as the index asks; a node that is not named is not asked for.
                for node_index in 0..node_total {
                    if !dirty[node_index] {
                        continue;
                    }
                    let node_id = NodeId::new(node_index);
                    index.candidates_into(node_index, &node_data[node_id], &mut candidates);
                    for (pass, (pseudo, state)) in passes.iter().enumerate() {
                        let bit = 1u16 << pass;
                        if live_passes & bit == 0 {
                            continue;
                        }
                        let state = *state;
                        for &rule_idx in &candidates {
                            if takes_part[rule_idx] & bit == 0 {
                                continue;
                            }
                            let rule_block: &CssRuleBlock = specific_rules[rule_idx];
                            if !crate::style::matches_html_element_metered(
                                &rule_block.path,
                                node_id,
                                &hierarchy,
                                node_data,
                                html_tree,
                                pseudo.clone(),
                                Some(ancestor_states),
                                Some((&mut sibling_memo, rule_idx)),
                                &self.match_work,
                            ) {
                                continue;
                            }
                            if rule_block.priority > azul_css::css::rule_priority::INLINE {
                                let overruled = new_overruled.entry(node_index).or_default();
                                for ty in rule_block
                                    .declarations
                                    .as_slice()
                                    .iter()
                                    .filter_map(CssDeclaration::get_type)
                                {
                                    if !overruled.contains(&(state, ty)) {
                                        overruled.push((state, ty));
                                    }
                                }
                            }
                            // No custom property and no `var()` (checked above): every
                            // declaration is a property or an `env()`.
                            for decl in rule_block.declarations.as_slice() {
                                if let Some(prop) = decl.resolve_in_cascade(dyn_ctx.as_deref()) {
                                    new_env.note(Some(node_index), decl);
                                    new_css[node_index].push(StatefulCssProperty {
                                        state,
                                        prop_type: prop.get_type(),
                                        property: prop,
                                        ua_origin: false,
                                        priority: rule_block.priority,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        } else if !self.global_css_props.is_empty() {
            return false;
        }

        // ---- Nothing below can fail: the rows are written ----
        self.left_state_mask = new_left_mask;
        if named == 0 && !any_moved {
            return true;
        }
        crate::styled_dom::count_nodes_cascaded(named);

        // A value a rule outranks never shows (`drop_outranked_values`); then each row as
        // `sort_each_and_flatten` leaves it: by (state, type), the last of a key kept.
        let mut global: Vec<(CssPropertyType, u8)> = Vec::new();
        for (ty, priority) in &global_priorities {
            match global.iter_mut().find(|(t, _)| t == ty) {
                Some(slot) => slot.1 = slot.1.max(*priority),
                None => global.push((*ty, *priority)),
            }
        }
        for (index, entries) in new_css.iter_mut().enumerate() {
            if !dirty[index] {
                continue;
            }
            let star = node_data.internal.get(index).is_some_and(star_selects);
            drop_outranked_values(entries, if star { &global } else { &[] });
            sort_row_keeping_the_last(entries);
        }
        self.inline_overruled
            .retain(|node, _| !dirty.get(*node).copied().unwrap_or(false));
        self.inline_overruled.extend(new_overruled);
        self.env_readers
            .nodes
            .retain(|node, _| !dirty.get(*node).copied().unwrap_or(false));
        self.env_readers.append(&mut new_env, 0);
        self.css_props.replace_rows(
            new_css
                .into_iter()
                .enumerate()
                .filter(|(index, _)| dirty[*index]),
        );

        // ---- What the named nodes inherited (`restyle_cascade`'s walk, then the UA pass) ----
        //
        // `rebuild`: the named nodes and their ancestors. A row is built for each; only the
        // named nodes' rows are kept.
        let mut rebuild = dirty.to_vec();
        for index in 0..node_count {
            if !dirty[index] {
                continue;
            }
            let mut up = items[index].parent_id();
            while let Some(parent) = up {
                if parent.index() >= node_count || rebuild[parent.index()] {
                    break;
                }
                rebuild[parent.index()] = true;
                up = items[parent.index()].parent_id();
            }
        }
        let rebuilt: Vec<usize> = (0..node_count).filter(|index| rebuild[*index]).collect();
        let mut rows: Vec<Vec<StatefulCssProperty>> = vec![Vec::new(); node_count];

        if rebuild[0] && !self.inherited_from_host.is_empty() {
            let from_host: Vec<StatefulCssProperty> = self
                .inherited_from_host
                .iter()
                .filter(|(prop_type, value)| {
                    !is_resolved_parent_inherited(*prop_type)
                        && !inherits_its_computed_length(&value.property)
                })
                .map(|(prop_type, value)| StatefulCssProperty {
                    state: PseudoStateType::Normal,
                    prop_type: *prop_type,
                    property: clone_inheritable_property(&value.property),
                    ua_origin: false,
                    priority: 0,
                })
                .collect();
            rows[0].extend(from_host);
        }

        for ParentWithNodeDepth { depth: _, node_id } in non_leaf_nodes {
            let Some(parent_id) = node_id.into_crate_internal() else {
                continue;
            };
            if !rebuild.get(parent_id.index()).copied().unwrap_or(false) {
                continue;
            }

            let all_states = [
                PseudoStateType::Normal,
                PseudoStateType::Hover,
                PseudoStateType::Active,
                PseudoStateType::Focus,
                PseudoStateType::FocusVisible,
                PseudoStateType::FocusWithin,
                PseudoStateType::SeatFocus,
                PseudoStateType::Dragging,
                PseudoStateType::DragOver,
                PseudoStateType::Placeholder,
                PseudoStateType::Backdrop,
            ];
            let states = if any_backdrop {
                &all_states[..]
            } else {
                &all_states[..all_states.len() - 1]
            };

            let mut inline_by_state: Vec<Vec<(CssPropertyType, CssProperty)>> = {
                let mut parent_inline = Vec::new();
                azul_css::css::inline_in_cascade_order(
                    self.inline_properties(&node_data[parent_id], parent_id.index()),
                    rank,
                    &mut parent_inline,
                );
                states
                    .iter()
                    .map(|&state| {
                        let mut picked: Vec<(CssPropertyType, CssProperty)> = Vec::new();
                        for &(prop, conds) in &parent_inline {
                            let conditions = conds.as_slice();
                            let decl_state = conditions
                                .iter()
                                .find_map(|c| match c {
                                    DynamicSelector::PseudoState(s) => Some(*s),
                                    _ => None,
                                })
                                .unwrap_or(PseudoStateType::Normal);
                            let applies = decl_state == state
                                && conditions.iter().all(|c| match c {
                                    DynamicSelector::PseudoState(s) => *s == state,
                                    other => condition_holds(
                                        dyn_ctx.as_deref(),
                                        no_context_theme.as_ref(),
                                        other,
                                    ),
                                });
                            let prop_type = prop.get_type();
                            if !applies
                                || !prop_type.is_inheritable()
                                || is_resolved_parent_inherited(prop_type)
                            {
                                continue;
                            }
                            let value = clone_inheritable_property(prop);
                            match picked.iter_mut().find(|(t, _)| *t == prop_type) {
                                Some(slot) => slot.1 = value,
                                None => picked.push((prop_type, value)),
                            }
                        }
                        picked
                    })
                    .collect()
            };

            for (state_index, &state) in states.iter().enumerate() {
                let parent_inheritable_inline = core::mem::take(&mut inline_by_state[state_index]);

                // The parent's matched rules: its row in the flat store lists one entry per
                // (state, type), the one that won - what the whole cascade picks out of the
                // row in its build phase (the last of a type).
                let parent_inheritable_css: Vec<(CssPropertyType, CssProperty)> = if css_is_empty {
                    Vec::new()
                } else {
                    let mut picked: Vec<(CssPropertyType, CssProperty)> = Vec::new();
                    for p in self.css_props.get_slice(parent_id.index()).iter().filter(|p| {
                        p.state == state
                            && p.prop_type.is_inheritable()
                            && !is_resolved_parent_inherited(p.prop_type)
                    }) {
                        let value = clone_inheritable_property(&p.property);
                        match picked.iter_mut().find(|(t, _)| *t == p.prop_type) {
                            Some(slot) => slot.1 = value,
                            None => picked.push((p.prop_type, value)),
                        }
                    }
                    picked
                };

                let parent_inheritable_cascaded: Vec<(CssPropertyType, CssProperty)> = rows
                    [parent_id.index()]
                .iter()
                .filter(|p| {
                    p.state == state
                        && p.prop_type.is_inheritable()
                        && !is_resolved_parent_inherited(p.prop_type)
                })
                .map(|p| (p.prop_type, clone_inheritable_property(&p.property)))
                .collect();

                if parent_inheritable_inline.is_empty()
                    && parent_inheritable_css.is_empty()
                    && parent_inheritable_cascaded.is_empty()
                {
                    continue;
                }

                let mut offered: Vec<&(CssPropertyType, CssProperty)> = Vec::new();
                for entry in parent_inheritable_inline
                    .iter()
                    .chain(parent_inheritable_css.iter())
                    .chain(parent_inheritable_cascaded.iter())
                {
                    if !offered.iter().any(|(t, _)| *t == entry.0) {
                        offered.push(entry);
                    }
                }
                offered.retain(|(_, v)| !inherits_its_computed_length(v));

                for child_id in parent_id.az_children(&hierarchy) {
                    if !rebuild.get(child_id.index()).copied().unwrap_or(false) {
                        continue;
                    }
                    let child_vec = &mut rows[child_id.index()];
                    for (prop_type, prop_value) in offered.iter().copied() {
                        if !child_vec
                            .iter()
                            .any(|p| p.state == state && p.prop_type == *prop_type)
                        {
                            child_vec.push(StatefulCssProperty {
                                state,
                                prop_type: *prop_type,
                                property: prop_value.clone(),
                                ua_origin: false,
                                priority: 0,
                            });
                        }
                    }
                }
            }
        }

        // The UA defaults (`apply_ua_css_inner`), for the same nodes.
        let ctx = self.dynamic_context.as_deref();
        let mut global_bits: PropTypeBits = [0u128; 2];
        for p in &self.global_css_props {
            prop_type_bit_set(&mut global_bits, p.get_type());
        }
        let mut declared: Vec<PropTypeBits> = vec![[0u128; 2]; node_count];
        for &index in &rebuilt {
            let node = &node_data.internal[index];
            let bits = &mut declared[index];
            for p in self
                .css_props
                .get_slice(index)
                .iter()
                .filter(|p| p.state == PseudoStateType::Normal)
            {
                prop_type_bit_set(bits, p.prop_type);
            }
            for (prop, conds) in self.inline_properties(node, index) {
                if conds.as_slice().is_empty() {
                    prop_type_bit_set(bits, prop.get_type());
                }
            }
            if star_selects(node) {
                bits[0] |= global_bits[0];
                bits[1] |= global_bits[1];
            }
        }
        for &index in &rebuilt {
            let node = &node_data.internal[index];
            // What beats the UA sheet: what the node declares, and on a text node what it
            // inherited (`normal_props_present`).
            let mut present = declared[index];
            if node.is_text_node() {
                for p in rows[index]
                    .iter()
                    .filter(|p| p.state == PseudoStateType::Normal)
                {
                    prop_type_bit_set(&mut present, p.prop_type);
                }
            }
            for prop_type in crate::ua_css::UA_PROPERTY_TYPES {
                if prop_type_bit_test(&present, *prop_type) {
                    continue;
                }
                let is_root = takes_root_ua_default(index, *prop_type, &self.inherited_from_host);
                let Some(ua_prop) = crate::ua_css::get_ua_default(node, is_root, *prop_type, ctx)
                else {
                    continue;
                };
                rows[index].push(StatefulCssProperty {
                    state: PseudoStateType::Normal,
                    prop_type: *prop_type,
                    property: ua_prop.clone(),
                    ua_origin: true,
                    priority: 0,
                });
            }
        }

        // The copies a UA default made stale (`hand_ua_defaults_down`), along the same nodes:
        // a node's parent is among them.
        let types: Vec<CssPropertyType> = crate::ua_css::UA_PROPERTY_TYPES
            .iter()
            .copied()
            .filter(|t| t.is_inheritable() && !is_resolved_parent_inherited(*t))
            .collect();
        let mut stale: Vec<Option<CssProperty>> = vec![None; node_count];
        for &ty in &types {
            for &index in &rebuilt {
                stale[index] = None;
            }
            for &node_index in &rebuilt {
                if prop_type_bit_test(&declared[node_index], ty) {
                    continue;
                }
                let is_normal_of = |e: &StatefulCssProperty| {
                    e.state == PseudoStateType::Normal && e.prop_type == ty
                };
                let copy = rows[node_index]
                    .iter()
                    .rev()
                    .find(|e| is_normal_of(e) && !e.ua_origin)
                    .map(|e| clone_inheritable_property(&e.property));
                let ua = rows[node_index]
                    .iter()
                    .rev()
                    .find(|e| is_normal_of(e) && e.ua_origin)
                    .map(|e| clone_inheritable_property(&e.property));
                let inherited_stale = items[node_index]
                    .parent_id()
                    .map(|p| p.index())
                    .filter(|p| *p < node_index)
                    .and_then(|p| stale[p].clone());
                let Some(copy) = copy else {
                    continue;
                };
                if inherited_stale.as_ref() == Some(&copy) {
                    rows[node_index].retain(|e| !is_normal_of(e) || e.ua_origin);
                    stale[node_index] = Some(copy);
                } else if ua.is_some_and(|u| u != copy) {
                    stale[node_index] = Some(copy);
                }
            }
        }

        // The named nodes' rows go into the store as `sort_cascaded_props` leaves a row, with
        // every resting value still in them: the two passes below read them so.
        let mut kept_rows: Vec<(usize, Vec<StatefulCssProperty>)> = Vec::with_capacity(named);
        for (index, mut row) in rows.into_iter().enumerate() {
            if dirty[index] {
                sort_row_keeping_the_last(&mut row);
                kept_rows.push((index, row));
            }
        }
        self.cascaded_props.replace_rows(kept_rows.iter().cloned());

        // ---- The computed inherited values of the named nodes (`compute_inherited_values`) ----
        self.computed_values.grow_to(node_count);
        self.computed_values.remove_nodes(dirty);
        self.css_wide_values
            .retain(|(node, _, _), _| !dirty.get(*node).copied().unwrap_or(false));
        let mut state_overlays: Vec<(usize, Vec<CssProperty>)> = Vec::new();
        // The inheritable types a node shows its children from a state, as the whole pass
        // keeps them (`compute_inherited_values`): written for a named node where the walk
        // finds any, and for the KEPT parent of a named node read once from what the cache
        // holds (`types_shown_from_a_state`; its entry may be empty).
        let mut from_a_state: alloc::collections::BTreeMap<usize, Vec<CssPropertyType>> =
            alloc::collections::BTreeMap::new();
        let mut ancestors: Vec<(usize, Vec<(CssPropertyType, CssPropertyWithOrigin)>, Vec<u32>)> =
            Vec::new();
        for node_index in 0..node_count {
            if !dirty[node_index] {
                continue;
            }
            let node_id = NodeId::new(node_index);
            let parent_id = items[node_index].parent_id();
            if let Some(pid) = parent_id {
                if !dirty.get(pid.index()).copied().unwrap_or(false)
                    && !from_a_state.contains_key(&pid.index())
                {
                    let shown = self.types_shown_from_a_state(
                        node_data.internal,
                        items,
                        node_states,
                        pid.index(),
                    );
                    from_a_state.insert(pid.index(), shown);
                }
            }
            let held_above: Vec<CssPropertyType> =
                parent_id.and_then(|pid| from_a_state.get(&pid.index())).map_or_else(Vec::new, Clone::clone);
            // The parent's stored values are on top of `ancestors` when the parent is a named
            // node (the walk stored them); a parent that keeps its values reads the store.
            let mut parent_on_top = false;
            if let Some(pid) = parent_id {
                while ancestors
                    .last()
                    .is_some_and(|(node, _, _)| *node != pid.index())
                {
                    ancestors.pop();
                }
                parent_on_top = !ancestors.is_empty();
            }
            // What the parent hands down on top of its resting values in the state it is
            // in: found by the walk for a named parent, asked of a kept one.
            let overlay: Option<Vec<CssProperty>> = parent_id.and_then(|pid| {
                if dirty.get(pid.index()).copied().unwrap_or(false) {
                    state_overlays
                        .iter()
                        .find(|(i, _)| *i == pid.index())
                        .map(|(_, held)| held.clone())
                } else {
                    let state = node_states.get(pid.index())?;
                    let held = self.state_inherited_values(
                        node_data.internal.get(pid.index())?,
                        pid.index(),
                        state,
                    );
                    (!held.is_empty()).then_some(held)
                }
            });
            let parent_owned: Option<Vec<(CssPropertyType, CssPropertyWithOrigin)>> =
                match parent_id {
                    Some(_) if parent_on_top && overlay.is_none() => None,
                    Some(pid) => {
                        let mut values = if parent_on_top {
                            ancestors
                                .last()
                                .map(|(_, values, _)| values.clone())
                                .unwrap_or_default()
                        } else {
                            self.computed_values.values_for(pid.index())
                        };
                        if let Some(held) = overlay.as_ref() {
                            for property in held {
                                let ty = property.get_type();
                                let entry = (
                                    ty,
                                    CssPropertyWithOrigin {
                                        property: property.clone(),
                                        origin: CssPropertyOrigin::Own,
                                    },
                                );
                                match values.binary_search_by_key(&ty, |(k, _)| *k) {
                                    Ok(at) => values[at] = entry,
                                    Err(at) => values.insert(at, entry),
                                }
                            }
                        }
                        Some(values)
                    }
                    None if !self.inherited_from_host.is_empty() => {
                        Some(self.inherited_from_host.clone())
                    }
                    None => None,
                };
            let parent_computed: Option<&Vec<(CssPropertyType, CssPropertyWithOrigin)>> =
                match parent_owned.as_ref() {
                    Some(owned) => Some(owned),
                    None if parent_on_top => ancestors.last().map(|(_, values, _)| values),
                    None => None,
                };

            let mut ctx_node = InheritanceContext {
                node_id,
                parent_id,
                computed_values: Vec::new(),
            };
            let keyword_values =
                self.css_wide_values_of_node(node_data.internal, node_index, parent_id);
            self.css_wide_values.extend(keyword_values);
            if let Some(parent_values) = parent_computed {
                Self::inherit_from_parent(&mut ctx_node, parent_values);
            }
            self.apply_cascade_properties(
                &mut ctx_node,
                node_id,
                parent_computed,
                node_data.internal,
                node_index,
                &held_above,
            );
            let mut hands_on: Vec<CssPropertyType> = Vec::new();
            if let Some(state) = node_states.get(node_index) {
                let held =
                    self.state_inherited_values(&node_data.internal[node_index], node_index, state);
                if !held.is_empty() {
                    hands_on.extend(held.iter().map(CssProperty::get_type));
                    state_overlays.push((node_index, held));
                }
            }
            for ty in &held_above {
                if !hands_on.contains(ty) && holds_inherited(&ctx_node.computed_values, *ty) {
                    hands_on.push(*ty);
                }
            }
            if !hands_on.is_empty() && items[node_index].last_child_id().is_some() {
                from_a_state.insert(node_index, hands_on);
            }
            Self::compute_font_relative_line_height(&mut ctx_node);

            let inheritable: Vec<(CssPropertyType, CssPropertyWithOrigin)> = ctx_node
                .computed_values
                .iter()
                .filter(|(pt, _)| pt.is_inheritable())
                .cloned()
                .collect();
            let near = if parent_on_top {
                ancestors
                    .last()
                    .map(|(_, values, buckets)| (values.as_slice(), buckets.as_slice()))
            } else {
                None
            };
            let buckets = self.computed_values.place_node(node_index, &inheritable, near);
            if items[node_index].last_child_id().is_some() {
                ancestors.push((node_index, inheritable, buckets));
            }
        }

        // ---- The compact entries of the named nodes ----
        let prev_font_hashes: Vec<u64> = self
            .compact_cache
            .as_ref()
            .map(|c| c.prev_font_hashes.clone())
            .unwrap_or_default();
        // ... and of the nodes whose own style moved in place: their slots hold its values.
        let with_the_moved: Vec<bool>;
        let compact_rows: &[bool] = if any_moved {
            with_the_moved = dirty
                .iter()
                .zip(moved_in_place)
                .map(|(dirty, moved)| *dirty || *moved)
                .collect();
            &with_the_moved
        } else {
            dirty
        };
        let compact = self.rebuild_compact_rows(
            node_data.internal,
            items,
            node_states,
            &prev_font_hashes,
            compact_rows,
        );
        self.compact_cache = Some(compact);

        // ---- The prune of the named nodes' rows (`prune_compact_normal_props`) ----
        self.cascaded_props
            .replace_rows(kept_rows.into_iter().map(|(index, mut row)| {
                row.retain(survives_the_prune);
                (index, row)
            }));

        self.invalidate_resolved_font_sizes();
        self.ua_applied = true;
        self.cascade_epoch = self.cascade_epoch.wrapping_add(1);
        true
    }

    /// This cascade re-indexed for ANOTHER ARENA: node `n` of it holds every per-node row
    /// node `source[n]` holds here - its matched rules, what it inherited, its computed
    /// inherited values, its compact entries, the properties a rule above the inline
    /// priority overrules, the values its CSS-wide keywords stand for, its entry among the
    /// readers of the environment - and a node with `source[n] == None` holds none. What a
    /// rebuild that changed the TREE starts from (`StyledDom::cascade_reusing`): the nodes
    /// that are what they were, under ancestors that are what they were, keep their rows
    /// under their new numbers, and [`Self::recascade_nodes`] makes the rows of the others.
    ///
    /// The tables as they lie: the two flat stores share their runs ([`FlatVecVec::gathered`]:
    /// the new nodes point at the runs the old ones pointed at), the inherited values are
    /// transposed ([`InheritedValues::remapped`]: each value's node list is renumbered), the
    /// compact cache is four vectors of rows and two side tables keyed by node, the rest are
    /// maps keyed by node.
    ///
    /// What is NOT per node is carried as it is and may say more than a new cascade would,
    /// never less: the flags the compact builder gathers DOM-wide (declared rare properties,
    /// viewport thresholds, the font-family table: a reader takes a set flag as "ask the
    /// cascade"), the environment flags, the `*` bucket (`recascade_nodes` compares it with
    /// the new sheet's and refuses a difference) and the left-state mask (`recascade_nodes`
    /// sets it for the new sheet). The retained sheet is left empty for the caller's; runtime
    /// overrides are not carried (they follow a node's identity:
    /// `StyledDom::migrate_user_overrides_from`); the lazily resolved font sizes start empty.
    ///
    /// `None` where this cascade is not one a node's rows can be taken from: it is not
    /// complete (no compact cache, rows in their build phase, a composed DOM), it resolves
    /// custom properties, or two new nodes name one old node.
    #[must_use]
    pub fn spliced(&self, source: &[Option<NodeId>]) -> Option<Self> {
        let old_count = self.node_count;
        let old_compact = self.compact_cache.as_ref()?;
        if self.css_props.len() != old_count
            || self.cascaded_props.len() != old_count
            || old_compact.node_count() != old_count
            || !self.ua_applied
            || self.composed_since_recompute
            || !self.resolved_inline.is_empty()
            || self.variables_depend_on_context
            || self.custom_property_envs
                != crate::custom_property_cascade::CustomPropertyEnvs::default()
        {
            return None;
        }
        let new_count = source.len();
        let mut new_of_old: Vec<Option<usize>> = vec![None; old_count];
        for (new_index, from) in source.iter().enumerate() {
            let Some(old) = *from else {
                continue;
            };
            let slot = new_of_old.get_mut(old.index())?;
            if slot.is_some() {
                return None;
            }
            *slot = Some(new_index);
        }
        let renumbered = |old: usize| new_of_old.get(old).copied().flatten();

        let mut compact = azul_css::compact_cache::CompactLayoutCache::with_capacity(new_count);
        compact.uses_viewport_units = old_compact.uses_viewport_units;
        compact.font_hash_to_families = old_compact.font_hash_to_families.clone();
        compact.has_dynamic_conditions = old_compact.has_dynamic_conditions;
        compact.dom_declared_flags = old_compact.dom_declared_flags;
        compact.inline_viewport_w.clone_from(&old_compact.inline_viewport_w);
        compact.inline_viewport_h.clone_from(&old_compact.inline_viewport_h);
        compact.inline_viewport_shape = old_compact.inline_viewport_shape;
        for (new_index, from) in source.iter().enumerate() {
            let Some(old) = from.map(|old| old.index()) else {
                continue;
            };
            let (Some(tier1), Some(dims), Some(cold), Some(text)) = (
                old_compact.tier1_enums.get(old),
                old_compact.tier2_dims.get(old),
                old_compact.tier2_cold.get(old),
                old_compact.tier2b_text.get(old),
            ) else {
                continue;
            };
            compact.tier1_enums[new_index] = *tier1;
            compact.tier2_dims[new_index] = *dims;
            compact.tier2_cold[new_index] = *cold;
            compact.tier2b_text[new_index] = *text;
            if let Some(hash) = old_compact.prev_font_hashes.get(old) {
                compact.prev_font_hashes[new_index] = *hash;
            }
        }
        for ((old, ty), value) in &old_compact.px_overflow {
            if let Some(new) = renumbered(*old as usize).and_then(|new| u32::try_from(new).ok()) {
                compact.px_overflow.insert((new, *ty), *value);
            }
        }

        let mut env_readers = EnvReaders {
            nodes: alloc::collections::BTreeMap::new(),
            beyond_nodes: self.env_readers.beyond_nodes,
            follows_viewport: self.env_readers.follows_viewport,
        };
        for (old, scope) in &self.env_readers.nodes {
            if let Some(new) = renumbered(*old) {
                env_readers.nodes.insert(new, *scope);
            }
        }

        Some(Self {
            node_count: new_count,
            dynamic_context: self.dynamic_context.clone(),
            ua_applied: true,
            cascade_epoch: self.cascade_epoch.wrapping_add(1),
            cascaded_props: self.cascaded_props.gathered(source)?,
            css_props: self.css_props.gathered(source)?,
            computed_values: self.computed_values.remapped(&new_of_old, new_count),
            compact_cache: Some(compact),
            global_css_props: self.global_css_props.clone(),
            inherited_from_host: self.inherited_from_host.clone(),
            left_state_mask: self.left_state_mask,
            inline_overruled: self
                .inline_overruled
                .iter()
                .filter_map(|(old, overruled)| Some((renumbered(*old)?, overruled.clone())))
                .collect(),
            css_wide_values: self
                .css_wide_values
                .iter()
                .filter_map(|((old, ty, keyword), value)| {
                    Some(((renumbered(*old)?, *ty, *keyword), value.clone()))
                })
                .collect(),
            env_readers,
            ..Self::empty(0)
        })
    }
}

/// One node's row as `FlatVecVec::sort_each_and_flatten` leaves it: sorted by (state,
/// property type), entries of one key in the order they were pushed, the LAST of a key kept.
fn sort_row_keeping_the_last(row: &mut Vec<StatefulCssProperty>) {
    row.sort_by_key(|p| (p.state, p.prop_type));
    let keys: Vec<(azul_css::dynamic_selector::PseudoStateType, CssPropertyType)> =
        row.iter().map(|p| (p.state, p.prop_type)).collect();
    let mut index = 0;
    row.retain(|_| {
        let at = index;
        index += 1;
        keys.get(at + 1) != Some(&keys[at])
    });
}

#[cfg(test)]
#[path = "prop_cache_test.rs"]
mod prop_cache_test;

/// The cascade rank of a declaration or rule with `conditions` for a
/// `StyledDom`: under the window's theme chain, or - without a context - under
/// the one-entry chain of the app theme the DOM is built for, the same rule
/// [`condition_holds`] follows.
#[inline]
pub(crate) fn rank_of(
    ctx: Option<&DynamicSelectorContext>,
    no_context_theme: Option<&AzString>,
    conditions: &[azul_css::dynamic_selector::DynamicSelector],
) -> usize {
    ctx.map_or_else(
        || {
            no_context_theme.map_or(azul_css::dynamic_selector::UNTHEMED_RANK, |t| {
                azul_css::dynamic_selector::cascade_rank(&[t.as_str()], conditions)
            })
        },
        |ctx| ctx.cascade_rank(conditions),
    )
}

/// Whether a non-pseudo condition holds for a `StyledDom`: against the
/// window's context, or - for a `StyledDom` no window has adopted yet -
/// only the app theme the DOM is built for
/// ([`DynamicSelector::matches_without_context`]). The ONE rule the property
/// cache and the compact cache share, so the two paths cannot disagree.
#[inline]
pub(crate) fn condition_holds(
    ctx: Option<&DynamicSelectorContext>,
    no_context_theme: Option<&AzString>,
    condition: &azul_css::dynamic_selector::DynamicSelector,
) -> bool {
    ctx.map_or_else(
        || no_context_theme.is_some_and(|t| condition.matches_without_context(t.as_str())),
        |ctx| condition.matches(ctx),
    )
}

/// Whether the host a DOM is hosted at hands the DOM's root a value of
/// `prop_type` ([`CssPropertyCache::inherited_from_host`], sorted by type).
#[inline]
pub(crate) fn host_supplies(
    inherited_from_host: &[(CssPropertyType, CssPropertyWithOrigin)],
    prop_type: CssPropertyType,
) -> bool {
    !inherited_from_host.is_empty()
        && inherited_from_host
            .binary_search_by_key(&prop_type, |(t, _)| *t)
            .is_ok()
}

/// Does node `node_index` take the DOCUMENT ROOT's UA default for
/// `prop_type` (`ua_css::get_ua_root_property_themed`, the `is_root` of
/// `ua_css::get_ua_default`)? The root - node 0 - does, unless its DOM is
/// hosted and the host supplies that type: the document-wide default stands
/// in for what a document's root inherits from nothing, and a hosted root
/// inherits its host's value instead. The ONE rule the UA pass, the compact
/// builder and the slow path's fallback share.
#[inline]
/// The length of a property that is one length (a font size, a padding, a margin, a min / max
/// size, a spacing): what a font-relative length is computed from.
fn pixel_value_of(prop: &CssProperty) -> Option<azul_css::props::basic::pixel::PixelValue> {
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
}

/// The initial value of `ty` where it is not what the readers take an `initial` keyword for
/// (what a node declaring nothing gets): an element without a user agent rule is a block here,
/// while the initial `display` is `inline`; a font size the readers do not know is the
/// parent's, while the initial one is `medium`.
fn initial_value_unlike_the_default(ty: CssPropertyType) -> Option<CssProperty> {
    match ty {
        CssPropertyType::Display => Some(CssProperty::Display(
            azul_css::css::CssPropertyValue::Exact(LayoutDisplay::Inline),
        )),
        CssPropertyType::FontSize => Some(CssProperty::FontSize(
            azul_css::css::CssPropertyValue::Exact(StyleFontSize {
                inner: azul_css::props::basic::pixel::PixelValue::px(
                    azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                ),
            }),
        )),
        _ => None,
    }
}

pub(crate) fn takes_root_ua_default(
    node_index: usize,
    prop_type: CssPropertyType,
    inherited_from_host: &[(CssPropertyType, CssPropertyWithOrigin)],
) -> bool {
    node_index == 0 && !host_supplies(inherited_from_host, prop_type)
}

impl CssPropertyCache {
    /// What a DOM HOSTED at `node_id` - the content of a `VirtualView` - inherits
    /// from it: the node's resting value of every inheritable property it has
    /// one for, as a child inherits it (the computed value: CSS Cascade 4 s7),
    /// ready for [`Self::inherited_from_host`].
    ///
    /// Read off `computed_values` - what the node's own children inherit:
    /// its declarations (author rules, inline ones, runtime overrides), its
    /// UA defaults and what it inherited itself, a `line-height` in `em` / `%`
    /// already a length and a relative `font-weight` a number. The `*` rules
    /// it matches are the one tier that store leaves out (the slow path and
    /// the compact builder apply them on their own), so those types are
    /// asked of the slow path. `font_size_px` is the node's computed font
    /// size, resolved by the caller with the layout's own resolution. A type
    /// the host has no value for is left out: the hosted root falls back to
    /// the initial value then, as the host did.
    ///
    /// The host's RESTING style (its `:hover` or `:focus` do not reach into
    /// the view: a restyle of the whole hosted DOM per pointer move is not
    /// what a view is for). Cheap - a handful of bucket probes - because it
    /// runs for every view on every layout pass (a kept view compares it
    /// with what its DOM inherits).
    #[must_use]
    pub fn inherited_values_for_hosted_dom(
        &self,
        node_data: &NodeData,
        node_id: &NodeId,
        font_size_px: f32,
    ) -> InheritedFromHost {
        use azul_css::{css::CssPropertyValue, props::basic::pixel::PixelValue};

        fn upsert(values: &mut InheritedFromHost, prop_type: CssPropertyType, property: CssProperty) {
            let entry = CssPropertyWithOrigin {
                property,
                origin: CssPropertyOrigin::Inherited,
            };
            match values.binary_search_by_key(&prop_type, |(t, _)| *t) {
                Ok(idx) => values[idx].1 = entry,
                Err(idx) => values.insert(idx, (prop_type, entry)),
            }
        }

        // Sorted by type, inheritable only (`store_if_changed`).
        let mut values: InheritedFromHost = self
            .computed_values
            .values_for(node_id.index())
            .into_iter()
            .map(|(prop_type, value)| {
                (
                    prop_type,
                    CssPropertyWithOrigin {
                        property: value.property,
                        origin: CssPropertyOrigin::Inherited,
                    },
                )
            })
            .collect();

        // `*` matches elements only (as in the compact builder).
        if star_selects(node_data) {
            let resting = StyledNodeState::default();
            let mut global_types: Vec<CssPropertyType> = self
                .global_css_props
                .iter()
                .map(CssProperty::get_type)
                .filter(|t| t.is_inheritable() && *t != CssPropertyType::FontSize)
                .collect();
            global_types.sort();
            global_types.dedup();
            for prop_type in global_types {
                let Some(value) = self.get_property(node_data, node_id, &resting, &prop_type)
                else {
                    continue;
                };
                let property = match value {
                    CssProperty::LineHeight(CssPropertyValue::Exact(lh))
                        if lh.is_font_relative_length() =>
                    {
                        CssProperty::LineHeight(CssPropertyValue::Exact(lh.computed(font_size_px)))
                    }
                    // `bolder` / `lighter` from a `*` rule computes against
                    // the host's parent, which only the cascade knows: the
                    // weight `computed_values` holds stays.
                    CssProperty::FontWeight(CssPropertyValue::Exact(w)) if w.is_relative() => {
                        continue;
                    }
                    other => clone_inheritable_property(other),
                };
                upsert(&mut values, prop_type, property);
            }
        }

        upsert(
            &mut values,
            CssPropertyType::FontSize,
            CssProperty::FontSize(CssPropertyValue::Exact(StyleFontSize {
                inner: PixelValue::px(font_size_px),
            })),
        );
        values
    }
}

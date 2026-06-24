// +spec:box-model:b3a79e - box assigned same styles as generating element; getters read from styled DOM per node
//! Centralized CSS property getters for the layout solver pipeline

use azul_core::{
    dom::{NodeId, NodeType},
    geom::LogicalSize,
    id::NodeId as CoreNodeId,
    styled_dom::{StyledDom, StyledNodeState},
};
use azul_css::{
    css::CssPropertyValue,
    props::{
        basic::{
            font::{StyleFontFamily, StyleFontFamilyVec, StyleFontStyle, StyleFontWeight},
            pixel::{DEFAULT_FONT_SIZE, PT_TO_PX},
            ColorU, PhysicalSize, PixelValue, PropertyContext, ResolutionContext,
        },
        layout::{
            grid::GridTemplateAreas, BoxDecorationBreak, BreakInside, LayoutAlignContent,
            LayoutAlignItems, LayoutBoxSizing, LayoutClear, LayoutDisplay, LayoutFlexDirection,
            LayoutFlexWrap, LayoutFloat, LayoutHeight, LayoutJustifyContent, LayoutOverflow,
            LayoutPosition, LayoutWidth, LayoutWritingMode, Orphans, PageBreak,
            StyleOverflowClipMargin, StyleScrollbarGutter, Widows,
        },
        property::{
            CssProperty, CssPropertyType, LayoutAlignContentValue, LayoutAlignItemsValue,
            LayoutAlignSelfValue, LayoutFlexBasisValue, LayoutFlexDirectionValue,
            LayoutFlexGrowValue, LayoutFlexShrinkValue, LayoutFlexWrapValue, LayoutGapValue,
            LayoutGridAutoColumnsValue, LayoutGridAutoFlowValue, LayoutGridAutoRowsValue,
            LayoutGridColumnValue, LayoutGridRowValue, LayoutGridTemplateColumnsValue,
            LayoutGridTemplateRowsValue, LayoutJustifyContentValue, LayoutJustifyItemsValue,
            LayoutJustifySelfValue,
        },
        style::{
            border_radius::StyleBorderRadius,
            lists::{StyleListStylePosition, StyleListStyleType},
            StyleAlignmentBaseline, StyleDirection, StyleDominantBaseline, StyleInitialLetterAlign,
            StyleInitialLetterWrap, StyleTextAlign, StyleTextBoxEdge, StyleTextBoxTrim,
            StyleUnicodeBidi, StyleUserSelect, StyleVerticalAlign, StyleVisibility,
            StyleWhiteSpace,
        },
    },
};

use crate::{
    font_traits::{ParsedFontTrait, StyleProperties},
    solver3::{
        display_list::{BorderRadius, PhysicalSizeImport},
        layout_tree::LayoutNode,
        scrollbar::ScrollbarRequirements,
    },
};

const DEFAULT_EM_SIZE: f32 = 16.0;
const DEFAULT_CARET_WIDTH_PX: f32 = 2.0;
const DEFAULT_CARET_BLINK_MS: u32 = 500;
const DEFAULT_TAB_SIZE: f32 = 8.0;
const SCROLLBAR_WIDTH_THIN: f32 = 8.0;
const SCROLLBAR_WIDTH_AUTO: f32 = 12.0;
const SCROLLBAR_HOVER_EXPAND_PX: f32 = 4.0;
const THUMB_HOVER_LIGHTEN: u8 = 30;
const THUMB_HOVER_ALPHA_ADD: u8 = 40;
const THUMB_ACTIVE_DARKEN: u8 = 15;

// Font-size resolution helper functions

/// Helper function to get element's computed font-size.
///
/// **Memoised** for the common `Normal` pseudo-state: the first
/// call on a given `StyledDom` populates
/// `css_property_cache.ptr.resolved_font_sizes_px` via a single
/// bottom-up DOM walk (N cascade walks total, stored as
/// `Vec<f32>`); every subsequent call is a single Vec index.
/// Non-normal state falls through to [`resolve_font_size_slow`].
///
/// Motivation: `AZ_PROP_COUNT=1` measured 329 629 `font-size`
/// cascade walks per cold layout on excel.html (~730 per node).
/// With this cache that collapses to ~500 total (one per node,
/// once), and subsequent layouts hit the Vec directly.
///
/// The semantics of the slow path are preserved exactly: the
/// `compute_all_font_sizes_px` walker mirrors the original's
/// `computed_values` → cascade → `DEFAULT_FONT_SIZE` ordering,
/// so rendered pixels are byte-identical.
#[must_use] pub fn get_element_font_size(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    // M12.7 FIX: the OnceLock-cached fast path
    // (`is_normal → resolved_font_sizes_px.get_or_init(|| compute_all_font_sizes_px) →
    // sizes.get`) MIS-LIFTS to wasm — it diverges (create_node_from_dom never returns →
    // empty LayoutTree → 0 rects). PROVEN by isolation: skipping it lets
    // get_element_font_size reach + return via resolve_font_size_slow, and
    // create_resolution_context completes (sub-step 1→4). resolve_font_size_slow is the
    // same resolution unmemoized (correct), so we always use it. (Native desktop is
    // unaffected in correctness; it loses the per-DOM memoization — a minor perf cost
    // only on the lifted web path's small DOMs. The cache-block lift bug — likely the
    // compute_all_font_sizes_px closure's control/FP — is documented for a later remill
    // fix that can restore the fast path.)
    let _ = compute_all_font_sizes_px; // referenced so other callers / native keep it
    resolve_font_size_slow(styled_dom, dom_id, node_state)
}

/// Bottom-up single-pass resolve of every node's font-size.
/// Parents are computed before children (DFS pre-order invariant
/// on `NodeId::index()`), so `em` inherits via the parent's
/// already-stored pixel value. `rem` reads from `sizes[0]` once
/// the root is populated (the root's own size resolves via the
/// `computed_values` short-circuit if set, otherwise DEFAULT).
///
/// Preserves the original resolution order exactly:
///
/// 1. `computed_values` binary search → if `FontSize` is pre-
///    resolved to a px value, use that.
/// 2. Full cascade via `cache.get_font_size(...)`; if an explicit
///    value is present, resolve with context.
/// 3. `DEFAULT_FONT_SIZE` fallback — NOT `parent_font_size`,
///    because the `computed_values` short-circuit at step 1 is
///    the cascade's inheritance channel (pre-populated for every
///    inheriting node).
fn compute_all_font_sizes_px(styled_dom: &StyledDom) -> Vec<f32> {
    use azul_css::props::{
        basic::length::SizeMetric,
        property::{CssProperty, CssPropertyType},
    };

    let n = styled_dom.node_data.len();
    let mut sizes = alloc::vec![DEFAULT_FONT_SIZE; n];
    if n == 0 {
        return sizes;
    }

    let data_container = styled_dom.node_data.as_container();
    let state_container = styled_dom.styled_nodes.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();
    let cache = &styled_dom.css_property_cache.ptr;

    for idx in 0..n {
        let dom_id = NodeId::new(idx);

        // Step 1: computed_values short-circuit (matches original).
        if let Some(vec) = cache.computed_values.get(idx) {
            if let Ok(cv_idx) = vec.binary_search_by_key(&CssPropertyType::FontSize, |(k, _)| *k) {
                if let CssProperty::FontSize(css_val) = &vec[cv_idx].1.property {
                    if let Some(fs) = css_val.get_property() {
                        if fs.inner.metric == SizeMetric::Px {
                            sizes[idx] = fs.inner.number.get();
                            continue;
                        }
                    }
                }
            }
        }

        // Step 2: full cascade walk.
        let parent_font_size = hierarchy
            .get(dom_id)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
            .map_or(DEFAULT_FONT_SIZE, |p| sizes[p.index()]);
        let root_font_size = sizes[0];

        let Some(node_data) = data_container.internal.get(idx) else {
            sizes[idx] = DEFAULT_FONT_SIZE;
            continue;
        };
        let Some(styled) = state_container.internal.get(idx) else {
            sizes[idx] = DEFAULT_FONT_SIZE;
            continue;
        };
        let node_state = &styled.styled_node_state;

        // Step 2.5: compact cache fast path — avoids a full cascade walk
        // per node. The build-time pass has already resolved em/% to px,
        // so the raw u32 here is the final pixel value when set.
        let mut fast_fs: Option<f32> = None;
        let mut compact_said_inherit = false;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_font_size_raw(idx);
                if raw == azul_css::compact_cache::U32_SENTINEL
                    || raw == azul_css::compact_cache::U32_INHERIT
                    || raw == azul_css::compact_cache::U32_INITIAL
                {
                    compact_said_inherit = true;
                } else if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                    // Already-resolved pixel value (em/% eliminated during build).
                    if pv.metric == SizeMetric::Px {
                        fast_fs = Some(pv.number.get());
                    } else {
                        // Shouldn't normally happen post-resolve, but fall through safely.
                        let context = ResolutionContext {
                            element_font_size: DEFAULT_FONT_SIZE,
                            parent_font_size,
                            root_font_size,
                            containing_block_size: PhysicalSize::new(0.0, 0.0),
                            element_size: None,
                            viewport_size: PhysicalSize::new(0.0, 0.0),
                        };
                        fast_fs =
                            Some(pv.resolve_with_context(&context, PropertyContext::FontSize));
                    }
                }
            }
        }
        if let Some(fs) = fast_fs {
            sizes[idx] = fs;
            continue;
        }
        if compact_said_inherit {
            sizes[idx] = parent_font_size;
            continue;
        }

        let resolved = cache
            .get_font_size(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property().copied())
            .map(|v| {
                let context = ResolutionContext {
                    element_font_size: DEFAULT_FONT_SIZE,
                    parent_font_size,
                    root_font_size,
                    containing_block_size: PhysicalSize::new(0.0, 0.0),
                    element_size: None,
                    viewport_size: PhysicalSize::new(0.0, 0.0),
                };
                v.inner
                    .resolve_with_context(&context, PropertyContext::FontSize)
            });

        // Step 3: fallback to DEFAULT (matches original .unwrap_or).
        sizes[idx] = resolved.unwrap_or(DEFAULT_FONT_SIZE);
    }
    sizes
}

/// Un-memoised recursive resolution, used as the fallback for
/// non-normal pseudo-states in [`get_element_font_size`] and
/// directly by tests that bypass the StyledDom-scoped cache.
/// Keeps the original semantics verbatim.
fn resolve_font_size_slow(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let cache = &styled_dom.css_property_cache.ptr;

    if let Some(vec) = cache.computed_values.get(dom_id.index()) {
        if let Ok(idx) = vec.binary_search_by_key(
            &CssPropertyType::FontSize,
            |(k, _)| *k,
        ) {
            if let CssProperty::FontSize(css_val) = &vec[idx].1.property
            {
                if let Some(fs) = css_val.get_property() {
                    if fs.inner.metric == azul_css::props::basic::length::SizeMetric::Px {
                        return fs.inner.number.get();
                    }
                }
            }
        }
    }

    let parent_font_size = styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        .map_or(DEFAULT_FONT_SIZE, |parent_id| resolve_font_size_slow(styled_dom, parent_id, node_state));

    let root_font_size = if dom_id == NodeId::new(0) {
        DEFAULT_FONT_SIZE
    } else {
        resolve_font_size_slow(styled_dom, NodeId::new(0), node_state)
    };

    cache
        .get_font_size(node_data, &dom_id, node_state)
        .and_then(|v| v.get_property().copied())
        .map_or(DEFAULT_FONT_SIZE, |v| {
            let context = ResolutionContext {
                element_font_size: DEFAULT_FONT_SIZE,
                parent_font_size,
                root_font_size,
                containing_block_size: PhysicalSize::new(0.0, 0.0),
                element_size: None,
                viewport_size: PhysicalSize::new(0.0, 0.0),
            };
            v.inner
                .resolve_with_context(&context, PropertyContext::FontSize)
        })
}

/// Helper function to get parent's computed font-size.
///
/// Retrieves the parent's own `StyledNodeState` so that pseudo-class-specific
/// font-size rules (e.g. `div:hover { font-size: 32px }`) are resolved
/// against the parent's actual state, not the child's.
#[must_use] pub fn get_parent_font_size(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    _node_state: &StyledNodeState, // child's state — intentionally unused
) -> f32 {
    styled_dom
        .node_hierarchy
        .as_container()
        .get(dom_id)
        .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        .map_or(DEFAULT_FONT_SIZE, |parent_id| {
            let parent_state = &styled_dom.styled_nodes.as_container()[parent_id].styled_node_state;
            get_element_font_size(styled_dom, parent_id, parent_state)
        })
}

/// Helper function to get root element's font-size.
///
/// Uses the root element's own `StyledNodeState` so that pseudo-class-specific
/// rules are resolved correctly regardless of which node triggered the call.
#[must_use] pub fn get_root_font_size(styled_dom: &StyledDom, _node_state: &StyledNodeState) -> f32 {
    let root_id = NodeId::new(0);
    let root_state = &styled_dom.styled_nodes.as_container()[root_id].styled_node_state;
    get_element_font_size(styled_dom, root_id, root_state)
}

/// A value that can be Auto, Initial, Inherit, or an explicit value.
/// This preserves CSS cascade semantics better than Option<T>.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[derive(Default)]
pub enum MultiValue<T> {
    /// CSS 'auto' keyword
    #[default]
    Auto,
    /// CSS 'initial' keyword - use initial value
    Initial,
    /// CSS 'inherit' keyword - inherit from parent
    Inherit,
    /// Explicit value (e.g., "10px", "50%")
    Exact(T),
}

impl<T> MultiValue<T> {
    /// Returns true if this is an Auto value
    pub const fn is_auto(&self) -> bool {
        matches!(self, Self::Auto)
    }

    /// Returns true if this is an explicit value
    pub const fn is_exact(&self) -> bool {
        matches!(self, Self::Exact(_))
    }

    /// Gets the exact value if present
    pub fn exact(self) -> Option<T> {
        match self {
            Self::Exact(v) => Some(v),
            _ => None,
        }
    }

    /// Gets the exact value or returns the provided default
    pub fn unwrap_or(self, default: T) -> T {
        match self {
            Self::Exact(v) => v,
            _ => default,
        }
    }

    /// Gets the exact value or returns `T::default()`
    pub fn unwrap_or_default(self) -> T
    where
        T: Default,
    {
        match self {
            Self::Exact(v) => v,
            _ => T::default(),
        }
    }

    /// Maps the inner value if Exact, otherwise returns self unchanged
    pub fn map<U, F>(self, f: F) -> MultiValue<U>
    where
        F: FnOnce(T) -> U,
    {
        match self {
            Self::Exact(v) => MultiValue::Exact(f(v)),
            Self::Auto => MultiValue::Auto,
            Self::Initial => MultiValue::Initial,
            Self::Inherit => MultiValue::Inherit,
        }
    }
}

// Implement helper methods for LayoutOverflow specifically
impl MultiValue<LayoutOverflow> {
    /// Returns true if this overflow value causes content to be clipped.
    /// This includes Hidden, Clip, Auto, and Scroll (all values except Visible).
    #[must_use] pub const fn is_clipped(&self) -> bool {
        matches!(
            self,
            Self::Exact(
                LayoutOverflow::Hidden
                    | LayoutOverflow::Clip
                    | LayoutOverflow::Auto
                    | LayoutOverflow::Scroll
            )
        )
    }

    #[must_use] pub const fn is_scroll(&self) -> bool {
        matches!(
            self,
            Self::Exact(LayoutOverflow::Scroll | LayoutOverflow::Auto)
        )
    }

    #[must_use] pub const fn is_auto_overflow(&self) -> bool {
        matches!(self, Self::Exact(LayoutOverflow::Auto))
    }

    #[must_use] pub const fn is_hidden(&self) -> bool {
        matches!(self, Self::Exact(LayoutOverflow::Hidden))
    }

    #[must_use] pub const fn is_hidden_or_clip(&self) -> bool {
        matches!(
            self,
            Self::Exact(LayoutOverflow::Hidden | LayoutOverflow::Clip)
        )
    }

    #[must_use] pub const fn is_scroll_explicit(&self) -> bool {
        matches!(self, Self::Exact(LayoutOverflow::Scroll))
    }

    #[must_use] pub const fn is_clip(&self) -> bool {
        matches!(self, Self::Exact(LayoutOverflow::Clip))
    }

    #[must_use] pub const fn is_visible_or_clip(&self) -> bool {
        matches!(
            self,
            Self::Exact(LayoutOverflow::Visible | LayoutOverflow::Clip)
        )
    }

    // +spec:overflow:833078 - visible/clip compute to auto/hidden if other axis is scrollable
    /// Resolves the computed value per CSS Overflow 3 § 3.1:
    /// visible/clip values compute to auto/hidden (respectively)
    /// if the other axis is neither visible nor clip.
    #[must_use] pub const fn resolve_computed(
        &self,
        other_axis: &Self,
    ) -> Self {
        match (self, other_axis) {
            (Self::Exact(val), Self::Exact(other)) => {
                Self::Exact(val.resolve_computed(*other))
            }
            _ => *self,
        }
    }
}

// Implement helper methods for LayoutPosition
impl MultiValue<LayoutPosition> {
    #[must_use] pub const fn is_absolute_or_fixed(&self) -> bool {
        matches!(
            self,
            Self::Exact(LayoutPosition::Absolute | LayoutPosition::Fixed)
        )
    }
}

// Implement helper methods for LayoutFloat
impl MultiValue<LayoutFloat> {
    #[must_use] pub const fn is_none(&self) -> bool {
        matches!(
            self,
            Self::Auto
                | Self::Initial
                | Self::Inherit
                | Self::Exact(LayoutFloat::None)
        )
    }
}


/// Helper macro to reduce boilerplate for simple CSS property getters
/// Returns the inner `PixelValue` wrapped in `MultiValue`
macro_rules! get_css_property_pixel {
    // Variant WITH compact cache fast path for i16-encoded resolved px properties
    ($fn_name:ident, $cache_method:ident, $ua_property:expr, compact_i16 = $compact_method:ident) => {
        #[must_use] pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<PixelValue> {
            // FAST PATH: compact cache for normal state (O(1) array lookup)
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    let raw = cc.$compact_method(node_id.index());
                    if raw == azul_css::compact_cache::I16_AUTO {
                        return MultiValue::Auto;
                    }
                    if raw == azul_css::compact_cache::I16_INITIAL {
                        return MultiValue::Initial;
                    }
                    if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                        // Valid value: decode i16 ×10 → px
                        return MultiValue::Exact(PixelValue::px(f32::from(raw) / 10.0));
                    }
                    // I16_SENTINEL or I16_INHERIT → fall through to slow path
                }
            }

            let node_data = &styled_dom.node_data.as_container()[node_id];

            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(ref val) = author_css {
                if val.is_auto() {
                    return MultiValue::Auto;
                }
                if let Some(exact) = val.get_property().copied() {
                    return MultiValue::Exact(exact.inner);
                }
            }

            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(inner) = ua_prop.get_pixel_inner() {
                    return MultiValue::Exact(inner);
                }
            }

            MultiValue::Initial
        }
    };
    // Variant WITHOUT compact cache (original behavior)
    ($fn_name:ident, $cache_method:ident, $ua_property:expr) => {
        pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<PixelValue> {
            let node_data = &styled_dom.node_data.as_container()[node_id];

            // 1. Check author CSS first (includes inline styles - highest priority)
            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            // NOTE: Check for Auto FIRST — CssPropertyValue::Auto is a valid value
            // that should NOT fall through to UA CSS. Previously, get_property()
            // returned None for Auto, causing inline "margin: auto" to be ignored.
            if let Some(ref val) = author_css {
                if val.is_auto() {
                    return MultiValue::Auto;
                }
                if let Some(exact) = val.get_property().copied() {
                    return MultiValue::Exact(exact.inner);
                }
                // For Initial, Inherit, None, Revert, Unset - fall through to UA CSS
            }

            // 2. Check User Agent CSS (only if author CSS didn't set a value)
            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(inner) = ua_prop.get_pixel_inner() {
                    return MultiValue::Exact(inner);
                }
            }

            // 3. Fallback to Initial (not set)
            // IMPORTANT: Use Initial, not Auto! In CSS, the initial value for
            // margin is 0, not auto. Using Auto here caused margins to be treated
            // as "margin: auto" which blocks align-self: stretch in flexbox.
            MultiValue::Initial
        }
    };
}

/// Helper trait to extract `PixelValue` from any `CssProperty` variant
trait CssPropertyPixelInner {
    fn get_pixel_inner(&self) -> Option<PixelValue>;
}

impl CssPropertyPixelInner for CssProperty {
    fn get_pixel_inner(&self) -> Option<PixelValue> {
        match self {
            Self::Left(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::Right(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::Top(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::Bottom(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::MarginLeft(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::MarginRight(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::MarginTop(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::MarginBottom(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::PaddingLeft(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::PaddingRight(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::PaddingTop(CssPropertyValue::Exact(v)) => Some(v.inner),
            Self::PaddingBottom(CssPropertyValue::Exact(v)) => Some(v.inner),
            _ => None,
        }
    }
}

/// Generic macro for CSS properties with UA CSS fallback - returns `MultiValue`<T>
macro_rules! get_css_property {
    // Variant WITH compact cache fast path (for enum properties in Tier 1)
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr, compact = $compact_method:ident) => {
        #[must_use] pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            // FAST PATH: compact cache for normal state (O(1) array + bitshift)
            // NOTE (M12.7): skipping this fast path does NOT fix get_display_type's
            // divergence — the slow path / the `match get_display_type(...)` on the
            // LayoutDisplay enum (a niche-discriminant) mis-lifts too. So this isn't the
            // cache (unlike the font-size fix); it's the deeper niche/enum decode. Kept.
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    return MultiValue::Exact(cc.$compact_method(node_id.index()));
                }
            }

            // SLOW PATH: full cascade resolution
            let node_data = &styled_dom.node_data.as_container()[node_id];

            // 1. Check author CSS first
            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            // 2. Check User Agent CSS
            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            // 3. Fallback to Auto (not set)
            MultiValue::Auto
        }
    };
    // Variant WITH compact cache for u32-encoded dimension enums (LayoutWidth/LayoutHeight)
    // These types have Auto, Px(PixelValue), MinContent, MaxContent, Calc variants
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr, compact_u32_dim = $compact_raw_method:ident, $px_variant:path, $auto_variant:path, $min_content_variant:path, $max_content_variant:path) => {
        #[must_use] pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            // FAST PATH: compact cache for normal state
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    let raw = cc.$compact_raw_method(node_id.index());
                    match raw {
                        azul_css::compact_cache::U32_AUTO => return MultiValue::Auto,
                        azul_css::compact_cache::U32_INITIAL => return MultiValue::Initial,
                        azul_css::compact_cache::U32_NONE => return MultiValue::Auto,
                        azul_css::compact_cache::U32_MIN_CONTENT => return MultiValue::Exact($min_content_variant),
                        azul_css::compact_cache::U32_MAX_CONTENT => return MultiValue::Exact($max_content_variant),
                        azul_css::compact_cache::U32_SENTINEL | azul_css::compact_cache::U32_INHERIT => {
                            // fall through to slow path
                        }
                        _ => {
                            // Valid encoded pixel value
                            if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                                return MultiValue::Exact($px_variant(pv));
                            }
                            // decode failed → slow path
                        }
                    }
                }
            }

            // SLOW PATH: full cascade resolution
            let node_data = &styled_dom.node_data.as_container()[node_id];

            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            MultiValue::Auto
        }
    };
    // Variant WITH compact cache for u32-encoded dimension structs (LayoutMinWidth etc.)
    // These types are struct { inner: PixelValue }
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr, compact_u32_struct = $compact_raw_method:ident) => {
        #[must_use] pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            // FAST PATH: compact cache for normal state
            if node_state.is_normal() {
                if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
                    let raw = cc.$compact_raw_method(node_id.index());
                    match raw {
                        azul_css::compact_cache::U32_AUTO | azul_css::compact_cache::U32_NONE => return MultiValue::Auto,
                        azul_css::compact_cache::U32_INITIAL => return MultiValue::Initial,
                        azul_css::compact_cache::U32_SENTINEL | azul_css::compact_cache::U32_INHERIT => {
                            // fall through to slow path
                        }
                        _ => {
                            if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                                return MultiValue::Exact(
                                    <$return_type as azul_css::props::PixelValueTaker>::from_pixel_value(pv)
                                );
                            }
                        }
                    }
                }
            }

            // SLOW PATH
            let node_data = &styled_dom.node_data.as_container()[node_id];

            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            MultiValue::Auto
        }
    };
    // Variant WITHOUT compact cache (original behavior)
    ($fn_name:ident, $cache_method:ident, $return_type:ty, $ua_property:expr) => {
        #[must_use] pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> MultiValue<$return_type> {
            let node_data = &styled_dom.node_data.as_container()[node_id];

            // 1. Check author CSS first
            let author_css = styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state);

            if let Some(val) = author_css.and_then(|v| v.get_property().cloned()) {
                return MultiValue::Exact(val);
            }

            // 2. Check User Agent CSS
            let ua_css = azul_core::ua_css::get_ua_property(&node_data.node_type, $ua_property);

            if let Some(ua_prop) = ua_css {
                if let Some(val) = extract_property_value::<$return_type>(ua_prop) {
                    return MultiValue::Exact(val);
                }
            }

            // 3. Fallback to Auto (not set)
            MultiValue::Auto
        }
    };
}

/// Helper trait to extract typed values from UA CSS properties
trait ExtractPropertyValue<T> {
    fn extract(&self) -> Option<T>;
}

fn extract_property_value<T>(prop: &CssProperty) -> Option<T>
where
    CssProperty: ExtractPropertyValue<T>,
{
    prop.extract()
}

// Implement extraction for all layout types

impl ExtractPropertyValue<LayoutWidth> for CssProperty {
    fn extract(&self) -> Option<LayoutWidth> {
        match self {
            Self::Width(CssPropertyValue::Exact(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutHeight> for CssProperty {
    fn extract(&self) -> Option<LayoutHeight> {
        match self {
            Self::Height(CssPropertyValue::Exact(v)) => Some(v.clone()),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMinWidth> for CssProperty {
    fn extract(&self) -> Option<LayoutMinWidth> {
        match self {
            Self::MinWidth(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMinHeight> for CssProperty {
    fn extract(&self) -> Option<LayoutMinHeight> {
        match self {
            Self::MinHeight(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMaxWidth> for CssProperty {
    fn extract(&self) -> Option<LayoutMaxWidth> {
        match self {
            Self::MaxWidth(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutMaxHeight> for CssProperty {
    fn extract(&self) -> Option<LayoutMaxHeight> {
        match self {
            Self::MaxHeight(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutDisplay> for CssProperty {
    fn extract(&self) -> Option<LayoutDisplay> {
        match self {
            Self::Display(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutWritingMode> for CssProperty {
    fn extract(&self) -> Option<LayoutWritingMode> {
        match self {
            Self::WritingMode(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutFlexWrap> for CssProperty {
    fn extract(&self) -> Option<LayoutFlexWrap> {
        match self {
            Self::FlexWrap(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutJustifyContent> for CssProperty {
    fn extract(&self) -> Option<LayoutJustifyContent> {
        match self {
            Self::JustifyContent(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleTextAlign> for CssProperty {
    fn extract(&self) -> Option<StyleTextAlign> {
        match self {
            Self::TextAlign(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutFloat> for CssProperty {
    fn extract(&self) -> Option<LayoutFloat> {
        match self {
            Self::Float(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutClear> for CssProperty {
    fn extract(&self) -> Option<LayoutClear> {
        match self {
            Self::Clear(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutOverflow> for CssProperty {
    fn extract(&self) -> Option<LayoutOverflow> {
        match self {
            Self::OverflowX(CssPropertyValue::Exact(v))
            | Self::OverflowY(CssPropertyValue::Exact(v))
            | Self::OverflowBlock(CssPropertyValue::Exact(v))
            | Self::OverflowInline(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutPosition> for CssProperty {
    fn extract(&self) -> Option<LayoutPosition> {
        match self {
            Self::Position(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutBoxSizing> for CssProperty {
    fn extract(&self) -> Option<LayoutBoxSizing> {
        match self {
            Self::BoxSizing(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<PixelValue> for CssProperty {
    fn extract(&self) -> Option<PixelValue> {
        self.get_pixel_inner()
    }
}

impl ExtractPropertyValue<LayoutFlexDirection> for CssProperty {
    fn extract(&self) -> Option<LayoutFlexDirection> {
        match self {
            Self::FlexDirection(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutAlignItems> for CssProperty {
    fn extract(&self) -> Option<LayoutAlignItems> {
        match self {
            Self::AlignItems(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutAlignContent> for CssProperty {
    fn extract(&self) -> Option<LayoutAlignContent> {
        match self {
            Self::AlignContent(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleFontWeight> for CssProperty {
    fn extract(&self) -> Option<StyleFontWeight> {
        match self {
            Self::FontWeight(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleFontStyle> for CssProperty {
    fn extract(&self) -> Option<StyleFontStyle> {
        match self {
            Self::FontStyle(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleVisibility> for CssProperty {
    fn extract(&self) -> Option<StyleVisibility> {
        match self {
            Self::Visibility(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleWhiteSpace> for CssProperty {
    fn extract(&self) -> Option<StyleWhiteSpace> {
        match self {
            Self::WhiteSpace(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleDirection> for CssProperty {
    fn extract(&self) -> Option<StyleDirection> {
        match self {
            Self::Direction(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleUnicodeBidi> for CssProperty {
    fn extract(&self) -> Option<StyleUnicodeBidi> {
        match self {
            Self::UnicodeBidi(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleTextBoxTrim> for CssProperty {
    fn extract(&self) -> Option<StyleTextBoxTrim> {
        match self {
            Self::TextBoxTrim(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleTextBoxEdge> for CssProperty {
    fn extract(&self) -> Option<StyleTextBoxEdge> {
        match self {
            Self::TextBoxEdge(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleDominantBaseline> for CssProperty {
    fn extract(&self) -> Option<StyleDominantBaseline> {
        match self {
            Self::DominantBaseline(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleAlignmentBaseline> for CssProperty {
    fn extract(&self) -> Option<StyleAlignmentBaseline> {
        match self {
            Self::AlignmentBaseline(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleInitialLetterAlign> for CssProperty {
    fn extract(&self) -> Option<StyleInitialLetterAlign> {
        match self {
            Self::InitialLetterAlign(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleInitialLetterWrap> for CssProperty {
    fn extract(&self) -> Option<StyleInitialLetterWrap> {
        match self {
            Self::InitialLetterWrap(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleScrollbarGutter> for CssProperty {
    fn extract(&self) -> Option<StyleScrollbarGutter> {
        match self {
            Self::ScrollbarGutter(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleOverflowClipMargin> for CssProperty {
    fn extract(&self) -> Option<StyleOverflowClipMargin> {
        match self {
            Self::OverflowClipMargin(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleVerticalAlign> for CssProperty {
    fn extract(&self) -> Option<StyleVerticalAlign> {
        match self {
            Self::VerticalAlign(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

get_css_property!(
    get_writing_mode,
    get_writing_mode,
    LayoutWritingMode,
    CssPropertyType::WritingMode,
    compact = get_writing_mode
);

get_css_property!(
    get_css_width,
    get_width,
    LayoutWidth,
    CssPropertyType::Width,
    compact_u32_dim = get_width_raw,
    LayoutWidth::Px,
    LayoutWidth::Auto,
    LayoutWidth::MinContent,
    LayoutWidth::MaxContent
);

get_css_property!(
    get_css_height,
    get_height,
    LayoutHeight,
    CssPropertyType::Height,
    compact_u32_dim = get_height_raw,
    LayoutHeight::Px,
    LayoutHeight::Auto,
    LayoutHeight::MinContent,
    LayoutHeight::MaxContent
);

get_css_property!(
    get_wrap,
    get_flex_wrap,
    LayoutFlexWrap,
    CssPropertyType::FlexWrap,
    compact = get_flex_wrap
);

get_css_property!(
    get_justify_content,
    get_justify_content,
    LayoutJustifyContent,
    CssPropertyType::JustifyContent,
    compact = get_justify_content
);

get_css_property!(
    get_text_align,
    get_text_align,
    StyleTextAlign,
    CssPropertyType::TextAlign,
    compact = get_text_align
);

get_css_property!(
    get_float,
    get_float,
    LayoutFloat,
    CssPropertyType::Float,
    compact = get_float
);

get_css_property!(
    get_clear,
    get_clear,
    LayoutClear,
    CssPropertyType::Clear,
    compact = get_clear
);

get_css_property!(
    get_overflow_x,
    get_overflow_x,
    LayoutOverflow,
    CssPropertyType::OverflowX,
    compact = get_overflow_x
);

get_css_property!(
    get_overflow_y,
    get_overflow_y,
    LayoutOverflow,
    CssPropertyType::OverflowY,
    compact = get_overflow_y
);

// +spec:overflow:17654b - overflow-block and overflow-inline logical properties resolve to physical overflow based on writing mode
get_css_property!(
    get_overflow_block,
    get_overflow_block,
    LayoutOverflow,
    CssPropertyType::OverflowBlock
);

get_css_property!(
    get_overflow_inline,
    get_overflow_inline,
    LayoutOverflow,
    CssPropertyType::OverflowInline
);

get_css_property!(
    get_position,
    get_position,
    LayoutPosition,
    CssPropertyType::Position,
    compact = get_position
);

get_css_property!(
    get_css_box_sizing,
    get_box_sizing,
    LayoutBoxSizing,
    CssPropertyType::BoxSizing,
    compact = get_box_sizing
);

get_css_property!(
    get_flex_direction,
    get_flex_direction,
    LayoutFlexDirection,
    CssPropertyType::FlexDirection,
    compact = get_flex_direction
);

get_css_property!(
    get_align_items,
    get_align_items,
    LayoutAlignItems,
    CssPropertyType::AlignItems,
    compact = get_align_items
);

get_css_property!(
    get_align_content,
    get_align_content,
    LayoutAlignContent,
    CssPropertyType::AlignContent,
    compact = get_align_content
);

get_css_property!(
    get_font_weight_property,
    get_font_weight,
    StyleFontWeight,
    CssPropertyType::FontWeight,
    compact = get_font_weight
);

get_css_property!(
    get_font_style_property,
    get_font_style,
    StyleFontStyle,
    CssPropertyType::FontStyle,
    compact = get_font_style
);

get_css_property!(
    get_visibility,
    get_visibility,
    StyleVisibility,
    CssPropertyType::Visibility,
    compact = get_visibility
);

get_css_property!(
    get_white_space_property,
    get_white_space,
    StyleWhiteSpace,
    CssPropertyType::WhiteSpace,
    compact = get_white_space
);

// +spec:writing-modes:3af12f - unicode-bidi does not affect direction for layout; we use direction property directly
get_css_property!(
    get_direction_property,
    get_direction,
    StyleDirection,
    CssPropertyType::Direction,
    compact = get_direction
);

// +spec:display-property:346799 - inline-level elements with unicode-bidi:normal have no effect on text ordering
// +spec:writing-modes:3e2632 - unicode-bidi property resolves embedding level for bidi algorithm (LRE/RLE/PDF)
// +spec:writing-modes:d2c94f - direction+unicode-bidi properties map to UAX#9 bidirectional algorithm
get_css_property!(
    get_unicode_bidi_property,
    get_unicode_bidi,
    StyleUnicodeBidi,
    CssPropertyType::UnicodeBidi
);

// +spec:display-property:db5125 - text-box-trim on inline boxes trims content box to text-box-edge metric
// +spec:display-property:dceb24 - text-box-trim on inline boxes: content edges coincide with text baselines
get_css_property!(
    get_text_box_trim_property,
    get_text_box_trim,
    StyleTextBoxTrim,
    CssPropertyType::TextBoxTrim
);

get_css_property!(
    get_text_box_edge_property,
    get_text_box_edge,
    StyleTextBoxEdge,
    CssPropertyType::TextBoxEdge
);

get_css_property!(
    get_dominant_baseline_property,
    get_dominant_baseline,
    StyleDominantBaseline,
    CssPropertyType::DominantBaseline
);

get_css_property!(
    get_alignment_baseline_property,
    get_alignment_baseline,
    StyleAlignmentBaseline,
    CssPropertyType::AlignmentBaseline
);

get_css_property!(
    get_initial_letter_align_property,
    get_initial_letter_align,
    StyleInitialLetterAlign,
    CssPropertyType::InitialLetterAlign
);

get_css_property!(
    get_initial_letter_wrap_property,
    get_initial_letter_wrap,
    StyleInitialLetterWrap,
    CssPropertyType::InitialLetterWrap
);

// +spec:overflow:5d15e2 - block-start/block-end scrollbar gutter follows same rules as inline gutters when auto
//
// Hand-rolled fast path: 99% of nodes don't set scrollbar-gutter, and the
// default is `auto`. The compact cache stores the enum in 2 bits of
// tier2_cold.hot_flags, so we can return the answer without a cascade walk.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[must_use] pub fn get_scrollbar_gutter_property(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> MultiValue<StyleScrollbarGutter> {
    // FAST PATH: 2-bit enum in hot_flags
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let bits = cc.get_scrollbar_gutter_bits(node_id.index());
            let val = match bits {
                azul_css::compact_cache::SCROLLBAR_GUTTER_AUTO => StyleScrollbarGutter::Auto,
                azul_css::compact_cache::SCROLLBAR_GUTTER_STABLE => StyleScrollbarGutter::Stable,
                azul_css::compact_cache::SCROLLBAR_GUTTER_BOTH_EDGES => {
                    StyleScrollbarGutter::StableBothEdges
                }
                _ => StyleScrollbarGutter::Auto,
            };
            return MultiValue::Exact(val);
        }
    }

    // SLOW PATH: cascade resolution for pseudo-states or missing cache
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let author_css = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_gutter(node_data, &node_id, node_state);
    if let Some(val) = author_css.and_then(|v| v.get_property().copied()) {
        return MultiValue::Exact(val);
    }
    MultiValue::Auto
}

get_css_property!(
    get_overflow_clip_margin_property,
    get_overflow_clip_margin,
    StyleOverflowClipMargin,
    CssPropertyType::OverflowClipMargin
);

get_css_property!(
    get_object_fit_property,
    get_object_fit,
    StyleObjectFit,
    CssPropertyType::ObjectFit
);

// +spec:writing-modes:257296 - text-orientation getter for vertical typesetting (upright/sideways)
//
// Hand-rolled (not macro-generated) to attach a negative fast-path: most
// nodes have no text-orientation declared (default = Mixed), so we avoid a
// cascade walk per fc.rs call (which is called ~2× per node).
#[must_use] pub fn get_text_orientation_property(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> MultiValue<StyleTextOrientation> {
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_text_orientation(node_id.index()) {
                return MultiValue::Auto;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    if let Some(val) = styled_dom
        .css_property_cache
        .ptr
        .get_text_orientation(node_data, &node_id, node_state)
        .and_then(|v| v.get_property().copied())
    {
        return MultiValue::Exact(val);
    }
    let ua = azul_core::ua_css::get_ua_property(
        &node_data.node_type,
        CssPropertyType::TextOrientation,
    );
    if let Some(ua_prop) = ua {
        if let Some(val) = extract_property_value::<StyleTextOrientation>(ua_prop) {
            return MultiValue::Exact(val);
        }
    }
    MultiValue::Auto
}

get_css_property!(
    get_object_position_property,
    get_object_position,
    StyleObjectPosition,
    CssPropertyType::ObjectPosition
);

get_css_property!(
    get_aspect_ratio_property,
    get_aspect_ratio,
    StyleAspectRatio,
    CssPropertyType::AspectRatio
);

// NOTE: vertical-align does NOT use the compact cache because the compact cache
// only stores keyword variants (3 bits = 8 values) and silently drops
// Percentage/Length values by mapping them to Baseline. Always use the slow path.
#[must_use] pub fn get_vertical_align_property(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> MultiValue<StyleVerticalAlign> {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    let author_css = styled_dom
        .css_property_cache
        .ptr
        .get_vertical_align(node_data, &node_id, node_state);

    if let Some(val) = author_css.and_then(|v| v.get_property().copied()) {
        return MultiValue::Exact(val);
    }

    let ua_css = azul_core::ua_css::get_ua_property(
        &node_data.node_type,
        CssPropertyType::VerticalAlign,
    );

    if let Some(ua_prop) = ua_css {
        if let Some(val) = extract_property_value::<StyleVerticalAlign>(ua_prop) {
            return MultiValue::Exact(val);
        }
    }

    MultiValue::Auto
}
// Complex Property Getters

/// Get border radius for all four corners (raw CSS property values)
#[must_use] pub fn get_style_border_radius(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> StyleBorderRadius {
    use azul_css::props::basic::pixel::PixelValue;
    // FAST PATH: all four corners live in tier2_cold as i16 px × 10. The
    // common case (no rounded corners anywhere) reads four bytes and bails.
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let idx = node_id.index();
            let decode = |raw: i16| -> PixelValue {
                if raw >= azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                    PixelValue::px(0.0)
                } else {
                    PixelValue::px(f32::from(raw) / 10.0)
                }
            };
            return StyleBorderRadius {
                top_left: decode(cc.get_border_top_left_radius_raw(idx)),
                top_right: decode(cc.get_border_top_right_radius_raw(idx)),
                bottom_right: decode(cc.get_border_bottom_right_radius_raw(idx)),
                bottom_left: decode(cc.get_border_bottom_left_radius_raw(idx)),
            };
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];

    let top_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    let top_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    let bottom_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    let bottom_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property_or_default())
        .map(|v| v.inner)
        .unwrap_or_default();

    StyleBorderRadius {
        top_left,
        top_right,
        bottom_right,
        bottom_left,
    }
}

/// Get border radius for all four corners (resolved to pixels)
///
/// # Arguments
/// * `element_size` - The element's own size (width × height) for % resolution. According to CSS
///   spec, border-radius % uses element's own dimensions.
#[must_use] pub fn get_border_radius(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    element_size: PhysicalSizeImport,
    viewport_size: LogicalSize,
) -> BorderRadius {
    use azul_css::props::basic::{PhysicalSize, PropertyContext, ResolutionContext};

    // FAST PATH: all four corners as i16 px × 10 in tier2_cold. The
    // overwhelmingly common case (no rounded corners) reads four bytes and
    // returns zeros without a cascade walk.
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let idx = node_id.index();
            let tl = cc.get_border_top_left_radius_raw(idx);
            let tr = cc.get_border_top_right_radius_raw(idx);
            let br = cc.get_border_bottom_right_radius_raw(idx);
            let bl = cc.get_border_bottom_left_radius_raw(idx);
            // sentinel = "unset" = 0 px (no corner radius)
            let thresh = azul_css::compact_cache::I16_SENTINEL_THRESHOLD;
            let decode = |raw: i16| -> f32 {
                if raw >= thresh {
                    0.0
                } else {
                    f32::from(raw) / 10.0
                }
            };
            return BorderRadius {
                top_left: decode(tl),
                top_right: decode(tr),
                bottom_right: decode(br),
                bottom_left: decode(bl),
            };
        }
    }

    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get font sizes for em/rem resolution
    let element_font_size = get_element_font_size(styled_dom, node_id, node_state);
    let parent_font_size = styled_dom
        .node_hierarchy
        .as_container()
        .get(node_id)
        .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
        .map_or(DEFAULT_FONT_SIZE, |p| get_element_font_size(styled_dom, p, node_state));
    let root_font_size = get_root_font_size(styled_dom, node_state);

    // Create resolution context
    let context = ResolutionContext {
        element_font_size,
        parent_font_size,
        root_font_size,
        containing_block_size: PhysicalSize::new(0.0, 0.0), // Not used for border-radius
        element_size: Some(PhysicalSize::new(element_size.width, element_size.height)),
        viewport_size: PhysicalSize::new(viewport_size.width, viewport_size.height),
    };

    let top_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().copied())
        .unwrap_or_default();

    let top_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_top_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().copied())
        .unwrap_or_default();

    let bottom_right = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_right_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().copied())
        .unwrap_or_default();

    let bottom_left = styled_dom
        .css_property_cache
        .ptr
        .get_border_bottom_left_radius(node_data, &node_id, node_state)
        .and_then(|br| br.get_property().copied())
        .unwrap_or_default();

    BorderRadius {
        top_left: top_left
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
        top_right: top_right
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
        bottom_right: bottom_right
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
        bottom_left: bottom_left
            .inner
            .resolve_with_context(&context, PropertyContext::BorderRadius),
    }
}

// +spec:stacking-contexts:a93e62 - stack level from z-index for stacking context ordering
// +spec:stacking-contexts:ae50ae - z-index specifies stack level; auto resolves to 0 (inherited from parent stacking context)
/// Get z-index for stacking context ordering.
///
/// Returns the resolved integer z-index value:
/// - `z-index: auto` → 0 (participates in parent's stacking context)
/// - `z-index: <integer>` → that integer value
#[must_use] pub fn get_z_index(styled_dom: &StyledDom, node_id: Option<NodeId>) -> i32 {
    use azul_css::props::layout::position::LayoutZIndex;

    let Some(node_id) = node_id else {
        return 0;
    };

    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let raw = cc.get_z_index(node_id.index());
            if raw == azul_css::compact_cache::I16_AUTO {
                return 0;
            }
            if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                return i32::from(raw);
            }
            // I16_SENTINEL → fall through to slow path
        }
    }

    // SLOW PATH
    let node_data = &styled_dom.node_data.as_container()[node_id];

    styled_dom
        .css_property_cache
        .ptr
        .get_z_index(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map_or(0, |z| match z {
            LayoutZIndex::Auto => 0,
            LayoutZIndex::Integer(i) => *i,
        })
}

// +spec:positioning:c041c4 - positioned elements with z-index != auto establish stacking contexts
// z-index:<integer> ALWAYS establishes new stacking context on positioned elements
/// Returns true if z-index is `auto` (the initial value), false if it's an explicit `<integer>`.
/// This distinction matters for stacking context creation per §9.9.1.
#[must_use] pub fn is_z_index_auto(styled_dom: &StyledDom, node_id: Option<NodeId>) -> bool {
    use azul_css::props::layout::position::LayoutZIndex;

    let Some(node_id) = node_id else {
        return true;
    };

    let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let raw = cc.get_z_index(node_id.index());
            if raw == azul_css::compact_cache::I16_AUTO {
                return true;
            }
            if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                return false; // explicit integer
            }
            // I16_SENTINEL → fall through to slow path
        }
    }

    // SLOW PATH
    let node_data = &styled_dom.node_data.as_container()[node_id];

    styled_dom
        .css_property_cache
        .ptr
        .get_z_index(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .is_none_or(|z| matches!(z, LayoutZIndex::Auto)) // no value = auto
}

// Rendering Property Getters

/// Information about background color for a node
///
/// # CSS Background Propagation (Special Case for HTML Root)
///
/// According to CSS Backgrounds and Borders Module Level 3, Section "The Canvas Background
/// and the HTML `<body>` Element":
///
/// For HTML documents where the root element is `<html>`, if the computed value of
/// `background-image` on the root element is `none` AND its `background-color` is `transparent`,
/// user agents **must propagate** the computed values of the background properties from the
/// first `<body>` child element to the root element.
///
/// This behavior exists for backwards compatibility with older HTML where backgrounds were
/// typically set on `<body>` using `bgcolor` attributes, and ensures that the `<body>`
/// background covers the entire viewport/canvas even when `<body>` itself has constrained
/// dimensions.
///
/// Implementation: When requesting the background of an `<html>` node, we first check if it
/// has a transparent background with no image. If so, we look for a `<body>` child and use
/// its background instead.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[must_use] pub fn get_background_color(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> ColorU {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    let cache = &styled_dom.css_property_cache.ptr;

    // Fast path: Get this node's background.
    // Negative fast path: if compact cache says `has_background == 0` on a
    // normal-state node, skip the cascade walk entirely. Only declared backgrounds
    // set the bit, so `false` is a safe "unconditionally transparent" signal.
    let get_node_bg = |nid: NodeId, ndata: &azul_core::dom::NodeData, state: &StyledNodeState| {
        if state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if !cc.has_background(nid.index()) {
                    return None;
                }
            }
        }
        cache
            .get_background_content(ndata, &nid, state)
            .and_then(|bg| bg.get_property())
            .and_then(|bg_vec| bg_vec.get(0).cloned())
            .and_then(|first_bg| match &first_bg {
                azul_css::props::style::StyleBackgroundContent::Color(color) => Some(*color),
                azul_css::props::style::StyleBackgroundContent::Image(_) => None, // Has image, not transparent
                _ => None,
            })
    };

    let own_bg = get_node_bg(node_id, node_data, node_state);

    // CSS Background Propagation: Special handling for <html> root element
    // Only check propagation if this is an Html node AND has transparent background (no
    // color/image)
    if !matches!(node_data.node_type, NodeType::Html) || own_bg.is_some() {
        // Not Html or has its own background - return own background or transparent
        return own_bg.unwrap_or(ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        });
    }

    // Html node with transparent background - check if we should propagate from <body>
    let first_child = styled_dom
        .node_hierarchy
        .as_container()
        .get(node_id)
        .and_then(|node| node.first_child_id(node_id));

    let Some(first_child) = first_child else {
        return ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    };

    let first_child_data = &styled_dom.node_data.as_container()[first_child];

    // Check if first child is <body>
    if !matches!(first_child_data.node_type, NodeType::Body) {
        return ColorU {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        };
    }

    // Propagate <body>'s background to <html> (canvas)
    let first_child_state = &styled_dom.styled_nodes.as_container()[first_child].styled_node_state;
    get_node_bg(first_child, first_child_data, first_child_state).unwrap_or(ColorU {
        r: 0,
        g: 0,
        b: 0,
        a: 0,
    })
}

/// Returns all background content layers for a node (colors, gradients, images).
/// This is used for rendering backgrounds that may include linear/radial/conic gradients.
///
/// CSS Background Propagation (CSS Backgrounds 3, Section 2.11.2):
/// For HTML documents, if the root `<html>` element has no background (transparent with no image),
/// propagate the background from the first `<body>` child element.
#[must_use] pub fn get_background_contents(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Vec<azul_css::props::style::StyleBackgroundContent> {
    use azul_core::dom::NodeType;
    use azul_css::props::style::StyleBackgroundContent;

    let node_data = &styled_dom.node_data.as_container()[node_id];
    let cache = &styled_dom.css_property_cache.ptr;

    // Helper to get backgrounds for a node.
    // Negative fast path: if compact cache says `has_background == 0` on a normal
    // pseudo-state node, return empty without walking the cascade.
    let get_node_backgrounds = |nid: NodeId,
                                ndata: &azul_core::dom::NodeData,
                                state: &StyledNodeState|
     -> Vec<StyleBackgroundContent> {
        if state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if !cc.has_background(nid.index()) {
                    return Vec::new();
                }
            }
        }
        cache
            .get_background_content(ndata, &nid, state)
            .and_then(|bg| bg.get_property())
            .map(|bg_vec| bg_vec.iter().cloned().collect())
            .unwrap_or_default()
    };

    let own_backgrounds = get_node_backgrounds(node_id, node_data, node_state);

    // CSS Background Propagation: Special handling for <html> root element
    // Only check propagation if this is an Html node AND has no backgrounds
    if !matches!(node_data.node_type, NodeType::Html) || !own_backgrounds.is_empty() {
        return own_backgrounds;
    }

    // Html node with no backgrounds - check if we should propagate from <body>
    let first_child = styled_dom
        .node_hierarchy
        .as_container()
        .get(node_id)
        .and_then(|node| node.first_child_id(node_id));

    let Some(first_child) = first_child else {
        return own_backgrounds;
    };

    let first_child_data = &styled_dom.node_data.as_container()[first_child];

    // Check if first child is <body>
    if !matches!(first_child_data.node_type, NodeType::Body) {
        return own_backgrounds;
    }

    // Propagate <body>'s backgrounds to <html> (canvas)
    let first_child_state = &styled_dom.styled_nodes.as_container()[first_child].styled_node_state;
    get_node_backgrounds(first_child, first_child_data, first_child_state)
}

/// Information about border rendering
pub struct BorderInfo {
    pub widths: crate::solver3::display_list::StyleBorderWidths,
    pub colors: crate::solver3::display_list::StyleBorderColors,
    pub styles: crate::solver3::display_list::StyleBorderStyles,
}

#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use] pub fn get_border_info(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> BorderInfo {
    use crate::solver3::display_list::{StyleBorderColors, StyleBorderStyles, StyleBorderWidths};
    use azul_css::css::CssPropertyValue;
    use azul_css::props::basic::color::ColorU;
    use azul_css::props::basic::pixel::PixelValue;
    use azul_css::props::style::border::{
        BorderStyle, StyleBorderBottomColor, StyleBorderBottomStyle, StyleBorderLeftColor,
        StyleBorderLeftStyle, StyleBorderRightColor, StyleBorderRightStyle, StyleBorderTopColor,
        StyleBorderTopStyle,
    };
    use azul_css::props::style::{
        LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
        LayoutBorderTopWidth,
    };

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let idx = node_id.index();

            // Border widths: decode from compact i16 (resolved px × 10).
            // Previously this block called the slow convenience getters
            // despite being in the "fast path" branch — 2014 slow walks
            // per width × 4 widths per cold excel.html layout. Fixed
            // 2026-04-17.
            let make_width_px = |raw: i16| -> Option<PixelValue> {
                if raw == azul_css::compact_cache::I16_AUTO
                    || raw == azul_css::compact_cache::I16_INITIAL
                    || raw >= azul_css::compact_cache::I16_SENTINEL_THRESHOLD
                {
                    None
                } else {
                    Some(PixelValue::px(f32::from(raw) / 10.0))
                }
            };
            let widths = StyleBorderWidths {
                top: make_width_px(cc.get_border_top_width_raw(idx))
                    .map(|px| CssPropertyValue::Exact(LayoutBorderTopWidth { inner: px })),
                right: make_width_px(cc.get_border_right_width_raw(idx))
                    .map(|px| CssPropertyValue::Exact(LayoutBorderRightWidth { inner: px })),
                bottom: make_width_px(cc.get_border_bottom_width_raw(idx))
                    .map(|px| CssPropertyValue::Exact(LayoutBorderBottomWidth { inner: px })),
                left: make_width_px(cc.get_border_left_width_raw(idx))
                    .map(|px| CssPropertyValue::Exact(LayoutBorderLeftWidth { inner: px })),
            };

            // Border colors from compact cache
            let make_color = |raw: u32| -> Option<ColorU> {
                if raw == 0 {
                    None
                } else {
                    Some(ColorU {
                        r: ((raw >> 24) & 0xFF) as u8,
                        g: ((raw >> 16) & 0xFF) as u8,
                        b: ((raw >> 8) & 0xFF) as u8,
                        a: (raw & 0xFF) as u8,
                    })
                }
            };

            let colors = StyleBorderColors {
                top: make_color(cc.get_border_top_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderTopColor { inner: c })),
                right: make_color(cc.get_border_right_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderRightColor { inner: c })),
                bottom: make_color(cc.get_border_bottom_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderBottomColor { inner: c })),
                left: make_color(cc.get_border_left_color_raw(idx))
                    .map(|c| CssPropertyValue::Exact(StyleBorderLeftColor { inner: c })),
            };

            // Border styles from compact cache
            let styles = StyleBorderStyles {
                top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                    inner: cc.get_border_top_style(idx),
                })),
                right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                    inner: cc.get_border_right_style(idx),
                })),
                bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                    inner: cc.get_border_bottom_style(idx),
                })),
                left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                    inner: cc.get_border_left_style(idx),
                })),
            };

            return BorderInfo {
                widths,
                colors,
                styles,
            };
        }
    }

    // SLOW PATH: full cascade
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Get all border widths
    let widths = StyleBorderWidths {
        top: styled_dom
            .css_property_cache
            .ptr
            .get_border_top_width(node_data, &node_id, node_state)
            .copied(),
        right: styled_dom
            .css_property_cache
            .ptr
            .get_border_right_width(node_data, &node_id, node_state)
            .copied(),
        bottom: styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_width(node_data, &node_id, node_state)
            .copied(),
        left: styled_dom
            .css_property_cache
            .ptr
            .get_border_left_width(node_data, &node_id, node_state)
            .copied(),
    };

    // Get all border colors
    let colors = StyleBorderColors {
        top: styled_dom
            .css_property_cache
            .ptr
            .get_border_top_color(node_data, &node_id, node_state)
            .copied(),
        right: styled_dom
            .css_property_cache
            .ptr
            .get_border_right_color(node_data, &node_id, node_state)
            .copied(),
        bottom: styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_color(node_data, &node_id, node_state)
            .copied(),
        left: styled_dom
            .css_property_cache
            .ptr
            .get_border_left_color(node_data, &node_id, node_state)
            .copied(),
    };

    // Get all border styles
    let styles = StyleBorderStyles {
        top: styled_dom
            .css_property_cache
            .ptr
            .get_border_top_style(node_data, &node_id, node_state)
            .copied(),
        right: styled_dom
            .css_property_cache
            .ptr
            .get_border_right_style(node_data, &node_id, node_state)
            .copied(),
        bottom: styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_style(node_data, &node_id, node_state)
            .copied(),
        left: styled_dom
            .css_property_cache
            .ptr
            .get_border_left_style(node_data, &node_id, node_state)
            .copied(),
    };

    BorderInfo {
        widths,
        colors,
        styles,
    }
}

/// Convert `BorderInfo` to `InlineBorderInfo` for inline elements
///
/// This resolves the CSS property values to concrete pixel values and colors
/// that can be used during text rendering.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn get_inline_border_info(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    border_info: &BorderInfo,
    viewport: PhysicalSize,
) -> Option<crate::text3::cache::InlineBorderInfo> {
    use crate::text3::cache::InlineBorderInfo;

    // Fetch padding values for inline elements. Viewport units (vw/vh/...) resolve
    // against the real viewport instead of being treated as raw pixels.
    fn resolve_padding(
        mv: MultiValue<PixelValue>,
        viewport: PhysicalSize,
    ) -> f32 {
        match mv {
            MultiValue::Exact(pv) => super::calc::resolve_pixel_value_with_viewport(
                &pv,
                0.0,
                DEFAULT_FONT_SIZE,
                DEFAULT_FONT_SIZE,
                viewport.width,
                viewport.height,
            ),
            _ => 0.0,
        }
    }

    macro_rules! border_width_px {
        ($field:expr) => {
            $field
                .as_ref()
                .and_then(|v| v.get_property())
                .map(|w| w.inner.number.get())
                .unwrap_or(0.0)
        };
    }

    macro_rules! border_color {
        ($field:expr) => {
            $field
                .as_ref()
                .and_then(|v| v.get_property())
                .map(|c| c.inner)
                .unwrap_or(ColorU::BLACK)
        };
    }

    // Extract border-radius (simplified - uses the average of all corners if uniform)
    fn get_border_radius_px(
        styled_dom: &StyledDom,
        node_id: NodeId,
        node_state: &StyledNodeState,
    ) -> Option<f32> {
        let node_data = &styled_dom.node_data.as_container()[node_id];

        let top_left = styled_dom
            .css_property_cache
            .ptr
            .get_border_top_left_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().copied())
            .map(|v| v.inner.number.get());

        let top_right = styled_dom
            .css_property_cache
            .ptr
            .get_border_top_right_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().copied())
            .map(|v| v.inner.number.get());

        let bottom_left = styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_left_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().copied())
            .map(|v| v.inner.number.get());

        let bottom_right = styled_dom
            .css_property_cache
            .ptr
            .get_border_bottom_right_radius(node_data, &node_id, node_state)
            .and_then(|br| br.get_property().copied())
            .map(|v| v.inner.number.get());

        // If any radius is defined, use the maximum (for inline, uniform radius is most common)
        let radii: Vec<f32> = [top_left, top_right, bottom_left, bottom_right]
            .into_iter()
            .flatten()
            .collect();

        if radii.is_empty() {
            None
        } else {
            Some(radii.into_iter().fold(0.0f32, f32::max))
        }
    }

    let top = border_width_px!(&border_info.widths.top);
    let right = border_width_px!(&border_info.widths.right);
    let bottom = border_width_px!(&border_info.widths.bottom);
    let left = border_width_px!(&border_info.widths.left);

    let p_top = resolve_padding(get_css_padding_top(styled_dom, node_id, node_state), viewport);
    let p_right = resolve_padding(get_css_padding_right(styled_dom, node_id, node_state), viewport);
    let p_bottom = resolve_padding(get_css_padding_bottom(styled_dom, node_id, node_state), viewport);
    let p_left = resolve_padding(get_css_padding_left(styled_dom, node_id, node_state), viewport);

    // Only return Some if there's actually a border or padding
    let has_border = top > 0.0 || right > 0.0 || bottom > 0.0 || left > 0.0;
    let has_padding = p_top > 0.0 || p_right > 0.0 || p_bottom > 0.0 || p_left > 0.0;
    if !has_border && !has_padding {
        return None;
    }

    // CSS 2.2 §8.6: detect direction for visual-order border/padding rendering in bidi
    let is_rtl = matches!(
        get_direction_property(styled_dom, node_id, node_state),
        MultiValue::Exact(StyleDirection::Rtl)
    );

    Some(InlineBorderInfo {
        top,
        right,
        bottom,
        left,
        top_color: border_color!(&border_info.colors.top),
        right_color: border_color!(&border_info.colors.right),
        bottom_color: border_color!(&border_info.colors.bottom),
        left_color: border_color!(&border_info.colors.left),
        radius: get_border_radius_px(styled_dom, node_id, node_state),
        padding_top: p_top,
        padding_right: p_right,
        padding_bottom: p_bottom,
        padding_left: p_left,
        is_first_fragment: true,
        is_last_fragment: true,
        is_rtl,
    })
}

// Selection and Caret Styling

/// Style information for text selection rendering
#[derive(Debug, Clone, Copy, Default)]
pub struct SelectionStyle {
    /// Background color of the selection highlight
    pub bg_color: ColorU,
    /// Text color when selected (overrides normal text color)
    pub text_color: Option<ColorU>,
    /// Border radius for selection rectangles
    pub radius: f32,
}

/// Get selection style for a node
#[must_use] pub fn get_selection_style(
    styled_dom: &StyledDom,
    node_id: Option<NodeId>,
    system_style: Option<&std::sync::Arc<azul_css::system::SystemStyle>>,
) -> SelectionStyle {
    let Some(node_id) = node_id else {
        return SelectionStyle::default();
    };

    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &StyledNodeState::default();

    // Try to get selection background from CSS, otherwise use system color, otherwise hard-coded default
    let default_bg = system_style
        .and_then(|ss| ss.colors.selection_background.as_option().copied())
        .unwrap_or(ColorU {
            r: 51,
            g: 153,
            b: 255, // Standard blue selection color
            a: 128, // Semi-transparent
        });

    let bg_color = styled_dom
        .css_property_cache
        .ptr
        .get_selection_background_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().copied())
        .map_or(default_bg, |c| c.inner);

    // Try to get selection text color from CSS, otherwise use system color
    let default_text = system_style.and_then(|ss| ss.colors.selection_text.as_option().copied());

    let text_color = styled_dom
        .css_property_cache
        .ptr
        .get_selection_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().copied())
        .map(|c| c.inner)
        .or(default_text);

    let radius = styled_dom
        .css_property_cache
        .ptr
        .get_selection_radius(node_data, &node_id, node_state)
        .and_then(|r| r.get_property().copied())
        .map_or(0.0, |r| r.inner.to_pixels_internal(0.0, DEFAULT_EM_SIZE, DEFAULT_EM_SIZE));

    SelectionStyle {
        bg_color,
        text_color,
        radius,
    }
}

/// Style information for caret rendering.
#[derive(Debug, Clone, Copy)]
pub struct CaretStyle {
    /// Color of the caret bar
    pub color: ColorU,
    /// Width of the caret bar in pixels
    pub width: f32,
    /// Blink animation duration in milliseconds (0 = no blink)
    pub animation_duration: u32,
}

impl Default for CaretStyle {
    fn default() -> Self {
        Self {
            color: ColorU::BLACK,
            width: DEFAULT_CARET_WIDTH_PX,
            animation_duration: DEFAULT_CARET_BLINK_MS,
        }
    }
}

/// Get caret style for a node
#[must_use] pub fn get_caret_style(styled_dom: &StyledDom, node_id: Option<NodeId>) -> CaretStyle {
    let Some(node_id) = node_id else {
        return CaretStyle::default();
    };

    let node_data = &styled_dom.node_data.as_container()[node_id];
    let node_state = &StyledNodeState::default();

    let color = styled_dom
        .css_property_cache
        .ptr
        .get_caret_color(node_data, &node_id, node_state)
        .and_then(|c| c.get_property().copied())
        .map(|c| c.inner)
        // CSS `caret-color: auto` (the initial value) resolves to currentColor — the
        // element's text color — which by construction contrasts with the background.
        // Falling back to BLACK made the caret invisible on dark backgrounds / dark
        // system themes (and `color` IS inherited while `caret-color` may not be, so a
        // child text node still gets the right colour here).
        .unwrap_or_else(|| {
            styled_dom
                .css_property_cache
                .ptr
                .get_text_color_or_default(node_data, &node_id, node_state)
                .inner
        });

    let width = styled_dom
        .css_property_cache
        .ptr
        .get_caret_width(node_data, &node_id, node_state)
        .and_then(|w| w.get_property().copied())
        .map_or(DEFAULT_CARET_WIDTH_PX, |w| w.inner.to_pixels_internal(0.0, DEFAULT_EM_SIZE, DEFAULT_EM_SIZE));

    let animation_duration = styled_dom
        .css_property_cache
        .ptr
        .get_caret_animation_duration(node_data, &node_id, node_state)
        .and_then(|d| d.get_property().copied())
        .map_or(DEFAULT_CARET_BLINK_MS, |d| d.inner.inner);

    CaretStyle {
        color,
        width,
        animation_duration,
    }
}

// Scrollbar Information

/// Get scrollbar information from a layout node.
///
/// Scrollbar requirements are computed during the layout phase in two paths:
/// - BFC layout: `compute_scrollbar_info()` in cache.rs
/// - Taffy layout: set in the measure callback in `taffy_bridge.rs`
///
/// If neither path set `scrollbar_info`, the node genuinely does not need
/// scrollbars. The previous heuristic (>3 children = force overflow) caused
/// false-positive scrollbars on normal containers.
#[must_use] pub fn get_scrollbar_info_from_layout(node: &LayoutNode) -> ScrollbarRequirements {
    node.scrollbar_info.clone().unwrap_or_default()
}

/// Resolve the **layout-effective** scrollbar width for a node, in pixels.
///
/// This combines three inputs:
/// 1. CSS `scrollbar-width` property on the node (`auto` → 16, `thin` → 8, `none` → 0)
/// 2. OS-level `ScrollbarPreferences.visibility` (overlay scrollbars → 0 layout reservation)
/// 3. Custom `-azul-scrollbar-style` width override
///
/// For **overlay** scrollbars (macOS `WhenScrolling`, or equivalent), this returns `0.0`
/// because overlay scrollbars are painted on top of content and do not consume layout space.
/// The scrollbar is still *rendered*, but no space is reserved during layout.
// +spec:overflow:b83014 - overlay scrollbars do not create scrollbar gutters
///
/// During display-list generation, use `get_scrollbar_style()` instead — that returns
/// the full visual style including the *paint* width (which may be non-zero for overlay).
pub fn get_layout_scrollbar_width_px<T: ParsedFontTrait>(
    ctx: &crate::solver3::LayoutContext<'_, T>,
    dom_id: NodeId,
    styled_node_state: &StyledNodeState,
) -> f32 {
    // Resolve the full scrollbar style (includes per-node CSS overrides + system style).
    // `reserve_width_px` already accounts for overlay vs legacy:
    //   overlay (WhenScrolling) → 0.0
    //   legacy (Always)         → visual_width_px
    let style = get_scrollbar_style(
        ctx.styled_dom,
        dom_id,
        styled_node_state,
        ctx.system_style.as_deref(),
    );
    style.reserve_width_px
}

get_css_property!(
    get_display_property_internal,
    get_display,
    LayoutDisplay,
    CssPropertyType::Display,
    compact = get_display
);

#[must_use] pub fn get_display_property(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> MultiValue<LayoutDisplay> {
    let Some(id) = dom_id else {
        return MultiValue::Exact(LayoutDisplay::Inline);
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    get_display_property_internal(styled_dom, id, node_state)
}

/// CSS Display Module Level 3: Blockification of display values.
///
/// When an element is floated, absolutely positioned, or is the root element,
/// its computed display value may be "blockified" per the table in CSS Display 3 §2.7.
/// This function returns the blockified display value without mutating any state.
#[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[must_use] pub const fn blockify_display(raw_display: LayoutDisplay) -> LayoutDisplay {
    match raw_display {
        // Inline-level display types become their block-level equivalents
        LayoutDisplay::Inline => LayoutDisplay::Block,
        // Per CSS Display 3 §2.7: inline-block blockifies to block
        // (for legacy reasons, loses its flow-root nature)
        LayoutDisplay::InlineBlock => LayoutDisplay::Block,
        LayoutDisplay::InlineFlex => LayoutDisplay::Flex,
        LayoutDisplay::InlineTable => LayoutDisplay::Table,
        LayoutDisplay::InlineGrid => LayoutDisplay::Grid,
        // CSS 2.2 §9.7: table-internal display values blockify to block
        // for absolutely positioned, floated, or root elements
        LayoutDisplay::TableRowGroup
        | LayoutDisplay::TableColumn
        | LayoutDisplay::TableColumnGroup
        | LayoutDisplay::TableHeaderGroup
        | LayoutDisplay::TableFooterGroup
        | LayoutDisplay::TableRow
        | LayoutDisplay::TableCell
        | LayoutDisplay::TableCaption => LayoutDisplay::Block,
        // Already block-level types are unchanged
        other => other,
    }
}

// +spec:positioning:c31c24 - blockification is a computed-value change for absolute/float/root elements
/// Resolves the computed display value for an element, applying blockification
/// rules per CSS Display Module Level 3 §2.7.
// +spec:display-property:641ac5 - computed display value applies blockification/inlinification (not "as specified")
///
/// This centralizes the blockification decision so that all layout phases
/// (`layout_tree`, sizing, positioning) use consistent display values.
// +spec:floats:52aea6 - computed display blockified for floated/positioned/root elements
// +spec:positioning:ce02a1 - out-of-flow boxes (floated or absolutely positioned) get blockified display
#[must_use] pub fn get_computed_display(
    raw_display: LayoutDisplay,
    is_absolute_or_fixed: bool,
    is_floated: bool,
    is_root: bool,
    is_flex_grid_child: bool,
) -> LayoutDisplay {
    if raw_display == LayoutDisplay::None {
        return LayoutDisplay::None;
    }
    // +spec:positioning:69468c - absolute/fixed blockifies the box
    if is_absolute_or_fixed || is_floated || is_root || is_flex_grid_child {
        blockify_display(raw_display)
    } else {
        raw_display
    }
}

// +spec:font-metrics:f7affa - vertical-align shorthand: maps CSS vertical-align values to inline layout alignment
/// Reads the CSS `vertical-align` property for a DOM node and converts it to
/// the text3 `VerticalAlign` enum used during inline layout.
// +spec:display-property:24c160 - vertical-align aligns inline-level box within the line
#[must_use] pub fn get_vertical_align_for_node(
    styled_dom: &StyledDom,
    dom_id: NodeId,
) -> crate::text3::cache::VerticalAlign {
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let va = match get_vertical_align_property(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleVerticalAlign::default(),
    };
    match va {
        StyleVerticalAlign::Baseline => crate::text3::cache::VerticalAlign::Baseline,
        StyleVerticalAlign::Top => crate::text3::cache::VerticalAlign::Top,
        StyleVerticalAlign::Middle => crate::text3::cache::VerticalAlign::Middle,
        StyleVerticalAlign::Bottom => crate::text3::cache::VerticalAlign::Bottom,
        StyleVerticalAlign::Sub => crate::text3::cache::VerticalAlign::Sub,
        StyleVerticalAlign::Superscript => crate::text3::cache::VerticalAlign::Super,
        StyleVerticalAlign::TextTop => crate::text3::cache::VerticalAlign::TextTop,
        StyleVerticalAlign::TextBottom => crate::text3::cache::VerticalAlign::TextBottom,
        // +spec:line-height:b41ee3 - percentage vertical-align: raise/lower by % of line-height, 0% = baseline
        StyleVerticalAlign::Percentage(p) => {
            let font_size = get_element_font_size(styled_dom, dom_id, node_state);
            let line_height = get_line_height_value(styled_dom, dom_id, node_state)
                .map_or(font_size * 1.2, |lh| lh.inner.normalized() * font_size);
            crate::text3::cache::VerticalAlign::Offset(p.normalized() * line_height)
        }
        // §10.8.1: <length> is absolute offset from baseline
        StyleVerticalAlign::Length(l) => {
            let font_size = get_element_font_size(styled_dom, dom_id, node_state);
            // TODO(superplan): viewport units (vw/vh/...) in a vertical-align <length>
            // fall back to raw pixels here because this getter has no viewport ctx.
            // Threading `viewport_size` requires changing this fn's signature, but one
            // of its callers (`sizing.rs::process_layout_children`) lives outside
            // Group 2's file ownership — deferred. (The sibling path in
            // fc.rs::translate_to_text3_constraints already resolves it via
            // `resolve_pixel_value_with_viewport`.)
            let px = super::calc::resolve_pixel_value(&l, 0.0, font_size, font_size);
            crate::text3::cache::VerticalAlign::Offset(px)
        }
    }
}

#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Panics
///
/// Panics only on an internal indexing invariant (an in-range `get().unwrap()` over the font-family list).
pub fn get_style_properties(
    styled_dom: &StyledDom,
    dom_id: NodeId,
    system_style: Option<&std::sync::Arc<azul_css::system::SystemStyle>>,
    viewport_size: PhysicalSize,
) -> StyleProperties {
    use azul_css::props::basic::{PhysicalSize, PropertyContext, ResolutionContext};

    let node_data = &styled_dom.node_data.as_container()[dom_id];
    let node_state = &styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
    let cache = &styled_dom.css_property_cache.ptr;

    // Fast path: use compact cache reverse map (works for inherited values on text nodes).
    // Slow path: only for non-normal pseudo states (:hover, :focus, etc.)
    let font_families = if node_state.is_normal() {
        cache
            .compact_cache
            .as_ref()
            .and_then(|cc| {
                let fh = cc.tier2b_text[dom_id.index()].font_family_hash;
                if fh == 0 {
                    return None;
                }
                cc.font_hash_to_families.get(&fh).cloned()
            })
            .unwrap_or_else(|| {
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("serif".into())])
            })
    } else {
        cache
            .get_font_family(node_data, &dom_id, node_state)
            .and_then(|v| v.get_property().cloned())
            .unwrap_or_else(|| {
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("serif".into())])
            })
    };

    // Get parent's font-size for proper em resolution in font-size property.
    // FAST PATH: `get_parent_font_size` goes through `get_element_font_size`
    // which hits the memoised `resolved_font_sizes_px` Vec (O(1) array index).
    // The old code here walked the full CSS cascade for every call — 1485
    // slow walks per cold excel.html layout. Replaced 2026-04-17.
    let parent_font_size = get_parent_font_size(styled_dom, dom_id, node_state);

    let root_font_size = get_root_font_size(styled_dom, node_state);

    // Create resolution context for font-size (em refers to parent)
    let font_size_context = ResolutionContext {
        element_font_size: DEFAULT_FONT_SIZE, /* Not used for font-size property */
        parent_font_size,
        root_font_size,
        containing_block_size: PhysicalSize::new(0.0, 0.0),
        element_size: None,
        viewport_size,
    };

    // Get font-size: either from this node's CSS, or inherit from parent
    // font-size is an inheritable property, so if the node doesn't have
    // an explicit font-size, it should inherit from the parent (not default to 16px)
    let font_size = {
        // FAST PATH: compact cache for normal state.
        // Sentinel/inherit/initial → inherit from parent directly (which is
        // what the slow cascade walk would fall back to via `.unwrap_or(parent_font_size)`
        // anyway — avoid the walk entirely).
        let mut fast_font_size: Option<f32> = None;
        let mut compact_said_inherit = false;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_font_size_raw(dom_id.index());
                if raw == azul_css::compact_cache::U32_SENTINEL
                    || raw == azul_css::compact_cache::U32_INHERIT
                    || raw == azul_css::compact_cache::U32_INITIAL
                {
                    compact_said_inherit = true;
                } else if let Some(pv) = azul_css::compact_cache::decode_pixel_value_u32(raw) {
                    fast_font_size = Some(
                        pv.resolve_with_context(&font_size_context, PropertyContext::FontSize),
                    );
                }
            }
        }
        fast_font_size.unwrap_or_else(|| {
            if compact_said_inherit {
                parent_font_size
            } else {
                cache
                    .get_font_size(node_data, &dom_id, node_state)
                    .and_then(|v| v.get_property().copied())
                    .map_or(parent_font_size, |v| {
                        v.inner
                            .resolve_with_context(&font_size_context, PropertyContext::FontSize)
                    })
            }
        })
    };

    let color_from_cache = {
        // FAST PATH: compact cache for text color
        let mut fast_color = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_text_color_raw(dom_id.index());
                if raw != 0 {
                    // Decode 0xRRGGBBAA → ColorU
                    fast_color = Some(ColorU {
                        r: (raw >> 24) as u8,
                        g: (raw >> 16) as u8,
                        b: (raw >> 8) as u8,
                        a: raw as u8,
                    });
                }
            }
        }
        fast_color.or_else(|| {
            cache
                .get_text_color(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().copied())
                .map(|v| v.inner)
        })
    };

    // CSS initial value for 'color' is UA-dependent but conventionally black.
    // Do NOT use system_style.colors.text here — that reflects the OS theme
    // (e.g. white on macOS dark mode) and would produce white text on
    // explicitly light-colored backgrounds.  System colors (CanvasText etc.)
    // should only be used when referenced through CSS system-color keywords.
    let color = color_from_cache.unwrap_or(ColorU::BLACK);

    // +spec:font-metrics:e480da - line-height: normal/number/length/percentage resolution
    let line_height = {
        // FAST PATH: compact cache for line-height (stored as normalized × 1000 i16).
        // When the cache returns Some → we have a resolved value.
        // When it returns None AND node_state is normal → the compact cache stored
        // the sentinel, which means "line-height: normal" (the spec default).
        // Previously we fell through to a cascade walk here — but the default
        // has already been authoritatively decided by the builder, so the walk
        // would only ever re-confirm "no value, normal". 1600 pure-waste walks
        // per cold excel.html layout. Short-circuit to Normal directly.
        let mut fast_lh = None;
        let mut sentinel_normal = false;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(normalized) = cc.get_line_height(dom_id.index()) {
                    fast_lh = Some(crate::text3::cache::LineHeight::Px(
                        normalized / 100.0 * font_size,
                    ));
                } else {
                    // Sentinel in compact cache = "normal" (CSS default).
                    sentinel_normal = true;
                }
            }
        }
        if sentinel_normal {
            crate::text3::cache::LineHeight::Normal
        } else {
            fast_lh.unwrap_or_else(|| {
                cache
                    .get_line_height(node_data, &dom_id, node_state)
                    .and_then(|v| v.get_property().copied())
                    .map_or(crate::text3::cache::LineHeight::Normal, |v| crate::text3::cache::LineHeight::Px(v.inner.normalized() * font_size))
            })
        }
    };

    // Get background color for INLINE elements only
    // CSS background-color is NOT inherited. For block-level elements (th, td, div, etc.),
    // the background is painted separately by paint_element_background() in display_list.rs.
    // Only inline elements (span, em, strong, a, etc.) should have their background color
    // propagated through StyleProperties for the text rendering pipeline.
    //
    // FAST PATH: use the compact-cache-backed display getter. The old code
    // here called `cache.get_display(..)` (the 3-arg convenience method on
    // CssPropertyCache) which routes through `get_property_slow` — 1485 slow
    // walks per cold excel.html layout. Replaced 2026-04-17.
    let display = match get_display_property(styled_dom, Some(dom_id)) {
        MultiValue::Exact(v) => v,
        _ => LayoutDisplay::Inline,
    };

    // For inline and inline-block elements, get background content and border info
    // Block elements have their backgrounds/borders painted by display_list.rs
    let (background_color, background_content, border) =
        if matches!(display, LayoutDisplay::Inline | LayoutDisplay::InlineBlock) {
            let bg = get_background_color(styled_dom, dom_id, node_state);
            let bg_color = if bg.a > 0 { Some(bg) } else { None };

            // Get full background contents (including gradients)
            let bg_contents = get_background_contents(styled_dom, dom_id, node_state);

            // Get border info for inline elements
            let border_info = get_border_info(styled_dom, dom_id, node_state);
            let inline_border =
                get_inline_border_info(styled_dom, dom_id, node_state, &border_info, viewport_size);

            (bg_color, bg_contents, inline_border)
        } else {
            // Block-level elements: background/border is painted by display_list.rs
            // via push_backgrounds_and_border() in DisplayListBuilder
            (None, Vec::new(), None)
        };

    // Query font-weight from CSS cache
    let font_weight = match get_font_weight_property(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleFontWeight::Normal,
    };

    // Query font-style from CSS cache
    let font_style = match get_font_style_property(styled_dom, dom_id, node_state) {
        MultiValue::Exact(v) => v,
        _ => StyleFontStyle::Normal,
    };

    // Convert StyleFontWeight/StyleFontStyle to fontconfig types
    let fc_weight = super::fc::convert_font_weight(font_weight);
    let fc_style = super::fc::convert_font_style(font_style);

    // Check if any font family is a FontRef - if so, use FontStack::Ref
    // This allows embedded fonts (like Material Icons) to bypass fontconfig
    let font_stack = {
        let font_ref = (0..font_families.len()).find_map(|i| match font_families.get(i).unwrap() {
            azul_css::props::basic::font::StyleFontFamily::Ref(r) => Some(r.clone()),
            _ => None,
        });

        font_ref.map_or_else(
            || {
                // Get platform for resolving system font types. None on the paged /
                // PDF layout path (system_style is hard-coded None there);
                // build_font_selector_stack then resolves via Platform::current() so
                // the names stay in lock-step with the font-loading pass.
                let platform = system_style.map(|ss| &ss.platform);
                FontStack::Stack(build_font_selector_stack(
                    &font_families,
                    platform,
                    fc_weight,
                    fc_style,
                ))
            },
            FontStack::Ref,
        )
    };

    // Get letter-spacing from CSS
    let letter_spacing = {
        // FAST PATH: compact cache for letter-spacing (i16 resolved px × 10)
        let mut fast_ls = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(px_val) = cc.get_letter_spacing(dom_id.index()) {
                    fast_ls = Some(crate::text3::cache::Spacing::Px(px_val.round() as i32));
                }
            }
        }
        fast_ls.unwrap_or_else(|| {
            cache
                .get_letter_spacing(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().copied())
                .map(|v| {
                    let px_value = v
                        .inner
                        .resolve_with_context(&font_size_context, PropertyContext::FontSize);
                    crate::text3::cache::Spacing::Px(px_value.round() as i32)
                })
                .unwrap_or_default()
        })
    };

    // Get word-spacing from CSS
    let word_spacing = {
        // FAST PATH: compact cache for word-spacing (i16 resolved px × 10)
        let mut fast_ws = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if let Some(px_val) = cc.get_word_spacing(dom_id.index()) {
                    fast_ws = Some(crate::text3::cache::Spacing::Px(px_val.round() as i32));
                }
            }
        }
        fast_ws.unwrap_or_else(|| {
            cache
                .get_word_spacing(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().copied())
                .map(|v| {
                    let px_value = v
                        .inner
                        .resolve_with_context(&font_size_context, PropertyContext::FontSize);
                    crate::text3::cache::Spacing::Px(px_value.round() as i32)
                })
                .unwrap_or_default()
        })
    };

    // Get text-decoration from CSS.
    //
    // Fast path: the compact cache keeps a `has_text_decoration` flag. If
    // unset (the overwhelmingly common case — plain body text has no
    // decoration set), skip the 4-pseudo-state × 6-layer cascade walk
    // entirely. Only nodes that actually set text-decoration pay the walk.
    let text_decoration = {
        let mut skip_walk = false;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                if !cc.has_text_decoration(dom_id.index()) {
                    skip_walk = true;
                }
            }
        }
        if skip_walk {
            crate::text3::cache::TextDecoration::default()
        } else {
            cache
                .get_text_decoration(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().copied())
                .map(crate::text3::cache::TextDecoration::from_css)
                .unwrap_or_default()
        }
    };

    // Get tab-size (tab-size) from CSS.
    //
    // tab-size defaults to `I16_SENTINEL` in the compact cache builder
    // (spec default is "8", meaning 8 space widths). The old fallback
    // called `cache.get_tab_size(..)` (slow cascade) for every node whose
    // raw was SENTINEL — virtually every node, because almost nothing sets
    // tab-size. That was 1485 pure-waste slow walks per cold layout.
    //
    // New behaviour: sentinel → 8.0 directly. Only walk the cascade when
    // the compact cache is genuinely unavailable (no `compact_cache`) or
    // the node is in a pseudo-state that bypassed the cache.
    let tab_size = {
        let mut fast_tab = None;
        if node_state.is_normal() {
            if let Some(ref cc) = cache.compact_cache {
                let raw = cc.get_tab_size_raw(dom_id.index());
                if raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD {
                    fast_tab = Some(f32::from(raw) / 10.0);
                } else {
                    // Sentinel / Inherit / Initial → spec default is 8.
                    fast_tab = Some(8.0);
                }
            }
        }
        fast_tab.unwrap_or_else(|| {
            cache
                .get_tab_size(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property().copied())
                .map_or(DEFAULT_TAB_SIZE, |v| v.inner.number.get())
        })
    };

    

    StyleProperties {
        font_stack,
        font_size_px: font_size,
        color,
        background_color,
        background_content,
        border,
        line_height,
        letter_spacing,
        word_spacing,
        text_decoration,
        tab_size,
        // These still use defaults - could be extended in future:
        // font_features, font_variations, text_transform, writing_mode,
        // text_orientation, text_combine_upright, font_variant_*
        ..Default::default()
    }
}

#[must_use] pub fn get_list_style_type(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> StyleListStyleType {
    let Some(id) = dom_id else {
        return StyleListStyleType::default();
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_list_style_type(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

#[must_use] pub fn get_list_style_position(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> StyleListStylePosition {
    let Some(id) = dom_id else {
        return StyleListStylePosition::default();
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_list_style_position(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or_default()
}

// New: Taffy Bridge Getters - Box Model Properties with Ua Css Fallback

use azul_css::props::layout::{
    LayoutInsetBottom, LayoutLeft, LayoutMarginBottom, LayoutMarginLeft, LayoutMarginRight,
    LayoutMarginTop, LayoutMaxHeight, LayoutMaxWidth, LayoutMinHeight, LayoutMinWidth,
    LayoutPaddingBottom, LayoutPaddingLeft, LayoutPaddingRight, LayoutPaddingTop, LayoutRight,
    LayoutTop,
};

/// Get inset (position) properties - returns MultiValue<PixelValue>
get_css_property_pixel!(
    get_css_left,
    get_left,
    CssPropertyType::Left,
    compact_i16 = get_left
);
get_css_property_pixel!(
    get_css_right,
    get_right,
    CssPropertyType::Right,
    compact_i16 = get_right
);
get_css_property_pixel!(
    get_css_top,
    get_top,
    CssPropertyType::Top,
    compact_i16 = get_top
);
get_css_property_pixel!(
    get_css_bottom,
    get_bottom,
    CssPropertyType::Bottom,
    compact_i16 = get_bottom
);

/// Get margin properties - returns MultiValue<PixelValue>
get_css_property_pixel!(
    get_css_margin_left,
    get_margin_left,
    CssPropertyType::MarginLeft,
    compact_i16 = get_margin_left_raw
);
get_css_property_pixel!(
    get_css_margin_right,
    get_margin_right,
    CssPropertyType::MarginRight,
    compact_i16 = get_margin_right_raw
);
get_css_property_pixel!(
    get_css_margin_top,
    get_margin_top,
    CssPropertyType::MarginTop,
    compact_i16 = get_margin_top_raw
);
get_css_property_pixel!(
    get_css_margin_bottom,
    get_margin_bottom,
    CssPropertyType::MarginBottom,
    compact_i16 = get_margin_bottom_raw
);

/// Get padding properties - returns MultiValue<PixelValue>
get_css_property_pixel!(
    get_css_padding_left,
    get_padding_left,
    CssPropertyType::PaddingLeft,
    compact_i16 = get_padding_left_raw
);
get_css_property_pixel!(
    get_css_padding_right,
    get_padding_right,
    CssPropertyType::PaddingRight,
    compact_i16 = get_padding_right_raw
);
get_css_property_pixel!(
    get_css_padding_top,
    get_padding_top,
    CssPropertyType::PaddingTop,
    compact_i16 = get_padding_top_raw
);
get_css_property_pixel!(
    get_css_padding_bottom,
    get_padding_bottom,
    CssPropertyType::PaddingBottom,
    compact_i16 = get_padding_bottom_raw
);

/// Get min/max size properties
get_css_property!(
    get_css_min_width,
    get_min_width,
    LayoutMinWidth,
    CssPropertyType::MinWidth,
    compact_u32_struct = get_min_width_raw
);

get_css_property!(
    get_css_min_height,
    get_min_height,
    LayoutMinHeight,
    CssPropertyType::MinHeight,
    compact_u32_struct = get_min_height_raw
);

get_css_property!(
    get_css_max_width,
    get_max_width,
    LayoutMaxWidth,
    CssPropertyType::MaxWidth,
    compact_u32_struct = get_max_width_raw
);

get_css_property!(
    get_css_max_height,
    get_max_height,
    LayoutMaxHeight,
    CssPropertyType::MaxHeight,
    compact_u32_struct = get_max_height_raw
);

/// Get border width properties (no UA CSS fallback needed, defaults to 0)
get_css_property_pixel!(
    get_css_border_left_width,
    get_border_left_width,
    CssPropertyType::BorderLeftWidth,
    compact_i16 = get_border_left_width_raw
);
get_css_property_pixel!(
    get_css_border_right_width,
    get_border_right_width,
    CssPropertyType::BorderRightWidth,
    compact_i16 = get_border_right_width_raw
);
get_css_property_pixel!(
    get_css_border_top_width,
    get_border_top_width,
    CssPropertyType::BorderTopWidth,
    compact_i16 = get_border_top_width_raw
);
get_css_property_pixel!(
    get_css_border_bottom_width,
    get_border_bottom_width,
    CssPropertyType::BorderBottomWidth,
    compact_i16 = get_border_bottom_width_raw
);

// Fragmentation (page breaking) properties

/// Get break-before property for paged media
#[must_use] pub fn get_break_before(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PageBreak {
    let Some(id) = dom_id else {
        return PageBreak::Auto;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    // Negative fast path: break-* is almost never declared.
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_break(id.index()) {
                return PageBreak::Auto;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[id];
    styled_dom
        .css_property_cache
        .ptr
        .get_break_before(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or(PageBreak::Auto)
}

/// Get break-after property for paged media
#[must_use] pub fn get_break_after(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> PageBreak {
    let Some(id) = dom_id else {
        return PageBreak::Auto;
    };
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_break(id.index()) {
                return PageBreak::Auto;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[id];
    styled_dom
        .css_property_cache
        .ptr
        .get_break_after(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or(PageBreak::Auto)
}

/// Check if a `PageBreak` value forces a page break (always, page, left, right, etc.)
#[must_use] pub const fn is_forced_page_break(page_break: PageBreak) -> bool {
    matches!(
        page_break,
        PageBreak::Always
            | PageBreak::Page
            | PageBreak::Left
            | PageBreak::Right
            | PageBreak::Recto
            | PageBreak::Verso
            | PageBreak::All
    )
}

/// Get break-inside property for paged media
#[must_use] pub fn get_break_inside(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> BreakInside {
    let Some(id) = dom_id else {
        return BreakInside::Auto;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_break_inside(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or(BreakInside::Auto)
}

/// Get orphans property (minimum lines at bottom of page)
#[must_use] pub fn get_orphans(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> u32 {
    let Some(id) = dom_id else {
        return 2; // Default value
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_orphans(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .map_or(2, |o| o.inner)
}

/// Get widows property (minimum lines at top of page)
#[must_use] pub fn get_widows(styled_dom: &StyledDom, dom_id: Option<NodeId>) -> u32 {
    let Some(id) = dom_id else {
        return 2; // Default value
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_widows(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .map_or(2, |w| w.inner)
}

/// Get box-decoration-break property
#[must_use] pub fn get_box_decoration_break(
    styled_dom: &StyledDom,
    dom_id: Option<NodeId>,
) -> BoxDecorationBreak {
    let Some(id) = dom_id else {
        return BoxDecorationBreak::Slice;
    };
    let node_data = &styled_dom.node_data.as_container()[id];
    let node_state = &styled_dom.styled_nodes.as_container()[id].styled_node_state;
    styled_dom
        .css_property_cache
        .ptr
        .get_box_decoration_break(node_data, &id, node_state)
        .and_then(|v| v.get_property().copied())
        .unwrap_or(BoxDecorationBreak::Slice)
}

// Helper functions for break properties

/// Check if a `PageBreak` value is avoid
#[must_use] pub const fn is_avoid_page_break(page_break: &PageBreak) -> bool {
    matches!(page_break, PageBreak::Avoid | PageBreak::AvoidPage)
}

/// Check if a `BreakInside` value prevents breaks
#[must_use] pub const fn is_avoid_break_inside(break_inside: &BreakInside) -> bool {
    matches!(
        break_inside,
        BreakInside::Avoid | BreakInside::AvoidPage | BreakInside::AvoidColumn
    )
}

// Font Chain Resolution - Pre-Layout Font Loading

use std::collections::HashMap;

use rust_fontconfig::{
    FcFontCache, FcWeight, FontFallbackChain, PatternMatch, UnicodeRange,
    DEFAULT_UNICODE_FALLBACK_SCRIPTS,
};

use crate::text3::cache::{FontChainKey, FontChainKeyOrRef, FontSelector, FontStack, FontStyle};

/// Build a fontconfig `FontSelector` stack from a list of CSS font families.
///
/// Shared by `get_style_properties` and `collect_font_stacks_from_styled_dom`.
/// `Ref` families are skipped (callers handle embedded fonts via `FontStack::Ref`),
/// `SystemType` families expand to the platform's fallback chain, and the generic
/// `sans-serif`/`serif`/`monospace` fallbacks are appended if not already present.
///
/// When `platform` is `None` (e.g. the paged / PDF layout path that hard-codes
/// `system_style = None`), system fonts resolve via `Platform::current()` so the
/// names stay in lock-step with the font-loading pass (which always uses
/// `Platform::current()`); diverging to a bare "sans-serif" would not match the
/// names the loader registered → zero glyphs → text collapses to 0 width.
// The `platform` binding uses a pre-declared `let current;` so the else branch can
// extend the lifetime of a freshly-computed Platform and hand back a reference to it;
// map_or_else cannot express this (the closure would return a dangling local ref).
#[allow(clippy::option_if_let_else)]
fn build_font_selector_stack(
    font_families: &StyleFontFamilyVec,
    platform: Option<&azul_css::system::Platform>,
    fc_weight: FcWeight,
    fc_style: FontStyle,
) -> Vec<FontSelector> {
    let mut stack = Vec::with_capacity(font_families.len() + 3);

    for i in 0..font_families.len() {
        let family = font_families.get(i).unwrap();
        if matches!(family, StyleFontFamily::Ref(_)) {
            continue;
        }
        if let StyleFontFamily::SystemType(system_type) = family {
            let current;
            let platform = if let Some(p) = platform { p } else {
                current = azul_css::system::Platform::current();
                &current
            };
            let font_names = system_type.get_fallback_chain(platform);
            let system_weight = if system_type.is_bold() {
                FcWeight::Bold
            } else {
                fc_weight
            };
            let system_style = if system_type.is_italic() {
                FontStyle::Italic
            } else {
                fc_style
            };
            for font_name in font_names {
                stack.push(FontSelector {
                    family: font_name.to_string(),
                    weight: system_weight,
                    style: system_style,
                    unicode_ranges: Vec::new(),
                });
            }
        } else {
            stack.push(FontSelector {
                family: family.as_string(),
                weight: fc_weight,
                style: fc_style,
                unicode_ranges: Vec::new(),
            });
        }
    }

    for fallback in &["sans-serif", "serif", "monospace"] {
        if !stack
            .iter()
            .any(|f| f.family.eq_ignore_ascii_case(fallback))
        {
            stack.push(FontSelector {
                family: (*fallback).to_string(),
                weight: FcWeight::Normal,
                style: FontStyle::Normal,
                unicode_ranges: Vec::new(),
            });
        }
    }

    stack
}

/// Result of collecting font stacks from a `StyledDom`
/// Contains all unique font stacks and the mapping from `StyleFontFamiliesHash` to `FontChainKey`
#[derive(Debug, Clone)]
pub struct CollectedFontStacks {
    /// All unique font stacks found in the document (system/file fonts via fontconfig)
    pub font_stacks: Vec<Vec<FontSelector>>,
    /// Map from the font stack hash to the index in `font_stacks`
    pub hash_to_index: HashMap<u64, usize>,
    /// Direct `FontRefs` that bypass fontconfig (e.g., embedded icon fonts)
    /// These are keyed by their pointer address for uniqueness
    pub font_refs: HashMap<usize, azul_css::props::basic::font::FontRef>,
}

/// Resolved font chains ready for use in layout
/// This is the result of resolving font stacks against `FcFontCache`
#[derive(Debug, Clone)]
pub struct ResolvedFontChains {
    /// Map from `FontChainKeyOrRef` to the resolved `FontFallbackChain`
    /// For `FontChainKeyOrRef::Ref` variants, the `FontFallbackChain` contains
    /// a single-font chain that covers the entire Unicode range.
    pub chains: HashMap<FontChainKeyOrRef, FontFallbackChain>,
}

impl ResolvedFontChains {
    /// Get a font chain by its key
    #[must_use] pub fn get(&self, key: &FontChainKeyOrRef) -> Option<&FontFallbackChain> {
        self.chains.get(key)
    }

    /// Get a font chain by `FontChainKey` (for system fonts)
    #[must_use] pub fn get_by_chain_key(&self, key: &FontChainKey) -> Option<&FontFallbackChain> {
        self.chains.get(&FontChainKeyOrRef::Chain(key.clone()))
    }

    /// Get a font chain for a font stack (via fontconfig)
    #[must_use] pub fn get_for_font_stack(&self, font_stack: &[FontSelector]) -> Option<&FontFallbackChain> {
        let key = FontChainKeyOrRef::Chain(FontChainKey::from_selectors(font_stack));
        self.chains.get(&key)
    }

    /// Get a font chain for a `FontRef` pointer
    #[must_use] pub fn get_for_font_ref(&self, ptr: usize) -> Option<&FontFallbackChain> {
        self.chains.get(&FontChainKeyOrRef::Ref(ptr))
    }

    /// Consume self and return the inner `HashMap` with `FontChainKeyOrRef` keys
    ///
    /// This is useful when you need access to both Chain and Ref variants.
    #[must_use] pub fn into_inner(self) -> HashMap<FontChainKeyOrRef, FontFallbackChain> {
        self.chains
    }

    /// Consume self and return only the fontconfig-resolved chains
    ///
    /// This filters out `FontRef` entries and returns only the chains
    /// resolved via fontconfig. This is what `FontManager` expects.
    #[must_use] pub fn into_fontconfig_chains(self) -> HashMap<FontChainKey, FontFallbackChain> {
        // (2026-06-10: reverted to HashMap end-to-end — the empty-hashbrown RawIter hang behind
        // the 2026-06-05 BTreeMap migration was the un-mirrored EMPTY_GROUP static, fixed
        // transpiler-side in symbol_table.rs::compute_hashbrown_empty_group_ranges.)
        let mut out: HashMap<FontChainKey, FontFallbackChain> = HashMap::new();
        if self.chains.is_empty() {
            return out;
        }
        for (key, chain) in self.chains {
            if let FontChainKeyOrRef::Chain(chain_key) = key {
                out.insert(chain_key, chain);
            }
        }
        out
    }

    /// Get the number of resolved chains
    #[must_use] pub fn len(&self) -> usize {
        self.chains.len()
    }

    /// Check if there are no resolved chains
    #[must_use] pub fn is_empty(&self) -> bool {
        self.chains.is_empty()
    }

    /// Get the number of direct `FontRefs`
    #[must_use] pub fn font_refs_len(&self) -> usize {
        self.chains.keys().filter(|k| k.is_ref()).count()
    }
}

/// Collect all unique font stacks from a `StyledDom`
///
/// This is a pure function that iterates over all nodes in the DOM and
/// extracts the font-family property from each node that has text content.
///
/// # Arguments
/// * `styled_dom` - The styled DOM to extract font stacks from
/// * `platform` - The current platform for resolving system font types
///
/// # Returns
/// A `CollectedFontStacks` containing all unique font stacks and a hash-to-index mapping
#[allow(clippy::cast_possible_truncation)] // bounded graphics/coord/font/fixed-point/debug-marker cast
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use] pub fn collect_font_stacks_from_styled_dom(
    styled_dom: &StyledDom,
    platform: &azul_css::system::Platform,
) -> CollectedFontStacks {
    use azul_css::compact_cache::{
        FONT_STYLE_MASK, FONT_STYLE_SHIFT, FONT_WEIGHT_MASK, FONT_WEIGHT_SHIFT,
    };

    let mut font_stacks = Vec::new();
    let mut hash_to_index: HashMap<u64, usize> = HashMap::new();
    let mut font_refs: HashMap<usize, azul_css::props::basic::font::FontRef> = HashMap::new();

    let node_data = styled_dom.node_data.as_container();
    let cache = &styled_dom.css_property_cache.ptr;
    let Some(compact) = cache.compact_cache.as_ref() else {
        return CollectedFontStacks {
            font_stacks,
            hash_to_index,
            font_refs,
        };
    };

    // Phase 1: Scan compact cache arrays (just u64 reads) to find unique
    // (font_family_hash, weight, style) tuples. Record one representative
    // node index per unique tuple for the expensive CSS lookup in Phase 2.
    // Key: (font_family_hash, weight_encoded, style_encoded) → representative node index
    // (2026-06-10: reverted to HashMap — the historic g81/g47 empty-hashbrown mis-lift was the
    // un-mirrored EMPTY_GROUP static, fixed transpiler-side in symbol_table.rs::
    // compute_hashbrown_empty_group_ranges. std HashMap lifts correctly now; RandomState seeds
    // via the transpiler's HashmapRandomKeys fixed-seed body.)
    let mut unique_font_keys: HashMap<(u64, u8, u8), usize> = HashMap::new();
    let node_count = node_data.internal.len();

    // WEB-LIFT: probe node_type bytes (NodeType #[repr(C,u8)], Text=177 per AzDom_createText).
    // 0x406D0..DC = n1.node_type bytes[0,1,2,4]; 0x406E0 = n0.node_type byte[0] (body disc).
    if node_count > 1 {
        let p1 = (&raw const node_data.internal[1].node_type).cast::<u8>();
        let p0 = (&raw const node_data.internal[0].node_type).cast::<u8>();
        unsafe {
            crate::az_mark(0x606D0_u32, u32::from(core::ptr::read(p1)));
            crate::az_mark(0x606D4_u32, u32::from(core::ptr::read(p1.add(1))));
            crate::az_mark(0x606D8_u32, u32::from(core::ptr::read(p1.add(2))));
            crate::az_mark(0x606DC_u32, u32::from(core::ptr::read(p1.add(4))));
            crate::az_mark(0x606E0_u32, u32::from(core::ptr::read(p0)));
        }
    }
    for i in 0..node_count {
        // Only text nodes need fonts. WEB-LIFT: the lifted `matches!(node_type,
        // NodeType::Text(_))` MIS-LIFTS (compares against a mis-lifted discriminant
        // constant) — text nodes never match → no font stack → no chain → text h=0.
        // NodeType is #[repr(C,u8)] so the discriminant is the u8 at offset 0; Text=177
        // (per AzDom_createText: `mov w8,#0xb1; strb w8,[x19]`). Compare the raw
        // discriminant to the literal 177 (a source literal lifts correctly).
        let nt_disc = unsafe {
            core::ptr::read((&raw const node_data.internal[i].node_type).cast::<u8>())
        };
        let is_text = nt_disc == 177
            || matches!(node_data.internal[i].node_type, NodeType::Text(_));
        if !is_text {
            continue;
        }
        let fh = compact.tier2b_text[i].font_family_hash;
        let t1 = compact.tier1_enums[i];
        let weight_bits = ((t1 >> FONT_WEIGHT_SHIFT) & FONT_WEIGHT_MASK) as u8;
        let style_bits = ((t1 >> FONT_STYLE_SHIFT) & FONT_STYLE_MASK) as u8;
        let key = (fh, weight_bits, style_bits);
        unique_font_keys.entry(key).or_insert(i);
    }

    // WASM-ONLY PROBE (REVERT): why 0 chains? 0x406C0=tag(5E5E0003), C4=node_count,
    // C8=unique_font_keys.len() (#text nodes matched in Phase 1). If C8=0 → the lifted
    // `matches!(node_type, NodeType::Text(_))` FAILS for the text node (node_type mis-lift)
    // → no font stack → no chain → text h=0. C is the count of NodeType::Text via a raw
    // discriminant byte read (node_type tag), to compare against the matches! result.
    {
        let mut raw_text = 0u32;
        for i in 0..node_count {
            // NodeType is repr(C,u8)-ish; read the leading discriminant byte directly.
            let nt_ptr = (&raw const node_data.internal[i].node_type).cast::<u8>();
            let disc = unsafe { core::ptr::read_volatile(nt_ptr) };
            // Text is one specific discriminant; count whatever the body node ISN'T.
            if disc != unsafe { core::ptr::read_volatile((&raw const node_data.internal[0].node_type).cast::<u8>()) } {
                raw_text += 1;
            }
        }
        unsafe {
            crate::az_mark(0x606C0_u32, (0x5E5E_0003_u32));
            crate::az_mark(0x606C4_u32, (node_count as u32));
            crate::az_mark(0x606C8_u32, (unique_font_keys.len() as u32));
            crate::az_mark(0x606CC_u32, (raw_text));
        }
    }

    // Phase 2: For each unique tuple, do ONE expensive CSS lookup on the
    // representative node to get the actual font-family names.
    let styled_nodes = styled_dom.styled_nodes.as_container();

    for (&(fh, _wb, _sb), &repr_idx) in &unique_font_keys {
        let Some(dom_id) = NodeId::from_usize(repr_idx) else {
            continue;
        };
        let node_state = &styled_nodes[dom_id].styled_node_state;

        // Use reverse map from compact cache: hash → actual font families.
        // This works for ALL nodes including text nodes that inherit font-family
        // via compact cache (where get_property_slow would return None).
        let font_families = compact
            .font_hash_to_families
            .get(&fh)
            .cloned()
            .unwrap_or_else(|| {
                StyleFontFamilyVec::from_vec(vec![StyleFontFamily::System("serif".into())])
            });

        // Check for embedded FontRef
        if let Some(first_family) = font_families.get(0) {
            if let StyleFontFamily::Ref(font_ref) = first_family {
                let ptr = font_ref.parsed as usize;
                font_refs.entry(ptr).or_insert_with(|| font_ref.clone());
                continue;
            }
        }

        let font_weight = match get_font_weight_property(styled_dom, dom_id, node_state) {
            MultiValue::Exact(v) => v,
            _ => StyleFontWeight::Normal,
        };
        let font_style = match get_font_style_property(styled_dom, dom_id, node_state) {
            MultiValue::Exact(v) => v,
            _ => StyleFontStyle::Normal,
        };

        let fc_weight = super::fc::convert_font_weight(font_weight);
        let fc_style = super::fc::convert_font_style(font_style);

        let font_stack =
            build_font_selector_stack(&font_families, Some(platform), fc_weight, fc_style);

        if font_stack.is_empty() {
            continue;
        }

        let key = FontChainKey::from_selectors(&font_stack);
        let hash = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            key.hash(&mut hasher);
            hasher.finish()
        };

        hash_to_index.entry(hash).or_insert_with(|| {
            let idx = font_stacks.len();
            font_stacks.push(font_stack);
            idx
        });
    }

    CollectedFontStacks {
        font_stacks,
        hash_to_index,
        font_refs,
    }
}

/// Resolve all font chains for the collected font stacks
///
/// This is a pure function that takes the collected font stacks and resolves
/// them against the `FcFontCache` to produce `FontFallbackChains`.
///
/// # Arguments
/// * `collected` - The collected font stacks from `collect_font_stacks_from_styled_dom`
/// * `fc_cache` - The fontconfig cache to resolve fonts against
///
/// # Returns
/// A `ResolvedFontChains` containing all resolved font chains
/// Walk every text node in `styled_dom` and collect the set of
/// non-ASCII codepoints actually present in the document.
///
/// Used by [`prune_chain_to_used_chars`] to drop CSS-fallback fonts
/// from a resolved chain when the *first* match in a `css_fallbacks`
/// group already covers everything the page asks for. ASCII (`< 0x80`)
/// is universally covered by every Latin font we'd resolve, so we
/// skip it here to keep the set small. Unicode characters in the
/// returned set are deduped + sorted via `BTreeSet`.
///
/// Cost: O(total text length). Cheap relative to layout itself.
#[must_use] pub fn collect_used_codepoints(styled_dom: &StyledDom) -> std::collections::BTreeSet<u32> {
    let mut out = std::collections::BTreeSet::new();
    let node_data = styled_dom.node_data.as_container();
    for node in node_data.internal {
        let NodeType::Text(s) = &node.node_type else {
            continue;
        };
        for c in s.as_str().chars() {
            let cp = c as u32;
            if cp >= 0x80 {
                out.insert(cp);
            }
        }
    }
    out
}

/// Like [`collect_used_codepoints`] but keeps ASCII.
///
/// The fast-probe
/// path (`FcFontRegistry::request_fonts_fast`) *does* need ASCII:
/// "the font has to cover every codepoint I will render" is only
/// true if we tell it every codepoint, and "Segoe UI" not being
/// installed on macOS means even ASCII has to fall through to a
/// system default.
///
/// `collect_used_codepoints` strips ASCII because its caller
/// (`prune_chain_to_used_chars`) runs *after* resolution to trim an
/// already-resolved chain and every Latin-covering font passes ASCII
/// trivially. That assumption doesn't hold during probing.
#[must_use] pub fn collect_used_codepoints_all(styled_dom: &StyledDom) -> std::collections::BTreeSet<char> {
    let mut out = std::collections::BTreeSet::new();
    let node_data = styled_dom.node_data.as_container();
    for node in node_data.internal {
        let NodeType::Text(s) = &node.node_type else {
            continue;
        };
        for c in s.as_str().chars() {
            out.insert(c);
        }
    }
    out
}

/// Trim a [`FontFallbackChain`] down to the minimum set of `FontMatch`
/// entries needed to cover `used_chars` (typically from
/// [`collect_used_codepoints`]).
///
/// For each `css_fallbacks` group, walk matches in the resolver's
/// preferred order and keep them until every codepoint in
/// `used_chars` is covered (per the OS/2 unicode-range bits cached
/// in `FontMatch.unicode_ranges`). Always keeps at least the first
/// match per group so a font listed in CSS doesn't disappear.
///
/// `unicode_fallbacks` is filtered to only include fonts whose
/// ranges intersect `used_chars` — Phase-6's
/// [`scripts_present_in_styled_dom`] already scopes the *script
/// blocks* but a single block (e.g. CJK Unified, U+4E00..U+9FFF)
/// can have hundreds of matching system fonts; this prunes them
/// down to the few that actually cover the codepoints used.
///
/// On excel.html (~ASCII-only) this drops the per-chain
/// `css_fallbacks` from 5 → 1 in each group, eliminating ~20 of
/// the 26 fonts that would otherwise be parsed by
/// `load_fonts_from_disk`.
pub fn prune_chain_to_used_chars(
    chain: &mut FontFallbackChain,
    used_chars: &std::collections::BTreeSet<u32>,
) {
    fn fm_covers(fm: &rust_fontconfig::FontMatch, cp: u32) -> bool {
        fm.unicode_ranges
            .iter()
            .any(|r| cp >= r.start && cp <= r.end)
    }

    for group in &mut chain.css_fallbacks {
        if group.fonts.is_empty() {
            continue;
        }
        // Track which non-ASCII chars still need coverage as we walk
        // matches in order. We always keep at least the first match.
        let mut needed: Vec<u32> = used_chars.iter().copied().collect();
        needed.retain(|&cp| !fm_covers(&group.fonts[0], cp));
        let mut keep = 1;
        for fm in group.fonts.iter().skip(1) {
            if needed.is_empty() {
                break;
            }
            keep += 1;
            needed.retain(|&cp| !fm_covers(fm, cp));
        }
        group.fonts.truncate(keep);
    }

    chain
        .unicode_fallbacks
        .retain(|fm| used_chars.iter().any(|&cp| fm_covers(fm, cp)));
}

/// Scan text-node content in `styled_dom` and return the subset of
/// [`rust_fontconfig::DEFAULT_UNICODE_FALLBACK_SCRIPTS`] whose code-point
/// ranges actually appear in any text.
///
/// Short-circuits once all seven
/// ranges have been seen.
///
/// Callers pass the result as `scripts_hint` to
/// [`resolve_font_chains`] / [`collect_and_resolve_font_chains_with_registration`];
/// `rust_fontconfig::FcFontCache::resolve_font_chain_with_scripts` then
/// only pulls in Unicode-fallback fonts for scripts the document
/// actually uses. An ASCII-only page returns an empty vector, which
/// avoids dragging Arial Unicode MS, CJK fonts, etc. into the
/// resolved chain and therefore into the eager-load step.
#[must_use] pub fn scripts_present_in_styled_dom(styled_dom: &StyledDom) -> Vec<UnicodeRange> {
    let scripts = DEFAULT_UNICODE_FALLBACK_SCRIPTS;
    let mut seen = vec![false; scripts.len()];
    let mut hits = 0usize;
    let node_data = styled_dom.node_data.as_container();
    'outer: for node in node_data.internal {
        let text: &str = match &node.node_type {
            NodeType::Text(s) => s.as_str(),
            _ => continue,
        };
        for c in text.chars() {
            let cp = c as u32;
            // Cheap reject: everything below the first fallback-script
            // range (Cyrillic starts at U+0400) is covered by the CSS
            // fallbacks' own glyphs — no reason to probe.
            if cp < 0x0400 {
                continue;
            }
            for (idx, r) in scripts.iter().enumerate() {
                if !seen[idx] && cp >= r.start && cp <= r.end {
                    seen[idx] = true;
                    hits += 1;
                    if hits == scripts.len() {
                        break 'outer;
                    }
                    break;
                }
            }
        }
    }
    scripts
        .iter()
        .enumerate()
        .filter_map(|(i, r)| if seen[i] { Some(*r) } else { None })
        .collect()
}

/// Resolve font chains for a collected set of stacks.
///
/// `scripts_hint`:
/// - `None` keeps the original "all 7 default scripts" behaviour
///   (Cyrillic / Arabic / Devanagari / Hiragana / Katakana / CJK /
///   Hangul) — equivalent to passing
///   `Some(rust_fontconfig::DEFAULT_UNICODE_FALLBACK_SCRIPTS)`.
/// - `Some(&[])` attaches *no* Unicode fallbacks, suitable for
///   ASCII-only documents. Combined with `prune_chain_to_used_chars`
///   this is what eliminates Arial Unicode MS / CJK / Arabic font
///   loads on Latin-only pages.
/// - `Some(ranges)` attaches fallbacks only for the listed scripts.
///   Production callers compute this via
///   [`scripts_present_in_styled_dom`].
#[must_use] pub fn resolve_font_chains(
    collected: &CollectedFontStacks,
    fc_cache: &FcFontCache,
    scripts_hint: Option<&[UnicodeRange]>,
) -> ResolvedFontChains {
    resolve_font_chains_with_registry(collected, fc_cache, None, scripts_hint)
}

/// Registry-aware variant of [`resolve_font_chains`].
///
/// When `registry`
/// is `Some`, each chain resolution goes through
/// [`rust_fontconfig::registry::FcFontRegistry::request_and_resolve_with_scripts`]
/// which priority-bumps the builder for families not yet in the
/// snapshot and waits for them — the "scout-on-demand" path that
/// avoids the eager common-stack pre-parse.
///
/// When `registry` is `None`, falls back to
/// [`rust_fontconfig::FcFontCache::resolve_font_chain_with_scripts`]
/// against the passed-in snapshot, which is what
/// [`resolve_font_chains`] does and what every code path did before
/// Phase 3.
#[must_use] pub fn resolve_font_chains_with_registry(
    collected: &CollectedFontStacks,
    fc_cache: &FcFontCache,
    registry: Option<&rust_fontconfig::registry::FcFontRegistry>,
    scripts_hint: Option<&[UnicodeRange]>,
) -> ResolvedFontChains {
    let mut chains = HashMap::new();

    // Resolve system/file font stacks via fontconfig
    for font_stack in &collected.font_stacks {
        if font_stack.is_empty() {
            continue;
        }

        // Build font families list
        // (2026-06-10) Build the key through the ONE canonical constructor
        // (FontChainKey::from_selectors — first-wins dedup + the same empty-stack
        // fallback) so the stored key always matches the shaping-time lookup key.
        let canonical_key = FontChainKey::from_selectors(font_stack);
        let font_families = canonical_key.font_families.clone();

        let weight = font_stack[0].weight;
        let is_italic = font_stack[0].style == FontStyle::Italic;
        let is_oblique = font_stack[0].style == FontStyle::Oblique;

        let cache_key = FontChainKeyOrRef::Chain(FontChainKey {
            font_families: font_families.clone(),
            weight,
            italic: is_italic,
            oblique: is_oblique,
        });

        // Skip if already resolved
        if chains.contains_key(&cache_key) {
            continue;
        }

        // Resolve the font chain
        // IMPORTANT: Use False (not DontCare) when style is Normal.
        // DontCare means "accept italic too" which can match italic fonts.
        // False means "must NOT be italic" which correctly prefers Normal.
        let italic = if is_italic {
            PatternMatch::True
        } else {
            PatternMatch::False
        };
        let oblique = if is_oblique {
            PatternMatch::True
        } else {
            PatternMatch::False
        };

        // Registry-aware resolve: scout-on-demand path when available.
        // See `resolve_font_chains_with_registry` doc for rationale.
        let chain = registry.map_or_else(
            || {
                let mut trace = Vec::new();
                fc_cache.resolve_font_chain_with_scripts(
                    &font_families,
                    weight,
                    italic,
                    oblique,
                    scripts_hint,
                    &mut trace,
                )
            },
            |reg| {
                reg.request_and_resolve_with_scripts(
                    &font_families,
                    weight,
                    italic,
                    oblique,
                    scripts_hint,
                )
            },
        );

        // WEB-LIFT last resort (in azul-layout, NOT rust-fontconfig — so the fragile
        // `with_memory_fonts` isn't re-codegen'd into a trapping shape): the lifted
        // resolve_font_chain query path can return an EMPTY chain even when a fallback
        // font IS registered (generic→OS-name expansion + token/unicode query is
        // lift-fragile). If the chain has no fonts, append the first registered font so
        // load_missing_for_chains / resolve_char find it and text shapes (not measure 0).
        let mut chain = chain;
        let total_fonts = chain.css_fallbacks.iter().map(|g| g.fonts.len()).sum::<usize>()
            + chain.unicode_fallbacks.len();
        if total_fonts == 0 {
            if let Some((_pattern, id)) = fc_cache.list().first() {
                // Vec::new() ranges (not pattern.unicode_ranges.clone()) — the Vec-clone
                // mis-lifts on the web backend and empty == "no range restriction" here.
                chain.unicode_fallbacks.push(rust_fontconfig::FontMatch {
                    id: *id,
                    unicode_ranges: Vec::new(),
                    fallbacks: Vec::new(),
                });
            }
        }

        chains.insert(cache_key, chain);
    }

    // NOTE: FontRefs bypass fontconfig entirely — the shaping code checks
    // style.font_stack for FontStack::Ref and uses the font data directly.
    // No entries are inserted into `chains` for them.

    ResolvedFontChains { chains }
}

/// WEB-LIFT last resort, applied LIFT-SAFELY. The lifted backend drops in-place
/// mutations made through `BTreeMap::values_mut()` (the pushed `FontMatch` is silently
/// lost — same class as the cascade `From` mapped-collect drop) and mis-lifts the
/// `pattern.unicode_ranges.clone()` Vec-clone. So this rebuilds the map with an explicit
/// `for` loop (no `values_mut`) and appends a coverage-agnostic fallback using
/// `Vec::new()` ranges (the convention already used across this file for "no specific
/// range restriction"). Applied on BOTH resolver return paths — the fast path otherwise
/// returns chains with no last resort at all, so when the lifted
/// `query_matches`/`find_unicode_fallbacks` yields an empty chain even though a fallback
/// font IS registered, the text node measures 0 → `LayoutError::InvalidTree`.
fn ensure_chains_nonempty(resolved: &mut ResolvedFontChains, fc_cache: &FcFontCache) {
    let fallback_id = match fc_cache.list().first() {
        Some((_pattern, id)) => *id,
        None => return,
    };
    let keys: Vec<FontChainKeyOrRef> = resolved.chains.keys().cloned().collect();
    let mut rebuilt: HashMap<FontChainKeyOrRef, FontFallbackChain> =
        HashMap::new();
    for key in keys {
        if let Some(mut chain) = resolved.chains.remove(&key) {
            let total = chain.css_fallbacks.iter().map(|g| g.fonts.len()).sum::<usize>()
                + chain.unicode_fallbacks.len();
            if total == 0 {
                chain.unicode_fallbacks.push(rust_fontconfig::FontMatch {
                    id: fallback_id,
                    unicode_ranges: Vec::new(),
                    fallbacks: Vec::new(),
                });
            }
            rebuilt.insert(key, chain);
        }
    }
    resolved.chains = rebuilt;
}

/// Convenience function that collects and resolves font chains in one call
///
/// # Arguments
/// * `styled_dom` - The styled DOM to extract font stacks from
/// * `fc_cache` - The fontconfig cache to resolve fonts against
/// * `platform` - The current platform for resolving system font types
///
/// # Returns
/// A `ResolvedFontChains` containing all resolved font chains
/// Collect font stacks, register embedded fonts, and resolve font chains
/// in a single pass over the DOM nodes. Replaces the old two-pass approach
/// where `register_embedded_fonts_from_styled_dom` + `collect_and_resolve_font_chains`
/// each independently scanned all nodes.
pub fn collect_and_resolve_font_chains_with_registration<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    fc_cache: &FcFontCache,
    font_manager: &crate::text3::cache::FontManager<T>,
    platform: &azul_css::system::Platform,
) -> ResolvedFontChains {
    let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);

    // Register embedded FontRefs (from the same scan, no second pass)
    for font_ref in collected.font_refs.values() {
        font_manager.register_embedded_font(font_ref);
    }

    // Fast path (rust-fontconfig 4.2): when a registry is attached
    // we can resolve each stack by cmap-probing candidate files
    // against the codepoints the DOM actually uses, instead of
    // letting `request_fonts` eagerly parse every CSS fallback
    // via allsorts. On excel.html this drops `font_chain_resolve`
    // from ~128 ms / 49 faces parsed to ~5 ms / 3 faces.
    //
    // Falls back to the legacy pattern-map resolver when:
    //   - no registry is present (offline `FcFontCache` callers)
    //   - the DOM has no text codepoints (no shaping to be done,
    //     so cmap-probing has nothing to check and partial-cover
    //     entries would be surprising)
    if let Some(registry) = font_manager.registry.as_deref() {
        let used_chars = collect_used_codepoints_all(styled_dom);
        if !used_chars.is_empty() {
            let mut fast = resolve_font_chains_fast(&collected, registry, &used_chars);
            ensure_chains_nonempty(&mut fast, fc_cache);
            return fast;
        }
    }

    // Legacy path: pattern-map resolver. Only reached when the
    // caller passes an `FcFontCache` without a live registry
    // (ad-hoc tests, the PDF writer, etc.).
    let scripts = scripts_present_in_styled_dom(styled_dom);
    let mut resolved = resolve_font_chains_with_registry(
        &collected,
        fc_cache,
        font_manager.registry.as_deref(),
        Some(&scripts),
    );

    let used_chars = collect_used_codepoints(styled_dom);
    for chain in resolved.chains.values_mut() {
        prune_chain_to_used_chars(chain, &used_chars);
    }
    // WEB-LIFT last resort (AFTER the prune, so it survives — the prune drops fonts
    // whose parsed cmap doesn't cover used_chars, which removes the registered fallback
    // before it's parsed): if a chain ended up empty, append the first registered font
    // so load_missing_for_chains finds it and text shapes instead of measuring 0.
    // LIFT-SAFE rebuild (see ensure_chains_nonempty) — the old `values_mut()` +
    // `unicode_ranges.clone()` version dropped the push in the lifted backend, leaving
    // the chain empty (web-text-min n1 measured 0xfffffffe/auto → InvalidTree).
    ensure_chains_nonempty(&mut resolved, fc_cache);
    resolved
}

/// Fast-path resolver backed by [`FcFontRegistry::request_fonts_fast`].
///
/// Iterates `collected.font_stacks`, shapes each `(stack, weight,
/// italic, oblique)` combo into a cmap-probe request carrying the
/// DOM's codepoint set, calls the registry, and returns a
/// `ResolvedFontChains` keyed by `FontChainKeyOrRef::Chain` — the
/// same keys the legacy resolver emits, so downstream code
/// (`load_missing_for_chains`, `shape_with_font_fallback`) is
/// unchanged.
pub fn resolve_font_chains_fast(
    collected: &CollectedFontStacks,
    registry: &rust_fontconfig::registry::FcFontRegistry,
    codepoints: &std::collections::BTreeSet<char>,
) -> ResolvedFontChains {
    use rust_fontconfig::PatternMatch;

    static DBG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let dbg = *DBG.get_or_init(|| std::env::var_os("AZ_FAST_RESOLVE_DEBUG").is_some());

    let mut chains: HashMap<FontChainKeyOrRef, FontFallbackChain> = HashMap::new();

    for font_stack in &collected.font_stacks {
        if font_stack.is_empty() {
            continue;
        }

        // (2026-06-10) Build the key through the ONE canonical constructor
        // (FontChainKey::from_selectors — first-wins dedup + the same empty-stack
        // fallback) so the stored key always matches the shaping-time lookup key.
        let canonical_key = FontChainKey::from_selectors(font_stack);
        let font_families = canonical_key.font_families.clone();

        let weight = font_stack[0].weight;
        let is_italic = font_stack[0].style == FontStyle::Italic;
        let is_oblique = font_stack[0].style == FontStyle::Oblique;

        let cache_key = FontChainKeyOrRef::Chain(FontChainKey {
            font_families: font_families.clone(),
            weight,
            italic: is_italic,
            oblique: is_oblique,
        });

        if chains.contains_key(&cache_key) {
            continue;
        }

        let italic_match = if is_italic {
            PatternMatch::True
        } else {
            PatternMatch::False
        };

        let request = vec![(font_families.clone(), codepoints.clone())];
        let mut chains_out = registry.request_fonts_fast(&request, weight, italic_match);
        if dbg {
            let total_fonts: usize = chains_out
                .iter()
                .map(|c| c.css_fallbacks.iter().map(|g| g.fonts.len()).sum::<usize>())
                .sum();
            eprintln!(
                "[FAST] stack {:?} w={:?} i={:?} → {} groups, {} faces",
                font_families,
                weight,
                italic_match,
                chains_out
                    .first()
                    .map_or(0, |c| c.css_fallbacks.len()),
                total_fonts,
            );
        }
        if let Some(chain) = chains_out.pop() {
            chains.insert(cache_key, chain);
        }
    }

    ResolvedFontChains { chains }
}

/// Legacy wrapper: collect + resolve without registration. Kept for
/// backward compatibility; defaults to the full 7-script unicode
/// fallback set.
#[must_use] pub fn collect_and_resolve_font_chains(
    styled_dom: &StyledDom,
    fc_cache: &FcFontCache,
    platform: &azul_css::system::Platform,
) -> ResolvedFontChains {
    let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);
    resolve_font_chains(&collected, fc_cache, None)
}

/// Legacy wrapper: register only. Prefer `collect_and_resolve_font_chains_with_registration`.
pub fn register_embedded_fonts_from_styled_dom<T: ParsedFontTrait>(
    styled_dom: &StyledDom,
    font_manager: &crate::text3::cache::FontManager<T>,
    platform: &azul_css::system::Platform,
) {
    let collected = collect_font_stacks_from_styled_dom(styled_dom, platform);
    for font_ref in collected.font_refs.values() {
        font_manager.register_embedded_font(font_ref);
    }
}

// Font Loading Functions

use std::collections::HashSet;

use rust_fontconfig::FontId;

/// Extract all unique `FontIds` from resolved font chains
///
/// This function collects all `FontIds` that are referenced in the font chains,
/// which represents the complete set of fonts that may be needed for rendering.
#[must_use] pub fn collect_font_ids_from_chains(chains: &ResolvedFontChains) -> HashSet<FontId> {
    let mut font_ids = HashSet::new();

    // M12.7: hashbrown's RawIterRange (the .values() iterator below) mis-lifts
    // to wasm and loops forever on an empty map; is_empty() is len-based, so
    // bail out before iterating when there are no chains (web bare-body case).
    if chains.chains.is_empty() {
        return font_ids;
    }

    for chain in chains.chains.values() {
        // Collect from CSS fallbacks
        for group in &chain.css_fallbacks {
            for font in &group.fonts {
                font_ids.insert(font.id);
            }
        }

        // Collect from Unicode fallbacks
        for font in &chain.unicode_fallbacks {
            font_ids.insert(font.id);
        }
    }

    font_ids
}

/// Compute which fonts need to be loaded (diff with already loaded fonts)
///
/// # Arguments
/// * `required_fonts` - Set of `FontIds` that are needed
/// * `already_loaded` - Set of `FontIds` that are already loaded
///
/// # Returns
/// Set of `FontIds` that need to be loaded
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
#[must_use] pub fn compute_fonts_to_load(
    required_fonts: &HashSet<FontId>,
    already_loaded: &HashSet<FontId>,
) -> HashSet<FontId> {
    // M12.7: `.difference()` drives hashbrown's RawIterRange, which mis-lifts
    // to wasm and loops on an empty map. Nothing required → nothing to load.
    if required_fonts.is_empty() {
        return HashSet::new();
    }
    required_fonts.difference(already_loaded).copied().collect()
}

/// Result of loading fonts
#[derive(Debug)]
pub struct FontLoadResult<T> {
    /// Successfully loaded fonts
    pub loaded: HashMap<FontId, T>,
    /// `FontIds` that failed to load, with error messages
    pub failed: Vec<(FontId, String)>,
}

/// Load fonts from disk using the provided loader function
///
/// This is a generic function that works with any font loading implementation.
/// The `load_fn` parameter should be a function that takes font bytes and an index,
/// and returns a parsed font or an error.
///
/// # Arguments
/// * `font_ids` - Set of `FontIds` to load
/// * `fc_cache` - The fontconfig cache to get font paths from
/// * `load_fn` - Function to load and parse font bytes
///
/// # Returns
/// A `FontLoadResult` containing successfully loaded fonts and any failures
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
pub fn load_fonts_from_disk<T, F>(
    font_ids: &HashSet<FontId>,
    fc_cache: &FcFontCache,
    load_fn: F,
) -> FontLoadResult<T>
where
    // Bytes come in as `Arc<FontBytes>` so the loader can retain
    // them cheaply (one `Arc::clone` per retained copy). On disk the
    // backing is an mmap, so untouched glyf/CFF pages don't count
    // toward RSS — the layout shaper only faults in pages it reads.
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> Result<T, crate::text3::cache::LayoutError>,
{
    let mut loaded = HashMap::new();
    let mut failed = Vec::new();

    for font_id in font_ids {
        // Get font bytes from fc_cache as a shared mmap. Faces backed
        // by the same .ttc all observe the same `Arc<FontBytes>` via
        // rust_fontconfig's `shared_bytes` dedup.
        let Some(font_bytes) = fc_cache.get_font_bytes(font_id) else {
            failed.push((
                *font_id,
                format!("Could not get font bytes for {font_id:?}"),
            ));
            continue;
        };

        // Get font index (for font collections like .ttc files)
        let font_index = fc_cache
            .get_font_by_id(font_id)
            .map_or(0, |source| match source {
                rust_fontconfig::OwnedFontSource::Disk(path) => path.font_index,
                rust_fontconfig::OwnedFontSource::Memory(font) => font.font_index,
            });

        // Load the font using the provided function
        match load_fn(font_bytes, font_index) {
            Ok(font) => {
                loaded.insert(*font_id, font);
            }
            Err(e) => {
                failed.push((
                    *font_id,
                    format!("Failed to parse font {font_id:?}: {e:?}"),
                ));
            }
        }
    }

    FontLoadResult { loaded, failed }
}

/// Convenience function to load all required fonts for a styled DOM
///
/// This function:
/// 1. Collects all font stacks from the DOM
/// 2. Resolves them to font chains
/// 3. Extracts all required `FontIds`
/// 4. Computes which fonts need to be loaded (diff with already loaded)
/// 5. Loads the missing fonts
///
/// # Arguments
/// * `styled_dom` - The styled DOM to extract font requirements from
/// * `fc_cache` - The fontconfig cache
/// * `already_loaded` - Set of `FontIds` that are already loaded
/// * `load_fn` - Function to load and parse font bytes
/// * `platform` - The current platform for resolving system font types
///
/// # Returns
/// A tuple of (`ResolvedFontChains`, `FontLoadResult`)
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
pub fn resolve_and_load_fonts<T, F>(
    styled_dom: &StyledDom,
    fc_cache: &FcFontCache,
    already_loaded: &HashSet<FontId>,
    load_fn: F,
    platform: &azul_css::system::Platform,
) -> (ResolvedFontChains, FontLoadResult<T>)
where
    F: Fn(
        std::sync::Arc<rust_fontconfig::FontBytes>,
        usize,
    ) -> Result<T, crate::text3::cache::LayoutError>,
{
    // Step 1-2: Collect and resolve font chains
    let chains = collect_and_resolve_font_chains(styled_dom, fc_cache, platform);

    // Step 3: Extract all required FontIds
    let required_fonts = collect_font_ids_from_chains(&chains);

    // Step 4: Compute diff
    let fonts_to_load = compute_fonts_to_load(&required_fonts, already_loaded);

    // Step 5: Load missing fonts
    let load_result = load_fonts_from_disk(&fonts_to_load, fc_cache, load_fn);

    (chains, load_result)
}

// ============================================================================
// Scrollbar Style Getters
// ============================================================================

use azul_css::props::style::scrollbar::{
    LayoutScrollbarWidth, ScrollbarVisibilityMode, StyleScrollbarColor,
};

/// Computed scrollbar style for a node.
///
/// All visual defaults (colors, width) come from the UA CSS conditional rules
/// in `core/src/ua_css.rs` — individual `CssPropertyWithConditions` entries for
/// `scrollbar-color` and `scrollbar-width`, keyed on `@os` / `@theme`.
///
/// Overlay behaviour (fade timing, visibility, clip) is derived from the
/// resolved `scrollbar-width` mode:
///   - `thin`  → overlay:  fade 500/200 ms, `WhenScrolling`, clip = true
///   - `auto`  → classic:  no fade, `Always`, clip = false
///   - `none`  → hidden:   no fade, `Always`, clip = false
///
/// Per-node CSS overrides (in priority order):
///   1. `-azul-scrollbar-style`  (full `ScrollbarInfo` override)
///   2. `scrollbar-width`        (overrides width + overlay mode)
///   3. `scrollbar-color`        (overrides thumb / track colours)
#[derive(Debug, Clone)]
pub struct ComputedScrollbarStyle {
    /// The scrollbar width mode (auto/thin/none)
    pub width_mode: LayoutScrollbarWidth,
    /// Visual width in pixels — used for rendering track + thumb.
    /// Non-zero even for overlay scrollbars.
    pub visual_width_px: f32,
    /// Reserve width in pixels — layout space subtracted from content area.
    /// 0 for overlay scrollbars, equal to `visual_width_px` for legacy.
    pub reserve_width_px: f32,
    /// Thumb color
    pub thumb_color: ColorU,
    /// Track color
    pub track_color: ColorU,
    /// Button color (for scroll arrows)
    pub button_color: ColorU,
    /// Corner color (where scrollbars meet)
    pub corner_color: ColorU,
    /// Whether to clip the scrollbar to the container's border-radius
    pub clip_to_container_border: bool,
    /// Delay in ms before scrollbar starts fading out (0 = never fade)
    pub fade_delay_ms: u32,
    /// Duration of fade-out animation in ms (0 = instant)
    pub fade_duration_ms: u32,
    /// Scrollbar visibility mode (always / when-scrolling / auto)
    pub visibility: ScrollbarVisibilityMode,
    /// Whether to show top/bottom (or left/right) arrow buttons.
    /// When false, the track spans the entire scrollbar length.
    pub show_scroll_buttons: bool,
    /// Size of each arrow button in px (square: width = height).
    /// Only used when `show_scroll_buttons == true`.
    pub scroll_button_size_px: f32,
    /// Whether to show the corner rect where V and H scrollbars meet.
    pub show_corner_rect: bool,
    /// Thumb color when hovered (None = use `thumb_color`)
    pub thumb_color_hover: Option<ColorU>,
    /// Thumb color when pressed/active (None = use `thumb_color`)
    pub thumb_color_active: Option<ColorU>,
    /// Track color when hovered (None = use `track_color`)
    pub track_color_hover: Option<ColorU>,
    /// Visual width when hovered (None = use `visual_width_px`)
    pub visual_width_px_hover: Option<f32>,
    /// Visual width when pressed (None = use `visual_width_px`)
    pub visual_width_px_active: Option<f32>,
}

impl Default for ComputedScrollbarStyle {
    fn default() -> Self {
        // Evaluate UA CSS rules with a default context (no OS info).
        // Picks the unconditional fallback: classic light, auto width.
        let ctx = azul_css::dynamic_selector::DynamicSelectorContext::default();
        let ua = azul_core::ua_css::evaluate_ua_scrollbar_css(&ctx);
        Self::from_ua_resolved(&ua)
    }
}

impl ComputedScrollbarStyle {
    /// Build from resolved UA scrollbar CSS properties.
    ///
    /// Each property is read individually from the resolved UA CSS.
    fn from_ua_resolved(ua: &azul_core::ua_css::ResolvedUaScrollbar) -> Self {
        let width_mode = ua.width;
        let visibility = ua.visibility;
        let fade_delay_ms = ua.fade_delay.ms;
        let fade_duration_ms = ua.fade_duration.ms;

        let visual_width_px = match width_mode {
            LayoutScrollbarWidth::Thin => SCROLLBAR_WIDTH_THIN,
            LayoutScrollbarWidth::Auto => SCROLLBAR_WIDTH_AUTO,
            LayoutScrollbarWidth::None => 0.0,
        };

        // Overlay scrollbars don't reserve layout space and hide buttons / corner.
        let is_overlay = visibility == ScrollbarVisibilityMode::WhenScrolling;
        let reserve_width_px = if is_overlay { 0.0 } else { visual_width_px };
        let show_scroll_buttons = !is_overlay;
        let scroll_button_size_px = if is_overlay { 0.0 } else { visual_width_px };
        let show_corner_rect = !is_overlay;

        let (thumb_color, track_color) = match ua.color {
            StyleScrollbarColor::Custom(c) => (c.thumb, c.track),
            StyleScrollbarColor::Auto => (ColorU::TRANSPARENT, ColorU::TRANSPARENT),
        };

        // Compute hover / active variants:
        // Hover: lighten thumb, widen by +SCROLLBAR_HOVER_EXPAND_PX
        // Active: darken thumb, widen by +SCROLLBAR_HOVER_EXPAND_PX
        let thumb_hover = ColorU {
            r: thumb_color.r.saturating_add(THUMB_HOVER_LIGHTEN),
            g: thumb_color.g.saturating_add(THUMB_HOVER_LIGHTEN),
            b: thumb_color.b.saturating_add(THUMB_HOVER_LIGHTEN),
            a: thumb_color.a.saturating_add(THUMB_HOVER_ALPHA_ADD),
        };
        let thumb_active = ColorU {
            r: thumb_color.r.saturating_sub(THUMB_ACTIVE_DARKEN),
            g: thumb_color.g.saturating_sub(THUMB_ACTIVE_DARKEN),
            b: thumb_color.b.saturating_sub(THUMB_ACTIVE_DARKEN),
            a: 255,
        };
        let track_hover = ColorU {
            r: track_color.r,
            g: track_color.g,
            b: track_color.b,
            a: track_color.a.saturating_add(THUMB_HOVER_ALPHA_ADD),
        };
        let hover_width = visual_width_px + SCROLLBAR_HOVER_EXPAND_PX;
        let active_width = visual_width_px + SCROLLBAR_HOVER_EXPAND_PX;

        Self {
            width_mode,
            visual_width_px,
            reserve_width_px,
            thumb_color,
            track_color,
            button_color: ColorU::TRANSPARENT,
            corner_color: ColorU::TRANSPARENT,
            clip_to_container_border: is_overlay,
            fade_delay_ms,
            fade_duration_ms,
            visibility,
            show_scroll_buttons,
            scroll_button_size_px,
            show_corner_rect,
            thumb_color_hover: Some(thumb_hover),
            thumb_color_active: Some(thumb_active),
            track_color_hover: Some(track_hover),
            visual_width_px_hover: Some(hover_width),
            visual_width_px_active: Some(active_width),
        }
    }
}

/// Get the computed scrollbar style for a node.
///
/// Resolution order (later wins):
///   1. UA scrollbar CSS (`CssPropertyWithConditions` in `ua_css.rs`,
///      evaluated via `@os` / `@theme` conditions)
///   2. CSS `-azul-scrollbar-style` (full `ScrollbarInfo` customisation)
///   3. CSS `scrollbar-width`  (overrides width only)
///   4. CSS `scrollbar-color`  (overrides thumb / track colours)
///   5. CSS `-azul-scrollbar-visibility` (overrides visibility + clip)
///   6. CSS `-azul-scrollbar-fade-delay` (overrides fade delay)
///   7. CSS `-azul-scrollbar-fade-duration` (overrides fade duration)
///
/// When `system_style` is `None`, falls back to the unconditional UA rule
/// (classic light scrollbar).
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
#[must_use] pub fn get_scrollbar_style(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
    system_style: Option<&azul_css::system::SystemStyle>,
) -> ComputedScrollbarStyle {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    // Step 1: Evaluate UA scrollbar CSS using the DynamicSelector system.
    let ctx = system_style.map_or_else(
        azul_css::dynamic_selector::DynamicSelectorContext::default,
        azul_css::dynamic_selector::DynamicSelectorContext::from_system_style,
    );
    let ua = azul_core::ua_css::evaluate_ua_scrollbar_css(&ctx);
    let result = ComputedScrollbarStyle::from_ua_resolved(&ua);

    // FAST PATH: 99% of nodes have no scrollbar CSS. Bail before walking 8 × cascade.
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_scrollbar_css(node_id.index()) {
                return result;
            }
        }
    }
    let mut result = result;

    // Step 2: Check individual scrollbar part backgrounds
    if let Some(track) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_track(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.track_color = extract_color_from_background(track);
    }
    if let Some(thumb) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_thumb(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.thumb_color = extract_color_from_background(thumb);
    }
    if let Some(button) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_button(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.button_color = extract_color_from_background(button);
    }
    if let Some(corner) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_corner(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.corner_color = extract_color_from_background(corner);
    }

    // Step 3: Check for scrollbar-width (overrides width only, not overlay)
    if let Some(scrollbar_width) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_width(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.width_mode = *scrollbar_width;
        let w = match scrollbar_width {
            LayoutScrollbarWidth::Auto => SCROLLBAR_WIDTH_AUTO,
            LayoutScrollbarWidth::Thin => SCROLLBAR_WIDTH_THIN,
            LayoutScrollbarWidth::None => 0.0,
        };
        result.visual_width_px = w;
        if result.visibility != ScrollbarVisibilityMode::WhenScrolling {
            result.reserve_width_px = w;
        }
    }

    // Step 4: Check for scrollbar-color (overrides thumb/track colors)
    if let Some(scrollbar_color) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_color(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        match scrollbar_color {
            StyleScrollbarColor::Auto => { /* keep */ }
            StyleScrollbarColor::Custom(custom) => {
                result.thumb_color = custom.thumb;
                result.track_color = custom.track;
            }
        }
    }

    // Step 5: Check for -azul-scrollbar-visibility
    if let Some(vis) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_visibility(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.visibility = *vis;
        result.clip_to_container_border = *vis == ScrollbarVisibilityMode::WhenScrolling;
        // Overlay mode: no reserved layout space, hide buttons and corner
        let is_overlay = *vis == ScrollbarVisibilityMode::WhenScrolling;
        if is_overlay {
            result.reserve_width_px = 0.0;
            result.show_scroll_buttons = false;
            result.scroll_button_size_px = 0.0;
            result.show_corner_rect = false;
        } else {
            result.reserve_width_px = result.visual_width_px;
        }
    }

    // Step 6: Check for -azul-scrollbar-fade-delay
    if let Some(delay) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_fade_delay(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.fade_delay_ms = delay.ms;
    }

    // Step 7: Check for -azul-scrollbar-fade-duration
    if let Some(dur) = styled_dom
        .css_property_cache
        .ptr
        .get_scrollbar_fade_duration(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
    {
        result.fade_duration_ms = dur.ms;
    }

    result
}

/// Cached wrapper for [`get_scrollbar_style`] that reuses the
/// memo stored on `LayoutContext`.
///
/// The underlying call performs
/// 9 cascade walks per node (track/thumb/button/corner/width/
/// color/visibility/fade-delay/fade-duration). The BFC, Taffy,
/// and display-list callers all hit the same node many times
/// inside a single layout pass, so caching turns ~21 rebuilds per
/// node into one.
///
/// Falls back to the uncached `get_scrollbar_style` when no ctx
/// is available (shouldn't happen in the current code paths).
pub fn get_scrollbar_style_cached<T: ParsedFontTrait>(
    ctx: &crate::solver3::LayoutContext<'_, T>,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> ComputedScrollbarStyle {
    if let Some(s) = ctx.scrollbar_style_cache.borrow().get(&node_id) {
        return s.clone();
    }
    let style = get_scrollbar_style(
        ctx.styled_dom,
        node_id,
        node_state,
        ctx.system_style.as_deref(),
    );
    ctx.scrollbar_style_cache
        .borrow_mut()
        .insert(node_id, style.clone());
    style
}

/// Helper to extract a solid color from a `StyleBackgroundContent`
const fn extract_color_from_background(
    bg: &azul_css::props::style::background::StyleBackgroundContent,
) -> ColorU {
    use azul_css::props::style::background::StyleBackgroundContent;
    match bg {
        StyleBackgroundContent::Color(c) => *c,
        _ => ColorU::TRANSPARENT,
    }
}

/// Check if a node should clip its scrollbar to the container's border-radius
#[must_use] pub fn should_clip_scrollbar_to_border(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> bool {
    let style = get_scrollbar_style(styled_dom, node_id, node_state, None);
    style.clip_to_container_border
}

/// Get the scrollbar visual width in pixels for a node (used for rendering)
#[must_use] pub fn get_scrollbar_width_px(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let style = get_scrollbar_style(styled_dom, node_id, node_state, None);
    style.visual_width_px
}

/// Checks if text in a node is selectable based on CSS `user-select` property.
///
/// Returns `true` if the text can be selected (default behavior),
/// `false` if `user-select: none` is set.
#[must_use] pub fn is_text_selectable(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> bool {
    let node_data = &styled_dom.node_data.as_container()[node_id];

    styled_dom
        .css_property_cache
        .ptr
        .get_user_select(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .is_none_or(|us| *us != StyleUserSelect::None) // Default: text is selectable
}

/// Checks if a node has the `contenteditable` attribute set directly.
///
/// Returns `true` if:
/// - The node has `contenteditable: true` set via `.set_contenteditable(true)`
/// - OR the node has `contenteditable` attribute set to `true`
///
/// This does NOT check inheritance - use `is_node_contenteditable_inherited` for that.
#[must_use] pub fn is_node_contenteditable(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    use azul_core::dom::AttributeType;

    let node_data = &styled_dom.node_data.as_container()[node_id];

    // First check the direct contenteditable field (primary method)
    if node_data.is_contenteditable() {
        return true;
    }

    // Also check the attribute for backwards compatibility
    // Only return true if the attribute value is explicitly true
    node_data
        .attributes()
        .as_ref()
        .iter()
        .any(|attr| matches!(attr, AttributeType::ContentEditable(true)))
}
// =============================================================================
// Additional ExtractPropertyValue impls (not in compact cache tier 1/2)
// =============================================================================

use azul_css::props::layout::table::{
    LayoutTableLayout, StyleBorderCollapse, StyleCaptionSide, StyleEmptyCells,
};
use azul_css::props::layout::text::LayoutTextJustify;
use azul_css::props::style::effects::StyleAspectRatio;
use azul_css::props::style::effects::StyleCursor;
use azul_css::props::style::effects::StyleObjectFit;
use azul_css::props::style::effects::StyleObjectPosition;
use azul_css::props::style::effects::StyleTextOrientation;
use azul_css::props::style::text::StyleHyphens;
use azul_css::props::style::text::StyleLineBreak;
use azul_css::props::style::text::StyleOverflowWrap;
use azul_css::props::style::text::StyleTextAlignLast;
use azul_css::props::style::text::StyleWordBreak;

impl ExtractPropertyValue<LayoutTextJustify> for CssProperty {
    fn extract(&self) -> Option<LayoutTextJustify> {
        match self {
            Self::TextJustify(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleHyphens> for CssProperty {
    fn extract(&self) -> Option<StyleHyphens> {
        match self {
            Self::Hyphens(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleWordBreak> for CssProperty {
    fn extract(&self) -> Option<StyleWordBreak> {
        match self {
            Self::WordBreak(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleOverflowWrap> for CssProperty {
    fn extract(&self) -> Option<StyleOverflowWrap> {
        match self {
            Self::OverflowWrap(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleLineBreak> for CssProperty {
    fn extract(&self) -> Option<StyleLineBreak> {
        match self {
            Self::LineBreak(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleTextAlignLast> for CssProperty {
    fn extract(&self) -> Option<StyleTextAlignLast> {
        match self {
            Self::TextAlignLast(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleObjectFit> for CssProperty {
    fn extract(&self) -> Option<StyleObjectFit> {
        match self {
            Self::ObjectFit(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleTextOrientation> for CssProperty {
    fn extract(&self) -> Option<StyleTextOrientation> {
        match self {
            Self::TextOrientation(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleObjectPosition> for CssProperty {
    fn extract(&self) -> Option<StyleObjectPosition> {
        match self {
            Self::ObjectPosition(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleAspectRatio> for CssProperty {
    fn extract(&self) -> Option<StyleAspectRatio> {
        match self {
            Self::AspectRatio(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<LayoutTableLayout> for CssProperty {
    fn extract(&self) -> Option<LayoutTableLayout> {
        match self {
            Self::TableLayout(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleBorderCollapse> for CssProperty {
    fn extract(&self) -> Option<StyleBorderCollapse> {
        match self {
            Self::BorderCollapse(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleCaptionSide> for CssProperty {
    fn extract(&self) -> Option<StyleCaptionSide> {
        match self {
            Self::CaptionSide(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleEmptyCells> for CssProperty {
    fn extract(&self) -> Option<StyleEmptyCells> {
        match self {
            Self::EmptyCells(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

impl ExtractPropertyValue<StyleCursor> for CssProperty {
    fn extract(&self) -> Option<StyleCursor> {
        match self {
            Self::Cursor(CssPropertyValue::Exact(v)) => Some(*v),
            _ => None,
        }
    }
}

// =============================================================================
// Additional macro-based getters (not covered by compact cache fast-path getters)
// =============================================================================

get_css_property!(
    get_text_justify,
    get_text_justify,
    LayoutTextJustify,
    CssPropertyType::TextJustify
);

get_css_property!(
    get_hyphens,
    get_hyphens,
    StyleHyphens,
    CssPropertyType::Hyphens
);

get_css_property!(
    get_word_break,
    get_word_break,
    StyleWordBreak,
    CssPropertyType::WordBreak
);

get_css_property!(
    get_overflow_wrap,
    get_overflow_wrap,
    StyleOverflowWrap,
    CssPropertyType::OverflowWrap
);

get_css_property!(
    get_line_break,
    get_line_break,
    StyleLineBreak,
    CssPropertyType::LineBreak
);

get_css_property!(
    get_text_align_last,
    get_text_align_last,
    StyleTextAlignLast,
    CssPropertyType::TextAlignLast
);

get_css_property!(
    get_table_layout,
    get_table_layout,
    LayoutTableLayout,
    CssPropertyType::TableLayout
);

get_css_property!(
    get_border_collapse,
    get_border_collapse,
    StyleBorderCollapse,
    CssPropertyType::BorderCollapse,
    compact = get_border_collapse
);

get_css_property!(
    get_caption_side,
    get_caption_side,
    StyleCaptionSide,
    CssPropertyType::CaptionSide
);

get_css_property!(
    get_empty_cells,
    get_empty_cells,
    StyleEmptyCells,
    CssPropertyType::EmptyCells
);

get_css_property!(
    get_cursor_property,
    get_cursor,
    StyleCursor,
    CssPropertyType::Cursor
);

// =============================================================================
// Handwritten getters (Option<T>, special logic, or non-standard returns)
// =============================================================================

/// Get height property value for IFC text layout height reference.
#[must_use] pub fn get_height_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<LayoutHeight> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get shape-inside property. Returns Option<ShapeInside> (cloned).
#[must_use] pub fn get_shape_inside(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::shape::ShapeInside> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_shape_inside(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get shape-outside property. Returns Option<ShapeOutside> (cloned).
#[must_use] pub fn get_shape_outside(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::shape::ShapeOutside> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_shape_outside(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get line-height as the full `StyleLineHeight` value for caller resolution.
#[must_use] pub fn get_line_height_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleLineHeight> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_line_height(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get text-indent as the full `StyleTextIndent` value for caller resolution.
#[must_use] pub fn get_text_indent_value(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleTextIndent> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_text_indent(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get column-count property. Returns Option<ColumnCount>.
#[must_use] pub fn get_column_count(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::column::ColumnCount> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_column_count(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get initial-letter property. Returns Option<StyleInitialLetter>.
#[must_use] pub fn get_initial_letter(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleInitialLetter> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_initial_letter(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get line-clamp property. Returns Option<StyleLineClamp>.
#[must_use] pub fn get_line_clamp(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleLineClamp> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_line_clamp(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get hanging-punctuation property. Returns Option<StyleHangingPunctuation>.
#[must_use] pub fn get_hanging_punctuation(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleHangingPunctuation> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_hanging_punctuation(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get text-combine-upright property. Returns Option<StyleTextCombineUpright>.
#[must_use] pub fn get_text_combine_upright(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::text::StyleTextCombineUpright> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_text_combine_upright(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
}

/// Get exclusion-margin value. Returns f32 (default 0.0).
#[must_use] pub fn get_exclusion_margin(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> f32 {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_exclusion_margin(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map_or(0.0, |v| v.inner.get())
}

/// Get hyphenation-language property. Returns Option<StyleHyphenationLanguage>.
#[must_use] pub fn get_hyphenation_language(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::exclusion::StyleHyphenationLanguage> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_hyphenation_language(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get border-spacing property.
#[must_use] pub fn get_border_spacing(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> azul_css::props::layout::table::LayoutBorderSpacing {
    use azul_css::props::basic::pixel::PixelValue;

    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let h_raw = cc.get_border_spacing_h_raw(node_id.index());
            let v_raw = cc.get_border_spacing_v_raw(node_id.index());
            // Both 0 means no border-spacing set (default)
            // Sentinel means non-px unit → slow path
            if h_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
                && v_raw < azul_css::compact_cache::I16_SENTINEL_THRESHOLD
            {
                return azul_css::props::layout::table::LayoutBorderSpacing {
                    horizontal: PixelValue::px(f32::from(h_raw) / 10.0),
                    vertical: PixelValue::px(f32::from(v_raw) / 10.0),
                };
            }
        }
    }

    // SLOW PATH
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_border_spacing(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .copied()
        .unwrap_or_default()
}

/// Get opacity value. Returns f32 (default 1.0).
///
/// GPU fast path: the compact cache encodes opacity as a u8 (0-254, 255 = unset).
/// Avoids the 4-pseudo-state × 6-layer cascade walk for animations reading opacity
/// across every node each frame.
#[must_use] pub fn get_opacity(styled_dom: &StyledDom, node_id: NodeId, node_state: &StyledNodeState) -> f32 {
    // FAST PATH: compact cache for normal state
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            let raw = cc.get_opacity_raw(node_id.index());
            if raw == azul_css::compact_cache::OPACITY_SENTINEL {
                return 1.0;
            }
            return f32::from(raw) / 254.0;
        }
    }
    // SLOW PATH: fall back to cascade walk (state != normal, or no compact cache)
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_opacity(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map_or(1.0, |v| v.inner.normalized())
}

/// Get filter property. Returns Option with cloned filter list.
#[must_use] pub fn get_filter(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::filter::StyleFilterVec> {
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_filter(node_id.index()) {
                return None;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_filter(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get backdrop-filter property. Returns Option with cloned filter list.
#[must_use] pub fn get_backdrop_filter(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::filter::StyleFilterVec> {
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_backdrop_filter(node_id.index()) {
                return None;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_backdrop_filter(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Compact-cache negative fast path for all 4 box-shadow sides.
/// Most nodes have no shadow; cheap to check one bit vs. 4 cascade walks.
#[inline]
fn box_shadow_fast_bail(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> bool {
    if !node_state.is_normal() {
        return false;
    }
    if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
        return !cc.has_box_shadow(node_id.index());
    }
    false
}

/// Get box-shadow for left side. Returns Option<StyleBoxShadow> (cloned).
#[must_use] pub fn get_box_shadow_left(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    if box_shadow_fast_bail(styled_dom, node_id, node_state) {
        return None;
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_box_shadow_left(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| (**v))
}

/// Get box-shadow for right side. Returns Option<StyleBoxShadow> (cloned).
#[must_use] pub fn get_box_shadow_right(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    if box_shadow_fast_bail(styled_dom, node_id, node_state) {
        return None;
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_box_shadow_right(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| (**v))
}

/// Get box-shadow for top side. Returns Option<StyleBoxShadow> (cloned).
#[must_use] pub fn get_box_shadow_top(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    if box_shadow_fast_bail(styled_dom, node_id, node_state) {
        return None;
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_box_shadow_top(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| (**v))
}

/// Get box-shadow for bottom side. Returns Option<StyleBoxShadow> (cloned).
#[must_use] pub fn get_box_shadow_bottom(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    if box_shadow_fast_bail(styled_dom, node_id, node_state) {
        return None;
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_box_shadow_bottom(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| (**v))
}

/// Get text-shadow property. Returns Option<StyleBoxShadow> (cloned).
#[must_use] pub fn get_text_shadow(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::box_shadow::StyleBoxShadow> {
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_text_shadow(node_id.index()) {
                return None;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_text_shadow(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .map(|v| (**v))
}

/// Get transform property. Returns Option (non-empty transform list, cloned).
///
/// GPU fast path: the compact cache keeps a `has_transform` flag. If unset,
/// skips the cascade walk entirely — which is the overwhelming case since most
/// nodes have no transform. Only nodes that actually have a transform pay the
/// slow-walk cost to retrieve the parsed value.
#[must_use] pub fn get_transform(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::transform::StyleTransformVec> {
    // FAST PATH: bit check in compact cache
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_transform(node_id.index()) {
                return None;
            }
            // has_transform set → fall through to cascade walk for the value
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_transform(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get counter-reset property. Returns Option<CounterReset> (cloned).
#[must_use] pub fn get_counter_reset(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::content::CounterReset> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_counter_reset(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// Get counter-increment property. Returns Option<CounterIncrement> (cloned).
#[must_use] pub fn get_counter_increment(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::style::content::CounterIncrement> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_counter_increment(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

/// W3C-conformant contenteditable inheritance check.
///
/// In the W3C model, the `contenteditable` attribute is **inherited**:
/// - A node is editable if it has `contenteditable="true"` set directly
/// - OR if its parent has `isContentEditable` as true
/// - UNLESS the node explicitly sets `contenteditable="false"`
///
/// This function traverses up the DOM tree to determine editability.
///
/// # Returns
///
/// - `true` if the node is editable (either directly or via inheritance)
/// - `false` if the node is not editable or has `contenteditable="false"`
///
/// # Example
///
/// ```html
/// <div contenteditable="true">
///   A                              <!-- editable (inherited) -->
///   <div contenteditable="false">
///     B                            <!-- NOT editable (explicitly false) -->
///   </div>
///   C                              <!-- editable (inherited) -->
/// </div>
/// ```
#[must_use] pub fn is_node_contenteditable_inherited(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    use azul_core::dom::AttributeType;

    let node_data_container = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();

    let mut current_node_id = Some(node_id);

    while let Some(nid) = current_node_id {
        let node_data = &node_data_container[nid];

        // First check the direct contenteditable field (set via set_contenteditable())
        // This takes precedence as it's the API-level setting
        if node_data.is_contenteditable() {
            return true;
        }

        // Then check for explicit contenteditable attribute on this node
        // This handles HTML-style contenteditable="true" or contenteditable="false"
        for attr in node_data.attributes().as_ref() {
            if let AttributeType::ContentEditable(is_editable) = attr {
                // If explicitly set to true, node is editable
                // If explicitly set to false, node is NOT editable (blocks inheritance)
                return *is_editable;
            }
        }

        // No explicit setting on this node, check parent for inheritance
        current_node_id = hierarchy.get(nid).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    }

    // Reached root without finding contenteditable - not editable
    false
}

/// Find the contenteditable ancestor of a node.
///
/// When focus lands on a text node inside a contenteditable container,
/// we need to find the actual container that has the `contenteditable` attribute.
///
/// # Returns
///
/// - `Some(node_id)` of the contenteditable ancestor (may be the node itself)
/// - `None` if no contenteditable ancestor exists
#[must_use] pub fn find_contenteditable_ancestor(styled_dom: &StyledDom, node_id: NodeId) -> Option<NodeId> {
    use azul_core::dom::AttributeType;

    let node_data_container = styled_dom.node_data.as_container();
    let hierarchy = styled_dom.node_hierarchy.as_container();

    let mut current_node_id = Some(node_id);

    while let Some(nid) = current_node_id {
        let node_data = &node_data_container[nid];

        // First check the direct contenteditable field (set via set_contenteditable())
        if node_data.is_contenteditable() {
            return Some(nid);
        }

        // Then check for contenteditable attribute on this node
        for attr in node_data.attributes().as_ref() {
            if let AttributeType::ContentEditable(is_editable) = attr {
                if *is_editable {
                    return Some(nid);
                }
                // Explicitly not editable - stop search
                return None;
            }
        }

        // Check parent
        current_node_id = hierarchy.get(nid).and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
    }

    None
}

// --- Taffy bridge property getters ---
//
// These getters return `Option<CssPropertyValue<T>>` (cloned from cache) for use
// by taffy_bridge.rs. The conversion from CssPropertyValue to taffy types is done
// in taffy_bridge.rs itself. Routing access through these functions centralizes
// all CSS property lookups for future cache optimizations (e.g., FxHash migration).

macro_rules! get_css_property_value {
    ($fn_name:ident, $cache_method:ident, $ret_type:ty) => {
        #[must_use] pub fn $fn_name(
            styled_dom: &StyledDom,
            node_id: NodeId,
            node_state: &StyledNodeState,
        ) -> Option<$ret_type> {
            let node_data = &styled_dom.node_data.as_container()[node_id];
            styled_dom
                .css_property_cache
                .ptr
                .$cache_method(node_data, &node_id, node_state)
                .cloned()
        }
    };
}

// Flexbox properties
get_css_property_value!(
    get_flex_direction_prop,
    get_flex_direction,
    LayoutFlexDirectionValue
);
get_css_property_value!(get_flex_wrap_prop, get_flex_wrap, LayoutFlexWrapValue);
get_css_property_value!(get_flex_grow_prop, get_flex_grow, LayoutFlexGrowValue);
get_css_property_value!(get_flex_shrink_prop, get_flex_shrink, LayoutFlexShrinkValue);
get_css_property_value!(get_flex_basis_prop, get_flex_basis, LayoutFlexBasisValue);

// Alignment properties
get_css_property_value!(get_align_items_prop, get_align_items, LayoutAlignItemsValue);
get_css_property_value!(get_align_self_prop, get_align_self, LayoutAlignSelfValue);
get_css_property_value!(
    get_align_content_prop,
    get_align_content,
    LayoutAlignContentValue
);
get_css_property_value!(
    get_justify_content_prop,
    get_justify_content,
    LayoutJustifyContentValue
);
get_css_property_value!(
    get_justify_items_prop,
    get_justify_items,
    LayoutJustifyItemsValue
);
get_css_property_value!(
    get_justify_self_prop,
    get_justify_self,
    LayoutJustifySelfValue
);

// Gap
get_css_property_value!(get_gap_prop, get_gap, LayoutGapValue);

// Grid properties
get_css_property_value!(
    get_grid_template_rows_prop,
    get_grid_template_rows,
    LayoutGridTemplateRowsValue
);
get_css_property_value!(
    get_grid_template_columns_prop,
    get_grid_template_columns,
    LayoutGridTemplateColumnsValue
);
get_css_property_value!(
    get_grid_auto_rows_prop,
    get_grid_auto_rows,
    LayoutGridAutoRowsValue
);
get_css_property_value!(
    get_grid_auto_columns_prop,
    get_grid_auto_columns,
    LayoutGridAutoColumnsValue
);
get_css_property_value!(
    get_grid_auto_flow_prop,
    get_grid_auto_flow,
    LayoutGridAutoFlowValue
);
get_css_property_value!(get_grid_column_prop, get_grid_column, LayoutGridColumnValue);
get_css_property_value!(get_grid_row_prop, get_grid_row, LayoutGridRowValue);

/// Get grid-template-areas property.
///
/// Uses the generic `get_property()` since `CssPropertyCache` lacks a specific getter.
/// Returns the inner `GridTemplateAreas` value (already unwrapped from `CssPropertyValue`).
#[must_use] pub fn get_grid_template_areas_prop(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<GridTemplateAreas> {
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_property(
            node_data,
            &node_id,
            node_state,
            &CssPropertyType::GridTemplateAreas,
        )
        .and_then(|p| {
            if let CssProperty::GridTemplateAreas(v) = p {
                v.get_property().cloned()
            } else {
                None
            }
        })
}

/// Get clip-path property. Returns the `ClipPath` value for the node.
///
/// CSS Masking Module Level 1, section 3:
/// The clip-path property creates a clipping region that determines which parts
/// of an element are visible. Returns None for `clip-path: none` (default).
#[must_use] pub fn get_clip_path(
    styled_dom: &StyledDom,
    node_id: NodeId,
    node_state: &StyledNodeState,
) -> Option<azul_css::props::layout::shape::ClipPath> {
    // Negative fast path: most nodes have `clip-path: none`.
    if node_state.is_normal() {
        if let Some(ref cc) = styled_dom.css_property_cache.ptr.compact_cache {
            if !cc.has_clip_path(node_id.index()) {
                return None;
            }
        }
    }
    let node_data = &styled_dom.node_data.as_container()[node_id];
    styled_dom
        .css_property_cache
        .ptr
        .get_clip_path(node_data, &node_id, node_state)
        .and_then(|v| v.get_property())
        .cloned()
}

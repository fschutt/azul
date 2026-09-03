//! Generates a renderer-agnostic display list from a laid-out tree.
//!
//! This module is the bridge between the layout solver and the compositor/renderer.
//! Key types:
//! - [`DisplayList`] — flat, paint-order-sorted list of drawing commands
//! - [`DisplayListItem`] — a single drawing primitive or state-management command
//! - [`DisplayListBuilder`] — internal builder that accumulates items during generation
//!
//! Entry points:
//! - [`generate_display_list`] — converts a laid-out [`LayoutTree`] into a [`DisplayList`]
//! - [`paginate_display_list_with_slicer_and_breaks`] — slices a display list into pages
//!
//! Coordinates are in **absolute window-logical pixels** ([`WindowLogicalRect`]).
//! `HiDPI` scaling and scroll-offset conversion happen in the compositor.

use crate::solver3::layout_tree::LayoutNodeId;
use std::{
    collections::{BTreeMap, HashMap},
    sync::Arc,
};

use azul_core::{
    dom::{DomId, FormattingContext, NodeId, NodeType, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::GpuValueCache,
    hit_test::{CursorType, ScrollPosition, TAG_TYPE_CURSOR, TAG_TYPE_DOM_NODE},
    resources::{IdNamespace, ImageRef, OpacityKey, RendererResources, TransformKey},
    selection::{Selection, SelectionRange, TextSelection},
    styled_dom::StyledDom,
    transform::ComputedTransform3D,
    ui_solver::GlyphInstance,
};
use azul_css::{
    codegen::format::GetHash,
    css::CssPropertyValue,
    props::{
        basic::{ColorU, FontRef, PixelValue},
        layout::{LayoutDisplay, LayoutOverflow, LayoutPosition},
        property::{CssProperty, CssPropertyType},
        style::{
            background::{ConicGradient, ExtendMode, LinearGradient, RadialGradient},
            border_radius::StyleBorderRadius,
            box_shadow::{BoxShadowClipMode, StyleBoxShadow},
            filter::{StyleFilter, StyleFilterVec},
            BorderStyle, LayoutBorderBottomWidth, LayoutBorderLeftWidth, LayoutBorderRightWidth,
            LayoutBorderTopWidth, StyleBorderBottomColor, StyleBorderBottomStyle,
            StyleBorderLeftColor, StyleBorderLeftStyle, StyleBorderRightColor,
            StyleBorderRightStyle, StyleBorderTopColor, StyleBorderTopStyle,
        },
    },
    LayoutDebugMessage,
};

#[cfg(feature = "text_layout")]
use crate::text3;
#[cfg(feature = "text_layout")]
use crate::text3::cache::{InlineShape, PositionedItem};
use crate::{
    debug_info,
    font_traits::{
        FontHash, FontLoaderTrait, ImageSource, InlineContent, ParsedFontTrait, ShapedItem,
        UnifiedLayout,
    },
    solver3::{
        getters::{
            get_background_color, get_background_contents, get_border_info, get_border_radius,
            get_break_after, get_break_before, get_caret_style, get_overflow_clip_margin_property,
            get_overflow_x, get_overflow_y, get_scrollbar_gutter_property,
            get_scrollbar_info_from_layout, get_scrollbar_style, get_selection_style,
            get_style_border_radius, get_visibility, get_z_index, is_forced_page_break, BorderInfo,
            CaretStyle, ComputedScrollbarStyle, SelectionStyle,
        },
        layout_tree::{LayoutNode, LayoutNodeHot, LayoutNodeWarm, LayoutTree},
        positioning::get_position_type,
        scrollbar::{compute_scrollbar_geometry_with_button_size, ScrollbarRequirements},
        LayoutContext, LayoutError, Result,
    },
};

const APPROX_ASCENT_RATIO: f32 = 0.8;
const APPROX_UNDERLINE_THICKNESS_RATIO: f32 = 0.08;
const APPROX_UNDERLINE_OFFSET_RATIO: f32 = 0.12;
const APPROX_STRIKETHROUGH_OFFSET_RATIO: f32 = 0.3;
const APPROX_OVERLINE_OFFSET_RATIO: f32 = 0.85;
const APPROX_ELLIPSIS_WIDTH_RATIO: f32 = 0.6;
const DEFAULT_A4_WIDTH_PT: f32 = 595.0;
const DEFAULT_SHADOW_FONT_SIZE_PX: f32 = 16.0;

/// Border widths for all four sides.
///
/// Each field is optional to allow partial border specifications.
/// Used in [`DisplayListItem::Border`] to specify per-side border widths.
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderWidths {
    /// Top border width (CSS `border-top-width`)
    pub top: Option<CssPropertyValue<LayoutBorderTopWidth>>,
    /// Right border width (CSS `border-right-width`)
    pub right: Option<CssPropertyValue<LayoutBorderRightWidth>>,
    /// Bottom border width (CSS `border-bottom-width`)
    pub bottom: Option<CssPropertyValue<LayoutBorderBottomWidth>>,
    /// Left border width (CSS `border-left-width`)
    pub left: Option<CssPropertyValue<LayoutBorderLeftWidth>>,
}

/// Border colors for all four sides.
///
/// Each field is optional to allow partial border specifications.
/// Used in [`DisplayListItem::Border`] to specify per-side border colors.
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderColors {
    /// Top border color (CSS `border-top-color`)
    pub top: Option<CssPropertyValue<StyleBorderTopColor>>,
    /// Right border color (CSS `border-right-color`)
    pub right: Option<CssPropertyValue<StyleBorderRightColor>>,
    /// Bottom border color (CSS `border-bottom-color`)
    pub bottom: Option<CssPropertyValue<StyleBorderBottomColor>>,
    /// Left border color (CSS `border-left-color`)
    pub left: Option<CssPropertyValue<StyleBorderLeftColor>>,
}

/// Border styles for all four sides.
///
/// Each field is optional to allow partial border specifications.
/// Used in [`DisplayListItem::Border`] to specify per-side border styles
/// (solid, dashed, dotted, none, etc.).
#[derive(Debug, Clone, Copy)]
pub struct StyleBorderStyles {
    /// Top border style (CSS `border-top-style`)
    pub top: Option<CssPropertyValue<StyleBorderTopStyle>>,
    /// Right border style (CSS `border-right-style`)
    pub right: Option<CssPropertyValue<StyleBorderRightStyle>>,
    /// Bottom border style (CSS `border-bottom-style`)
    pub bottom: Option<CssPropertyValue<StyleBorderBottomStyle>>,
    /// Left border style (CSS `border-left-style`)
    pub left: Option<CssPropertyValue<StyleBorderLeftStyle>>,
}

/// A rectangle in border-box coordinates (includes padding and border).
/// This is what layout calculates and stores in `used_size` and absolute positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderBoxRect(pub LogicalRect);

/// A `LogicalRect` known to be in **absolute window coordinates** (as output
/// by the layout engine).
///
/// All spatial bounds stored in [`DisplayListItem`] use
/// this type so that the compositor is *forced* to convert them to
/// frame-relative coordinates before passing them to `WebRender`.
///
/// ## Coordinate-space contract
///
/// * **Layout engine** produces `WindowLogicalRect` values.
/// * **Compositor** converts via `resolve_rect()` → `WebRender` `LayoutRect`.
/// * Passing a `WindowLogicalRect` directly to a `WebRender` push function is a
///   **type error** (it wraps `LogicalRect`, not `LayoutRect`).
///
#[derive(Debug, Copy, Clone, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct WindowLogicalRect(pub LogicalRect);

/// Anything at or below this is the unassigned-position sentinel, not geometry.
///
/// The sentinel is exactly `f32::MIN`; the threshold is deliberately a little
/// looser so a sentinel that has been through one offset addition (which barely
/// moves a value of that magnitude) is still recognised.
const UNASSIGNED_POSITION_LIMIT: f32 = f32::MIN / 2.0;

impl WindowLogicalRect {
    #[inline]
    #[must_use]
    pub const fn new(origin: LogicalPosition, size: LogicalSize) -> Self {
        Self(LogicalRect::new(origin, size))
    }

    #[inline]
    #[must_use]
    pub const fn zero() -> Self {
        Self(LogicalRect::zero())
    }

    /// Access the inner `LogicalRect` (still in window space – the caller is
    /// responsible for applying any offset conversion).
    #[inline]
    #[must_use]
    pub const fn inner(&self) -> &LogicalRect {
        &self.0
    }

    #[inline]
    #[must_use]
    pub const fn into_inner(self) -> LogicalRect {
        self.0
    }

    // Convenience accessors
    #[inline]
    #[must_use]
    pub const fn origin(&self) -> LogicalPosition {
        self.0.origin
    }
    #[inline]
    #[must_use]
    pub const fn size(&self) -> LogicalSize {
        self.0.size
    }
}

impl From<LogicalRect> for WindowLogicalRect {
    #[inline]
    fn from(r: LogicalRect) -> Self {
        Self(r)
    }
}

impl From<WindowLogicalRect> for LogicalRect {
    #[inline]
    fn from(w: WindowLogicalRect) -> Self {
        w.0
    }
}

/// Simple struct for passing element dimensions to border-radius calculation
#[derive(Debug, Clone, Copy)]
pub struct PhysicalSizeImport {
    pub width: f32,
    pub height: f32,
}

/// Complete drawing information for a scrollbar with all visual components.
///
/// This contains the resolved geometry and colors for all scrollbar parts:
/// - Track: The background area where the thumb slides
/// - Thumb: The draggable indicator showing current scroll position
/// - Buttons: Optional up/down or left/right arrow buttons
/// - Corner: The area where horizontal and vertical scrollbars meet
#[derive(Debug, Clone, PartialEq)]
pub struct ScrollbarDrawInfo {
    /// Overall bounds of the entire scrollbar (including track and buttons)
    pub bounds: WindowLogicalRect,
    /// Scrollbar orientation (horizontal or vertical)
    pub orientation: ScrollbarOrientation,

    // Track area (the background rail)
    /// Bounds of the track area
    pub track_bounds: WindowLogicalRect,
    /// Color of the track background
    pub track_color: ColorU,

    // Thumb (the draggable part)
    /// Bounds of the thumb
    pub thumb_bounds: WindowLogicalRect,
    /// Color of the thumb
    pub thumb_color: ColorU,
    /// Border radius for rounded thumb corners
    pub thumb_border_radius: BorderRadius,

    // Optional buttons (arrows at ends)
    /// Optional decrement button bounds (up/left arrow)
    pub button_decrement_bounds: Option<WindowLogicalRect>,
    /// Optional increment button bounds (down/right arrow)
    pub button_increment_bounds: Option<WindowLogicalRect>,
    /// Color for buttons
    pub button_color: ColorU,

    /// Optional opacity key for GPU-side fading animation.
    pub opacity_key: Option<OpacityKey>,
    /// Optional transform key for GPU-side scrollbar thumb positioning.
    /// When present, the compositor will wrap the thumb in a `PushReferenceFrame`
    /// with `PropertyBinding::Binding` so `WebRender` can animate the thumb position
    /// without rebuilding the display list.
    pub thumb_transform_key: Option<TransformKey>,
    /// Initial transform for the scrollbar thumb (current scroll position).
    /// This is the transform applied when the display list is first built.
    /// During GPU-only scroll, `synchronize_gpu_values` updates this dynamically.
    pub thumb_initial_transform: ComputedTransform3D,
    /// Optional hit-test ID for `WebRender` hit-testing.
    pub hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    /// Whether to clip scrollbar to container's border-radius
    pub clip_to_container_border: bool,
    /// Container's border-radius (for clipping)
    pub container_border_radius: BorderRadius,
    /// Scrollbar visibility mode — used by back-registration to choose initial opacity.
    /// `Always` → initial opacity 1.0; `WhenScrolling` → initial opacity 0.0.
    pub visibility: azul_css::props::style::scrollbar::ScrollbarVisibilityMode,
}

impl BorderBoxRect {
    /// Convert border-box to content-box by subtracting padding and border.
    /// Content-box is where inline layout and text actually render.
    #[must_use]
    pub fn to_content_box(
        self,
        padding: &crate::solver3::geometry::EdgeSizes,
        border: &crate::solver3::geometry::EdgeSizes,
    ) -> ContentBoxRect {
        ContentBoxRect(LogicalRect {
            origin: LogicalPosition {
                x: self.0.origin.x + padding.left + border.left,
                y: self.0.origin.y + padding.top + border.top,
            },
            size: LogicalSize {
                width: self.0.size.width
                    - padding.left
                    - padding.right
                    - border.left
                    - border.right,
                height: self.0.size.height
                    - padding.top
                    - padding.bottom
                    - border.top
                    - border.bottom,
            },
        })
    }

    /// Convert border-box to padding-box by subtracting only the border.
    ///
    /// THE SCROLLPORT. CSS Overflow 3 §2: "scrolling occurs within the padding
    /// box", and azul measures overflow content against the padding box too
    /// (`compute_scrollbar_geometry`, `register_scroll_nodes`), so the clamp
    /// range `max_scroll = content − scrollport` is only self-consistent if
    /// every consumer takes ORIGIN AND SIZE off this box.
    #[must_use]
    pub fn to_padding_box(self, border: &crate::solver3::geometry::EdgeSizes) -> PaddingBoxRect {
        PaddingBoxRect(LogicalRect {
            origin: LogicalPosition {
                x: self.0.origin.x + border.left,
                y: self.0.origin.y + border.top,
            },
            size: LogicalSize {
                width: self.0.size.width - border.left - border.right,
                height: self.0.size.height - border.top - border.bottom,
            },
        })
    }

    /// Get the inner `LogicalRect`
    #[must_use]
    pub const fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// A rectangle in padding-box coordinates (excludes border, includes padding).
///
/// This is the CSS **scrollport**: the box a scroll container's content is
/// clipped to and scrolls within.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaddingBoxRect(pub LogicalRect);

impl PaddingBoxRect {
    /// Get the inner `LogicalRect`
    #[must_use]
    pub const fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// A rectangle in content-box coordinates (excludes padding and border).
/// This is where text and inline content is positioned by the inline formatter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentBoxRect(pub LogicalRect);

impl ContentBoxRect {
    /// Get the inner `LogicalRect`
    #[must_use]
    pub const fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// The final, renderer-agnostic output of the layout engine.
///
/// This is a flat list of drawing and state-management commands, already sorted
/// according to the CSS paint order. A renderer can consume this list directly.
/// A CSS-forced page break recorded while building the display list.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ForcedBreak {
    /// Absolute document-space Y of the break.
    pub y: f32,
    /// The node whose `break-before`/`break-after` property forced it.
    /// `None` only for synthetic inputs (tests, hand-built lists).
    pub causing_node: Option<NodeId>,
}

#[derive(Debug, Default, Clone)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
    /// Optional mapping from item index to the DOM `NodeId` that generated it.
    /// Used for pagination to look up CSS break properties.
    /// Not all items have a source node (e.g., synthesized decorations).
    pub node_mapping: Vec<Option<NodeId>>,
    /// Forced page breaks (from break-before/break-after: always), with the
    /// node whose break property caused each one. Y coordinates are absolute
    /// in the infinite canvas coordinate system; the slicer aligns page
    /// boundaries with these positions.
    pub forced_page_breaks: Vec<ForcedBreak>,
    /// Index ranges (start, end) of display list items that belong to fixed-position elements.
    /// In paged media, these items are replicated on every page (CSS Positioned Layout §2.1).
    pub fixed_position_item_ranges: Vec<(usize, usize)>,
    /// Per-item PROVEN uniform background (Text items only, None elsewhere).
    /// Parallel to `items` like `node_mapping`. Lives on the LIST rather
    /// than in the `Text` variant because printpdf (crates.io, immutable)
    /// pattern-matches the variant exhaustively — a new variant field is a
    /// downstream semver break; a new list field is not. See
    /// `compute_uniform_text_bg` for what "proven" means and the raster's
    /// pre-blended tile path for what it buys (~6 ms/repaint of per-pixel
    /// linear LCD compositing).
    pub uniform_text_bgs: Vec<Option<(ColorU, WindowLogicalRect)>>,
    /// Per-item (layout-tree index, emission phase) — the DL-PATCHING key.
    /// `node_mapping` attributes items to DOM ids, but patching substitutes
    /// per LAYOUT-NODE PAINT CALL: on a resize-skip pass the tree object and
    /// its indices are unchanged, so a node's cached items can replace a
    /// paint call — IF the run for the bg/border call can be told apart from
    /// the run for the content call and from items the SC walk itself pushes
    /// (scroll-frame hit areas, clips, scrollbars — those always re-emit
    /// fresh). Parallel to `items`, like `node_mapping`.
    pub layout_node_mapping: Vec<Option<(usize, EmitPhase)>>,
}

impl DisplayList {
    /// Insert one item at `at`, keeping EVERY parallel array and index range
    /// consistent.
    ///
    /// `items` has four vectors parallel to it (`node_mapping`,
    /// `uniform_text_bgs`, `layout_node_mapping`) plus
    /// `fixed_position_item_ranges`, which stores INDEX RANGES into `items`.
    /// A bare `items.insert(at, x)` therefore misaligns the per-item side
    /// tables from `at` onwards — the raster then reads another item's proven
    /// background, and the patch key attributes an item to the wrong layout
    /// node — and silently moves every fixed-position range off its items.
    /// Appending at the END is the only insertion that gets away with it, which
    /// is why this was not needed until the focus ring had to go INSIDE a
    /// scroll frame.
    ///
    /// `at` is clamped to the end, so an index computed from a stale view of
    /// the list cannot panic here.
    pub fn insert_item(&mut self, at: usize, item: DisplayListItem, node: Option<NodeId>) {
        let at = at.min(self.items.len());
        self.items.insert(at, item);
        // The side tables are allowed to be SHORTER than `items` (older code
        // appended without extending them), so grow before indexing.
        Self::insert_parallel(&mut self.node_mapping, at, node);
        Self::insert_parallel(&mut self.uniform_text_bgs, at, None);
        Self::insert_parallel(&mut self.layout_node_mapping, at, None);
        for (start, end) in &mut self.fixed_position_item_ranges {
            // A range that CONTAINS the insertion point grows; one entirely
            // after it slides. `end` is exclusive, so `at == *end` is "after".
            if *start >= at {
                *start += 1;
            }
            if *end > at {
                *end += 1;
            }
        }
    }

    /// Insert into a side table that may be shorter than `items`, padding with
    /// the default first so the entry lands at the right index.
    fn insert_parallel<T: Clone + Default>(v: &mut Vec<T>, at: usize, value: T) {
        if v.len() < at {
            v.resize(at, T::default());
        }
        v.insert(at, value);
    }

    /// Approximate heap bytes retained by this display list — the
    /// item vec + parallel vecs + every item's owned heap. The memory
    /// report used to charge a FLAT 2 KiB for a cached display list;
    /// on a paginated document the `Text` items alone hold an offset
    /// COPY of every painted glyph (20 B each), which made the single
    /// biggest DL cost invisible ("a zero is not a measurement" — and
    /// so is a constant).
    ///
    /// Returns `(bytes, text_instances, item_count)` — the counters that
    /// explain the byte figure (glyph copies vs item slots), same pattern
    /// as the cluster counters on the tree report.
    #[must_use]
    pub fn retained_bytes(&self) -> (usize, usize, usize) {
        use core::mem::size_of;
        let mut bytes = self.items.capacity() * size_of::<DisplayListItem>()
            + self.node_mapping.capacity() * size_of::<Option<NodeId>>()
            + self.forced_page_breaks.capacity() * size_of::<ForcedBreak>()
            + self.fixed_position_item_ranges.capacity() * size_of::<(usize, usize)>()
            + self.uniform_text_bgs.capacity() * size_of::<Option<(ColorU, WindowLogicalRect)>>()
            + self.layout_node_mapping.capacity() * size_of::<Option<(usize, EmitPhase)>>();
        let mut text_instances = 0usize;
        for item in &self.items {
            bytes += Self::item_heap_bytes(item, &mut text_instances);
        }
        (bytes, text_instances, self.items.len())
    }

    /// Owned heap hanging off one item. EXHAUSTIVE on purpose — a new
    /// variant must fail this match so its heap gets classified rather
    /// than silently uncounted. `Arc`/`ImageRef` fields are CLONES of
    /// allocations owned (and counted) elsewhere → 0 here.
    const fn item_heap_bytes(item: &DisplayListItem, text_instances: &mut usize) -> usize {
        use core::mem::size_of;
        use DisplayListItem as I;
        match item {
            I::Text { glyphs, .. } => {
                *text_instances += glyphs.len();
                glyphs.capacity() * size_of::<GlyphInstance>()
            }
            I::ScrollBarStyled { .. } => size_of::<ScrollbarDrawInfo>(),
            // The rings the stroke walks. Bounded by the path, not the
            // document, and cloned from the DOM's own geometry.
            I::StrokedPath { .. } => 0,
            I::LinearGradient { gradient, .. } => {
                gradient.stops.len()
                    * size_of::<azul_css::props::style::NormalizedLinearColorStop>()
            }
            I::RadialGradient { gradient, .. } => {
                gradient.stops.len()
                    * size_of::<azul_css::props::style::NormalizedLinearColorStop>()
            }
            I::ConicGradient { gradient, .. } => {
                gradient.stops.len()
                    * size_of::<azul_css::props::style::NormalizedRadialColorStop>()
            }
            I::PushFilter { filters, .. } | I::PushBackdropFilter { filters, .. } => {
                filters.capacity() * size_of::<StyleFilter>()
            }
            // Arc / ImageRef clones — owned and counted elsewhere.
            I::TextLayout { .. } | I::Image { .. } | I::PushImageMaskClip { .. } => 0,
            // Flat payloads: everything lives inline in the enum variant,
            // already covered by `items.capacity() * size_of::<DisplayListItem>()`.
            I::Rect { .. }
            | I::SelectionRect { .. }
            | I::CursorRect { .. }
            | I::Border { .. }
            | I::Underline { .. }
            | I::Strikethrough { .. }
            | I::Overline { .. }
            | I::ScrollBar { .. }
            | I::VirtualView { .. }
            | I::VirtualViewPlaceholder { .. }
            | I::PushClip { .. }
            | I::PopClip
            | I::PopImageMaskClip
            | I::PushScrollFrame { .. }
            | I::PopScrollFrame
            | I::PushStackingContext { .. }
            | I::PopStackingContext
            | I::PushReferenceFrame { .. }
            | I::PopReferenceFrame
            | I::HitTestArea { .. }
            | I::BoxShadow { .. }
            | I::PopFilter
            | I::PopBackdropFilter
            | I::PushOpacity { .. }
            | I::PopOpacity
            | I::PushTextShadow { .. }
            | I::PopTextShadow => 0,
        }
    }
}

/// Translate a display-list item's geometry by `delta` (DL patching:
/// same item content, moved node). Every variant the two paint fns can
/// emit is handled; variants outside `patchable_item` never reach this
/// (`PatchState::build` forces their node into the re-emit set).
#[must_use]
pub(crate) fn translate_item(mut item: DisplayListItem, delta: LogicalPosition) -> DisplayListItem {
    use DisplayListItem as I;
    if delta.x == 0.0 && delta.y == 0.0 {
        return item;
    }
    let shift = |b: &mut WindowLogicalRect| {
        b.0.origin.x += delta.x;
        b.0.origin.y += delta.y;
    };
    match &mut item {
        I::Rect { bounds, .. }
        | I::SelectionRect { bounds, .. }
        | I::CursorRect { bounds, .. }
        | I::Border { bounds, .. }
        | I::TextLayout { bounds, .. }
        | I::Underline { bounds, .. }
        | I::Strikethrough { bounds, .. }
        | I::Overline { bounds, .. }
        | I::Image { bounds, .. }
        | I::LinearGradient { bounds, .. }
        | I::RadialGradient { bounds, .. }
        | I::ConicGradient { bounds, .. }
        | I::BoxShadow { bounds, .. }
        | I::HitTestArea { bounds, .. } => shift(bounds),
        I::Text {
            glyphs, clip_rect, ..
        } => {
            for g in glyphs.iter_mut() {
                g.point.x += delta.x;
                g.point.y += delta.y;
            }
            shift(clip_rect);
        }
        // No geometry of their own.
        I::PushTextShadow { .. } | I::PopTextShadow => {}
        // Anything else is untranslatable by contract (see patchable_item).
        _ => {}
    }
    item
}

/// Whether an item may appear in a PATCHED (copied+translated) run. The
/// build step forces nodes emitting anything else into the re-emit set —
/// scroll frames, clips, stacking contexts, virtual views, filters and
/// scrollbars all carry state beyond a rect and must re-emit fresh.
#[must_use]
pub(crate) const fn patchable_item(item: &DisplayListItem) -> bool {
    use DisplayListItem as I;
    matches!(
        item,
        I::Rect { .. }
            | I::SelectionRect { .. }
            | I::CursorRect { .. }
            | I::Border { .. }
            | I::TextLayout { .. }
            | I::Text { .. }
            | I::Underline { .. }
            | I::Strikethrough { .. }
            | I::Overline { .. }
            | I::Image { .. }
            | I::LinearGradient { .. }
            | I::RadialGradient { .. }
            | I::ConicGradient { .. }
            | I::BoxShadow { .. }
            | I::HitTestArea { .. }
            | I::PushTextShadow { .. }
            | I::PopTextShadow
    )
}

/// Runtime toggle for DL patching. 0 = uninitialized (resolve from
/// `AZ_NO_DL_PATCH` on first read), 1 = enabled, 2 = disabled. A runtime
/// atomic rather than a latched `OnceLock` so tests can A/B the patched and
/// full paths in one process (the golden-equality gate), and so a live
/// session can be flipped for diagnosis.
static DL_PATCHING: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

#[must_use]
pub fn dl_patching_enabled() -> bool {
    use core::sync::atomic::Ordering;
    match DL_PATCHING.load(Ordering::Relaxed) {
        1 => true,
        2 => false,
        _ => {
            let on = std::env::var_os("AZ_NO_DL_PATCH").is_none();
            DL_PATCHING.store(if on { 1 } else { 2 }, Ordering::Relaxed);
            on
        }
    }
}

/// Force DL patching on/off (tests + live diagnosis; overrides the env).
pub fn set_dl_patching_enabled(on: bool) {
    use core::sync::atomic::Ordering;
    DL_PATCHING.store(if on { 1 } else { 2 }, Ordering::Relaxed);
}

/// Which emission site produced a display-list item. See
/// [`DisplayList::layout_node_mapping`].
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum EmitPhase {
    /// Pushed by the stacking-context walk itself (clips, scroll frames,
    /// scrollable hit areas, scrollbars) — never substituted from cache.
    ScWalk,
    /// Pushed by `paint_node_background_and_border`.
    BgBorder,
    /// Pushed by `paint_node_content`.
    Content,
}

impl DisplayList {
    /// THE display-list ↔ DOM identity invariant, checkable after every
    /// build or patch.
    ///
    /// `node_mapping` is how thirty-plus consumers (damage attribution,
    /// pagination break lookup, hit-testing, the coming display-list PATCHING)
    /// resolve an item back to the DOM. A stale or out-of-range id does not
    /// fail — it succeeds at describing the WRONG node, which is the silent
    /// corruption class `assert_dom_ids_are_in_range` documents on the tree
    /// side (a ribbon tab click once repainted rects of unrelated nodes; it
    /// was only caught because an index happened to trip a bounds check).
    /// This is the missing display-list-side counterpart, and the hard gate
    /// the DL-patching work patches AGAINST: every patch must leave this
    /// invariant intact.
    ///
    /// Checks: (1) `node_mapping` covers `items` 1:1; (2) every mapped id
    /// indexes into `styled_dom`; (3) every item's bounds, where it has any,
    /// is finite (a NaN origin poisons every damage union it touches).
    /// Deliberately NOT checked: bounds-within-node-rect — decorations
    /// (shadows, outlines, selection) legitimately paint outside their node.
    ///
    /// # Errors
    /// Returns a message naming the first offending item index.
    pub fn validate_node_mapping(
        &self,
        styled_dom: &StyledDom,
    ) -> core::result::Result<(), String> {
        if self.node_mapping.len() != self.items.len() {
            return Err(format!(
                "node_mapping covers {} items but the list has {}",
                self.node_mapping.len(),
                self.items.len()
            ));
        }
        let dom_len = styled_dom.node_data.as_ref().len();
        for (i, (item, mapping)) in self.items.iter().zip(self.node_mapping.iter()).enumerate() {
            if let Some(node_id) = mapping {
                if node_id.index() >= dom_len {
                    return Err(format!(
                        "item {i} maps to NodeId {} but the DOM has {dom_len} nodes",
                        node_id.index()
                    ));
                }
            }
            if let Some(b) = item.bounds() {
                if !b.origin.x.is_finite()
                    || !b.origin.y.is_finite()
                    || !b.size.width.is_finite()
                    || !b.size.height.is_finite()
                {
                    return Err(format!("item {i} has non-finite bounds {b:?}"));
                }
            }
        }
        Ok(())
    }

    /// Patch text glyph data for a specific layout node without rebuilding
    /// the entire display list. Returns the damage rect covering all
    /// affected text items, or None if no matching items found.
    ///
    /// Used for `GlyphSwap` incremental relayout: glyphs changed but
    /// positions are identical, so only the glyph IDs need updating.
    pub(crate) fn patch_text_glyphs(
        &mut self,
        node_index: usize,
        new_glyphs_by_run: &[Vec<GlyphInstance>],
    ) -> Option<LogicalRect> {
        let mut run_idx = 0;
        let mut damage: Option<LogicalRect> = None;

        for item in &mut self.items {
            if let DisplayListItem::Text {
                ref mut glyphs,
                ref clip_rect,
                source_node_index: Some(src_idx),
                ..
            } = item
            {
                if *src_idx == node_index && run_idx < new_glyphs_by_run.len() {
                    glyphs.clone_from(&new_glyphs_by_run[run_idx]);
                    let bounds = *clip_rect.inner();
                    damage = Some(damage.map_or(bounds, |d| {
                        // rect union (was crate::cpurender::union_rect, which
                        // is gated behind the `cpurender` feature; inlined here
                        // so display-list damage tracking works without it / on WASM)
                        let x = d.origin.x.min(bounds.origin.x);
                        let y = d.origin.y.min(bounds.origin.y);
                        let right =
                            (d.origin.x + d.size.width).max(bounds.origin.x + bounds.size.width);
                        let bottom =
                            (d.origin.y + d.size.height).max(bounds.origin.y + bounds.size.height);
                        LogicalRect {
                            origin: LogicalPosition { x, y },
                            size: LogicalSize {
                                width: right - x,
                                height: bottom - y,
                            },
                        }
                    }));
                    run_idx += 1;
                }
            }
        }

        damage
    }

    /// Rewrite the `ImageRef` of every `Image` item generated by `node`
    /// without rebuilding the display list, returning the union of the
    /// patched bounds (None if the node produced no image items).
    ///
    /// The single caller is the content chokepoint
    /// (`LayoutWindow::apply_content_change`, paint tier): geometry is
    /// unchanged, only the content identity moves — the backend display-list
    /// diff sees the `ImageRef` id change and damages exactly these bounds.
    /// Patch PAINT COLOURS in place for a node's SUBTREE — the per-tick fast
    /// path of a paint-only `animation` transition. Items attributed (via
    /// `node_mapping`) to a node id in `range` whose colour equals `from`
    /// take `to`; the from-match is what implements inheritance without a
    /// cascade pass (a descendant with its OWN explicit colour differs from
    /// `from` and is untouched). Returns the damage union, `None` when
    /// nothing matched (the caller falls back to a full rebuild).
    pub fn patch_paint_colors(
        &mut self,
        range: core::ops::Range<usize>,
        from: ColorU,
        to: ColorU,
        patch_text: bool,
        patch_background: bool,
    ) -> Option<LogicalRect> {
        let mut damage: Option<LogicalRect> = None;
        let mut grow = |bounds: LogicalRect| {
            damage = Some(damage.map_or(bounds, |d| {
                let x = d.origin.x.min(bounds.origin.x);
                let y = d.origin.y.min(bounds.origin.y);
                let right = (d.origin.x + d.size.width).max(bounds.origin.x + bounds.size.width);
                let bottom = (d.origin.y + d.size.height).max(bounds.origin.y + bounds.size.height);
                LogicalRect {
                    origin: LogicalPosition { x, y },
                    size: LogicalSize {
                        width: right - x,
                        height: bottom - y,
                    },
                }
            }));
        };
        for (idx, item) in self.items.iter_mut().enumerate() {
            let Some(node) = self.node_mapping.get(idx).copied().flatten() else {
                continue;
            };
            if !range.contains(&node.index()) {
                continue;
            }
            match item {
                DisplayListItem::Text {
                    color, clip_rect, ..
                } if patch_text && *color == from => {
                    *color = to;
                    grow(*clip_rect.inner());
                }
                DisplayListItem::Underline { color, bounds, .. }
                | DisplayListItem::Strikethrough { color, bounds, .. }
                    if patch_text && *color == from =>
                {
                    *color = to;
                    grow(*bounds.inner());
                }
                DisplayListItem::Rect { color, bounds, .. }
                    if patch_background && *color == from =>
                {
                    *color = to;
                    grow(*bounds.inner());
                }
                _ => {}
            }
        }
        damage
    }

    pub fn patch_node_image(&mut self, node: NodeId, image: &ImageRef) -> Option<LogicalRect> {
        let mut damage: Option<LogicalRect> = None;

        for (idx, item) in self.items.iter_mut().enumerate() {
            if self.node_mapping.get(idx).copied().flatten() != Some(node) {
                continue;
            }
            if let DisplayListItem::Image {
                bounds,
                image: item_image,
                ..
            } = item
            {
                *item_image = image.clone();
                let bounds = *bounds.inner();
                damage = Some(damage.map_or(bounds, |d| {
                    let x = d.origin.x.min(bounds.origin.x);
                    let y = d.origin.y.min(bounds.origin.y);
                    let right =
                        (d.origin.x + d.size.width).max(bounds.origin.x + bounds.size.width);
                    let bottom =
                        (d.origin.y + d.size.height).max(bounds.origin.y + bounds.size.height);
                    LogicalRect {
                        origin: LogicalPosition { x, y },
                        size: LogicalSize {
                            width: right - x,
                            height: bottom - y,
                        },
                    }
                }));
            }
        }

        damage
    }

    /// Compute a damage rect from the difference between old and new text
    /// layout results, starting from a given line index.
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    pub(crate) fn compute_text_damage_rect(
        old_items: &[PositionedItem],
        new_items: &[PositionedItem],
        container_origin: LogicalPosition,
        affected_line: usize,
    ) -> LogicalRect {
        let expand = |items: &[PositionedItem]| -> (f32, f32, f32, f32) {
            let mut lx = f32::MAX;
            let mut ly = f32::MAX;
            let mut rx = f32::MIN;
            let mut ry = f32::MIN;
            for item in items {
                if item.line_index >= affected_line {
                    let bounds = item.item.bounds();
                    let x = container_origin.x + item.position.x;
                    let y = container_origin.y + item.position.y;
                    lx = lx.min(x);
                    ly = ly.min(y);
                    rx = rx.max(x + bounds.width);
                    ry = ry.max(y + bounds.height);
                }
            }
            (lx, ly, rx, ry)
        };

        let (olx, oly, orx, ory) = expand(old_items);
        let (nlx, nly, nrx, nry) = expand(new_items);
        let min_x = olx.min(nlx);
        let min_y = oly.min(nly);
        let max_x = orx.max(nrx);
        let max_y = ory.max(nry);

        if min_x > max_x || min_y > max_y {
            return LogicalRect::default();
        }

        LogicalRect {
            origin: LogicalPosition { x: min_x, y: min_y },
            size: LogicalSize {
                width: max_x - min_x,
                height: max_y - min_y,
            },
        }
    }

    /// Generates a JSON representation of the display list for debugging.
    /// This includes clip chain analysis showing how clips are stacked.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    pub fn to_debug_json(&self) -> String {
        use std::fmt::Write;
        let mut json = String::new();
        writeln!(json, "{{").unwrap();
        writeln!(json, "  \"total_items\": {},", self.items.len()).unwrap();
        writeln!(json, "  \"items\": [").unwrap();

        let mut clip_depth = 0i32;
        let mut scroll_depth = 0i32;
        let mut stacking_depth = 0i32;

        for (i, item) in self.items.iter().enumerate() {
            let comma = if i < self.items.len() - 1 { "," } else { "" };
            let node_id = self.node_mapping.get(i).and_then(|n| *n);

            match item {
                DisplayListItem::PushClip {
                    bounds,
                    border_radius,
                } => {
                    clip_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"PushClip\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},", 
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(json, "      \"border_radius\": {{ \"tl\": {:.1}, \"tr\": {:.1}, \"bl\": {:.1}, \"br\": {:.1} }},",
                        border_radius.top_left, border_radius.top_right,
                        border_radius.bottom_left, border_radius.bottom_right).unwrap();
                    writeln!(json, "      \"node_id\": {node_id:?}").unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                DisplayListItem::PopClip => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"PopClip\",").unwrap();
                    writeln!(json, "      \"clip_depth_before\": {clip_depth},").unwrap();
                    writeln!(json, "      \"clip_depth_after\": {}", clip_depth - 1).unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                    clip_depth -= 1;
                }
                DisplayListItem::PushScrollFrame {
                    clip_bounds,
                    content_size,
                    scroll_id,
                } => {
                    scroll_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"PushScrollFrame\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"clip_bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        clip_bounds.0.origin.x, clip_bounds.0.origin.y,
                        clip_bounds.0.size.width, clip_bounds.0.size.height).unwrap();
                    writeln!(
                        json,
                        "      \"content_size\": {{ \"w\": {:.1}, \"h\": {:.1} }},",
                        content_size.width, content_size.height
                    )
                    .unwrap();
                    writeln!(json, "      \"scroll_id\": {scroll_id},").unwrap();
                    writeln!(json, "      \"node_id\": {node_id:?}").unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                DisplayListItem::PopScrollFrame => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"PopScrollFrame\",").unwrap();
                    writeln!(json, "      \"scroll_depth_before\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth_after\": {}", scroll_depth - 1).unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                    scroll_depth -= 1;
                }
                DisplayListItem::PushStackingContext { z_index, bounds } => {
                    stacking_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"PushStackingContext\",").unwrap();
                    writeln!(json, "      \"stacking_depth\": {stacking_depth},").unwrap();
                    writeln!(json, "      \"z_index\": {z_index},").unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }}",
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                DisplayListItem::PopStackingContext => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"PopStackingContext\",").unwrap();
                    writeln!(json, "      \"stacking_depth_before\": {stacking_depth},").unwrap();
                    writeln!(
                        json,
                        "      \"stacking_depth_after\": {}",
                        stacking_depth - 1
                    )
                    .unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                    stacking_depth -= 1;
                }
                DisplayListItem::Rect {
                    bounds,
                    color,
                    border_radius,
                } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"Rect\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(
                        json,
                        "      \"color\": \"rgba({},{},{},{})\",",
                        color.r, color.g, color.b, color.a
                    )
                    .unwrap();
                    writeln!(json, "      \"node_id\": {node_id:?}").unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                DisplayListItem::Border { bounds, .. } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"Border\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(json, "      \"node_id\": {node_id:?}").unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                DisplayListItem::ScrollBarStyled { info } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"ScrollBarStyled\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"orientation\": \"{:?}\",", info.orientation).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }}",
                        info.bounds.0.origin.x, info.bounds.0.origin.y,
                        info.bounds.0.size.width, info.bounds.0.size.height).unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                DisplayListItem::Text {
                    glyphs,
                    font_size_px,
                    clip_rect,
                    source_node_index,
                    ..
                } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(json, "      \"type\": \"Text\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"glyphs\": {},", glyphs.len()).unwrap();
                    writeln!(json, "      \"font_size_px\": {font_size_px},").unwrap();
                    writeln!(json, "      \"clip_rect\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        clip_rect.0.origin.x, clip_rect.0.origin.y,
                        clip_rect.0.size.width, clip_rect.0.size.height).unwrap();
                    writeln!(json, "      \"source_node_index\": {source_node_index:?},").unwrap();
                    writeln!(json, "      \"node_id\": {node_id:?}").unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
                _ => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {i},").unwrap();
                    writeln!(
                        json,
                        "      \"type\": \"{:?}\",",
                        std::mem::discriminant(item)
                    )
                    .unwrap();
                    writeln!(json, "      \"clip_depth\": {clip_depth},").unwrap();
                    writeln!(json, "      \"scroll_depth\": {scroll_depth},").unwrap();
                    writeln!(json, "      \"node_id\": {node_id:?}").unwrap();
                    writeln!(json, "    }}{comma}").unwrap();
                }
            }
        }

        writeln!(json, "  ],").unwrap();
        writeln!(json, "  \"final_clip_depth\": {clip_depth},").unwrap();
        writeln!(json, "  \"final_scroll_depth\": {scroll_depth},").unwrap();
        writeln!(json, "  \"final_stacking_depth\": {stacking_depth},").unwrap();
        writeln!(
            json,
            "  \"balanced\": {}",
            clip_depth == 0 && scroll_depth == 0 && stacking_depth == 0
        )
        .unwrap();
        writeln!(json, "}}").unwrap();

        json
    }
}

/// A command in the display list. Can be either a drawing primitive or a
/// state-management instruction for the renderer's graphics context.
#[derive(Debug, Clone)]
pub enum DisplayListItem {
    // Drawing Primitives
    /// A filled rectangle with optional rounded corners.
    /// Used for backgrounds, colored boxes, and other solid fills.
    Rect {
        /// The rectangle bounds in absolute window coordinates
        bounds: WindowLogicalRect,
        /// The fill color (RGBA)
        color: ColorU,
        /// Corner radii for rounded rectangles
        border_radius: BorderRadius,
    },
    /// A selection highlight rectangle (e.g., for text selection).
    /// Rendered behind text to show selected regions.
    SelectionRect {
        /// The rectangle bounds in absolute window coordinates
        bounds: WindowLogicalRect,
        /// Corner radii for rounded selection
        border_radius: BorderRadius,
        /// The selection highlight color (typically semi-transparent)
        color: ColorU,
    },
    /// A text cursor (caret) rectangle.
    /// Typically a thin vertical line indicating text insertion point.
    CursorRect {
        /// The cursor bounds (usually narrow width)
        bounds: WindowLogicalRect,
        /// The cursor color
        color: ColorU,
    },
    /// A CSS border with per-side widths, colors, and styles.
    /// Supports different styles per side (solid, dashed, dotted, etc.).
    Border {
        /// The border-box bounds
        bounds: WindowLogicalRect,
        /// Border widths for each side
        widths: StyleBorderWidths,
        /// Border colors for each side
        colors: StyleBorderColors,
        /// Border styles for each side (solid, dashed, etc.)
        styles: StyleBorderStyles,
        /// Corner radii for rounded borders
        border_radius: StyleBorderRadius,
    },
    /// Text layout with full metadata (for PDF, accessibility, etc.)
    /// This is pushed BEFORE the individual Text items and contains
    /// the original text, glyph-to-unicode mapping, and positioning info
    TextLayout {
        layout: Arc<dyn std::any::Any + Send + Sync>, // Type-erased UnifiedLayout
        bounds: WindowLogicalRect,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
    },
    /// Text rendered with individual glyph positioning (for simple renderers)
    Text {
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
        clip_rect: WindowLogicalRect,
        /// Layout node index that produced this text run.
        /// Enables patching glyphs without full display list regeneration.
        source_node_index: Option<usize>,
    },
    /// Underline decoration for text (CSS text-decoration: underline)
    Underline {
        bounds: WindowLogicalRect,
        color: ColorU,
        thickness: f32,
    },
    /// Strikethrough decoration for text (CSS text-decoration: line-through)
    Strikethrough {
        bounds: WindowLogicalRect,
        color: ColorU,
        thickness: f32,
    },
    /// Overline decoration for text (CSS text-decoration: overline)
    Overline {
        bounds: WindowLogicalRect,
        color: ColorU,
        thickness: f32,
    },
    Image {
        bounds: WindowLogicalRect,
        image: ImageRef,
        border_radius: BorderRadius,
    },
    /// A dedicated primitive for a scrollbar with optional GPU-animated opacity.
    /// This is a simple single-color scrollbar used for basic rendering.
    ScrollBar {
        bounds: WindowLogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        /// Optional opacity key for GPU-side fading animation.
        /// If present, the renderer will use this key to look up dynamic opacity.
        /// If None, the alpha channel of `color` is used directly.
        opacity_key: Option<OpacityKey>,
        /// Optional hit-test ID for `WebRender` hit-testing.
        /// If present, allows event handlers to identify which scrollbar component was clicked.
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    },
    /// A fully styled scrollbar with separate track, thumb, and optional buttons.
    /// Used when CSS scrollbar properties are specified.
    ScrollBarStyled {
        /// Complete drawing information for all scrollbar components
        info: Box<ScrollbarDrawInfo>,
    },

    /// An embedded `VirtualView` that references a child DOM with its own display list.
    /// The renderer will look up the child display list by `child_dom_id` and
    /// render it within the bounds. The `VirtualView` viewport is rendered in parent
    /// coordinate space (NOT inside a scroll frame) so it stays stationary.
    /// Scroll offset is communicated to the `VirtualView` callback, not via `WebRender`.
    VirtualView {
        /// The `DomId` of the child DOM (similar to webrender's `pipeline_id`)
        child_dom_id: DomId,
        /// The bounds where the `VirtualView` should be rendered
        bounds: WindowLogicalRect,
        /// The clip rect for the `VirtualView` content
        clip_rect: WindowLogicalRect,
        /// How far to shift the child content inside `bounds`, in logical px:
        /// `materialized_window_origin - current_scroll_offset`.
        ///
        /// This is what makes a `VirtualView` SCROLL rather than merely
        /// re-materialize. It is deliberately derived from the materialized
        /// window's origin and the live scroll offset ONLY — never from the
        /// virtual document size — so refining the document estimate (as
        /// background pagination lands) resizes the scrollbar and moves
        /// nothing on screen.
        content_offset: LogicalPosition,
    },

    /// Placeholder emitted during display list generation for `VirtualView` nodes.
    /// `window.rs` replaces this with a real `VirtualView` item after invoking
    /// the `VirtualView` callback. This avoids the need for post-hoc scroll frame
    /// scanning — `window.rs` simply finds the placeholder by `node_id`.
    ///
    /// Unlike regular scrollable nodes, `VirtualView` nodes do NOT get a
    /// PushScrollFrame/PopScrollFrame pair. Scroll state is managed by
    /// `ScrollManager` and passed to the `VirtualView` callback as `scroll_offset`.
    VirtualViewPlaceholder {
        /// The DOM `NodeId` of the `VirtualView` element in the parent DOM
        node_id: NodeId,
        /// The layout bounds of the `VirtualView` container
        bounds: WindowLogicalRect,
        /// The clip rect (same as bounds initially, may be adjusted)
        clip_rect: WindowLogicalRect,
    },

    // --- State-Management Commands ---
    /// Pushes a new clipping rectangle onto the renderer's clip stack.
    /// All subsequent primitives will be clipped by this rect until a `PopClip`.
    PushClip {
        bounds: WindowLogicalRect,
        border_radius: BorderRadius,
    },
    /// Pops the current clip from the renderer's clip stack.
    PopClip,

    /// Pushes an image-based clip mask onto the renderer's clip stack.
    /// The mask image should be R8 format: white (255) = visible, black (0) = clipped.
    /// All subsequent primitives will be masked until `PopImageMaskClip`.
    PushImageMaskClip {
        /// The bounds of the element being clipped
        bounds: WindowLogicalRect,
        /// The mask image (R8 format)
        mask_image: ImageRef,
        /// The rect within which the mask is applied
        mask_rect: WindowLogicalRect,
    },
    /// Pops the current image mask clip from the renderer's clip stack.
    PopImageMaskClip,

    /// A STROKED vector path - SVG's `stroke`, and the PDF model's second
    /// paint operator.
    ///
    /// Not a `Border`: a border follows the box, a stroke follows the
    /// GEOMETRY. Drawing a stroked outline as a rectangle inside the shape's
    /// own clip leaves a hairline at the corners and nothing anywhere else,
    /// which is what a line drawn with `fill="none"` looked like.
    ///
    /// The geometry is carried in USER SPACE with the `<svg>`'s `view_box`, so
    /// the renderer maps it into `bounds` itself - a stroke scaled by
    /// pre-transforming the points would scale the WIDTH with them, and a 2px
    /// rule in a 16-unit icon would come out 8px wide in a 64px slot.
    ///
    /// A CPU renderer strokes this directly; a GPU one rasterises it into a
    /// mask and paints `color` through it.
    StrokedPath {
        /// The box the geometry maps into, in absolute window coordinates.
        bounds: WindowLogicalRect,
        /// The path, in the user space `view_box` describes.
        path: azul_core::svg::SvgMultiPolygon,
        /// `(min_x, min_y, width, height)` of the enclosing `<svg>`, or `None`
        /// when the geometry is already in window-logical px.
        view_box: Option<(f32, f32, f32, f32)>,
        /// The stroke colour.
        color: ColorU,
        /// The stroke width, in the same user space as the geometry.
        width: f32,
        /// The stroke, pre-rasterised as an R8 coverage mask over `bounds`.
        ///
        /// For the GPU backend, which has no vector rasteriser: it paints
        /// `color` through this mask. The CPU backend IGNORES it and strokes
        /// the real outline, which has no resolution ceiling. `None` when the
        /// build has no rasteriser at all - the CPU path still draws, the GPU
        /// path draws nothing rather than something wrong.
        mask: Option<ImageRef>,
    },

    /// Defines a scrollable area. This is a specialized clip that also
    /// establishes a new coordinate system for its children, which can be offset.
    PushScrollFrame {
        /// The clip rect in the parent's coordinate space.
        clip_bounds: WindowLogicalRect,
        /// The total size of the scrollable content.
        content_size: LogicalSize,
        /// An ID for the renderer to track this scrollable area between frames.
        scroll_id: LocalScrollId,
    },
    /// Pops the current scroll frame.
    PopScrollFrame,

    /// Pushes a new stacking context for proper z-index layering.
    /// All subsequent primitives until `PopStackingContext` will be in this stacking context.
    PushStackingContext {
        /// The z-index for this stacking context (for debugging/validation)
        z_index: i32,
        /// The bounds of the stacking context root element
        bounds: WindowLogicalRect,
    },
    /// Pops the current stacking context.
    PopStackingContext,

    /// Pushes a reference frame with a GPU-accelerated transform.
    /// Used for CSS transforms and drag visual offsets.
    /// Creates a new spatial coordinate system for all children.
    PushReferenceFrame {
        /// The transform key for GPU-animated property binding
        transform_key: TransformKey,
        /// The initial transform value (identity for drag, computed for CSS transform)
        initial_transform: ComputedTransform3D,
        /// The bounds of the reference frame (origin = transform origin)
        bounds: WindowLogicalRect,
    },
    /// Pops the current reference frame.
    PopReferenceFrame,

    /// Defines a region for hit-testing.
    HitTestArea {
        bounds: WindowLogicalRect,
        tag: DisplayListTagId, // This would be a renderer-agnostic ID type
    },

    // --- Gradient Primitives ---
    /// A linear gradient fill.
    LinearGradient {
        bounds: WindowLogicalRect,
        gradient: LinearGradient,
        border_radius: BorderRadius,
    },
    /// A radial gradient fill.
    RadialGradient {
        bounds: WindowLogicalRect,
        gradient: RadialGradient,
        border_radius: BorderRadius,
    },
    /// A conic (angular) gradient fill.
    ConicGradient {
        bounds: WindowLogicalRect,
        gradient: ConicGradient,
        border_radius: BorderRadius,
    },

    // --- Shadow Effects ---
    /// A box shadow (either outset or inset).
    BoxShadow {
        bounds: WindowLogicalRect,
        shadow: StyleBoxShadow,
        border_radius: BorderRadius,
    },

    // --- Filter Effects ---
    /// Push a filter effect that applies to subsequent content.
    PushFilter {
        bounds: WindowLogicalRect,
        filters: Vec<StyleFilter>,
    },
    /// Pop a previously pushed filter.
    PopFilter,

    /// Push a backdrop filter (applies to content behind the element).
    PushBackdropFilter {
        bounds: WindowLogicalRect,
        filters: Vec<StyleFilter>,
    },
    /// Pop a previously pushed backdrop filter.
    PopBackdropFilter,

    /// Push an opacity layer.
    PushOpacity {
        bounds: WindowLogicalRect,
        /// The BAKED (CSS) opacity — the value at display-list build time.
        opacity: f32,
        /// GPU binding for ANIMATED opacity (enter/exit fades, diff-driven
        /// animation). Same contract as `PushReferenceFrame.transform_key`:
        /// the display list carries the KEY, the per-tick value lives in the
        /// `GpuValueCache` animation channel, so the cached list serves every
        /// tick while the fade advances. `None` for plain CSS opacity.
        opacity_key: Option<OpacityKey>,
    },
    /// Pop an opacity layer.
    PopOpacity,

    /// Push a text shadow that applies to subsequent text content.
    PushTextShadow { shadow: StyleBoxShadow },
    /// Pop all text shadows.
    PopTextShadow,
}

/// Intersect `a` with `b`; if they do not overlap, fall back to `b`.
///
/// Used to clamp a computed ink extent to its clip rect. The fallback is
/// deliberately the CLIP and not an empty rect: a non-overlapping result
/// means the extent was computed wrongly, and in that case damaging too
/// much is recoverable while damaging nothing leaves stale pixels.
/// The caret of an editable that has no text yet, in the node's CONTENT-BOX
/// space: at the origin, one line-height tall (`font-size × line-height`,
/// the `normal` 1.2 when unset), one pixel wide — the line box the first
/// character will create. Height is floored at 1 px so a zero font never
/// produces an invisible caret.
#[must_use]
pub fn empty_editable_caret_rect(font_size_px: f32, line_height: f32) -> LogicalRect {
    let height = (font_size_px * line_height).max(1.0);
    let height = if height.is_finite() { height } else { 1.0 };
    LogicalRect {
        origin: LogicalPosition::zero(),
        size: LogicalSize { width: 1.0, height },
    }
}

fn intersect_or(a: LogicalRect, b: LogicalRect) -> LogicalRect {
    let x0 = a.origin.x.max(b.origin.x);
    let y0 = a.origin.y.max(b.origin.y);
    let x1 = (a.origin.x + a.size.width).min(b.origin.x + b.size.width);
    let y1 = (a.origin.y + a.size.height).min(b.origin.y + b.size.height);
    if x1 > x0 && y1 > y0 {
        LogicalRect {
            origin: LogicalPosition { x: x0, y: y0 },
            size: LogicalSize {
                width: x1 - x0,
                height: y1 - y0,
            },
        }
    } else {
        b
    }
}

impl DisplayListItem {
    /// Compare two display list items for visual equality (same appearance when rendered).
    /// Used by damage computation to detect content changes within the same bounds.
    /// Conservative: returns `false` (assumes different) for complex types like Arc<dyn Any>.
    // Exact float equality is intentional: this is frame-to-frame damage detection,
    // so any bit-level change in a coordinate/color/thickness SHOULD force a redraw.
    // An epsilon comparison would wrongly skip sub-epsilon visual updates.
    #[allow(clippy::float_cmp)]
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    #[allow(clippy::match_same_arms)]
    // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    /// Is this a structure-opening marker (clip / scroll frame / stacking
    /// context / text shadow / image mask)? Paired with [`Self::is_pop_marker`]
    /// and [`Self::matching_pop`] by the page slicer's E17 re-derivation.
    #[must_use]
    pub const fn is_push_marker(&self) -> bool {
        matches!(
            self,
            Self::PushClip { .. }
                | Self::PushScrollFrame { .. }
                | Self::PushStackingContext { .. }
                | Self::PushTextShadow { .. }
                | Self::PushImageMaskClip { .. }
        )
    }

    /// Is this a structure-closing marker?
    #[must_use]
    pub const fn is_pop_marker(&self) -> bool {
        matches!(
            self,
            Self::PopClip
                | Self::PopScrollFrame
                | Self::PopStackingContext
                | Self::PopTextShadow
                | Self::PopImageMaskClip
        )
    }

    /// The Pop item that closes this Push marker (None for non-markers).
    #[must_use]
    pub const fn matching_pop(&self) -> Option<Self> {
        match self {
            Self::PushClip { .. } => Some(Self::PopClip),
            Self::PushScrollFrame { .. } => Some(Self::PopScrollFrame),
            Self::PushStackingContext { .. } => Some(Self::PopStackingContext),
            Self::PushTextShadow { .. } => Some(Self::PopTextShadow),
            Self::PushImageMaskClip { .. } => Some(Self::PopImageMaskClip),
            _ => None,
        }
    }

    #[must_use]
    pub fn is_visually_equal(&self, other: &Self) -> bool {
        if std::mem::discriminant(self) != std::mem::discriminant(other) {
            return false;
        }
        match (self, other) {
            (
                Self::Rect {
                    bounds: b1,
                    color: c1,
                    border_radius: br1,
                },
                Self::Rect {
                    bounds: b2,
                    color: c2,
                    border_radius: br2,
                },
            ) => {
                b1 == b2
                    && c1 == c2
                    && br1.top_left == br2.top_left
                    && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left
                    && br1.bottom_right == br2.bottom_right
            }
            (
                Self::SelectionRect {
                    bounds: b1,
                    border_radius: br1,
                    color: c1,
                },
                Self::SelectionRect {
                    bounds: b2,
                    border_radius: br2,
                    color: c2,
                },
            ) => {
                b1 == b2
                    && c1 == c2
                    && br1.top_left == br2.top_left
                    && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left
                    && br1.bottom_right == br2.bottom_right
            }
            (
                Self::CursorRect {
                    bounds: b1,
                    color: c1,
                },
                Self::CursorRect {
                    bounds: b2,
                    color: c2,
                },
            ) => b1 == b2 && c1 == c2,
            (
                Self::Text {
                    glyphs: g1,
                    font_hash: fh1,
                    font_size_px: fs1,
                    color: c1,
                    clip_rect: cr1,
                    ..
                },
                Self::Text {
                    glyphs: g2,
                    font_hash: fh2,
                    font_size_px: fs2,
                    color: c2,
                    clip_rect: cr2,
                    ..
                },
            ) => {
                cr1 == cr2
                    && c1 == c2
                    && fh1 == fh2
                    && fs1 == fs2
                    && g1.len() == g2.len()
                    && g1.iter().zip(g2.iter()).all(|(a, b)| {
                        a.index == b.index && a.point.x == b.point.x && a.point.y == b.point.y
                    })
            }
            (
                Self::Underline {
                    bounds: b1,
                    color: c1,
                    thickness: t1,
                },
                Self::Underline {
                    bounds: b2,
                    color: c2,
                    thickness: t2,
                },
            ) => b1 == b2 && c1 == c2 && t1 == t2,
            (
                Self::Strikethrough {
                    bounds: b1,
                    color: c1,
                    thickness: t1,
                },
                Self::Strikethrough {
                    bounds: b2,
                    color: c2,
                    thickness: t2,
                },
            ) => b1 == b2 && c1 == c2 && t1 == t2,
            (
                Self::Overline {
                    bounds: b1,
                    color: c1,
                    thickness: t1,
                },
                Self::Overline {
                    bounds: b2,
                    color: c2,
                    thickness: t2,
                },
            ) => b1 == b2 && c1 == c2 && t1 == t2,
            (
                Self::Border {
                    bounds: b1,
                    widths: w1,
                    colors: c1,
                    styles: s1,
                    ..
                },
                Self::Border {
                    bounds: b2,
                    widths: w2,
                    colors: c2,
                    styles: s2,
                    ..
                },
            ) => {
                b1 == b2
                    && w1.top == w2.top
                    && w1.right == w2.right
                    && w1.bottom == w2.bottom
                    && w1.left == w2.left
                    && c1.top == c2.top
                    && c1.right == c2.right
                    && c1.bottom == c2.bottom
                    && c1.left == c2.left
                    && s1.top == s2.top
                    && s1.right == s2.right
                    && s1.bottom == s2.bottom
                    && s1.left == s2.left
            }
            (
                Self::Image {
                    bounds: b1,
                    image: i1,
                    border_radius: br1,
                },
                Self::Image {
                    bounds: b2,
                    image: i2,
                    border_radius: br2,
                },
            ) => {
                b1 == b2
                    // Compare the never-reused ImageRef identity, NOT the data
                    // pointer: `id` exists precisely because heap addresses
                    // get reused (core/resources.rs), and pointer identity
                    // also can't see "same image object, new pixels" swaps
                    // (per-frame producers replacing an ImageRef with an
                    // equal-address reallocation reported no damage).
                    && i1.get_hash() == i2.get_hash()
                    && br1.top_left == br2.top_left && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left && br1.bottom_right == br2.bottom_right
            }
            (
                Self::BoxShadow {
                    bounds: b1,
                    shadow: s1,
                    border_radius: br1,
                },
                Self::BoxShadow {
                    bounds: b2,
                    shadow: s2,
                    border_radius: br2,
                },
            ) => {
                b1 == b2
                    && s1 == s2
                    && br1.top_left == br2.top_left
                    && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left
                    && br1.bottom_right == br2.bottom_right
            }
            (
                Self::LinearGradient {
                    bounds: b1,
                    gradient: g1,
                    border_radius: br1,
                },
                Self::LinearGradient {
                    bounds: b2,
                    gradient: g2,
                    border_radius: br2,
                },
            ) => {
                b1 == b2
                    && g1 == g2
                    && br1.top_left == br2.top_left
                    && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left
                    && br1.bottom_right == br2.bottom_right
            }
            (
                Self::RadialGradient {
                    bounds: b1,
                    gradient: g1,
                    border_radius: br1,
                },
                Self::RadialGradient {
                    bounds: b2,
                    gradient: g2,
                    border_radius: br2,
                },
            ) => {
                b1 == b2
                    && g1 == g2
                    && br1.top_left == br2.top_left
                    && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left
                    && br1.bottom_right == br2.bottom_right
            }
            (
                Self::ConicGradient {
                    bounds: b1,
                    gradient: g1,
                    border_radius: br1,
                },
                Self::ConicGradient {
                    bounds: b2,
                    gradient: g2,
                    border_radius: br2,
                },
            ) => {
                b1 == b2
                    && g1 == g2
                    && br1.top_left == br2.top_left
                    && br1.top_right == br2.top_right
                    && br1.bottom_left == br2.bottom_left
                    && br1.bottom_right == br2.bottom_right
            }
            (
                Self::ScrollBar {
                    bounds: b1,
                    color: c1,
                    ..
                },
                Self::ScrollBar {
                    bounds: b2,
                    color: c2,
                    ..
                },
            ) => b1 == b2 && c1 == c2,
            (Self::PushClip { bounds: b1, .. }, Self::PushClip { bounds: b2, .. }) => b1 == b2,
            (
                Self::PushScrollFrame {
                    clip_bounds: b1,
                    scroll_id: s1,
                    ..
                },
                Self::PushScrollFrame {
                    clip_bounds: b2,
                    scroll_id: s2,
                    ..
                },
            ) => b1 == b2 && s1 == s2,
            (
                Self::PushStackingContext {
                    z_index: z1,
                    bounds: b1,
                },
                Self::PushStackingContext {
                    z_index: z2,
                    bounds: b2,
                },
            ) => z1 == z2 && b1 == b2,
            (
                Self::PushOpacity {
                    bounds: b1,
                    opacity: o1,
                    opacity_key: k1,
                },
                Self::PushOpacity {
                    bounds: b2,
                    opacity: o2,
                    opacity_key: k2,
                },
            ) => {
                // The key participates: two lists binding different keys draw
                // from different animation channels and are NOT interchangeable
                // (same rule the GPU-damage diff relies on).
                b1 == b2 && o1 == o2 && k1 == k2
            }
            // Pop items with no fields are always equal (discriminant already matched)
            (Self::PopClip, Self::PopClip)
            | (Self::PopImageMaskClip, Self::PopImageMaskClip)
            | (Self::PopScrollFrame, Self::PopScrollFrame)
            | (Self::PopStackingContext, Self::PopStackingContext)
            | (Self::PopReferenceFrame, Self::PopReferenceFrame)
            | (Self::PopFilter, Self::PopFilter)
            | (Self::PopBackdropFilter, Self::PopBackdropFilter)
            | (Self::PopOpacity, Self::PopOpacity)
            | (Self::PopTextShadow, Self::PopTextShadow) => true,
            // HitTestArea paints NO pixels (hit-testing only), so two of them are
            // always visually equal — a moved/changed hit region never needs a
            // repaint on its own. Without this it hit `_ => false` and forced
            // false-positive damage on every relayout (#12).
            (Self::HitTestArea { .. }, Self::HitTestArea { .. }) => true,
            // TextLayout: visually equal iff same box / font / colour AND the same
            // underlying (type-erased) layout allocation. A no-op relayout reuses
            // the cached layout Arc (pointer identity holds); a real text change
            // reshapes into a new Arc. Without this it hit `_ => false` and
            // reported damage every frame (#12).
            (
                Self::TextLayout {
                    layout: l1,
                    bounds: b1,
                    font_hash: fh1,
                    font_size_px: fs1,
                    color: c1,
                },
                Self::TextLayout {
                    layout: l2,
                    bounds: b2,
                    font_hash: fh2,
                    font_size_px: fs2,
                    color: c2,
                },
            ) => b1 == b2 && fh1 == fh2 && fs1 == fs2 && c1 == c2 && Arc::ptr_eq(l1, l2),
            // ScrollBarStyled: equal iff the STATIC drawing info matches. The
            // LIVE thumb position/opacity are read from the GPU value cache at
            // raster time (thumb_transform_key/opacity_key) — value changes are
            // damaged by the render_frame GPU-value diff, NOT by this item
            // comparison. Without this arm every scrollbar'd window re-damaged
            // its bar every frame (`_ => false`), so `FrameDamage::None` was
            // unreachable and idle windows re-rendered + re-presented forever.
            (Self::ScrollBarStyled { info: i1 }, Self::ScrollBarStyled { info: i2 }) => i1 == i2,
            // VirtualView: the item only carries WHERE the child renders; the
            // child DOM's content changes are detected by
            // compute_virtual_view_damage (child display-list diff).
            (
                Self::VirtualView {
                    child_dom_id: d1,
                    bounds: b1,
                    clip_rect: c1,
                    ..
                },
                Self::VirtualView {
                    child_dom_id: d2,
                    bounds: b2,
                    clip_rect: c2,
                    ..
                },
            ) => d1 == d2 && b1 == b2 && c1 == c2,
            (
                Self::VirtualViewPlaceholder { bounds: b1, .. },
                Self::VirtualViewPlaceholder { bounds: b2, .. },
            ) => b1 == b2,
            // PushReferenceFrame: the LIVE transform (drag, animation) is a GPU
            // cache value keyed by transform_key — covered by the GPU-value
            // diff, same as the scrollbar thumb.
            (
                Self::PushReferenceFrame {
                    transform_key: k1,
                    initial_transform: t1,
                    bounds: b1,
                },
                Self::PushReferenceFrame {
                    transform_key: k2,
                    initial_transform: t2,
                    bounds: b2,
                },
            ) => k1 == k2 && t1 == t2 && b1 == b2,
            (
                Self::PushFilter {
                    bounds: b1,
                    filters: f1,
                },
                Self::PushFilter {
                    bounds: b2,
                    filters: f2,
                },
            ) => b1 == b2 && f1 == f2,
            (
                Self::PushBackdropFilter {
                    bounds: b1,
                    filters: f1,
                },
                Self::PushBackdropFilter {
                    bounds: b2,
                    filters: f2,
                },
            ) => b1 == b2 && f1 == f2,
            // PushImageMaskClip: ImageRef comparison is by underlying data hash
            // (cheap identity), so a swapped mask image reports unequal.
            (
                Self::PushImageMaskClip {
                    bounds: b1,
                    mask_image: m1,
                    mask_rect: r1,
                },
                Self::PushImageMaskClip {
                    bounds: b2,
                    mask_image: m2,
                    mask_rect: r2,
                },
            ) => b1 == b2 && r1 == r2 && m1.get_hash() == m2.get_hash(),
            (Self::PushTextShadow { shadow: s1 }, Self::PushTextShadow { shadow: s2 }) => s1 == s2,
            // For other complex types (Image, gradients, etc.),
            // conservatively assume different
            _ => false,
        }
    }

    /// Returns true if this item is a state-management command (Push/Pop)
    /// that must always be processed to maintain correct stacks.
    #[must_use]
    pub const fn is_state_management(&self) -> bool {
        matches!(
            self,
            Self::PushClip { .. }
                | Self::PopClip
                | Self::PushImageMaskClip { .. }
                | Self::PopImageMaskClip
                | Self::PushScrollFrame { .. }
                | Self::PopScrollFrame
                | Self::PushStackingContext { .. }
                | Self::PopStackingContext
                | Self::PushReferenceFrame { .. }
                | Self::PopReferenceFrame
                | Self::PushFilter { .. }
                | Self::PopFilter
                | Self::PushBackdropFilter { .. }
                | Self::PopBackdropFilter
                | Self::PushOpacity { .. }
                | Self::PopOpacity
                | Self::PushTextShadow { .. }
                | Self::PopTextShadow
        )
    }

    /// Return the visual bounding rect including effects that extend beyond
    /// content bounds (e.g. box-shadow spread/blur/offset). Used for damage
    /// rect computation where we need the full repaint area.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[must_use]
    pub fn visual_bounds(&self) -> Option<LogicalRect> {
        match self {
            // TextLayout paints NOTHING — `render_single_item` skips it
            // outright ("metadata for PDF/accessibility"). So it must not
            // contribute damage either.
            //
            // It did. Its bounds is the enclosing node's whole content box
            // and its payload (the unified layout) changes on every edit, so
            // `is_visually_equal` said "changed" and the damage diff unioned
            // that box in — repainting the entire editor for a one-character
            // keystroke. It dominated the union completely: measured on a
            // 400x300 contenteditable, the two items that differ per
            // keystroke are this one at 356x65 (identical before and after)
            // and the caret at 2x22. Tightening TEXT damage could never show
            // up while a non-painting item was damaging everything.
            //
            // `None` here is what push/pop items already return: no pixels,
            // no damage. `bounds()` is left alone — it is the item filter in
            // the damaged renderer, and the render arm for TextLayout is a
            // no-op either way.
            Self::TextLayout { .. } => None,
            // Text damages only where its GLYPHS are, not its whole clip box.
            //
            // `bounds()` reports a text run's `clip_rect`, and the emission
            // site sets that to the node's entire viewport-sized content box.
            // Since `compute_display_list_damage` unions `visual_bounds()`,
            // that made damage for a text change as coarse as the enclosing
            // text node however little of it changed — measured at 384x48 for
            // a one-character edit, i.e. every line of the paragraph.
            //
            // `bounds()` deliberately keeps returning the clip rect: it is the
            // item filter in `render_display_list_damaged`, where being too
            // WIDE only costs a redundant intersection test, while being too
            // narrow would skip an item that should have painted. Only the
            // damage side is tightened here.
            //
            // The extent is padded generously and then clamped to the clip.
            // Ink routinely exceeds the advance box — diacritics, italic
            // overhang, `f`/`j` descenders, emoji — and under-damaging leaves
            // stale pixels on screen, which is worse than the coarseness this
            // replaces. The padding is in em, so it scales with the font.
            Self::Text {
                glyphs,
                clip_rect,
                font_size_px,
                ..
            } => {
                let clip = *clip_rect.inner();
                if glyphs.is_empty() {
                    return Some(clip);
                }
                let em = *font_size_px;
                let mut min_x = f32::MAX;
                let mut max_x = f32::MIN;
                let mut min_y = f32::MAX;
                let mut max_y = f32::MIN;
                for g in glyphs {
                    min_x = min_x.min(g.point.x);
                    max_x = max_x.max(g.point.x);
                    min_y = min_y.min(g.point.y);
                    max_y = max_y.max(g.point.y);
                }
                // `point` is the pen position ON the baseline, so the box has
                // to grow upward by the ascent and downward by the descent;
                // 1.5em/0.75em covers every Latin face plus stacked marks.
                // Horizontally the last glyph's own advance is still to come,
                // hence the extra em on the right.
                let ink = LogicalRect {
                    origin: LogicalPosition {
                        x: min_x - em,
                        y: min_y - em * 1.5,
                    },
                    size: LogicalSize {
                        width: (max_x - min_x) + em * 2.0,
                        height: (max_y - min_y) + em * 2.25,
                    },
                };
                Some(intersect_or(ink, clip))
            }
            Self::BoxShadow { bounds, shadow, .. } => {
                let b = *bounds.inner();
                // Shadow can extend beyond element bounds by offset + spread + blur
                let ox = shadow
                    .offset_x
                    .to_pixels_internal(DEFAULT_SHADOW_FONT_SIZE_PX, DEFAULT_SHADOW_FONT_SIZE_PX)
                    .abs();
                let oy = shadow
                    .offset_y
                    .to_pixels_internal(DEFAULT_SHADOW_FONT_SIZE_PX, DEFAULT_SHADOW_FONT_SIZE_PX)
                    .abs();
                let blur = shadow
                    .blur_radius
                    .to_pixels_internal(DEFAULT_SHADOW_FONT_SIZE_PX, DEFAULT_SHADOW_FONT_SIZE_PX)
                    .abs();
                let spread = shadow
                    .spread_radius
                    .to_pixels_internal(DEFAULT_SHADOW_FONT_SIZE_PX, DEFAULT_SHADOW_FONT_SIZE_PX)
                    .abs();
                let expand = ox + oy + blur + spread;
                Some(LogicalRect {
                    origin: LogicalPosition {
                        x: b.origin.x - expand,
                        y: b.origin.y - expand,
                    },
                    size: LogicalSize {
                        width: b.size.width + expand * 2.0,
                        height: b.size.height + expand * 2.0,
                    },
                })
            }
            _ => self.bounds(),
        }
    }

    /// Return the bounding rect of this item, or None for push/pop commands
    /// that don't have their own visual bounds.
    #[allow(clippy::match_same_arms)]
    // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    #[must_use]
    pub fn bounds(&self) -> Option<LogicalRect> {
        match self {
            Self::Rect { bounds, .. }
            | Self::SelectionRect { bounds, .. }
            | Self::CursorRect { bounds, .. }
            | Self::Border { bounds, .. }
            | Self::Text {
                clip_rect: bounds, ..
            }
            | Self::TextLayout { bounds, .. }
            | Self::Underline { bounds, .. }
            | Self::Strikethrough { bounds, .. }
            | Self::Overline { bounds, .. }
            | Self::Image { bounds, .. }
            | Self::ScrollBar { bounds, .. }
            | Self::StrokedPath { bounds, .. }
            | Self::LinearGradient { bounds, .. }
            | Self::RadialGradient { bounds, .. }
            | Self::ConicGradient { bounds, .. }
            | Self::BoxShadow { bounds, .. }
            | Self::VirtualView { bounds, .. }
            | Self::VirtualViewPlaceholder { bounds, .. }
            | Self::HitTestArea { bounds, .. }
            | Self::PushClip { bounds, .. }
            | Self::PushImageMaskClip { bounds, .. }
            | Self::PushScrollFrame {
                clip_bounds: bounds,
                ..
            }
            | Self::PushStackingContext { bounds, .. }
            | Self::PushReferenceFrame { bounds, .. }
            | Self::PushFilter { bounds, .. }
            | Self::PushBackdropFilter { bounds, .. }
            | Self::PushOpacity { bounds, .. } => Some(*bounds.inner()),
            Self::ScrollBarStyled { info, .. } => Some(*info.bounds.inner()),
            Self::PushTextShadow { .. } => None, // text shadow has no bounds, affects following text
            Self::PopClip
            | Self::PopImageMaskClip
            | Self::PopScrollFrame
            | Self::PopStackingContext
            | Self::PopReferenceFrame
            | Self::PopFilter
            | Self::PopBackdropFilter
            | Self::PopOpacity
            | Self::PopTextShadow => None,
        }
    }
}

// Helper structs for the DisplayList
#[derive(Debug, Copy, Clone, Default, PartialEq)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

impl BorderRadius {
    #[must_use]
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_left == 0.0
            && self.bottom_right == 0.0
    }
}

// Dummy types for compilation
pub type LocalScrollId = u64;
/// Display list tag ID as (payload, `type_marker`) tuple.
/// The u16 field is used as a namespace marker:
/// - 0x0100 = DOM Node (regular interactive elements)
/// - 0x0200 = Scrollbar component
pub(crate) type DisplayListTagId = (u64, u16);

/// Internal builder to accumulate display list items during generation.
#[derive(Debug, Default)]
struct DisplayListBuilder {
    items: Vec<DisplayListItem>,
    node_mapping: Vec<Option<NodeId>>,
    /// Current node being processed (set by generator)
    current_node: Option<NodeId>,
    /// Collected debug messages (transferred to ctx on finalize)
    debug_messages: Vec<LayoutDebugMessage>,
    /// Whether debug logging is enabled
    debug_enabled: bool,
    /// Y-positions where forced page breaks should occur
    forced_page_breaks: Vec<ForcedBreak>,
    /// Index ranges of items from fixed-position elements (for paged media replication)
    fixed_position_item_ranges: Vec<(usize, usize)>,
    /// Start index of the current fixed-position element being built, if any
    fixed_position_start: Option<usize>,
    /// Current (layout index, phase) for `layout_node_mapping` — see
    /// [`DisplayList::layout_node_mapping`].
    current_layout: Option<(usize, EmitPhase)>,
    layout_node_mapping: Vec<Option<(usize, EmitPhase)>>,
    /// One-shot: the uniform background for the NEXT pushed item (set by
    /// `push_text_run` / the patch copy just before pushing a Text item).
    next_text_bg: Option<(ColorU, WindowLogicalRect)>,
    uniform_text_bgs: Vec<Option<(ColorU, WindowLogicalRect)>>,
}

impl DisplayListBuilder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) const fn with_debug(debug_enabled: bool) -> Self {
        Self {
            items: Vec::new(),
            node_mapping: Vec::new(),
            current_node: None,
            debug_messages: Vec::new(),
            debug_enabled,
            forced_page_breaks: Vec::new(),
            fixed_position_item_ranges: Vec::new(),
            fixed_position_start: None,
            current_layout: None,
            layout_node_mapping: Vec::new(),
            next_text_bg: None,
            uniform_text_bgs: Vec::new(),
        }
    }

    /// Log a debug message if debug is enabled
    fn debug_log(&mut self, message: String) {
        if self.debug_enabled {
            self.debug_messages.push(LayoutDebugMessage::info(message));
        }
    }

    /// Build the display list and transfer debug messages to the provided option
    pub(crate) fn build_with_debug(
        mut self,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> DisplayList {
        // Transfer collected debug messages to the context
        if let Some(msgs) = debug_messages.as_mut() {
            msgs.append(&mut self.debug_messages);
        }
        DisplayList {
            items: self.items,
            node_mapping: self.node_mapping,
            forced_page_breaks: self.forced_page_breaks,
            fixed_position_item_ranges: self.fixed_position_item_ranges,
            layout_node_mapping: self.layout_node_mapping,
            uniform_text_bgs: self.uniform_text_bgs,
        }
    }

    /// Set the current node context for subsequent push operations
    pub(crate) const fn set_current_node(&mut self, node_id: Option<NodeId>) {
        self.current_node = node_id;
    }

    /// Tag subsequent pushes with (layout index, phase). `None` = untagged
    /// (equivalent to `ScWalk` for patching purposes — never substituted).
    pub(crate) const fn set_current_layout(&mut self, tag: Option<(usize, EmitPhase)>) {
        self.current_layout = tag;
    }

    /// The current node context (to save/restore around pushes that must be
    /// attributed to a DIFFERENT node, e.g. inline images inside an IFC).
    pub(crate) const fn current_node(&self) -> Option<NodeId> {
        self.current_node
    }

    /// Mark the start of a fixed-position element's display items.
    pub(crate) const fn begin_fixed_position_element(&mut self) {
        self.fixed_position_start = Some(self.items.len());
    }

    /// Mark the end of a fixed-position element's display items.
    /// Records the (start, end) index range for paged media replication.
    pub(crate) fn end_fixed_position_element(&mut self) {
        if let Some(start) = self.fixed_position_start.take() {
            let end = self.items.len();
            if end > start {
                self.fixed_position_item_ranges.push((start, end));
            }
        }
    }

    /// Register a forced page break at the given Y position, remembering the
    /// node whose break property caused it (diagnostics: surfaces in
    /// `PageBreakPosition::causing_node`).
    /// This is used for CSS break-before: always and break-after: always.
    pub(crate) fn add_forced_page_break(&mut self, y_position: f32, causing_node: Option<NodeId>) {
        // Avoid duplicates and keep sorted by Y
        if !self.forced_page_breaks.iter().any(|b| b.y == y_position) {
            self.forced_page_breaks.push(ForcedBreak {
                y: y_position,
                causing_node,
            });
            self.forced_page_breaks
                .sort_by(|a, b| a.y.partial_cmp(&b.y).unwrap_or(core::cmp::Ordering::Equal));
        }
    }

    /// Push an item and record its node mapping.
    ///
    /// Drops items whose geometry is still the UNASSIGNED-POSITION SENTINEL.
    ///
    /// `calculated_positions` seeds entries with `f32::MIN`, and an inline text
    /// node's own box position is never assigned — see the comment on
    /// `paint_text_selection`, which anchors to the IFC root precisely because
    /// "the node's OWN box position is never assigned (stays the `f32::MIN`
    /// sentinel)". That workaround is local to one caller; the other thirteen
    /// readers of `calculated_positions` get the raw sentinel.
    ///
    /// The published 0.2.0 azul-self-test-linux display list carried 24 of
    /// them, e.g.
    ///
    /// ```text
    /// Border      { bounds: WindowLogicalRect(624x0 @ (-3.4028235e38, -3.4028235e38)) }
    /// HitTestArea { bounds: WindowLogicalRect(624x0 @ (-3.4028235e38, -3.4028235e38)) }
    /// ```
    ///
    /// A `Border` at -3.4e38 is merely invisible. A `HitTestArea` there is
    /// ACTIVELY WRONG: it is a live hit-test rectangle at a coordinate no
    /// pointer can reach, and any arithmetic on it (offsetting into a parent
    /// space, unioning a bounding box) produces infinities that then propagate
    /// into rects that ARE on screen.
    ///
    /// Dropping is right rather than clamping to the origin: the item has no
    /// position, and inventing (0,0) would put a phantom border and a live
    /// hit target in the top-left corner of the window. An item nobody can
    /// see and nobody can click is exactly what an unpositioned node should
    /// contribute.
    fn push_item(&mut self, item: DisplayListItem) {
        // A TextLayout carrying `FontHash::invalid()` (0) describes text in a
        // font that does not exist. Verified before dropping it, because
        // "harmless metadata" is a claim that has to be checked:
        //   * compositor2.rs   — explicit no-op, "handled elsewhere (via
        //                        PushCachedTextRuns)"; the GPU path never draws it
        //   * cpurender/raster — "TextLayout is metadata for PDF/accessibility -
        //                        skip in CPU rendering"
        //   * hit-testing      — uses separate HitTestArea items
        //   * selection        — uses get_selection_rects on the inline layout,
        //                        not display-list items
        //   * scan_used_fonts  — reads font_hash (fixed separately: 0 was being
        //                        turned into a phantom FontKey and marked live)
        //   * PDF export       — cannot render text in a font it cannot resolve
        // So nothing draws, hits, or selects it, and the one consumer that read
        // it was harmed by doing so.
        if let DisplayListItem::TextLayout { font_hash, .. } = &item {
            if font_hash.font_hash == 0 {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[azul][displaylist] dropping a TextLayout with \
                     FontHash::invalid() — its font never resolved, so no \
                     renderer would have drawn it"
                );
                return;
            }
        }
        if let Some(b) = item.bounds() {
            let unassigned = |v: f32| !v.is_finite() || v <= UNASSIGNED_POSITION_LIMIT;
            if unassigned(b.origin.x) || unassigned(b.origin.y) {
                #[cfg(debug_assertions)]
                eprintln!(
                    "[azul][displaylist] dropping {item:?} at an unassigned \
                     position ({}, {}) — its layout node never received a \
                     computed position",
                    b.origin.x, b.origin.y,
                );
                return;
            }
        }
        // Structural push/pop items get NO node attribution. Before this,
        // they inherited whatever `current_node` the LAST paint call leaked —
        // a PopStackingContext attributed to some text node that happened to
        // paint last. No consumer legitimately reads a break property or a
        // damage source off a Push/Pop, and the leak made attribution
        // pass-history-dependent (the DL-patch golden test caught patched
        // and full passes disagreeing about it).
        let attr = if matches!(
            item,
            DisplayListItem::PushClip { .. }
                | DisplayListItem::PopClip
                | DisplayListItem::PushImageMaskClip { .. }
                | DisplayListItem::PopImageMaskClip
                | DisplayListItem::PushScrollFrame { .. }
                | DisplayListItem::PopScrollFrame
                | DisplayListItem::PushStackingContext { .. }
                | DisplayListItem::PopStackingContext
                | DisplayListItem::PushReferenceFrame { .. }
                | DisplayListItem::PopReferenceFrame
                | DisplayListItem::PushFilter { .. }
                | DisplayListItem::PopFilter
                | DisplayListItem::PushBackdropFilter { .. }
                | DisplayListItem::PopBackdropFilter
                | DisplayListItem::PushOpacity { .. }
                | DisplayListItem::PopOpacity
        ) {
            None
        } else {
            self.current_node
        };
        self.items.push(item);
        self.node_mapping.push(attr);
        self.layout_node_mapping.push(self.current_layout);
        self.uniform_text_bgs.push(self.next_text_bg.take());
    }

    pub(crate) fn build(self) -> DisplayList {
        DisplayList {
            items: self.items,
            node_mapping: self.node_mapping,
            forced_page_breaks: self.forced_page_breaks,
            fixed_position_item_ranges: self.fixed_position_item_ranges,
            layout_node_mapping: self.layout_node_mapping,
            uniform_text_bgs: self.uniform_text_bgs,
        }
    }

    pub(crate) fn push_hit_test_area(&mut self, bounds: LogicalRect, tag: DisplayListTagId) {
        self.push_item(DisplayListItem::HitTestArea {
            bounds: bounds.into(),
            tag,
        });
    }

    /// Push a simple single-color scrollbar (legacy method).
    pub(crate) fn push_scrollbar(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        orientation: ScrollbarOrientation,
        opacity_key: Option<OpacityKey>,
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    ) {
        if color.a > 0 || opacity_key.is_some() {
            // Optimization: Don't draw fully transparent items without opacity keys.
            self.push_item(DisplayListItem::ScrollBar {
                bounds: bounds.into(),
                color,
                orientation,
                opacity_key,
                hit_id,
            });
        }
    }

    /// Push a fully styled scrollbar with track, thumb, and optional buttons.
    pub(crate) fn push_scrollbar_styled(&mut self, info: ScrollbarDrawInfo) {
        // Only push if at least the thumb or track is visible
        if info.thumb_color.a > 0 || info.track_color.a > 0 || info.opacity_key.is_some() {
            self.push_item(DisplayListItem::ScrollBarStyled {
                info: Box::new(info),
            });
        }
    }

    pub(crate) fn push_rect(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        border_radius: BorderRadius,
    ) {
        if color.a > 0 {
            // Optimization: Don't draw fully transparent items.
            self.push_item(DisplayListItem::Rect {
                bounds: bounds.into(),
                color,
                border_radius,
            });
        }
    }

    /// Unified method to paint all background layers and border for an element.
    ///
    /// This consolidates the background/border painting logic that was previously
    /// duplicated across:
    /// - `paint_node_background_and_border()` for block elements
    /// - `paint_inline_shape()` for inline-block elements
    ///
    /// The backgrounds are painted in order (back to front per CSS spec), followed
    /// by the border.
    pub(crate) fn push_backgrounds_and_border(
        &mut self,
        bounds: LogicalRect,
        background_contents: &[azul_css::props::style::StyleBackgroundContent],
        border_info: &BorderInfo,
        simple_border_radius: BorderRadius,
        style_border_radius: StyleBorderRadius,
        image_cache: &azul_core::resources::ImageCache,
    ) {
        use azul_css::props::style::StyleBackgroundContent;

        // Paint all background layers in order (CSS paints backgrounds back to front)
        for bg in background_contents {
            match bg {
                StyleBackgroundContent::Color(color) => {
                    self.push_rect(bounds, *color, simple_border_radius);
                }
                StyleBackgroundContent::LinearGradient(gradient) => {
                    self.push_linear_gradient(bounds, gradient.clone(), simple_border_radius);
                }
                StyleBackgroundContent::RadialGradient(gradient) => {
                    self.push_radial_gradient(bounds, gradient.clone(), simple_border_radius);
                }
                StyleBackgroundContent::ConicGradient(gradient) => {
                    self.push_conic_gradient(bounds, gradient.clone(), simple_border_radius);
                }
                StyleBackgroundContent::Image(image_id) => {
                    if let Some(image_ref) = image_cache.get_css_image_id(image_id) {
                        self.push_image(bounds, image_ref.clone(), simple_border_radius);
                    }
                }
                StyleBackgroundContent::SystemColor(_s) => {
                    // TODO(superplan g8): resolve via SystemColorRef::resolve(&SystemColors,
                    // fallback) and push_rect. SystemColors is not threaded into the
                    // display-list builder yet, so `background: system:<name>` currently
                    // parses but paints nothing (graceful no-op rather than a wrong color).
                }
            }
        }

        // Paint border
        self.push_border(
            bounds,
            border_info.widths,
            border_info.colors,
            border_info.styles,
            style_border_radius,
        );
    }

    /// Paint backgrounds and border for inline text elements.
    ///
    /// Similar to `push_backgrounds_and_border` but uses `InlineBorderInfo` which stores
    /// pre-resolved pixel values instead of CSS property values. This is used for
    /// inline (display: inline) elements where the border info is computed during
    /// text layout and stored in the glyph runs.
    pub(crate) fn push_inline_backgrounds_and_border(
        &mut self,
        bounds: LogicalRect,
        background_color: Option<ColorU>,
        background_contents: &[azul_css::props::style::StyleBackgroundContent],
        border: Option<&crate::text3::cache::InlineBorderInfo>,
        image_cache: &azul_core::resources::ImageCache,
    ) {
        use azul_css::props::style::StyleBackgroundContent;

        // Paint solid background color if present
        if let Some(bg_color) = background_color {
            self.push_rect(bounds, bg_color, BorderRadius::default());
        }

        // Paint all background layers in order (CSS paints backgrounds back to front)
        for bg in background_contents {
            match bg {
                StyleBackgroundContent::Color(color) => {
                    self.push_rect(bounds, *color, BorderRadius::default());
                }
                StyleBackgroundContent::LinearGradient(gradient) => {
                    self.push_linear_gradient(bounds, gradient.clone(), BorderRadius::default());
                }
                StyleBackgroundContent::RadialGradient(gradient) => {
                    self.push_radial_gradient(bounds, gradient.clone(), BorderRadius::default());
                }
                StyleBackgroundContent::ConicGradient(gradient) => {
                    self.push_conic_gradient(bounds, gradient.clone(), BorderRadius::default());
                }
                StyleBackgroundContent::Image(image_id) => {
                    if let Some(image_ref) = image_cache.get_css_image_id(image_id) {
                        self.push_image(bounds, image_ref.clone(), BorderRadius::default());
                    }
                }
                StyleBackgroundContent::SystemColor(_s) => {
                    // TODO(superplan g8): resolve via SystemColorRef::resolve(&SystemColors,
                    // fallback) and push_rect. SystemColors is not threaded into the
                    // display-list builder yet, so `background: system:<name>` currently
                    // parses but paints nothing (graceful no-op rather than a wrong color).
                }
            }
        }

        // Paint border if present
        // CSS 2.2 §8.6: suppress left/right borders at split points, respecting direction
        if let Some(border) = border {
            let effective_left = if border.left_inset() > 0.0 {
                border.left
            } else {
                0.0
            };
            let effective_right = if border.right_inset() > 0.0 {
                border.right
            } else {
                0.0
            };
            if border.top > 0.0
                || effective_right > 0.0
                || border.bottom > 0.0
                || effective_left > 0.0
            {
                let border_widths = StyleBorderWidths {
                    top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth {
                        inner: PixelValue::px(border.top),
                    })),
                    right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth {
                        inner: PixelValue::px(effective_right),
                    })),
                    bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth {
                        inner: PixelValue::px(border.bottom),
                    })),
                    left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth {
                        inner: PixelValue::px(effective_left),
                    })),
                };
                let border_colors = StyleBorderColors {
                    top: Some(CssPropertyValue::Exact(StyleBorderTopColor {
                        inner: border.top_color,
                    })),
                    right: Some(CssPropertyValue::Exact(StyleBorderRightColor {
                        inner: border.right_color,
                    })),
                    bottom: Some(CssPropertyValue::Exact(StyleBorderBottomColor {
                        inner: border.bottom_color,
                    })),
                    left: Some(CssPropertyValue::Exact(StyleBorderLeftColor {
                        inner: border.left_color,
                    })),
                };
                let border_styles = StyleBorderStyles {
                    top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                        inner: BorderStyle::Solid,
                    })),
                    right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                        inner: BorderStyle::Solid,
                    })),
                    bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                        inner: BorderStyle::Solid,
                    })),
                    left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                        inner: BorderStyle::Solid,
                    })),
                };
                let radius_px = PixelValue::px(border.radius.unwrap_or(0.0));
                let border_radius = StyleBorderRadius {
                    top_left: radius_px,
                    top_right: radius_px,
                    bottom_left: radius_px,
                    bottom_right: radius_px,
                };

                self.push_border(
                    bounds,
                    border_widths,
                    border_colors,
                    border_styles,
                    border_radius,
                );
            }
        }
    }

    /// Push a linear gradient background
    pub(crate) fn push_linear_gradient(
        &mut self,
        bounds: LogicalRect,
        gradient: LinearGradient,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::LinearGradient {
            bounds: bounds.into(),
            gradient,
            border_radius,
        });
    }

    /// Push a radial gradient background
    pub(crate) fn push_radial_gradient(
        &mut self,
        bounds: LogicalRect,
        gradient: RadialGradient,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::RadialGradient {
            bounds: bounds.into(),
            gradient,
            border_radius,
        });
    }

    /// Push a conic gradient background
    pub(crate) fn push_conic_gradient(
        &mut self,
        bounds: LogicalRect,
        gradient: ConicGradient,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::ConicGradient {
            bounds: bounds.into(),
            gradient,
            border_radius,
        });
    }

    pub(crate) fn push_selection_rect(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        border_radius: BorderRadius,
    ) {
        if color.a > 0 {
            self.push_item(DisplayListItem::SelectionRect {
                bounds: bounds.into(),
                color,
                border_radius,
            });
        }
    }

    pub(crate) fn push_cursor_rect(&mut self, bounds: LogicalRect, color: ColorU) {
        // Always emit the caret item — even with alpha == 0 in the blink-off phase — so
        // the display-list item COUNT stays stable across blink phases. That lets
        // compute_display_list_damage diff it down to a caret-sized rect instead of
        // falling back to a full-window repaint every ~530ms.
        self.push_item(DisplayListItem::CursorRect {
            bounds: bounds.into(),
            color,
        });
    }
    pub(crate) fn push_clip(&mut self, bounds: LogicalRect, border_radius: BorderRadius) {
        self.push_item(DisplayListItem::PushClip {
            bounds: bounds.into(),
            border_radius,
        });
    }
    pub(crate) fn pop_clip(&mut self) {
        self.push_item(DisplayListItem::PopClip);
    }
    pub(crate) fn push_image_mask_clip(
        &mut self,
        bounds: LogicalRect,
        mask_image: ImageRef,
        mask_rect: LogicalRect,
    ) {
        self.push_item(DisplayListItem::PushImageMaskClip {
            bounds: bounds.into(),
            mask_image,
            mask_rect: mask_rect.into(),
        });
    }
    pub(crate) fn pop_image_mask_clip(&mut self) {
        self.push_item(DisplayListItem::PopImageMaskClip);
    }
    pub(crate) fn push_scroll_frame(
        &mut self,
        clip_bounds: LogicalRect,
        content_size: LogicalSize,
        scroll_id: LocalScrollId,
    ) {
        self.push_item(DisplayListItem::PushScrollFrame {
            clip_bounds: clip_bounds.into(),
            content_size,
            scroll_id,
        });
    }
    pub(crate) fn pop_scroll_frame(&mut self) {
        self.push_item(DisplayListItem::PopScrollFrame);
    }
    pub(crate) fn push_virtual_view_placeholder(
        &mut self,
        node_id: NodeId,
        bounds: LogicalRect,
        clip_rect: LogicalRect,
    ) {
        self.push_item(DisplayListItem::VirtualViewPlaceholder {
            node_id,
            bounds: bounds.into(),
            clip_rect: clip_rect.into(),
        });
    }
    pub(crate) fn push_border(
        &mut self,
        bounds: LogicalRect,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        border_radius: StyleBorderRadius,
    ) {
        // A `Border` item exists only when some side actually PAINTS: a
        // positive width AND a style other than `none` / `hidden` on that
        // side (CSS Backgrounds 3 §4.2: `border-style: none` computes the
        // width to 0). "Any width is Some and any style is Some" was not
        // that test — the compact property cache answers `Some(0px)` and
        // `Some(none)` for every UNSET border, so every node in its normal
        // state carried an invisible zero-width Border item, while the same
        // node in a `:hover` / `:focus` / `:active` state (resolved through
        // the slow path, which answers `None`) carried none. A pseudo-state
        // flip therefore changed the item list without any restyle, and the
        // patched display list drifted from the wholesale build by exactly
        // those phantoms (`bug-scroll-offsets-hit-test` under
        // AZ_PATCH_VERIFY).
        let has_visible_border = Self::border_side_paints(
            widths
                .top
                .as_ref()
                .and_then(|w| w.get_property())
                .map(|w| w.inner),
            styles
                .top
                .as_ref()
                .and_then(|s| s.get_property())
                .map(|s| s.inner),
        ) || Self::border_side_paints(
            widths
                .right
                .as_ref()
                .and_then(|w| w.get_property())
                .map(|w| w.inner),
            styles
                .right
                .as_ref()
                .and_then(|s| s.get_property())
                .map(|s| s.inner),
        ) || Self::border_side_paints(
            widths
                .bottom
                .as_ref()
                .and_then(|w| w.get_property())
                .map(|w| w.inner),
            styles
                .bottom
                .as_ref()
                .and_then(|s| s.get_property())
                .map(|s| s.inner),
        ) || Self::border_side_paints(
            widths
                .left
                .as_ref()
                .and_then(|w| w.get_property())
                .map(|w| w.inner),
            styles
                .left
                .as_ref()
                .and_then(|s| s.get_property())
                .map(|s| s.inner),
        );

        if has_visible_border {
            self.push_item(DisplayListItem::Border {
                bounds: bounds.into(),
                widths,
                colors,
                styles,
                border_radius,
            });
        }
    }

    /// One border side paints when its width resolves above zero and its
    /// style draws something (`none` and `hidden` never do). Widths are
    /// resolved the way the rasterizer resolves them (no percentage basis,
    /// the default font size for em).
    fn border_side_paints(width: Option<PixelValue>, style: Option<BorderStyle>) -> bool {
        let width = width.map_or(0.0, |w| {
            w.to_pixels_internal(
                0.0,
                azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
                azul_css::props::basic::pixel::DEFAULT_FONT_SIZE,
            )
        });
        width > 0.0 && style.is_some_and(|s| !matches!(s, BorderStyle::None | BorderStyle::Hidden))
    }

    pub(crate) fn push_stacking_context(&mut self, z_index: i32, bounds: LogicalRect) {
        self.push_item(DisplayListItem::PushStackingContext {
            z_index,
            bounds: bounds.into(),
        });
    }

    pub(crate) fn pop_stacking_context(&mut self) {
        self.push_item(DisplayListItem::PopStackingContext);
    }

    pub(crate) fn push_reference_frame(
        &mut self,
        transform_key: TransformKey,
        initial_transform: ComputedTransform3D,
        bounds: LogicalRect,
    ) {
        self.push_item(DisplayListItem::PushReferenceFrame {
            transform_key,
            initial_transform,
            bounds: bounds.into(),
        });
    }

    pub(crate) fn pop_reference_frame(&mut self) {
        self.push_item(DisplayListItem::PopReferenceFrame);
    }

    pub(crate) fn push_text_run(
        &mut self,
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash, // Just the hash, not the full FontRef
        font_size_px: f32,
        color: ColorU,
        clip_rect: LogicalRect,
        source_node_index: Option<usize>,
        uniform_bg: Option<(ColorU, WindowLogicalRect)>,
    ) {
        self.debug_log(format!(
            "[push_text_run] {} glyphs, font_size={}px, color=({},{},{},{}), clip={:?}",
            glyphs.len(),
            font_size_px,
            color.r,
            color.g,
            color.b,
            color.a,
            clip_rect
        ));

        if !glyphs.is_empty() && color.a > 0 {
            self.next_text_bg = uniform_bg;
            self.push_item(DisplayListItem::Text {
                glyphs,
                font_hash,
                font_size_px,
                color,
                clip_rect: clip_rect.into(),
                source_node_index,
            });
        } else {
            self.debug_log(format!(
                "[push_text_run] SKIPPED: glyphs.is_empty()={}, color.a={}",
                glyphs.is_empty(),
                color.a
            ));
        }
    }

    pub(crate) fn push_text_layout(
        &mut self,
        layout: Arc<dyn std::any::Any + Send + Sync>,
        bounds: LogicalRect,
        font_hash: FontHash,
        font_size_px: f32,
        color: ColorU,
    ) {
        if color.a > 0 {
            self.push_item(DisplayListItem::TextLayout {
                layout,
                bounds: bounds.into(),
                font_hash,
                font_size_px,
                color,
            });
        }
    }

    pub(crate) fn push_underline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Underline {
                bounds: bounds.into(),
                color,
                thickness,
            });
        }
    }

    pub(crate) fn push_strikethrough(
        &mut self,
        bounds: LogicalRect,
        color: ColorU,
        thickness: f32,
    ) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Strikethrough {
                bounds: bounds.into(),
                color,
                thickness,
            });
        }
    }

    pub(crate) fn push_overline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Overline {
                bounds: bounds.into(),
                color,
                thickness,
            });
        }
    }

    pub(crate) fn push_image(
        &mut self,
        bounds: LogicalRect,
        image: ImageRef,
        border_radius: BorderRadius,
    ) {
        self.push_item(DisplayListItem::Image {
            bounds: bounds.into(),
            image,
            border_radius,
        });
    }
}

/// Main entry point for generating the display list.
#[allow(clippy::implicit_hasher)] // internal helper; only ever called with the default-hasher HashMap/HashSet
/// # Errors
///
/// Returns a `LayoutError` if display-list generation fails.
pub fn generate_display_list<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    calculated_positions: &super::PositionVec,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &HashMap<LayoutNodeId, u64>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
) -> Result<DisplayList> {
    generate_display_list_impl(
        ctx,
        tree,
        calculated_positions,
        scroll_offsets,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
        None,
    )
}

/// [`generate_display_list`] + the DL-PATCHING source. With `patch` armed
/// (resize-skip passes), unchanged nodes' paint calls are substituted by
/// their previous items translated by the node's position delta — only
/// re-flowed IFCs and size-changed nodes actually re-emit.
#[allow(clippy::too_many_arguments)]
pub fn generate_display_list_impl<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<'_, T>,
    tree: &LayoutTree,
    calculated_positions: &super::PositionVec,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &HashMap<LayoutNodeId, u64>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
    patch: Option<PatchState<'_>>,
) -> Result<DisplayList> {
    debug_info!(
        ctx,
        "[DisplayList] generate_display_list: tree has {} nodes, {} positions calculated",
        tree.nodes.len(),
        calculated_positions.len()
    );
    // AZ_PROFILE=cpu breakdown of the DL build. Inert (one relaxed load)
    // unless recording is on — see probe::set_recording.
    let _dl_span = crate::probe::Probe::span("dl_generate");

    debug_info!(ctx, "Starting display list generation");
    debug_info!(
        ctx,
        "Collecting stacking contexts from root node {}",
        tree.root
    );

    let positioned_tree = PositionedTree {
        tree,
        calculated_positions,
    };
    let mut generator = DisplayListGenerator::new(
        ctx,
        scroll_offsets,
        &positioned_tree,
        scroll_ids,
        gpu_value_cache,
        renderer_resources,
        id_namespace,
        dom_id,
    );
    generator.patch = patch;

    // Create builder with debug enabled if ctx has debug messages
    let debug_enabled = generator.ctx.debug_messages.is_some();
    let mut builder = DisplayListBuilder::with_debug(debug_enabled);

    // 0. Canvas background propagation (CSS 2.1 § 14.2):
    //    "The background of the root element becomes the background of the canvas."
    //    If the root (html) has a transparent background, propagate from <body>.
    //    The canvas background fills the ENTIRE viewport, not just the root's content box.
    //    This is critical when <html> doesn't have height:100% — without this,
    //    the body's background only covers the body's content area, not the viewport.
    {
        let root_node = tree.get(LayoutNodeId::new(tree.root));
        if let Some(root) = root_node {
            if let Some(root_dom_id) = root.dom_node_id {
                let root_state = generator.get_styled_node_state(root_dom_id);
                let canvas_bg =
                    get_background_color(generator.ctx.styled_dom, root_dom_id, &root_state);
                if canvas_bg.a > 0 {
                    let viewport_rect = LogicalRect {
                        origin: LogicalPosition::zero(),
                        size: generator.ctx.viewport_size,
                    };
                    builder.push_rect(viewport_rect, canvas_bg, BorderRadius::default());
                    debug_info!(
                        generator.ctx,
                        "[DisplayList] Canvas background: color=({},{},{},{}), size={:?}",
                        canvas_bg.r,
                        canvas_bg.g,
                        canvas_bg.b,
                        canvas_bg.a,
                        generator.ctx.viewport_size
                    );
                }
            }
        }
    }

    // +spec:stacking-contexts:33d435 - CSS 2.2 painting order: build stacking context tree then traverse in z-order
    // +spec:stacking-contexts:887766 - CSS2 §9.9 stacking contexts, z-index layering, and painting order
    // 1. Build a tree of stacking contexts, which defines the global paint order.
    // +spec:display-property:9a419c - root element always forms a stacking context (it's the tree root)
    let stacking_context_tree = {
        let _p = crate::probe::Probe::span("dl_collect_stacking");
        generator.collect_stacking_contexts(tree.root)?
    };

    // 2. Traverse the stacking context tree to generate display items in the correct order.
    debug_info!(
        generator.ctx,
        "Generating display items from stacking context tree"
    );
    generator.generate_for_stacking_context(&mut builder, &stacking_context_tree)?;

    // Build display list and transfer debug messages to context
    let display_list = builder.build_with_debug(generator.ctx.debug_messages);
    debug_info!(
        generator.ctx,
        "[DisplayList] Generated {} display items",
        display_list.items.len()
    );
    Ok(display_list)
}

/// A helper struct that holds all necessary state and context for the generation process.
struct DisplayListGenerator<'a, 'b, T: ParsedFontTrait> {
    ctx: &'a mut LayoutContext<'b, T>,
    scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
    positioned_tree: &'a PositionedTree<'a>,
    scroll_ids: &'a HashMap<LayoutNodeId, u64>,
    gpu_value_cache: Option<&'a GpuValueCache>,
    renderer_resources: &'a RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
    /// DL-PATCHING source (resize-skip passes only): the previous pass's
    /// display list plus per-node deltas. `None` = full generation.
    patch: Option<PatchState<'a>>,
}

/// State for patched display-list generation. Built by `layout_document`
/// on a resize-skip pass — the tree object and its indices are unchanged,
/// so per-(layout node, phase) item runs from the PREVIOUS list can replace
/// the paint calls of every node that neither re-flowed its inline content
/// nor changed size; their geometry moves by the node's position delta.
pub(crate) struct PatchState<'a> {
    prev: &'a DisplayList,
    /// Per (layout idx, phase): queue of item ranges in `prev`, walk order.
    runs: HashMap<(usize, u8), std::collections::VecDeque<(usize, usize)>>,
    /// Per layout idx: (`new_pos` - `old_pos`).
    deltas: Vec<LogicalPosition>,
    /// Nodes that MUST re-emit (re-flowed IFC, size changed).
    reemit: std::collections::BTreeSet<usize>,
}

/// Presentation summary of a patched pass: which single translation moved
/// most of the frame, and which rects must be repainted anyway. Feeds the
/// CPU compositor's translate-aware presentation (blit the retained pixmap
/// by `dominant_delta`, repaint `exceptions` + exposed strips) — the round-3
/// path that makes a drag frame memory-bandwidth instead of rendering.
#[derive(Debug, Clone)]
pub struct PatchMoveSummary {
    /// The translation shared by the largest moved item AREA (logical px).
    pub dominant_delta: LogicalPosition,
    /// OLD rects of the dominant movers, one blit each. Per-rect (not a
    /// union): the union would include the gaps BETWEEN movers — static
    /// backdrop that must not be dragged. Only movers that paint an OPAQUE
    /// background over their own rect qualify (a translucent mover's pixels
    /// are a blend with the static backdrop, which does not translate).
    pub mover_rects_old: Vec<LogicalRect>,
    /// Union of `mover_rects_old` — the region the translated DIFF may
    /// classify as moves.
    pub moved_region_old: LogicalRect,
    /// Rects that must repaint regardless: re-emitted nodes, off-delta
    /// movers, non-opaque movers, and NON-ANCESTOR static nodes overlapping
    /// a mover (ancestors paint BELOW their descendants — the root
    /// background under a page is covered by the opaque page and never
    /// dragged visibly; a static sibling painted ABOVE would be) — each as
    /// old∪new bounds.
    pub exceptions: Vec<LogicalRect>,
}

/// Compute a [`PatchMoveSummary`] from the same inputs `PatchState::build`
/// consumes. Returns `None` when the frame has no dominant translation
/// (nothing moved, or the exception list would exceed its cap — at which
/// point a plain full repaint is the better deal).
#[allow(clippy::too_many_lines)]
#[must_use]
pub fn compute_patch_move_summary(
    prev_positions: &[LogicalPosition],
    new_positions: &[LogicalPosition],
    prev_sizes: &[Option<LogicalSize>],
    new_sizes: &[Option<LogicalSize>],
    reemit: &std::collections::BTreeSet<usize>,
    // parents[i] = layout parent of node i (ancestor exclusion);
    // opaque_bg[i] = node i paints a fully opaque background over its own
    // rect (blit eligibility for movers).
    parents: &[Option<usize>],
    opaque_bg: &[bool],
) -> Option<PatchMoveSummary> {
    const EPS: f32 = 0.01;
    const MAX_EXCEPTIONS: usize = 64;
    let n = prev_positions
        .len()
        .min(new_positions.len())
        .min(prev_sizes.len())
        .min(new_sizes.len());

    // Group movers by exact delta bits, weighted by new area.
    let mut groups: HashMap<(u32, u32), f32> = HashMap::new();
    for i in 0..n {
        if reemit.contains(&i) {
            continue;
        }
        let dx = new_positions[i].x - prev_positions[i].x;
        let dy = new_positions[i].y - prev_positions[i].y;
        if dx.abs() < EPS && dy.abs() < EPS {
            continue;
        }
        let area = new_sizes[i].map_or(1.0, |s| (s.width * s.height).max(1.0));
        *groups.entry((dx.to_bits(), dy.to_bits())).or_insert(0.0) += area;
    }
    let ((dxb, dyb), _) = groups
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(core::cmp::Ordering::Equal))?;
    let dominant = LogicalPosition {
        x: f32::from_bits(dxb),
        y: f32::from_bits(dyb),
    };

    let rect_of = |pos: LogicalPosition, size: Option<LogicalSize>| -> Option<LogicalRect> {
        let s = size?;
        if s.width <= 0.0 || s.height <= 0.0 || !pos.x.is_finite() || !pos.y.is_finite() {
            return None;
        }
        Some(LogicalRect {
            origin: pos,
            size: s,
        })
    };
    let union = |a: Option<LogicalRect>, b: LogicalRect| -> LogicalRect {
        match a {
            None => b,
            Some(a) => {
                let x0 = a.origin.x.min(b.origin.x);
                let y0 = a.origin.y.min(b.origin.y);
                let x1 = (a.origin.x + a.size.width).max(b.origin.x + b.size.width);
                let y1 = (a.origin.y + a.size.height).max(b.origin.y + b.size.height);
                LogicalRect {
                    origin: LogicalPosition { x: x0, y: y0 },
                    size: LogicalSize {
                        width: x1 - x0,
                        height: y1 - y0,
                    },
                }
            }
        }
    };
    let intersects = |a: &LogicalRect, b: &LogicalRect| -> bool {
        a.origin.x < b.origin.x + b.size.width
            && b.origin.x < a.origin.x + a.size.width
            && a.origin.y < b.origin.y + b.size.height
            && b.origin.y < a.origin.y + a.size.height
    };

    // Pass 1: dominant movers. Only OPAQUE-background movers may blit;
    // the rest become exceptions in pass 2. Keep only TOP-LEVEL mover
    // rects (drop movers whose ancestor is already a collected mover —
    // the ancestor's blit carries them).
    const MAX_MOVERS: usize = 32;
    let mut is_dominant_mover = vec![false; n];
    for i in 0..n {
        if reemit.contains(&i) {
            continue;
        }
        let dx = new_positions[i].x - prev_positions[i].x;
        let dy = new_positions[i].y - prev_positions[i].y;
        if (dx - dominant.x).abs() < EPS
            && (dy - dominant.y).abs() < EPS
            && (dx.abs() >= EPS || dy.abs() >= EPS)
        {
            is_dominant_mover[i] = true;
        }
    }
    let covered_by_mover_ancestor = |i: usize| -> bool {
        let mut cur = parents.get(i).copied().flatten();
        while let Some(p) = cur {
            if is_dominant_mover.get(p).copied().unwrap_or(false)
                && opaque_bg.get(p).copied().unwrap_or(false)
            {
                return true;
            }
            cur = parents.get(p).copied().flatten();
        }
        false
    };
    let mut mover_rects_old: Vec<LogicalRect> = Vec::new();
    let mut moved_region: Option<LogicalRect> = None;
    for i in 0..n {
        if !is_dominant_mover[i]
            || !opaque_bg.get(i).copied().unwrap_or(false)
            || covered_by_mover_ancestor(i)
        {
            continue;
        }
        if let Some(r) = rect_of(prev_positions[i], prev_sizes[i]) {
            mover_rects_old.push(r);
            if mover_rects_old.len() > MAX_MOVERS {
                return None; // too many independent blits — full repaint
            }
            moved_region = Some(union(moved_region, r));
        }
    }
    let moved_region_old = moved_region?;
    // Descendant-of-blitted-mover test for pass 2: a static (or any) node
    // whose OPAQUE mover ancestor blits is carried by that blit and needs
    // no exception; equally, dominant movers inside a blitted ancestor are
    // fine. Everything else that touches a mover rect must repaint.
    let inside_blit = |i: usize| -> bool {
        is_dominant_mover[i] && opaque_bg.get(i).copied().unwrap_or(false)
            || covered_by_mover_ancestor(i)
    };

    // Pass 2: exceptions — re-emitted nodes, off-delta movers, non-opaque
    // dominant movers outside any blitted ancestor, and NON-ANCESTOR
    // static nodes overlapping a blitted mover. Ancestors of a mover paint
    // BELOW it (covered by the mover's opaque background) — no exception.
    let is_ancestor_of_any_mover = |i: usize| -> bool {
        // ancestor test via the parent chains of the collected movers is
        // O(movers × depth); movers are ≤32 and depth is small.
        for (j, dom) in is_dominant_mover.iter().enumerate() {
            if !dom || !opaque_bg.get(j).copied().unwrap_or(false) {
                continue;
            }
            let mut cur = parents.get(j).copied().flatten();
            while let Some(p) = cur {
                if p == i {
                    return true;
                }
                cur = parents.get(p).copied().flatten();
            }
        }
        false
    };
    let mut exceptions: Vec<LogicalRect> = Vec::new();
    for i in 0..n {
        let dx = new_positions[i].x - prev_positions[i].x;
        let dy = new_positions[i].y - prev_positions[i].y;
        let is_static = dx.abs() < EPS && dy.abs() < EPS;
        let old_r = rect_of(prev_positions[i], prev_sizes[i]);
        let new_r = rect_of(new_positions[i], new_sizes[i]);
        let needs_exception = if reemit.contains(&i) {
            // NOT an exception: a re-emitted node's items genuinely differ
            // (fresh content / new size), so the translated DIFF damages
            // them itself — old∪new bounds — which also repaints over any
            // pixels a blit dragged through them. Listing them here
            // double-covered the damage and, for the size-changed ROOT
            // (width:100% on every resize), turned the whole viewport into
            // an exception — the blit was decorative and the NC could not
            // go red.
            false
        } else if inside_blit(i) {
            false
        } else if is_static {
            // Static content BELOW the movers (their ancestors) is covered
            // by the opaque blits; anything else touching a mover rect
            // would be visibly dragged or stamped over — repaint it.
            !is_ancestor_of_any_mover(i)
                && old_r.is_some_and(|r| mover_rects_old.iter().any(|m| intersects(&r, m)))
        } else {
            true // off-delta mover (or non-opaque dominant outside a blit)
        };
        if needs_exception {
            let mut acc: Option<LogicalRect> = None;
            if let Some(r) = old_r {
                acc = Some(union(acc, r));
            }
            if let Some(r) = new_r {
                acc = Some(union(acc, r));
            }
            if let Some(r) = acc {
                exceptions.push(r);
                if exceptions.len() > MAX_EXCEPTIONS {
                    return None; // too fragmented — full repaint is cheaper
                }
            }
        }
    }

    Some(PatchMoveSummary {
        dominant_delta: dominant,
        mover_rects_old,
        moved_region_old,
        exceptions,
    })
}

impl<'a> PatchState<'a> {
    pub(crate) fn build(
        prev: &'a DisplayList,
        prev_positions: &[LogicalPosition],
        new_positions: &[LogicalPosition],
        prev_sizes: &[Option<LogicalSize>],
        new_sizes: &[Option<LogicalSize>],
        reflowed_ifcs: &std::collections::BTreeSet<usize>,
    ) -> Option<Self> {
        // The tagging is as old as the patcher — a cached list from before
        // the tagging (or a page slice) cannot be patched.
        if prev.layout_node_mapping.len() != prev.items.len() {
            return None;
        }
        let n = new_positions.len().min(prev_positions.len());

        let mut reemit: std::collections::BTreeSet<usize> = reflowed_ifcs.clone();
        for i in 0..n.min(prev_sizes.len()).min(new_sizes.len()) {
            if prev_sizes[i] != new_sizes[i] {
                reemit.insert(i);
            }
        }

        let mut deltas = vec![LogicalPosition::zero(); new_positions.len()];
        for i in 0..n {
            deltas[i] = LogicalPosition {
                x: new_positions[i].x - prev_positions[i].x,
                y: new_positions[i].y - prev_positions[i].y,
            };
        }

        // Contiguous same-(node,phase) runs, in list order.
        let mut runs: HashMap<(usize, u8), std::collections::VecDeque<(usize, usize)>> =
            HashMap::new();
        let mut i = 0usize;
        while i < prev.items.len() {
            let tag = prev.layout_node_mapping[i];
            let start = i;
            while i < prev.items.len() && prev.layout_node_mapping[i] == tag {
                i += 1;
            }
            if let Some((node, phase)) = tag {
                if !matches!(phase, EmitPhase::ScWalk) {
                    if prev.items[start..i].iter().all(patchable_item) {
                        runs.entry((node, phase as u8))
                            .or_default()
                            .push_back((start, i));
                    } else {
                        // The run carries state beyond translatable geometry
                        // (virtual view, filter, …) — this node re-emits.
                        reemit.insert(node);
                    }
                }
            }
        }

        Some(Self {
            prev,
            runs,
            deltas,
            reemit,
        })
    }
}

// +spec:stacking-contexts:9e85a3 - Stacking context tree: hierarchical, nested, atomic painting order
/// Represents a node in the CSS stacking context tree, not the DOM tree.
#[derive(Debug)]
struct StackingContext {
    node_index: usize,
    z_index: i32,
    child_contexts: Vec<StackingContext>,
    /// Children that do not create their own stacking contexts and are painted in DOM order.
    in_flow_children: Vec<usize>,
}

impl<'a, 'b, T> DisplayListGenerator<'a, 'b, T>
where
    T: ParsedFontTrait + Sync + 'static,
{
    pub(crate) const fn new(
        ctx: &'a mut LayoutContext<'b, T>,
        scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
        positioned_tree: &'a PositionedTree<'a>,
        scroll_ids: &'a HashMap<LayoutNodeId, u64>,
        gpu_value_cache: Option<&'a GpuValueCache>,
        renderer_resources: &'a RendererResources,
        id_namespace: IdNamespace,
        dom_id: DomId,
    ) -> Self {
        Self {
            ctx,
            scroll_offsets,
            positioned_tree,
            scroll_ids,
            gpu_value_cache,
            renderer_resources,
            id_namespace,
            dom_id,
            patch: None,
        }
    }

    /// Prove the SOLID OPAQUE color a text run inside this IFC sits on, or
    /// `None`. Correct-by-construction: only claims a background when the
    /// nearest layout ancestor that paints ANY background paints exactly one
    /// solid opaque color (no gradients/images, no translucency) and no
    /// selection touches the node (selection rects paint between background
    /// and glyphs). No claim for canvas-direct text — the clear color is a
    /// renderer detail (`AZ_DEBUG_FILL` can change it).
    fn compute_uniform_text_bg(
        &self,
        source_node_index: usize,
    ) -> Option<(ColorU, WindowLogicalRect)> {
        let tree = self.positioned_tree.tree;
        let dom_id_opt = tree
            .get(LayoutNodeId::new(source_node_index))
            .and_then(|n| n.dom_node_id);
        if let (Some(sel), Some(dom_id)) = (
            self.ctx.text_selections.get(&self.ctx.styled_dom.dom_id),
            dom_id_opt,
        ) {
            if sel.affected_nodes.contains_key(&dom_id) {
                return None;
            }
        }
        let mut cur = Some(source_node_index);
        while let Some(idx) = cur {
            if let Some(dom_id) = tree.get(LayoutNodeId::new(idx)).and_then(|n| n.dom_node_id) {
                let state = self.get_styled_node_state(dom_id);
                let contents = get_background_contents(self.ctx.styled_dom, dom_id, &state);
                let mut saw_any = false;
                for c in &contents {
                    use azul_css::props::style::StyleBackgroundContent as B;
                    match c {
                        B::Color(col) if col.a == 255 => {
                            // The PROOF's extent: this node's painted rect.
                            // The LCD fringe of a glyph at the very edge of
                            // it spills onto UNPROVEN pixels — the raster
                            // must sweep those glyphs (a pre-blended tile
                            // stamped across the boundary bakes the wrong
                            // neighbour into the fringe; caught by the
                            // round-3 blit gate as an 8-pixel divergence at
                            // the page edge).
                            let rect = self.get_paint_rect(idx)?;
                            return Some((*col, rect.into()));
                        }
                        B::Color(col) if col.a == 0 => {}
                        // translucent color, gradient or image — unprovable
                        _ => return None,
                    }
                    saw_any = true;
                }
                let _ = saw_any;
            }
            cur = tree.get(LayoutNodeId::new(idx)).and_then(|n| n.parent);
        }
        None
    }

    /// DL-PATCHING substitution: if a patch source is armed and this node's
    /// (phase) run can be reused, copy it translated by the node's position
    /// delta and report `true` (the caller skips painting). `false` = paint
    /// normally (no patch, node in the re-emit set, or no cached run left —
    /// the last one falls back to a fresh paint, which is always correct).
    fn try_copy_cached_run(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        phase: EmitPhase,
    ) -> bool {
        let Some(patch) = self.patch.as_mut() else {
            return false;
        };
        if patch.reemit.contains(&node_index) {
            return false;
        }
        let Some(run) = patch
            .runs
            .get_mut(&(node_index, phase as u8))
            .and_then(std::collections::VecDeque::pop_front)
        else {
            // No cached run for this call. For a node that emitted nothing
            // last pass this is the COMMON case — and emitting nothing again
            // is exactly right, so treat it as a successful (empty) copy.
            // A node that DID emit but whose run count changed cannot occur
            // without a reflow/size change (both force re-emit).
            drop(crate::probe::Probe::span("dl_patch_empty"));
            return true;
        };
        let delta = patch
            .deltas
            .get(node_index)
            .copied()
            .unwrap_or_else(LogicalPosition::zero);
        drop(crate::probe::Probe::span("dl_patch_copy"));
        // DOM attribution is PER ITEM, not per run: a content run can switch
        // attribution mid-run (e.g. an IFC root's items ending with a text
        // child's id) — the golden test caught a first-item flattening here.
        let saved_node = builder.current_node;
        builder.set_current_layout(Some((node_index, phase)));
        for i in run.0..run.1 {
            builder.set_current_node(patch.prev.node_mapping.get(i).copied().flatten());
            builder.next_text_bg = patch.prev.uniform_text_bgs.get(i).copied().flatten();
            let item = translate_item(patch.prev.items[i].clone(), delta);
            builder.push_item(item);
        }
        builder.set_current_layout(None);
        builder.set_current_node(saved_node);
        true
    }

    /// Helper to get styled node state for a node
    fn get_styled_node_state(&self, dom_id: NodeId) -> azul_core::styled_dom::StyledNodeState {
        self.ctx
            .styled_dom
            .styled_nodes
            .as_container()
            .get(dom_id)
            .map(|n| n.styled_node_state)
            .unwrap_or_default()
    }

    // +spec:overflow:visibility - CSS 2.2 §11.2: visibility:hidden makes the box invisible
    // but still affects layout. Checked per-node because visibility is inherited and a child
    // with visibility:visible inside a hidden parent must still be painted.
    fn is_node_hidden(&self, node_index: usize) -> bool {
        use azul_css::props::style::effects::StyleVisibility;
        let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(node_index)) else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };
        let node_state = self.get_styled_node_state(dom_id);
        matches!(
            get_visibility(self.ctx.styled_dom, dom_id, &node_state),
            crate::solver3::getters::MultiValue::Exact(
                StyleVisibility::Hidden | StyleVisibility::Collapse
            )
        )
    }

    /// Gets the cursor type for a text node from its CSS properties.
    /// Defaults to Text (I-beam) cursor if no explicit cursor is set.
    #[allow(clippy::match_same_arms)] // enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
    fn get_cursor_type_for_text_node(&self, node_id: NodeId) -> CursorType {
        use azul_css::props::style::effects::StyleCursor;

        let styled_node_state = self.get_styled_node_state(node_id);
        let node_data_container = self.ctx.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id);

        // Query the cursor CSS property for this text node
        if let Some(node_data) = node_data {
            if let Some(CssPropertyValue::Exact(cursor)) = self
                .ctx
                .styled_dom
                .get_css_property_cache()
                .get_cursor(node_data, &node_id, &styled_node_state)
            {
                return match cursor {
                    StyleCursor::Default => CursorType::Default,
                    StyleCursor::Pointer => CursorType::Pointer,
                    StyleCursor::Text => CursorType::Text,
                    StyleCursor::Crosshair => CursorType::Crosshair,
                    StyleCursor::Move => CursorType::Move,
                    StyleCursor::Help => CursorType::Help,
                    StyleCursor::Wait => CursorType::Wait,
                    StyleCursor::Progress => CursorType::Progress,
                    StyleCursor::NsResize => CursorType::NsResize,
                    StyleCursor::EwResize => CursorType::EwResize,
                    StyleCursor::NeswResize => CursorType::NeswResize,
                    StyleCursor::NwseResize => CursorType::NwseResize,
                    StyleCursor::NResize => CursorType::NResize,
                    StyleCursor::SResize => CursorType::SResize,
                    StyleCursor::EResize => CursorType::EResize,
                    StyleCursor::WResize => CursorType::WResize,
                    StyleCursor::Grab => CursorType::Grab,
                    StyleCursor::Grabbing => CursorType::Grabbing,
                    StyleCursor::RowResize => CursorType::RowResize,
                    StyleCursor::ColResize => CursorType::ColResize,
                    // Map less common cursors to closest available
                    StyleCursor::SeResize | StyleCursor::NeswResize => CursorType::NeswResize,
                    StyleCursor::ZoomIn | StyleCursor::ZoomOut => CursorType::Default,
                    StyleCursor::Copy | StyleCursor::Alias => CursorType::Default,
                    StyleCursor::Cell => CursorType::Crosshair,
                    StyleCursor::AllScroll => CursorType::Move,
                    StyleCursor::ContextMenu => CursorType::Default,
                    StyleCursor::VerticalText => CursorType::Text,
                    StyleCursor::Unset => CursorType::Text, // Default to text for text nodes
                };
            }
        }

        // Default: Text cursor (I-beam) for text nodes
        CursorType::Text
    }

    /// Emits drawing commands for text selections only (not cursor).
    /// The cursor is drawn separately via `paint_cursor()`.
    fn paint_selections(&self, builder: &mut DisplayListBuilder, node_index: usize) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        // Get inline layout using the unified helper that handles IFC membership
        // This is critical: text nodes don't have their own inline_layout_result,
        // but they have ifc_membership pointing to their IFC root
        // (d6h) Materialized: sentinel-safe caret/selection geometry.
        let Some(layout) = self
            .positioned_tree
            .tree
            .materialized_inline_layout_for_node(node_index)
        else {
            return Ok(());
        };

        // Get the absolute position of this node (border-box position)
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(node_index)
            .copied()
            .unwrap_or_default();

        // Selection rects from `get_selection_rects` are in the IFC root's content-box
        // coordinate space. For an inline text node, the node's OWN box position is never
        // assigned (stays the `f32::MIN` sentinel), so we must anchor to the IFC root's
        // position + padding/border — exactly the box that owns the inline layout. (The
        // caret avoids this because paint_cursor runs on the IFC-root node directly.)
        let anchor = self.ifc_content_box_origin(node_index, node_pos);
        let content_box_offset_x = anchor.x;
        let content_box_offset_y = anchor.y;

        // Check if text is selectable (respects CSS user-select property)
        let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let is_selectable =
            super::getters::is_text_selectable(self.ctx.styled_dom, dom_id, node_state);
        if !is_selectable {
            return Ok(());
        }

        // === NEW: Check text_selections first (multi-node selection support) ===
        if let Some(text_selection) = self.ctx.text_selections.get(&self.ctx.styled_dom.dom_id) {
            if let Some(ranges) = text_selection.affected_nodes.get(&dom_id) {
                let is_collapsed = text_selection.is_collapsed();
                // Only draw selection highlight if NOT collapsed
                if !is_collapsed {
                    // A multi-cursor session (Ctrl+D) puts EVERY occurrence on
                    // this one node, so every range gets its own rects.
                    let rects: Vec<LogicalRect> = ranges
                        .iter()
                        .flat_map(|range| layout.get_selection_rects(range))
                        .collect();
                    let style = get_selection_style(
                        self.ctx.styled_dom,
                        Some(dom_id),
                        self.ctx.system_style.as_ref(),
                    );

                    let border_radius = BorderRadius {
                        top_left: style.radius,
                        top_right: style.radius,
                        bottom_left: style.radius,
                        bottom_right: style.radius,
                    };

                    for mut rect in rects {
                        rect.origin.x += content_box_offset_x;
                        rect.origin.y += content_box_offset_y;
                        builder.push_selection_rect(rect, style.bg_color, border_radius);
                    }
                }

                return Ok(());
            }
        }

        Ok(())
    }

    /// Selection rects covering this IFC, already translated into the same
    /// space as the offset glyph positions, plus the `::selection` text colour
    /// — or `None` when there is nothing to recolour.
    ///
    /// Resolves the selection through `ifc_root_owns_dom_node` rather than off
    /// the IFC root's own `dom_node_id`: an editing session keys `affected_nodes`
    /// on the TEXT node (`initialize_editing` puts the caret there), while this
    /// pass runs on the root that owns the inline layout. A cross-block
    /// selection keys on the block, which is its own IFC root — both resolve.
    /// The `user-select` gate matches `paint_selections` on purpose: the
    /// recolour must cover exactly the glyphs the highlight covers.
    /// The origin of the content box that owns an IFC's inline layout.
    ///
    /// Selection rects come out of `get_selection_rects` in this space. An
    /// inline text node's own box position is never assigned (it keeps the
    /// `f32::MIN` sentinel), so the anchor has to be the IFC ROOT's box — which
    /// is why both the highlight and the recolour resolve it here rather than
    /// each computing its own.
    fn ifc_content_box_origin(
        &self,
        node_index: usize,
        fallback: LogicalPosition,
    ) -> LogicalPosition {
        let ifc_root_index = self
            .positioned_tree
            .tree
            .get_ifc_root_layout_index(node_index);
        let pos = self
            .positioned_tree
            .calculated_positions
            .get(ifc_root_index)
            .copied()
            .unwrap_or(fallback);
        let bp = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(ifc_root_index))
            .map(|n| n.box_props.unpack());
        let (pad_left, pad_top, bor_left, bor_top) = bp.map_or((0.0, 0.0, 0.0, 0.0), |b| {
            (b.padding.left, b.padding.top, b.border.left, b.border.top)
        });
        LogicalPosition::new(pos.x + pad_left + bor_left, pos.y + pad_top + bor_top)
    }

    fn selection_recolour_for_ifc(
        &self,
        source_node_index: usize,
    ) -> Option<(Vec<LogicalRect>, ColorU)> {
        let sel = self.ctx.text_selections.get(&self.ctx.styled_dom.dom_id)?;
        if sel.is_collapsed() {
            return None;
        }
        let (dom_id, ranges) = sel
            .affected_nodes
            .iter()
            .find(|&(node, _)| self.ifc_root_owns_dom_node(source_node_index, *node))?;
        let node_state = self.get_styled_node_state(*dom_id);
        if !super::getters::is_text_selectable(self.ctx.styled_dom, *dom_id, &node_state) {
            return None;
        }
        let style = get_selection_style(
            self.ctx.styled_dom,
            Some(*dom_id),
            self.ctx.system_style.as_ref(),
        );
        let text_color = style.text_color?;
        // The SAME geometry the highlight is painted from: the sentinel-safe
        // materialized layout, anchored on the IFC root's content box. Reading
        // the raw cached layout here yielded no rects at all, so the recolour
        // silently never applied.
        let layout = self
            .positioned_tree
            .tree
            .materialized_inline_layout_for_node(source_node_index)?;
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(source_node_index)
            .copied()
            .unwrap_or_default();
        let anchor = self.ifc_content_box_origin(source_node_index, node_pos);
        let rects: Vec<LogicalRect> = ranges
            .iter()
            .flat_map(|r| layout.get_selection_rects(r))
            .map(|mut r| {
                r.origin.x += anchor.x;
                r.origin.y += anchor.y;
                r
            })
            .collect();
        if rects.is_empty() {
            return None;
        }
        Some((rects, text_color))
    }

    /// Does the IFC rooted at `node_index` own the inline content of `dom_id`?
    ///
    /// The editing session's text node need not be a DIRECT child of the IFC
    /// root — `p > span > text` puts it one level deeper — so resolve it through
    /// `ifc_membership`, which every inline descendant of the root carries,
    /// instead of scanning the root's children.
    fn ifc_root_owns_dom_node(&self, node_index: usize, dom_id: NodeId) -> bool {
        let tree = self.positioned_tree.tree;
        if let Some(indices) = tree.dom_to_layout.get(&dom_id) {
            return indices
                .iter()
                .any(|&idx| tree.get_ifc_root_layout_index(idx.index()) == node_index);
        }
        // The node generated NO box of its own — an EMPTY text node is
        // filtered out of the layout tree — so it belongs to the IFC of the
        // nearest DOM ancestor that did generate one. This is the focused
        // empty editable: the caret session sits on the value's empty text
        // node, and the strut line box lives on its `<p>`.
        let hierarchy = self.ctx.styled_dom.node_hierarchy.as_container();
        let mut current = hierarchy
            .get(dom_id)
            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
        while let Some(parent) = current {
            if let Some(indices) = tree.dom_to_layout.get(&parent) {
                return indices.iter().any(|&idx| {
                    idx.index() == node_index
                        || tree.get_ifc_root_layout_index(idx.index()) == node_index
                });
            }
            current = hierarchy
                .get(parent)
                .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
        }
        false
    }

    /// Emits drawing commands for all text cursors (carets).
    ///
    /// Iterates over `ctx.cursor_locations` to support multi-cursor rendering.
    /// An editable with NO text gets the strut caret of
    /// [`empty_editable_caret_rect`].
    /// Preedit underline is only rendered for the primary (last) cursor.
    #[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    fn paint_cursor(&self, builder: &mut DisplayListBuilder, node_index: usize) -> Result<()> {
        // NOTE: we deliberately do NOT early-return in the blink-off phase. Emitting the
        // caret item every frame — with alpha forced to 0 when invisible (see caret_color
        // below) — keeps the display-list item COUNT stable across blink phases, so
        // compute_display_list_damage yields a tiny caret-sized damage rect instead of
        // bailing to a full-window repaint on every ~530ms blink toggle.

        // Early exit if no cursor locations
        if self.ctx.cursor_locations.is_empty() {
            return Ok(());
        }

        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        // Check if this node is contenteditable
        let is_contenteditable =
            super::getters::is_node_contenteditable_inherited(self.ctx.styled_dom, dom_id);
        if !is_contenteditable {
            return Ok(());
        }

        // Check if text is selectable
        let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let is_selectable =
            super::getters::is_text_selectable(self.ctx.styled_dom, dom_id, node_state);
        if !is_selectable {
            return Ok(());
        }

        // Get inline layout
        // (d6h) Materialized: sentinel-safe caret/selection geometry.
        let Some(layout) = self
            .positioned_tree
            .tree
            .materialized_inline_layout_for_node(node_index)
        else {
            return Ok(());
        };

        // Compute content-box offset once
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(node_index)
            .copied()
            .unwrap_or_default();
        let bp = node.box_props.unpack();
        let padding = &bp.padding;
        let border = &bp.border;
        let content_box_offset_x = node_pos.x + padding.left + border.left;
        let content_box_offset_y = node_pos.y + padding.top + border.top;

        let style = get_caret_style(self.ctx.styled_dom, Some(dom_id));

        // Find the index of the last (primary) cursor that belongs to this DOM/node,
        // so preedit underline is only drawn on the actual primary cursor.
        let primary_idx_for_this_node = self
            .ctx
            .cursor_locations
            .iter()
            .enumerate()
            .rev()
            .find(|(_, loc)| {
                loc.dom == self.ctx.styled_dom.dom_id
                    && (loc.node == dom_id || self.ifc_root_owns_dom_node(node_index, loc.node))
            })
            .map(|(i, _)| i);

        for (i, location) in self.ctx.cursor_locations.iter().enumerate() {
            let cursor = &location.cursor;
            // Check DOM ID matches
            if self.ctx.styled_dom.dom_id != location.dom {
                continue;
            }

            // Check this node contains the cursor
            if dom_id != location.node
                && !self.ifc_root_owns_dom_node(node_index, location.node)
            {
                continue;
            }

            // Get cursor rect from text layout — or, for an EMPTY editable,
            // the strut caret. `get_cursor_rect` anchors the caret to a
            // cluster, and an editable with no text has none, so nothing was
            // painted: a focused, empty TextInput showed no caret at all
            // (and, with the placeholder hidden on focus, nothing else
            // either — the field looked dead). The first character will
            // create a line box at the content-box origin one line-height
            // tall; the caret stands where that line will be.
            let rect = match layout.get_cursor_rect(cursor) {
                Some(rect) => rect,
                None if layout.items.is_empty() => {
                    let font_size = super::getters::get_element_font_size(
                        self.ctx.styled_dom,
                        dom_id,
                        node_state,
                    );
                    let line_height = super::getters::get_line_height_value(
                        self.ctx.styled_dom,
                        dom_id,
                        node_state,
                    )
                    .map_or(1.2, |lh| lh.inner.normalized());
                    empty_editable_caret_rect(font_size, line_height)
                }
                None => continue,
            };
            let mut rect = rect;

            rect.origin.x += content_box_offset_x;
            rect.origin.y += content_box_offset_y;
            rect.size.width = style.width;

            // Blink: keep the caret item present every frame (stable item count for
            // incremental damage) but make it invisible in the off phase by zeroing alpha.
            // PER-PARTICIPANT COLOUR (U1). A shared editing session paints
            // each person's caret in their own colour; `caret-color` answers
            // "what colour is the caret in this field", which is a different
            // question and cannot name a person. An owner the app has not
            // registered - the local caret included - falls back to the CSS
            // value, so a single-user app is unaffected.
            let owner_color = self
                .ctx
                .owner_colors
                .get(&location.owner)
                .copied()
                .unwrap_or(style.color);
            let caret_color = if self.ctx.cursor_is_visible {
                owner_color
            } else {
                ColorU {
                    a: 0,
                    ..owner_color
                }
            };
            builder.push_cursor_rect(rect, caret_color);

            // Preedit underline only on the primary cursor for this node
            let is_primary = primary_idx_for_this_node == Some(i);
            if is_primary {
                if let Some(ref preedit) = self.ctx.preedit_text {
                    if !preedit.is_empty() {
                        // The composition IS in this layout: apply_preedit_to_
                        // text_cache splices it into the run at the cursor's
                        // byte offset and re-shapes, so the underline can span
                        // the MEASURED advance of the composed clusters. The
                        // old `char_count × max(caret_width, 8px)` guess was
                        // wrong for every script whose advances aren't ~8px —
                        // a CJK cluster is roughly double that.
                        let run = cursor.cluster_id.source_run;
                        let start_byte = cursor.cluster_id.start_byte_in_run;
                        let end_byte = start_byte
                            .saturating_add(u32::try_from(preedit.len()).unwrap_or(u32::MAX));
                        // (line_index, min_x, max_x) of the composed clusters.
                        // Clusters on a later line belong to a composition that
                        // wrapped; they would need their own rect, so the span
                        // stays on the line the caret sits on.
                        let mut span: Option<(usize, f32, f32)> = None;
                        for item in &layout.items {
                            let Some(cluster) = item.item.as_cluster() else {
                                continue;
                            };
                            let id = cluster.source_cluster_id;
                            if id.source_run != run
                                || id.start_byte_in_run < start_byte
                                || id.start_byte_in_run >= end_byte
                            {
                                continue;
                            }
                            let edge = item.position.x + cluster.advance;
                            let (lo, hi) = (item.position.x.min(edge), item.position.x.max(edge));
                            span = Some(match span {
                                Some((line, min_x, max_x)) if line == item.line_index => {
                                    (line, min_x.min(lo), max_x.max(hi))
                                }
                                Some(other_line) => other_line,
                                None => (item.line_index, lo, hi),
                            });
                        }
                        let (underline_x, preedit_width) = if let Some((_, lo, hi)) = span {
                            (lo + content_box_offset_x, hi - lo)
                        } else {
                            // Composition not (yet) in the cache — keep the
                            // old estimate rather than drawing nothing.
                            let char_count = preedit.chars().count() as f32;
                            (
                                rect.origin.x + rect.size.width,
                                char_count * style.width.max(8.0),
                            )
                        };
                        let underline_bounds = LogicalRect {
                            origin: LogicalPosition {
                                x: underline_x,
                                y: rect.origin.y + rect.size.height - 2.0,
                            },
                            size: LogicalSize {
                                width: preedit_width,
                                height: 2.0,
                            },
                        };
                        builder.push_underline(underline_bounds, style.color, 2.0);
                    }
                }
            }
        }

        Ok(())
    }

    /// Emits drawing commands for selection and cursor.
    /// Delegates to `paint_selections()` and `paint_cursor()`.
    /// Recursively builds the tree of stacking contexts starting from a given layout node.
    // +spec:writing-modes:a86a28 - preorder depth-first traversal of the rendering tree in logical order
    fn collect_stacking_contexts(&mut self, node_index: usize) -> Result<StackingContext> {
        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
        let z_index = get_z_index(self.ctx.styled_dom, node.dom_node_id);

        if let Some(dom_id) = node.dom_node_id {
            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(
                self.ctx,
                "Collecting stacking context for node {} ({:?}), z-index={}",
                node_index,
                node_type.get_node_type(),
                z_index
            );
        }

        let mut child_contexts = Vec::new();
        let mut in_flow_children = Vec::new();

        for &child_index in self.positioned_tree.tree.children(node_index) {
            if self.establishes_stacking_context(child_index) {
                child_contexts.push(self.collect_stacking_contexts(child_index)?);
            } else {
                in_flow_children.push(child_index);
                // Recurse into non-stacking-context children to find nested
                // stacking contexts. Per CSS 2.2 Appendix E, these are promoted
                // to be child stacking contexts of the nearest ancestor SC.
                self.find_nested_stacking_contexts(child_index, &mut child_contexts)?;
            }
        }

        Ok(StackingContext {
            node_index,
            z_index,
            child_contexts,
            in_flow_children,
        })
    }

    /// Recursively searches non-stacking-context subtrees for nested stacking
    /// contexts, promoting them to the parent stacking context's child list.
    fn find_nested_stacking_contexts(
        &mut self,
        parent_index: usize,
        child_contexts: &mut Vec<StackingContext>,
    ) -> Result<()> {
        for &child_index in self.positioned_tree.tree.children(parent_index) {
            if self.establishes_stacking_context(child_index) {
                child_contexts.push(self.collect_stacking_contexts(child_index)?);
            } else {
                self.find_nested_stacking_contexts(child_index, child_contexts)?;
            }
        }
        Ok(())
    }

    // +spec:box-model:de94ab - stacking context painting order (negative z, in-flow, z=0, positive z)
    // +spec:display-property:337069 - CSS 2.2 E.2 painting order: stacking contexts sorted by z-index, in-flow children in tree order
    // +spec:display-property:7b0a87 - CSS 2.2 E.2 painting order: negative z-index, in-flow, z-index 0/auto, positive z-index
    // +spec:stacking-contexts:5cbdfb - full CSS painting order (bg, neg-z, in-flow, z0, pos-z)
    // +spec:stacking-contexts:3ded3a - CSS 2.2 Appendix E painting order: definitions and tree order traversal
    // +spec:stacking-contexts:973368 - CSS 2.2 Appendix E.2 painting order: bg/border, negative z, in-flow, zero z, positive z
    // +spec:stacking-contexts:464bb7 - CSS 2.2 §9.9.1 painting order: negative z-index, in-flow, z-index 0, positive z-index (recursive)
    /// Recursively traverses the stacking context tree, emitting drawing commands to the builder
    /// according to the CSS Painting Algorithm specification.
    // +spec:display-property:39e879 - CSS 2.2 E.2 painting order for block-level and inline-level elements
    // +spec:display-property:de4c66 - CSS 2.2 E.2 stacking context paint order (canvas bg, negative z, in-flow, floats, inline, positive z)
    // +spec:overflow:6e48b4 - CSS 2.2 Appendix E painting order: bg/border, negative z-index, in-flow, floats, z-index 0/auto, positive z-index
    // +spec:stacking-contexts:55ca96 - CSS 2.2 E.2 painting order: backgrounds, negative z-index, in-flow, z-index 0/auto, positive z-index
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn generate_for_stacking_context(
        &mut self,
        builder: &mut DisplayListBuilder,
        context: &StackingContext,
    ) -> Result<()> {
        // Before painting the node, check if it establishes a new clip or scroll frame.
        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(context.node_index))
            .ok_or(LayoutError::InvalidTree)?;

        if let Some(dom_id) = node.dom_node_id {
            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(
                self.ctx,
                "Painting stacking context for node {} ({:?}), z-index={}, {} child contexts, {} \
                 in-flow children",
                context.node_index,
                node_type.get_node_type(),
                context.z_index,
                context.child_contexts.len(),
                context.in_flow_children.len()
            );
        }

        // Set current node BEFORE pushing stacking context so that
        // the PushStackingContext item gets the correct node_mapping entry.
        // This is critical for drag visual offset matching.
        builder.set_current_node(node.dom_node_id);

        // Track fixed-position elements for paged media replication (CSS Positioned Layout §2.1)
        let is_fixed_position = node.dom_node_id.is_some_and(|dom_id| {
            get_position_type(self.ctx.styled_dom, Some(dom_id)) == LayoutPosition::Fixed
        });
        if is_fixed_position {
            builder.begin_fixed_position_element();
        }

        // Check if this node has a GPU-accelerated transform (CSS transform or drag).
        // If so, wrap in a reference frame so WebRender can animate it on the GPU.
        let has_reference_frame = node.dom_node_id.and_then(|dom_id| {
            self.gpu_value_cache.and_then(|cache| {
                // CSS transform first, then the ANIMATION channel. An
                // engine-driven transition animates nodes that have no CSS
                // `transform` of their own, so without this second lookup they
                // get no reference frame and the element jumps to its
                // destination instead of travelling there.
                let (key, transform) = cache
                    .css_transform_keys
                    .get(&dom_id)
                    .zip(cache.css_current_transform_values.get(&dom_id))
                    .or_else(|| {
                        cache
                            .anim_transform_keys
                            .get(&dom_id)
                            .zip(cache.anim_current_transform_values.get(&dom_id))
                    })?;
                Some((*key, *transform))
            })
        });
        // Push a stacking context for WebRender
        // Get the node's bounds for the stacking context
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(context.node_index)
            .copied()
            .unwrap_or_default();
        let node_size = node.used_size.unwrap_or(LogicalSize {
            width: 0.0,
            height: 0.0,
        });
        let node_bounds = LogicalRect {
            origin: node_pos,
            size: node_size,
        };

        // Push reference frame BEFORE stacking context if node has a transform
        if let Some((transform_key, initial_transform)) = has_reference_frame {
            builder.push_reference_frame(transform_key, initial_transform, node_bounds);
        }

        builder.push_stacking_context(context.z_index, node_bounds);

        // Push opacity/filter effects if the node has them
        let mut pushed_opacity = false;
        let mut pushed_filter = false;
        let mut pushed_backdrop_filter = false;

        if let Some(dom_id) = node.dom_node_id {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Opacity (GPU: fast path via compact cache)
            let opacity =
                crate::solver3::getters::get_opacity(self.ctx.styled_dom, dom_id, node_state);

            // ANIMATED opacity binds a key from the animation channel — the
            // exact twin of `has_reference_frame` for transforms: the list
            // carries the key, the value flows per tick through the GPU cache,
            // and an enter/exit fade needs a layer even while the BAKED CSS
            // opacity is 1.0.
            let anim_opacity_key = self.gpu_value_cache.and_then(|cache| {
                cache
                    .anim_opacity_keys
                    .get(&dom_id)
                    .zip(cache.anim_current_opacity_values.get(&dom_id))
                    .map(|(k, _)| *k)
            });
            if opacity < 1.0 || anim_opacity_key.is_some() {
                builder.push_item(DisplayListItem::PushOpacity {
                    bounds: node_bounds.into(),
                    opacity,
                    opacity_key: anim_opacity_key,
                });
                pushed_opacity = true;
            }

            // Filter
            if let Some(filter_vec_value) = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_filter(node_data, &dom_id, node_state)
            {
                if let Some(filter_vec) = filter_vec_value.get_property() {
                    let filters: Vec<_> = filter_vec.as_ref().to_vec();
                    if !filters.is_empty() {
                        builder.push_item(DisplayListItem::PushFilter {
                            bounds: node_bounds.into(),
                            filters,
                        });
                        pushed_filter = true;
                    }
                }
            }

            // Backdrop filter
            if let Some(backdrop_filter_value) = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_backdrop_filter(node_data, &dom_id, node_state)
            {
                if let Some(filter_vec) = backdrop_filter_value.get_property() {
                    let filters: Vec<_> = filter_vec.as_ref().to_vec();
                    if !filters.is_empty() {
                        builder.push_item(DisplayListItem::PushBackdropFilter {
                            bounds: node_bounds.into(),
                            filters,
                        });
                        pushed_backdrop_filter = true;
                    }
                }
            }
        }

        // 0b. Push image mask clip if this node has one.
        // This wraps background, border, and all children so the SVG mask clips everything.
        let did_push_image_mask = self.push_image_mask_clip(builder, context.node_index);

        // +spec:box-model:84b238 - CSS 2.2 E.2 painting order: bg/border, negative z, in-flow, z=0, positive z
        // 1. Paint background and borders for the context's root element.
        // This must be BEFORE push_node_clips so the container background
        // is rendered in parent space (stationary), not scroll space.
        // +spec:overflow:40052b - backgrounds paint at border-box, scrollbars overlay on top (scrollbar-extended background positioning area)
        self.paint_node_background_and_border(builder, context.node_index)?;

        // 1b. For scrollable containers, push the hit-test area BEFORE the scroll frame
        // so the hit-test covers the entire container box (including visible area),
        // not just the scrolled content. This ensures scroll wheel events hit the
        // container regardless of scroll position.
        // +spec:overflow:visibility - visibility:hidden scroll containers must not allow
        // interactive scrolling per CSS 2.2 §11.2
        if !self.is_node_hidden(context.node_index) {
            if let Some(dom_id) = node.dom_node_id {
                let styled_node_state = self.get_styled_node_state(dom_id);
                let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
                let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
                if overflow_x.is_scroll() || overflow_y.is_scroll() {
                    if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, node.dom_node_id) {
                        builder.push_hit_test_area(node_bounds, tag_id);
                    }
                }
            }
        }

        // 2. Push clips and scroll frames AFTER painting background
        // +spec:positioning:ddc554 - overflow clips apply to absolutely positioned descendants
        // when this node is their containing block (stacking contexts painted within clip scope)
        // TODO: CSS Overflow 3 says overflow clips should NOT apply to abs-pos descendants
        // whose containing block is above this clipper. Currently all descendants are clipped.
        // The containing_block_index field on LayoutNode is set for this purpose.
        let did_push_clip_or_scroll = self.push_node_clips(builder, context.node_index, node);

        // +spec:display-contents:434de8 - E.2 painting order: negative z-index, in-flow, z-index 0/auto, positive z-index
        // 3. Paint child stacking contexts with negative z-indices.
        let mut negative_z_children: Vec<_> = context
            .child_contexts
            .iter()
            .filter(|c| c.z_index < 0)
            .collect();
        negative_z_children.sort_by_key(|c| c.z_index);
        for child in negative_z_children {
            self.generate_for_stacking_context(builder, child)?;
        }

        // 4. Paint the in-flow descendants of the context root.
        self.paint_in_flow_descendants(builder, context.node_index, &context.in_flow_children)?;

        // +spec:stacking-contexts:9a4eb3 - z-index:auto/0 positioned descendants painted in tree order
        // 5. Paint child stacking contexts with z-index: 0 / auto.
        for child in context.child_contexts.iter().filter(|c| c.z_index == 0) {
            self.generate_for_stacking_context(builder, child)?;
        }

        // +spec:stacking-contexts:198fa4 - positive z-index stacking contexts painted in z-index order then tree order
        // 6. Paint child stacking contexts with positive z-indices.
        let mut positive_z_children: Vec<_> = context
            .child_contexts
            .iter()
            .filter(|c| c.z_index > 0)
            .collect();

        positive_z_children.sort_by_key(|c| c.z_index);

        for child in positive_z_children {
            self.generate_for_stacking_context(builder, child)?;
        }

        // Pop image mask clip (before filter/opacity since it was pushed after them)
        if did_push_image_mask {
            builder.pop_image_mask_clip();
        }
        // The stroke follows the geometry, not the fill region - it must be
        // outside the mask the fill was painted through.
        self.paint_svg_stroke(builder, context.node_index);

        // Pop filter/opacity effects (in reverse order of push)
        if pushed_backdrop_filter {
            builder.push_item(DisplayListItem::PopBackdropFilter);
        }
        if pushed_filter {
            builder.push_item(DisplayListItem::PopFilter);
        }
        if pushed_opacity {
            builder.push_item(DisplayListItem::PopOpacity);
        }

        // Pop the stacking context for WebRender
        builder.pop_stacking_context();

        // Pop reference frame if we pushed one
        if has_reference_frame.is_some() {
            builder.pop_reference_frame();
        }

        // End fixed-position tracking (records the item range for paged media replication)
        if is_fixed_position {
            builder.end_fixed_position_element();
        }

        // After painting the node and all its descendants, pop any contexts it pushed.
        // For VirtualView nodes, emit the placeholder INSIDE the clip (before PopClip)
        // so the virtualized view viewport is clipped to the container.
        if did_push_clip_or_scroll {
            // Emit VirtualViewPlaceholder before popping the clip so it's inside PushClip/PopClip
            if let Some(dom_id) = node.dom_node_id {
                if self.is_virtual_view_node(dom_id) {
                    builder.push_virtual_view_placeholder(dom_id, node_bounds, node_bounds);
                }
            }
            self.pop_node_clips(builder, node);
        } else {
            // Even without clips, emit VirtualViewPlaceholder for VirtualView nodes
            if let Some(dom_id) = node.dom_node_id {
                if self.is_virtual_view_node(dom_id) {
                    builder.push_virtual_view_placeholder(dom_id, node_bounds, node_bounds);
                }
            }
        }

        // Paint scrollbars AFTER popping the clip, so they appear on top of content
        // and are not clipped by the scroll frame
        self.paint_scrollbars(builder, context.node_index)?;

        Ok(())
    }

    /// Paints the content and non-stacking-context children.
    #[allow(clippy::too_many_lines, clippy::cognitive_complexity)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn paint_in_flow_descendants(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        children_indices: &[usize],
    ) -> Result<()> {
        // NOTE: We do NOT paint the node's background here - that was already done by
        // generate_for_stacking_context! Only paint selection, cursor, and content for the
        // current node

        // 2. Selection highlights go UNDER the content...
        self.paint_selections(builder, node_index)?;

        // 3. Paint the node's own content (text, images, hit-test areas).
        self.paint_node_content(builder, node_index)?;

        // 4. ...and the caret goes OVER it. Emitted before the text, the
        // caret was erased wherever an LCD glyph tile (an opaque pre-blended
        // copy, glyph box + FIR pad) landed on it - the caret segment inside
        // the last typed glyph's box vanished (2026-08-31).
        self.paint_cursor(builder, node_index)?;

        // +spec:display-property:86a3de - inline-level boxes painted in document order; z-index does not apply
        // +spec:floats:b8c494 - E.2 painting order: non-positioned floats painted after block-level descendants, in tree order
        // 4. Recursively paint the in-flow children in correct CSS painting order:
        //    - First: Non-float, non-dragging block-level children
        //    - Then: Float, non-dragging children (so they appear on top)
        //    - Finally: Dragging children (so they appear on top of everything per W3C spec)

        // Separate children into floats, non-floats, and dragging.
        // Skip children that establish stacking contexts - those are painted
        // separately via generate_for_stacking_context with proper z-ordering.
        let mut non_float_children = Vec::new();
        let mut float_children = Vec::new();
        let mut dragging_children = Vec::new();

        for &child_index in children_indices {
            // Skip stacking context children - they're painted by the stacking
            // context tree traversal, not by the in-flow descendant path.
            if self.establishes_stacking_context(child_index) {
                continue;
            }
            let child_node = self
                .positioned_tree
                .tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child is being dragged (paint last for z-order)
            let is_dragging = child_node.dom_node_id.is_some_and(|dom_id| {
                let styled_node_state = self.get_styled_node_state(dom_id);
                styled_node_state.dragging
            });

            if is_dragging {
                dragging_children.push(child_index);
                continue;
            }

            // Check if this child is a float
            let is_float = if let Some(dom_id) = child_node.dom_node_id {
                use crate::solver3::getters::get_float;
                let styled_node_state = self.get_styled_node_state(dom_id);
                let float_value = get_float(self.ctx.styled_dom, dom_id, &styled_node_state);
                !matches!(
                    float_value.unwrap_or_default(),
                    azul_css::props::layout::LayoutFloat::None
                )
            } else {
                false
            };

            if is_float {
                float_children.push(child_index);
            } else {
                non_float_children.push(child_index);
            }
        }

        // Paint non-float children first
        for child_index in non_float_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child has a GPU transform (CSS transform or drag)
            let child_ref_frame = child_node.dom_node_id.and_then(|dom_id| {
                self.gpu_value_cache.and_then(|cache| {
                    // CSS transform first, then the ANIMATION channel — an
                    // engine-driven transition animates nodes that have no CSS
                    // `transform` of their own, and without this they get no
                    // reference frame and jump to their destination.
                    let (key, transform) = cache
                        .css_transform_keys
                        .get(&dom_id)
                        .zip(cache.css_current_transform_values.get(&dom_id))
                        .or_else(|| {
                            cache
                                .anim_transform_keys
                                .get(&dom_id)
                                .zip(cache.anim_current_transform_values.get(&dom_id))
                        })?;
                    Some((*key, *transform))
                })
            });

            // Push reference frame if child has a transform
            if let Some((transform_key, initial_transform)) = child_ref_frame {
                let child_pos = self
                    .positioned_tree
                    .calculated_positions
                    .get(child_index)
                    .copied()
                    .unwrap_or_default();
                let child_size = child_node.used_size.unwrap_or(LogicalSize {
                    width: 0.0,
                    height: 0.0,
                });
                let child_bounds = LogicalRect {
                    origin: child_pos,
                    size: child_size,
                };
                builder.set_current_node(child_node.dom_node_id);
                builder.push_reference_frame(transform_key, initial_transform, child_bounds);
            }

            // Push image mask clip if this child has one (wraps background + children)
            let did_push_child_image_mask = self.push_image_mask_clip(builder, child_index);

            // IMPORTANT: Paint background and border BEFORE pushing clips!
            // This ensures the container's background is in parent space (stationary),
            // not in scroll space. Same logic as generate_for_stacking_context.
            self.paint_node_background_and_border(builder, child_index)?;

            // Push clips and scroll frames AFTER painting background
            let did_push_clip = self.push_node_clips(builder, child_index, child_node);

            // Paint descendants inside the clip/scroll frame
            self.paint_in_flow_descendants(
                builder,
                child_index,
                self.positioned_tree.tree.children(child_index),
            )?;

            // For VirtualView children: emit placeholder INSIDE the clip
            if let Some(dom_id) = child_node.dom_node_id {
                if self.is_virtual_view_node(dom_id) {
                    let child_bounds = self.get_paint_rect(child_index).unwrap_or_default();
                    builder.push_virtual_view_placeholder(dom_id, child_bounds, child_bounds);
                }
            }

            // Pop the child's clips.
            if did_push_clip {
                self.pop_node_clips(builder, child_node);
            }

            // Pop image mask clip
            if did_push_child_image_mask {
                builder.pop_image_mask_clip();
            }
            // The stroke follows the geometry, not the fill region - it must
            // be outside the mask the fill was painted through.
            self.paint_svg_stroke(builder, child_index);

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;

            // Pop reference frame if we pushed one
            if child_ref_frame.is_some() {
                builder.pop_reference_frame();
            }
        }

        // +spec:positioning:1bcbb5 - floats rendered in front of non-positioned in-flow blocks, but behind in-flow inlines
        // Paint float children AFTER non-floats (so they appear on top)
        for child_index in float_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child has a GPU transform (CSS transform or drag)
            let child_ref_frame = child_node.dom_node_id.and_then(|dom_id| {
                self.gpu_value_cache.and_then(|cache| {
                    // CSS transform first, then the ANIMATION channel — an
                    // engine-driven transition animates nodes that have no CSS
                    // `transform` of their own, and without this they get no
                    // reference frame and jump to their destination.
                    let (key, transform) = cache
                        .css_transform_keys
                        .get(&dom_id)
                        .zip(cache.css_current_transform_values.get(&dom_id))
                        .or_else(|| {
                            cache
                                .anim_transform_keys
                                .get(&dom_id)
                                .zip(cache.anim_current_transform_values.get(&dom_id))
                        })?;
                    Some((*key, *transform))
                })
            });

            // Push reference frame if child has a transform
            if let Some((transform_key, initial_transform)) = child_ref_frame {
                let child_pos = self
                    .positioned_tree
                    .calculated_positions
                    .get(child_index)
                    .copied()
                    .unwrap_or_default();
                let child_size = child_node.used_size.unwrap_or(LogicalSize {
                    width: 0.0,
                    height: 0.0,
                });
                let child_bounds = LogicalRect {
                    origin: child_pos,
                    size: child_size,
                };
                builder.set_current_node(child_node.dom_node_id);
                builder.push_reference_frame(transform_key, initial_transform, child_bounds);
            }

            // Same as above: push image mask, paint background, then clips
            let did_push_child_image_mask = self.push_image_mask_clip(builder, child_index);
            self.paint_node_background_and_border(builder, child_index)?;
            let did_push_clip = self.push_node_clips(builder, child_index, child_node);
            self.paint_in_flow_descendants(
                builder,
                child_index,
                self.positioned_tree.tree.children(child_index),
            )?;

            // For VirtualView children: emit placeholder INSIDE the clip
            if let Some(dom_id) = child_node.dom_node_id {
                if self.is_virtual_view_node(dom_id) {
                    let child_bounds = self.get_paint_rect(child_index).unwrap_or_default();
                    builder.push_virtual_view_placeholder(dom_id, child_bounds, child_bounds);
                }
            }

            if did_push_clip {
                self.pop_node_clips(builder, child_node);
            }
            if did_push_child_image_mask {
                builder.pop_image_mask_clip();
            }
            // The stroke follows the geometry, not the fill region - it must
            // be outside the mask the fill was painted through.
            self.paint_svg_stroke(builder, child_index);

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;

            // Pop reference frame if we pushed one
            if child_ref_frame.is_some() {
                builder.pop_reference_frame();
            }
        }

        // Paint dragging children LAST so they appear on top of everything (W3C spec)
        for child_index in dragging_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(LayoutNodeId::new(child_index))
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child has a GPU transform (CSS transform or drag)
            let child_ref_frame = child_node.dom_node_id.and_then(|dom_id| {
                self.gpu_value_cache.and_then(|cache| {
                    // CSS transform first, then the ANIMATION channel — an
                    // engine-driven transition animates nodes that have no CSS
                    // `transform` of their own, and without this they get no
                    // reference frame and jump to their destination.
                    let (key, transform) = cache
                        .css_transform_keys
                        .get(&dom_id)
                        .zip(cache.css_current_transform_values.get(&dom_id))
                        .or_else(|| {
                            cache
                                .anim_transform_keys
                                .get(&dom_id)
                                .zip(cache.anim_current_transform_values.get(&dom_id))
                        })?;
                    Some((*key, *transform))
                })
            });

            // Push reference frame if child has a transform
            if let Some((transform_key, initial_transform)) = child_ref_frame {
                let child_pos = self
                    .positioned_tree
                    .calculated_positions
                    .get(child_index)
                    .copied()
                    .unwrap_or_default();
                let child_size = child_node.used_size.unwrap_or(LogicalSize {
                    width: 0.0,
                    height: 0.0,
                });
                let child_bounds = LogicalRect {
                    origin: child_pos,
                    size: child_size,
                };
                builder.set_current_node(child_node.dom_node_id);
                builder.push_reference_frame(transform_key, initial_transform, child_bounds);
            }

            // Same as above: push image mask, paint background, then clips
            let did_push_child_image_mask = self.push_image_mask_clip(builder, child_index);
            self.paint_node_background_and_border(builder, child_index)?;
            let did_push_clip = self.push_node_clips(builder, child_index, child_node);
            self.paint_in_flow_descendants(
                builder,
                child_index,
                self.positioned_tree.tree.children(child_index),
            )?;

            // For VirtualView children: emit placeholder INSIDE the clip
            if let Some(dom_id) = child_node.dom_node_id {
                if self.is_virtual_view_node(dom_id) {
                    let child_bounds = self.get_paint_rect(child_index).unwrap_or_default();
                    builder.push_virtual_view_placeholder(dom_id, child_bounds, child_bounds);
                }
            }

            if did_push_clip {
                self.pop_node_clips(builder, child_node);
            }
            if did_push_child_image_mask {
                builder.pop_image_mask_clip();
            }
            // The stroke follows the geometry, not the fill region - it must
            // be outside the mask the fill was painted through.
            self.paint_svg_stroke(builder, child_index);

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;

            // Pop reference frame if we pushed one
            if child_ref_frame.is_some() {
                builder.pop_reference_frame();
            }
        }

        Ok(())
    }

    /// Returns true if the given DOM node is a `VirtualView` node.
    fn is_virtual_view_node(&self, dom_id: NodeId) -> bool {
        let node_data_container = self.ctx.styled_dom.node_data.as_container();
        node_data_container
            .get(dom_id)
            .is_some_and(|nd| matches!(nd.get_node_type(), NodeType::VirtualView))
    }

    /// Checks if a node has an image mask clip and pushes `PushImageMaskClip` if so.
    /// Returns true if a clip was pushed (caller must pop it).
    /// Paint this node's SVG stroke, if it has one.
    ///
    /// Called AFTER the node's own image-mask clip is popped, and that is the
    /// whole point: the mask is the shape's FILL region, and in SVG (and in
    /// PDF) a stroke is not clipped by the fill. An open path - a line, an
    /// arc, a chevron - encloses no area at all, so its fill mask is empty and
    /// a stroke drawn inside it disappears completely. That is exactly how a
    /// 4px rule rendered as nothing.
    fn paint_svg_stroke(&self, builder: &mut DisplayListBuilder, node_index: usize) {
        let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(node_index)) else {
            return;
        };
        let Some(dom_id) = node.dom_node_id else {
            return;
        };
        let styled_node_state = self.get_styled_node_state(dom_id);
        let border_info = get_border_info(self.ctx.styled_dom, dom_id, &styled_node_state);
        let Some((path, color, width)) = self.svg_stroke_for(dom_id, &border_info) else {
            return;
        };
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return;
        };
        let view_box = self.enclosing_view_box(dom_id);
        #[cfg(feature = "cpurender")]
        let mask = rasterize_svg_stroke_to_r8(&path, &paint_rect, view_box, width);
        #[cfg(not(feature = "cpurender"))]
        let mask = None;
        builder.push_item(DisplayListItem::StrokedPath {
            bounds: paint_rect.into(),
            path,
            view_box,
            color,
            width,
            mask,
        });
    }

    /// This node's STROKE, if it has geometry and a border to stroke it with.
    ///
    /// `stroke`/`stroke-width` are accepted spellings of
    /// `border-color`/`border-width`, so they arrive through the ordinary
    /// cascade and land in `border_info` - one width and one colour are read
    /// back out here (SVG has a single stroke, not four sides; the top slot is
    /// the one the shorthand fills).
    fn svg_stroke_for(
        &self,
        dom_id: NodeId,
        border_info: &BorderInfo,
    ) -> Option<(azul_core::svg::SvgMultiPolygon, ColorU, f32)> {
        let node_data = self.ctx.styled_dom.node_data.as_container();
        let azul_core::dom::SvgNodeData::Path(path) = node_data.get(dom_id)?.get_svg_data()? else {
            return None;
        };
        let width = border_info
            .widths
            .top
            .and_then(azul_css::css::CssPropertyValue::get_property_owned)
            .map(|w| w.inner.to_pixels_internal(0.0, 16.0, 16.0))?;
        if !width.is_finite() || width <= 0.0 {
            return None;
        }
        let color = border_info
            .colors
            .top
            .and_then(azul_css::css::CssPropertyValue::get_property_owned)
            .map(|c| c.inner)?;
        if color.a == 0 {
            return None;
        }
        Some((path.clone(), color, width))
    }

    /// The `viewBox` of the nearest `<svg>` ancestor of `node` (itself
    /// included), as `(min_x, min_y, width, height)`.
    ///
    /// An SVG shape's geometry is in its `<svg>`'s USER SPACE, and the element
    /// it paints into is sized in CSS px; this is the only place that knows
    /// both. Bounded by the tree depth, and `None` for a clip-path on an
    /// ordinary element - which has no user space and whose geometry is
    /// already window-logical.
    fn enclosing_view_box(&self, node: NodeId) -> Option<(f32, f32, f32, f32)> {
        let node_data = self.ctx.styled_dom.node_data.as_container();
        let hierarchy = self.ctx.styled_dom.node_hierarchy.as_container();
        let mut cursor = Some(node);
        while let Some(id) = cursor {
            if let Some(azul_core::dom::SvgNodeData::ViewBox {
                min_x,
                min_y,
                width,
                height,
            }) = node_data.get(id).and_then(azul_core::dom::NodeData::get_svg_data)
            {
                return Some((*min_x, *min_y, *width, *height));
            }
            cursor = hierarchy.get(id).and_then(|h| h.parent_id());
        }
        None
    }

    fn push_image_mask_clip(&self, builder: &mut DisplayListBuilder, node_index: usize) -> bool {
        let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(node_index)) else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };
        let node_data_container = self.ctx.styled_dom.node_data.as_container();
        let Some(node_data) = node_data_container.get(dom_id) else {
            return false;
        };
        match node_data.get_svg_data() {
            Some(azul_core::dom::SvgNodeData::ImageClipMask(clip_mask)) => {
                let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();
                // Convert mask rect from element-local to window-logical coordinates
                let mask_rect = LogicalRect {
                    origin: LogicalPosition {
                        x: paint_rect.origin.x + clip_mask.rect.origin.x,
                        y: paint_rect.origin.y + clip_mask.rect.origin.y,
                    },
                    size: clip_mask.rect.size,
                };
                builder.push_image_mask_clip(paint_rect, clip_mask.image.clone(), mask_rect);
                true
            }
            #[cfg(feature = "cpurender")]
            Some(azul_core::dom::SvgNodeData::Path(svg_clip)) => {
                let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();
                // The coordinate system the geometry was authored in: the
                // nearest `<svg>` ancestor's viewBox. Without it a shape drawn
                // at 16 units paints a sixteenth of a 256px slot.
                let view_box = self.enclosing_view_box(dom_id);
                rasterize_svg_clip_to_r8(svg_clip, &paint_rect, view_box).is_some_and(
                    |mask_image| {
                        builder.push_image_mask_clip(paint_rect, mask_image, paint_rect);
                        true
                    },
                )
            }
            #[cfg(not(feature = "cpurender"))]
            Some(azul_core::dom::SvgNodeData::Path(_)) => {
                // The DOM asked for an SVG clip mask; without `cpurender` the
                // clip silently does not clip. Say so once.
                static ANNOUNCE: std::sync::Once = std::sync::Once::new();
                ANNOUNCE.call_once(|| {
                    eprintln!(
                        "[azul][svg] an SVG clip-path is present, but this build has \
                         no `cpurender` feature — SVG clips will NOT clip"
                    );
                });
                false
            }
            // Other SvgNodeData variants (shapes, gradients, etc.) don't produce clip masks
            Some(_) => false,
            None => false,
        }
    }

    // +spec:overflow:531bd2 - ancestor clips accumulate via push_clip/pop_clip stack (cumulative intersection)
    // +spec:overflow:8098ec - overflow clipping/scrolling; abs-pos elements with containing block outside scroller are not scrolled
    /// Checks if a node requires clipping or scrolling and pushes the appropriate commands.
    /// Returns true if any command was pushed.
    ///
    /// // +spec:containing-block:62aa5c - overflow clipping applies to all content except
    /// // descendants whose containing block is the viewport or an ancestor of this element
    /// // (i.e. absolutely positioned elements that escape the overflow container).
    /// // TODO: exempt abs-pos descendants whose containing block is an ancestor of this node.
    ///
    /// For `VirtualView` nodes with `overflow: scroll/auto`, we intentionally skip
    /// `PushScrollFrame` / `PopScrollFrame`. `VirtualView` scroll state is managed by
    /// `ScrollManager`, not `WebRender`'s APZ. Instead we emit only a `PushClip`
    /// and later an `VirtualViewPlaceholder` (see `generate_for_stacking_context`).
    #[allow(clippy::similar_names)] // domain-standard coordinate/geometry/short-lived names
    fn push_node_clips(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        node: &LayoutNodeHot,
    ) -> bool {
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };

        let styled_node_state = self.get_styled_node_state(dom_id);

        let raw_overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let raw_overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
        // +spec:overflow:833078 - resolve visible/clip to auto/hidden per CSS Overflow 3 §3.1
        let overflow_x = raw_overflow_x.resolve_computed(&raw_overflow_y);
        let overflow_y = raw_overflow_y.resolve_computed(&raw_overflow_x);

        let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();
        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        // +spec:positioning:9c261b - clip-path (modern replacement for legacy 'clip' property)
        // The legacy CSS 2.2 'clip' property applied only to absolutely positioned elements;
        // clip-path supersedes it and applies to all elements per CSS Masking Level 1.
        // If present, push a clip region derived from the clip-path shape.
        // This is evaluated before overflow clips; both can be active simultaneously.
        let has_clip_path =
            super::getters::get_clip_path(self.ctx.styled_dom, dom_id, &styled_node_state)
                .is_some_and(|clip_path| {
                    if let Some((clip_rect, radius)) = resolve_clip_path(&clip_path, paint_rect) {
                        let br = if radius > 0.0 {
                            BorderRadius {
                                top_left: radius,
                                top_right: radius,
                                bottom_left: radius,
                                bottom_right: radius,
                            }
                        } else {
                            BorderRadius::default()
                        };
                        builder.push_clip(clip_rect, br);
                        true
                    } else {
                        false
                    }
                });

        // +spec:overflow:6890f2 - text-overflow: clip inline content at end line box edge when overflow != visible
        // +spec:overflow:77d7ce - clipping region defines visible portion of border box; default is not clipped
        let needs_clip = overflow_x.is_clipped() || overflow_y.is_clipped();

        if !needs_clip {
            return has_clip_path;
        }

        // +spec:overflow:c52f2a - clipping region is rounded to element's border-radius
        // +spec:overflow:913b23 - when both axes are clip, region is rounded per overflow-clip-margin
        // +spec:overflow:449d69 - when one axis is clip and the other is visible, clipping region is not rounded
        // +spec:overflow:449d69 - when one axis is clip and the other is visible, clipping region is not rounded
        let ox_clip =
            overflow_x.is_clipped() && !overflow_x.is_scroll() && !overflow_x.is_auto_overflow();
        let oy_clip =
            overflow_y.is_clipped() && !overflow_y.is_scroll() && !overflow_y.is_auto_overflow();
        let ox_visible = !overflow_x.is_clipped();
        let oy_visible = !overflow_y.is_clipped();
        let border_radius = if (ox_clip && oy_visible) || (oy_clip && ox_visible) {
            BorderRadius::default()
        } else {
            border_radius
        };

        let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();

        let bp = node.box_props.unpack();
        let border = &bp.border;

        // Get scrollbar info to adjust clip rect for content area
        let scrollbar_info = self
            .positioned_tree
            .tree
            .warm(LayoutNodeId::new(node_index))
            .and_then(|w| w.scrollbar_info)
            .unwrap_or_default();

        // +spec:overflow:13cacb - clip rect clamped to 0 so zero-size clips hide all pixels
        // +spec:overflow:9207bc - clip rect computed from border-box edges (analogous to CSS 2.2 clip: rect() offsets)
        // +spec:overflow:3d5b53 - overflow clips to padding edge, scroll mechanism for scroll/auto
        // The clip rect for content should exclude the scrollbar area
        // Scrollbars are drawn inside the border-box, on the right/bottom edges
        // +spec:overflow:a825a6 - TODO: abs-pos elements with containing block outside this
        // element should not be clipped (currently all DOM children are clipped)
        let mut clip_rect = LogicalRect {
            origin: LogicalPosition {
                x: paint_rect.origin.x + border.left,
                y: paint_rect.origin.y + border.top,
            },
            size: LogicalSize {
                // Reduce width/height by scrollbar dimensions so content doesn't overlap scrollbar
                width: (paint_rect.size.width
                    - border.left
                    - border.right
                    - scrollbar_info.scrollbar_width)
                    .max(0.0),
                height: (paint_rect.size.height
                    - border.top
                    - border.bottom
                    - scrollbar_info.scrollbar_height)
                    .max(0.0),
            },
        };

        // +spec:overflow:342f47 - overflow-clip-margin expands clip edge for overflow:clip only
        // Per CSS Overflow 3 §3.2: overflow-clip-margin has no effect on overflow:hidden
        // or overflow:scroll. It only expands the overflow clip edge when overflow:clip is used.
        apply_overflow_clip_margin(
            &mut clip_rect,
            &overflow_x,
            &overflow_y,
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
        );

        let is_virtual_view = self.is_virtual_view_node(dom_id);

        // +spec:overflow:484889 - clip content in unreachable scrollable overflow region
        // +spec:overflow:917dae - scrollable overflow rect is a rectangle in box's own coordinate system
        // Every clipped node pushes a clip (scrollable, hidden, or clip alike).
        builder.push_clip(clip_rect, border_radius);
        // Regular scrollable nodes ALSO push a scroll frame: WebRender's APZ
        // manages the offset via define_scroll_frame, CPU renderers translate
        // children by scroll_offset. VirtualView scroll state is instead managed
        // by ScrollManager and passed to the callback as scroll_offset, with the
        // VirtualViewPlaceholder emitted after pop_node_clips in
        // generate_for_stacking_context — so VirtualView nodes get only the clip.
        if (overflow_x.is_scroll() || overflow_y.is_scroll()) && !is_virtual_view {
            let scroll_id = self
                .scroll_ids
                .get(&LayoutNodeId::new(node_index))
                .copied()
                .unwrap_or(0);
            let content_size = get_scroll_content_size(
                node,
                self.positioned_tree
                    .tree
                    .warm(LayoutNodeId::new(node_index)),
            );
            builder.push_scroll_frame(clip_rect, content_size, scroll_id);
        }

        true
    }

    /// Pops any clip/scroll commands associated with a node.
    fn pop_node_clips(&self, builder: &mut DisplayListBuilder, node: &LayoutNodeHot) {
        let Some(dom_id) = node.dom_node_id else {
            return;
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        // Mirror push_node_clips EXACTLY: resolve visible/clip → auto/hidden per
        // CSS Overflow 3 §3.1 (an axis computes to auto/hidden when the *other*
        // axis is a scroll container). push_node_clips decides whether to emit a
        // scroll frame from the RESOLVED values; popping from the RAW values can
        // disagree. Concretely: the auto-injected titlebar title has
        // overflow-x:hidden, overflow-y:visible → push resolves y→auto (a scroll
        // container, since is_scroll() counts Auto) and emits PushClip +
        // PushScrollFrame, but pop saw raw y=visible (is_scroll=false) and emitted
        // only PopClip → an unbalanced PushScrollFrame. The layer allocator then
        // extends the titlebar's scroll layer to the end of the list, swallowing
        // the document body into the titlebar's clip rect (blank window) and
        // underflowing the clip stack. Resolving here keeps push/pop symmetric.
        let raw_overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let raw_overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_x = raw_overflow_x.resolve_computed(&raw_overflow_y);
        let overflow_y = raw_overflow_y.resolve_computed(&raw_overflow_x);

        let paint_rect = self
            .get_paint_rect(
                self.positioned_tree
                    .tree
                    .nodes
                    .iter()
                    .position(|n| n.dom_node_id == Some(dom_id))
                    .unwrap_or(0),
            )
            .unwrap_or_default();

        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        let needs_clip = overflow_x.is_clipped() || overflow_y.is_clipped();

        let is_virtual_view = self.is_virtual_view_node(dom_id);

        if needs_clip {
            // Regular (non-VirtualView) scroll/auto also pushed a scroll frame;
            // pop it first (LIFO) before the shared clip. Hidden/clip and
            // VirtualView scroll only pushed a clip.
            if (overflow_x.is_scroll() || overflow_y.is_scroll()) && !is_virtual_view {
                builder.pop_scroll_frame();
            }
            builder.pop_clip();
        }

        // Pop the clip-path clip if one was pushed.
        // This mirrors the push_node_clips logic: if clip-path is set,
        // a PushClip was emitted before any overflow clips.
        // We pop it last (stack order: clip-path pushed first, popped last).
        if let Some(clip_path) =
            super::getters::get_clip_path(self.ctx.styled_dom, dom_id, &styled_node_state)
        {
            if resolve_clip_path(&clip_path, paint_rect).is_some() {
                builder.pop_clip();
            }
        }
    }

    /// Calculates the final paint-time rectangle for a node.
    ///
    /// ## Coordinate Space
    ///
    /// Returns the node's position in **absolute window coordinates** (logical pixels).
    /// This is the coordinate space used throughout the display list:
    ///
    /// - Origin: Top-left corner of the window
    /// - Units: Logical pixels (`HiDPI` scaling happens in compositor2.rs)
    /// - Scroll: NOT applied here - `WebRender` scroll frames handle scroll offset
    ///   transformation internally via `define_scroll_frame()`
    ///
    /// ## Important
    ///
    /// Do NOT manually subtract scroll offset here! `WebRender`'s scroll spatial
    /// transforms handle this. Subtracting here would cause double-offset and
    /// parallax effects (backgrounds and text moving at different speeds).
    fn get_paint_rect(&self, node_index: usize) -> Option<LogicalRect> {
        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))?;
        let pos = self
            .positioned_tree
            .calculated_positions
            .get(node_index)
            .copied()
            .unwrap_or_default();
        let size = node.used_size.unwrap_or_default();

        // NOTE: Scroll offset is NOT applied here!
        // WebRender scroll frames handle scroll transformation.
        // See compositor2.rs PushScrollFrame for details.

        Some(LogicalRect::new(pos, size))
    }

    /// Emits drawing commands for the background and border of a single node.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn paint_node_background_and_border(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let _p = crate::probe::Probe::span("dl_bg_border");
        if self.try_copy_cached_run(builder, node_index, EmitPhase::BgBorder) {
            return Ok(());
        }
        builder.set_current_layout(Some((node_index, EmitPhase::BgBorder)));
        let result = self.paint_node_background_and_border_inner(builder, node_index);
        builder.set_current_layout(None);
        result
    }

    fn paint_node_background_and_border_inner(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };
        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        // Check for CSS break-before/break-after properties and register forced page breaks
        // This is used by the pagination slicer to insert page breaks at correct positions
        if let Some(dom_id) = node.dom_node_id {
            let break_before = get_break_before(self.ctx.styled_dom, Some(dom_id));
            let break_after = get_break_after(self.ctx.styled_dom, Some(dom_id));

            // For break-before: always, insert a page break at the top of this element
            if is_forced_page_break(break_before) {
                let y_position = paint_rect.origin.y;
                builder.add_forced_page_break(y_position, Some(dom_id));
                debug_info!(
                    self.ctx,
                    "Registered forced page break BEFORE node {} at y={}",
                    node_index,
                    y_position
                );
            }

            // For break-after: always, insert a page break at the bottom of this element
            if is_forced_page_break(break_after) {
                let y_position = paint_rect.origin.y + paint_rect.size.height;
                builder.add_forced_page_break(y_position, Some(dom_id));
                debug_info!(
                    self.ctx,
                    "Registered forced page break AFTER node {} at y={}",
                    node_index,
                    y_position
                );
            }
        }

        // CSS 2.2 §11.2: visibility:hidden — box is invisible but still affects layout.
        // Skip painting background/border for hidden nodes, but traversal continues
        // so visible descendants are still painted.
        if self.is_node_hidden(node_index) {
            return Ok(());
        }

        // Skip inline and inline-block elements ONLY if they participate in an IFC (Inline Formatting Context).
        // In Flex or Grid containers, inline-block elements are treated as flex/grid items and must be painted here.
        // Inline elements participate in inline formatting context and their backgrounds
        // must be positioned by the text layout engine, not the block layout engine
        //
        // IMPORTANT: The parent check must look at the PARENT NODE's formatting_context,
        // not the current node's. If parent is Flex/Grid, we paint this element as a flex/grid item.
        // Also check parent_formatting_context field which stores parent's FC during tree construction.
        let warm = self
            .positioned_tree
            .tree
            .warm(LayoutNodeId::new(node_index));
        let parent_is_flex_or_grid = warm
            .and_then(|w| {
                w.parent_formatting_context
                    .as_ref()
                    .map(|fc| matches!(fc, FormattingContext::Flex | FormattingContext::Grid))
            })
            .unwrap_or(false);

        if let Some(dom_id) = node.dom_node_id {
            let display = {
                use crate::solver3::getters::get_display_property;
                get_display_property(self.ctx.styled_dom, Some(dom_id))
                    .unwrap_or(LayoutDisplay::Inline)
            };

            if display == LayoutDisplay::InlineBlock || display == LayoutDisplay::Inline {
                debug_info!(
                    self.ctx,
                    "[paint_node] node {} has display={:?}, parent_formatting_context={:?}, parent_is_flex_or_grid={}",
                    node_index,
                    display,
                    warm.and_then(|w| w.parent_formatting_context.as_ref()),
                    parent_is_flex_or_grid
                );

                if !parent_is_flex_or_grid {
                    // Normally, text3 handles inline/inline-block backgrounds via
                    // InlineShape (inline-block) or glyph runs (inline). However,
                    // if this inline-block establishes a stacking context (e.g.
                    // position:relative + z-index, opacity < 1, transform), we MUST
                    // paint its background here. generate_for_stacking_context paints
                    // background (step 1) → children (steps 3-6). If we skip the
                    // background, paint_inline_shape in the parent's paint_node_content
                    // would paint it AFTER the children, obscuring them.
                    if display == LayoutDisplay::InlineBlock
                        && self.establishes_stacking_context(node_index)
                    {
                        // Fall through to paint background/border now
                    } else {
                        return Ok(());
                    }
                }
                // Fall through to paint this element - it's a flex/grid item
            }
        }

        // CSS 2.2 Section 17.5.1: Tables in the visual formatting model
        // Table-internal elements (row groups, rows, columns, column groups) have their
        // backgrounds painted by paint_table_items() in the correct 6-layer order.
        // Skip background painting here to avoid double-painting at wrong positions
        // (calculated_positions for TR elements may not reflect row offsets correctly;
        // paint_table_items computes row rects from cell bounding boxes instead).
        // Table CELLS still need content painting via paint_in_flow_descendants, so
        // we only skip the background/border here — content painting continues normally.
        if matches!(
            node.formatting_context,
            FormattingContext::TableRowGroup
                | FormattingContext::TableRow
                | FormattingContext::TableColumnGroup
        ) {
            return Ok(());
        }

        // Tables have a special 6-layer background painting order
        if matches!(node.formatting_context, FormattingContext::Table) {
            debug_info!(
                self.ctx,
                "Painting table backgrounds/borders for node {} at {:?}",
                node_index,
                paint_rect
            );
            // Delegate to specialized table painting function
            return self.paint_table_items(builder, node_index);
        }

        // CSS 2.2 section 17.5.1: a cell's BACKGROUND belongs to layer 6 of
        // the table paint order and is painted by paint_table_items — never
        // here, or it re-paints ON TOP of whatever the table painted after
        // layer 6 (the resolved collapsed borders). The cell's own BORDER is
        // painted here in the separate-borders model only; in the collapsing
        // model the table paints one resolved border per grid edge
        // (paint_collapsed_table_borders), so the cell contributes nothing.
        let is_table_cell = matches!(node.formatting_context, FormattingContext::TableCell);
        if is_table_cell && self.is_inside_collapsed_table(node_index) {
            return Ok(());
        }

        if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let background_contents = if is_table_cell {
                Vec::new()
            } else {
                get_background_contents(self.ctx.styled_dom, dom_id, &styled_node_state)
            };
            let border_info = get_border_info(self.ctx.styled_dom, dom_id, &styled_node_state);

            let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            debug_info!(
                self.ctx,
                "Painting background/border for node {} ({:?}) at {:?}, backgrounds={:?}",
                node_index,
                node_type.get_node_type(),
                paint_rect,
                background_contents.len()
            );

            // Get both versions: simple BorderRadius for rect clipping and StyleBorderRadius for
            // border rendering
            let element_size = PhysicalSizeImport {
                width: paint_rect.size.width,
                height: paint_rect.size.height,
            };
            let simple_border_radius = get_border_radius(
                self.ctx.styled_dom,
                dom_id,
                &styled_node_state,
                element_size,
                self.ctx.viewport_size,
            );
            let style_border_radius =
                get_style_border_radius(self.ctx.styled_dom, dom_id, &styled_node_state);

            // Paint box shadows before backgrounds (CSS spec: shadows render behind the element)
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // +spec:overflow:bb4308 - box shadows are ink overflow: painted outside border box, not affecting layout
            // Check all four sides for box-shadow (azul stores them per-side).
            // Routed through `super::getters::*` so the compact-cache has_box_shadow
            // fast path fires — most nodes have no shadow and skip 4 cascade walks.
            for shadow in [
                super::getters::get_box_shadow_left(self.ctx.styled_dom, dom_id, node_state),
                super::getters::get_box_shadow_right(self.ctx.styled_dom, dom_id, node_state),
                super::getters::get_box_shadow_top(self.ctx.styled_dom, dom_id, node_state),
                super::getters::get_box_shadow_bottom(self.ctx.styled_dom, dom_id, node_state),
            ]
            .into_iter()
            .flatten()
            {
                builder.push_item(DisplayListItem::BoxShadow {
                    bounds: paint_rect.into(),
                    shadow,
                    border_radius: simple_border_radius,
                });
            }

            // An SVG SHAPE takes its border as a STROKE. A stroke follows the
            // geometry; a border follows the box - and the box is clipped to
            // the geometry, so a rectangular border on a shape leaves a
            // hairline at the corners and nothing anywhere else. The border
            // slots are how `stroke`/`stroke-width` reach the cascade (see
            // `COMBINED_CSS_PROPERTIES_KEY_MAP`), so they are consumed here
            // and NOT handed to the rectangle painter.
            let stroke = self.svg_stroke_for(dom_id, &border_info);
            let border_info = if stroke.is_some() {
                let mut without_border = border_info.clone();
                without_border.widths = StyleBorderWidths {
                    top: None,
                    right: None,
                    bottom: None,
                    left: None,
                };
                without_border
            } else {
                border_info.clone()
            };

            // Use unified background/border painting
            builder.push_backgrounds_and_border(
                paint_rect,
                &background_contents,
                &border_info,
                simple_border_radius,
                style_border_radius,
                self.ctx.image_cache,
            );

            // The stroke is NOT painted here: it must land OUTSIDE this
            // node's own clip mask. See `paint_svg_stroke`.
            let _ = stroke;
        }

        Ok(())
    }

    //   backgrounds are invisible, allowing table background to show through
    // +spec:box-model:124815 - Table layer background painting order (6 layers: table, col-group, col, row-group, row, cell)
    // +spec:positioning:702985 - Table background painting in 6 layers (17.5.1)
    // +spec:table-layout:7370dc - Table layers and transparency: 6-layer background painting order
    // +spec:table-layout:7a5909 - table layers: 6-layer background paint order (table/colgroup/col/rowgroup/row/cell)
    /// CSS 2.2 Section 17.5.1: Table background painting in 6 layers
    ///
    /// Implements the CSS 2.2 specification for table background painting order.
    /// Unlike regular block elements, tables paint backgrounds in layers from back to front:
    ///
    /// 1. Table background (lowest layer)
    /// 2. Column group backgrounds
    /// 3. Column backgrounds
    /// 4. Row group backgrounds
    /// 5. Row backgrounds
    /// 6. Cell backgrounds (topmost layer)
    ///
    /// Then borders are painted (respecting border-collapse mode).
    /// Finally, cell content is painted on top of everything.
    ///
    /// This function generates simple display list items (Rect, Border) in the correct
    /// CSS paint order, making `WebRender` integration trivial.
    fn paint_table_items(
        &self,
        builder: &mut DisplayListBuilder,
        table_index: usize,
    ) -> Result<()> {
        let table_node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(table_index))
            .ok_or(LayoutError::InvalidTree)?;

        let Some(table_paint_rect) = self.get_paint_rect(table_index) else {
            return Ok(());
        };

        // Layer 1: Table background
        if let Some(dom_id) = table_node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
            let element_size = PhysicalSizeImport {
                width: table_paint_rect.size.width,
                height: table_paint_rect.size.height,
            };
            let border_radius = get_border_radius(
                self.ctx.styled_dom,
                dom_id,
                &styled_node_state,
                element_size,
                self.ctx.viewport_size,
            );

            builder.push_rect(table_paint_rect, bg_color, border_radius);
        }

        // Traverse table children to paint layers 2-6

        // Layer 2: Column group backgrounds
        // Layer 3: Column backgrounds (columns are children of column groups)
        for &child_idx in self.positioned_tree.tree.children(table_index) {
            let child_node = self.positioned_tree.tree.get(LayoutNodeId::new(child_idx));
            if let Some(node) = child_node {
                if matches!(node.formatting_context, FormattingContext::TableColumnGroup) {
                    // Paint column group background
                    self.paint_element_background(builder, child_idx);

                    // Paint backgrounds of individual columns within this group
                    for &col_idx in self.positioned_tree.tree.children(child_idx) {
                        self.paint_element_background(builder, col_idx);
                    }
                }
            }
        }

        // Layer 4: Row group backgrounds (tbody, thead, tfoot)
        // Layer 5: Row backgrounds
        // Layer 6: Cell backgrounds
        for &child_idx in self.positioned_tree.tree.children(table_index) {
            let child_node = self.positioned_tree.tree.get(LayoutNodeId::new(child_idx));
            if let Some(node) = child_node {
                match node.formatting_context {
                    FormattingContext::TableRowGroup => {
                        // Paint row group background
                        self.paint_element_background(builder, child_idx);

                        // Paint rows within this group
                        for &row_idx in self.positioned_tree.tree.children(child_idx) {
                            self.paint_table_row_and_cells(builder, row_idx);
                        }
                    }
                    FormattingContext::TableRow => {
                        // Direct row child (no row group wrapper)
                        self.paint_table_row_and_cells(builder, child_idx);
                    }
                    _ => {}
                }
            }
        }

        // Borders: separate model cells paint their own borders in the normal
        // flow; the collapsing model paints ONE resolved border per grid edge
        // here, on top of all table backgrounds (cell borders are suppressed
        // in paint_node_background_and_border for collapsed tables).
        if self.table_is_border_collapsed(table_index) {
            self.paint_collapsed_table_borders(builder, table_index);
        }

        Ok(())
    }

    /// Whether `table_index` (a `FormattingContext::Table` node) uses the
    /// collapsing border model. Anonymous wrapper tables have no DOM node and
    /// keep the initial value (separate).
    fn table_is_border_collapsed(&self, table_index: usize) -> bool {
        use azul_css::props::layout::table::StyleBorderCollapse;
        let Some(node) = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(table_index))
        else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };
        if let Some(ref cc) = self.ctx.styled_dom.css_property_cache.ptr.compact_cache {
            return cc.get_border_collapse(dom_id.index()) == StyleBorderCollapse::Collapse;
        }
        let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
        let node_state = self.get_styled_node_state(dom_id);
        self.ctx
            .styled_dom
            .css_property_cache
            .ptr
            .get_border_collapse(node_data, &dom_id, &node_state)
            .and_then(|prop| prop.get_property().copied())
            .unwrap_or(StyleBorderCollapse::Separate)
            == StyleBorderCollapse::Collapse
    }

    /// Whether a node lives inside a `border-collapse: collapse` table
    /// (walks the layout-tree parent chain to the nearest Table node).
    fn is_inside_collapsed_table(&self, node_index: usize) -> bool {
        let mut cur = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .and_then(|n| n.parent);
        while let Some(idx) = cur {
            let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(idx)) else {
                return false;
            };
            if matches!(node.formatting_context, FormattingContext::Table) {
                return self.table_is_border_collapsed(idx);
            }
            cur = node.parent;
        }
        false
    }

    /// CSS 2.2 section 17.6.2: paint the collapsing-border grid.
    ///
    /// For every grid edge the participating borders (the two adjacent cells
    /// on interior edges; cell + table on perimeter edges; the row's own
    /// border on horizontal edges) compete via
    /// `BorderInfo::resolve_conflict` (hidden wins, then wider, then style
    /// priority, then source priority) and the single winner is painted as a
    /// strip CENTERED on the grid line — perimeter borders deliberately
    /// straddle the table edge, exactly like browsers render them.
    ///
    /// v1 limitations, acceptable for the current corpus and safe (worst
    /// case: a border strip at a slightly wrong offset, never a double
    /// border): cells are paired positionally per row (colspan/rowspan
    /// pairing is approximate), column/column-group borders do not
    /// participate, and non-solid winners (dashed/dotted/double) paint as a
    /// solid strip of the winning color.
    fn paint_collapsed_table_borders(&self, builder: &mut DisplayListBuilder, table_index: usize) {
        use crate::solver3::fc::{
            get_border_info as collapsed_border_info, BorderInfo as CollapsedBorder, BorderSource,
        };
        use azul_css::props::style::border::BorderStyle;

        // (cell layout-tree index, paint rect, owning row index) per row
        let mut rows: Vec<(usize, Vec<(usize, LogicalRect)>)> = Vec::new();
        for &child_idx in self.positioned_tree.tree.children(table_index) {
            let Some(child) = self.positioned_tree.tree.get(LayoutNodeId::new(child_idx)) else {
                continue;
            };
            match child.formatting_context {
                FormattingContext::TableRowGroup => {
                    for &row_idx in self.positioned_tree.tree.children(child_idx) {
                        rows.push((row_idx, self.collect_row_cells(row_idx)));
                    }
                }
                FormattingContext::TableRow => {
                    rows.push((child_idx, self.collect_row_cells(child_idx)));
                }
                _ => {}
            }
        }
        rows.retain(|(_, cells)| !cells.is_empty());
        if rows.is_empty() {
            return;
        }

        let cell_border = |idx: usize| -> Option<[CollapsedBorder; 4]> {
            let node = self.positioned_tree.tree.get(LayoutNodeId::new(idx))?;
            Some(collapsed_border_info(self.ctx, node, BorderSource::Cell).into())
        };
        let row_border = |idx: usize| -> Option<[CollapsedBorder; 4]> {
            let node = self.positioned_tree.tree.get(LayoutNodeId::new(idx))?;
            Some(collapsed_border_info(self.ctx, node, BorderSource::Row).into())
        };
        let table_border: Option<[CollapsedBorder; 4]> = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(table_index))
            .map(|node| collapsed_border_info(self.ctx, node, BorderSource::Table).into());
        const TOP: usize = 0;
        const RIGHT: usize = 1;
        const BOTTOM: usize = 2;
        const LEFT: usize = 3;

        // Fold the participants down to one winner. `resolve_conflict`
        // returning None means `hidden` participated: paint nothing.
        let resolve = |participants: &[Option<CollapsedBorder>]| -> Option<CollapsedBorder> {
            let mut winner: Option<CollapsedBorder> = None;
            for b in participants.iter().flatten() {
                winner = match winner {
                    None => Some(*b),
                    Some(w) => Some(CollapsedBorder::resolve_conflict(&w, b)?),
                };
            }
            let w = winner?;
            (w.width > 0.0 && !matches!(w.style, BorderStyle::None | BorderStyle::Hidden))
                .then_some(w)
        };

        // Vertical edges, one strip per (row, boundary).
        for (row_idx, cells) in &rows {
            let _ = row_idx;
            let n = cells.len();
            for boundary in 0..=n {
                let left_cell = boundary.checked_sub(1).and_then(|i| cells.get(i));
                let right_cell = cells.get(boundary);
                let winner = resolve(&[
                    left_cell
                        .and_then(|(i, _)| cell_border(*i))
                        .map(|b| b[RIGHT]),
                    right_cell
                        .and_then(|(i, _)| cell_border(*i))
                        .map(|b| b[LEFT]),
                    // table border participates on the perimeter only
                    if boundary == 0 {
                        table_border.map(|b| b[LEFT])
                    } else if boundary == n {
                        table_border.map(|b| b[RIGHT])
                    } else {
                        None
                    },
                ]);
                let Some(w) = winner else { continue };
                let (x, y0, y1) = match (left_cell, right_cell) {
                    (Some((_, lr)), _) => (
                        lr.origin.x + lr.size.width,
                        lr.origin.y,
                        lr.origin.y + lr.size.height,
                    ),
                    (None, Some((_, rr))) => {
                        (rr.origin.x, rr.origin.y, rr.origin.y + rr.size.height)
                    }
                    (None, None) => continue,
                };
                builder.push_rect(
                    LogicalRect::new(
                        LogicalPosition::new(w.width.mul_add(-0.5, x), y0),
                        LogicalSize::new(w.width, y1 - y0),
                    ),
                    w.color,
                    BorderRadius::default(),
                );
            }
        }

        // Horizontal edges: for each row boundary, one strip per column
        // segment (positional pairing of the cells above/below).
        let n_rows = rows.len();
        for boundary in 0..=n_rows {
            let above = boundary.checked_sub(1).and_then(|i| rows.get(i));
            let below = rows.get(boundary);
            let segments: &[(usize, LogicalRect)] = match below.or(above) {
                Some((_, cells)) => cells,
                None => continue,
            };
            for (col, (_, seg_rect)) in segments.iter().enumerate() {
                let above_cell = above.and_then(|(_, cells)| cells.get(col));
                let below_cell = below.and_then(|(_, cells)| cells.get(col));
                let winner = resolve(&[
                    above_cell
                        .and_then(|(i, _)| cell_border(*i))
                        .map(|b| b[BOTTOM]),
                    above.and_then(|(r, _)| row_border(*r)).map(|b| b[BOTTOM]),
                    below_cell
                        .and_then(|(i, _)| cell_border(*i))
                        .map(|b| b[TOP]),
                    below.and_then(|(r, _)| row_border(*r)).map(|b| b[TOP]),
                    if boundary == 0 {
                        table_border.map(|b| b[TOP])
                    } else if boundary == n_rows {
                        table_border.map(|b| b[BOTTOM])
                    } else {
                        None
                    },
                ]);
                let Some(w) = winner else { continue };
                let y = match above_cell.or(below_cell) {
                    Some((_, r)) if above_cell.is_some() => r.origin.y + r.size.height,
                    Some((_, r)) => r.origin.y,
                    None => continue,
                };
                // Extend the strip by its own half-width at both ends so the
                // corners where a wider horizontal border meets a narrower
                // vertical one are filled.
                let x0 = w.width.mul_add(-0.5, seg_rect.origin.x);
                let x1 = w
                    .width
                    .mul_add(0.5, seg_rect.origin.x + seg_rect.size.width);
                builder.push_rect(
                    LogicalRect::new(
                        LogicalPosition::new(x0, w.width.mul_add(-0.5, y)),
                        LogicalSize::new(x1 - x0, w.width),
                    ),
                    w.color,
                    BorderRadius::default(),
                );
            }
        }
    }

    /// Paint rects of a row's cell children, in tree order.
    fn collect_row_cells(&self, row_idx: usize) -> Vec<(usize, LogicalRect)> {
        self.positioned_tree
            .tree
            .children(row_idx)
            .iter()
            .filter_map(|&cell_idx| {
                let rect = self.get_paint_rect(cell_idx)?;
                Some((cell_idx, rect))
            })
            .collect()
    }

    /// Helper function to paint a table row's background and then its cells' backgrounds
    /// Layer 5: Row background
    /// Layer 6: Cell backgrounds (painted after row, so they appear on top)
    fn paint_table_row_and_cells(&self, builder: &mut DisplayListBuilder, row_idx: usize) {
        // Layer 5: Paint row background.
        // Rows don't have entries in calculated_positions (adding them would
        // double-offset cells during position recursion). Compute the row rect
        // from the bounding box of its cell children.
        if let Some(row_node) = self.positioned_tree.tree.get(LayoutNodeId::new(row_idx)) {
            if let Some(dom_id) = row_node.dom_node_id {
                let styled_node_state = self.get_styled_node_state(dom_id);
                let bg_color =
                    get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);
                if bg_color.a > 0 {
                    // Compute row rect from cell children
                    let mut min_x = f32::MAX;
                    let mut min_y = f32::MAX;
                    let mut max_x = f32::MIN;
                    let mut max_y = f32::MIN;
                    for &cell_idx in self.positioned_tree.tree.children(row_idx) {
                        if let Some(cell_rect) = self.get_paint_rect(cell_idx) {
                            min_x = min_x.min(cell_rect.origin.x);
                            min_y = min_y.min(cell_rect.origin.y);
                            max_x = max_x.max(cell_rect.origin.x + cell_rect.size.width);
                            max_y = max_y.max(cell_rect.origin.y + cell_rect.size.height);
                        }
                    }
                    if min_x < max_x && min_y < max_y {
                        let row_rect = LogicalRect::new(
                            LogicalPosition::new(min_x, min_y),
                            LogicalSize::new(max_x - min_x, max_y - min_y),
                        );
                        builder.push_rect(row_rect, bg_color, BorderRadius::default());
                    }
                }
            }
        }

        // Layer 6: Paint cell backgrounds (topmost layer)
        if let Some(_node) = self.positioned_tree.tree.get(LayoutNodeId::new(row_idx)) {
            for &cell_idx in self.positioned_tree.tree.children(row_idx) {
                self.paint_element_background(builder, cell_idx);
            }
        }
    }

    /// Helper function to paint an element's background (used for all table elements)
    /// Reads background-color and border-radius from CSS properties and emits `push_rect()`
    fn paint_element_background(&self, builder: &mut DisplayListBuilder, node_index: usize) {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return;
        };

        let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(node_index)) else {
            return;
        };
        let Some(dom_id) = node.dom_node_id else {
            return;
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);

        // Only paint if background color has alpha > 0 (optimization)
        if bg_color.a == 0 {
            return;
        }

        let element_size = PhysicalSizeImport {
            width: paint_rect.size.width,
            height: paint_rect.size.height,
        };
        let border_radius = get_border_radius(
            self.ctx.styled_dom,
            dom_id,
            &styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        builder.push_rect(paint_rect, bg_color, border_radius);
    }

    /// Emits drawing commands for the foreground content, including hit-test areas and scrollbars.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn paint_node_content(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let _p = crate::probe::Probe::span("dl_content");
        // ENGINE-LEVEL PLACEHOLDER (`placeholder` attribute): painted here,
        // never modelled as a DOM node, so it cannot intercept clicks, own
        // clusters an editing consumer could seat a caret in, or latch a
        // stale visibility override - (empty && unfocused) is recomputed
        // from scratch on every build.
        //
        // Emitted OUTSIDE the cached content run, and BEFORE the copy: the
        // run recorded while the host was empty carries the prompt item, and
        // `try_copy_cached_run` replays a run wholesale - so a cached copy
        // re-emitted the prompt over text the user had just typed (device
        // report, 2026-08-31). Kept out of the cache, the prompt is decided
        // fresh on every emission, cached run or not.
        if let Some(rect) = self.get_paint_rect(node_index) {
            self.maybe_paint_placeholder_prompt(builder, node_index, &rect)?;
        }
        if self.try_copy_cached_run(builder, node_index, EmitPhase::Content) {
            return Ok(());
        }
        builder.set_current_layout(Some((node_index, EmitPhase::Content)));
        let result = self.paint_node_content_inner(builder, node_index);
        builder.set_current_layout(None);
        result
    }

    fn paint_node_content_inner(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        // CSS 2.2 §11.2: visibility:hidden — skip painting content for hidden nodes.
        if self.is_node_hidden(node_index) {
            return Ok(());
        }

        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;
        let node_warm = self
            .positioned_tree
            .tree
            .warm(LayoutNodeId::new(node_index));

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        let Some(mut paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // For text nodes (with inline layout), the used_size might be 0x0.
        // In this case, compute the bounds from the inline layout result.
        if paint_rect.size.width == 0.0 || paint_rect.size.height == 0.0 {
            if let Some(cached_layout) = node_warm.and_then(|w| w.inline_layout_result.as_ref()) {
                let content_bounds = cached_layout.layout.bounds();
                paint_rect.size.width = content_bounds.width;
                paint_rect.size.height = content_bounds.height;
            }
        }

        // Add a hit-test area for this node if it's interactive.
        // NOTE: For scrollable containers (overflow: scroll/auto), the hit-test area
        // was already pushed in generate_for_stacking_context BEFORE the scroll frame,
        // so we skip it here to avoid duplicate hit-test areas that would scroll with content.
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, node.dom_node_id) {
            let is_scrollable = if let Some(dom_id) = node.dom_node_id {
                let styled_node_state = self.get_styled_node_state(dom_id);
                let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
                let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);
                overflow_x.is_scroll() || overflow_y.is_scroll()
            } else {
                false
            };

            // Push hit-test area for this node ONLY if it's not a scrollable container.
            // Scrollable containers already have their hit-test area pushed BEFORE the scroll frame
            // in generate_for_stacking_context, ensuring the hit-test stays stationary in parent space
            // while content scrolls. Pushing it again here would create a duplicate that scrolls
            // with content, causing hit-test failures when scrolled to the bottom.
            if !is_scrollable {
                builder.push_hit_test_area(paint_rect, tag_id);
            }
        }

        // Paint the node's visible content.
        if let Some(cached_layout) = node_warm.and_then(|w| w.inline_layout_result.as_ref()) {
            let inline_layout = &cached_layout.layout;
            // (d6h) Dense-aware count: the stored sparse may be the
            // retirement sentinel; logs should report the real content.
            let logged_item_count = cached_layout
                .dense
                .as_deref()
                .filter(|d| !d.clusters.is_empty())
                .map_or(inline_layout.items.len(), |d| d.clusters.len());
            debug_info!(
                self.ctx,
                "[paint_node] node {} has inline_layout with {} items",
                node_index,
                logged_item_count
            );

            if let Some(dom_id) = node.dom_node_id {
                let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                debug_info!(
                    self.ctx,
                    "Painting inline content for node {} ({:?}) at {:?}, {} layout items",
                    node_index,
                    node_type.get_node_type(),
                    paint_rect,
                    logged_item_count
                );
            }

            // paint_rect is the border-box, but inline layout positions are relative to
            // content-box. Use type-safe conversion to make this clear and avoid manual
            // calculations.
            let border_box = BorderBoxRect(paint_rect);
            let nbp = node.box_props.unpack();
            let mut content_box_rect = border_box.to_content_box(&nbp.padding, &nbp.border).rect();

            // Save the viewport-sized content box for clipping BEFORE expanding
            // to full scroll content size. Text must be clipped to the viewport
            // when overflow is hidden/scroll/auto, not to the full content size.
            let mut viewport_clip_rect = content_box_rect;

            // For scrollable containers, extend the content rect to the full content size.
            // The scroll frame handles clipping - we need to paint ALL content, not just
            // what fits in the viewport. Otherwise glyphs beyond the viewport are not rendered.
            let content_size = get_scroll_content_size(node, node_warm);
            if content_size.height > content_box_rect.size.height {
                content_box_rect.size.height = content_size.height;
            }
            if content_size.width > content_box_rect.size.width {
                content_box_rect.size.width = content_size.width;
            }

            // An overflow-VISIBLE axis does not clip its own inline content:
            // the ink paints past the box and only the nearest clipping
            // ANCESTOR (scroll frame / overflow clip) bounds it. The tight
            // box here was wrong twice over for such a node — the renderer
            // paints those glyphs anyway, and `Text::visual_bounds()` clamps
            // its DAMAGE into this rect. After a text edit grew an IFC past
            // its stale box (`used_size` only catches up on the next
            // relayout), every moved line's pixels lay outside the clamp and
            // the incremental present never repainted them — the TextArea's
            // garbled-lines-below-the-Enter bug. Per axis: only an axis that
            // actually clips (hidden/scroll/auto) keeps the viewport bound;
            // a visible axis takes the full content extent computed above,
            // which `reshape_text_node` keeps fresh between relayouts.
            if let Some(dom_id) = node.dom_node_id {
                let st = self.get_styled_node_state(dom_id);
                let clips = |ov: crate::solver3::getters::MultiValue<LayoutOverflow>| {
                    matches!(
                        ov,
                        crate::solver3::getters::MultiValue::Exact(
                            LayoutOverflow::Hidden | LayoutOverflow::Scroll | LayoutOverflow::Auto
                        )
                    )
                };
                if !clips(get_overflow_x(self.ctx.styled_dom, dom_id, &st)) {
                    viewport_clip_rect.size.width = content_box_rect.size.width;
                }
                if !clips(get_overflow_y(self.ctx.styled_dom, dom_id, &st)) {
                    viewport_clip_rect.size.height = content_box_rect.size.height;
                }
            }

            // Check for text-shadow and wrap inline content with push/pop shadow
            let mut pushed_text_shadow = false;
            if let Some(dom_id) = node.dom_node_id {
                let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
                if let Some(shadow_val) = self
                    .ctx
                    .styled_dom
                    .css_property_cache
                    .ptr
                    .get_text_shadow(node_data, &dom_id, node_state)
                {
                    if let Some(shadow) = shadow_val.get_property() {
                        builder.push_item(DisplayListItem::PushTextShadow { shadow: (**shadow) });
                        pushed_text_shadow = true;
                    }
                }
            }

            self.paint_inline_content(
                builder,
                content_box_rect,
                viewport_clip_rect,
                inline_layout,
                cached_layout.dense.as_deref(),
                &cached_layout.payload,
                &cached_layout.glyph_runs,
                node_index,
            );

            if pushed_text_shadow {
                builder.push_item(DisplayListItem::PopTextShadow);
            }
        } else if let Some(dom_id) = node.dom_node_id {
            // +spec:replaced-elements:edd21b - block-level replaced element painted atomically per E.2
            // +spec:replaced-elements:516b2a - replaced content painted atomically in painting order
            // This node might be a simple replaced element, like an <img> tag.
            // Content resolves overlay→DOM: a runtime-swapped image or produced
            // callback frame (overlay) wins over the immutable DOM's ImageRef.
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if matches!(node_data.get_node_type(), NodeType::Image(_)) {
                if let Some(image_ref) = self.ctx.resolved_content().image_for_paint(dom_id) {
                    debug_info!(
                        self.ctx,
                        "Painting image for node {} at {:?}",
                        node_index,
                        paint_rect
                    );
                    // Get border-radius so the compositor can clip the image to rounded corners
                    let styled_node_state = self.get_styled_node_state(dom_id);
                    let element_size = PhysicalSizeImport {
                        width: paint_rect.size.width,
                        height: paint_rect.size.height,
                    };
                    let border_radius = get_border_radius(
                        self.ctx.styled_dom,
                        dom_id,
                        &styled_node_state,
                        element_size,
                        self.ctx.viewport_size,
                    );
                    // Store the ImageRef directly in the display list
                    builder.push_image(paint_rect, image_ref, border_radius);
                }
            }
        }

        Ok(())
    }

    /// Paints the `placeholder` attribute's prompt for an EMPTY, unfocused
    /// contenteditable host: the host's own resolved style at half alpha,
    /// one line at the content-box origin, clipped to the content box.
    ///
    /// Deliberately NOT a DOM node: no hit area, no clusters, no focus
    /// participation, no show/hide state - the overlay-`<p>` design this
    /// replaces intercepted clicks meant for the editable and its
    /// imperative visibility toggles latched (TextArea's prompt never
    /// returned after blur, 2026-08-31).
    #[cfg(feature = "text_layout")]
    #[allow(clippy::cast_precision_loss)]
    fn maybe_paint_placeholder_prompt(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        paint_rect: &LogicalRect,
    ) -> Result<()> {
        let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(node_index)) else {
            return Ok(());
        };
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
        let Some(text) = node_data.get_placeholder() else {
            return Ok(());
        };
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PH_DEBUG").is_some() {
            std::eprintln!("[ph] attr present");
        }
        if text.trim().is_empty() {
            return Ok(());
        }
        if !super::getters::is_node_contenteditable_inherited(self.ctx.styled_dom, dom_id) {
            return Ok(());
        }
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PH_DEBUG").is_some() {
            std::eprintln!("[ph] editable ok");
        }
        // Only an EMPTY host shows its prompt, and "empty" is asked of the
        // CONTENT, not of the layout: an earlier version read
        // `inline_layout_result` and treated its ABSENCE as emptiness, so a
        // filled but defocused field painted the prompt UNDER its own text.
        //
        // The question is asked of the EDITING HOST, not of the node holding
        // the attribute. A live edit is recorded against the host (the
        // focused contenteditable container), while the attribute rides on
        // the value line inside it - so a check that only looked at the
        // attribute's node and its descendants never saw the text the user
        // had just typed, and the prompt painted over it (device report,
        // 2026-08-31). Walk UP to the host, then check the host and its
        // whole subtree, resolving overlay-then-DOM the way the a11y tree
        // and the exporters do, so an uncommitted edit counts immediately.
        let resolved = self.ctx.resolved_content();
        let hierarchy = self.ctx.styled_dom.node_hierarchy.as_container();
        let node_data = self.ctx.styled_dom.node_data.as_container();
        let host = {
            let mut h = dom_id;
            loop {
                if node_data.get(h).is_some_and(azul_core::dom::NodeData::is_contenteditable) {
                    break h;
                }
                match hierarchy
                    .get(h)
                    .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id)
                {
                    Some(p) => h = p,
                    None => break dom_id,
                }
            }
        };
        let has_text = |n: NodeId| {
            resolved
                .text_for_node(n)
                .is_some_and(|t| !t.trim().is_empty())
        };
        let mut content_empty = !has_text(host);
        if content_empty {
            let mut stack = hierarchy
                .get(host)
                .and_then(|h| h.first_child_id(host))
                .into_iter()
                .collect::<Vec<_>>();
            while let Some(n) = stack.pop() {
                if has_text(n) {
                    content_empty = false;
                    break;
                }
                if let Some(h) = hierarchy.get(n) {
                    if let Some(c) = h.first_child_id(n) {
                        stack.push(c);
                    }
                    if let Some(sib) = h.next_sibling_id() {
                        stack.push(sib);
                    }
                }
            }
        }
        if !content_empty {
            return Ok(());
        }
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PH_DEBUG").is_some() {
            std::eprintln!("[ph] empty ok");
        }
        if super::getters::is_focus_within_or_above(self.ctx.styled_dom, dom_id) {
            return Ok(());
        }
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PH_DEBUG").is_some() {
            std::eprintln!("[ph] unfocused ok");
        }

        let bp = node.box_props.unpack();
        let content_box = BorderBoxRect(*paint_rect)
            .to_content_box(&bp.padding, &bp.border)
            .rect();
        if content_box.size.width <= 0.0 || content_box.size.height <= 0.0 {
            return Ok(());
        }

        // THE PROMPT'S OWN STYLE, through the real cascade: resolve the node
        // with `::placeholder` turned on, so author rules
        // (`.field::placeholder { color: ... }`) and the widgets' inline
        // `on_placeholder(...)` declarations both land here. Everything the
        // rule does not set falls back to the host's own value, which is why
        // an unstyled prompt still inherits the field's font.
        let style = std::sync::Arc::new(super::getters::get_style_properties_for_state(
            self.ctx.styled_dom,
            dom_id,
            self.ctx.system_style.as_ref(),
            azul_css::props::basic::PhysicalSize {
                width: self.ctx.viewport_size.width,
                height: self.ctx.viewport_size.height,
            },
            &{
                let mut st = self.get_styled_node_state(dom_id);
                st.placeholder = true;
                st
            },
        ));
        // Whether the cascade actually said anything about the prompt's
        // colour. If it did not, the engine keeps its own default (the
        // host's colour at half alpha) rather than painting the value's
        // full-strength colour on top of nothing.
        let prompt_color_declared = self
            .ctx
            .styled_dom
            .css_property_cache
            .ptr
            .get_text_color(
                &self.ctx.styled_dom.node_data.as_container()[dom_id],
                &dom_id,
                &{
                    let mut st = self.get_styled_node_state(dom_id);
                    st.placeholder = true;
                    st
                },
            )
            .is_some();

        let loaded_fonts = self.ctx.font_manager.get_loaded_fonts();
        let glyphs = crate::text3::cache::shape_placeholder_text(
            text,
            &style,
            &self.ctx.font_manager.font_chain_cache,
            &self.ctx.font_manager.fc_cache,
            &loaded_fonts,
        );
        #[cfg(feature = "std")]
        if std::env::var_os("AZ_PH_DEBUG").is_some() {
            std::eprintln!("[ph] shaped {} glyphs (chain cached: {})", glyphs.len(),
                !self.ctx.font_manager.font_chain_cache.is_empty());
        }
        if glyphs.is_empty() {
            return Ok(());
        }

        // A declared `::placeholder` colour is used verbatim. With none, the
        // default is the host's own colour at half alpha - readable on any
        // theme, with no second colour channel to keep in sync.
        let color = if prompt_color_declared {
            style.color
        } else {
            azul_css::props::basic::ColorU {
                a: style.color.a / 2,
                ..style.color
            }
        };

        // One line at the content-box origin: baseline = origin + scaled
        // ascent of the first glyph's face.
        let first = &glyphs[0];
        let upem = f32::from(first.font_metrics.units_per_em.max(1));
        let baseline_y =
            content_box.origin.y + first.font_metrics.ascent / upem * style.font_size_px;

        // Split into per-font runs (fallback can mix faces).
        let mut pen_x = content_box.origin.x;
        let mut run: Vec<GlyphInstance> = Vec::with_capacity(glyphs.len());
        let mut run_hash = first.font_hash;
        for g in &glyphs {
            if g.font_hash != run_hash && !run.is_empty() {
                builder.push_text_run(
                    core::mem::take(&mut run),
                    crate::text3::cache::FontHash { font_hash: run_hash },
                    style.font_size_px,
                    color,
                    content_box,
                    None,
                    None,
                );
                run_hash = g.font_hash;
            }
            run.push(GlyphInstance {
                index: u32::from(g.glyph_id),
                point: LogicalPosition {
                    x: pen_x + g.offset.x,
                    y: baseline_y - g.offset.y,
                },
                size: LogicalSize::zero(),
            });
            pen_x += g.advance + g.kerning;
        }
        if !run.is_empty() {
            builder.push_text_run(
                run,
                crate::text3::cache::FontHash { font_hash: run_hash },
                style.font_size_px,
                color,
                content_box,
                None,
                None,
            );
        }
        Ok(())
    }

    #[cfg(not(feature = "text_layout"))]
    fn maybe_paint_placeholder_prompt(
        &mut self,
        _builder: &mut DisplayListBuilder,
        _node_index: usize,
        _paint_rect: &LogicalRect,
    ) -> Result<()> {
        Ok(())
    }

    /// Emits drawing commands for scrollbars. This is called AFTER popping the scroll frame
    /// clip so scrollbars appear on top of content and are not clipped.
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn paint_scrollbars(&self, builder: &mut DisplayListBuilder, node_index: usize) -> Result<()> {
        // CSS 2.2 §11.2: visibility:hidden scroll containers must not paint scrollbars,
        // but their layout space is preserved (already handled by layout).
        if self.is_node_hidden(node_index) {
            return Ok(());
        }

        let node = self
            .positioned_tree
            .tree
            .get(LayoutNodeId::new(node_index))
            .ok_or(LayoutError::InvalidTree)?;

        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // Check if we need to draw scrollbars for this node.
        let mut scrollbar_info = self
            .positioned_tree
            .tree
            .warm(LayoutNodeId::new(node_index))
            .and_then(|w| w.scrollbar_info)
            .unwrap_or_default();

        // Get node_id for GPU cache lookup and CSS style lookup
        let node_id = node.dom_node_id;

        // A VirtualView is a replaced element with NO flow content, so the
        // layout-side necessity test (`check_scrollbar_necessity`: laid-out
        // content > container) can never fire for it and `overflow: auto` would
        // stay bar-less no matter how large the virtualized document is. The
        // virtual size does reach the display list — the ScrollManager puts the
        // callback's `virtual_scroll_size` into `ScrollPosition::children_rect`,
        // which the thumb geometry below already prefers — so decide from that,
        // through the one function that owns the rule.
        //
        // The answer cannot simply be read off `warm.scrollbar_info` here:
        // `register_scroll_nodes` writes the same amendment back into the tree
        // for the GPU/hit-test consumers, but it runs AFTER this display list is
        // built, and the layout pass that precedes this one has already
        // recomputed `warm.scrollbar_info` from the laid-out sizes.
        if let Some(nid) = node_id {
            if std::env::var("AZ_VV_BAR_TRACE").is_ok()
                && self
                    .ctx
                    .styled_dom
                    .node_data
                    .as_container()
                    .get(nid)
                    .is_some_and(azul_core::dom::NodeData::is_virtual_view_node)
            {
                eprintln!(
                    "[vv-bar] nid={nid:?} offsets_hit={} n_offsets={} reqs_before={scrollbar_info:?}",
                    self.scroll_offsets.contains_key(&nid),
                    self.scroll_offsets.len(),
                );
            }
            if let Some(pos) = self.scroll_offsets.get(&nid) {
                let bp = node.box_props.unpack();
                let border = &bp.border;
                let padding_box_size = LogicalSize::new(
                    (paint_rect.size.width - border.left - border.right).max(0.0),
                    (paint_rect.size.height - border.top - border.bottom).max(0.0),
                );
                let raised = crate::solver3::cache::apply_virtual_scroll_necessity(
                    self.ctx.styled_dom,
                    nid,
                    pos.children_rect.size,
                    padding_box_size,
                    &mut scrollbar_info,
                );
                if std::env::var("AZ_VV_BAR_TRACE").is_ok() {
                    eprintln!(
                        "[vv-bar] nid={nid:?} raised={raised} virt={:?} padbox={padding_box_size:?} reqs_after={scrollbar_info:?}",
                        pos.children_rect.size,
                    );
                }
            }
        }

        // Get CSS scrollbar style for this node (cached per LayoutContext).
        let scrollbar_style = node_id
            .map(|nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                crate::solver3::getters::get_scrollbar_style_cached(self.ctx, nid, node_state)
            })
            .unwrap_or_default();

        // Skip if scrollbar-width: none
        if matches!(
            scrollbar_style.width_mode,
            azul_css::props::style::scrollbar::LayoutScrollbarWidth::None
        ) {
            return Ok(());
        }

        // +spec:overflow:3dfb2c - when scrollbar gutter is present but scrollbar is not,
        // paint the gutter background as an extension of the padding
        let scrollbar_gutter = node_id
            .and_then(|nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                get_scrollbar_gutter_property(self.ctx.styled_dom, nid, node_state).exact()
            })
            .unwrap_or_default();
        let gutter_is_stable = matches!(
            scrollbar_gutter,
            azul_css::props::layout::overflow::StyleScrollbarGutter::Stable
                | azul_css::props::layout::overflow::StyleScrollbarGutter::StableBothEdges
        );
        let gutter_both_edges = matches!(
            scrollbar_gutter,
            azul_css::props::layout::overflow::StyleScrollbarGutter::StableBothEdges
        );

        if gutter_is_stable {
            let gbp = node.box_props.unpack();
            let border = &gbp.border;
            let gutter_width = scrollbar_style.visual_width_px;
            // Paint gutter as padding extension when scrollbar is absent
            let bg_color = node_id.map_or(ColorU::TRANSPARENT, |nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                get_background_color(self.ctx.styled_dom, nid, node_state)
            });

            if !scrollbar_info.needs_vertical && gutter_width > 0.0 {
                // Right-side gutter (inline-end)
                let gutter_rect = LogicalRect {
                    origin: LogicalPosition::new(
                        paint_rect.origin.x + paint_rect.size.width - border.right - gutter_width,
                        paint_rect.origin.y + border.top,
                    ),
                    size: LogicalSize::new(
                        gutter_width,
                        (paint_rect.size.height - border.top - border.bottom).max(0.0),
                    ),
                };
                builder.push_rect(gutter_rect, bg_color, BorderRadius::default());

                // Both-edges: also paint left-side gutter (inline-start)
                if gutter_both_edges {
                    let left_gutter_rect = LogicalRect {
                        origin: LogicalPosition::new(
                            paint_rect.origin.x + border.left,
                            paint_rect.origin.y + border.top,
                        ),
                        size: LogicalSize::new(
                            gutter_width,
                            (paint_rect.size.height - border.top - border.bottom).max(0.0),
                        ),
                    };
                    builder.push_rect(left_gutter_rect, bg_color, BorderRadius::default());
                }
            }
        }

        // Get border dimensions to position scrollbar inside the border-box
        let sbp = node.box_props.unpack();
        let border = &sbp.border;

        // Get border-radius for potential clipping
        let container_border_radius = node_id
            .map(|nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                let element_size = PhysicalSizeImport {
                    width: paint_rect.size.width,
                    height: paint_rect.size.height,
                };
                let viewport_size =
                    LogicalSize::new(self.ctx.viewport_size.width, self.ctx.viewport_size.height);
                get_border_radius(
                    self.ctx.styled_dom,
                    nid,
                    node_state,
                    element_size,
                    viewport_size,
                )
            })
            .unwrap_or_default();

        // Calculate the inner rect (content-box) where scrollbars should be placed
        // Scrollbars are positioned inside the border, at the right/bottom edges
        let inner_rect = LogicalRect {
            origin: LogicalPosition::new(
                paint_rect.origin.x + border.left,
                paint_rect.origin.y + border.top,
            ),
            size: LogicalSize::new(
                (paint_rect.size.width - border.left - border.right).max(0.0),
                (paint_rect.size.height - border.top - border.bottom).max(0.0),
            ),
        };

        // Get scroll position for thumb calculation.
        // `children_rect.origin` IS the scroll offset (see
        // `ScrollManager::get_scroll_states_for_dom`); `parent_rect.origin` is
        // an ABSOLUTE window coordinate, so subtracting it here mixed two
        // spaces and started the thumb `container.y` px down its own track for
        // any scroller not at the window origin. The GPU-only scroll path
        // (`GpuStateManager::update_scrollbar_transforms`) feeds the raw
        // `current_offset` into the same geometry fn — these two MUST agree,
        // because the transform written here is the initial value of the key
        // that path later overwrites.
        let (scroll_offset_x, scroll_offset_y) = node_id
            .and_then(|nid| {
                self.scroll_offsets
                    .get(&nid)
                    .map(|pos| (pos.children_rect.origin.x, pos.children_rect.origin.y))
            })
            .unwrap_or((0.0, 0.0));

        // Get content size for thumb proportional sizing
        // Use the node's get_content_size() method which returns the actual content size
        // from overflow_content_size (set during layout) or computes it from text/children.
        // For VirtualView nodes, the virtual_scroll_size (propagated through ScrollPosition.children_rect)
        // is more accurate than the layout-computed content size.
        let content_size = node_id
            .and_then(|nid| self.scroll_offsets.get(&nid))
            .map_or_else(
                || {
                    self.positioned_tree
                        .tree
                        .get_content_size(LayoutNodeId::new(node_index))
                },
                |pos| pos.children_rect.size,
            );

        // The HANDLE's own width, which is not the groove's: Breeze centres a
        // 6px handle in a 21px groove, and Adwaita/macOS inset theirs too.
        // Filling the groove made every azul scrollbar read as a fat pill next
        // to a native one. `None` keeps the old fill-the-groove behaviour.
        let handle_width = scrollbar_style
            .handle_width_px
            .unwrap_or(scrollbar_style.visual_width_px)
            .min(scrollbar_style.visual_width_px)
            .max(1.0);
        // How far in from each side the handle sits, so it stays centred.
        let handle_inset = ((scrollbar_style.visual_width_px - handle_width) / 2.0).max(0.0);
        // Radius comes from the platform; a capsule is the fallback, not a law.
        let thumb_radius = scrollbar_style
            .handle_radius_px
            .unwrap_or(handle_width / 2.0)
            .max(0.0);
        let thumb_border_radius = BorderRadius {
            top_left: thumb_radius,
            top_right: thumb_radius,
            bottom_left: thumb_radius,
            bottom_right: thumb_radius,
        };

        if scrollbar_info.needs_vertical {
            // Look up opacity key from GPU cache for GPU-animated opacity.
            // If a key already exists in the cache from a previous frame, reuse it.
            // Otherwise, create a new unique key. The key will be registered
            // in the GPU cache after layout_document returns (same pattern as
            // transform keys). This ensures the display list ALWAYS has an
            // opacity binding, so GPU-only scroll updates can animate it.
            let opacity_key = node_id.map(|nid| {
                self.gpu_value_cache
                    .and_then(|cache| {
                        cache
                            .scrollbar_v_opacity_keys
                            .get(&(self.dom_id, nid))
                            .copied()
                    })
                    .unwrap_or_else(OpacityKey::unique)
            });

            // Vertical scrollbar: use shared geometry computation
            let button_size = if scrollbar_style.show_scroll_buttons {
                scrollbar_style.scroll_button_size_px
            } else {
                0.0
            };
            let v_geom = compute_scrollbar_geometry_with_button_size(
                ScrollbarOrientation::Vertical,
                inner_rect,
                content_size,
                scroll_offset_y,
                scrollbar_style.visual_width_px,
                scrollbar_info.needs_horizontal,
                button_size,
            );

            // Position thumb after the top button; GPU transform moves it within usable track
            // Inset horizontally so the handle is CENTRED in the groove
            // rather than filling it (Breeze: 6px handle, 21px groove).
            let thumb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    v_geom.track_rect.origin.x + handle_inset,
                    v_geom.track_rect.origin.y + v_geom.button_size,
                ),
                size: LogicalSize::new(
                    (v_geom.width_px - 2.0 * handle_inset).max(1.0),
                    v_geom.thumb_length,
                ),
            };

            // Look up transform key from GPU cache for GPU-animated thumb positioning.
            // If a key already exists in the cache from a previous frame, reuse it.
            // Otherwise, create a new unique key. The key will be registered
            // in the GPU cache after layout_document returns.
            let thumb_transform_key = node_id.map(|nid| {
                self.gpu_value_cache
                    .and_then(|cache| cache.transform_keys.get(&nid).copied())
                    .unwrap_or_else(TransformKey::unique)
            });

            // Initial transform: translate thumb within usable region.
            // Quantised — see `quantize_thumb_offset`; the GPU updater that
            // overwrites this value rounds identically.
            let thumb_initial_transform = ComputedTransform3D::new_translation(
                0.0,
                crate::solver3::scrollbar::quantize_thumb_offset(v_geom.thumb_offset),
                0.0,
            );

            // Generate hit-test ID for vertical scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::VerticalThumb(self.dom_id, nid));

            // Buttons at top/bottom of track (only if enabled in style)
            let (button_decrement_bounds, button_increment_bounds) =
                if scrollbar_style.show_scroll_buttons && v_geom.button_size > 0.0 {
                    (
                        Some(LogicalRect {
                            origin: v_geom.track_rect.origin,
                            size: LogicalSize::new(v_geom.button_size, v_geom.button_size),
                        }),
                        Some(LogicalRect {
                            origin: LogicalPosition::new(
                                v_geom.track_rect.origin.x,
                                v_geom.track_rect.origin.y + v_geom.track_rect.size.height
                                    - v_geom.button_size,
                            ),
                            size: LogicalSize::new(v_geom.button_size, v_geom.button_size),
                        }),
                    )
                } else {
                    (None, None)
                };
            builder.push_scrollbar_styled(ScrollbarDrawInfo {
                bounds: v_geom.track_rect.into(),
                orientation: ScrollbarOrientation::Vertical,
                track_bounds: v_geom.track_rect.into(),
                track_color: scrollbar_style.track_color,
                thumb_bounds: thumb_bounds.into(),
                thumb_color: scrollbar_style.thumb_color,
                thumb_border_radius,
                button_decrement_bounds: button_decrement_bounds.map(Into::into),
                button_increment_bounds: button_increment_bounds.map(Into::into),
                button_color: scrollbar_style.button_color,
                opacity_key,
                thumb_transform_key,
                thumb_initial_transform,
                hit_id,
                clip_to_container_border: scrollbar_style.clip_to_container_border,
                container_border_radius,
                visibility: scrollbar_style.visibility,
            });
        }

        if scrollbar_info.needs_horizontal {
            // Look up horizontal opacity key from GPU cache (same pattern as vertical).
            let opacity_key = node_id.map(|nid| {
                self.gpu_value_cache
                    .and_then(|cache| {
                        cache
                            .scrollbar_h_opacity_keys
                            .get(&(self.dom_id, nid))
                            .copied()
                    })
                    .unwrap_or_else(OpacityKey::unique)
            });

            // Horizontal scrollbar: use shared geometry computation
            let h_button_size = if scrollbar_style.show_scroll_buttons {
                scrollbar_style.scroll_button_size_px
            } else {
                0.0
            };
            let h_geom = compute_scrollbar_geometry_with_button_size(
                ScrollbarOrientation::Horizontal,
                inner_rect,
                content_size,
                scroll_offset_x,
                scrollbar_style.visual_width_px,
                scrollbar_info.needs_vertical,
                h_button_size,
            );

            // Position thumb after the left button; GPU transform moves it within usable track
            // Same inset on the other axis: the handle is centred in the groove.
            let thumb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    h_geom.track_rect.origin.x + h_geom.button_size,
                    h_geom.track_rect.origin.y + handle_inset,
                ),
                size: LogicalSize::new(
                    h_geom.thumb_length,
                    (h_geom.width_px - 2.0 * handle_inset).max(1.0),
                ),
            };

            // Look up horizontal transform key from GPU cache for GPU-animated thumb positioning.
            let thumb_transform_key = node_id.map(|nid| {
                self.gpu_value_cache
                    .and_then(|cache| cache.h_transform_keys.get(&nid).copied())
                    .unwrap_or_else(TransformKey::unique)
            });
            let thumb_initial_transform = ComputedTransform3D::new_translation(
                crate::solver3::scrollbar::quantize_thumb_offset(h_geom.thumb_offset),
                0.0,
                0.0,
            );

            // Generate hit-test ID for horizontal scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::HorizontalThumb(self.dom_id, nid));

            // Buttons at left/right of track (only if enabled in style)
            let (button_decrement_bounds, button_increment_bounds) =
                if scrollbar_style.show_scroll_buttons && h_geom.button_size > 0.0 {
                    (
                        Some(LogicalRect {
                            origin: h_geom.track_rect.origin,
                            size: LogicalSize::new(h_geom.button_size, h_geom.button_size),
                        }),
                        Some(LogicalRect {
                            origin: LogicalPosition::new(
                                h_geom.track_rect.origin.x + h_geom.track_rect.size.width
                                    - h_geom.button_size,
                                h_geom.track_rect.origin.y,
                            ),
                            size: LogicalSize::new(h_geom.button_size, h_geom.button_size),
                        }),
                    )
                } else {
                    (None, None)
                };
            builder.push_scrollbar_styled(ScrollbarDrawInfo {
                bounds: h_geom.track_rect.into(),
                orientation: ScrollbarOrientation::Horizontal,
                track_bounds: h_geom.track_rect.into(),
                track_color: scrollbar_style.track_color,
                thumb_bounds: thumb_bounds.into(),
                thumb_color: scrollbar_style.thumb_color,
                thumb_border_radius,
                button_decrement_bounds: button_decrement_bounds.map(Into::into),
                button_increment_bounds: button_increment_bounds.map(Into::into),
                button_color: scrollbar_style.button_color,
                opacity_key,
                thumb_transform_key,
                thumb_initial_transform,
                hit_id,
                clip_to_container_border: scrollbar_style.clip_to_container_border,
                container_border_radius,
                visibility: scrollbar_style.visibility,
            });
        }

        Ok(())
    }

    /// Converts the rich layout information from `text3` into drawing commands.
    #[allow(clippy::suboptimal_flops)] // mul_add not guaranteed faster/available without target +fma; keep explicit a*b+c
    #[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
    fn paint_inline_content(
        &self,
        builder: &mut DisplayListBuilder,
        container_rect: LogicalRect,
        viewport_clip_rect: LogicalRect,
        layout: &Arc<UnifiedLayout>,
        dense_view: Option<&crate::text3::dense::DenseText>,
        payload: &Arc<dyn std::any::Any + Send + Sync>,
        glyph_runs: &[crate::text3::glyphs::CompactGlyphRun],
        source_node_index: usize,
    ) {
        let _p = crate::probe::Probe::span("dl_inline_text");
        // TODO: This will always paint images over the glyphs
        // TODO: Handle z-index within inline content (e.g. background images)
        // NOTE: Text decorations (underline, strikethrough, overline) are handled in push_text_layout_to_display_list
        // TODO: Text shadows not yet implemented
        // NOTE: Text-overflow ellipsis is handled via apply_text_overflow_ellipsis()
        // which can be called as a post-processing step on the display list when
        // the node has overflow:hidden and text-overflow:ellipsis CSS properties.
        // +spec:overflow:7807b1 - text-overflow ellipsis side depends on direction (RTL clips left, LTR clips right); not yet implemented
        // +spec:overflow:bbf9c1 - text-overflow ellipsis should only truncate content
        // that is actually clipped; as content scrolls into view, show it instead of ellipsis
        // TODO: Handle text overflowing (based on container_rect and overflow behavior)

        // Calculate actual content bounds from the layout
        // Use these bounds instead of container_rect to avoid inflated bounds
        // that extend beyond actual text content
        // (d6h) Sentinel-aware: bounds() over the empty retirement
        // sentinel gated EVERY TextLayout DL item off — textless PDF
        // export and missing a11y metadata (caught by
        // dom_to_pdf_embeds_text_fonts under the d7 default flip). The
        // dense extent is the same math as the scroll-extent arm.
        let layout_bounds = if layout.items.is_empty() {
            match dense_view.filter(|d| !d.clusters.is_empty()) {
                Some(d) => {
                    let w = d
                        .clusters
                        .iter()
                        .map(|c| c.x + c.advance)
                        .fold(0.0f32, f32::max);
                    let h = d
                        .lines
                        .iter()
                        .map(|l| l.top_y + l.height)
                        .fold(0.0f32, f32::max);
                    crate::text3::cache::Rect {
                        x: 0.0,
                        y: 0.0,
                        width: w,
                        height: h,
                    }
                }
                None => layout.bounds(),
            }
        } else {
            layout.bounds()
        };
        let actual_bounds = if layout_bounds.width > 0.0 && layout_bounds.height > 0.0 {
            LogicalRect {
                origin: container_rect.origin,
                size: LogicalSize {
                    width: layout_bounds.width,
                    height: layout_bounds.height,
                },
            }
        } else {
            // If layout has no content, don't push TextLayout item at all
            // This prevents 0x0 TextLayout items that pollute height calculation
            LogicalRect {
                origin: container_rect.origin,
                size: LogicalSize::default(),
            }
        };

        // Only push TextLayout if layout has actual content
        // This prevents empty TextLayout items with 0x0 bounds at various Y positions
        // from affecting pagination height calculations
        //
        // (#25 / user ruling 2026-08-12) AND only for PAGED (PDF-export)
        // display lists: `TextLayout` is the type-erased shaped-layout
        // payload printpdf's bridge and the font-embedding walk consume.
        // The screen renderers all no-op it (raster: "metadata for
        // PDF/accessibility — skip"; compositor2: logs and moves on), so
        // a screen DL was carrying one dead item + payload-Arc clone per
        // painted IFC. `fragmentation_context.is_some()` IS the
        // screen-vs-paged discriminator (paged entries own it; the window
        // path never sets it) — no new target input needed.
        let is_paged_target = self.ctx.fragmentation_context.is_some();
        if is_paged_target && (layout_bounds.width > 0.0 || layout_bounds.height > 0.0) {
            // The item-level font is the layout's PRIMARY font: the first
            // shaped glyph that resolved one. This used to be a hardcoded
            // `FontHash::from_hash(0)` placeholder ("will be updated per
            // glyph run" — nothing ever did), so every TextLayout shipped
            // `font_hash: 0`, and once `push_item` began dropping hash-0
            // TextLayouts as unresolved, ALL of them vanished — text
            // disappeared from the PDF export and a11y metadata. Per-glyph
            // consumers still read each glyph's own hash; staying at 0 here
            // is now reserved for a layout in which NO glyph resolved, which
            // is exactly the case the drop guard exists for.
            // §3.2 (d3): from the dense runs when retained (runs carry
            // font_hash + style directly; hash-0 runs = unresolved, skipped
            // exactly like the sparse scan). Falls back to the sparse walk
            // for mixed layouts or when no dense view is retained.
            let mut primary: Option<(u64, f32)> = None;
            if let Some(d) = dense_view {
                // (d6h) Sentinel-aware: empty stored items + non-empty
                // dense = the retirement form, dense is authoritative.
                if !d.clusters.is_empty()
                    && (layout.items.is_empty() || d.clusters.len() == layout.items.len())
                {
                    primary = d
                        .runs
                        .iter()
                        .find(|r| r.font_hash != 0)
                        .map(|r| (r.font_hash, r.style.font_size_px));
                }
            }
            if primary.is_none() {
                for positioned in &layout.items {
                    let (glyphs, arm_style) = match &positioned.item {
                        ShapedItem::Cluster(c) => (&c.glyphs, &c.style),
                        ShapedItem::CombinedBlock { glyphs, style, .. } => (glyphs, style),
                        _ => continue,
                    };
                    if let Some(g) = glyphs.iter().find(|g| g.font_hash != 0) {
                        primary = Some((g.font_hash, arm_style.font_size_px));
                        break;
                    }
                }
            }
            let (primary_hash, primary_size) = primary.unwrap_or((0, 12.0));
            // Clone the CACHED Arc, do not re-wrap a deep clone: TextLayout
            // damage diffing is Arc::ptr_eq, so a fresh Arc per rebuild made
            // every blink / tween tick repaint the whole text run (and deep-
            // cloned all shaped glyphs per frame). Real text changes replace
            // the cached Arc, so ptr_eq still fires damage then.
            builder.push_text_layout(
                // (d5) The CACHED payload Arc — TextPayload{dense,sparse}
                // when the dense view is retained, the bare layout Arc
                // otherwise. Cloned from the cache entry, so ptr_eq damage
                // diffing sees the same allocation across paints exactly
                // as before.
                payload.clone(),
                actual_bounds,
                FontHash::from_hash(primary_hash),
                primary_size,
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                }, // Default color
            );
        }

        // Precomputed at CachedInlineLayout store time — the run-grouping
        // walk used to re-run here on EVERY paint of every IFC.

        // FIRST PASS: Render backgrounds (solid colors, gradients) and borders for each glyph run
        // This must happen BEFORE rendering text so that backgrounds appear behind text.
        for glyph_run in glyph_runs {
            // Calculate the bounding box for this glyph run
            if let (Some(first_glyph), Some(last_glyph)) =
                (glyph_run.glyphs.first(), glyph_run.glyphs.last())
            {
                // Calculate run bounds from glyph positions
                let run_start_x = container_rect.origin.x + first_glyph.point.x;
                let run_end_x = container_rect.origin.x + last_glyph.point.x;
                let run_width = (run_end_x - run_start_x).max(0.0);

                // Skip if run has no width
                if run_width <= 0.0 {
                    continue;
                }

                // Approximate height based on font size (baseline is at glyph.point.y)
                let baseline_y = container_rect.origin.y + first_glyph.point.y;
                let font_size = glyph_run.font_size_px;
                let ascent = font_size * APPROX_ASCENT_RATIO;

                let mut run_bounds = LogicalRect::new(
                    LogicalPosition::new(run_start_x, baseline_y - ascent),
                    LogicalSize::new(run_width, font_size),
                );

                // Expand run_bounds by padding + border so the background/border
                // rect covers the full inline box, not just the glyph area.
                if let Some(border) = &glyph_run.border {
                    let left_inset = border.left_inset();
                    let right_inset = border.right_inset();
                    let top_inset = border.top_inset();
                    let bottom_inset = border.bottom_inset();

                    run_bounds.origin.x -= left_inset;
                    run_bounds.origin.y -= top_inset;
                    run_bounds.size.width += left_inset + right_inset;
                    run_bounds.size.height += top_inset + bottom_inset;
                }

                builder.push_inline_backgrounds_and_border(
                    run_bounds,
                    glyph_run.background_color,
                    &glyph_run.background_content,
                    glyph_run.border.as_ref(),
                    self.ctx.image_cache,
                );
            }
        }

        // The IFC-level background proof, refined per run below (a run
        // carrying its OWN background paints it directly underneath).
        let ifc_uniform_bg = self.compute_uniform_text_bg(source_node_index);

        // The `::selection` recolour band, resolved once for the whole IFC.
        let selection_recolour = self.selection_recolour_for_ifc(source_node_index);

        // SECOND PASS: Render text runs
        for glyph_run in glyph_runs {
            // Clip text to the viewport-sized content box, not the full scroll
            // content area. This prevents text from overflowing outside the
            // container when overflow is hidden/scroll/auto.
            let clip_rect = viewport_clip_rect;

            // Offset glyph positions by the container origin (text layout is
            // relative to (0,0) of the IFC). (#25) The runs are stored
            // compact; this expansion builds the same Vec the pre-#25 code
            // built by copy-then-offset — construct instead of memcpy.
            let offset_glyphs: Vec<GlyphInstance> = glyph_run
                .glyphs
                .to_vec_offset(container_rect.origin.x, container_rect.origin.y);

            // Store only the font hash in the display list to keep it lean
            let uniform_bg = if glyph_run.background_content.is_empty() {
                match glyph_run.background_color {
                    // A span's own opaque background covers only the run —
                    // no cheap proven-RECT for the fringe boundary here, so
                    // the run takes the sweep. (The common body-text case is
                    // "no span background" → the ancestor proof below.)
                    Some(c) if c.a == 255 => {
                        let _ = c;
                        None
                    }
                    Some(c) if c.a > 0 => None, // translucent span bg
                    _ => ifc_uniform_bg,
                }
            } else {
                None // gradient/image span background — unprovable
            };
            // Colour is paint-only and deliberately excluded from the text
            // layout hash, so a cached run can outlive the cascade that
            // resolved its colour (the deactivated-ribbon-tab KNOWN GAP:
            // a colour-only change never re-lays the IFC owner, and the
            // baked value painted stale). Re-resolve against the CURRENT
            // cascade at build time; the baked colour only serves runs
            // without a source node (markers, synthesized content).
            let live_color = glyph_run
                .source_node_id
                .and_then(|nid| {
                    let sd = self.ctx.styled_dom;
                    let styled_nodes = sd.styled_nodes.as_container();
                    if nid.index() >= styled_nodes.len() {
                        return None;
                    }
                    let cache = &sd.css_property_cache.ptr;
                    let node_data = sd.node_data.as_container();
                    // ANCESTOR USER OVERRIDES participate in inheritance: an
                    // `animation: color ..` transition overrides `color` on a
                    // CONTAINER, and the precomputed inherited tables cannot
                    // see it — the text painted the stale colour (found by
                    // the css_anim_perf_transition damage law: the "colour
                    // transition" repainted nothing). Walk self -> root: the
                    // nearest override wins unless a closer node declares its
                    // OWN colour, which re-roots inheritance below it.
                    let hierarchy = sd.node_hierarchy.as_container();
                    let ty = azul_css::props::property::CssPropertyType::TextColor;
                    let mut cur = Some(nid);
                    while let Some(n) = cur {
                        if let Some(azul_css::props::property::CssProperty::TextColor(v)) =
                            cache.get_user_override(&n, &ty)
                        {
                            if let Some(c) = v.get_property() {
                                return Some(c.inner);
                            }
                            break;
                        }
                        if n.index() < node_data.len()
                            && cache.has_own_declaration(&node_data[n], &n, &ty)
                        {
                            break;
                        }
                        cur = hierarchy
                            .get(n)
                            .and_then(azul_core::styled_dom::NodeHierarchyItem::parent_id);
                    }
                    let node_state = &styled_nodes[nid].styled_node_state;
                    Some(
                        cache
                            .get_text_color_or_default(&node_data[nid], &nid, node_state)
                            .inner,
                    )
                })
                .unwrap_or(glyph_run.color);
            match &selection_recolour {
                Some((rects, selected_color)) => {
                    // A glyph's `point` is its pen position ON THE BASELINE at
                    // the left edge of its advance. A selection rect covers
                    // [start_x, end_x) of the clusters it spans and the whole
                    // line box vertically, so the origin of a selected glyph
                    // falls inside it while the origin of the first glyph
                    // AFTER the selection sits exactly on its right edge —
                    // hence the half-open x test. (A zero-advance mark sitting
                    // exactly on the right edge stays unselected; it is one
                    // combining mark at the very end of a selection.)
                    let inside = |g: &GlyphInstance| {
                        rects.iter().any(|r| {
                            g.point.x >= r.min_x() - 0.5
                                && g.point.x < r.max_x() - 0.5
                                && g.point.y >= r.min_y()
                                && g.point.y <= r.max_y()
                        })
                    };
                    let (selected, normal): (Vec<GlyphInstance>, Vec<GlyphInstance>) =
                        offset_glyphs.into_iter().partition(inside);
                    if !normal.is_empty() {
                        builder.push_text_run(
                            normal,
                            FontHash::from_hash(glyph_run.font_hash),
                            glyph_run.font_size_px,
                            live_color,
                            clip_rect,
                            Some(source_node_index),
                            uniform_bg,
                        );
                    }
                    if !selected.is_empty() {
                        builder.push_text_run(
                            selected,
                            FontHash::from_hash(glyph_run.font_hash),
                            glyph_run.font_size_px,
                            *selected_color,
                            clip_rect,
                            Some(source_node_index),
                            // The proven background under a selected glyph is
                            // the HIGHLIGHT, not the ancestor's colour.
                            None,
                        );
                    }
                }
                None => {
                    builder.push_text_run(
                        offset_glyphs,
                        FontHash::from_hash(glyph_run.font_hash),
                        glyph_run.font_size_px,
                        live_color,
                        clip_rect,
                        Some(source_node_index),
                        uniform_bg,
                    );
                }
            }

            // Render text decorations if present OR if this is IME composition preview
            let needs_underline = glyph_run.text_decoration.underline || glyph_run.is_ime_preview;
            let needs_strikethrough = glyph_run.text_decoration.strikethrough;
            let needs_overline = glyph_run.text_decoration.overline;

            if needs_underline || needs_strikethrough || needs_overline {
                // Calculate the bounding box for this glyph run
                if let (Some(first_glyph), Some(last_glyph)) =
                    (glyph_run.glyphs.first(), glyph_run.glyphs.last())
                {
                    let decoration_start_x = container_rect.origin.x + first_glyph.point.x;
                    let decoration_end_x = container_rect.origin.x + last_glyph.point.x;
                    let decoration_width = decoration_end_x - decoration_start_x;

                    // Use font metrics to determine decoration positions
                    // Standard ratios based on CSS specification
                    let font_size = glyph_run.font_size_px;
                    let thickness = (font_size * APPROX_UNDERLINE_THICKNESS_RATIO).max(1.0);

                    // Baseline is at glyph.point.y
                    let baseline_y = container_rect.origin.y + first_glyph.point.y;

                    if needs_underline {
                        // Underline is typically 10-15% below baseline
                        // IME composition always gets underlined
                        let underline_y = baseline_y + (font_size * APPROX_UNDERLINE_OFFSET_RATIO);
                        let underline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, underline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_underline(underline_bounds, glyph_run.color, thickness);
                    }

                    if needs_strikethrough {
                        // Strikethrough is typically 40% above baseline (middle of x-height)
                        let strikethrough_y =
                            baseline_y - (font_size * APPROX_STRIKETHROUGH_OFFSET_RATIO);
                        let strikethrough_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, strikethrough_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_strikethrough(
                            strikethrough_bounds,
                            glyph_run.color,
                            thickness,
                        );
                    }

                    if needs_overline {
                        // Overline is typically at cap-height (75% above baseline)
                        let overline_y = baseline_y - (font_size * APPROX_OVERLINE_OFFSET_RATIO);
                        let overline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, overline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_overline(overline_bounds, glyph_run.color, thickness);
                    }
                }
            }
        }

        // THIRD PASS: Generate hit-test areas for text runs
        // This enables cursor resolution directly on text nodes instead of their containers
        for glyph_run in glyph_runs {
            // Only generate hit-test areas for runs with a source node id
            let Some(source_node_id) = glyph_run.source_node_id else {
                continue;
            };

            // Calculate the bounding box for this glyph run
            if let (Some(first_glyph), Some(last_glyph)) =
                (glyph_run.glyphs.first(), glyph_run.glyphs.last())
            {
                let run_start_x = container_rect.origin.x + first_glyph.point.x;
                let run_end_x = container_rect.origin.x + last_glyph.point.x;
                let run_width = (run_end_x - run_start_x).max(0.0);

                // Skip if run has no width
                if run_width <= 0.0 {
                    continue;
                }

                // Calculate run bounds using font metrics
                let baseline_y = container_rect.origin.y + first_glyph.point.y;
                let font_size = glyph_run.font_size_px;
                let ascent = font_size * APPROX_ASCENT_RATIO;

                let run_bounds = LogicalRect::new(
                    LogicalPosition::new(run_start_x, baseline_y - ascent),
                    LogicalSize::new(run_width, font_size),
                );

                // Query the cursor type for this text node from the CSS property cache
                // Default to Text cursor (I-beam) for text nodes
                let cursor_type = self.get_cursor_type_for_text_node(source_node_id);

                // Construct the hit-test tag for cursor resolution
                // tag.0 = DomId (upper 32 bits) | NodeId (lower 32 bits)
                // tag.1 = TAG_TYPE_CURSOR | cursor_type
                let tag_value =
                    ((self.dom_id.inner as u64) << 32) | (source_node_id.index() as u64);
                let tag_type = TAG_TYPE_CURSOR | (cursor_type as u16);
                let tag_id = (tag_value, tag_type);

                builder.push_hit_test_area(run_bounds, tag_id);
            }
        }

        // Render inline objects (images, shapes/inline-blocks, etc.)
        // These are positioned by the text3 engine and need to be rendered at their calculated
        // positions. §3.2 (d3): a pure-cluster layout (dense len == items
        // len) has no objects BY CONSTRUCTION — skip the walk entirely.
        // (d6h) Sentinel-aware: the retirement form (empty items, dense
        // non-empty) is pure-cluster by construction.
        let pure_clusters = dense_view.is_some_and(|d| {
            !d.clusters.is_empty()
                && (layout.items.is_empty() || d.clusters.len() == layout.items.len())
        });
        if !pure_clusters {
            for positioned_item in &layout.items {
                self.paint_inline_object(builder, container_rect.origin, positioned_item);
            }
        }
    }

    /// Paints a single inline object (image, shape, or inline-block)
    fn paint_inline_object(
        &self,
        builder: &mut DisplayListBuilder,
        base_pos: LogicalPosition,
        positioned_item: &PositionedItem,
    ) {
        let ShapedItem::Object {
            content, bounds, ..
        } = &positioned_item.item
        else {
            // Other item types (e.g., breaks) don't produce painted output.
            return;
        };

        // Calculate the absolute position of this object
        // positioned_item.position is relative to the container
        let object_bounds = LogicalRect::new(
            LogicalPosition::new(
                base_pos.x + positioned_item.position.x,
                base_pos.y + positioned_item.position.y,
            ),
            LogicalSize::new(bounds.width, bounds.height),
        );

        match content {
            InlineContent::Image(image) => {
                if let ImageSource::Node(image_node) = &image.source {
                    // Live overlay→DOM resolution: the IFC snapshotted only the
                    // NODE (see fc.rs), so a runtime image swap is visible on
                    // every DL build. Attribute the item to the image node —
                    // not the IFC root — so the chokepoint's in-place patch
                    // (`DisplayList::patch_node_image`) finds it.
                    if let Some(image_ref) =
                        self.ctx.resolved_content().image_for_paint(*image_node)
                    {
                        let prev_node = builder.current_node();
                        builder.set_current_node(Some(*image_node));
                        builder.push_image(object_bounds, image_ref, BorderRadius::default());
                        builder.set_current_node(prev_node);
                    }
                } else if let Some(image_ref) = get_image_ref_for_image_source(
                    &image.source,
                    self.ctx.image_cache,
                    object_bounds.size,
                ) {
                    builder.push_image(object_bounds, image_ref, BorderRadius::default());
                }
            }
            InlineContent::Shape(shape) => {
                self.paint_inline_shape(builder, object_bounds, shape, bounds);
            }
            _ => {}
        }
    }

    // +spec:inline-block:a60a89 - inline-block painted atomically as pseudo-stacking-context per E.2
    /// Paints an inline shape (inline-block background and border)
    fn paint_inline_shape(
        &self,
        builder: &mut DisplayListBuilder,
        object_bounds: LogicalRect,
        shape: &InlineShape,
        bounds: &crate::text3::cache::Rect,
    ) {
        // Render inline-block backgrounds and borders using their CSS styling
        // The text3 engine positions these correctly in the inline flow
        let Some(node_id) = shape.source_node_id else {
            return;
        };

        // If this inline-block establishes a stacking context, its background was
        // already painted by paint_node_background_and_border (called from
        // generate_for_stacking_context). Painting again here would cause
        // double-rendering. Skip it.
        if let Some(indices) = self.positioned_tree.tree.dom_to_layout.get(&node_id) {
            if let Some(&idx) = indices.first() {
                if self.establishes_stacking_context(idx.index()) {
                    return;
                }
            }
        }

        let styled_node_state =
            &self.ctx.styled_dom.styled_nodes.as_container()[node_id].styled_node_state;

        // Get all background layers (colors, gradients, images)
        let background_contents =
            get_background_contents(self.ctx.styled_dom, node_id, styled_node_state);

        // Get border information
        let border_info = get_border_info(self.ctx.styled_dom, node_id, styled_node_state);

        // FIX: object_bounds is the margin-box position from text3.
        // We need to convert to border-box for painting backgrounds/borders.
        let margins = self
            .positioned_tree
            .tree
            .dom_to_layout
            .get(&node_id)
            .map_or_else(crate::solver3::geometry::EdgeSizes::default, |indices| {
                indices
                    .first()
                    .map_or_else(crate::solver3::geometry::EdgeSizes::default, |&idx| {
                        self.positioned_tree.tree.nodes[idx.index()]
                            .box_props
                            .unpack()
                            .margin
                    })
            });

        // Convert margin-box bounds to border-box bounds
        let border_box_bounds = LogicalRect {
            origin: LogicalPosition {
                x: object_bounds.origin.x + margins.left,
                y: object_bounds.origin.y + margins.top,
            },
            size: LogicalSize {
                width: (object_bounds.size.width - margins.left - margins.right).max(0.0),
                height: (object_bounds.size.height - margins.top - margins.bottom).max(0.0),
            },
        };

        let element_size = PhysicalSizeImport {
            width: border_box_bounds.size.width,
            height: border_box_bounds.size.height,
        };

        // Get border radius for background clipping
        let simple_border_radius = get_border_radius(
            self.ctx.styled_dom,
            node_id,
            styled_node_state,
            element_size,
            self.ctx.viewport_size,
        );

        // Get style border radius for border rendering
        let style_border_radius =
            get_style_border_radius(self.ctx.styled_dom, node_id, styled_node_state);

        // Use unified background/border painting with border-box bounds
        builder.push_backgrounds_and_border(
            border_box_bounds,
            &background_contents,
            &border_info,
            simple_border_radius,
            style_border_radius,
            self.ctx.image_cache,
        );

        // Push hit-test area for this inline-block element
        // This is critical for buttons and other inline-block elements to receive
        // mouse events and display the correct cursor (e.g., cursor: pointer)
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, Some(node_id)) {
            builder.push_hit_test_area(border_box_bounds, tag_id);
        }
    }

    // +spec:overflow:d1d5f6 - CSS 2.2 §9.9.1 stacking context creation and 7-layer paint order
    /// Determines if a node establishes a new stacking context based on CSS rules.
    // +spec:overflow:47b791 - z-index applies to positioned boxes; z-index:auto does not establish stacking context
    // +spec:positioning:8c6efd - Stacking contexts: positioned elements with z-index != auto establish new stacking context
    // +spec:positioning:b84cfa - z-index stacking context creation: integer z-index on positioned elements creates SC; auto on fixed/root creates SC
    // +spec:positioning:d06368 - relative/absolute with z-index:auto do not form stacking context but are painted as if they did
    fn establishes_stacking_context(&self, node_index: usize) -> bool {
        let Some(node) = self.positioned_tree.tree.get(LayoutNodeId::new(node_index)) else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };

        let position = get_position_type(self.ctx.styled_dom, Some(dom_id));
        let z_auto = crate::solver3::getters::is_z_index_auto(self.ctx.styled_dom, Some(dom_id));

        // +spec:position-sticky:66ba22 - fixed and sticky positioned boxes form a stacking context
        if position == LayoutPosition::Fixed || position == LayoutPosition::Sticky {
            return true;
        }

        // +spec:positioning:d06368 - relative/absolute with z-index:auto do not form stacking context
        // z-index:auto on position:absolute does NOT establish stacking context
        if position == LayoutPosition::Absolute {
            return !z_auto;
        }

        // position:relative with explicit z-index integer establishes stacking context
        if position == LayoutPosition::Relative && !z_auto {
            return true;
        }

        if let Some(styled_node) = self.ctx.styled_dom.styled_nodes.as_container().get(dom_id) {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Opacity < 1 (GPU: fast path via compact cache)
            if crate::solver3::getters::get_opacity(self.ctx.styled_dom, dom_id, node_state) < 1.0 {
                return true;
            }

            // Transform != none (GPU: has_transform bit check, then slow walk only if set)
            if let Some(t) =
                crate::solver3::getters::get_transform(self.ctx.styled_dom, dom_id, node_state)
            {
                if !t.is_empty() {
                    return true;
                }
            }
        }

        false
    }
}

/// Helper struct to pass layout results to the display list generator.
///
/// Combines the layout tree with pre-calculated absolute positions for each node.
/// The positions are stored separately because they are computed in a final
/// positioning pass after layout is complete.
#[derive(Debug)]
pub struct PositionedTree<'a> {
    /// The layout tree containing all nodes with their computed sizes
    pub tree: &'a LayoutTree,
    /// Map from node index to its absolute position in the document
    pub calculated_positions: &'a super::PositionVec,
}

/// Expands `clip_rect` outward by the `overflow-clip-margin` value on axes that use `overflow: clip`.
///
/// Per CSS Overflow 3 §3.2, `overflow-clip-margin` only applies to `overflow: clip` —
/// it has no effect on `overflow: hidden`, `scroll`, or `auto`.
#[allow(clippy::trivially_copy_pass_by_ref)] // <=8B Copy param kept by-ref intentionally (hot pixel/coord path or to avoid churning call sites for a perf-neutral change)
fn apply_overflow_clip_margin(
    clip_rect: &mut LogicalRect,
    overflow_x: &super::getters::MultiValue<LayoutOverflow>,
    overflow_y: &super::getters::MultiValue<LayoutOverflow>,
    styled_dom: &StyledDom,
    dom_id: NodeId,
    styled_node_state: &azul_core::styled_dom::StyledNodeState,
) {
    if !overflow_x.is_clip() && !overflow_y.is_clip() {
        return;
    }
    let clip_margin = get_overflow_clip_margin_property(styled_dom, dom_id, styled_node_state);
    let Some(margin_val) = clip_margin.exact() else {
        return;
    };
    let m = margin_val.inner.to_pixels_internal(0.0, 0.0, 0.0).max(0.0);
    if m <= 0.0 {
        return;
    }
    if overflow_x.is_clip() {
        clip_rect.origin.x -= m;
        clip_rect.size.width += m * 2.0;
    }
    if overflow_y.is_clip() {
        clip_rect.origin.y -= m;
        clip_rect.size.height += m * 2.0;
    }
}

fn get_scroll_id(id: Option<NodeId>) -> LocalScrollId {
    id.map_or(0, |i| i.index() as u64)
}

/// Calculates the actual content size of a node, including all children and text.
/// This is used to determine if scrollbars should appear for overflow: auto.
// +spec:overflow:c2ed94 - replaced element overflow is ink overflow (not scrollable);
// replaced elements (images) don't contribute scrollable overflow here
fn get_scroll_content_size(node: &LayoutNodeHot, warm: Option<&LayoutNodeWarm>) -> LogicalSize {
    // The stored `overflow_content_size` participates but does NOT
    // early-return: it is a snapshot from some earlier pass, and for a node
    // whose IFC laid out after it was written (the absolutely positioned
    // placeholder <p> on the first device frame) it can be a few pixels
    // tall. The per-axis overflow-visible clip rescue takes THIS value as
    // the live extent, so the early return collapsed the placeholder's clip
    // to a strip of glyph tops until the next full restyle rebuilt it
    // (2026-08-31 first-draw report). Union it with the node size and the
    // live inline extent below - a cached scroll extent is not authoritative
    // for a node whose inline content is fresher.
    let stored = warm.and_then(|w| w.overflow_content_size);

    // Start with the node's own size
    let mut content_size = node.used_size.unwrap_or_default();
    if let Some(st) = stored {
        content_size.width = content_size.width.max(st.width);
        content_size.height = content_size.height.max(st.height);
    }

    // If this node has text layout, calculate the bounds of all text items
    if let Some(cached_layout) = warm.and_then(|w| w.inline_layout_result.as_ref()) {
        let text_layout = &cached_layout.layout;
        // §3.2 (d3): the extent from the DENSE arrays when retained and the
        // layout is pure clusters — width from base advances (== the sparse
        // bounds().width since d2), height from the d3-filled line heights.
        // Mixed layouts (objects/tabs) keep the sparse walk.
        let dense_extent = cached_layout.dense.as_ref().and_then(|d| {
            // (d6h) Sentinel-aware: post-retirement the stored items are
            // EMPTY and dense IS the content — the old equality guard
            // read n != 0 and rejected the dense arm, zeroing the
            // extent (no scrollbar, dead caret-reveal; caught by
            // caret_scroll_glide under the d7 default flip). Mixed
            // layouts (flag-off with objects/tabs) still fall back.
            if d.clusters.is_empty() {
                return None;
            }
            if !text_layout.items.is_empty() && d.clusters.len() != text_layout.items.len() {
                return None;
            }
            let max_x = d
                .clusters
                .iter()
                .map(|c| c.x + c.advance)
                .fold(0.0f32, f32::max);
            let max_y = d
                .lines
                .iter()
                .map(|l| l.top_y + l.height)
                .fold(0.0f32, f32::max);
            Some((max_x, max_y))
        });
        let (max_x, max_y) = if let Some((dx, dy)) = dense_extent {
            if std::env::var("AZ_DENSE_TEXT").as_deref() == Ok("verify") {
                let mut sx: f32 = 0.0;
                let mut sy: f32 = 0.0;
                for positioned_item in &text_layout.items {
                    let item_bounds = positioned_item.item.bounds();
                    sx = sx.max(positioned_item.position.x + item_bounds.width);
                    sy = sy.max(positioned_item.position.y + item_bounds.height);
                }
                assert!(
                    (sx - dx).abs() < 0.01 && (sy - dy).abs() < 0.01,
                    "d3 verify: scroll extent diverged (sparse {sx}x{sy} vs dense {dx}x{dy})"
                );
            }
            (dx, dy)
        } else {
            let mut max_x: f32 = 0.0;
            let mut max_y: f32 = 0.0;
            for positioned_item in &text_layout.items {
                let item_bounds = positioned_item.item.bounds();
                let item_right = positioned_item.position.x + item_bounds.width;
                let item_bottom = positioned_item.position.y + item_bounds.height;
                max_x = max_x.max(item_right);
                max_y = max_y.max(item_bottom);
            }
            (max_x, max_y)
        };

        // Use the maximum extent as content size if it's larger
        content_size.width = content_size.width.max(max_x);
        content_size.height = content_size.height.max(max_y);
    }

    content_size
}

fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<DisplayListTagId> {
    let node_id = id?;
    let tag_mapping = dom
        .tag_ids_to_node_ids
        .as_ref()
        .iter()
        .find(|m| m.node_id.into_crate_internal() == Some(node_id))?;
    Some((tag_mapping.tag_id.inner, TAG_TYPE_DOM_NODE))
}

/// Resolve an [`ImageSource`] (as carried by an inline `InlineContent::Image`)
/// to a concrete [`ImageRef`] ready for `push_image`.
///
/// `target_size` is the object's logical box size, used only to size the raster
/// when rasterizing an SVG source.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
fn get_image_ref_for_image_source(
    source: &ImageSource,
    image_cache: &azul_core::resources::ImageCache,
    target_size: LogicalSize,
) -> Option<ImageRef> {
    match source {
        ImageSource::Ref(image_ref) => Some(image_ref.clone()),
        // Node-scoped content resolves through the overlay→DOM resolver, which
        // needs the LayoutContext — `paint_inline_object` handles that arm
        // BEFORE calling here. A caller without a resolver cannot paint it.
        ImageSource::Node(_) => None,
        ImageSource::Url(url) => {
            // CSS url() image — resolved exactly like `background-image`: look it
            // up in the ImageCache by its CSS id (see push_backgrounds_and_border).
            let css_id: azul_css::AzString = url.clone().into();
            image_cache.get_css_image_id(&css_id).cloned()
        }
        ImageSource::Data(bytes) => {
            // Encoded image bytes (PNG/JPEG/…): decode to a RawImage, then build
            // an ImageRef. The `decode` module is gated on `std` and the decoder
            // itself needs the `image` crate (`image_decoding`).
            #[cfg(all(feature = "std", feature = "image_decoding"))]
            {
                use crate::image::decode::{
                    decode_raw_image_from_any_bytes, ResultRawImageDecodeImageError,
                };
                if let ResultRawImageDecodeImageError::Ok(raw) =
                    decode_raw_image_from_any_bytes(bytes)
                {
                    return ImageRef::new_rawimage(raw);
                }
                None
            }
            #[cfg(not(all(feature = "std", feature = "image_decoding")))]
            {
                // The document handed us encoded image bytes and this build
                // cannot decode them — the image just vanishes. Say so once.
                static ANNOUNCE: std::sync::Once = std::sync::Once::new();
                ANNOUNCE.call_once(|| {
                    eprintln!(
                        "[azul][image] encoded image data present, but this build \
                         lacks the `image_decoding` (+`std`) feature — images from \
                         encoded bytes will NOT appear"
                    );
                });
                let _ = bytes;
                None
            }
        }
        ImageSource::Svg(svg) => {
            // Rasterize the SVG source to the object's box size using the CPU SVG
            // renderer. Needs the `cpurender` feature.
            #[cfg(feature = "cpurender")]
            {
                let w = (target_size.width.round() as u32).max(1);
                let h = (target_size.height.round() as u32).max(1);
                crate::cpurender::render_svg_to_imageref(svg.as_bytes(), w, h).ok()
            }
            #[cfg(not(feature = "cpurender"))]
            {
                // Inline SVG images rasterize through the CPU renderer; without
                // it they silently vanish from the frame. Say so once.
                static ANNOUNCE: std::sync::Once = std::sync::Once::new();
                ANNOUNCE.call_once(|| {
                    eprintln!(
                        "[azul][svg] an SVG image source is present, but this build \
                         has no `cpurender` feature — SVG images will NOT appear"
                    );
                });
                let _ = (svg, target_size);
                None
            }
        }
        ImageSource::Placeholder(_) => {
            // Layout-only placeholder: reserves space, paints nothing.
            None
        }
    }
}

/// Get the bounds of a display list item in window-logical coordinates.
fn get_display_item_bounds(item: &DisplayListItem) -> Option<WindowLogicalRect> {
    item.bounds().map(WindowLogicalRect::from)
}

/// Clip a display list item to page bounds and offset to page-relative coordinates.
/// Returns None if the item is completely outside the page bounds.
#[allow(clippy::match_same_arms)]
// enum/value mapping/dispatch table: one arm per input variant (or cross-type bindings that can't merge)
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn clip_and_offset_display_item(
    item: &DisplayListItem,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    match item {
        // Pagination moves items between pages by translating their bounds.
        // A stroked path carries its geometry in USER SPACE and is mapped
        // into `bounds` by the renderer, so translating the box translates
        // the drawing with it - nothing else has to move.
        DisplayListItem::StrokedPath {
            bounds,
            path,
            view_box,
            color,
            width,
            mask,
        } => {
            let b = bounds.into_inner();
            if b.origin.y + b.size.height < page_top || b.origin.y > page_bottom {
                return None;
            }
            Some(DisplayListItem::StrokedPath {
                bounds: offset_rect_y(b, -page_top).into(),
                path: path.clone(),
                view_box: *view_box,
                color: *color,
                width: *width,
                mask: mask.clone(),
            })
        }
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => clip_rect_item(
            bounds.into_inner(),
            *color,
            *border_radius,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => clip_border_item(
            bounds.into_inner(),
            *widths,
            *colors,
            *styles,
            *border_radius,
            page_top,
            page_bottom,
        ),

        DisplayListItem::SelectionRect {
            bounds,
            border_radius,
            color,
        } => clip_selection_rect_item(
            bounds.into_inner(),
            *border_radius,
            *color,
            page_top,
            page_bottom,
        ),

        DisplayListItem::CursorRect { bounds, color } => {
            clip_cursor_rect_item(bounds.into_inner(), *color, page_top, page_bottom)
        }

        DisplayListItem::Image {
            bounds,
            image,
            border_radius,
        } => clip_image_item(
            bounds.into_inner(),
            image.clone(),
            *border_radius,
            page_top,
            page_bottom,
        ),

        DisplayListItem::TextLayout {
            layout,
            bounds,
            font_hash,
            font_size_px,
            color,
        } => clip_text_layout_item(
            layout,
            bounds.into_inner(),
            *font_hash,
            *font_size_px,
            *color,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px,
            color,
            clip_rect,
            ..
        } => clip_text_item(
            glyphs,
            *font_hash,
            *font_size_px,
            *color,
            clip_rect.into_inner(),
            page_top,
            page_bottom,
        ),

        DisplayListItem::Underline {
            bounds,
            color,
            thickness,
        } => clip_text_decoration_item(
            bounds.into_inner(),
            *color,
            *thickness,
            TextDecorationType::Underline,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Strikethrough {
            bounds,
            color,
            thickness,
        } => clip_text_decoration_item(
            bounds.into_inner(),
            *color,
            *thickness,
            TextDecorationType::Strikethrough,
            page_top,
            page_bottom,
        ),

        DisplayListItem::Overline {
            bounds,
            color,
            thickness,
        } => clip_text_decoration_item(
            bounds.into_inner(),
            *color,
            *thickness,
            TextDecorationType::Overline,
            page_top,
            page_bottom,
        ),

        DisplayListItem::ScrollBar {
            bounds,
            color,
            orientation,
            opacity_key,
            hit_id,
        } => clip_scrollbar_item(
            bounds.into_inner(),
            *color,
            *orientation,
            *opacity_key,
            *hit_id,
            page_top,
            page_bottom,
        ),

        DisplayListItem::HitTestArea { bounds, tag } => {
            clip_hit_test_area_item(bounds.into_inner(), *tag, page_top, page_bottom)
        }

        DisplayListItem::VirtualView {
            child_dom_id,
            bounds,
            clip_rect,
            content_offset,
        } => clip_virtual_view_item(
            *child_dom_id,
            bounds.into_inner(),
            clip_rect.into_inner(),
            *content_offset,
            page_top,
            page_bottom,
        ),

        // ScrollBarStyled - clip based on overall bounds
        DisplayListItem::ScrollBarStyled { info } => {
            let bounds = info.bounds;
            if bounds.0.origin.y + bounds.0.size.height < page_top
                || bounds.0.origin.y > page_bottom
            {
                None
            } else {
                // Clone and offset all the internal bounds
                let mut clipped_info = (**info).clone();
                let y_offset = -page_top;
                clipped_info.bounds =
                    offset_rect_y(clipped_info.bounds.into_inner(), y_offset).into();
                clipped_info.track_bounds =
                    offset_rect_y(clipped_info.track_bounds.into_inner(), y_offset).into();
                clipped_info.thumb_bounds =
                    offset_rect_y(clipped_info.thumb_bounds.into_inner(), y_offset).into();
                if let Some(b) = clipped_info.button_decrement_bounds {
                    clipped_info.button_decrement_bounds =
                        Some(offset_rect_y(b.into_inner(), y_offset).into());
                }
                if let Some(b) = clipped_info.button_increment_bounds {
                    clipped_info.button_increment_bounds =
                        Some(offset_rect_y(b.into_inner(), y_offset).into());
                }
                Some(DisplayListItem::ScrollBarStyled {
                    info: Box::new(clipped_info),
                })
            }
        }

        // State management items - skip for now (would need proper per-page tracking)
        DisplayListItem::PushClip { .. }
        | DisplayListItem::PopClip
        | DisplayListItem::PushScrollFrame { .. }
        | DisplayListItem::PopScrollFrame
        | DisplayListItem::PushStackingContext { .. }
        | DisplayListItem::PopStackingContext
        | DisplayListItem::VirtualViewPlaceholder { .. } => None,

        // Gradient items - simple bounds check
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.0.origin.y + bounds.0.size.height < page_top
                || bounds.0.origin.y > page_bottom
            {
                None
            } else {
                Some(DisplayListItem::LinearGradient {
                    bounds: offset_rect_y(bounds.into_inner(), -page_top).into(),
                    gradient: gradient.clone(),
                    border_radius: *border_radius,
                })
            }
        }
        DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.0.origin.y + bounds.0.size.height < page_top
                || bounds.0.origin.y > page_bottom
            {
                None
            } else {
                Some(DisplayListItem::RadialGradient {
                    bounds: offset_rect_y(bounds.into_inner(), -page_top).into(),
                    gradient: gradient.clone(),
                    border_radius: *border_radius,
                })
            }
        }
        DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.0.origin.y + bounds.0.size.height < page_top
                || bounds.0.origin.y > page_bottom
            {
                None
            } else {
                Some(DisplayListItem::ConicGradient {
                    bounds: offset_rect_y(bounds.into_inner(), -page_top).into(),
                    gradient: gradient.clone(),
                    border_radius: *border_radius,
                })
            }
        }

        // BoxShadow - simple bounds check
        DisplayListItem::BoxShadow {
            bounds,
            shadow,
            border_radius,
        } => {
            if bounds.0.origin.y + bounds.0.size.height < page_top
                || bounds.0.origin.y > page_bottom
            {
                None
            } else {
                Some(DisplayListItem::BoxShadow {
                    bounds: offset_rect_y(bounds.into_inner(), -page_top).into(),
                    shadow: *shadow,
                    border_radius: *border_radius,
                })
            }
        }

        // Filter effects - skip for now (would need proper per-page tracking)
        DisplayListItem::PushFilter { .. }
        | DisplayListItem::PopFilter
        | DisplayListItem::PushBackdropFilter { .. }
        | DisplayListItem::PopBackdropFilter
        | DisplayListItem::PushOpacity { .. }
        | DisplayListItem::PopOpacity
        | DisplayListItem::PushReferenceFrame { .. }
        | DisplayListItem::PopReferenceFrame
        | DisplayListItem::PushTextShadow { .. }
        | DisplayListItem::PopTextShadow
        | DisplayListItem::PushImageMaskClip { .. }
        | DisplayListItem::PopImageMaskClip => None,
    }
}

// Helper functions for clip_and_offset_display_item

/// Internal enum for text decoration type dispatch
#[derive(Debug, Clone, Copy)]
enum TextDecorationType {
    Underline,
    Strikethrough,
    Overline,
}

/// Clips a filled rectangle to page bounds.
fn clip_rect_item(
    bounds: LogicalRect,
    color: ColorU,
    border_radius: BorderRadius,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::Rect {
        bounds: clipped.into(),
        color,
        border_radius,
    })
}

/// Clips a border to page bounds, hiding top/bottom borders when clipped.
fn clip_border_item(
    bounds: LogicalRect,
    widths: StyleBorderWidths,
    colors: StyleBorderColors,
    styles: StyleBorderStyles,
    border_radius: StyleBorderRadius,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    let original_bounds = bounds;
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| {
        let new_widths = adjust_border_widths_for_clipping(
            widths,
            original_bounds,
            clipped,
            page_top,
            page_bottom,
        );
        DisplayListItem::Border {
            bounds: clipped.into(),
            widths: new_widths,
            colors,
            styles,
            border_radius,
        }
    })
}

/// Adjusts border widths when a border is clipped at page boundaries.
/// Hides top border if clipped at top, bottom border if clipped at bottom.
fn adjust_border_widths_for_clipping(
    mut widths: StyleBorderWidths,
    original_bounds: LogicalRect,
    clipped: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> StyleBorderWidths {
    // Hide top border if we clipped the top
    if clipped.origin.y > 0.0 && original_bounds.origin.y < page_top {
        widths.top = None;
    }

    // Hide bottom border if we clipped the bottom
    let original_bottom = original_bounds.origin.y + original_bounds.size.height;
    let clipped_bottom = clipped.origin.y + clipped.size.height;
    if original_bottom > page_bottom && clipped_bottom >= page_bottom - page_top - 1.0 {
        widths.bottom = None;
    }

    widths
}

/// Clips a selection rectangle to page bounds.
fn clip_selection_rect_item(
    bounds: LogicalRect,
    border_radius: BorderRadius,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::SelectionRect {
        bounds: clipped.into(),
        border_radius,
        color,
    })
}

/// Clips a cursor rectangle to page bounds.
fn clip_cursor_rect_item(
    bounds: LogicalRect,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::CursorRect {
        bounds: clipped.into(),
        color,
    })
}

/// Clips an image to page bounds if it overlaps the page.
fn clip_image_item(
    bounds: LogicalRect,
    image: ImageRef,
    border_radius: BorderRadius,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&bounds, page_top, page_bottom) {
        return None;
    }
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::Image {
        bounds: clipped.into(),
        image,
        border_radius,
    })
}

/// Clips a text layout block to page bounds, filtering individual text items.
fn clip_text_layout_item(
    layout: &Arc<dyn std::any::Any + Send + Sync>,
    bounds: LogicalRect,
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&bounds, page_top, page_bottom) {
        return None;
    }

    // (d5/d6h) TextPayload carries both forms. Post-retirement the
    // sparse half is the EMPTY sentinel — the clipper materializes the
    // items from the dense arrays on demand (transient, export-time
    // only; exactness pinned by the d6h expansion gate).
    #[cfg(feature = "text_layout")]
    if let Some(p) = layout.downcast_ref::<crate::solver3::layout_tree::TextPayload>() {
        if p.sparse.items.is_empty() && !p.dense.clusters.is_empty() {
            let expanded = UnifiedLayout {
                items: p.dense.to_unified_items(),
                overflow: p.sparse.overflow.clone(),
            };
            return clip_unified_layout(
                &expanded,
                bounds,
                font_hash,
                font_size_px,
                color,
                page_top,
                page_bottom,
            );
        }
        return clip_unified_layout(
            &p.sparse,
            bounds,
            font_hash,
            font_size_px,
            color,
            page_top,
            page_bottom,
        );
    }

    // Try to downcast and filter UnifiedLayout items
    #[cfg(feature = "text_layout")]
    if let Some(unified_layout) = layout.downcast_ref::<UnifiedLayout>() {
        return clip_unified_layout(
            unified_layout,
            bounds,
            font_hash,
            font_size_px,
            color,
            page_top,
            page_bottom,
        );
    }

    // Fallback: simple bounds offset (legacy behavior)
    Some(DisplayListItem::TextLayout {
        layout: layout.clone(),
        bounds: offset_rect_y(bounds, -page_top).into(),
        font_hash,
        font_size_px,
        color,
    })
}

/// Clips a `UnifiedLayout` by filtering items to those on the current page.
#[cfg(feature = "text_layout")]
fn clip_unified_layout(
    unified_layout: &UnifiedLayout,
    bounds: LogicalRect,
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    let layout_origin_y = bounds.origin.y;
    let layout_origin_x = bounds.origin.x;

    // Filter items whose center falls within this page
    let filtered_items: Vec<_> = unified_layout
        .items
        .iter()
        .filter(|item| item_center_on_page(item, layout_origin_y, page_top, page_bottom))
        .cloned()
        .collect();

    if filtered_items.is_empty() {
        return None;
    }

    // Calculate new origin for page-relative positioning
    let new_origin_y = (layout_origin_y - page_top).max(0.0);

    // Transform items to page-relative coordinates and calculate bounds
    let (offset_items, min_y, max_y, max_width) =
        transform_items_to_page_coords(filtered_items, layout_origin_y, page_top, new_origin_y);

    let new_layout = UnifiedLayout {
        items: offset_items,
        overflow: unified_layout.overflow.clone(),
    };

    let new_bounds = LogicalRect {
        origin: LogicalPosition {
            x: layout_origin_x,
            y: new_origin_y,
        },
        size: LogicalSize {
            width: max_width.max(bounds.size.width),
            height: (max_y - min_y.min(0.0)).max(0.0),
        },
    };

    Some(DisplayListItem::TextLayout {
        layout: Arc::new(new_layout),
        bounds: new_bounds.into(),
        font_hash,
        font_size_px,
        color,
    })
}

/// Checks if an item's center point falls within the page bounds.
#[cfg(feature = "text_layout")]
fn item_center_on_page(
    item: &PositionedItem,
    layout_origin_y: f32,
    page_top: f32,
    page_bottom: f32,
) -> bool {
    let item_y_absolute = layout_origin_y + item.position.y;
    let item_height = item.item.bounds().height;
    let item_center_y = item_y_absolute + (item_height / 2.0);
    item_center_y >= page_top && item_center_y < page_bottom
}

/// Transforms filtered items to page-relative coordinates.
/// Returns (items, `min_y`, `max_y`, `max_width`).
#[cfg(feature = "text_layout")]
fn transform_items_to_page_coords(
    items: Vec<PositionedItem>,
    layout_origin_y: f32,
    page_top: f32,
    new_origin_y: f32,
) -> (Vec<PositionedItem>, f32, f32, f32) {
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;
    let mut max_width = 0.0f32;

    let offset_items: Vec<_> = items
        .into_iter()
        .map(|mut item| {
            let abs_y = layout_origin_y + item.position.y;
            let page_y = abs_y - page_top;
            let new_item_y = page_y - new_origin_y;

            let item_bounds = item.item.bounds();
            min_y = min_y.min(new_item_y);
            max_y = max_y.max(new_item_y + item_bounds.height);
            max_width = max_width.max(item.position.x + item_bounds.width);

            item.position.y = new_item_y;
            item
        })
        .collect();

    (offset_items, min_y, max_y, max_width)
}

/// Clips a text glyph run to page bounds, filtering individual glyphs.
fn clip_text_item(
    glyphs: &[GlyphInstance],
    font_hash: FontHash,
    font_size_px: f32,
    color: ColorU,
    clip_rect: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&clip_rect, page_top, page_bottom) {
        return None;
    }

    // Filter glyphs using center-point decision (baseline position)
    let page_glyphs: Vec<_> = glyphs
        .iter()
        .filter(|g| g.point.y >= page_top && g.point.y < page_bottom)
        .map(|g| GlyphInstance {
            index: g.index,
            point: LogicalPosition {
                x: g.point.x,
                y: g.point.y - page_top,
            },
            size: g.size,
        })
        .collect();

    if page_glyphs.is_empty() {
        return None;
    }

    Some(DisplayListItem::Text {
        glyphs: page_glyphs,
        font_hash,
        font_size_px,
        color,
        clip_rect: offset_rect_y(clip_rect, -page_top).into(),
        source_node_index: None,
    })
}

/// Clips a text decoration (underline, strikethrough, or overline) to page bounds.
fn clip_text_decoration_item(
    bounds: LogicalRect,
    color: ColorU,
    thickness: f32,
    decoration_type: TextDecorationType,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| match decoration_type {
        TextDecorationType::Underline => DisplayListItem::Underline {
            bounds: clipped.into(),
            color,
            thickness,
        },
        TextDecorationType::Strikethrough => DisplayListItem::Strikethrough {
            bounds: clipped.into(),
            color,
            thickness,
        },
        TextDecorationType::Overline => DisplayListItem::Overline {
            bounds: clipped.into(),
            color,
            thickness,
        },
    })
}

/// Clips a scrollbar to page bounds.
fn clip_scrollbar_item(
    bounds: LogicalRect,
    color: ColorU,
    orientation: ScrollbarOrientation,
    opacity_key: Option<OpacityKey>,
    hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::ScrollBar {
        bounds: clipped.into(),
        color,
        orientation,
        opacity_key,
        hit_id,
    })
}

/// Clips a hit test area to page bounds.
fn clip_hit_test_area_item(
    bounds: LogicalRect,
    tag: DisplayListTagId,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::HitTestArea {
        bounds: clipped.into(),
        tag,
    })
}

/// Clips a virtualized view to page bounds.
fn clip_virtual_view_item(
    child_dom_id: DomId,
    bounds: LogicalRect,
    clip_rect: LogicalRect,
    content_offset: LogicalPosition,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::VirtualView {
        child_dom_id,
        bounds: clipped.into(),
        clip_rect: offset_rect_y(clip_rect, -page_top).into(),
        // A page break moves the BOX, not the window the content sits in.
        content_offset,
    })
}

/// Clip a rectangle to page bounds and offset to page-relative coordinates.
/// Returns None if the rectangle is completely outside the page.
fn clip_rect_bounds(bounds: LogicalRect, page_top: f32, page_bottom: f32) -> Option<LogicalRect> {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;

    // Check if completely outside page
    if item_bottom <= page_top || item_top >= page_bottom {
        return None;
    }

    // Calculate clipped bounds
    let clipped_top = item_top.max(page_top);
    let clipped_bottom = item_bottom.min(page_bottom);
    let clipped_height = clipped_bottom - clipped_top;

    // Offset to page-relative coordinates
    let page_relative_y = clipped_top - page_top;

    Some(LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: page_relative_y,
        },
        size: LogicalSize {
            width: bounds.size.width,
            height: clipped_height,
        },
    })
}

/// Check if a rectangle intersects the page bounds.
fn rect_intersects(bounds: &LogicalRect, page_top: f32, page_bottom: f32) -> bool {
    let item_top = bounds.origin.y;
    let item_bottom = bounds.origin.y + bounds.size.height;
    item_bottom > page_top && item_top < page_bottom
}

/// Offset a rectangle's Y coordinate.
fn offset_rect_y(bounds: LogicalRect, offset_y: f32) -> LogicalRect {
    LogicalRect {
        origin: LogicalPosition {
            x: bounds.origin.x,
            y: bounds.origin.y + offset_y,
        },
        size: bounds.size,
    }
}

// Slicer based pagination: "Infinite Canvas with Clipping"
//
// This approach treats pages as "viewports" into a single infinite canvas:
//
// 1. Layout generates ONE display list on an infinite vertical strip
// 2. Each page is a clip rectangle that "views" a portion of that strip
// 3. Items that span page boundaries are clipped and appear on BOTH pages

use azul_css::props::layout::fragmentation::{BreakInside, PageBreak};

use crate::solver3::page_breaks::{self, BreakPolicy};
use crate::solver3::pagination::{
    HeaderFooterConfig, MarginBoxContent, PageInfo, TableHeaderInfo, TableHeaderTracker,
};

/// Configuration for the slicer-based pagination.
#[derive(Debug, Clone, Default)]
pub struct SlicerConfig {
    /// Height of each page's content area (excludes margins, headers, footers)
    pub page_content_height: f32,
    /// Height of "dead zone" between pages (for margins, headers, footers)
    /// This represents space that content should NOT overlap with
    pub page_gap: f32,
    /// Whether to clip items that span page boundaries (true) or push them to next page (false)
    pub allow_clipping: bool,
    /// Header and footer configuration
    pub header_footer: HeaderFooterConfig,
    /// Width of the page content area (for centering headers/footers)
    pub page_width: f32,
    /// Table headers that need repetition across pages
    pub table_headers: TableHeaderTracker,
    /// Break-awareness policy (all-off default = plain interval slicing).
    pub break_policy: BreakPolicy,
    /// office-suite-style per-page setup (default + overrides + parity). `None`
    /// = every page uses `header_footer` / `page_content_height` uniformly.
    pub page_sequence: Option<crate::solver3::pagination::PageSequence>,
}

impl SlicerConfig {
    /// Create a simple slicer config with no gaps between pages.
    #[must_use]
    pub fn simple(page_height: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: 0.0,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: DEFAULT_A4_WIDTH_PT, // Default A4 width in points
            table_headers: TableHeaderTracker::default(),
            break_policy: BreakPolicy::default(),
            page_sequence: None,
        }
    }

    /// Create a slicer config with margins/gaps between pages.
    #[must_use]
    pub fn with_gap(page_height: f32, gap: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: gap,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: DEFAULT_A4_WIDTH_PT,
            table_headers: TableHeaderTracker::default(),
            break_policy: BreakPolicy::default(),
            page_sequence: None,
        }
    }

    /// Set the break-awareness policy.
    #[must_use]
    pub const fn with_break_policy(mut self, policy: BreakPolicy) -> Self {
        self.break_policy = policy;
        self
    }

    /// Add header/footer configuration.
    #[must_use]
    pub fn with_header_footer(mut self, config: HeaderFooterConfig) -> Self {
        self.header_footer = config;
        self
    }

    /// Set the page width (for header/footer positioning).
    #[must_use]
    pub const fn with_page_width(mut self, width: f32) -> Self {
        self.page_width = width;
        self
    }

    /// Add table headers for repetition.
    #[must_use]
    pub fn with_table_headers(mut self, tracker: TableHeaderTracker) -> Self {
        self.table_headers = tracker;
        self
    }

    /// Register a single table header.
    pub fn register_table_header(&mut self, info: TableHeaderInfo) {
        self.table_headers.register_table_header(info);
    }

    /// The total height of a page "slot" including the gap.
    #[must_use]
    pub fn page_slot_height(&self) -> f32 {
        self.page_content_height + self.page_gap
    }

    /// Calculate which page a Y coordinate falls on.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    #[must_use]
    pub fn page_for_y(&self, y: f32) -> usize {
        if self.page_slot_height() <= 0.0 {
            return 0;
        }
        (y / self.page_slot_height()).floor() as usize
    }

    /// Get the Y range for a specific page (in infinite canvas coordinates).
    #[allow(clippy::cast_precision_loss)] // bounded graphics/coord/font/fixed-point/debug-marker cast
    #[must_use]
    pub fn page_bounds(&self, page_index: usize) -> (f32, f32) {
        let start = page_index as f32 * self.page_slot_height();
        let end = start + self.page_content_height;
        (start, end)
    }
}

/// Paginate with CSS break property support.
///
/// This function calculates page boundaries based on CSS break-before, break-after,
/// and break-inside properties, then clips content to those boundaries.
///
/// **Key insight**: Items are NEVER shifted. Instead, page boundaries are adjusted
/// to honor break properties.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
/// # Errors
///
/// Returns a `LayoutError` if paginating the display list fails.
pub fn paginate_display_list_with_slicer_and_breaks(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    renderer_resources: &RendererResources,
) -> Result<Vec<DisplayList>> {
    if config.page_content_height <= 0.0 || config.page_content_height >= f32::MAX {
        return Ok(vec![full_display_list]);
    }

    // Step 1: Calculate page break positions based on CSS properties
    // (forced break-before/after positions + regular interval breaks).
    let constraints = page_breaks::PageConstraints::from_slicer_config(config);
    let breaks =
        page_breaks::compute_page_breaks_from_display_list(&full_display_list, &constraints);

    paginate_display_list_with_breaks(full_display_list, config, &breaks, renderer_resources)
}

/// Paginate against a PRE-COMPUTED break analysis (see
/// [`page_breaks::compute_page_breaks_from_display_list`]).
///
/// The sibling of [`paginate_display_list_with_slicer_and_breaks`], which
/// computes the breaks itself and delegates here. Lets embedders analyze
/// pagination once and materialize pages separately.
/// # Errors
///
/// Returns a `LayoutError` if paginating the display list fails.
pub fn paginate_display_list_with_breaks(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    breaks: &[page_breaks::PageBreakPosition],
    renderer_resources: &RendererResources,
) -> Result<Vec<DisplayList>> {
    Ok(paginate_pages_impl(
        full_display_list,
        config,
        breaks,
        renderer_resources,
        None,
    ))
}

/// Materialize ONE page of a paginated document — the lazy-viewer entry.
///
/// A document editor computes breaks once (`compute_page_breaks*`), then
/// materializes only the visible pages. Headers/footers still show the
/// correct "page N of TOTAL" because the total comes from `breaks`, not from
/// how many pages were materialized.
///
/// Returns an empty display list when `page_index` is out of range.
/// # Errors
///
/// Returns a `LayoutError` if paginating the display list fails.
pub fn paginate_single_page(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    breaks: &[page_breaks::PageBreakPosition],
    renderer_resources: &RendererResources,
    page_index: usize,
) -> Result<DisplayList> {
    let mut pages = paginate_pages_impl(
        full_display_list,
        config,
        breaks,
        renderer_resources,
        Some(page_index),
    );
    Ok(pages.pop().unwrap_or_default())
}

#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
fn paginate_pages_impl(
    full_display_list: DisplayList,
    config: &SlicerConfig,
    breaks: &[page_breaks::PageBreakPosition],
    renderer_resources: &RendererResources,
    only_page: Option<usize>,
) -> Vec<DisplayList> {
    if config.page_content_height <= 0.0 || config.page_content_height >= f32::MAX {
        return vec![full_display_list];
    }

    let total_height = calculate_display_list_height(&full_display_list);
    let mut page_spans = page_breaks::page_spans(breaks, total_height);
    if page_spans.is_empty() {
        // An empty/zero-height document still produces one page tall enough
        // for the first page, so headers/footers render on it.
        let constraints = page_breaks::PageConstraints::from_slicer_config(config);
        page_spans.push((0.0, total_height.max(constraints.first_page_content_height)));
    }

    let num_pages = page_spans.len();

    // Create per-page display lists by slicing the master list
    let mut pages: Vec<DisplayList> =
        Vec::with_capacity(num_pages.min(only_page.map_or(usize::MAX, |_| 1)));

    for (page_idx, &(content_start_y, content_end_y)) in page_spans.iter().enumerate() {
        // Lazy single-page materialization: skip every other span. PageInfo
        // below still receives the TRUE total (num_pages).
        if only_page.is_some_and(|p| p != page_idx) {
            continue;
        }
        // Generate page info for header/footer content
        let page_info = PageInfo::new(page_idx + 1, num_pages);

        // Calculate per-page header/footer space. With a page SEQUENCE the
        // decoration comes from THIS page's setup (override/first/parity/
        // default) — the classic office suites model.
        let hf = config
            .page_sequence
            .as_ref()
            .map_or(&config.header_footer, |s| {
                &s.setup_for_page(page_idx).header_footer
            });
        let skip_this_page = hf.skip_first_page && page_info.is_first;
        let header_space = if hf.show_header && !skip_this_page {
            hf.header_height
        } else {
            0.0
        };
        let footer_space = if hf.show_footer && !skip_this_page {
            hf.footer_height
        } else {
            0.0
        };

        let _ = footer_space; // Currently unused but reserved for future

        let mut page_items = Vec::new();
        let mut page_node_mapping = Vec::new();

        // 1. Add header if enabled
        if hf.show_header && !skip_this_page {
            let header_text = hf.header_text(page_info);
            if !header_text.is_empty() {
                let header_items = generate_text_display_items(
                    &header_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: 0.0 },
                        size: LogicalSize {
                            width: config.page_width,
                            height: hf.header_height,
                        },
                    },
                    hf.font_size,
                    hf.text_color,
                    TextAlignment::Center,
                    renderer_resources,
                );
                for item in header_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }

        // 2. Inject repeated table headers (if any). Placement is X-AWARE:
        // two sibling tables can only both straddle a page top when they sit
        // SIDE BY SIDE (columns) — each repeated thead then stays in its own
        // x-band at the page top, and each table's continued rows shift down
        // by the summed heights of the theads whose x-band OVERLAPS theirs
        // (side-by-side: their own thead only; nested tables: outer + inner
        // stack). The old code stacked every thead vertically AND shifted
        // ALL content by max(height) — two straddling tables overlapped.
        struct TheadBand {
            x_start: f32,
            x_end: f32,
            /// Cumulative shift for content whose x overlaps this band
            /// (this table's thead + every x-overlapping one injected
            /// above it).
            shift: f32,
            /// Rebased end of the table's content on this page (below it
            /// the SUM of all thead heights applies — matching the break
            /// pass's per-page reservation).
            table_end_page_local: f32,
        }
        let straddlers = config.table_headers.straddling_tables_for_page(
            page_idx,
            content_start_y,
            content_end_y,
        );
        let mut bands: Vec<TheadBand> = Vec::new();
        let mut sum_all_theads = 0.0f32;
        for table in &straddlers {
            // The table's x-extent, from its thead's own geometry.
            let (mut x_start, mut x_end) = (f32::MAX, f32::MIN);
            for item in &table.thead_items {
                if let Some(b) = item.visual_bounds() {
                    x_start = x_start.min(b.origin.x);
                    x_end = x_end.max(b.origin.x + b.size.width);
                }
            }
            if x_start > x_end {
                (x_start, x_end) = (f32::MIN, f32::MAX); // no geometry: full width
            }
            // Stack below previously injected theads whose x-band overlaps
            // (nested tables); disjoint (side-by-side) theads sit at the top.
            let stacked_above: f32 = bands
                .iter()
                .filter(|b| b.x_start < x_end && x_start < b.x_end)
                .map(|b| b.shift)
                .fold(0.0, f32::max);
            let thead_y = header_space + stacked_above;
            for item in &table.thead_items {
                page_items.push(offset_display_item_y(item, thead_y));
                page_node_mapping.push(None);
            }
            bands.push(TheadBand {
                x_start,
                x_end,
                shift: stacked_above + table.thead_height,
                table_end_page_local: table.table_end_y - content_start_y,
            });
            sum_all_theads += table.thead_height;
        }

        // 3. Band-dependent shift for content items: inside a straddling
        // table's x-band and y-range → that band's shift; below EVERY
        // straddling table → the full sum (the break pass reserved it).
        let below_all_y = bands
            .iter()
            .map(|b| b.table_end_page_local)
            .fold(0.0f32, f32::max);
        let thead_shift_for = |b: LogicalRect| -> f32 {
            if bands.is_empty() {
                return 0.0;
            }
            if b.origin.y >= below_all_y {
                return sum_all_theads;
            }
            let x_center = b.size.width.mul_add(0.5, b.origin.x);
            bands
                .iter()
                .filter(|band| x_center >= band.x_start && x_center < band.x_end)
                .map(|band| band.shift)
                .fold(0.0, f32::max)
        };

        // 4. Slice and offset content items (skip fixed-position items, they are added in step 4b)
        //
        // E17: the clip / stacking / scroll-frame / text-shadow / image-mask
        // STRUCTURE is re-derived per page. Push/Pop markers are tracked in
        // a live stack and emitted LAZILY: a marker chain materializes only
        // when the first content item UNDER it survives this page's Y-band
        // (so pages without a subtree's content carry none of its markers,
        // and marker-free lists produce byte-identical output to before).
        // Balancing Pops are appended at the page end. Marker bounds get the
        // same rebase as the content they wrap (clip rects intersected with
        // the page band, so a partially-on-page clip keeps clipping).
        //
        // Previously EVERY marker was dropped at the page boundary: exports
        // showed unclipped overflow bleed and flattened z-order / opacity on
        // pages after the first affected one.
        struct OpenMarker {
            item_idx: usize,
            /// Already emitted into `page_items`?
            emitted: bool,
        }
        let mut marker_stack: Vec<OpenMarker> = Vec::new();

        // Rebase a MARKER item into page-local space: intersect its rect
        // with the page band (a clip partially on this page keeps clipping;
        // an off-page clip collapses to an empty rect — its content is
        // off-page anyway), rebase to page-local Y, apply the header offset.
        let rebase_band = |r: LogicalRect| -> LogicalRect {
            let top = r.origin.y.max(content_start_y);
            let bottom = (r.origin.y + r.size.height).min(content_end_y);
            LogicalRect {
                origin: LogicalPosition {
                    x: r.origin.x,
                    y: (top - content_start_y) + header_space,
                },
                size: LogicalSize {
                    width: r.size.width,
                    height: (bottom - top).max(0.0),
                },
            }
        };
        let rebase_marker = |item: &DisplayListItem| -> DisplayListItem {
            match item {
                DisplayListItem::PushClip {
                    bounds,
                    border_radius,
                } => DisplayListItem::PushClip {
                    bounds: rebase_band(bounds.0).into(),
                    border_radius: *border_radius,
                },
                DisplayListItem::PushStackingContext { z_index, bounds } => {
                    DisplayListItem::PushStackingContext {
                        z_index: *z_index,
                        bounds: rebase_band(bounds.0).into(),
                    }
                }
                DisplayListItem::PushScrollFrame {
                    clip_bounds,
                    content_size,
                    scroll_id,
                } => DisplayListItem::PushScrollFrame {
                    clip_bounds: rebase_band(clip_bounds.0).into(),
                    content_size: *content_size,
                    scroll_id: *scroll_id,
                },
                DisplayListItem::PushImageMaskClip {
                    bounds,
                    mask_image,
                    mask_rect,
                } => DisplayListItem::PushImageMaskClip {
                    bounds: rebase_band(bounds.0).into(),
                    mask_image: mask_image.clone(),
                    mask_rect: rebase_band(mask_rect.0).into(),
                },
                // PushTextShadow carries only the shadow definition.
                other => other.clone(),
            }
        };

        for (item_idx, item) in full_display_list.items.iter().enumerate() {
            // Maintain the live marker stack for EVERY item, retained or not.
            if item.is_push_marker() {
                marker_stack.push(OpenMarker {
                    item_idx,
                    emitted: false,
                });
                continue;
            }
            if item.is_pop_marker() {
                if let Some(open) = marker_stack.pop() {
                    if open.emitted {
                        page_items.push(item.clone());
                        page_node_mapping.push(None);
                    }
                }
                continue;
            }

            // Skip items that belong to fixed-position elements (they are replicated separately)
            let is_fixed = full_display_list
                .fixed_position_item_ranges
                .iter()
                .any(|&(start, end)| item_idx >= start && item_idx < end);
            if is_fixed {
                continue;
            }
            if let Some(clipped_item) =
                clip_and_offset_display_item(item, content_start_y, content_end_y)
            {
                // The item survives this page: materialize its (not yet
                // emitted) enclosing marker chain, outermost first.
                for open in &mut marker_stack {
                    if !open.emitted {
                        page_items.push(rebase_marker(&full_display_list.items[open.item_idx]));
                        page_node_mapping.push(None);
                        open.emitted = true;
                    }
                }
                // Page-local y of the clipped item decides which thead band
                // shifts it (items without geometry keep the base offset).
                let item_shift = clipped_item.visual_bounds().map_or(0.0, thead_shift_for);
                let content_y_offset = header_space + item_shift;
                let final_item = if content_y_offset > 0.0 {
                    offset_display_item_y(&clipped_item, content_y_offset)
                } else {
                    clipped_item
                };
                page_items.push(final_item);
                let node_mapping = full_display_list
                    .node_mapping
                    .get(item_idx)
                    .copied()
                    .flatten();
                page_node_mapping.push(node_mapping);
            }
        }

        // Balance: close every marker still open AND emitted at the page end
        // (innermost first).
        for open in marker_stack.iter().rev() {
            if open.emitted {
                if let Some(pop) = full_display_list.items[open.item_idx].matching_pop() {
                    page_items.push(pop);
                    page_node_mapping.push(None);
                }
            }
        }

        // 4b. Replicate fixed-position items on every page (CSS Positioned Layout §2.1)
        // Fixed-position boxes are fixed relative to the page box, so they appear
        // at the same position on every page without Y-offset adjustment.
        for &(start, end) in &full_display_list.fixed_position_item_ranges {
            for item_idx in start..end {
                if let Some(item) = full_display_list.items.get(item_idx) {
                    // Fixed-position boxes anchor to the PAGE box: offset by
                    // the page header only, never by content thead shifts.
                    let final_item = if header_space > 0.0 {
                        offset_display_item_y(item, header_space)
                    } else {
                        item.clone()
                    };
                    page_items.push(final_item);
                    let node_mapping = full_display_list
                        .node_mapping
                        .get(item_idx)
                        .copied()
                        .flatten();
                    page_node_mapping.push(node_mapping);
                }
            }
        }

        // 5. Add footer if enabled
        if hf.show_footer && !skip_this_page {
            let footer_text = hf.footer_text(page_info);
            if !footer_text.is_empty() {
                // THIS page's content height: under a PageSequence pages
                // differ (first/parity/override); the uniform config height
                // is only the no-sequence fallback.
                let page_h = config
                    .page_sequence
                    .as_ref()
                    .map_or(config.page_content_height, |s| {
                        s.setup_for_page(page_idx).content_height()
                    });
                let footer_y = page_h - hf.footer_height;
                let footer_items = generate_text_display_items(
                    &footer_text,
                    LogicalRect {
                        origin: LogicalPosition {
                            x: 0.0,
                            y: footer_y,
                        },
                        size: LogicalSize {
                            width: config.page_width,
                            height: hf.footer_height,
                        },
                    },
                    hf.font_size,
                    hf.text_color,
                    TextAlignment::Center,
                    renderer_resources,
                );
                for item in footer_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }

        pages.push(DisplayList {
            items: page_items,
            node_mapping: page_node_mapping,
            forced_page_breaks: Vec::new(),
            fixed_position_item_ranges: Vec::new(), // Already handled during pagination
            // Page slices are consumed by renderers only — patching operates
            // on the WINDOWED cached DL, never on page slices.
            layout_node_mapping: Vec::new(),
            uniform_text_bgs: Vec::new(),
        });
    }

    // Ensure at least one page
    if pages.is_empty() {
        pages.push(DisplayList::default());
    }

    pages
}

/// Calculate page break positions respecting CSS forced page breaks.
///
/// Returns a vector of (`start_y`, `end_y`) tuples representing each page's content bounds.
///
/// Shim over [`page_breaks::compute_page_breaks_from_display_list`] +
/// [`page_breaks::page_spans`], kept so the pre-extraction tests keep pinning
/// the observable spans through the same signature.
#[cfg(test)]
fn calculate_page_break_positions(
    display_list: &DisplayList,
    first_page_height: f32,
    normal_page_height: f32,
) -> Vec<(f32, f32)> {
    let total_height = calculate_display_list_height(display_list);

    if total_height <= 0.0 || first_page_height <= 0.0 {
        return vec![(0.0, total_height.max(first_page_height))];
    }

    let constraints = page_breaks::PageConstraints {
        first_page_content_height: first_page_height,
        normal_page_content_height: normal_page_height,
    };
    let breaks = page_breaks::compute_page_breaks_from_display_list(display_list, &constraints);
    let mut spans = page_breaks::page_spans(&breaks, total_height);
    if spans.is_empty() {
        spans.push((0.0, total_height.max(first_page_height)));
    }
    spans
}

/// Text alignment for generated header/footer text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Helper to offset all Y coordinates of a display item.
#[allow(clippy::too_many_lines)] // large but cohesive: single-purpose layout/render/parse routine (one branch per case)
pub(crate) fn offset_display_item_y(item: &DisplayListItem, y_offset: f32) -> DisplayListItem {
    if y_offset == 0.0 {
        return item.clone();
    }

    match item {
        DisplayListItem::StrokedPath {
            bounds,
            path,
            view_box,
            color,
            width,
            mask,
        } => DisplayListItem::StrokedPath {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            path: path.clone(),
            view_box: *view_box,
            color: *color,
            width: *width,
            mask: mask.clone(),
        },
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => DisplayListItem::Rect {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            color: *color,
            border_radius: *border_radius,
        },
        DisplayListItem::Border {
            bounds,
            widths,
            colors,
            styles,
            border_radius,
        } => DisplayListItem::Border {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            widths: *widths,
            colors: *colors,
            styles: *styles,
            border_radius: *border_radius,
        },
        DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px,
            color,
            clip_rect,
            ..
        } => {
            let offset_glyphs: Vec<GlyphInstance> = glyphs
                .iter()
                .map(|g| GlyphInstance {
                    index: g.index,
                    point: LogicalPosition {
                        x: g.point.x,
                        y: g.point.y + y_offset,
                    },
                    size: g.size,
                })
                .collect();
            DisplayListItem::Text {
                glyphs: offset_glyphs,
                font_hash: *font_hash,
                font_size_px: *font_size_px,
                color: *color,
                clip_rect: offset_rect_y(clip_rect.into_inner(), y_offset).into(),
                source_node_index: None,
            }
        }
        DisplayListItem::TextLayout {
            layout,
            bounds,
            font_hash,
            font_size_px,
            color,
        } => DisplayListItem::TextLayout {
            layout: layout.clone(),
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            font_hash: *font_hash,
            font_size_px: *font_size_px,
            color: *color,
        },
        DisplayListItem::Image {
            bounds,
            image,
            border_radius,
        } => DisplayListItem::Image {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            image: image.clone(),
            border_radius: *border_radius,
        },
        // Pass through other items with their bounds offset
        DisplayListItem::SelectionRect {
            bounds,
            border_radius,
            color,
        } => DisplayListItem::SelectionRect {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            border_radius: *border_radius,
            color: *color,
        },
        DisplayListItem::CursorRect { bounds, color } => DisplayListItem::CursorRect {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            color: *color,
        },
        DisplayListItem::Underline {
            bounds,
            color,
            thickness,
        } => DisplayListItem::Underline {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            color: *color,
            thickness: *thickness,
        },
        DisplayListItem::Strikethrough {
            bounds,
            color,
            thickness,
        } => DisplayListItem::Strikethrough {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            color: *color,
            thickness: *thickness,
        },
        DisplayListItem::Overline {
            bounds,
            color,
            thickness,
        } => DisplayListItem::Overline {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            color: *color,
            thickness: *thickness,
        },
        DisplayListItem::ScrollBar {
            bounds,
            color,
            orientation,
            opacity_key,
            hit_id,
        } => DisplayListItem::ScrollBar {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            color: *color,
            orientation: *orientation,
            opacity_key: *opacity_key,
            hit_id: *hit_id,
        },
        DisplayListItem::HitTestArea { bounds, tag } => DisplayListItem::HitTestArea {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            tag: *tag,
        },
        DisplayListItem::PushClip {
            bounds,
            border_radius,
        } => DisplayListItem::PushClip {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            border_radius: *border_radius,
        },
        DisplayListItem::PushScrollFrame {
            clip_bounds,
            content_size,
            scroll_id,
        } => DisplayListItem::PushScrollFrame {
            clip_bounds: offset_rect_y(clip_bounds.into_inner(), y_offset).into(),
            content_size: *content_size,
            scroll_id: *scroll_id,
        },
        DisplayListItem::PushStackingContext { bounds, z_index } => {
            DisplayListItem::PushStackingContext {
                bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
                z_index: *z_index,
            }
        }
        DisplayListItem::VirtualView {
            child_dom_id,
            bounds,
            clip_rect,
            content_offset,
        } => DisplayListItem::VirtualView {
            child_dom_id: *child_dom_id,
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            clip_rect: offset_rect_y(clip_rect.into_inner(), y_offset).into(),
            // Paginating shifts the box down the page; the content's position
            // WITHIN its window is unrelated and must not move.
            content_offset: *content_offset,
        },
        DisplayListItem::VirtualViewPlaceholder {
            node_id,
            bounds,
            clip_rect,
        } => DisplayListItem::VirtualViewPlaceholder {
            node_id: *node_id,
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            clip_rect: offset_rect_y(clip_rect.into_inner(), y_offset).into(),
        },
        // Pass through stateless items
        DisplayListItem::PopClip => DisplayListItem::PopClip,
        DisplayListItem::PopScrollFrame => DisplayListItem::PopScrollFrame,
        DisplayListItem::PopStackingContext => DisplayListItem::PopStackingContext,

        // Gradient items
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => DisplayListItem::LinearGradient {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            gradient: gradient.clone(),
            border_radius: *border_radius,
        },
        DisplayListItem::RadialGradient {
            bounds,
            gradient,
            border_radius,
        } => DisplayListItem::RadialGradient {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            gradient: gradient.clone(),
            border_radius: *border_radius,
        },
        DisplayListItem::ConicGradient {
            bounds,
            gradient,
            border_radius,
        } => DisplayListItem::ConicGradient {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            gradient: gradient.clone(),
            border_radius: *border_radius,
        },

        // BoxShadow
        DisplayListItem::BoxShadow {
            bounds,
            shadow,
            border_radius,
        } => DisplayListItem::BoxShadow {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            shadow: *shadow,
            border_radius: *border_radius,
        },

        // Filter effects
        DisplayListItem::PushFilter { bounds, filters } => DisplayListItem::PushFilter {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            filters: filters.clone(),
        },
        DisplayListItem::PopFilter => DisplayListItem::PopFilter,
        DisplayListItem::PushBackdropFilter { bounds, filters } => {
            DisplayListItem::PushBackdropFilter {
                bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
                filters: filters.clone(),
            }
        }
        DisplayListItem::PopBackdropFilter => DisplayListItem::PopBackdropFilter,
        DisplayListItem::PushOpacity {
            bounds,
            opacity,
            opacity_key,
        } => DisplayListItem::PushOpacity {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            opacity: *opacity,
            opacity_key: *opacity_key,
        },
        DisplayListItem::PopOpacity => DisplayListItem::PopOpacity,
        DisplayListItem::ScrollBarStyled { info } => {
            let mut offset_info = (**info).clone();
            offset_info.bounds = offset_rect_y(offset_info.bounds.into_inner(), y_offset).into();
            offset_info.track_bounds =
                offset_rect_y(offset_info.track_bounds.into_inner(), y_offset).into();
            offset_info.thumb_bounds =
                offset_rect_y(offset_info.thumb_bounds.into_inner(), y_offset).into();
            if let Some(b) = offset_info.button_decrement_bounds {
                offset_info.button_decrement_bounds =
                    Some(offset_rect_y(b.into_inner(), y_offset).into());
            }
            if let Some(b) = offset_info.button_increment_bounds {
                offset_info.button_increment_bounds =
                    Some(offset_rect_y(b.into_inner(), y_offset).into());
            }
            DisplayListItem::ScrollBarStyled {
                info: Box::new(offset_info),
            }
        }

        // Reference frames - offset the bounds
        DisplayListItem::PushReferenceFrame {
            transform_key,
            initial_transform,
            bounds,
        } => DisplayListItem::PushReferenceFrame {
            transform_key: *transform_key,
            initial_transform: *initial_transform,
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
        },
        DisplayListItem::PopReferenceFrame => DisplayListItem::PopReferenceFrame,
        DisplayListItem::PushTextShadow { shadow } => {
            DisplayListItem::PushTextShadow { shadow: *shadow }
        }
        DisplayListItem::PopTextShadow => DisplayListItem::PopTextShadow,
        DisplayListItem::PushImageMaskClip {
            bounds,
            mask_image,
            mask_rect,
        } => DisplayListItem::PushImageMaskClip {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            mask_image: mask_image.clone(),
            mask_rect: offset_rect_y(mask_rect.into_inner(), y_offset).into(),
        },
        DisplayListItem::PopImageMaskClip => DisplayListItem::PopImageMaskClip,
    }
}

/// Generate display list items for simple text (paginated headers/footers).
///
/// Shapes `text` against a registered font (chosen from `renderer_resources`)
/// and emits a single [`DisplayListItem::Text`] whose glyph indices are the
/// font's real GIDs and whose `font_hash` is the font's registered hash, so the
/// renderer (`cpurender::render_text`) can resolve and paint it.
///
/// This is a deliberately simple shaper for short, single-line running
/// headers/footers (e.g. "Page 1 of 3"): per-character cmap lookup + horizontal
/// advance, no complex shaping / kerning / bidi. The full text pipeline is not
/// used because the pagination call site does not carry a styled run — only the
/// header/footer string and a font size/color.
///
/// History: a previous stub fabricated glyphs whose `index` was the Unicode
/// *codepoint* and whose `font_hash` was `0`, which matched no registered font
/// (the renderer logged "Font hash 0 not found" and painted nothing). A later
/// revision returned an empty list. Both rendered no header/footer text; this
/// emits real glyphs.
///
/// Returns an empty list only when `text` is empty, no font is registered, or
/// the chosen font has degenerate metrics (`units_per_em == 0`).
fn generate_text_display_items(
    text: &str,
    bounds: LogicalRect,
    font_size: f32,
    color: ColorU,
    alignment: TextAlignment,
    renderer_resources: &RendererResources,
) -> Vec<DisplayListItem> {
    if text.is_empty() || font_size <= 0.0 {
        return Vec::new();
    }

    // Pick the first registered font. Running headers/footers do not carry a
    // styled run, so there is no per-node font family to resolve; the document's
    // registered font is a reasonable choice for the page furniture.
    let Some((_font_key, (font_ref, _instances))) =
        renderer_resources.currently_registered_fonts.iter().next()
    else {
        return Vec::new();
    };

    let parsed = crate::font_ref_to_parsed_font(font_ref);
    let units_per_em = f32::from(parsed.font_metrics.units_per_em);
    if units_per_em <= 0.0 {
        return Vec::new();
    }
    let scale = font_size / units_per_em;
    let font_hash = parsed.hash;

    // First pass: shape (cmap lookup + advance) and accumulate total width.
    let mut shaped: Vec<(u16, f32)> = Vec::new(); // (glyph_id, advance_px)
    let mut total_width = 0.0f32;
    for c in text.chars() {
        let gid = parsed.lookup_glyph_index(c as u32).unwrap_or(0);
        let advance = f32::from(parsed.get_horizontal_advance(gid)) * scale;
        shaped.push((gid, advance));
        total_width += advance;
    }

    if shaped.is_empty() {
        return Vec::new();
    }

    // Horizontal placement within the box.
    let start_x = match alignment {
        TextAlignment::Center => (bounds.size.width - total_width).mul_add(0.5, bounds.origin.x),
        TextAlignment::Right => bounds.origin.x + (bounds.size.width - total_width),
        TextAlignment::Left => bounds.origin.x,
    };

    // Vertical placement: center the text's em-box in the header/footer band and
    // place the baseline accordingly (point.y is the glyph baseline).
    let ascent_px = parsed.font_metrics.ascent * scale;
    let descent_px = parsed.font_metrics.descent * scale; // hhea descender, usually negative
    let text_height = ascent_px - descent_px;
    let baseline_y = bounds.origin.y + (bounds.size.height - text_height).mul_add(0.5, ascent_px);

    let mut pen_x = start_x;
    let mut glyphs: Vec<GlyphInstance> = Vec::with_capacity(shaped.len());
    for (gid, advance) in shaped {
        let size = parsed
            .get_glyph_size(gid, font_size)
            .unwrap_or(LogicalSize {
                width: advance,
                height: font_size,
            });
        glyphs.push(GlyphInstance {
            index: u32::from(gid),
            point: LogicalPosition {
                x: pen_x,
                y: baseline_y,
            },
            size,
        });
        pen_x += advance;
    }

    vec![DisplayListItem::Text {
        glyphs,
        font_hash: FontHash::from_hash(font_hash),
        font_size_px: font_size,
        color,
        clip_rect: bounds.into(),
        source_node_index: None,
    }]
}

/// Calculate the total height of a display list (max Y + height of all items).
pub(crate) fn calculate_display_list_height(display_list: &DisplayList) -> f32 {
    let mut max_bottom = 0.0f32;

    for item in &display_list.items {
        if let Some(bounds) = get_display_item_bounds(item) {
            // Skip items with zero height - they don't contribute to visible content
            if bounds.0.size.height < 0.1 {
                continue;
            }

            let item_bottom = bounds.0.origin.y + bounds.0.size.height;
            if item_bottom > max_bottom {
                max_bottom = item_bottom;
            }
        }
    }

    max_bottom
}

/// Break property information for pagination decisions.
#[derive(Debug, Clone, Copy, Default)]
// fields mirror the CSS break-before / break-after / break-inside properties
#[allow(clippy::struct_field_names)]
struct BreakProperties {
    break_before: PageBreak,
    break_after: PageBreak,
    break_inside: BreakInside,
}

// ============================================================================
// TEXT-OVERFLOW STUB
// ============================================================================

/// Applies text-overflow ellipsis handling to a display list.
///
/// CSS UI Module Level 3, section 6.2 (text-overflow):
/// When inline content overflows a block container that has `overflow: hidden`
/// (or clip/scroll) and `text-overflow: ellipsis`, the overflowing text should
/// be replaced with an ellipsis character (U+2026) or a custom string.
///
/// This is a display-list post-processing step that modifies glyph runs
/// to show an ellipsis when text overflows its container. It operates on
/// the assumption that the container already has a `PushClip` that clips
/// the overflow -- this function additionally replaces the trailing glyphs
/// with an ellipsis so the user gets a visual indicator of truncation.
///
/// # Parameters
/// - `display_list`: The display list to modify (text items may be clipped/replaced)
/// - `container_bounds`: The bounds of the containing block (overflow boundary)
/// - `_ellipsis`: The ellipsis string (currently unused; U+2026 glyph index is used)
///
/// # Algorithm
/// 1. For each Text item in the display list, check if any glyphs extend
///    past the container's right edge (inline-end in LTR).
/// 2. If so, find the last glyph that fits entirely within the container,
///    accounting for the width of the ellipsis character.
/// 3. Remove all glyphs after that point.
/// 4. Append an ellipsis glyph (U+2026 = glyph index 0x2026 as a fallback;
///    proper glyph lookup requires font metrics not available here).
///
/// Note: This is a best-effort implementation. A pixel-perfect version would
/// need access to font metrics to measure the exact ellipsis glyph width and
/// to look up the correct glyph index for the ellipsis in each font.
// +spec:overflow:f175b9 - bidi ellipsis: characters visually at the end edge of the line are hidden for ellipsis
pub(crate) fn apply_text_overflow_ellipsis(
    display_list: &mut DisplayList,
    container_bounds: LogicalRect,
    _ellipsis: &str,
) {
    let container_right = container_bounds.origin.x + container_bounds.size.width;

    // Approximate ellipsis width as ~0.6 * font_size (typical for "..." in most fonts).
    // This is a heuristic; proper implementation requires font metric access.
    for item in &mut display_list.items {
        if let DisplayListItem::Text {
            glyphs,
            font_size_px,
            clip_rect,
            ..
        } = item
        {
            if glyphs.is_empty() {
                continue;
            }

            // Check if any glyph extends past the container right edge
            let last_glyph = &glyphs[glyphs.len() - 1];
            let last_glyph_right = last_glyph.point.x + last_glyph.size.width;

            if last_glyph_right <= container_right {
                continue; // No overflow, nothing to do
            }

            // Estimate ellipsis width
            let ellipsis_width = *font_size_px * APPROX_ELLIPSIS_WIDTH_RATIO;
            let truncation_edge = container_right - ellipsis_width;

            // Find the last glyph that fits before the truncation edge
            let mut keep_count = 0;
            for (i, glyph) in glyphs.iter().enumerate() {
                let glyph_right = glyph.point.x + glyph.size.width;
                if glyph_right > truncation_edge {
                    break;
                }
                keep_count = i + 1;
            }

            // Truncate the glyphs
            glyphs.truncate(keep_count);

            // Append an ellipsis glyph. We use Unicode codepoint U+2026
            // (HORIZONTAL ELLIPSIS) as the glyph index. This is a common
            // convention; renderers that use proper glyph IDs will need to
            // map this to the font's actual glyph index.
            let ellipsis_x = glyphs.last().map_or(container_bounds.origin.x, |last| {
                last.point.x + last.size.width
            });

            let ellipsis_glyph = GlyphInstance {
                index: 0x2026, // U+2026 HORIZONTAL ELLIPSIS
                point: LogicalPosition::new(
                    ellipsis_x,
                    glyphs
                        .first()
                        .map_or(container_bounds.origin.y, |g| g.point.y),
                ),
                size: LogicalSize::new(ellipsis_width, *font_size_px),
            };

            glyphs.push(ellipsis_glyph);

            // Update the clip rect to match the container bounds so
            // the ellipsis is visible but nothing past it is shown
            *clip_rect = container_bounds.into();
        }
    }
}

// ============================================================================
// CLIP-PATH STUB
// ============================================================================

/// Resolves a CSS clip-path shape to a clipping rectangle.
///
/// CSS Masking Module Level 1, section 3 (clip-path):
/// The clip-path property creates a clipping region that determines which parts
/// of an element are visible. Content outside the clipping region is hidden.
///
/// Currently supported clip-path values:
/// - `inset()` - rectangular clip with optional rounding
/// - `circle()` - approximated as bounding box rectangle
/// - `ellipse()` - approximated as bounding box rectangle
/// - `polygon()` - approximated as axis-aligned bounding box
/// - `none` - no clipping (returns None)
///
/// # Parameters
/// - `clip_path`: The resolved clip-path CSS property value
/// - `node_bounds`: The reference box for resolving clip-path values
///
/// # Returns
/// A `(LogicalRect, f32)` tuple: the clip rectangle and border radius,
/// or `None` if no clipping should be applied.
///
/// Note: Circle, ellipse, and polygon shapes are approximated as axis-aligned
/// bounding boxes. A full implementation would use path-based clipping in the
/// renderer, but rectangular clips work for the most common use cases.
#[allow(clippy::many_single_char_names)] // domain-standard coordinate/geometry/short-lived names
pub(crate) fn resolve_clip_path(
    clip_path: &azul_css::props::layout::shape::ClipPath,
    node_bounds: LogicalRect,
) -> Option<(LogicalRect, f32)> {
    use azul_css::props::layout::shape::ClipPath;
    use azul_css::shape::CssShape;

    match clip_path {
        ClipPath::None => None,
        ClipPath::Shape(shape) => {
            match shape {
                CssShape::Inset(inset) => {
                    // CSS inset() creates a rectangular clip inset from each edge.
                    // inset(top right bottom left round border-radius)
                    let x = node_bounds.origin.x + inset.inset_left;
                    let y = node_bounds.origin.y + inset.inset_top;
                    let w =
                        (node_bounds.size.width - inset.inset_left - inset.inset_right).max(0.0);
                    let h =
                        (node_bounds.size.height - inset.inset_top - inset.inset_bottom).max(0.0);
                    let radius = match inset.border_radius {
                        azul_css::corety::OptionF32::Some(r) => r,
                        azul_css::corety::OptionF32::None => 0.0,
                    };
                    Some((
                        LogicalRect {
                            origin: LogicalPosition::new(x, y),
                            size: LogicalSize::new(w, h),
                        },
                        radius,
                    ))
                }
                CssShape::Circle(circle) => {
                    // Approximate circle as a square bounding box centered at the circle center.
                    // CSS circle(radius at cx cy). The center point coordinates are in
                    // absolute units (pre-resolved by the CSS parser).
                    let cx = node_bounds.origin.x + circle.center.x;
                    let cy = node_bounds.origin.y + circle.center.y;
                    let r = circle.radius;
                    Some((
                        LogicalRect {
                            origin: LogicalPosition::new(cx - r, cy - r),
                            size: LogicalSize::new(r * 2.0, r * 2.0),
                        },
                        r,
                    ))
                }
                CssShape::Ellipse(ellipse) => {
                    // Approximate ellipse as its bounding box.
                    let cx = node_bounds.origin.x + ellipse.center.x;
                    let cy = node_bounds.origin.y + ellipse.center.y;
                    let rx = ellipse.radius_x;
                    let ry = ellipse.radius_y;
                    let radius = rx.min(ry);
                    Some((
                        LogicalRect {
                            origin: LogicalPosition::new(cx - rx, cy - ry),
                            size: LogicalSize::new(rx * 2.0, ry * 2.0),
                        },
                        radius,
                    ))
                }
                CssShape::Polygon(polygon) => {
                    // Compute the axis-aligned bounding box of the polygon.
                    if polygon.points.is_empty() {
                        return None;
                    }
                    let mut min_x = f32::INFINITY;
                    let mut min_y = f32::INFINITY;
                    let mut max_x = f32::NEG_INFINITY;
                    let mut max_y = f32::NEG_INFINITY;
                    for point in &polygon.points {
                        // Polygon points are in absolute coordinates (pre-resolved)
                        let px = node_bounds.origin.x + point.x;
                        let py = node_bounds.origin.y + point.y;
                        min_x = min_x.min(px);
                        min_y = min_y.min(py);
                        max_x = max_x.max(px);
                        max_y = max_y.max(py);
                    }
                    Some((
                        LogicalRect {
                            origin: LogicalPosition::new(min_x, min_y),
                            size: LogicalSize::new(
                                (max_x - min_x).max(0.0),
                                (max_y - min_y).max(0.0),
                            ),
                        },
                        0.0,
                    ))
                }
                CssShape::Path(_) => {
                    // SVG paths are not supported for clip-path yet.
                    // Return the full node bounds (no clipping).
                    None
                }
            }
        }
    }
}

/// Applies a CSS clip-path to the display list by inserting PushClip/PopClip.
///
/// This is a post-processing step that wraps all items between `start_index`
/// and the current end of the display list in a clip region derived from
/// the clip-path shape.
///
/// # Parameters
/// - `display_list`: The display list to modify
/// - `start_index`: The index of the first item belonging to this node
/// - `clip_rect`: The resolved clip rectangle
/// - `border_radius`: The border radius for the clip (from inset round, or circle)
pub(crate) fn apply_clip_path(
    display_list: &mut DisplayList,
    start_index: usize,
    clip_rect: LogicalRect,
    border_radius: f32,
) {
    let br = if border_radius > 0.0 {
        BorderRadius {
            top_left: border_radius,
            top_right: border_radius,
            bottom_left: border_radius,
            bottom_right: border_radius,
        }
    } else {
        BorderRadius::default()
    };

    // Insert PushClip at start_index
    display_list.items.insert(
        start_index,
        DisplayListItem::PushClip {
            bounds: clip_rect.into(),
            border_radius: br,
        },
    );
    // Insert a corresponding None in node_mapping
    if display_list.node_mapping.len() >= start_index {
        display_list.node_mapping.insert(start_index, None);
    }

    // Append PopClip at the end
    display_list.items.push(DisplayListItem::PopClip);
    display_list.node_mapping.push(None);
}

/// Rasterize an `SvgMultiPolygon` clip path into an R8 image mask at the given paint rect size.
///
/// Returns `None` if the rect has zero size.
#[cfg(feature = "cpurender")]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)] // bounded graphics/coord/font/fixed-point/debug-marker cast
/// The STROKE of a path, rasterised as an R8 coverage mask over `paint_rect`.
///
/// The GPU backend's half of `DisplayListItem::StrokedPath`: WebRender has no
/// vector rasteriser, so the stroke is painted as a colour through this mask.
/// The CPU backend does not use it - it strokes the outline directly, which
/// stays sharp at any zoom.
///
/// Rasterised at 2x the logical box for the same reason an icon is: the mask
/// is scaled to the element on screen, and a 1x mask is visibly soft on a
/// HiDPI display.
#[cfg(feature = "cpurender")]
fn rasterize_svg_stroke_to_r8(
    path: &azul_core::svg::SvgMultiPolygon,
    paint_rect: &LogicalRect,
    view_box: Option<(f32, f32, f32, f32)>,
    width: f32,
) -> Option<ImageRef> {
    use agg_rust::{
        basics::FillingRule, color::Rgba8, conv_stroke::ConvStroke, pixfmt_rgba::PixfmtRgba32,
        rasterizer_scanline_aa::RasterizerScanlineAa, renderer_base::RendererBase,
        renderer_scanline::render_scanlines_aa_solid, rendering_buffer::RowAccessor,
        scanline_u::ScanlineU8,
    };
    use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

    const OVERSAMPLE: f32 = 2.0;
    let w = (paint_rect.size.width * OVERSAMPLE).ceil() as u32;
    let h = (paint_rect.size.height * OVERSAMPLE).ceil() as u32;
    if w == 0 || h == 0 || !width.is_finite() || width <= 0.0 {
        return None;
    }

    let (sx, sy, tx, ty) =
        crate::cpurender::pixmap::svg_user_space_mapping(paint_rect, view_box, OVERSAMPLE);
    let mx = |x: f32| f64::from((x + tx) * sx);
    let my = |y: f32| f64::from((y + ty) * sy);
    let mut geometry = crate::cpurender::pixmap::svg_path_to_agg(path, &mx, &my);

    let scale = f64::from((sx + sy) / 2.0);
    // Flattened before stroking, for the same reason the fill is: the stroker
    // walks VERTICES, and a raw curve command has none to offset.
    let mut flattened = agg_rust::conv_curve::ConvCurve::new(&mut geometry);
    let mut stroke = ConvStroke::new(&mut flattened);
    stroke.set_width((f64::from(width) * scale).max(1.0));
    stroke.set_line_cap(agg_rust::math_stroke::LineCap::Round);
    stroke.set_line_join(agg_rust::math_stroke::LineJoin::Round);

    let mut rgba_buf = vec![0u8; (w * h * 4) as usize];
    {
        let stride = (w * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(rgba_buf.as_mut_ptr(), w, h, stride) };
        let pf = PixfmtRgba32::new(&mut ra);
        let mut rb = RendererBase::new(pf);
        let mut ras = RasterizerScanlineAa::new();
        ras.filling_rule(FillingRule::NonZero);
        ras.add_path(&mut stroke, 0);
        let mut sl = ScanlineU8::new();
        let white = Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &white);
    }

    let r8_data: Vec<u8> = rgba_buf.chunks_exact(4).map(|px| px[3]).collect();
    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(r8_data.into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::R8,
        tag: Vec::new().into(),
    })
}

fn rasterize_svg_clip_to_r8(
    svg_clip: &azul_core::svg::SvgMultiPolygon,
    paint_rect: &LogicalRect,
    view_box: Option<(f32, f32, f32, f32)>,
) -> Option<ImageRef> {
    use agg_rust::{
        basics::FillingRule, color::Rgba8, path_storage::PathStorage, pixfmt_rgba::PixfmtRgba32,
        rasterizer_scanline_aa::RasterizerScanlineAa, renderer_base::RendererBase,
        renderer_scanline::render_scanlines_aa_solid, rendering_buffer::RowAccessor,
        scanline_u::ScanlineU8,
    };
    use azul_core::resources::{ImageRef, RawImage, RawImageData, RawImageFormat};

    // OVERSAMPLED, for the same reason the stroke mask is: the mask is
    // rasterised here in LOGICAL pixels and applied in DEVICE pixels, so on a
    // HiDPI display it is scaled UP. A 1x mask has no detail to scale, and a
    // circle arrives with a staircase edge no matter how it is resampled.
    const OVERSAMPLE: f32 = 2.0;
    let w = (paint_rect.size.width * OVERSAMPLE).ceil() as u32;
    let h = (paint_rect.size.height * OVERSAMPLE).ceil() as u32;
    if w == 0 || h == 0 {
        return None;
    }

    // USER SPACE -> MASK PIXELS.
    //
    // With a viewBox the geometry is in the `<svg>`'s own coordinate system
    // and has to be SCALED into the box the element ended up with: an icon
    // drawn at 16 units in a 64px slot must fill the slot, not a sixteenth of
    // it. Without one (a bare clip-path on an ordinary element) the geometry
    // is already in window-logical px, and only the paint rect's origin is
    // subtracted - the behaviour every existing clip mask relies on.
    let (sx, sy, tx, ty) = match view_box {
        Some((min_x, min_y, vb_w, vb_h)) if vb_w > 0.0 && vb_h > 0.0 => (
            paint_rect.size.width * OVERSAMPLE / vb_w,
            paint_rect.size.height * OVERSAMPLE / vb_h,
            -min_x,
            -min_y,
        ),
        _ => (
            OVERSAMPLE,
            OVERSAMPLE,
            -paint_rect.origin.x,
            -paint_rect.origin.y,
        ),
    };
    let mx = |x: f32| f64::from((x + tx) * sx);
    let my = |y: f32| f64::from((y + ty) * sy);

    // Build agg PathStorage from SvgMultiPolygon
    let mut path = PathStorage::new();
    for ring in svg_clip.rings.as_ref() {
        let mut first = true;
        for item in ring.items.as_ref() {
            match item {
                azul_core::svg::SvgPathElement::Line(l) => {
                    if first {
                        path.move_to(
                            mx(l.start.x),
                            my(l.start.y),
                        );
                        first = false;
                    }
                    path.line_to(
                        mx(l.end.x),
                        my(l.end.y),
                    );
                }
                azul_core::svg::SvgPathElement::QuadraticCurve(q) => {
                    if first {
                        path.move_to(
                            mx(q.start.x),
                            my(q.start.y),
                        );
                        first = false;
                    }
                    path.curve3(
                        mx(q.ctrl.x),
                        my(q.ctrl.y),
                        mx(q.end.x),
                        my(q.end.y),
                    );
                }
                azul_core::svg::SvgPathElement::CubicCurve(c) => {
                    if first {
                        path.move_to(
                            mx(c.start.x),
                            my(c.start.y),
                        );
                        first = false;
                    }
                    path.curve4(
                        mx(c.ctrl_1.x),
                        my(c.ctrl_1.y),
                        mx(c.ctrl_2.x),
                        my(c.ctrl_2.y),
                        mx(c.end.x),
                        my(c.end.y),
                    );
                }
            }
        }
    }

    // Rasterize to RGBA32 buffer
    let mut rgba_buf = vec![0u8; (w * h * 4) as usize];
    {
        let stride = (w * 4) as i32;
        let mut ra = unsafe { RowAccessor::new_with_buf(rgba_buf.as_mut_ptr(), w, h, stride) };
        let pf = PixfmtRgba32::new(&mut ra);
        let mut rb = RendererBase::new(pf);

        let mut ras = RasterizerScanlineAa::new();
        ras.filling_rule(FillingRule::NonZero);
        // FLATTENED first. `PathStorage` stores curve3/curve4 as COMMANDS;
        // the rasteriser does not subdivide them, so feeding it the raw
        // storage draws straight lines between the curve endpoints - a circle
        // built from four cubics came out as a four-sided blob. `ConvCurve` is
        // the adaptor that turns those commands into line segments.
        let mut flattened = agg_rust::conv_curve::ConvCurve::new(&mut path);
        ras.add_path(&mut flattened, 0);

        let mut sl = ScanlineU8::new();
        let white = Rgba8 {
            r: 255,
            g: 255,
            b: 255,
            a: 255,
        };
        render_scanlines_aa_solid(&mut ras, &mut sl, &mut rb, &white);
    }

    // Extract alpha channel as R8 mask
    let r8_data: Vec<u8> = rgba_buf.chunks_exact(4).map(|px| px[3]).collect();

    ImageRef::new_rawimage(RawImage {
        pixels: RawImageData::U8(r8_data.into()),
        width: w as usize,
        height: h as usize,
        premultiplied_alpha: false,
        data_format: RawImageFormat::R8,
        tag: Vec::new().into(),
    })
}

#[cfg(test)]
mod pagination_text_tests {
    use super::*;
    use crate::font::parsed::ParsedFont;
    use azul_core::resources::{FontKey, IdNamespace};

    /// Loads a system font for testing, retaining source bytes so advances work.
    fn load_test_font() -> Option<ParsedFont> {
        let candidates = [
            "/System/Library/Fonts/Supplemental/Times New Roman.ttf",
            "/System/Library/Fonts/Helvetica.ttc",
            "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
            "/usr/share/fonts/truetype/liberation/LiberationSans-Regular.ttf",
            "C:/Windows/Fonts/arial.ttf",
        ];
        for path in candidates {
            if let Ok(bytes) = std::fs::read(path) {
                let arc = Arc::new(rust_fontconfig::FontBytes::Owned(Arc::from(
                    bytes.as_slice(),
                )));
                if let Some(font) = ParsedFont::from_bytes(&bytes, 0, &mut Vec::new())
                    .map(|f| f.with_source_bytes(arc))
                {
                    return Some(font);
                }
            }
        }
        None
    }

    fn renderer_resources_with(font: ParsedFont) -> RendererResources {
        let mut rr = RendererResources::default();
        let font_ref = crate::parsed_font_to_font_ref(font);
        let key = FontKey::unique(IdNamespace(0));
        let hash = crate::font_ref_to_parsed_font(&font_ref).hash;
        rr.font_hash_map.insert(hash, key);
        rr.currently_registered_fonts
            .insert(key, (font_ref, BTreeMap::default()));
        rr
    }

    /// The pagination header/footer text path must emit real glyph display items
    /// (the audit flagged it as a no-op rendering nothing).
    #[test]
    fn generate_text_display_items_emits_glyphs() {
        let Some(font) = load_test_font() else {
            eprintln!("[skip] no system font available");
            return;
        };
        let expected_hash = font.hash;
        let rr = renderer_resources_with(font);

        let bounds = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 400.0,
                height: 30.0,
            },
        };
        let items = generate_text_display_items(
            "Page 1 of 3",
            bounds,
            14.0,
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            TextAlignment::Center,
            &rr,
        );

        assert_eq!(items.len(), 1, "expected exactly one Text item");
        match &items[0] {
            DisplayListItem::Text {
                glyphs,
                font_hash,
                color,
                ..
            } => {
                assert_eq!(glyphs.len(), "Page 1 of 3".chars().count());
                assert_eq!(
                    font_hash.font_hash, expected_hash,
                    "must use registered font hash"
                );
                assert_ne!(font_hash.font_hash, 0, "hash 0 resolves no font");
                assert_eq!(color.a, 255);
                // Glyph IDs must be real (cmap-resolved), not raw codepoints.
                let p_gid = glyphs[0].index;
                assert_ne!(
                    p_gid, 'P' as u32,
                    "glyph index must be a GID, not a codepoint"
                );
                // Pen must advance: x coordinates strictly increase across the run.
                assert!(glyphs[1].point.x > glyphs[0].point.x, "pen did not advance");
            }
            other => panic!("expected DisplayListItem::Text, got {other:?}"),
        }
    }

    /// With no registered fonts there is nothing to shape against -> empty.
    #[test]
    fn generate_text_display_items_empty_without_font() {
        let rr = RendererResources::default();
        let bounds = LogicalRect {
            origin: LogicalPosition { x: 0.0, y: 0.0 },
            size: LogicalSize {
                width: 400.0,
                height: 30.0,
            },
        };
        let items = generate_text_display_items(
            "Header",
            bounds,
            14.0,
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            TextAlignment::Center,
            &rr,
        );
        assert!(items.is_empty());
    }
}

#[cfg(test)]
#[allow(clippy::float_cmp, clippy::too_many_lines)]
mod autotest_generated {
    use super::*;

    // ---------------------------------------------------------------------
    // Construction helpers
    // ---------------------------------------------------------------------

    fn rect(x: f32, y: f32, w: f32, h: f32) -> LogicalRect {
        LogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h))
    }

    fn opaque() -> ColorU {
        ColorU {
            r: 10,
            g: 20,
            b: 30,
            a: 255,
        }
    }

    fn glyph(index: u32, x: f32, y: f32) -> GlyphInstance {
        GlyphInstance {
            index,
            point: LogicalPosition::new(x, y),
            size: LogicalSize::new(8.0, 12.0),
        }
    }

    fn no_widths() -> StyleBorderWidths {
        StyleBorderWidths {
            top: None,
            right: None,
            bottom: None,
            left: None,
        }
    }

    fn all_widths() -> StyleBorderWidths {
        StyleBorderWidths {
            top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth::default())),
            right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth::default())),
            bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth::default())),
            left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth::default())),
        }
    }

    fn no_colors() -> StyleBorderColors {
        StyleBorderColors {
            top: None,
            right: None,
            bottom: None,
            left: None,
        }
    }

    fn no_styles() -> StyleBorderStyles {
        StyleBorderStyles {
            top: None,
            right: None,
            bottom: None,
            left: None,
        }
    }

    /// Every side `Some(none)` — the default style, and what the compact
    /// cache answers for an unset border.
    fn all_styles() -> StyleBorderStyles {
        StyleBorderStyles {
            top: Some(CssPropertyValue::Exact(StyleBorderTopStyle::default())),
            right: Some(CssPropertyValue::Exact(StyleBorderRightStyle::default())),
            bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle::default())),
            left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle::default())),
        }
    }

    fn all_solid_styles() -> StyleBorderStyles {
        StyleBorderStyles {
            top: Some(CssPropertyValue::Exact(StyleBorderTopStyle {
                inner: BorderStyle::Solid,
            })),
            right: Some(CssPropertyValue::Exact(StyleBorderRightStyle {
                inner: BorderStyle::Solid,
            })),
            bottom: Some(CssPropertyValue::Exact(StyleBorderBottomStyle {
                inner: BorderStyle::Solid,
            })),
            left: Some(CssPropertyValue::Exact(StyleBorderLeftStyle {
                inner: BorderStyle::Solid,
            })),
        }
    }

    /// Every side `Some(0px)` — what the compact cache answers for an unset
    /// border width.
    fn zero_widths() -> StyleBorderWidths {
        StyleBorderWidths {
            top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth {
                inner: PixelValue::px(0.0),
            })),
            right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth {
                inner: PixelValue::px(0.0),
            })),
            bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth {
                inner: PixelValue::px(0.0),
            })),
            left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth {
                inner: PixelValue::px(0.0),
            })),
        }
    }

    fn zero_style_radius() -> StyleBorderRadius {
        StyleBorderRadius {
            top_left: PixelValue::zero(),
            top_right: PixelValue::zero(),
            bottom_left: PixelValue::zero(),
            bottom_right: PixelValue::zero(),
        }
    }

    fn test_image() -> ImageRef {
        ImageRef::null_image(
            4,
            4,
            azul_core::resources::RawImageFormat::RGBA8,
            Vec::new(),
        )
    }

    fn text_item(
        src: Option<usize>,
        clip: LogicalRect,
        glyphs: Vec<GlyphInstance>,
    ) -> DisplayListItem {
        DisplayListItem::Text {
            glyphs,
            font_hash: FontHash::from_hash(7),
            font_size_px: 16.0,
            color: opaque(),
            clip_rect: clip.into(),
            source_node_index: src,
        }
    }

    fn list_of(items: Vec<DisplayListItem>) -> DisplayList {
        let node_mapping = vec![None; items.len()];
        DisplayList {
            items,
            node_mapping,
            ..DisplayList::default()
        }
    }

    #[cfg(feature = "text_layout")]
    fn positioned(line_index: usize, x: f32, y: f32, w: f32, h: f32) -> PositionedItem {
        PositionedItem {
            item: ShapedItem::Tab {
                source: azul_core::selection::ContentIndex {
                    run_index: 0,
                    item_index: 0,
                },
                bounds: text3::cache::Rect {
                    x: 0.0,
                    y: 0.0,
                    width: w,
                    height: h,
                },
            },
            position: text3::cache::Point { x, y },
            line_index,
        }
    }

    // ---------------------------------------------------------------------
    // WindowLogicalRect / BorderBoxRect / ContentBoxRect
    // ---------------------------------------------------------------------

    #[test]
    fn window_logical_rect_accessors_roundtrip() {
        let origin = LogicalPosition::new(-3.5, 12.25);
        let size = LogicalSize::new(100.0, 40.0);
        let w = WindowLogicalRect::new(origin, size);

        assert_eq!(w.origin(), origin);
        assert_eq!(w.size(), size);
        assert_eq!(*w.inner(), LogicalRect::new(origin, size));
        assert_eq!(w.into_inner(), LogicalRect::new(origin, size));
        // From/Into must be the identity on the wrapped rect.
        assert_eq!(WindowLogicalRect::from(w.into_inner()), w);
        assert_eq!(LogicalRect::from(w), w.into_inner());
    }

    #[test]
    fn window_logical_rect_zero_is_neutral() {
        let z = WindowLogicalRect::zero();
        assert_eq!(z, WindowLogicalRect::default());
        assert_eq!(z.origin(), LogicalPosition::zero());
        assert_eq!(z.size(), LogicalSize::zero());
        assert_eq!(z.into_inner(), LogicalRect::zero());
    }

    #[test]
    fn window_logical_rect_extreme_values_do_not_panic() {
        for (x, y, w, h) in [
            (f32::MAX, f32::MAX, f32::MAX, f32::MAX),
            (f32::MIN, f32::MIN, 0.0, 0.0),
            (f32::INFINITY, f32::NEG_INFINITY, f32::INFINITY, 0.0),
            (f32::NAN, f32::NAN, f32::NAN, f32::NAN),
        ] {
            let r = WindowLogicalRect::new(LogicalPosition::new(x, y), LogicalSize::new(w, h));
            // Accessors must round-trip the raw bits regardless of how odd they are.
            assert_eq!(r.origin().x.to_bits(), x.to_bits());
            assert_eq!(r.size().height.to_bits(), h.to_bits());
            let _ = format!("{r:?}");
        }
    }

    #[test]
    fn border_box_to_content_box_subtracts_padding_and_border() {
        let bb = BorderBoxRect(rect(10.0, 20.0, 100.0, 50.0));
        let padding = crate::solver3::geometry::EdgeSizes {
            top: 1.0,
            right: 2.0,
            bottom: 3.0,
            left: 4.0,
        };
        let border = crate::solver3::geometry::EdgeSizes {
            top: 5.0,
            right: 6.0,
            bottom: 7.0,
            left: 8.0,
        };

        let cb = bb.to_content_box(&padding, &border);
        assert_eq!(cb.rect(), rect(22.0, 26.0, 80.0, 34.0));
        assert_eq!(
            bb.rect(),
            rect(10.0, 20.0, 100.0, 50.0),
            "receiver copy is unchanged"
        );
    }

    #[test]
    fn border_box_to_content_box_zero_edges_is_identity() {
        let bb = BorderBoxRect(rect(1.0, 2.0, 3.0, 4.0));
        let zero = crate::solver3::geometry::EdgeSizes::default();
        assert_eq!(bb.to_content_box(&zero, &zero).rect(), bb.rect());
    }

    #[test]
    fn border_box_to_content_box_overinset_yields_negative_size_not_a_panic() {
        // padding + border exceed the box: the result is a NEGATIVE content box.
        // Nothing clamps it, so downstream code must tolerate it. Pin that here so
        // a future clamp is a deliberate, visible change.
        let bb = BorderBoxRect(rect(0.0, 0.0, 10.0, 10.0));
        let big = crate::solver3::geometry::EdgeSizes {
            top: 50.0,
            right: 50.0,
            bottom: 50.0,
            left: 50.0,
        };
        let cb = bb.to_content_box(&big, &big);
        assert!(cb.rect().size.width < 0.0);
        assert!(cb.rect().size.height < 0.0);
    }

    #[test]
    fn border_box_to_content_box_nan_and_inf_do_not_panic() {
        let bb = BorderBoxRect(rect(0.0, 0.0, f32::MAX, f32::MAX));
        let nan = crate::solver3::geometry::EdgeSizes {
            top: f32::NAN,
            right: f32::NAN,
            bottom: f32::NAN,
            left: f32::NAN,
        };
        let inf = crate::solver3::geometry::EdgeSizes {
            top: f32::INFINITY,
            right: f32::INFINITY,
            bottom: f32::INFINITY,
            left: f32::INFINITY,
        };
        assert!(bb.to_content_box(&nan, &nan).rect().size.width.is_nan());
        // MAX - inf - inf ... = -inf (defined, not a trap)
        assert!(bb
            .to_content_box(&inf, &inf)
            .rect()
            .size
            .width
            .is_infinite());
    }

    #[test]
    fn content_box_rect_getter_returns_wrapped_rect() {
        let r = rect(-1.0, -2.0, 0.0, 0.0);
        assert_eq!(ContentBoxRect(r).rect(), r);
        assert_eq!(
            ContentBoxRect(LogicalRect::zero()).rect(),
            LogicalRect::zero()
        );
    }

    // ---------------------------------------------------------------------
    // BorderRadius::is_zero
    // ---------------------------------------------------------------------

    #[test]
    fn border_radius_is_zero_basic() {
        assert!(BorderRadius::default().is_zero());
        assert!(!BorderRadius {
            top_left: 1.0,
            ..BorderRadius::default()
        }
        .is_zero());
        assert!(!BorderRadius {
            bottom_right: 0.001,
            ..BorderRadius::default()
        }
        .is_zero());
    }

    #[test]
    fn border_radius_is_zero_edge_floats() {
        // -0.0 == 0.0 in IEEE-754, so a negative zero radius still counts as zero.
        assert!(BorderRadius {
            top_left: -0.0,
            top_right: -0.0,
            bottom_left: -0.0,
            bottom_right: -0.0
        }
        .is_zero());
        // NaN != 0.0, so a NaN radius is (conservatively) *not* zero — no panic.
        assert!(!BorderRadius {
            top_left: f32::NAN,
            ..BorderRadius::default()
        }
        .is_zero());
        assert!(!BorderRadius {
            top_right: f32::INFINITY,
            ..BorderRadius::default()
        }
        .is_zero());
        // A negative radius is not zero either.
        assert!(!BorderRadius {
            bottom_left: -5.0,
            ..BorderRadius::default()
        }
        .is_zero());
    }

    // ---------------------------------------------------------------------
    // DisplayListItem predicates / getters
    // ---------------------------------------------------------------------

    #[test]
    fn is_state_management_true_for_push_pop_only() {
        let state = [
            DisplayListItem::PushClip {
                bounds: rect(0.0, 0.0, 1.0, 1.0).into(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PopClip,
            DisplayListItem::PopImageMaskClip,
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PopStackingContext,
            DisplayListItem::PopReferenceFrame,
            DisplayListItem::PopFilter,
            DisplayListItem::PopBackdropFilter,
            DisplayListItem::PopOpacity,
            DisplayListItem::PopTextShadow,
            DisplayListItem::PushStackingContext {
                z_index: 0,
                bounds: WindowLogicalRect::zero(),
            },
            DisplayListItem::PushOpacity {
                bounds: WindowLogicalRect::zero(),
                opacity: 0.5,
                opacity_key: None,
            },
            DisplayListItem::PushTextShadow {
                shadow: StyleBoxShadow::default(),
            },
        ];
        for item in &state {
            assert!(
                item.is_state_management(),
                "{item:?} must be state management"
            );
        }

        let drawing = [
            DisplayListItem::Rect {
                bounds: WindowLogicalRect::zero(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::CursorRect {
                bounds: WindowLogicalRect::zero(),
                color: opaque(),
            },
            // HitTestArea paints nothing but is NOT a stack command — it must not be forced through.
            DisplayListItem::HitTestArea {
                bounds: WindowLogicalRect::zero(),
                tag: (0, TAG_TYPE_DOM_NODE),
            },
            text_item(None, LogicalRect::zero(), Vec::new()),
        ];
        for item in &drawing {
            assert!(
                !item.is_state_management(),
                "{item:?} must NOT be state management"
            );
        }
    }

    #[test]
    fn bounds_reports_none_only_for_pop_and_text_shadow() {
        let r = rect(1.0, 2.0, 3.0, 4.0);
        assert_eq!(
            DisplayListItem::Rect {
                bounds: r.into(),
                color: opaque(),
                border_radius: BorderRadius::default()
            }
            .bounds(),
            Some(r)
        );
        // Text reports its CLIP rect as its bounds, not a glyph hull.
        assert_eq!(
            text_item(Some(0), r, vec![glyph(1, 999.0, 999.0)]).bounds(),
            Some(r)
        );
        assert_eq!(
            DisplayListItem::PushScrollFrame {
                clip_bounds: r.into(),
                content_size: LogicalSize::new(9.0, 9.0),
                scroll_id: 3,
            }
            .bounds(),
            Some(r)
        );
        assert_eq!(DisplayListItem::PopClip.bounds(), None);
        assert_eq!(DisplayListItem::PopOpacity.bounds(), None);
        assert_eq!(
            DisplayListItem::PushTextShadow {
                shadow: StyleBoxShadow::default()
            }
            .bounds(),
            None,
            "a text shadow has no bounds of its own"
        );
    }

    #[test]
    fn bounds_on_degenerate_rects_does_not_panic() {
        for r in [
            LogicalRect::zero(),
            rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN),
            rect(f32::MIN, f32::MIN, f32::MAX, f32::MAX),
            rect(0.0, 0.0, -10.0, -10.0),
        ] {
            let item = DisplayListItem::Rect {
                bounds: r.into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            };
            assert!(item.bounds().is_some());
            assert!(item.visual_bounds().is_some());
        }
    }

    #[test]
    fn visual_bounds_matches_bounds_for_non_shadow_items() {
        let r = rect(5.0, 6.0, 7.0, 8.0);
        let item = DisplayListItem::Rect {
            bounds: r.into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert_eq!(item.visual_bounds(), item.bounds());
        assert_eq!(DisplayListItem::PopClip.visual_bounds(), None);
    }

    #[test]
    fn visual_bounds_expands_box_shadow_by_offset_blur_and_spread() {
        use azul_css::props::basic::pixel::PixelValueNoPercent;
        let shadow = StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(2),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(3),
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(4),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(5),
            },
            clip_mode: BoxShadowClipMode::default(),
            color: ColorU::BLACK,
        };
        let item = DisplayListItem::BoxShadow {
            bounds: rect(100.0, 100.0, 50.0, 50.0).into(),
            shadow,
            border_radius: BorderRadius::default(),
        };
        // expand = |2| + |3| + |4| + |5| = 14, applied on every side.
        let vb = item.visual_bounds().expect("box shadow has visual bounds");
        assert_eq!(vb, rect(86.0, 86.0, 78.0, 78.0));
        // The visual bounds must strictly contain the paint bounds.
        let b = item.bounds().unwrap();
        assert!(vb.origin.x < b.origin.x && vb.size.width > b.size.width);
    }

    #[test]
    fn visual_bounds_box_shadow_with_negative_offsets_uses_absolute_values() {
        use azul_css::props::basic::pixel::PixelValueNoPercent;
        let shadow = StyleBoxShadow {
            offset_x: PixelValueNoPercent {
                inner: PixelValue::const_px(-10),
            },
            offset_y: PixelValueNoPercent {
                inner: PixelValue::const_px(-10),
            },
            blur_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            spread_radius: PixelValueNoPercent {
                inner: PixelValue::const_px(0),
            },
            clip_mode: BoxShadowClipMode::default(),
            color: ColorU::BLACK,
        };
        let item = DisplayListItem::BoxShadow {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            shadow,
            border_radius: BorderRadius::default(),
        };
        // .abs() is applied, so the shadow expands symmetrically by 20 in each direction.
        assert_eq!(
            item.visual_bounds().unwrap(),
            rect(-20.0, -20.0, 50.0, 50.0)
        );
    }

    // ---------------------------------------------------------------------
    // DisplayListItem::is_visually_equal
    // ---------------------------------------------------------------------

    #[test]
    fn is_visually_equal_reflexive_and_discriminant_guarded() {
        let a = DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert!(a.is_visually_equal(&a));
        assert!(
            !a.is_visually_equal(&DisplayListItem::PopClip),
            "different variants are never equal"
        );
        assert!(!DisplayListItem::PopClip.is_visually_equal(&a));
    }

    #[test]
    fn is_visually_equal_detects_field_changes() {
        let base = DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        let moved = DisplayListItem::Rect {
            bounds: rect(1.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        let recolored = DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            color: ColorU {
                r: 255,
                g: 0,
                b: 0,
                a: 255,
            },
            border_radius: BorderRadius::default(),
        };
        let rounded = DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius {
                top_left: 4.0,
                ..BorderRadius::default()
            },
        };
        assert!(!base.is_visually_equal(&moved));
        assert!(!base.is_visually_equal(&recolored));
        assert!(!base.is_visually_equal(&rounded));
    }

    #[test]
    fn is_visually_equal_pops_are_always_equal() {
        for (a, b) in [
            (DisplayListItem::PopClip, DisplayListItem::PopClip),
            (
                DisplayListItem::PopScrollFrame,
                DisplayListItem::PopScrollFrame,
            ),
            (DisplayListItem::PopOpacity, DisplayListItem::PopOpacity),
            (
                DisplayListItem::PopTextShadow,
                DisplayListItem::PopTextShadow,
            ),
        ] {
            assert!(a.is_visually_equal(&b));
        }
        assert!(!DisplayListItem::PopClip.is_visually_equal(&DisplayListItem::PopOpacity));
    }

    #[test]
    fn is_visually_equal_hit_test_areas_never_damage() {
        // Documented: hit-test areas paint no pixels, so ANY two are visually equal
        // (regression guard for issue #12 — a moved hit region must not force a repaint).
        let a = DisplayListItem::HitTestArea {
            bounds: rect(0.0, 0.0, 1.0, 1.0).into(),
            tag: (1, TAG_TYPE_DOM_NODE),
        };
        let b = DisplayListItem::HitTestArea {
            bounds: rect(500.0, 900.0, 7.0, 7.0).into(),
            tag: (99, TAG_TYPE_CURSOR),
        };
        assert!(a.is_visually_equal(&b));
    }

    #[test]
    fn is_visually_equal_text_layout_uses_arc_pointer_identity() {
        let shared: Arc<dyn std::any::Any + Send + Sync> = Arc::new(42u32);
        let other: Arc<dyn std::any::Any + Send + Sync> = Arc::new(42u32);
        let make = |layout: Arc<dyn std::any::Any + Send + Sync>| DisplayListItem::TextLayout {
            layout,
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            font_hash: FontHash::from_hash(1),
            font_size_px: 16.0,
            color: opaque(),
        };
        assert!(
            make(shared.clone()).is_visually_equal(&make(shared)),
            "same Arc => reuse => no damage"
        );
        assert!(
            !make(Arc::new(42u32)).is_visually_equal(&make(other)),
            "distinct allocations => conservatively different"
        );
    }

    #[test]
    fn is_visually_equal_image_uses_pointer_identity_not_content() {
        let img = test_image();
        let a = DisplayListItem::Image {
            bounds: rect(0.0, 0.0, 4.0, 4.0).into(),
            image: img.clone(),
            border_radius: BorderRadius::default(),
        };
        let same_alloc = DisplayListItem::Image {
            bounds: rect(0.0, 0.0, 4.0, 4.0).into(),
            image: img,
            border_radius: BorderRadius::default(),
        };
        assert!(a.is_visually_equal(&same_alloc));

        // A byte-identical but separately allocated image is conservatively "different".
        let b = DisplayListItem::Image {
            bounds: rect(0.0, 0.0, 4.0, 4.0).into(),
            image: test_image(),
            border_radius: BorderRadius::default(),
        };
        assert!(!a.is_visually_equal(&b));
    }

    #[test]
    fn is_visually_equal_text_compares_glyph_ids_and_positions() {
        let clip = rect(0.0, 0.0, 100.0, 20.0);
        let a = text_item(
            Some(1),
            clip,
            vec![glyph(5, 0.0, 10.0), glyph(6, 8.0, 10.0)],
        );
        let same = text_item(
            Some(999),
            clip,
            vec![glyph(5, 0.0, 10.0), glyph(6, 8.0, 10.0)],
        );
        let diff_gid = text_item(
            Some(1),
            clip,
            vec![glyph(5, 0.0, 10.0), glyph(7, 8.0, 10.0)],
        );
        let diff_pos = text_item(
            Some(1),
            clip,
            vec![glyph(5, 0.0, 10.0), glyph(6, 9.0, 10.0)],
        );
        let shorter = text_item(Some(1), clip, vec![glyph(5, 0.0, 10.0)]);
        let empty = text_item(Some(1), clip, Vec::new());

        assert!(
            a.is_visually_equal(&same),
            "source_node_index is not a visual property"
        );
        assert!(!a.is_visually_equal(&diff_gid));
        assert!(!a.is_visually_equal(&diff_pos));
        assert!(
            !a.is_visually_equal(&shorter),
            "glyph count mismatch => different"
        );
        assert!(!a.is_visually_equal(&empty));
        assert!(
            empty.is_visually_equal(&empty),
            "empty glyph runs are equal to each other"
        );
    }

    #[test]
    fn is_visually_equal_nan_thickness_is_conservatively_unequal() {
        // Raw f32 `==` on thickness: NaN != NaN, so even a self-comparison of a NaN
        // thickness reports "changed". That is the SAFE direction (forces a repaint),
        // and it must not panic.
        let a = DisplayListItem::Underline {
            bounds: rect(0.0, 0.0, 10.0, 1.0).into(),
            color: opaque(),
            thickness: f32::NAN,
        };
        assert!(!a.is_visually_equal(&a));
    }

    #[test]
    fn is_visually_equal_bounds_are_quantized_to_a_thousandth_of_a_pixel() {
        // LogicalRect equality is fixed-point (1/1000 px). Sub-quantum jitter is
        // deliberately treated as "no visual change" so float noise cannot damage.
        let a = DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        let jittered = DisplayListItem::Rect {
            bounds: rect(0.000_01, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert!(a.is_visually_equal(&jittered));

        // A whole-pixel move is above the quantum and *is* reported.
        let moved = DisplayListItem::Rect {
            bounds: rect(1.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert!(!a.is_visually_equal(&moved));
    }

    // ---------------------------------------------------------------------
    // DisplayListBuilder
    // ---------------------------------------------------------------------

    #[test]
    fn builder_new_is_empty_and_matches_with_debug_false() {
        let b = DisplayListBuilder::new();
        assert!(b.items.is_empty());
        assert!(b.node_mapping.is_empty());
        assert!(!b.debug_enabled);
        assert!(b.forced_page_breaks.is_empty());
        assert!(b.fixed_position_item_ranges.is_empty());
        assert!(b.fixed_position_start.is_none());

        let dl = DisplayListBuilder::new().build();
        assert!(dl.items.is_empty());
        assert!(dl.node_mapping.is_empty());
    }

    #[test]
    fn builder_with_debug_toggles_message_collection() {
        let mut off = DisplayListBuilder::with_debug(false);
        off.debug_log("dropped".to_string());
        assert!(off.debug_messages.is_empty());

        let mut on = DisplayListBuilder::with_debug(true);
        on.debug_log(String::new());
        on.debug_log("x".repeat(100_000)); // huge message must not panic
        on.debug_log("🦀 unicode \u{0}\u{FFFD} nul".to_string());
        assert_eq!(on.debug_messages.len(), 3);
    }

    #[test]
    fn builder_push_item_keeps_node_mapping_in_lockstep() {
        let mut b = DisplayListBuilder::new();
        b.set_current_node(Some(NodeId::new(4)));
        b.push_rect(rect(0.0, 0.0, 1.0, 1.0), opaque(), BorderRadius::default());
        b.set_current_node(None);
        b.pop_clip();

        let dl = b.build();
        assert_eq!(dl.items.len(), 2);
        assert_eq!(
            dl.items.len(),
            dl.node_mapping.len(),
            "node_mapping must parallel items"
        );
        assert_eq!(dl.node_mapping[0], Some(NodeId::new(4)));
        assert_eq!(dl.node_mapping[1], None);
    }

    #[test]
    fn builder_skips_fully_transparent_fills() {
        let mut b = DisplayListBuilder::new();
        b.push_rect(
            rect(0.0, 0.0, 10.0, 10.0),
            ColorU::TRANSPARENT,
            BorderRadius::default(),
        );
        b.push_selection_rect(
            rect(0.0, 0.0, 10.0, 10.0),
            ColorU::TRANSPARENT,
            BorderRadius::default(),
        );
        b.push_scrollbar(
            rect(0.0, 0.0, 10.0, 10.0),
            ColorU::TRANSPARENT,
            ScrollbarOrientation::Vertical,
            None,
            None,
        );
        assert!(
            b.items.is_empty(),
            "alpha == 0 with no opacity key paints nothing"
        );

        // An opacity key means the alpha is animated on the GPU — it must still be pushed.
        b.push_scrollbar(
            rect(0.0, 0.0, 10.0, 10.0),
            ColorU::TRANSPARENT,
            ScrollbarOrientation::Vertical,
            Some(OpacityKey::unique()),
            None,
        );
        assert_eq!(b.items.len(), 1);
    }

    #[test]
    fn builder_cursor_rect_is_emitted_even_when_invisible() {
        // Blink-off carets MUST still emit an item so the item count stays stable
        // across blink phases (otherwise damage falls back to a full-window repaint).
        let mut b = DisplayListBuilder::new();
        b.push_cursor_rect(rect(0.0, 0.0, 1.0, 16.0), ColorU::TRANSPARENT);
        assert_eq!(b.items.len(), 1);
        assert!(matches!(b.items[0], DisplayListItem::CursorRect { .. }));
    }

    #[test]
    fn builder_text_decorations_require_positive_thickness_and_alpha() {
        let bounds = rect(0.0, 0.0, 10.0, 2.0);
        for thickness in [0.0, -1.0, f32::NAN, f32::NEG_INFINITY] {
            let mut b = DisplayListBuilder::new();
            b.push_underline(bounds, opaque(), thickness);
            b.push_strikethrough(bounds, opaque(), thickness);
            b.push_overline(bounds, opaque(), thickness);
            assert!(b.items.is_empty(), "thickness {thickness} must not paint");
        }

        // +inf is > 0.0, so it *is* pushed (defined, no panic).
        let mut b = DisplayListBuilder::new();
        b.push_underline(bounds, opaque(), f32::INFINITY);
        assert_eq!(b.items.len(), 1);

        // Transparent decorations are skipped regardless of thickness.
        let mut b = DisplayListBuilder::new();
        b.push_overline(bounds, ColorU::TRANSPARENT, 3.0);
        assert!(b.items.is_empty());
    }

    #[test]
    fn builder_text_run_skips_empty_glyphs_and_transparent_color() {
        let clip = rect(0.0, 0.0, 100.0, 20.0);
        let mut b = DisplayListBuilder::new();
        b.push_text_run(
            Vec::new(),
            FontHash::invalid(),
            16.0,
            opaque(),
            clip,
            Some(0),
            None,
        );
        b.push_text_run(
            vec![glyph(1, 0.0, 0.0)],
            FontHash::invalid(),
            16.0,
            ColorU::TRANSPARENT,
            clip,
            Some(0),
            None,
        );
        assert!(b.items.is_empty());

        // NaN / huge font sizes are pass-through values, not a panic.
        b.push_text_run(
            vec![glyph(1, 0.0, 0.0)],
            FontHash::invalid(),
            f32::NAN,
            opaque(),
            clip,
            None,
            None,
        );
        b.push_text_run(
            vec![glyph(2, 0.0, 0.0)],
            FontHash::invalid(),
            f32::MAX,
            opaque(),
            clip,
            None,
            None,
        );
        assert_eq!(b.items.len(), 2);
    }

    #[test]
    fn builder_border_requires_a_positive_width_and_a_drawing_style() {
        let bounds = rect(0.0, 0.0, 10.0, 10.0);

        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            no_widths(),
            no_colors(),
            no_styles(),
            zero_style_radius(),
        );
        assert!(
            b.items.is_empty(),
            "no widths + no styles => nothing to draw"
        );

        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            all_widths(),
            no_colors(),
            no_styles(),
            zero_style_radius(),
        );
        assert!(b.items.is_empty(), "width without style => nothing to draw");

        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            no_widths(),
            no_colors(),
            all_solid_styles(),
            zero_style_radius(),
        );
        assert!(b.items.is_empty(), "style without width => nothing to draw");

        // THE CLASS: the compact cache's answer for an UNSET border — every
        // side `Some(0px)` with style `Some(none)` — must not become an item,
        // or every normal-state node carries a phantom Border that its
        // hovered/focused self (slow path: `None`) does not.
        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            zero_widths(),
            no_colors(),
            all_styles(),
            zero_style_radius(),
        );
        assert!(
            b.items.is_empty(),
            "0px + style none (the unset encoding) => nothing to draw"
        );

        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            all_widths(),
            no_colors(),
            all_styles(),
            zero_style_radius(),
        );
        assert!(
            b.items.is_empty(),
            "3px + style none paints nothing (CSS: none zeroes the width)"
        );

        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            zero_widths(),
            no_colors(),
            all_solid_styles(),
            zero_style_radius(),
        );
        assert!(b.items.is_empty(), "0px solid => nothing to draw");

        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            all_widths(),
            no_colors(),
            all_solid_styles(),
            zero_style_radius(),
        );
        assert_eq!(b.items.len(), 1, "3px solid paints");

        // One painting side is enough.
        let mut one_side = zero_widths();
        one_side.left = Some(CssPropertyValue::Exact(LayoutBorderLeftWidth {
            inner: PixelValue::px(1.0),
        }));
        let mut b = DisplayListBuilder::new();
        b.push_border(
            bounds,
            one_side,
            no_colors(),
            all_solid_styles(),
            zero_style_radius(),
        );
        assert_eq!(b.items.len(), 1, "a single 1px solid side paints");
    }

    #[test]
    fn builder_forced_page_breaks_are_deduped_and_sorted() {
        let mut b = DisplayListBuilder::new();
        b.add_forced_page_break(300.0, None);
        b.add_forced_page_break(100.0, None);
        b.add_forced_page_break(300.0, None); // exact duplicate
        b.add_forced_page_break(200.0, None);
        b.add_forced_page_break(0.0, None);
        b.add_forced_page_break(-50.0, None); // negative is accepted verbatim
        b.add_forced_page_break(f32::INFINITY, None);
        b.add_forced_page_break(f32::NEG_INFINITY, None);
        assert_eq!(
            b.forced_page_breaks
                .iter()
                .map(|fb| fb.y)
                .collect::<Vec<_>>(),
            vec![
                f32::NEG_INFINITY,
                -50.0,
                0.0,
                100.0,
                200.0,
                300.0,
                f32::INFINITY
            ]
        );
    }

    #[test]
    fn builder_forced_page_break_nan_is_never_deduped() {
        // `contains(&NaN)` is always false (NaN != NaN), so NaN breaks are NOT deduped —
        // repeated calls accumulate. The sort uses partial_cmp().unwrap_or(Equal), so it
        // survives the non-total order rather than panicking on an `unwrap`.
        let mut b = DisplayListBuilder::new();
        b.add_forced_page_break(f32::NAN, None);
        b.add_forced_page_break(f32::NAN, None);
        b.add_forced_page_break(100.0, None);
        assert_eq!(
            b.forced_page_breaks.len(),
            3,
            "a NaN break is never deduped"
        );
        assert_eq!(
            b.forced_page_breaks.iter().filter(|v| v.y.is_nan()).count(),
            2
        );
    }

    #[test]
    fn builder_fixed_position_ranges_need_a_begin_and_at_least_one_item() {
        // end without begin: no-op, no panic.
        let mut b = DisplayListBuilder::new();
        b.end_fixed_position_element();
        assert!(b.fixed_position_item_ranges.is_empty());

        // begin + end with zero items in between: nothing recorded (end > start is false).
        let mut b = DisplayListBuilder::new();
        b.begin_fixed_position_element();
        b.end_fixed_position_element();
        assert!(b.fixed_position_item_ranges.is_empty());

        // begin + items + end: the half-open [start, end) range is recorded.
        let mut b = DisplayListBuilder::new();
        b.pop_clip(); // one pre-existing item at index 0
        b.begin_fixed_position_element();
        b.push_rect(rect(0.0, 0.0, 1.0, 1.0), opaque(), BorderRadius::default());
        b.push_rect(rect(0.0, 0.0, 2.0, 2.0), opaque(), BorderRadius::default());
        b.end_fixed_position_element();
        assert_eq!(b.fixed_position_item_ranges, vec![(1, 3)]);

        // A second end() without a matching begin() must not re-record.
        b.end_fixed_position_element();
        assert_eq!(b.fixed_position_item_ranges.len(), 1);
    }

    #[test]
    fn builder_build_with_debug_transfers_messages() {
        let mut b = DisplayListBuilder::with_debug(true);
        b.debug_log("hello".to_string());
        b.push_rect(rect(0.0, 0.0, 1.0, 1.0), opaque(), BorderRadius::default());

        let mut sink = Some(vec![LayoutDebugMessage::info("pre-existing".to_string())]);
        let dl = b.build_with_debug(&mut sink);
        let msgs = sink.expect("sink stays Some");
        assert_eq!(msgs.len(), 2, "messages are appended, not replaced");
        assert_eq!(dl.items.len(), 1);

        // A None sink must swallow the messages rather than panic.
        let mut b = DisplayListBuilder::with_debug(true);
        b.debug_log("dropped".to_string());
        let mut none_sink: Option<Vec<LayoutDebugMessage>> = None;
        let dl = b.build_with_debug(&mut none_sink);
        assert!(none_sink.is_none());
        assert!(dl.items.is_empty());
    }

    #[test]
    fn builder_stack_pushes_accept_extreme_arguments() {
        let mut b = DisplayListBuilder::new();
        // i32 extremes for z-index, degenerate bounds, and an all-NaN clip.
        b.push_stacking_context(i32::MIN, rect(f32::NAN, f32::NAN, f32::NAN, f32::NAN));
        b.push_stacking_context(i32::MAX, rect(0.0, 0.0, -1.0, -1.0));
        b.pop_stacking_context();
        b.push_clip(
            rect(f32::MIN, f32::MIN, f32::MAX, f32::MAX),
            BorderRadius {
                top_left: f32::INFINITY,
                ..BorderRadius::default()
            },
        );
        b.pop_clip();
        b.push_scroll_frame(
            LogicalRect::zero(),
            LogicalSize::new(f32::MAX, f32::MAX),
            u64::MAX,
        );
        b.pop_scroll_frame();
        b.push_image_mask_clip(
            LogicalRect::zero(),
            test_image(),
            rect(0.0, 0.0, -5.0, -5.0),
        );
        b.pop_image_mask_clip();
        b.push_reference_frame(
            TransformKey::unique(),
            ComputedTransform3D::IDENTITY,
            LogicalRect::zero(),
        );
        b.pop_reference_frame();
        b.push_virtual_view_placeholder(NodeId::ZERO, LogicalRect::zero(), LogicalRect::zero());
        b.push_hit_test_area(rect(0.0, 0.0, 1.0, 1.0), (u64::MAX, TAG_TYPE_CURSOR));
        b.push_image(LogicalRect::zero(), test_image(), BorderRadius::default());
        b.push_linear_gradient(
            LogicalRect::zero(),
            LinearGradient::default(),
            BorderRadius::default(),
        );
        b.push_radial_gradient(
            LogicalRect::zero(),
            RadialGradient::default(),
            BorderRadius::default(),
        );
        b.push_conic_gradient(
            LogicalRect::zero(),
            ConicGradient::default(),
            BorderRadius::default(),
        );

        let dl = b.build();
        // 17 until push_item started dropping items at the unassigned-position
        // sentinel. Two pushes above use NaN / f32::MIN ORIGINS — the NaN
        // stacking context and the f32::MIN clip — and those are exactly what
        // the drop exists to remove, so they no longer reach the list. The
        // point of this test is that extreme arguments do not PANIC and do not
        // desync the node mapping; both still hold.
        assert_eq!(
            dl.items.len(),
            15,
            "expected the NaN-origin stacking context and the f32::MIN-origin \
             clip to be dropped as unassigned positions; everything else is \
             real geometry and must survive"
        );
        assert_eq!(dl.items.len(), dl.node_mapping.len());
        // The mapping must stay in lockstep — dropping an item without dropping
        // its node entry would misattribute every later item to the wrong node.
        assert!(
            !dl.items.is_empty(),
            "the drop must not swallow well-formed items"
        );
    }

    // ---------------------------------------------------------------------
    // Free geometry helpers: rect_intersects / clip_rect_bounds / offset_rect_y
    // ---------------------------------------------------------------------

    #[test]
    fn rect_intersects_uses_a_half_open_page_interval() {
        let page = (100.0f32, 200.0f32);
        // Fully inside.
        assert!(rect_intersects(
            &rect(0.0, 120.0, 10.0, 10.0),
            page.0,
            page.1
        ));
        // Straddling both edges.
        assert!(rect_intersects(
            &rect(0.0, 50.0, 10.0, 300.0),
            page.0,
            page.1
        ));
        // Entirely above / below.
        assert!(!rect_intersects(
            &rect(0.0, 0.0, 10.0, 10.0),
            page.0,
            page.1
        ));
        assert!(!rect_intersects(
            &rect(0.0, 500.0, 10.0, 10.0),
            page.0,
            page.1
        ));
        // Touching exactly: bottom edge == page_top is NOT an intersection...
        assert!(!rect_intersects(
            &rect(0.0, 90.0, 10.0, 10.0),
            page.0,
            page.1
        ));
        // ...and top edge == page_bottom is NOT either.
        assert!(!rect_intersects(
            &rect(0.0, 200.0, 10.0, 10.0),
            page.0,
            page.1
        ));
        // A zero-height rect strictly inside DOES intersect (its single edge is in range).
        assert!(rect_intersects(
            &rect(0.0, 150.0, 10.0, 0.0),
            page.0,
            page.1
        ));
        // ...but a zero-height rect sitting exactly on either page edge does not.
        assert!(!rect_intersects(
            &rect(0.0, 100.0, 10.0, 0.0),
            page.0,
            page.1
        ));
        assert!(!rect_intersects(
            &rect(0.0, 200.0, 10.0, 0.0),
            page.0,
            page.1
        ));
    }

    #[test]
    fn rect_intersects_with_nan_is_false_not_a_panic() {
        assert!(!rect_intersects(
            &rect(0.0, f32::NAN, 10.0, 10.0),
            0.0,
            100.0
        ));
        assert!(!rect_intersects(
            &rect(0.0, 10.0, 10.0, f32::NAN),
            0.0,
            100.0
        ));
        assert!(!rect_intersects(
            &rect(0.0, 10.0, 10.0, 10.0),
            f32::NAN,
            f32::NAN
        ));
        // Infinite page bounds cover everything finite.
        assert!(rect_intersects(
            &rect(0.0, 10.0, 10.0, 10.0),
            f32::NEG_INFINITY,
            f32::INFINITY
        ));
    }

    #[test]
    fn clip_rect_bounds_clips_and_rebases_to_page_relative_coords() {
        // Item straddles the top of page [100, 200): keep the visible slice, rebase to y=0.
        let clipped = clip_rect_bounds(rect(5.0, 50.0, 20.0, 100.0), 100.0, 200.0).unwrap();
        assert_eq!(clipped, rect(5.0, 0.0, 20.0, 50.0));

        // Item straddles the bottom: kept slice starts at its own offset into the page.
        let clipped = clip_rect_bounds(rect(5.0, 180.0, 20.0, 100.0), 100.0, 200.0).unwrap();
        assert_eq!(clipped, rect(5.0, 80.0, 20.0, 20.0));

        // Item strictly inside: only rebased, never resized.
        let clipped = clip_rect_bounds(rect(5.0, 120.0, 20.0, 30.0), 100.0, 200.0).unwrap();
        assert_eq!(clipped, rect(5.0, 20.0, 20.0, 30.0));

        // Item larger than the page on both sides: clamped to exactly the page height.
        let clipped = clip_rect_bounds(rect(0.0, 0.0, 20.0, 10_000.0), 100.0, 200.0).unwrap();
        assert_eq!(clipped, rect(0.0, 0.0, 20.0, 100.0));
    }

    #[test]
    fn clip_rect_bounds_rejects_off_page_and_edge_touching_rects() {
        assert_eq!(
            clip_rect_bounds(rect(0.0, 0.0, 10.0, 10.0), 100.0, 200.0),
            None
        );
        assert_eq!(
            clip_rect_bounds(rect(0.0, 300.0, 10.0, 10.0), 100.0, 200.0),
            None
        );
        // bottom == page_top -> outside (half-open interval).
        assert_eq!(
            clip_rect_bounds(rect(0.0, 90.0, 10.0, 10.0), 100.0, 200.0),
            None
        );
        // top == page_bottom -> outside.
        assert_eq!(
            clip_rect_bounds(rect(0.0, 200.0, 10.0, 10.0), 100.0, 200.0),
            None
        );
    }

    #[test]
    fn clip_rect_bounds_zero_height_rects() {
        // A zero-height rect sitting exactly on page_top is rejected (bottom <= top).
        assert_eq!(
            clip_rect_bounds(rect(0.0, 100.0, 10.0, 0.0), 100.0, 200.0),
            None
        );
        // A zero-height rect strictly inside survives as a zero-height slice.
        assert_eq!(
            clip_rect_bounds(rect(0.0, 150.0, 10.0, 0.0), 100.0, 200.0),
            Some(rect(0.0, 50.0, 10.0, 0.0))
        );
    }

    #[test]
    fn clip_rect_bounds_with_an_inverted_page_produces_a_degenerate_rect_not_a_panic() {
        // page_top > page_bottom is never produced by calculate_page_break_positions,
        // but nothing rejects it here: the height goes NEGATIVE. Pinned so the missing
        // guard is visible rather than silently feeding a negative rect downstream.
        let out = clip_rect_bounds(rect(0.0, 0.0, 10.0, 500.0), 200.0, 100.0)
            .expect("an inverted page is not rejected");
        assert!(
            out.size.height < 0.0,
            "no clamp: an inverted page yields a negative height"
        );
    }

    #[test]
    fn clip_rect_bounds_with_extreme_pages_does_not_panic() {
        // An unbounded page keeps the item intact (no clipping).
        let full = clip_rect_bounds(
            rect(0.0, 10.0, 10.0, 10.0),
            f32::NEG_INFINITY,
            f32::INFINITY,
        )
        .expect("an infinite page contains everything");
        assert_eq!(full.size, LogicalSize::new(10.0, 10.0));

        // NaN page bounds: every comparison is false, so the rect is kept. f32::max/min
        // drop the NaN operand, so the SIZE stays clean and only the rebased origin goes
        // NaN. Defined, total, and crucially NOT a panic.
        let nan_page = clip_rect_bounds(rect(0.0, 10.0, 10.0, 10.0), f32::NAN, f32::NAN)
            .expect("NaN bounds fail both rejection tests");
        assert!(
            nan_page.origin.y.is_nan(),
            "the page-relative rebase propagates NaN"
        );
        assert_eq!(
            nan_page.size.height, 10.0,
            "min/max ignore NaN, so the height survives"
        );

        // f32::MAX height must not overflow into a panic.
        assert_eq!(
            clip_rect_bounds(rect(0.0, 0.0, 10.0, f32::MAX), 0.0, 100.0),
            Some(rect(0.0, 0.0, 10.0, 100.0))
        );
    }

    #[test]
    fn offset_rect_y_only_moves_y_and_preserves_size() {
        assert_eq!(
            offset_rect_y(rect(1.0, 2.0, 3.0, 4.0), 0.0),
            rect(1.0, 2.0, 3.0, 4.0)
        );
        assert_eq!(
            offset_rect_y(rect(1.0, 2.0, 3.0, 4.0), 10.0),
            rect(1.0, 12.0, 3.0, 4.0)
        );
        assert_eq!(
            offset_rect_y(rect(1.0, 2.0, 3.0, 4.0), -10.0),
            rect(1.0, -8.0, 3.0, 4.0)
        );

        // Non-finite offsets are propagated, never trapped.
        assert!(offset_rect_y(rect(0.0, 0.0, 1.0, 1.0), f32::INFINITY)
            .origin
            .y
            .is_infinite());
        assert!(offset_rect_y(rect(0.0, 0.0, 1.0, 1.0), f32::NAN)
            .origin
            .y
            .is_nan());
        // MAX + MAX saturates to +inf under IEEE-754, not a wrap.
        assert!(offset_rect_y(rect(0.0, f32::MAX, 1.0, 1.0), f32::MAX)
            .origin
            .y
            .is_infinite());
        // The size is untouched in every case.
        assert_eq!(
            offset_rect_y(rect(0.0, 0.0, 3.0, 4.0), f32::NAN).size,
            LogicalSize::new(3.0, 4.0)
        );
    }

    // ---------------------------------------------------------------------
    // Per-item page clipping
    // ---------------------------------------------------------------------

    #[test]
    fn clip_rect_item_drops_off_page_and_rebases_on_page() {
        assert!(clip_rect_item(
            rect(0.0, 0.0, 10.0, 10.0),
            opaque(),
            BorderRadius::default(),
            100.0,
            200.0
        )
        .is_none());

        let item = clip_rect_item(
            rect(0.0, 150.0, 10.0, 100.0),
            opaque(),
            BorderRadius::default(),
            100.0,
            200.0,
        )
        .expect("overlaps the page");
        match item {
            DisplayListItem::Rect { bounds, color, .. } => {
                assert_eq!(bounds.into_inner(), rect(0.0, 50.0, 10.0, 50.0));
                assert_eq!(color, opaque());
            }
            other => panic!("expected Rect, got {other:?}"),
        }
    }

    #[test]
    fn clip_cursor_selection_hittest_and_scrollbar_items_share_the_page_test() {
        let off = rect(0.0, 0.0, 10.0, 10.0);
        let on = rect(0.0, 120.0, 10.0, 10.0);
        let (top, bottom) = (100.0, 200.0);

        assert!(clip_cursor_rect_item(off, opaque(), top, bottom).is_none());
        assert!(clip_cursor_rect_item(on, opaque(), top, bottom).is_some());

        assert!(
            clip_selection_rect_item(off, BorderRadius::default(), opaque(), top, bottom).is_none()
        );
        assert!(
            clip_selection_rect_item(on, BorderRadius::default(), opaque(), top, bottom).is_some()
        );

        assert!(clip_hit_test_area_item(off, (1, TAG_TYPE_DOM_NODE), top, bottom).is_none());
        assert!(clip_hit_test_area_item(on, (1, TAG_TYPE_DOM_NODE), top, bottom).is_some());

        assert!(clip_scrollbar_item(
            off,
            opaque(),
            ScrollbarOrientation::Vertical,
            None,
            None,
            top,
            bottom
        )
        .is_none());
        assert!(clip_scrollbar_item(
            on,
            opaque(),
            ScrollbarOrientation::Vertical,
            None,
            None,
            top,
            bottom
        )
        .is_some());

        assert!(clip_image_item(off, test_image(), BorderRadius::default(), top, bottom).is_none());
        assert!(clip_image_item(on, test_image(), BorderRadius::default(), top, bottom).is_some());

        let no_shift = LogicalPosition::zero();
        assert!(clip_virtual_view_item(DomId::ROOT_ID, off, off, no_shift, top, bottom).is_none());
        assert!(clip_virtual_view_item(DomId::ROOT_ID, on, on, no_shift, top, bottom).is_some());
    }

    #[test]
    fn clip_text_decoration_item_preserves_the_decoration_kind() {
        let on = rect(0.0, 120.0, 10.0, 2.0);
        let (top, bottom) = (100.0, 200.0);

        assert!(matches!(
            clip_text_decoration_item(
                on,
                opaque(),
                1.0,
                TextDecorationType::Underline,
                top,
                bottom
            ),
            Some(DisplayListItem::Underline { .. })
        ));
        assert!(matches!(
            clip_text_decoration_item(
                on,
                opaque(),
                1.0,
                TextDecorationType::Strikethrough,
                top,
                bottom
            ),
            Some(DisplayListItem::Strikethrough { .. })
        ));
        assert!(matches!(
            clip_text_decoration_item(on, opaque(), 1.0, TextDecorationType::Overline, top, bottom),
            Some(DisplayListItem::Overline { .. })
        ));
        // Off-page decorations are dropped even with a NaN thickness.
        assert!(clip_text_decoration_item(
            rect(0.0, 0.0, 10.0, 2.0),
            opaque(),
            f32::NAN,
            TextDecorationType::Underline,
            top,
            bottom
        )
        .is_none());
    }

    #[test]
    fn clip_text_item_filters_glyphs_by_baseline_into_a_half_open_page() {
        let clip = rect(0.0, 90.0, 200.0, 120.0);
        let glyphs = vec![
            glyph(1, 0.0, 99.0),   // above page  -> dropped
            glyph(2, 8.0, 100.0),  // exactly page_top -> KEPT (>= top)
            glyph(3, 16.0, 150.0), // inside -> kept
            glyph(4, 24.0, 200.0), // exactly page_bottom -> dropped (< bottom)
            glyph(5, 32.0, 250.0), // below -> dropped
        ];

        let item = clip_text_item(
            &glyphs,
            FontHash::from_hash(9),
            16.0,
            opaque(),
            clip,
            100.0,
            200.0,
        )
        .expect("some glyphs are on the page");
        match item {
            DisplayListItem::Text {
                glyphs: kept,
                clip_rect,
                source_node_index,
                ..
            } => {
                assert_eq!(kept.iter().map(|g| g.index).collect::<Vec<_>>(), vec![2, 3]);
                // Kept glyphs are rebased to page-relative Y.
                assert_eq!(kept[0].point.y, 0.0);
                assert_eq!(kept[1].point.y, 50.0);
                // X is never touched.
                assert_eq!(kept[0].point.x, 8.0);
                assert_eq!(clip_rect.into_inner().origin.y, -10.0);
                assert_eq!(
                    source_node_index, None,
                    "the paginated copy loses its source node"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn clip_text_item_returns_none_when_nothing_survives() {
        let clip = rect(0.0, 90.0, 200.0, 120.0);
        // The clip rect overlaps the page but no glyph baseline does.
        let outside = vec![glyph(1, 0.0, 95.0), glyph(2, 8.0, 400.0)];
        assert!(clip_text_item(
            &outside,
            FontHash::from_hash(1),
            16.0,
            opaque(),
            clip,
            100.0,
            200.0
        )
        .is_none());

        // An empty glyph run is dropped too.
        assert!(clip_text_item(
            &[],
            FontHash::from_hash(1),
            16.0,
            opaque(),
            clip,
            100.0,
            200.0
        )
        .is_none());

        // A clip rect entirely off the page short-circuits before glyph filtering.
        assert!(clip_text_item(
            &[glyph(1, 0.0, 150.0)],
            FontHash::from_hash(1),
            16.0,
            opaque(),
            rect(0.0, 0.0, 10.0, 10.0),
            100.0,
            200.0
        )
        .is_none());

        // NaN glyph baselines never satisfy either bound -> filtered out -> None.
        let nan_glyphs = vec![glyph(1, 0.0, f32::NAN)];
        assert!(clip_text_item(
            &nan_glyphs,
            FontHash::from_hash(1),
            16.0,
            opaque(),
            clip,
            100.0,
            200.0
        )
        .is_none());
    }

    #[test]
    fn clip_border_item_hides_the_bottom_border_when_clipped_at_the_bottom() {
        let (top, bottom) = (100.0f32, 200.0f32);
        // Spans past the bottom of the page.
        let original = rect(0.0, 50.0, 100.0, 200.0);
        let item = clip_border_item(
            original,
            all_widths(),
            no_colors(),
            all_styles(),
            zero_style_radius(),
            top,
            bottom,
        )
        .expect("overlaps the page");
        match item {
            DisplayListItem::Border { widths, .. } => {
                assert!(
                    widths.bottom.is_none(),
                    "a border cut by the page edge must not draw its bottom rule"
                );
                assert!(
                    widths.left.is_some() && widths.right.is_some(),
                    "side borders survive"
                );
            }
            other => panic!("expected Border, got {other:?}"),
        }
    }

    #[test]
    fn adjust_border_widths_keeps_every_side_for_a_fully_contained_border() {
        let (top, bottom) = (100.0f32, 200.0f32);
        let original = rect(0.0, 110.0, 100.0, 50.0); // strictly inside the page
        let clipped = clip_rect_bounds(original, top, bottom).unwrap();
        assert_eq!(
            clipped,
            rect(0.0, 10.0, 100.0, 50.0),
            "unclipped => only rebased"
        );

        let widths =
            adjust_border_widths_for_clipping(all_widths(), original, clipped, top, bottom);
        assert!(widths.top.is_some());
        assert!(widths.bottom.is_some());
        assert!(widths.left.is_some());
        assert!(widths.right.is_some());
    }

    #[test]
    fn adjust_border_widths_with_nan_page_bounds_does_not_panic() {
        let original = rect(0.0, 0.0, 10.0, 10.0);
        let clipped = rect(0.0, 0.0, 10.0, 10.0);
        let w =
            adjust_border_widths_for_clipping(all_widths(), original, clipped, f32::NAN, f32::NAN);
        // Every NaN comparison is false, so nothing is hidden — defined and total.
        assert!(w.top.is_some() && w.bottom.is_some());
    }

    #[test]
    fn clip_and_offset_display_item_drops_state_management_commands() {
        // Pagination cannot re-derive a clip/scroll/stacking stack per page, so those
        // items are deliberately dropped. Pin it so a change is visible.
        for item in [
            DisplayListItem::PushClip {
                bounds: rect(0.0, 120.0, 10.0, 10.0).into(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PopClip,
            DisplayListItem::PushScrollFrame {
                clip_bounds: rect(0.0, 120.0, 10.0, 10.0).into(),
                content_size: LogicalSize::new(10.0, 10.0),
                scroll_id: 1,
            },
            DisplayListItem::PopScrollFrame,
            DisplayListItem::PushStackingContext {
                z_index: 0,
                bounds: rect(0.0, 120.0, 10.0, 10.0).into(),
            },
            DisplayListItem::PopStackingContext,
            DisplayListItem::PopOpacity,
            DisplayListItem::PopTextShadow,
            DisplayListItem::VirtualViewPlaceholder {
                node_id: NodeId::ZERO,
                bounds: rect(0.0, 120.0, 10.0, 10.0).into(),
                clip_rect: rect(0.0, 120.0, 10.0, 10.0).into(),
            },
        ] {
            assert!(
                clip_and_offset_display_item(&item, 100.0, 200.0).is_none(),
                "{item:?} must be dropped by the paginator"
            );
        }
    }

    #[test]
    fn clip_and_offset_display_item_dispatches_drawing_items() {
        let on_page = DisplayListItem::Rect {
            bounds: rect(0.0, 150.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        let clipped = clip_and_offset_display_item(&on_page, 100.0, 200.0).expect("on page");
        assert_eq!(clipped.bounds(), Some(rect(0.0, 50.0, 10.0, 10.0)));

        let off_page = DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert!(clip_and_offset_display_item(&off_page, 100.0, 200.0).is_none());

        // Gradients use a plain bounds test (no sub-item filtering).
        let grad = DisplayListItem::LinearGradient {
            bounds: rect(0.0, 150.0, 10.0, 10.0).into(),
            gradient: LinearGradient::default(),
            border_radius: BorderRadius::default(),
        };
        assert!(clip_and_offset_display_item(&grad, 100.0, 200.0).is_some());
        assert!(clip_and_offset_display_item(&grad, 1000.0, 2000.0).is_none());
    }

    // ---------------------------------------------------------------------
    // get_display_item_bounds / offset_display_item_y / heights
    // ---------------------------------------------------------------------

    #[test]
    fn get_display_item_bounds_mirrors_item_bounds() {
        let r = rect(1.0, 2.0, 3.0, 4.0);
        let item = DisplayListItem::Rect {
            bounds: r.into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert_eq!(
            get_display_item_bounds(&item),
            Some(WindowLogicalRect::from(r))
        );
        assert_eq!(get_display_item_bounds(&DisplayListItem::PopClip), None);
    }

    #[test]
    fn offset_display_item_y_zero_offset_is_a_pure_clone() {
        let item = text_item(
            Some(3),
            rect(0.0, 10.0, 100.0, 20.0),
            vec![glyph(1, 5.0, 15.0)],
        );
        let same = offset_display_item_y(&item, 0.0);
        assert!(item.is_visually_equal(&same));
        assert_eq!(same.bounds(), item.bounds());
    }

    #[test]
    fn offset_display_item_y_moves_glyphs_and_clip_together() {
        let item = text_item(
            Some(3),
            rect(0.0, 10.0, 100.0, 20.0),
            vec![glyph(1, 5.0, 15.0), glyph(2, 13.0, 15.0)],
        );
        match offset_display_item_y(&item, -10.0) {
            DisplayListItem::Text {
                glyphs, clip_rect, ..
            } => {
                assert_eq!(clip_rect.into_inner(), rect(0.0, 0.0, 100.0, 20.0));
                assert_eq!(glyphs[0].point.y, 5.0);
                assert_eq!(glyphs[1].point.y, 5.0);
                assert_eq!(glyphs[0].point.x, 5.0, "X must not move");
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn offset_display_item_y_with_nonfinite_offsets_does_not_panic() {
        let item = DisplayListItem::Rect {
            bounds: rect(0.0, 10.0, 5.0, 5.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        };
        assert!(offset_display_item_y(&item, f32::NAN)
            .bounds()
            .unwrap()
            .origin
            .y
            .is_nan());
        assert!(offset_display_item_y(&item, f32::INFINITY)
            .bounds()
            .unwrap()
            .origin
            .y
            .is_infinite());
        assert!(offset_display_item_y(&item, f32::MIN)
            .bounds()
            .unwrap()
            .origin
            .y
            .is_finite());
    }

    #[test]
    fn calculate_display_list_height_ignores_hairline_items() {
        assert_eq!(calculate_display_list_height(&DisplayList::default()), 0.0);

        // Items thinner than 0.1px do not contribute (they are not "visible content").
        let dl = list_of(vec![
            DisplayListItem::Rect {
                bounds: rect(0.0, 5000.0, 10.0, 0.05).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::Rect {
                bounds: rect(0.0, 10.0, 10.0, 40.0).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
        ]);
        assert_eq!(calculate_display_list_height(&dl), 50.0);
    }

    #[test]
    fn calculate_display_list_height_never_returns_negative_or_nan() {
        // Negative and NaN geometry must floor at 0.0 rather than poison the height
        // (a NaN height would panic the paginator's `partial_cmp().unwrap()` sort).
        let dl = list_of(vec![
            DisplayListItem::Rect {
                bounds: rect(0.0, -500.0, 10.0, 100.0).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::Rect {
                bounds: rect(0.0, f32::NAN, 10.0, f32::NAN).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::PopClip, // no bounds at all
        ]);
        let h = calculate_display_list_height(&dl);
        assert!(
            !h.is_nan(),
            "a NaN total height would panic the page-break sort"
        );
        assert_eq!(h, 0.0);
    }

    // ---------------------------------------------------------------------
    // get_scroll_id
    // ---------------------------------------------------------------------

    #[test]
    fn get_scroll_id_maps_node_index_and_collides_none_with_node_zero() {
        assert_eq!(get_scroll_id(None), 0);
        assert_eq!(get_scroll_id(Some(NodeId::new(5))), 5);
        assert_eq!(
            get_scroll_id(Some(NodeId::new(usize::MAX))),
            usize::MAX as u64
        );
        // NOTE: the "no node" sentinel and the root node both map to 0.
        assert_eq!(get_scroll_id(Some(NodeId::ZERO)), get_scroll_id(None));
    }

    // ---------------------------------------------------------------------
    // SlicerConfig
    // ---------------------------------------------------------------------

    #[test]
    fn slicer_config_builders_set_the_fields_they_name() {
        let simple = SlicerConfig::simple(800.0);
        assert_eq!(simple.page_content_height, 800.0);
        assert_eq!(simple.page_gap, 0.0);
        assert!(simple.allow_clipping);
        assert_eq!(simple.page_width, DEFAULT_A4_WIDTH_PT);
        assert_eq!(simple.page_slot_height(), 800.0);

        let gapped = SlicerConfig::with_gap(800.0, 40.0);
        assert_eq!(gapped.page_gap, 40.0);
        assert_eq!(gapped.page_slot_height(), 840.0);

        let wide = SlicerConfig::simple(800.0).with_page_width(1000.0);
        assert_eq!(wide.page_width, 1000.0);
        assert_eq!(
            wide.page_content_height, 800.0,
            "with_page_width must not disturb the height"
        );
    }

    #[test]
    fn slicer_config_builders_accept_extreme_values() {
        for h in [0.0, -100.0, f32::MAX, f32::INFINITY, f32::NAN] {
            let c = SlicerConfig::simple(h);
            assert_eq!(c.page_content_height.to_bits(), h.to_bits());
            let _ = c.page_slot_height();
            let _ = c.page_for_y(10.0);
            let _ = c.page_bounds(0);
        }
        // NaN gap propagates into the slot height without panicking.
        assert!(SlicerConfig::with_gap(100.0, f32::NAN)
            .page_slot_height()
            .is_nan());
    }

    #[test]
    fn page_for_y_basic_and_boundary() {
        let c = SlicerConfig::simple(100.0);
        assert_eq!(c.page_for_y(0.0), 0);
        assert_eq!(c.page_for_y(99.999), 0);
        assert_eq!(
            c.page_for_y(100.0),
            1,
            "the page boundary belongs to the next page"
        );
        assert_eq!(c.page_for_y(250.0), 2);

        // The gap counts towards the slot: page 1 starts at 120, not 100.
        let g = SlicerConfig::with_gap(100.0, 20.0);
        assert_eq!(g.page_for_y(119.0), 0);
        assert_eq!(g.page_for_y(120.0), 1);
    }

    #[test]
    fn page_for_y_saturates_instead_of_wrapping_or_trapping() {
        let c = SlicerConfig::simple(100.0);
        // Negative Y floors to a negative page; the f32->usize cast SATURATES to 0
        // (Rust >= 1.45), it does not wrap to usize::MAX.
        assert_eq!(c.page_for_y(-1.0), 0);
        assert_eq!(c.page_for_y(-1e30), 0);
        assert_eq!(c.page_for_y(f32::NEG_INFINITY), 0);
        // NaN saturates to 0 as well.
        assert_eq!(c.page_for_y(f32::NAN), 0);
        // Enormous Y saturates to usize::MAX rather than overflowing.
        assert_eq!(c.page_for_y(f32::INFINITY), usize::MAX);
        assert_eq!(c.page_for_y(f32::MAX), usize::MAX);
    }

    #[test]
    fn page_for_y_with_a_nonpositive_slot_is_always_page_zero() {
        // Guard against a division by zero / infinite page index.
        assert_eq!(SlicerConfig::default().page_for_y(f32::MAX), 0);
        assert_eq!(SlicerConfig::simple(0.0).page_for_y(500.0), 0);
        assert_eq!(
            SlicerConfig::with_gap(100.0, -100.0).page_for_y(500.0),
            0,
            "slot == 0"
        );
        assert_eq!(
            SlicerConfig::with_gap(100.0, -500.0).page_for_y(500.0),
            0,
            "slot < 0"
        );
        // A NaN slot is not > 0.0, but it also fails the `<= 0.0` guard; the cast still
        // saturates NaN to 0 rather than trapping.
        assert_eq!(SlicerConfig::simple(f32::NAN).page_for_y(500.0), 0);
    }

    #[test]
    fn page_bounds_are_contiguous_without_a_gap_and_spaced_with_one() {
        let c = SlicerConfig::simple(100.0);
        assert_eq!(c.page_bounds(0), (0.0, 100.0));
        assert_eq!(c.page_bounds(1), (100.0, 200.0));
        assert_eq!(c.page_bounds(3), (300.0, 400.0));

        let g = SlicerConfig::with_gap(100.0, 20.0);
        assert_eq!(g.page_bounds(0), (0.0, 100.0));
        assert_eq!(
            g.page_bounds(2),
            (240.0, 340.0),
            "the gap is dead space between pages"
        );

        // page_for_y and page_bounds must agree.
        for page in 0..5usize {
            let (start, _end) = g.page_bounds(page);
            assert_eq!(g.page_for_y(start), page);
        }
    }

    #[test]
    fn page_bounds_at_extreme_indices_do_not_panic() {
        let c = SlicerConfig::simple(100.0);
        let (start, end) = c.page_bounds(usize::MAX);
        assert!(
            start.is_finite() && end.is_finite(),
            "usize::MAX as f32 * 100 stays in f32 range"
        );
        assert!(start > 0.0);

        // A zero-height config collapses every page onto (0, 0).
        assert_eq!(SlicerConfig::default().page_bounds(9999), (0.0, 0.0));
    }

    // ---------------------------------------------------------------------
    // calculate_page_break_positions
    // ---------------------------------------------------------------------

    #[test]
    fn page_breaks_for_an_empty_or_zero_height_list_is_a_single_page() {
        let pages = calculate_page_break_positions(&DisplayList::default(), 100.0, 100.0);
        assert_eq!(
            pages,
            vec![(0.0, 100.0)],
            "an empty document still has one page"
        );

        // first_page_height <= 0 short-circuits to a single page too.
        let dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        assert_eq!(
            calculate_page_break_positions(&dl, 0.0, 100.0),
            vec![(0.0, 250.0)]
        );
        assert_eq!(
            calculate_page_break_positions(&dl, -50.0, 100.0),
            vec![(0.0, 250.0)]
        );
    }

    #[test]
    fn page_breaks_split_at_regular_intervals() {
        let dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        let pages = calculate_page_break_positions(&dl, 100.0, 100.0);
        assert_eq!(pages, vec![(0.0, 100.0), (100.0, 200.0), (200.0, 250.0)]);

        // Pages must tile the document with no gaps and no overlaps.
        for w in pages.windows(2) {
            assert_eq!(w[0].1, w[1].0);
        }
        assert_eq!(pages.first().unwrap().0, 0.0);
        assert_eq!(pages.last().unwrap().1, 250.0);
    }

    #[test]
    fn page_breaks_honour_forced_breaks_and_merge_near_duplicates() {
        let mut dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        dl.forced_page_breaks = vec![ForcedBreak {
            y: 50.0,
            causing_node: None,
        }];
        assert_eq!(
            calculate_page_break_positions(&dl, 100.0, 100.0),
            vec![(0.0, 50.0), (50.0, 100.0), (100.0, 200.0), (200.0, 250.0)]
        );

        // A forced break within 1px of a regular break is merged, not duplicated
        // (a duplicate would emit a zero-height page) — and the FORCED break's
        // position wins the merge (CSS Fragmentation: forced breaks always
        // apply; before the page_breaks extraction the interval break at 100.0
        // silently swallowed the author's break at 100.5).
        dl.forced_page_breaks = vec![ForcedBreak {
            y: 100.5,
            causing_node: None,
        }];
        let pages = calculate_page_break_positions(&dl, 100.0, 100.0);
        assert_eq!(pages, vec![(0.0, 100.5), (100.5, 200.0), (200.0, 250.0)]);
        assert!(pages.iter().all(|(s, e)| e > s), "no zero-height pages");
    }

    #[test]
    fn page_breaks_ignore_out_of_range_and_nan_forced_breaks() {
        // A NaN forced break is filtered by the range check (NaN fails both
        // comparisons); the sort itself is total_cmp since the page_breaks
        // extraction, so even an unfiltered NaN could no longer panic.
        let mut dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        dl.forced_page_breaks = [f32::NAN, -10.0, 0.0, 250.0, 9999.0, f32::INFINITY]
            .into_iter()
            .map(|y| ForcedBreak {
                y,
                causing_node: None,
            })
            .collect();
        let pages = calculate_page_break_positions(&dl, 100.0, 100.0);
        // Only the regular interval breaks survive.
        assert_eq!(pages, vec![(0.0, 100.0), (100.0, 200.0), (200.0, 250.0)]);
    }

    #[test]
    fn page_breaks_with_a_nan_first_page_height_still_yields_one_page() {
        let dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        // `while NaN < total` is immediately false, so no regular breaks are produced
        // and the whole document lands on one page. Defined, and NOT a hang.
        let pages = calculate_page_break_positions(&dl, f32::NAN, 100.0);
        assert_eq!(pages, vec![(0.0, 250.0)]);
    }

    // ---------------------------------------------------------------------
    // paginate_display_list_with_slicer_and_breaks (public entry point)
    // ---------------------------------------------------------------------

    #[test]
    fn paginate_with_a_degenerate_page_height_returns_the_list_unsliced() {
        let rr = RendererResources::default();
        let dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);

        for h in [0.0, -1.0, f32::MAX, f32::INFINITY] {
            let pages = paginate_display_list_with_slicer_and_breaks(
                dl.clone(),
                &SlicerConfig::simple(h),
                &rr,
            )
            .expect("degenerate page height must not error");
            assert_eq!(pages.len(), 1, "page height {h} => no slicing");
            assert_eq!(pages[0].items.len(), 1);
        }
    }

    #[test]
    fn paginate_single_page_matches_the_full_pagination_page_for_page() {
        let rr = RendererResources::default();
        let mut dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        dl.forced_page_breaks = vec![ForcedBreak {
            y: 50.0,
            causing_node: None,
        }];
        let cfg = SlicerConfig::simple(100.0);
        let constraints = page_breaks::PageConstraints::from_slicer_config(&cfg);
        let breaks = page_breaks::compute_page_breaks_from_display_list(&dl, &constraints);

        let all = paginate_display_list_with_breaks(dl.clone(), &cfg, &breaks, &rr)
            .expect("full pagination");
        assert_eq!(all.len(), 4);
        for (idx, expected_page) in all.iter().enumerate() {
            let single =
                paginate_single_page(dl.clone(), &cfg, &breaks, &rr, idx).expect("single page");
            assert_eq!(
                single.items.len(),
                expected_page.items.len(),
                "page {idx}: lazy materialization must produce the same page"
            );
            let bounds = |p: &DisplayList| p.items.iter().find_map(DisplayListItem::bounds);
            assert_eq!(
                bounds(&single),
                bounds(expected_page),
                "page {idx} geometry"
            );
        }

        // Out of range: empty, not a panic and not page 0.
        let oob = paginate_single_page(dl, &cfg, &breaks, &rr, 99).expect("oob");
        assert!(oob.items.is_empty());
    }

    #[test]
    fn paginate_slices_content_into_pages() {
        let rr = RendererResources::default();
        let dl = list_of(vec![DisplayListItem::Rect {
            bounds: rect(0.0, 0.0, 10.0, 250.0).into(),
            color: opaque(),
            border_radius: BorderRadius::default(),
        }]);
        let pages =
            paginate_display_list_with_slicer_and_breaks(dl, &SlicerConfig::simple(100.0), &rr)
                .expect("pagination succeeds");
        assert_eq!(pages.len(), 3);
        // The tall rect is clipped onto every page it crosses, always rebased to y=0.
        for page in &pages {
            let r = page
                .items
                .iter()
                .find_map(DisplayListItem::bounds)
                .expect("each page keeps a slice");
            assert_eq!(r.origin.y, 0.0);
            assert!(r.size.height > 0.0 && r.size.height <= 100.0);
        }
    }

    // ---------------------------------------------------------------------
    // DisplayList::patch_text_glyphs
    // ---------------------------------------------------------------------

    #[test]
    fn patch_text_glyphs_replaces_matching_runs_and_returns_their_damage() {
        let clip = rect(10.0, 20.0, 100.0, 30.0);
        let mut dl = list_of(vec![text_item(Some(7), clip, vec![glyph(1, 0.0, 0.0)])]);

        let damage = dl.patch_text_glyphs(7, &[vec![glyph(42, 5.0, 6.0), glyph(43, 13.0, 6.0)]]);
        assert_eq!(damage, Some(clip), "damage covers the run's clip rect");
        match &dl.items[0] {
            DisplayListItem::Text { glyphs, .. } => {
                assert_eq!(
                    glyphs.iter().map(|g| g.index).collect::<Vec<_>>(),
                    vec![42, 43]
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn patch_text_glyphs_unions_damage_across_runs() {
        let a = rect(0.0, 0.0, 50.0, 10.0);
        let b = rect(100.0, 200.0, 50.0, 10.0);
        let mut dl = list_of(vec![
            text_item(Some(3), a, vec![glyph(1, 0.0, 0.0)]),
            text_item(Some(3), b, vec![glyph(2, 0.0, 0.0)]),
        ]);

        let damage = dl
            .patch_text_glyphs(3, &[vec![glyph(9, 0.0, 0.0)], vec![glyph(10, 0.0, 0.0)]])
            .expect("both runs matched");
        // The union spans from a's top-left to b's bottom-right.
        assert_eq!(damage, rect(0.0, 0.0, 150.0, 210.0));
    }

    #[test]
    fn patch_text_glyphs_returns_none_when_nothing_matches() {
        let clip = rect(0.0, 0.0, 10.0, 10.0);
        let mut dl = list_of(vec![
            text_item(Some(7), clip, vec![glyph(1, 0.0, 0.0)]),
            text_item(None, clip, vec![glyph(2, 0.0, 0.0)]), // no source node => never patched
            DisplayListItem::PopClip,
        ]);

        assert_eq!(
            dl.patch_text_glyphs(8, &[vec![glyph(9, 0.0, 0.0)]]),
            None,
            "wrong node index"
        );
        assert_eq!(
            dl.patch_text_glyphs(usize::MAX, &[vec![glyph(9, 0.0, 0.0)]]),
            None
        );
        assert_eq!(
            dl.patch_text_glyphs(7, &[]),
            None,
            "no replacement runs => nothing to do"
        );

        // Nothing was mutated by any of the failed patches.
        match &dl.items[0] {
            DisplayListItem::Text { glyphs, .. } => assert_eq!(glyphs[0].index, 1),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn patch_text_glyphs_stops_when_it_runs_out_of_replacement_runs() {
        let clip = rect(0.0, 0.0, 10.0, 10.0);
        let mut dl = list_of(vec![
            text_item(Some(1), clip, vec![glyph(100, 0.0, 0.0)]),
            text_item(Some(1), clip, vec![glyph(200, 0.0, 0.0)]),
            text_item(Some(1), clip, vec![glyph(300, 0.0, 0.0)]),
        ]);

        // Only one replacement run for three matching items: patch the first, leave the rest.
        assert!(dl
            .patch_text_glyphs(1, &[vec![glyph(999, 0.0, 0.0)]])
            .is_some());
        let ids: Vec<u32> = dl
            .items
            .iter()
            .map(|i| match i {
                DisplayListItem::Text { glyphs, .. } => glyphs[0].index,
                other => panic!("expected Text, got {other:?}"),
            })
            .collect();
        assert_eq!(ids, vec![999, 200, 300]);
    }

    #[test]
    fn patch_text_glyphs_accepts_an_empty_replacement_run() {
        let clip = rect(0.0, 0.0, 10.0, 10.0);
        let mut dl = list_of(vec![text_item(Some(0), clip, vec![glyph(1, 0.0, 0.0)])]);
        // An empty run erases the glyphs but still reports damage over the old area.
        assert_eq!(dl.patch_text_glyphs(0, &[Vec::new()]), Some(clip));
        match &dl.items[0] {
            DisplayListItem::Text { glyphs, .. } => assert!(glyphs.is_empty()),
            other => panic!("expected Text, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // DisplayList::compute_text_damage_rect (text_layout)
    // ---------------------------------------------------------------------

    #[cfg(feature = "text_layout")]
    #[test]
    fn compute_text_damage_rect_empty_inputs_yield_the_zero_rect() {
        let r =
            DisplayList::compute_text_damage_rect(&[], &[], LogicalPosition::new(100.0, 200.0), 0);
        assert_eq!(
            r,
            LogicalRect::zero(),
            "no items => no damage (not a MAX..MIN garbage rect)"
        );
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn compute_text_damage_rect_translates_by_the_container_origin() {
        let old = vec![positioned(0, 10.0, 20.0, 30.0, 40.0)];
        let r =
            DisplayList::compute_text_damage_rect(&old, &[], LogicalPosition::new(100.0, 200.0), 0);
        assert_eq!(r, rect(110.0, 220.0, 30.0, 40.0));
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn compute_text_damage_rect_unions_old_and_new() {
        let old = vec![positioned(0, 0.0, 0.0, 10.0, 10.0)];
        let new = vec![positioned(0, 90.0, 190.0, 10.0, 10.0)];
        let r = DisplayList::compute_text_damage_rect(&old, &new, LogicalPosition::zero(), 0);
        assert_eq!(
            r,
            rect(0.0, 0.0, 100.0, 200.0),
            "damage must cover both the before and after ink"
        );
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn compute_text_damage_rect_skips_lines_before_the_affected_line() {
        let items = vec![
            positioned(0, 0.0, 0.0, 1000.0, 10.0), // line 0 — untouched, must be excluded
            positioned(5, 10.0, 50.0, 20.0, 10.0), // line 5 — the reflowed line
        ];
        let r = DisplayList::compute_text_damage_rect(&items, &items, LogicalPosition::zero(), 5);
        assert_eq!(r, rect(10.0, 50.0, 20.0, 10.0));

        // An affected_line past every line damages nothing.
        let none = DisplayList::compute_text_damage_rect(
            &items,
            &items,
            LogicalPosition::zero(),
            usize::MAX,
        );
        assert_eq!(none, LogicalRect::zero());
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn compute_text_damage_rect_nan_positions_are_ignored_not_propagated() {
        // f32::min/max drop NaN operands, so a NaN-positioned item contributes nothing.
        let nan_only = vec![positioned(0, f32::NAN, f32::NAN, 10.0, 10.0)];
        let r = DisplayList::compute_text_damage_rect(&nan_only, &[], LogicalPosition::zero(), 0);
        assert_eq!(
            r,
            LogicalRect::zero(),
            "an all-NaN run must not produce a NaN damage rect"
        );

        // Mixed: min/max are applied PER AXIS, so the half-NaN item is not dropped
        // wholesale -- its NaN x contributes nothing, but its finite y=0.0 still
        // widens the union. The result is a safe non-NaN superset: y spans [0,15].
        let mixed = vec![
            positioned(0, f32::NAN, 0.0, 10.0, 10.0),
            positioned(0, 5.0, 5.0, 10.0, 10.0),
        ];
        let r = DisplayList::compute_text_damage_rect(&mixed, &[], LogicalPosition::zero(), 0);
        assert!(!r.origin.x.is_nan() && !r.size.width.is_nan());
        assert_eq!(r, rect(5.0, 0.0, 10.0, 15.0));
    }

    // ---------------------------------------------------------------------
    // item_center_on_page / transform_items_to_page_coords (text_layout)
    // ---------------------------------------------------------------------

    #[cfg(feature = "text_layout")]
    #[test]
    fn item_center_on_page_uses_the_item_midpoint_half_open() {
        let item = positioned(0, 0.0, 0.0, 10.0, 20.0); // height 20 => center at +10
                                                        // layout_origin_y 90 => absolute y 90, center 100 == page_top => on page.
        assert!(item_center_on_page(&item, 90.0, 100.0, 200.0));
        // center 99.99 => just above the page.
        assert!(!item_center_on_page(&item, 89.99, 100.0, 200.0));
        // center exactly page_bottom => OFF page (half-open interval).
        assert!(!item_center_on_page(&item, 190.0, 100.0, 200.0));
        assert!(item_center_on_page(&item, 189.0, 100.0, 200.0));
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn item_center_on_page_with_nonfinite_values_is_false_not_a_panic() {
        let item = positioned(0, 0.0, f32::NAN, 10.0, 20.0);
        assert!(!item_center_on_page(&item, 0.0, 100.0, 200.0));

        let inf = positioned(0, 0.0, f32::INFINITY, 10.0, 20.0);
        assert!(!item_center_on_page(&inf, 0.0, 100.0, 200.0));

        let ok = positioned(0, 0.0, 150.0, 10.0, 20.0);
        assert!(!item_center_on_page(&ok, 0.0, f32::NAN, f32::NAN));
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn transform_items_to_page_coords_rebases_and_reports_extents() {
        let items = vec![
            positioned(0, 0.0, 10.0, 30.0, 20.0),
            positioned(1, 5.0, 40.0, 50.0, 20.0),
        ];
        // layout starts at y=100; the page starts at y=100, so new_origin_y = 0.
        let (out, min_y, max_y, max_width) =
            transform_items_to_page_coords(items, 100.0, 100.0, 0.0);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].position.y, 10.0);
        assert_eq!(out[1].position.y, 40.0);
        assert_eq!(out[0].position.x, 0.0, "X is never rebased");
        assert_eq!(min_y, 10.0);
        assert_eq!(max_y, 60.0);
        assert_eq!(max_width, 55.0, "max_width is position.x + item width");
    }

    #[cfg(feature = "text_layout")]
    #[test]
    fn transform_items_to_page_coords_on_an_empty_input_returns_sentinel_extents() {
        let (out, min_y, max_y, max_width) =
            transform_items_to_page_coords(Vec::new(), 0.0, 0.0, 0.0);
        assert!(out.is_empty());
        // The seeds are returned untouched — callers MUST NOT treat these as real bounds.
        assert_eq!(min_y, f32::MAX);
        assert_eq!(max_y, f32::MIN);
        assert_eq!(max_width, 0.0);
    }

    // ---------------------------------------------------------------------
    // DisplayList::to_debug_json
    // ---------------------------------------------------------------------

    #[test]
    fn to_debug_json_on_an_empty_list_reports_a_balanced_zero_item_list() {
        let json = DisplayList::default().to_debug_json();
        assert!(json.contains("\"total_items\": 0"));
        assert!(json.contains("\"balanced\": true"));
        assert!(json.contains("\"final_clip_depth\": 0"));
    }

    #[test]
    fn to_debug_json_flags_an_unbalanced_clip_stack() {
        // A PushClip with no PopClip must be reported as unbalanced.
        let dl = list_of(vec![DisplayListItem::PushClip {
            bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
            border_radius: BorderRadius::default(),
        }]);
        let json = dl.to_debug_json();
        assert!(json.contains("\"final_clip_depth\": 1"));
        assert!(json.contains("\"balanced\": false"));

        // A stray PopClip drives the depth negative — also unbalanced, not a panic.
        let json = list_of(vec![DisplayListItem::PopClip]).to_debug_json();
        assert!(json.contains("\"final_clip_depth\": -1"));
        assert!(json.contains("\"balanced\": false"));
    }

    #[test]
    fn to_debug_json_survives_a_truncated_node_mapping_and_extreme_values() {
        // node_mapping shorter than items must not index out of bounds.
        let dl = DisplayList {
            items: vec![
                DisplayListItem::PushClip {
                    bounds: rect(f32::NAN, f32::INFINITY, f32::MAX, -1.0).into(),
                    border_radius: BorderRadius {
                        top_left: f32::NAN,
                        ..BorderRadius::default()
                    },
                },
                DisplayListItem::PopClip,
                DisplayListItem::PushStackingContext {
                    z_index: i32::MIN,
                    bounds: WindowLogicalRect::zero(),
                },
                DisplayListItem::PopStackingContext,
                DisplayListItem::PushScrollFrame {
                    clip_bounds: WindowLogicalRect::zero(),
                    content_size: LogicalSize::new(f32::MAX, f32::MAX),
                    scroll_id: u64::MAX,
                },
                DisplayListItem::PopScrollFrame,
                DisplayListItem::PopTextShadow, // exercises the `_ =>` fallback arm
            ],
            node_mapping: Vec::new(), // deliberately desynced
            ..DisplayList::default()
        };
        let json = dl.to_debug_json();
        assert!(json.contains("\"total_items\": 7"));
        assert!(
            json.contains("\"balanced\": true"),
            "every push is matched by a pop"
        );
    }

    // ---------------------------------------------------------------------
    // apply_text_overflow_ellipsis
    // ---------------------------------------------------------------------

    #[test]
    fn ellipsis_leaves_non_overflowing_text_alone() {
        let container = rect(0.0, 0.0, 100.0, 20.0);
        let glyphs = vec![glyph(1, 0.0, 10.0), glyph(2, 10.0, 10.0)]; // right edge 18 < 100
        let mut dl = list_of(vec![text_item(
            Some(0),
            rect(0.0, 0.0, 100.0, 20.0),
            glyphs.clone(),
        )]);

        apply_text_overflow_ellipsis(&mut dl, container, "…");
        match &dl.items[0] {
            DisplayListItem::Text { glyphs: g, .. } => {
                assert_eq!(g.len(), glyphs.len());
                assert_eq!(g.iter().map(|x| x.index).collect::<Vec<_>>(), vec![1, 2]);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn ellipsis_truncates_overflowing_text_and_appends_u2026() {
        let container = rect(0.0, 0.0, 50.0, 20.0);
        // Glyphs at x = 0,10,20,30,40,50 each 8 wide => right edges 8,18,28,38,48,58.
        let glyphs: Vec<_> = (0..6)
            .map(|i| glyph(i + 1, (i as f32) * 10.0, 10.0))
            .collect();
        let mut dl = list_of(vec![text_item(
            Some(0),
            rect(0.0, 0.0, 500.0, 20.0),
            glyphs,
        )]);

        apply_text_overflow_ellipsis(&mut dl, container, "…");
        match &dl.items[0] {
            DisplayListItem::Text {
                glyphs: g,
                clip_rect,
                ..
            } => {
                // font_size 16 => ellipsis width 9.6 => truncation edge 40.4;
                // glyph right edges 8/18/28/38 fit, 48 does not.
                assert_eq!(g.len(), 5, "4 kept glyphs + 1 ellipsis");
                assert_eq!(
                    g.last().unwrap().index,
                    0x2026,
                    "U+2026 HORIZONTAL ELLIPSIS"
                );
                assert_eq!(g[3].index, 4, "the last kept glyph");
                // The clip rect is retargeted to the container so nothing spills past it.
                assert_eq!(clip_rect.into_inner(), container);
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn ellipsis_on_a_container_too_narrow_for_any_glyph_leaves_only_the_ellipsis() {
        // keep_count == 0 => glyphs.truncate(0) => `glyphs.last()` is None. The fallback
        // must anchor the ellipsis to the container origin instead of panicking.
        let container = rect(3.0, 4.0, 1.0, 20.0);
        let glyphs = vec![glyph(1, 0.0, 10.0), glyph(2, 10.0, 10.0)];
        let mut dl = list_of(vec![text_item(
            Some(0),
            rect(0.0, 0.0, 500.0, 20.0),
            glyphs,
        )]);

        apply_text_overflow_ellipsis(&mut dl, container, "…");
        match &dl.items[0] {
            DisplayListItem::Text { glyphs: g, .. } => {
                assert_eq!(g.len(), 1);
                assert_eq!(g[0].index, 0x2026);
                assert_eq!(
                    g[0].point,
                    LogicalPosition::new(3.0, 4.0),
                    "anchored to the container origin"
                );
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    #[test]
    fn ellipsis_skips_empty_runs_and_non_text_items() {
        let container = rect(0.0, 0.0, 1.0, 20.0);
        let mut dl = list_of(vec![
            text_item(Some(0), rect(0.0, 0.0, 500.0, 20.0), Vec::new()),
            DisplayListItem::PopClip,
            DisplayListItem::Rect {
                bounds: rect(0.0, 0.0, 900.0, 20.0).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
        ]);
        apply_text_overflow_ellipsis(&mut dl, container, "…");
        match &dl.items[0] {
            DisplayListItem::Text { glyphs, .. } => {
                assert!(glyphs.is_empty(), "an empty run is left alone")
            }
            other => panic!("expected Text, got {other:?}"),
        }
        assert_eq!(dl.items.len(), 3, "no items added or removed");
    }

    #[test]
    fn ellipsis_with_a_nan_font_size_does_not_panic() {
        // A NaN ellipsis width makes every `glyph_right > truncation_edge` comparison
        // false, so nothing is truncated — but an ellipsis is still appended.
        let container = rect(0.0, 0.0, 50.0, 20.0);
        let glyphs = vec![glyph(1, 0.0, 10.0), glyph(2, 100.0, 10.0)];
        let mut dl = list_of(vec![DisplayListItem::Text {
            glyphs,
            font_hash: FontHash::invalid(),
            font_size_px: f32::NAN,
            color: opaque(),
            clip_rect: rect(0.0, 0.0, 500.0, 20.0).into(),
            source_node_index: None,
        }]);

        apply_text_overflow_ellipsis(&mut dl, container, "…");
        match &dl.items[0] {
            DisplayListItem::Text { glyphs: g, .. } => {
                assert_eq!(g.len(), 3, "both glyphs kept + ellipsis");
                assert_eq!(g.last().unwrap().index, 0x2026);
                assert!(g.last().unwrap().size.width.is_nan());
            }
            other => panic!("expected Text, got {other:?}"),
        }
    }

    // ---------------------------------------------------------------------
    // resolve_clip_path
    // ---------------------------------------------------------------------

    #[test]
    fn resolve_clip_path_none_is_no_clip() {
        use azul_css::props::layout::shape::ClipPath;
        assert_eq!(
            resolve_clip_path(&ClipPath::None, rect(0.0, 0.0, 100.0, 100.0)),
            None
        );
        assert_eq!(
            resolve_clip_path(&ClipPath::default(), rect(0.0, 0.0, 100.0, 100.0)),
            None
        );
    }

    #[test]
    fn resolve_clip_path_inset_shrinks_from_every_edge() {
        use azul_css::{
            corety::OptionF32,
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapeInset},
        };
        let path = ClipPath::Shape(CssShape::Inset(ShapeInset {
            inset_top: 10.0,
            inset_right: 20.0,
            inset_bottom: 30.0,
            inset_left: 40.0,
            border_radius: OptionF32::Some(6.0),
        }));
        let (r, radius) =
            resolve_clip_path(&path, rect(100.0, 200.0, 300.0, 400.0)).expect("inset clips");
        assert_eq!(r, rect(140.0, 210.0, 240.0, 360.0));
        assert_eq!(radius, 6.0);
    }

    #[test]
    fn resolve_clip_path_over_inset_clamps_the_size_to_zero() {
        use azul_css::{
            corety::OptionF32,
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapeInset},
        };
        let path = ClipPath::Shape(CssShape::Inset(ShapeInset {
            inset_top: 500.0,
            inset_right: 500.0,
            inset_bottom: 500.0,
            inset_left: 500.0,
            border_radius: OptionF32::None,
        }));
        let (r, radius) =
            resolve_clip_path(&path, rect(0.0, 0.0, 100.0, 100.0)).expect("still returns a rect");
        assert_eq!(
            r.size,
            LogicalSize::zero(),
            "insets larger than the box collapse, they do not go negative"
        );
        assert_eq!(radius, 0.0, "OptionF32::None => radius 0");
    }

    #[test]
    fn resolve_clip_path_circle_and_ellipse_use_their_bounding_box() {
        use azul_css::{
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapeCircle, ShapeEllipse, ShapePoint},
        };

        let circle = ClipPath::Shape(CssShape::Circle(ShapeCircle {
            center: ShapePoint { x: 50.0, y: 50.0 },
            radius: 20.0,
        }));
        let (r, radius) =
            resolve_clip_path(&circle, rect(100.0, 100.0, 200.0, 200.0)).expect("circle clips");
        assert_eq!(
            r,
            rect(130.0, 130.0, 40.0, 40.0),
            "centre is relative to the node origin"
        );
        assert_eq!(radius, 20.0);

        let ellipse = ClipPath::Shape(CssShape::Ellipse(ShapeEllipse {
            center: ShapePoint { x: 50.0, y: 50.0 },
            radius_x: 10.0,
            radius_y: 30.0,
        }));
        let (r, radius) =
            resolve_clip_path(&ellipse, rect(0.0, 0.0, 100.0, 100.0)).expect("ellipse clips");
        assert_eq!(r, rect(40.0, 20.0, 20.0, 60.0));
        assert_eq!(
            radius, 10.0,
            "the rounding uses the SMALLER of the two radii"
        );
    }

    #[test]
    fn resolve_clip_path_circle_with_a_degenerate_radius_does_not_panic() {
        use azul_css::{
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapeCircle, ShapePoint},
        };
        for radius in [0.0, -10.0, f32::NAN, f32::INFINITY] {
            let path = ClipPath::Shape(CssShape::Circle(ShapeCircle {
                center: ShapePoint { x: 0.0, y: 0.0 },
                radius,
            }));
            // The value is passed through unclamped; the contract here is only that it
            // returns a value deterministically instead of panicking.
            let out = resolve_clip_path(&path, rect(0.0, 0.0, 100.0, 100.0));
            assert!(out.is_some(), "radius {radius} must still resolve");
        }
    }

    #[test]
    fn resolve_clip_path_polygon_bbox_and_empty_polygon() {
        use azul_css::{
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapePoint, ShapePolygon},
        };

        // An empty polygon has no bounding box => no clip.
        let empty = ClipPath::Shape(CssShape::Polygon(ShapePolygon {
            points: Vec::new().into(),
        }));
        assert_eq!(
            resolve_clip_path(&empty, rect(0.0, 0.0, 100.0, 100.0)),
            None
        );

        // A real polygon collapses to its axis-aligned bounding box.
        let tri = ClipPath::Shape(CssShape::Polygon(ShapePolygon {
            points: vec![
                ShapePoint { x: 10.0, y: 90.0 },
                ShapePoint { x: 50.0, y: 10.0 },
                ShapePoint { x: 90.0, y: 90.0 },
            ]
            .into(),
        }));
        let (r, radius) =
            resolve_clip_path(&tri, rect(1000.0, 2000.0, 100.0, 100.0)).expect("polygon clips");
        assert_eq!(r, rect(1010.0, 2010.0, 80.0, 80.0));
        assert_eq!(radius, 0.0, "a polygon bbox is never rounded");
    }

    #[test]
    fn resolve_clip_path_polygon_with_nan_points_collapses_instead_of_panicking() {
        use azul_css::{
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapePoint, ShapePolygon},
        };
        let nan_poly = ClipPath::Shape(CssShape::Polygon(ShapePolygon {
            points: vec![ShapePoint {
                x: f32::NAN,
                y: f32::NAN,
            }]
            .into(),
        }));
        let (r, _) =
            resolve_clip_path(&nan_poly, rect(0.0, 0.0, 100.0, 100.0)).expect("still resolves");
        // f32::min/max drop NaN, leaving the ±INFINITY seeds; the `.max(0.0)` on the size
        // keeps the result degenerate-but-finite rather than negative.
        assert_eq!(r.size, LogicalSize::zero());
    }

    #[test]
    fn resolve_clip_path_svg_path_is_unsupported_and_does_not_clip() {
        use azul_css::{
            props::layout::shape::ClipPath,
            shape::{CssShape, ShapePath},
        };
        let path = ClipPath::Shape(CssShape::Path(ShapePath {
            data: String::from("M 0 0 L 10 10 Z").into(),
        }));
        assert_eq!(
            resolve_clip_path(&path, rect(0.0, 0.0, 100.0, 100.0)),
            None,
            "path() clip-paths are not implemented => no clipping (rather than a wrong clip)"
        );
    }

    // ---------------------------------------------------------------------
    // apply_clip_path
    // ---------------------------------------------------------------------

    #[test]
    fn apply_clip_path_wraps_the_tail_of_the_list_and_keeps_the_mapping_in_sync() {
        let mut dl = list_of(vec![
            DisplayListItem::Rect {
                bounds: rect(0.0, 0.0, 10.0, 10.0).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
            DisplayListItem::Rect {
                bounds: rect(0.0, 0.0, 20.0, 20.0).into(),
                color: opaque(),
                border_radius: BorderRadius::default(),
            },
        ]);

        apply_clip_path(&mut dl, 1, rect(5.0, 5.0, 50.0, 50.0), 8.0);

        assert_eq!(dl.items.len(), 4, "PushClip inserted + PopClip appended");
        assert_eq!(
            dl.items.len(),
            dl.node_mapping.len(),
            "node_mapping must stay parallel to items"
        );
        assert!(matches!(dl.items[1], DisplayListItem::PushClip { .. }));
        assert!(matches!(dl.items[3], DisplayListItem::PopClip));
        assert_eq!(dl.node_mapping[1], None);
        assert_eq!(dl.node_mapping[3], None);

        match &dl.items[1] {
            DisplayListItem::PushClip {
                bounds,
                border_radius,
            } => {
                assert_eq!(bounds.into_inner(), rect(5.0, 5.0, 50.0, 50.0));
                // A positive radius is applied uniformly to all four corners.
                assert_eq!(border_radius.top_left, 8.0);
                assert_eq!(border_radius.bottom_right, 8.0);
            }
            other => panic!("expected PushClip, got {other:?}"),
        }
    }

    #[test]
    fn apply_clip_path_with_a_nonpositive_radius_uses_square_corners() {
        for radius in [0.0, -5.0, f32::NAN] {
            let mut dl = list_of(vec![DisplayListItem::PopClip]);
            apply_clip_path(&mut dl, 0, rect(0.0, 0.0, 10.0, 10.0), radius);
            match &dl.items[0] {
                DisplayListItem::PushClip { border_radius, .. } => {
                    assert!(
                        border_radius.is_zero(),
                        "radius {radius} must not round the clip"
                    );
                }
                other => panic!("expected PushClip, got {other:?}"),
            }
        }
    }

    #[test]
    fn apply_clip_path_at_the_end_index_appends_rather_than_panicking() {
        let mut dl = list_of(vec![DisplayListItem::PopClip]);
        // start_index == items.len() is the boundary case and must be accepted.
        apply_clip_path(&mut dl, 1, rect(0.0, 0.0, 10.0, 10.0), 0.0);
        assert_eq!(dl.items.len(), 3);
        assert_eq!(dl.items.len(), dl.node_mapping.len());
        assert!(matches!(dl.items[1], DisplayListItem::PushClip { .. }));
        assert!(matches!(dl.items[2], DisplayListItem::PopClip));
    }

    #[test]
    #[should_panic(expected = "insertion index")]
    fn apply_clip_path_past_the_end_panics_on_the_vec_insert() {
        // BUG: `start_index` is not validated against `items.len()`, so an out-of-range
        // index panics inside `Vec::insert` instead of being rejected. Pinned so the
        // panic is a deliberate, visible contract rather than a latent crash.
        let mut dl = DisplayList::default();
        apply_clip_path(&mut dl, 3, rect(0.0, 0.0, 10.0, 10.0), 0.0);
    }

    // ======================================================================
    // E18: repeated theads for MULTIPLE straddling tables (side-by-side)
    // ======================================================================

    fn plain_rect(x: f32, y: f32, w: f32, h: f32) -> DisplayListItem {
        DisplayListItem::Rect {
            bounds: rect(x, y, w, h).into(),
            color: ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            border_radius: BorderRadius::default(),
        }
    }

    fn item_y(item: &DisplayListItem) -> f32 {
        item.visual_bounds().map(|b| b.origin.y).unwrap_or(f32::NAN)
    }
    fn item_x(item: &DisplayListItem) -> f32 {
        item.visual_bounds().map(|b| b.origin.x).unwrap_or(f32::NAN)
    }

    /// Two side-by-side tables (columns) straddling the same page top: each
    /// repeated thead stays in its own x-band AT the page top, each table's
    /// continued rows shift by ITS OWN thead height, and content below both
    /// shifts by the SUM (the break pass reserves the sum per page).
    #[test]
    fn side_by_side_straddling_tables_keep_their_own_thead_offsets() {
        use crate::solver3::pagination::{TableHeaderInfo, TableHeaderTracker};

        // Table A: x 0..280, thead 20 tall. Table B: x 320..600, thead 30.
        // Both span content y 0..350 over 200-tall pages: page 1 (y 200..400)
        // is a continuation page for both.
        let thead_a = vec![plain_rect(0.0, 0.0, 280.0, 20.0)];
        let thead_b = vec![plain_rect(320.0, 0.0, 280.0, 30.0)];
        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(TableHeaderInfo {
            table_node_index: 1,
            table_start_y: 0.0,
            table_end_y: 350.0,
            thead_items: thead_a,
            thead_height: 20.0,
            thead_offset_y: 0.0,
        });
        tracker.register_table_header(TableHeaderInfo {
            table_node_index: 2,
            table_start_y: 0.0,
            table_end_y: 350.0,
            thead_items: thead_b,
            thead_height: 30.0,
            thead_offset_y: 0.0,
        });

        // Content: one row of each table living on page 1 (content y
        // 200..240), plus a full-width paragraph BELOW both tables
        // (content y 360..380).
        let full = DisplayList {
            items: vec![
                plain_rect(0.0, 200.0, 280.0, 40.0),   // A's continued row
                plain_rect(320.0, 200.0, 280.0, 40.0), // B's continued row
                plain_rect(0.0, 360.0, 600.0, 20.0),   // below both
            ],
            node_mapping: vec![None, None, None],
            ..DisplayList::default()
        };

        let config = SlicerConfig {
            table_headers: tracker,
            ..SlicerConfig::simple(200.0)
        };
        let pages = paginate_display_list_with_slicer_and_breaks(
            full,
            &config,
            &RendererResources::default(),
        )
        .expect("paginate");
        assert!(
            pages.len() >= 2,
            "content spans two pages, got {}",
            pages.len()
        );
        let page1 = &pages[1];

        // Both theads sit AT the page top, in their own x-bands.
        let thead_a_y = page1
            .items
            .iter()
            .filter(|i| {
                item_x(i) < 300.0 && i.visual_bounds().is_some_and(|b| b.size.height == 20.0)
            })
            .map(item_y)
            .fold(f32::NAN, f32::min);
        let thead_b_y = page1
            .items
            .iter()
            .filter(|i| {
                item_x(i) >= 300.0 && i.visual_bounds().is_some_and(|b| b.size.height == 30.0)
            })
            .map(item_y)
            .fold(f32::NAN, f32::min);
        assert_eq!(thead_a_y, 0.0, "thead A at its column's page top");
        assert_eq!(
            thead_b_y, 0.0,
            "thead B at its OWN column's page top - NOT stacked below A"
        );

        // Each table's continued row shifts by ITS OWN thead height.
        let row_a = page1
            .items
            .iter()
            .find(|i| item_x(i) < 300.0 && i.visual_bounds().is_some_and(|b| b.size.height == 40.0))
            .expect("A's row on page 1");
        let row_b = page1
            .items
            .iter()
            .find(|i| {
                item_x(i) >= 300.0 && i.visual_bounds().is_some_and(|b| b.size.height == 40.0)
            })
            .expect("B's row on page 1");
        assert_eq!(item_y(row_a), 20.0, "A's row below A's 20px thead only");
        assert_eq!(
            item_y(row_b),
            30.0,
            "B's row below B's 30px thead only - the old max() collapse \
             shifted both by 30 and overlapped A"
        );

        // Content below BOTH tables shifts by the SUM (matches the break
        // pass's per-page reservation).
        let below = page1
            .items
            .iter()
            .find(|i| i.visual_bounds().is_some_and(|b| b.size.width == 600.0))
            .expect("below-both paragraph on page 1");
        assert_eq!(item_y(below), 160.0 + 50.0, "sum of both thead heights");
    }

    // ======================================================================
    // E17: clip / stacking structure is re-derived per page slice
    // ======================================================================

    /// A clipped, stacked subtree spanning pages 0-1: BOTH pages re-open the
    /// chain around the content that lands on them, balanced; a page without
    /// any of the subtree's content carries none of its markers.
    #[test]
    fn clip_and_stacking_chain_is_rederived_on_every_page_it_touches() {
        // Subtree: PushStackingContext > PushClip(y 50..350) > rect(60..340),
        // then a plain rect on page 2 (y 450..470) OUTSIDE the subtree.
        let full = DisplayList {
            items: vec![
                DisplayListItem::PushStackingContext {
                    z_index: 3,
                    bounds: rect(0.0, 50.0, 600.0, 300.0).into(),
                },
                DisplayListItem::PushClip {
                    bounds: rect(0.0, 50.0, 600.0, 300.0).into(),
                    border_radius: BorderRadius::default(),
                },
                plain_rect(0.0, 60.0, 600.0, 280.0),
                DisplayListItem::PopClip,
                DisplayListItem::PopStackingContext,
                plain_rect(0.0, 450.0, 600.0, 20.0),
            ],
            node_mapping: vec![None; 6],
            ..DisplayList::default()
        };
        let config = SlicerConfig::simple(200.0);
        let pages = paginate_display_list_with_slicer_and_breaks(
            full,
            &config,
            &RendererResources::default(),
        )
        .expect("paginate");
        assert!(pages.len() >= 3, "3 pages of content, got {}", pages.len());

        let structure = |page: &DisplayList| -> Vec<&'static str> {
            page.items
                .iter()
                .map(|i| match i {
                    DisplayListItem::PushStackingContext { .. } => "push_sc",
                    DisplayListItem::PopStackingContext => "pop_sc",
                    DisplayListItem::PushClip { .. } => "push_clip",
                    DisplayListItem::PopClip => "pop_clip",
                    DisplayListItem::Rect { .. } => "rect",
                    _ => "other",
                })
                .collect()
        };

        // Page 0: chain opens before the rect slice, closes after.
        assert_eq!(
            structure(&pages[0]),
            vec!["push_sc", "push_clip", "rect", "pop_clip", "pop_sc"],
            "page 0: {:?}",
            structure(&pages[0])
        );
        // Page 1: the SAME chain re-opens for the continued slice — this is
        // what the slicer used to drop entirely (unclipped bleed, flattened
        // z-order on continuation pages).
        assert_eq!(
            structure(&pages[1]),
            vec!["push_sc", "push_clip", "rect", "pop_clip", "pop_sc"],
            "page 1: {:?}",
            structure(&pages[1])
        );
        // The re-opened clip is INTERSECTED with page 1's band and rebased:
        // original clip 50..350 ∩ page [200, 400) → page-local 0..150.
        let clip_bounds = pages[1]
            .items
            .iter()
            .find_map(|i| match i {
                DisplayListItem::PushClip { bounds, .. } => Some(bounds.0),
                _ => None,
            })
            .expect("clip on page 1");
        assert_eq!(
            (clip_bounds.origin.y, clip_bounds.size.height),
            (0.0, 150.0),
            "clip clipped to the page band and rebased"
        );
        // Page 2: no subtree content — NO markers, just the plain rect.
        assert_eq!(
            structure(&pages[2]),
            vec!["rect"],
            "a page without the subtree's content carries none of its markers"
        );
        // The stacking context keeps its z-index on every page it touches.
        for page in &pages[..2] {
            let z = page.items.iter().find_map(|i| match i {
                DisplayListItem::PushStackingContext { z_index, .. } => Some(*z_index),
                _ => None,
            });
            assert_eq!(z, Some(3));
        }
    }

    /// One straddling table: everything shifts by exactly its thead height —
    /// the pre-E18 behavior, unchanged.
    #[test]
    fn single_straddling_table_keeps_the_uniform_shift() {
        use crate::solver3::pagination::{TableHeaderInfo, TableHeaderTracker};

        let mut tracker = TableHeaderTracker::new();
        tracker.register_table_header(TableHeaderInfo {
            table_node_index: 1,
            table_start_y: 0.0,
            table_end_y: 350.0,
            thead_items: vec![plain_rect(0.0, 0.0, 600.0, 25.0)],
            thead_height: 25.0,
            thead_offset_y: 0.0,
        });
        let full = DisplayList {
            items: vec![
                plain_rect(0.0, 200.0, 600.0, 40.0), // continued row
                plain_rect(0.0, 360.0, 600.0, 20.0), // below the table
            ],
            node_mapping: vec![None, None],
            ..DisplayList::default()
        };
        let config = SlicerConfig {
            table_headers: tracker,
            ..SlicerConfig::simple(200.0)
        };
        let pages = paginate_display_list_with_slicer_and_breaks(
            full,
            &config,
            &RendererResources::default(),
        )
        .expect("paginate");
        let page1 = &pages[1];
        let row = page1
            .items
            .iter()
            .find(|i| i.visual_bounds().is_some_and(|b| b.size.height == 40.0))
            .expect("row");
        let below = page1
            .items
            .iter()
            .find(|i| {
                i.visual_bounds()
                    .is_some_and(|b| b.size.height == 20.0 && b.size.width == 600.0)
            })
            .expect("below");
        assert_eq!(item_y(row), 25.0);
        assert_eq!(item_y(below), 160.0 + 25.0);
    }
}

#[cfg(test)]
mod dense_scroll_extent_tests {
    use super::*;
    use crate::text3::cache::{
        BidiDirection, ClusterFlags, ContentIndex, GraphemeClusterId, LayoutFontMetrics,
        OverflowInfo, Point, PositionedItem, ShapedCluster, ShapedGlyph, ShapedItem,
        StyleProperties, UnifiedLayout,
    };
    use crate::text3::dense::DenseText;
    use alloc::sync::Arc;

    /// One real cluster with real metrics so the dense extent is NON-zero
    /// and the sparse-vs-dense A/B is meaningful (an empty layout passes
    /// the guard vacuously at 0x0 — that is exactly the coverage hole the
    /// d3 NC exposed: no lib/corpus test reached this path with a dense
    /// view, so the verify assert could not fire).
    fn one_cluster_layout() -> UnifiedLayout {
        let metrics = LayoutFontMetrics {
            ascent: 800.0,
            descent: -200.0,
            cap_height: None,
            x_height: None,
            line_gap: 0.0,
            units_per_em: 1000,
        };
        let glyph = ShapedGlyph {
            kind: text3::cache::GlyphKind::Character,
            glyph_id: 7,
            cluster_offset: 0,
            advance: 10.0,
            kerning: 0.0,
            offset: Point::default(),
            vertical_advance: 0.0,
            vertical_offset: Point::default(),
            script: text3::script::Script::Latin,
            font_hash: 42,
            font_metrics: metrics,
        };
        let cluster = ShapedCluster {
            flags: ClusterFlags::classify("a"),
            source_text: std::sync::Arc::from("a"),
            source_byte_len: 1,
            source_cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            source_content_index: ContentIndex {
                run_index: 0,
                item_index: 0,
            },
            source_node_id: None,
            glyphs: smallvec::smallvec![glyph],
            advance: 10.0,
            direction: BidiDirection::Ltr,
            style: Arc::new(StyleProperties {
                font_size_px: 16.0,
                ..StyleProperties::default()
            }),
            marker_position_outside: None,
            is_first_fragment: true,
            is_last_fragment: true,
        };
        UnifiedLayout {
            items: vec![PositionedItem {
                item: ShapedItem::Cluster(cluster),
                position: Point { x: 5.0, y: 3.0 },
                line_index: 0,
            }],
            overflow: OverflowInfo::default(),
        }
    }

    /// (d5) The pagination clipper must treat a TextPayload payload
    /// EXACTLY like a bare UnifiedLayout payload — same clip decision,
    /// same output bounds. This is the only coverage the TextPayload
    /// arm has (it activates under AZ_DENSE_TEXT only, and no verify
    /// run paginates), so its NC is required to go red here.
    #[test]
    fn clip_text_layout_item_reads_through_the_text_payload() {
        use crate::solver3::layout_tree::TextPayload;
        let layout = Arc::new(one_cluster_layout());
        let dense = Arc::new(DenseText::from_unified(&layout));
        let bounds = LogicalRect::new(
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(100.0, 40.0),
        );
        let bare: Arc<dyn std::any::Any + Send + Sync> = layout.clone();
        let wrapped: Arc<dyn std::any::Any + Send + Sync> = Arc::new(TextPayload {
            dense,
            sparse: layout,
        });
        let via_bare = clip_text_layout_item(
            &bare,
            bounds,
            FontHash::from_hash(42),
            16.0,
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            0.0,
            1000.0,
        );
        let via_payload = clip_text_layout_item(
            &wrapped,
            bounds,
            FontHash::from_hash(42),
            16.0,
            ColorU {
                r: 0,
                g: 0,
                b: 0,
                a: 255,
            },
            0.0,
            1000.0,
        );
        match (via_bare, via_payload) {
            (
                Some(DisplayListItem::TextLayout { bounds: a, .. }),
                Some(DisplayListItem::TextLayout { bounds: b, .. }),
            ) => {
                assert_eq!(a, b, "payload route must produce identical clip bounds");
            }
            other => panic!("both routes must clip to TextLayout items, got {other:?}"),
        }
    }

    #[test]
    fn scroll_extent_from_dense_matches_the_sparse_walk() {
        let layout = one_cluster_layout();
        let dense = DenseText::from_unified(&layout);
        assert_eq!(
            dense.clusters.len(),
            layout.items.len(),
            "pure-cluster guard"
        );

        // Sparse reference extent.
        let mut sx: f32 = 0.0;
        let mut sy: f32 = 0.0;
        for it in &layout.items {
            let b = it.item.bounds();
            sx = sx.max(it.position.x + b.width);
            sy = sy.max(it.position.y + b.height);
        }
        assert!(sx > 14.0 && sy > 10.0, "non-trivial extent ({sx}x{sy})");

        // Dense extent — the exact computation get_scroll_content_size uses.
        let dx = dense
            .clusters
            .iter()
            .map(|c| c.x + c.advance)
            .fold(0.0f32, f32::max);
        let dy = dense
            .lines
            .iter()
            .map(|l| l.top_y + l.height)
            .fold(0.0f32, f32::max);
        assert!((sx - dx).abs() < 0.01, "width: sparse {sx} vs dense {dx}");
        assert!((sy - dy).abs() < 0.01, "height: sparse {sy} vs dense {dy}");
    }
}

/// The display list's INSERTION contract: `items` has three arrays parallel to
/// it plus index ranges pointing into it, and only an append can ignore them.
#[cfg(test)]
mod insert_item_tests {
    use super::*;

    /// A MID-LIST insert must keep every array parallel to `items` aligned and
    /// slide the index ranges that point into it.
    ///
    /// The focus ring is the first item the engine inserts anywhere but the
    /// end: it has to go INSIDE the focused node's scroll frame so it travels
    /// with the page. The first version used a bare `items.insert`, which left
    /// `uniform_text_bgs` and `layout_node_mapping` reading one item to the
    /// left of where they belong from the insertion point onwards, and every
    /// `fixed_position_item_ranges` entry pointing one item short.
    #[test]
    fn inserting_an_item_keeps_the_parallel_arrays_aligned() {
        let mut dl = DisplayList::default();
        for i in 0..4 {
            dl.items.push(DisplayListItem::PopClip);
            dl.node_mapping.push(Some(NodeId::new(i)));
            dl.uniform_text_bgs.push(None);
            dl.layout_node_mapping.push(Some((i, EmitPhase::Content)));
        }
        // A fixed-position range over items 2..4, i.e. the two after the
        // insertion point.
        dl.fixed_position_item_ranges.push((2, 4));

        dl.insert_item(1, DisplayListItem::PopClip, None);

        assert_eq!(dl.items.len(), 5);
        for len in [
            dl.node_mapping.len(),
            dl.uniform_text_bgs.len(),
            dl.layout_node_mapping.len(),
        ] {
            assert_eq!(len, dl.items.len(), "a side table fell out of step");
        }
        // The item that WAS at index 1 is now at 2, and its side entries moved
        // with it.
        assert_eq!(dl.node_mapping[2], Some(NodeId::new(1)));
        assert_eq!(dl.layout_node_mapping[2], Some((1, EmitPhase::Content)));
        // The inserted item owns index 1 and belongs to no node.
        assert_eq!(dl.node_mapping[1], None);
        assert_eq!(dl.layout_node_mapping[1], None);
        // The fixed-position range still covers the same two ITEMS.
        assert_eq!(dl.fixed_position_item_ranges[0], (3, 5));
    }

    /// A side table shorter than `items` (older code appended without growing
    /// them) must not make the insert panic or misplace the new entry.
    #[test]
    fn inserting_into_a_short_side_table_pads_it_first() {
        let mut dl = DisplayList::default();
        for _ in 0..3 {
            dl.items.push(DisplayListItem::PopClip);
        }
        // node_mapping is deliberately left empty.
        dl.insert_item(3, DisplayListItem::PopClip, None);
        assert_eq!(dl.items.len(), 4);
        assert_eq!(dl.node_mapping.len(), 4, "the table was padded to fit");
        assert!(dl.node_mapping.iter().all(Option::is_none));
    }
}

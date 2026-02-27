//! Generates a renderer-agnostic display list from a laid-out tree

use std::{collections::BTreeMap, sync::Arc};

use allsorts::glyph_position;
use azul_core::{
    dom::{DomId, FormattingContext, NodeId, NodeType, ScrollbarOrientation},
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    gpu::GpuValueCache,
    hit_test::ScrollPosition,
    hit_test_tag::{CursorType, TAG_TYPE_CURSOR},
    resources::{
        IdNamespace, ImageRef, OpacityKey, RendererResources, TransformKey,
    },
    transform::ComputedTransform3D,
    selection::{Selection, SelectionRange, SelectionState, TextSelection},
    styled_dom::StyledDom,
    ui_solver::GlyphInstance,
};
use azul_css::{
    css::CssPropertyValue,
    format_rust_code::GetHash,
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
            get_break_after, get_break_before, get_caret_style, get_overflow_x, get_overflow_y,
            get_scrollbar_info_from_layout, get_scrollbar_style, get_selection_style,
            get_style_border_radius, get_z_index, is_forced_page_break, BorderInfo, CaretStyle,
            ComputedScrollbarStyle, SelectionStyle,
        },
        layout_tree::{LayoutNode, LayoutTree},
        positioning::get_position_type,
        scrollbar::{ScrollbarRequirements, compute_scrollbar_geometry},
        LayoutContext, LayoutError, Result,
    },
};

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
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BorderBoxRect(pub LogicalRect);

/// A `LogicalRect` known to be in **absolute window coordinates** (as output
/// by the layout engine).  All spatial bounds stored in [`DisplayListItem`] use
/// this type so that the compositor is *forced* to convert them to
/// frame-relative coordinates before passing them to WebRender.
///
/// ## Coordinate-space contract
///
/// * **Layout engine** produces `WindowLogicalRect` values.
/// * **Compositor** converts via `resolve_rect()` → WebRender `LayoutRect`.
/// * Passing a `WindowLogicalRect` directly to a WebRender push function is a
///   **type error** (it wraps `LogicalRect`, not `LayoutRect`).
///
/// See `doc/SCROLL_COORDINATE_ARCHITECTURE.md` for background.
#[derive(Debug, Copy, Clone, Default, PartialEq, PartialOrd, Eq, Ord, Hash)]
pub struct WindowLogicalRect(pub LogicalRect);

impl WindowLogicalRect {
    #[inline]
    pub const fn new(origin: LogicalPosition, size: LogicalSize) -> Self {
        Self(LogicalRect::new(origin, size))
    }

    #[inline]
    pub const fn zero() -> Self {
        Self(LogicalRect::zero())
    }

    /// Access the inner `LogicalRect` (still in window space – the caller is
    /// responsible for applying any offset conversion).
    #[inline]
    pub const fn inner(&self) -> &LogicalRect {
        &self.0
    }

    #[inline]
    pub const fn into_inner(self) -> LogicalRect {
        self.0
    }

    // Convenience accessors
    #[inline] pub fn origin(&self) -> LogicalPosition { self.0.origin }
    #[inline] pub fn size(&self)   -> LogicalSize     { self.0.size }
}

impl From<LogicalRect> for WindowLogicalRect {
    #[inline]
    fn from(r: LogicalRect) -> Self { Self(r) }
}

impl From<WindowLogicalRect> for LogicalRect {
    #[inline]
    fn from(w: WindowLogicalRect) -> Self { w.0 }
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
#[derive(Debug, Clone)]
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
    /// When present, the compositor will wrap the thumb in a PushReferenceFrame
    /// with PropertyBinding::Binding so WebRender can animate the thumb position
    /// without rebuilding the display list.
    pub thumb_transform_key: Option<TransformKey>,
    /// Initial transform for the scrollbar thumb (current scroll position).
    /// This is the transform applied when the display list is first built.
    /// During GPU-only scroll, synchronize_gpu_values updates this dynamically.
    pub thumb_initial_transform: ComputedTransform3D,
    /// Optional hit-test ID for WebRender hit-testing.
    pub hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    /// Whether to clip scrollbar to container's border-radius
    pub clip_to_container_border: bool,
    /// Container's border-radius (for clipping)
    pub container_border_radius: BorderRadius,
}

impl BorderBoxRect {
    /// Convert border-box to content-box by subtracting padding and border.
    /// Content-box is where inline layout and text actually render.
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

    /// Get the inner LogicalRect
    pub fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// A rectangle in content-box coordinates (excludes padding and border).
/// This is where text and inline content is positioned by the inline formatter.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContentBoxRect(pub LogicalRect);

impl ContentBoxRect {
    /// Get the inner LogicalRect
    pub fn rect(&self) -> LogicalRect {
        self.0
    }
}

/// The final, renderer-agnostic output of the layout engine.
///
/// This is a flat list of drawing and state-management commands, already sorted
/// according to the CSS paint order. A renderer can consume this list directly.
#[derive(Debug, Default)]
pub struct DisplayList {
    pub items: Vec<DisplayListItem>,
    /// Optional mapping from item index to the DOM NodeId that generated it.
    /// Used for pagination to look up CSS break properties.
    /// Not all items have a source node (e.g., synthesized decorations).
    pub node_mapping: Vec<Option<NodeId>>,
    /// Y-positions where forced page breaks should occur (from break-before/break-after: always).
    /// These are absolute Y coordinates in the infinite canvas coordinate system.
    /// The slicer will ensure page boundaries align with these positions.
    pub forced_page_breaks: Vec<f32>,
}

impl DisplayList {
    /// Generates a JSON representation of the display list for debugging.
    /// This includes clip chain analysis showing how clips are stacked.
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
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PushClip\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},", 
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(json, "      \"border_radius\": {{ \"tl\": {:.1}, \"tr\": {:.1}, \"bl\": {:.1}, \"br\": {:.1} }},",
                        border_radius.top_left, border_radius.top_right,
                        border_radius.bottom_left, border_radius.bottom_right).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::PopClip => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PopClip\",").unwrap();
                    writeln!(json, "      \"clip_depth_before\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"clip_depth_after\": {}", clip_depth - 1).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                    clip_depth -= 1;
                }
                DisplayListItem::PushScrollFrame {
                    clip_bounds,
                    content_size,
                    scroll_id,
                } => {
                    scroll_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PushScrollFrame\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"clip_bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        clip_bounds.0.origin.x, clip_bounds.0.origin.y,
                        clip_bounds.0.size.width, clip_bounds.0.size.height).unwrap();
                    writeln!(
                        json,
                        "      \"content_size\": {{ \"w\": {:.1}, \"h\": {:.1} }},",
                        content_size.width, content_size.height
                    )
                    .unwrap();
                    writeln!(json, "      \"scroll_id\": {},", scroll_id).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::PopScrollFrame => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PopScrollFrame\",").unwrap();
                    writeln!(json, "      \"scroll_depth_before\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"scroll_depth_after\": {}", scroll_depth - 1).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                    scroll_depth -= 1;
                }
                DisplayListItem::PushStackingContext { z_index, bounds } => {
                    stacking_depth += 1;
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PushStackingContext\",").unwrap();
                    writeln!(json, "      \"stacking_depth\": {},", stacking_depth).unwrap();
                    writeln!(json, "      \"z_index\": {},", z_index).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }}",
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::PopStackingContext => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"PopStackingContext\",").unwrap();
                    writeln!(json, "      \"stacking_depth_before\": {},", stacking_depth).unwrap();
                    writeln!(
                        json,
                        "      \"stacking_depth_after\": {}",
                        stacking_depth - 1
                    )
                    .unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                    stacking_depth -= 1;
                }
                DisplayListItem::Rect {
                    bounds,
                    color,
                    border_radius,
                } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"Rect\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(
                        json,
                        "      \"color\": \"rgba({},{},{},{})\",",
                        color.r, color.g, color.b, color.a
                    )
                    .unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::Border { bounds, .. } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"Border\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }},",
                        bounds.0.origin.x, bounds.0.origin.y, bounds.0.size.width, bounds.0.size.height).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                DisplayListItem::ScrollBarStyled { info } => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(json, "      \"type\": \"ScrollBarStyled\",").unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"orientation\": \"{:?}\",", info.orientation).unwrap();
                    writeln!(json, "      \"bounds\": {{ \"x\": {:.1}, \"y\": {:.1}, \"w\": {:.1}, \"h\": {:.1} }}",
                        info.bounds.0.origin.x, info.bounds.0.origin.y,
                        info.bounds.0.size.width, info.bounds.0.size.height).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
                _ => {
                    writeln!(json, "    {{").unwrap();
                    writeln!(json, "      \"index\": {},", i).unwrap();
                    writeln!(
                        json,
                        "      \"type\": \"{:?}\",",
                        std::mem::discriminant(item)
                    )
                    .unwrap();
                    writeln!(json, "      \"clip_depth\": {},", clip_depth).unwrap();
                    writeln!(json, "      \"scroll_depth\": {},", scroll_depth).unwrap();
                    writeln!(json, "      \"node_id\": {:?}", node_id).unwrap();
                    writeln!(json, "    }}{}", comma).unwrap();
                }
            }
        }

        writeln!(json, "  ],").unwrap();
        writeln!(json, "  \"final_clip_depth\": {},", clip_depth).unwrap();
        writeln!(json, "  \"final_scroll_depth\": {},", scroll_depth).unwrap();
        writeln!(json, "  \"final_stacking_depth\": {},", stacking_depth).unwrap();
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
        font_hash: FontHash, // Changed from FontRef - just store the hash
        font_size_px: f32,
        color: ColorU,
        clip_rect: WindowLogicalRect,
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
        /// Optional hit-test ID for WebRender hit-testing.
        /// If present, allows event handlers to identify which scrollbar component was clicked.
        hit_id: Option<azul_core::hit_test::ScrollbarHitId>,
    },
    /// A fully styled scrollbar with separate track, thumb, and optional buttons.
    /// Used when CSS scrollbar properties are specified.
    ScrollBarStyled {
        /// Complete drawing information for all scrollbar components
        info: Box<ScrollbarDrawInfo>,
    },

    /// An embedded IFrame that references a child DOM with its own display list.
    /// The renderer will look up the child display list by child_dom_id and
    /// render it within the bounds. The IFrame viewport is rendered in parent
    /// coordinate space (NOT inside a scroll frame) so it stays stationary.
    /// Scroll offset is communicated to the IFrame callback, not via WebRender.
    IFrame {
        /// The DomId of the child DOM (similar to webrender's pipeline_id)
        child_dom_id: DomId,
        /// The bounds where the IFrame should be rendered
        bounds: WindowLogicalRect,
        /// The clip rect for the IFrame content
        clip_rect: WindowLogicalRect,
    },

    /// Placeholder emitted during display list generation for IFrame nodes.
    /// `window.rs` replaces this with a real `IFrame` item after invoking
    /// the IFrame callback. This avoids the need for post-hoc scroll frame
    /// scanning — `window.rs` simply finds the placeholder by `node_id`.
    ///
    /// Unlike regular scrollable nodes, IFrame nodes do NOT get a
    /// PushScrollFrame/PopScrollFrame pair. Scroll state is managed by
    /// `ScrollManager` and passed to the IFrame callback as `scroll_offset`.
    IFramePlaceholder {
        /// The DOM NodeId of the IFrame element in the parent DOM
        node_id: NodeId,
        /// The layout bounds of the IFrame container
        bounds: WindowLogicalRect,
        /// The clip rect (same as bounds initially, may be adjusted)
        clip_rect: WindowLogicalRect,
    },

    // --- State-Management Commands ---
    /// Pushes a new clipping rectangle onto the renderer's clip stack.
    /// All subsequent primitives will be clipped by this rect until a PopClip.
    PushClip {
        bounds: WindowLogicalRect,
        border_radius: BorderRadius,
    },
    /// Pops the current clip from the renderer's clip stack.
    PopClip,

    /// Defines a scrollable area. This is a specialized clip that also
    /// establishes a new coordinate system for its children, which can be offset.
    PushScrollFrame {
        /// The clip rect in the parent's coordinate space.
        clip_bounds: WindowLogicalRect,
        /// The total size of the scrollable content.
        content_size: LogicalSize,
        /// An ID for the renderer to track this scrollable area between frames.
        scroll_id: LocalScrollId, // This would be a renderer-agnostic ID type
    },
    /// Pops the current scroll frame.
    PopScrollFrame,

    /// Pushes a new stacking context for proper z-index layering.
    /// All subsequent primitives until PopStackingContext will be in this stacking context.
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
        opacity: f32,
    },
    /// Pop an opacity layer.
    PopOpacity,

    /// Push a text shadow that applies to subsequent text content.
    PushTextShadow {
        shadow: azul_css::props::style::box_shadow::StyleBoxShadow,
    },
    /// Pop all text shadows.
    PopTextShadow,
}

// Helper structs for the DisplayList
#[derive(Debug, Copy, Clone, Default)]
pub struct BorderRadius {
    pub top_left: f32,
    pub top_right: f32,
    pub bottom_left: f32,
    pub bottom_right: f32,
}

impl BorderRadius {
    pub fn is_zero(&self) -> bool {
        self.top_left == 0.0
            && self.top_right == 0.0
            && self.bottom_left == 0.0
            && self.bottom_right == 0.0
    }
}

// Dummy types for compilation
pub type LocalScrollId = u64;
/// Display list tag ID as (payload, type_marker) tuple.
/// The u16 field is used as a namespace marker:
/// - 0x0100 = DOM Node (regular interactive elements)
/// - 0x0200 = Scrollbar component
pub type DisplayListTagId = (u64, u16);

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
    forced_page_breaks: Vec<f32>,
}

impl DisplayListBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_debug(debug_enabled: bool) -> Self {
        Self {
            items: Vec::new(),
            node_mapping: Vec::new(),
            current_node: None,
            debug_messages: Vec::new(),
            debug_enabled,
            forced_page_breaks: Vec::new(),
        }
    }

    /// Log a debug message if debug is enabled
    fn debug_log(&mut self, message: String) {
        if self.debug_enabled {
            self.debug_messages.push(LayoutDebugMessage::info(message));
        }
    }

    /// Build the display list and transfer debug messages to the provided option
    pub fn build_with_debug(
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
        }
    }

    /// Set the current node context for subsequent push operations
    pub fn set_current_node(&mut self, node_id: Option<NodeId>) {
        self.current_node = node_id;
    }

    /// Register a forced page break at the given Y position.
    /// This is used for CSS break-before: always and break-after: always.
    pub fn add_forced_page_break(&mut self, y_position: f32) {
        // Avoid duplicates and keep sorted
        if !self.forced_page_breaks.contains(&y_position) {
            self.forced_page_breaks.push(y_position);
            self.forced_page_breaks.sort_by(|a, b| a.partial_cmp(b).unwrap());
        }
    }

    /// Push an item and record its node mapping
    fn push_item(&mut self, item: DisplayListItem) {
        self.items.push(item);
        self.node_mapping.push(self.current_node);
    }

    pub fn build(self) -> DisplayList {
        DisplayList {
            items: self.items,
            node_mapping: self.node_mapping,
            forced_page_breaks: self.forced_page_breaks,
        }
    }

    pub fn push_hit_test_area(&mut self, bounds: LogicalRect, tag: DisplayListTagId) {
        self.push_item(DisplayListItem::HitTestArea { bounds: bounds.into(), tag });
    }

    /// Push a simple single-color scrollbar (legacy method).
    pub fn push_scrollbar(
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
    pub fn push_scrollbar_styled(&mut self, info: ScrollbarDrawInfo) {
        // Only push if at least the thumb or track is visible
        if info.thumb_color.a > 0 || info.track_color.a > 0 || info.opacity_key.is_some() {
            self.push_item(DisplayListItem::ScrollBarStyled {
                info: Box::new(info),
            });
        }
    }

    pub fn push_rect(&mut self, bounds: LogicalRect, color: ColorU, border_radius: BorderRadius) {
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
    /// - paint_node_background_and_border() for block elements
    /// - paint_inline_shape() for inline-block elements
    ///
    /// The backgrounds are painted in order (back to front per CSS spec), followed
    /// by the border.
    pub fn push_backgrounds_and_border(
        &mut self,
        bounds: LogicalRect,
        background_contents: &[azul_css::props::style::StyleBackgroundContent],
        border_info: &BorderInfo,
        simple_border_radius: BorderRadius,
        style_border_radius: StyleBorderRadius,
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
                StyleBackgroundContent::Image(_image_id) => {
                    // TODO: Implement image backgrounds
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
    /// Similar to push_backgrounds_and_border but uses InlineBorderInfo which stores
    /// pre-resolved pixel values instead of CSS property values. This is used for
    /// inline (display: inline) elements where the border info is computed during
    /// text layout and stored in the glyph runs.
    pub fn push_inline_backgrounds_and_border(
        &mut self,
        bounds: LogicalRect,
        background_color: Option<ColorU>,
        background_contents: &[azul_css::props::style::StyleBackgroundContent],
        border: Option<&crate::text3::cache::InlineBorderInfo>,
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
                StyleBackgroundContent::Image(_image_id) => {
                    // TODO: Implement image backgrounds for inline text
                }
            }
        }

        // Paint border if present
        if let Some(border) = border {
            if border.top > 0.0 || border.right > 0.0 || border.bottom > 0.0 || border.left > 0.0 {
                let border_widths = StyleBorderWidths {
                    top: Some(CssPropertyValue::Exact(LayoutBorderTopWidth {
                        inner: PixelValue::px(border.top),
                    })),
                    right: Some(CssPropertyValue::Exact(LayoutBorderRightWidth {
                        inner: PixelValue::px(border.right),
                    })),
                    bottom: Some(CssPropertyValue::Exact(LayoutBorderBottomWidth {
                        inner: PixelValue::px(border.bottom),
                    })),
                    left: Some(CssPropertyValue::Exact(LayoutBorderLeftWidth {
                        inner: PixelValue::px(border.left),
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
    pub fn push_linear_gradient(
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
    pub fn push_radial_gradient(
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
    pub fn push_conic_gradient(
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

    pub fn push_selection_rect(
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

    pub fn push_cursor_rect(&mut self, bounds: LogicalRect, color: ColorU) {
        if color.a > 0 {
            self.push_item(DisplayListItem::CursorRect { bounds: bounds.into(), color });
        }
    }
    pub fn push_clip(&mut self, bounds: LogicalRect, border_radius: BorderRadius) {
        self.push_item(DisplayListItem::PushClip {
            bounds: bounds.into(),
            border_radius,
        });
    }
    pub fn pop_clip(&mut self) {
        self.push_item(DisplayListItem::PopClip);
    }
    pub fn push_scroll_frame(
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
    pub fn pop_scroll_frame(&mut self) {
        self.push_item(DisplayListItem::PopScrollFrame);
    }
    pub fn push_iframe_placeholder(
        &mut self,
        node_id: NodeId,
        bounds: LogicalRect,
        clip_rect: LogicalRect,
    ) {
        self.push_item(DisplayListItem::IFramePlaceholder {
            node_id,
            bounds: bounds.into(),
            clip_rect: clip_rect.into(),
        });
    }
    pub fn push_border(
        &mut self,
        bounds: LogicalRect,
        widths: StyleBorderWidths,
        colors: StyleBorderColors,
        styles: StyleBorderStyles,
        border_radius: StyleBorderRadius,
    ) {
        // Check if any border side is visible
        let has_visible_border = {
            let has_width = widths.top.is_some()
                || widths.right.is_some()
                || widths.bottom.is_some()
                || widths.left.is_some();
            let has_style = styles.top.is_some()
                || styles.right.is_some()
                || styles.bottom.is_some()
                || styles.left.is_some();
            has_width && has_style
        };

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

    pub fn push_stacking_context(&mut self, z_index: i32, bounds: LogicalRect) {
        self.push_item(DisplayListItem::PushStackingContext { z_index, bounds: bounds.into() });
    }

    pub fn pop_stacking_context(&mut self) {
        self.push_item(DisplayListItem::PopStackingContext);
    }

    pub fn push_reference_frame(
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

    pub fn pop_reference_frame(&mut self) {
        self.push_item(DisplayListItem::PopReferenceFrame);
    }

    pub fn push_text_run(
        &mut self,
        glyphs: Vec<GlyphInstance>,
        font_hash: FontHash, // Just the hash, not the full FontRef
        font_size_px: f32,
        color: ColorU,
        clip_rect: LogicalRect,
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
            self.push_item(DisplayListItem::Text {
                glyphs,
                font_hash,
                font_size_px,
                color,
                clip_rect: clip_rect.into(),
            });
        } else {
            self.debug_log(format!(
                "[push_text_run] SKIPPED: glyphs.is_empty()={}, color.a={}",
                glyphs.is_empty(),
                color.a
            ));
        }
    }

    pub fn push_text_layout(
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

    pub fn push_underline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Underline {
                bounds: bounds.into(),
                color,
                thickness,
            });
        }
    }

    pub fn push_strikethrough(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Strikethrough {
                bounds: bounds.into(),
                color,
                thickness,
            });
        }
    }

    pub fn push_overline(&mut self, bounds: LogicalRect, color: ColorU, thickness: f32) {
        if color.a > 0 && thickness > 0.0 {
            self.push_item(DisplayListItem::Overline {
                bounds: bounds.into(),
                color,
                thickness,
            });
        }
    }

    pub fn push_image(&mut self, bounds: LogicalRect, image: ImageRef) {
        self.push_item(DisplayListItem::Image { bounds: bounds.into(), image });
    }
}

/// Main entry point for generating the display list.
pub fn generate_display_list<T: ParsedFontTrait + Sync + 'static>(
    ctx: &mut LayoutContext<T>,
    tree: &LayoutTree,
    calculated_positions: &super::PositionVec,
    scroll_offsets: &BTreeMap<NodeId, ScrollPosition>,
    scroll_ids: &BTreeMap<usize, u64>,
    gpu_value_cache: Option<&GpuValueCache>,
    renderer_resources: &RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
) -> Result<DisplayList> {
    debug_info!(
        ctx,
        "[DisplayList] generate_display_list: tree has {} nodes, {} positions calculated",
        tree.nodes.len(),
        calculated_positions.len()
    );

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
        let root_node = tree.get(tree.root);
        if let Some(root) = root_node {
            if let Some(root_dom_id) = root.dom_node_id {
                let root_state = generator.get_styled_node_state(root_dom_id);
                let canvas_bg = get_background_color(
                    generator.ctx.styled_dom,
                    root_dom_id,
                    &root_state,
                );
                if canvas_bg.a > 0 {
                    let viewport_rect = LogicalRect {
                        origin: LogicalPosition::zero(),
                        size: generator.ctx.viewport_size,
                    };
                    builder.push_rect(viewport_rect, canvas_bg, BorderRadius::default());
                    debug_info!(
                        generator.ctx,
                        "[DisplayList] Canvas background: color=({},{},{},{}), size={:?}",
                        canvas_bg.r, canvas_bg.g, canvas_bg.b, canvas_bg.a,
                        generator.ctx.viewport_size
                    );
                }
            }
        }
    }

    // 1. Build a tree of stacking contexts, which defines the global paint order.
    let stacking_context_tree = generator.collect_stacking_contexts(tree.root)?;

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
    scroll_ids: &'a BTreeMap<usize, u64>,
    gpu_value_cache: Option<&'a GpuValueCache>,
    renderer_resources: &'a RendererResources,
    id_namespace: IdNamespace,
    dom_id: DomId,
}

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
    pub fn new(
        ctx: &'a mut LayoutContext<'b, T>,
        scroll_offsets: &'a BTreeMap<NodeId, ScrollPosition>,
        positioned_tree: &'a PositionedTree<'a>,
        scroll_ids: &'a BTreeMap<usize, u64>,
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
        }
    }

    /// Helper to get styled node state for a node
    fn get_styled_node_state(&self, dom_id: NodeId) -> azul_core::styled_dom::StyledNodeState {
        self.ctx
            .styled_dom
            .styled_nodes
            .as_container()
            .get(dom_id)
            .map(|n| n.styled_node_state.clone())
            .unwrap_or_default()
    }

    /// Gets the cursor type for a text node from its CSS properties.
    /// Defaults to Text (I-beam) cursor if no explicit cursor is set.
    fn get_cursor_type_for_text_node(&self, node_id: NodeId) -> CursorType {
        use azul_css::props::style::effects::StyleCursor;
        
        let styled_node_state = self.get_styled_node_state(node_id);
        let node_data_container = self.ctx.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id);
        
        // Query the cursor CSS property for this text node
        if let Some(node_data) = node_data {
            if let Some(cursor_value) = self.ctx.styled_dom.get_css_property_cache().get_cursor(
                node_data,
                &node_id,
                &styled_node_state,
            ) {
                if let CssPropertyValue::Exact(cursor) = cursor_value {
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
        }
        
        // Default: Text cursor (I-beam) for text nodes
        CursorType::Text
    }

    /// Emits drawing commands for text selections only (not cursor).
    /// The cursor is drawn separately via `paint_cursor()`.
    fn paint_selections(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        
        // Get inline layout using the unified helper that handles IFC membership
        // This is critical: text nodes don't have their own inline_layout_result,
        // but they have ifc_membership pointing to their IFC root
        let Some(layout) = self.positioned_tree.tree.get_inline_layout_for_node(node_index) else {
            return Ok(());
        };

        // Get the absolute position of this node (border-box position)
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(node_index)
            .copied()
            .unwrap_or_default();

        // Selection rects are relative to content-box origin
        let padding = &node.box_props.padding;
        let border = &node.box_props.border;
        let content_box_offset_x = node_pos.x + padding.left + border.left;
        let content_box_offset_y = node_pos.y + padding.top + border.top;

        // Check if text is selectable (respects CSS user-select property)
        let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let is_selectable = super::getters::is_text_selectable(self.ctx.styled_dom, dom_id, node_state);
        
        if !is_selectable {
            return Ok(());
        }

        // === NEW: Check text_selections first (multi-node selection support) ===
        if let Some(text_selection) = self.ctx.text_selections.get(&self.ctx.styled_dom.dom_id) {
            if let Some(range) = text_selection.affected_nodes.get(&dom_id) {
                let is_collapsed = text_selection.is_collapsed();
                
                // Only draw selection highlight if NOT collapsed
                if !is_collapsed {
                    let rects = layout.get_selection_rects(range);
                    let style = get_selection_style(self.ctx.styled_dom, Some(dom_id), self.ctx.system_style.as_ref());

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

        // === LEGACY: Fall back to old selections for backward compatibility ===
        let Some(selection_state) = self.ctx.selections.get(&self.ctx.styled_dom.dom_id) else {
            return Ok(());
        };

        if selection_state.node_id.node.into_crate_internal() != Some(dom_id) {
            return Ok(());
        }

        for selection in selection_state.selections.as_slice() {
            if let Selection::Range(range) = &selection {
                let rects = layout.get_selection_rects(range);
                let style = get_selection_style(self.ctx.styled_dom, Some(dom_id), self.ctx.system_style.as_ref());

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
        }

        Ok(())
    }

    /// Emits drawing commands for the text cursor (caret) only.
    /// This is separate from selections and reads from `ctx.cursor_location`.
    fn paint_cursor(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        // Early exit if cursor is not visible (blinking off phase)
        if !self.ctx.cursor_is_visible {
            return Ok(());
        }
        
        // Early exit if no cursor location is set
        let Some((cursor_dom_id, cursor_node_id, cursor)) = &self.ctx.cursor_location else {
            return Ok(());
        };

        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };
        
        // Only paint cursor on the node that has the cursor
        if dom_id != *cursor_node_id {
            return Ok(());
        }
        
        // Check DOM ID matches
        if self.ctx.styled_dom.dom_id != *cursor_dom_id {
            return Ok(());
        }

        // Get inline layout using the unified helper that handles IFC membership
        // This is critical: text nodes don't have their own inline_layout_result,
        // but they have ifc_membership pointing to their IFC root
        let Some(layout) = self.positioned_tree.tree.get_inline_layout_for_node(node_index) else {
            return Ok(());
        };

        // Check if this node is contenteditable (or inherits contenteditable from ancestor)
        // Text nodes don't have contenteditable directly, but inherit it from their container
        let is_contenteditable = super::getters::is_node_contenteditable_inherited(self.ctx.styled_dom, dom_id);
        if !is_contenteditable {
            return Ok(());
        }
        
        // Check if text is selectable
        let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
        let is_selectable = super::getters::is_text_selectable(self.ctx.styled_dom, dom_id, node_state);
        if !is_selectable {
            return Ok(());
        }

        // Get cursor rect from text layout
        let Some(mut rect) = layout.get_cursor_rect(cursor) else {
            return Ok(());
        };

        // Get the absolute position of this node (border-box position)
        let node_pos = self
            .positioned_tree
            .calculated_positions
            .get(node_index)
            .copied()
            .unwrap_or_default();

        // Adjust to content-box coordinates
        let padding = &node.box_props.padding;
        let border = &node.box_props.border;
        let content_box_offset_x = node_pos.x + padding.left + border.left;
        let content_box_offset_y = node_pos.y + padding.top + border.top;

        rect.origin.x += content_box_offset_x;
        rect.origin.y += content_box_offset_y;

        let style = get_caret_style(self.ctx.styled_dom, Some(dom_id));
        
        // Apply caret width from CSS (default is 2px, get_cursor_rect returns 1px)
        rect.size.width = style.width;
        
        builder.push_cursor_rect(rect, style.color);

        Ok(())
    }

    /// Emits drawing commands for selection and cursor.
    /// Delegates to `paint_selections()` and `paint_cursor()`.
    fn paint_selection_and_cursor(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        self.paint_selections(builder, node_index)?;
        self.paint_cursor(builder, node_index)?;
        Ok(())
    }

    /// Recursively builds the tree of stacking contexts starting from a given layout node.
    fn collect_stacking_contexts(&mut self, node_index: usize) -> Result<StackingContext> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
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

        for &child_index in &node.children {
            if self.establishes_stacking_context(child_index) {
                child_contexts.push(self.collect_stacking_contexts(child_index)?);
            } else {
                in_flow_children.push(child_index);
            }
        }

        Ok(StackingContext {
            node_index,
            z_index,
            child_contexts,
            in_flow_children,
        })
    }

    /// Recursively traverses the stacking context tree, emitting drawing commands to the builder
    /// according to the CSS Painting Algorithm specification.
    fn generate_for_stacking_context(
        &mut self,
        builder: &mut DisplayListBuilder,
        context: &StackingContext,
    ) -> Result<()> {
        // Before painting the node, check if it establishes a new clip or scroll frame.
        let node = self
            .positioned_tree
            .tree
            .get(context.node_index)
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

        // Check if this node has a GPU-accelerated transform (CSS transform or drag).
        // If so, wrap in a reference frame so WebRender can animate it on the GPU.
        let has_reference_frame = node.dom_node_id.and_then(|dom_id| {
            self.gpu_value_cache.and_then(|cache| {
                let key = cache.transform_keys.get(&dom_id)?;
                let transform = cache.current_transform_values.get(&dom_id)?;
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
            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Opacity
            let opacity = self.ctx.styled_dom.css_property_cache.ptr
                .get_opacity(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| v.inner.normalized())
                .unwrap_or(1.0);

            if opacity < 1.0 {
                builder.push_item(DisplayListItem::PushOpacity {
                    bounds: node_bounds.into(),
                    opacity,
                });
                pushed_opacity = true;
            }

            // Filter
            if let Some(filter_vec_value) = self.ctx.styled_dom.css_property_cache.ptr
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
            if let Some(backdrop_filter_value) = self.ctx.styled_dom.css_property_cache.ptr
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

        // 1. Paint background and borders for the context's root element.
        // This must be BEFORE push_node_clips so the container background
        // is rendered in parent space (stationary), not scroll space.
        self.paint_node_background_and_border(builder, context.node_index)?;

        // 1b. For scrollable containers, push the hit-test area BEFORE the scroll frame
        // so the hit-test covers the entire container box (including visible area),
        // not just the scrolled content. This ensures scroll wheel events hit the
        // container regardless of scroll position.
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

        // 2. Push clips and scroll frames AFTER painting background
        let did_push_clip_or_scroll = self.push_node_clips(builder, context.node_index, node)?;

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

        // 5. Paint child stacking contexts with z-index: 0 / auto.
        for child in context.child_contexts.iter().filter(|c| c.z_index == 0) {
            self.generate_for_stacking_context(builder, child)?;
        }

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

        // After painting the node and all its descendants, pop any contexts it pushed.
        // For IFrame nodes, emit the placeholder INSIDE the clip (before PopClip)
        // so the iframe viewport is clipped to the container.
        if did_push_clip_or_scroll {
            // Emit IFramePlaceholder before popping the clip so it's inside PushClip/PopClip
            if let Some(dom_id) = node.dom_node_id {
                if self.is_iframe_node(dom_id) {
                    builder.push_iframe_placeholder(dom_id, node_bounds, node_bounds);
                }
            }
            self.pop_node_clips(builder, node)?;
        } else {
            // Even without clips, emit IFramePlaceholder for IFrame nodes
            if let Some(dom_id) = node.dom_node_id {
                if self.is_iframe_node(dom_id) {
                    builder.push_iframe_placeholder(dom_id, node_bounds, node_bounds);
                }
            }
        }

        // Paint scrollbars AFTER popping the clip, so they appear on top of content
        // and are not clipped by the scroll frame
        self.paint_scrollbars(builder, context.node_index)?;

        Ok(())
    }

    /// Paints the content and non-stacking-context children.
    fn paint_in_flow_descendants(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        children_indices: &[usize],
    ) -> Result<()> {
        // NOTE: We do NOT paint the node's background here - that was already done by
        // generate_for_stacking_context! Only paint selection, cursor, and content for the
        // current node

        // 2. Paint selection highlights and the text cursor if applicable.
        self.paint_selection_and_cursor(builder, node_index)?;

        // 3. Paint the node's own content (text, images, hit-test areas).
        self.paint_node_content(builder, node_index)?;

        // 4. Recursively paint the in-flow children in correct CSS painting order:
        //    - First: Non-float, non-dragging block-level children
        //    - Then: Float, non-dragging children (so they appear on top)
        //    - Finally: Dragging children (so they appear on top of everything per W3C spec)

        // Separate children into floats, non-floats, and dragging
        let mut non_float_children = Vec::new();
        let mut float_children = Vec::new();
        let mut dragging_children = Vec::new();

        for &child_index in children_indices {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child is being dragged (paint last for z-order)
            let is_dragging = if let Some(dom_id) = child_node.dom_node_id {
                let styled_node_state = self.get_styled_node_state(dom_id);
                styled_node_state.dragging
            } else {
                false
            };

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
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child has a GPU transform (CSS transform or drag)
            let child_ref_frame = child_node.dom_node_id.and_then(|dom_id| {
                self.gpu_value_cache.and_then(|cache| {
                    let key = cache.transform_keys.get(&dom_id)?;
                    let transform = cache.current_transform_values.get(&dom_id)?;
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

            // IMPORTANT: Paint background and border BEFORE pushing clips!
            // This ensures the container's background is in parent space (stationary),
            // not in scroll space. Same logic as generate_for_stacking_context.
            self.paint_node_background_and_border(builder, child_index)?;

            // Push clips and scroll frames AFTER painting background
            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;

            // Paint descendants inside the clip/scroll frame
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

            // For IFrame children: emit placeholder INSIDE the clip
            if let Some(dom_id) = child_node.dom_node_id {
                if self.is_iframe_node(dom_id) {
                    let child_bounds = self.get_paint_rect(child_index).unwrap_or_default();
                    builder.push_iframe_placeholder(dom_id, child_bounds, child_bounds);
                }
            }

            // Pop the child's clips.
            if did_push_clip {
                self.pop_node_clips(builder, child_node)?;
            }

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;

            // Pop reference frame if we pushed one
            if child_ref_frame.is_some() {
                builder.pop_reference_frame();
            }
        }

        // Paint float children AFTER non-floats (so they appear on top)
        for child_index in float_children {
            let child_node = self
                .positioned_tree
                .tree
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child has a GPU transform (CSS transform or drag)
            let child_ref_frame = child_node.dom_node_id.and_then(|dom_id| {
                self.gpu_value_cache.and_then(|cache| {
                    let key = cache.transform_keys.get(&dom_id)?;
                    let transform = cache.current_transform_values.get(&dom_id)?;
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

            // Same as above: paint background BEFORE clips
            self.paint_node_background_and_border(builder, child_index)?;
            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

            // For IFrame children: emit placeholder INSIDE the clip
            if let Some(dom_id) = child_node.dom_node_id {
                if self.is_iframe_node(dom_id) {
                    let child_bounds = self.get_paint_rect(child_index).unwrap_or_default();
                    builder.push_iframe_placeholder(dom_id, child_bounds, child_bounds);
                }
            }

            if did_push_clip {
                self.pop_node_clips(builder, child_node)?;
            }

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
                .get(child_index)
                .ok_or(LayoutError::InvalidTree)?;

            // Check if this child has a GPU transform (CSS transform or drag)
            let child_ref_frame = child_node.dom_node_id.and_then(|dom_id| {
                self.gpu_value_cache.and_then(|cache| {
                    let key = cache.transform_keys.get(&dom_id)?;
                    let transform = cache.current_transform_values.get(&dom_id)?;
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

            // Same as above: paint background BEFORE clips
            self.paint_node_background_and_border(builder, child_index)?;
            let did_push_clip = self.push_node_clips(builder, child_index, child_node)?;
            self.paint_in_flow_descendants(builder, child_index, &child_node.children)?;

            // For IFrame children: emit placeholder INSIDE the clip
            if let Some(dom_id) = child_node.dom_node_id {
                if self.is_iframe_node(dom_id) {
                    let child_bounds = self.get_paint_rect(child_index).unwrap_or_default();
                    builder.push_iframe_placeholder(dom_id, child_bounds, child_bounds);
                }
            }

            if did_push_clip {
                self.pop_node_clips(builder, child_node)?;
            }

            // Paint scrollbars AFTER popping clips so they appear on top of content
            self.paint_scrollbars(builder, child_index)?;

            // Pop reference frame if we pushed one
            if child_ref_frame.is_some() {
                builder.pop_reference_frame();
            }
        }

        Ok(())
    }

    /// Returns true if the given DOM node is an IFrame node.
    fn is_iframe_node(&self, dom_id: NodeId) -> bool {
        let node_data_container = self.ctx.styled_dom.node_data.as_container();
        node_data_container
            .get(dom_id)
            .map(|nd| matches!(nd.get_node_type(), NodeType::IFrame(_)))
            .unwrap_or(false)
    }

    /// Checks if a node requires clipping or scrolling and pushes the appropriate commands.
    /// Returns true if any command was pushed.
    ///
    /// For IFrame nodes with `overflow: scroll/auto`, we intentionally skip
    /// `PushScrollFrame` / `PopScrollFrame`. IFrame scroll state is managed by
    /// `ScrollManager`, not WebRender's APZ. Instead we emit only a `PushClip`
    /// and later an `IFramePlaceholder` (see `generate_for_stacking_context`).
    fn push_node_clips(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
        node: &LayoutNode,
    ) -> Result<bool> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(false);
        };

        let styled_node_state = self.get_styled_node_state(dom_id);

        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);

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

        let needs_clip = overflow_x.is_clipped() || overflow_y.is_clipped();

        if !needs_clip {
            return Ok(false);
        }

        let paint_rect = self.get_paint_rect(node_index).unwrap_or_default();

        let border = &node.box_props.border;

        // Get scrollbar info to adjust clip rect for content area
        let scrollbar_info = get_scrollbar_info_from_layout(node);

        // The clip rect for content should exclude the scrollbar area
        // Scrollbars are drawn inside the border-box, on the right/bottom edges
        let clip_rect = LogicalRect {
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

        let is_iframe = self.is_iframe_node(dom_id);

        if overflow_x.is_scroll() || overflow_y.is_scroll() {
            if is_iframe {
                // IFrame nodes: only push a clip, NO scroll frame.
                // Scroll state is managed by ScrollManager and passed to the
                // IFrame callback as scroll_offset. The IFramePlaceholder is
                // emitted after pop_node_clips in generate_for_stacking_context.
                builder.push_clip(clip_rect, border_radius);
            } else {
                // Regular scrollable nodes: push clip AND scroll frame.
                // WebRender's APZ manages the scroll offset via define_scroll_frame.
                builder.push_clip(clip_rect, border_radius);
                let scroll_id = self.scroll_ids.get(&node_index).copied().unwrap_or(0);
                let content_size = get_scroll_content_size(node);
                builder.push_scroll_frame(clip_rect, content_size, scroll_id);
            }
        } else {
            // Simple clip for hidden/clip
            builder.push_clip(clip_rect, border_radius);
        }

        Ok(true)
    }

    /// Pops any clip/scroll commands associated with a node.
    fn pop_node_clips(&self, builder: &mut DisplayListBuilder, node: &LayoutNode) -> Result<()> {
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let overflow_x = get_overflow_x(self.ctx.styled_dom, dom_id, &styled_node_state);
        let overflow_y = get_overflow_y(self.ctx.styled_dom, dom_id, &styled_node_state);

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

        let needs_clip =
            overflow_x.is_clipped() || overflow_y.is_clipped() || !border_radius.is_zero();

        let is_iframe = self.is_iframe_node(dom_id);

        if needs_clip {
            if (overflow_x.is_scroll() || overflow_y.is_scroll()) && !is_iframe {
                // For regular (non-IFrame) scroll/auto, pop both scroll frame AND clip
                builder.pop_scroll_frame();
                builder.pop_clip();
            } else {
                // For hidden/clip, or IFrame scroll (which only pushed a clip)
                builder.pop_clip();
            }
        }
        Ok(())
    }

    /// Calculates the final paint-time rectangle for a node.
    /// 
    /// ## Coordinate Space
    /// 
    /// Returns the node's position in **absolute window coordinates** (logical pixels).
    /// This is the coordinate space used throughout the display list:
    /// 
    /// - Origin: Top-left corner of the window
    /// - Units: Logical pixels (HiDPI scaling happens in compositor2.rs)
    /// - Scroll: NOT applied here - WebRender scroll frames handle scroll offset
    ///   transformation internally via `define_scroll_frame()`
    /// 
    /// ## Important
    /// 
    /// Do NOT manually subtract scroll offset here! WebRender's scroll spatial
    /// transforms handle this. Subtracting here would cause double-offset and
    /// parallax effects (backgrounds and text moving at different speeds).
    fn get_paint_rect(&self, node_index: usize) -> Option<LogicalRect> {
        let node = self.positioned_tree.tree.get(node_index)?;
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
    fn paint_node_background_and_border(
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
            .get(node_index)
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
                builder.add_forced_page_break(y_position);
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
                builder.add_forced_page_break(y_position);
                debug_info!(
                    self.ctx,
                    "Registered forced page break AFTER node {} at y={}",
                    node_index,
                    y_position
                );
            }
        }

        // Skip inline and inline-block elements ONLY if they participate in an IFC (Inline Formatting Context).
        // In Flex or Grid containers, inline-block elements are treated as flex/grid items and must be painted here.
        // Inline elements participate in inline formatting context and their backgrounds
        // must be positioned by the text layout engine, not the block layout engine
        //
        // IMPORTANT: The parent check must look at the PARENT NODE's formatting_context,
        // not the current node's. If parent is Flex/Grid, we paint this element as a flex/grid item.
        // Also check parent_formatting_context field which stores parent's FC during tree construction.
        let parent_is_flex_or_grid = node.parent_formatting_context
            .as_ref()
            .map(|fc| matches!(fc, FormattingContext::Flex | FormattingContext::Grid))
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
                    node.parent_formatting_context,
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

        let border_radius = if let Some(dom_id) = node.dom_node_id {
            let styled_node_state = self.get_styled_node_state(dom_id);
            let background_contents =
                get_background_contents(self.ctx.styled_dom, dom_id, &styled_node_state);
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
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Check all four sides for box-shadow (azul stores them per-side)
            for get_shadow_fn in [
                azul_core::prop_cache::CssPropertyCache::get_box_shadow_left,
                azul_core::prop_cache::CssPropertyCache::get_box_shadow_right,
                azul_core::prop_cache::CssPropertyCache::get_box_shadow_top,
                azul_core::prop_cache::CssPropertyCache::get_box_shadow_bottom,
            ] {
                if let Some(shadow_value) = get_shadow_fn(
                    &self.ctx.styled_dom.css_property_cache.ptr,
                    node_data,
                    &dom_id,
                    &node_state,
                ) {
                    if let Some(shadow) = shadow_value.get_property() {
                        builder.push_item(DisplayListItem::BoxShadow {
                            bounds: paint_rect.into(),
                            shadow: shadow.clone(),
                            border_radius: simple_border_radius,
                        });
                    }
                }
            }

            // Use unified background/border painting
            builder.push_backgrounds_and_border(
                paint_rect,
                &background_contents,
                &border_info,
                simple_border_radius,
                style_border_radius,
            );

            simple_border_radius
        } else {
            BorderRadius::default()
        };

        Ok(())
    }

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
    /// CSS paint order, making WebRender integration trivial.
    fn paint_table_items(
        &self,
        builder: &mut DisplayListBuilder,
        table_index: usize,
    ) -> Result<()> {
        let table_node = self
            .positioned_tree
            .tree
            .get(table_index)
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
        for &child_idx in &table_node.children {
            let child_node = self.positioned_tree.tree.get(child_idx);
            if let Some(node) = child_node {
                if matches!(node.formatting_context, FormattingContext::TableColumnGroup) {
                    // Paint column group background
                    self.paint_element_background(builder, child_idx)?;

                    // Paint backgrounds of individual columns within this group
                    for &col_idx in &node.children {
                        self.paint_element_background(builder, col_idx)?;
                    }
                }
            }
        }

        // Layer 4: Row group backgrounds (tbody, thead, tfoot)
        // Layer 5: Row backgrounds
        // Layer 6: Cell backgrounds
        for &child_idx in &table_node.children {
            let child_node = self.positioned_tree.tree.get(child_idx);
            if let Some(node) = child_node {
                match node.formatting_context {
                    FormattingContext::TableRowGroup => {
                        // Paint row group background
                        self.paint_element_background(builder, child_idx)?;

                        // Paint rows within this group
                        for &row_idx in &node.children {
                            self.paint_table_row_and_cells(builder, row_idx)?;
                        }
                    }
                    FormattingContext::TableRow => {
                        // Direct row child (no row group wrapper)
                        self.paint_table_row_and_cells(builder, child_idx)?;
                    }
                    _ => {}
                }
            }
        }

        // Borders are painted separately after all backgrounds
        // This is handled by the normal rendering flow for each element
        // TODO: Implement border-collapse conflict resolution using BorderInfo::resolve_conflict()

        Ok(())
    }

    /// Helper function to paint a table row's background and then its cells' backgrounds
    /// Layer 5: Row background
    /// Layer 6: Cell backgrounds (painted after row, so they appear on top)
    fn paint_table_row_and_cells(
        &self,
        builder: &mut DisplayListBuilder,
        row_idx: usize,
    ) -> Result<()> {
        // Layer 5: Paint row background
        self.paint_element_background(builder, row_idx)?;

        // Layer 6: Paint cell backgrounds (topmost layer)
        let row_node = self.positioned_tree.tree.get(row_idx);
        if let Some(node) = row_node {
            for &cell_idx in &node.children {
                self.paint_element_background(builder, cell_idx)?;
            }
        }

        Ok(())
    }

    /// Helper function to paint an element's background (used for all table elements)
    /// Reads background-color and border-radius from CSS properties and emits push_rect()
    fn paint_element_background(
        &self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        let Some(node) = self.positioned_tree.tree.get(node_index) else {
            return Ok(());
        };
        let Some(dom_id) = node.dom_node_id else {
            return Ok(());
        };

        let styled_node_state = self.get_styled_node_state(dom_id);
        let bg_color = get_background_color(self.ctx.styled_dom, dom_id, &styled_node_state);

        // Only paint if background color has alpha > 0 (optimization)
        if bg_color.a == 0 {
            return Ok(());
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

        Ok(())
    }

    /// Emits drawing commands for the foreground content, including hit-test areas and scrollbars.
    fn paint_node_content(
        &mut self,
        builder: &mut DisplayListBuilder,
        node_index: usize,
    ) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        // Set current node for node mapping (for pagination break properties)
        builder.set_current_node(node.dom_node_id);

        let Some(mut paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // For text nodes (with inline layout), the used_size might be 0x0.
        // In this case, compute the bounds from the inline layout result.
        if paint_rect.size.width == 0.0 || paint_rect.size.height == 0.0 {
            if let Some(cached_layout) = &node.inline_layout_result {
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
        if let Some(cached_layout) = &node.inline_layout_result {
            let inline_layout = &cached_layout.layout;
            debug_info!(
                self.ctx,
                "[paint_node] node {} has inline_layout with {} items",
                node_index,
                inline_layout.items.len()
            );

            if let Some(dom_id) = node.dom_node_id {
                let node_type = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                debug_info!(
                    self.ctx,
                    "Painting inline content for node {} ({:?}) at {:?}, {} layout items",
                    node_index,
                    node_type.get_node_type(),
                    paint_rect,
                    inline_layout.items.len()
                );
            }

            // paint_rect is the border-box, but inline layout positions are relative to
            // content-box. Use type-safe conversion to make this clear and avoid manual
            // calculations.
            let border_box = BorderBoxRect(paint_rect);
            let mut content_box_rect =
                border_box.to_content_box(&node.box_props.padding, &node.box_props.border).rect();
            
            // For scrollable containers, extend the content rect to the full content size.
            // The scroll frame handles clipping - we need to paint ALL content, not just
            // what fits in the viewport. Otherwise glyphs beyond the viewport are not rendered.
            let content_size = get_scroll_content_size(node);
            if content_size.height > content_box_rect.size.height {
                content_box_rect.size.height = content_size.height;
            }
            if content_size.width > content_box_rect.size.width {
                content_box_rect.size.width = content_size.width;
            }

            // Check for text-shadow and wrap inline content with push/pop shadow
            let mut pushed_text_shadow = false;
            if let Some(dom_id) = node.dom_node_id {
                let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
                let node_state = &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;
                if let Some(shadow_val) = self.ctx.styled_dom.css_property_cache.ptr
                    .get_text_shadow(node_data, &dom_id, node_state)
                {
                    if let Some(shadow) = shadow_val.get_property() {
                        builder.push_item(DisplayListItem::PushTextShadow {
                            shadow: shadow.clone(),
                        });
                        pushed_text_shadow = true;
                    }
                }
            }

            self.paint_inline_content(builder, content_box_rect, inline_layout)?;

            if pushed_text_shadow {
                builder.push_item(DisplayListItem::PopTextShadow);
            }
        } else if let Some(dom_id) = node.dom_node_id {
            // This node might be a simple replaced element, like an <img> tag.
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            if let NodeType::Image(image_ref) = node_data.get_node_type() {
                debug_info!(
                    self.ctx,
                    "Painting image for node {} at {:?}",
                    node_index,
                    paint_rect
                );
                // Store the ImageRef directly in the display list
                builder.push_image(paint_rect, image_ref.clone());
            }
        }

        Ok(())
    }

    /// Emits drawing commands for scrollbars. This is called AFTER popping the scroll frame
    /// clip so scrollbars appear on top of content and are not clipped.
    fn paint_scrollbars(&self, builder: &mut DisplayListBuilder, node_index: usize) -> Result<()> {
        let node = self
            .positioned_tree
            .tree
            .get(node_index)
            .ok_or(LayoutError::InvalidTree)?;

        let Some(paint_rect) = self.get_paint_rect(node_index) else {
            return Ok(());
        };

        // Check if we need to draw scrollbars for this node.
        let scrollbar_info = get_scrollbar_info_from_layout(node);

        // Get node_id for GPU cache lookup and CSS style lookup
        let node_id = node.dom_node_id;

        // Get CSS scrollbar style for this node
        let scrollbar_style = node_id
            .map(|nid| {
                let node_state =
                    &self.ctx.styled_dom.styled_nodes.as_container()[nid].styled_node_state;
                get_scrollbar_style(self.ctx.styled_dom, nid, node_state)
            })
            .unwrap_or_default();

        // Skip if scrollbar-width: none
        if matches!(
            scrollbar_style.width_mode,
            azul_css::props::style::scrollbar::LayoutScrollbarWidth::None
        ) {
            return Ok(());
        }

        // Get border dimensions to position scrollbar inside the border-box
        let border = &node.box_props.border;

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

        // Get scroll position for thumb calculation
        // ScrollPosition contains parent_rect and children_rect
        // The scroll offset is the difference between children_rect.origin and parent_rect.origin
        let (scroll_offset_x, scroll_offset_y) = node_id
            .and_then(|nid| {
                self.scroll_offsets.get(&nid).map(|pos| {
                    (
                        pos.children_rect.origin.x - pos.parent_rect.origin.x,
                        pos.children_rect.origin.y - pos.parent_rect.origin.y,
                    )
                })
            })
            .unwrap_or((0.0, 0.0));

        // Get content size for thumb proportional sizing
        // Use the node's get_content_size() method which returns the actual content size
        // from overflow_content_size (set during layout) or computes it from text/children.
        // This is critical for correct thumb sizing - we must NOT use arbitrary multipliers.
        let content_size = node.get_content_size();

        // Calculate thumb border-radius (half the scrollbar width for pill-shaped thumb)
        let thumb_radius = scrollbar_style.width_px / 2.0;
        let thumb_border_radius = BorderRadius {
            top_left: thumb_radius,
            top_right: thumb_radius,
            bottom_left: thumb_radius,
            bottom_right: thumb_radius,
        };

        if scrollbar_info.needs_vertical {
            // Look up opacity key from GPU cache
            let opacity_key = node_id.and_then(|nid| {
                self.gpu_value_cache.and_then(|cache| {
                    cache
                        .scrollbar_v_opacity_keys
                        .get(&(self.dom_id, nid))
                        .copied()
                })
            });

            // Vertical scrollbar: use shared geometry computation
            let v_geom = compute_scrollbar_geometry(
                ScrollbarOrientation::Vertical,
                inner_rect,
                content_size,
                scroll_offset_y,
                scrollbar_style.width_px,
                scrollbar_info.needs_horizontal,
            );

            // Position thumb after the top button; GPU transform moves it within usable track
            let thumb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    v_geom.track_rect.origin.x,
                    v_geom.track_rect.origin.y + v_geom.button_size,
                ),
                size: LogicalSize::new(v_geom.width_px, v_geom.thumb_length),
            };

            // Look up transform key from GPU cache for GPU-animated thumb positioning.
            // If a key already exists in the cache from a previous frame, reuse it.
            // Otherwise, create a new unique key. The key will be registered
            // in the GPU cache after layout_document returns.
            let thumb_transform_key = node_id.map(|nid| {
                self.gpu_value_cache
                    .and_then(|cache| cache.transform_keys.get(&nid).copied())
                    .unwrap_or_else(|| TransformKey::unique())
            });

            // Initial transform: translate thumb within usable region
            let thumb_initial_transform =
                ComputedTransform3D::new_translation(0.0, v_geom.thumb_offset, 0.0);

            // Generate hit-test ID for vertical scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::VerticalThumb(self.dom_id, nid));

            // Buttons at top/bottom of track
            let button_decrement_bounds = Some(LogicalRect {
                origin: v_geom.track_rect.origin,
                size: LogicalSize::new(v_geom.button_size, v_geom.button_size),
            });
            let button_increment_bounds = Some(LogicalRect {
                origin: LogicalPosition::new(
                    v_geom.track_rect.origin.x,
                    v_geom.track_rect.origin.y + v_geom.track_rect.size.height - v_geom.button_size,
                ),
                size: LogicalSize::new(v_geom.button_size, v_geom.button_size),
            });
            // Light green color for debug visibility
            let debug_button_color = ColorU { r: 144, g: 238, b: 144, a: 255 };

            builder.push_scrollbar_styled(ScrollbarDrawInfo {
                bounds: v_geom.track_rect.into(),
                orientation: ScrollbarOrientation::Vertical,
                track_bounds: v_geom.track_rect.into(),
                track_color: scrollbar_style.track_color,
                thumb_bounds: thumb_bounds.into(),
                thumb_color: scrollbar_style.thumb_color,
                thumb_border_radius,
                button_decrement_bounds: button_decrement_bounds.map(|b| b.into()),
                button_increment_bounds: button_increment_bounds.map(|b| b.into()),
                button_color: debug_button_color,
                opacity_key,
                thumb_transform_key,
                thumb_initial_transform,
                hit_id,
                clip_to_container_border: scrollbar_style.clip_to_container_border,
                container_border_radius,
            });
        }

        if scrollbar_info.needs_horizontal {
            // Look up opacity key from GPU cache
            let opacity_key = node_id.and_then(|nid| {
                self.gpu_value_cache.and_then(|cache| {
                    cache
                        .scrollbar_h_opacity_keys
                        .get(&(self.dom_id, nid))
                        .copied()
                })
            });

            // Horizontal scrollbar: use shared geometry computation
            let h_geom = compute_scrollbar_geometry(
                ScrollbarOrientation::Horizontal,
                inner_rect,
                content_size,
                scroll_offset_x,
                scrollbar_style.width_px,
                scrollbar_info.needs_vertical,
            );

            // Position thumb after the left button; GPU transform moves it within usable track
            let thumb_bounds = LogicalRect {
                origin: LogicalPosition::new(
                    h_geom.track_rect.origin.x + h_geom.button_size,
                    h_geom.track_rect.origin.y,
                ),
                size: LogicalSize::new(h_geom.thumb_length, h_geom.width_px),
            };

            // Look up horizontal transform key from GPU cache for GPU-animated thumb positioning.
            let thumb_transform_key = node_id.map(|nid| {
                self.gpu_value_cache
                    .and_then(|cache| cache.h_transform_keys.get(&nid).copied())
                    .unwrap_or_else(|| TransformKey::unique())
            });
            let thumb_initial_transform =
                ComputedTransform3D::new_translation(h_geom.thumb_offset, 0.0, 0.0);

            // Generate hit-test ID for horizontal scrollbar thumb
            let hit_id = node_id
                .map(|nid| azul_core::hit_test::ScrollbarHitId::HorizontalThumb(self.dom_id, nid));

            // Buttons at left/right of track
            let button_decrement_bounds = Some(LogicalRect {
                origin: h_geom.track_rect.origin,
                size: LogicalSize::new(h_geom.button_size, h_geom.button_size),
            });
            let button_increment_bounds = Some(LogicalRect {
                origin: LogicalPosition::new(
                    h_geom.track_rect.origin.x + h_geom.track_rect.size.width - h_geom.button_size,
                    h_geom.track_rect.origin.y,
                ),
                size: LogicalSize::new(h_geom.button_size, h_geom.button_size),
            });
            // Light green color for debug visibility
            let debug_button_color = ColorU { r: 144, g: 238, b: 144, a: 255 };

            builder.push_scrollbar_styled(ScrollbarDrawInfo {
                bounds: h_geom.track_rect.into(),
                orientation: ScrollbarOrientation::Horizontal,
                track_bounds: h_geom.track_rect.into(),
                track_color: scrollbar_style.track_color,
                thumb_bounds: thumb_bounds.into(),
                thumb_color: scrollbar_style.thumb_color,
                thumb_border_radius,
                button_decrement_bounds: button_decrement_bounds.map(|b| b.into()),
                button_increment_bounds: button_increment_bounds.map(|b| b.into()),
                button_color: debug_button_color,
                opacity_key,
                thumb_transform_key,
                thumb_initial_transform,
                hit_id,
                clip_to_container_border: scrollbar_style.clip_to_container_border,
                container_border_radius,
            });
        }

        Ok(())
    }

    /// Converts the rich layout information from `text3` into drawing commands.
    fn paint_inline_content(
        &self,
        builder: &mut DisplayListBuilder,
        container_rect: LogicalRect,
        layout: &UnifiedLayout,
    ) -> Result<()> {
        // TODO: This will always paint images over the glyphs
        // TODO: Handle z-index within inline content (e.g. background images)
        // NOTE: Text decorations (underline, strikethrough, overline) are handled in push_text_layout_to_display_list
        // TODO: Text shadows not yet implemented
        // TODO: Handle text overflowing (based on container_rect and overflow behavior)

        // Calculate actual content bounds from the layout
        // Use these bounds instead of container_rect to avoid inflated bounds
        // that extend beyond actual text content
        let layout_bounds = layout.bounds();
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
        if layout_bounds.width > 0.0 || layout_bounds.height > 0.0 {
            builder.push_text_layout(
                Arc::new(layout.clone()) as Arc<dyn std::any::Any + Send + Sync>,
                actual_bounds,
                FontHash::from_hash(0), // Will be updated per glyph run
                12.0,                   // Default font size, will be updated per glyph run
                ColorU {
                    r: 0,
                    g: 0,
                    b: 0,
                    a: 255,
                }, // Default color
            );
        }

        let glyph_runs = crate::text3::glyphs::get_glyph_runs_simple(layout);

        // FIRST PASS: Render backgrounds (solid colors, gradients) and borders for each glyph run
        // This must happen BEFORE rendering text so that backgrounds appear behind text.
        for glyph_run in glyph_runs.iter() {
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
                let ascent = font_size * 0.8; // Approximate ascent

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

                // Use unified inline background/border painting
                builder.push_inline_backgrounds_and_border(
                    run_bounds,
                    glyph_run.background_color,
                    &glyph_run.background_content,
                    glyph_run.border.as_ref(),
                );
            }
        }

        // SECOND PASS: Render text runs
        for (idx, glyph_run) in glyph_runs.iter().enumerate() {
            let clip_rect = container_rect; // Clip to the container rect

            // Fix: Offset glyph positions by the container origin.
            // Text layout is relative to (0,0) of the IFC, but we need absolute coordinates.
            let offset_glyphs: Vec<GlyphInstance> = glyph_run
                .glyphs
                .iter()
                .map(|g| {
                    let mut g = g.clone();
                    g.point.x += container_rect.origin.x;
                    g.point.y += container_rect.origin.y;
                    g
                })
                .collect();

            // Store only the font hash in the display list to keep it lean
            builder.push_text_run(
                offset_glyphs,
                FontHash::from_hash(glyph_run.font_hash),
                glyph_run.font_size_px,
                glyph_run.color,
                clip_rect,
            );

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
                    let thickness = (font_size * 0.08).max(1.0); // ~8% of font size, min 1px

                    // Baseline is at glyph.point.y
                    let baseline_y = container_rect.origin.y + first_glyph.point.y;

                    if needs_underline {
                        // Underline is typically 10-15% below baseline
                        // IME composition always gets underlined
                        let underline_y = baseline_y + (font_size * 0.12);
                        let underline_bounds = LogicalRect::new(
                            LogicalPosition::new(decoration_start_x, underline_y),
                            LogicalSize::new(decoration_width, thickness),
                        );
                        builder.push_underline(underline_bounds, glyph_run.color, thickness);
                    }

                    if needs_strikethrough {
                        // Strikethrough is typically 40% above baseline (middle of x-height)
                        let strikethrough_y = baseline_y - (font_size * 0.3);
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
                        let overline_y = baseline_y - (font_size * 0.85);
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
        for glyph_run in glyph_runs.iter() {
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
                let ascent = font_size * 0.8; // Approximate ascent

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
                let tag_value = ((self.dom_id.inner as u64) << 32) | (source_node_id.index() as u64);
                let tag_type = TAG_TYPE_CURSOR | (cursor_type as u16);
                let tag_id = (tag_value, tag_type);

                builder.push_hit_test_area(run_bounds, tag_id);
            }
        }

        // Render inline objects (images, shapes/inline-blocks, etc.)
        // These are positioned by the text3 engine and need to be rendered at their calculated
        // positions
        for positioned_item in &layout.items {
            self.paint_inline_object(builder, container_rect.origin, positioned_item)?;
        }
        Ok(())
    }

    /// Paints a single inline object (image, shape, or inline-block)
    fn paint_inline_object(
        &self,
        builder: &mut DisplayListBuilder,
        base_pos: LogicalPosition,
        positioned_item: &PositionedItem,
    ) -> Result<()> {
        let ShapedItem::Object {
            content, bounds, ..
        } = &positioned_item.item
        else {
            // Other item types (e.g., breaks) don't produce painted output.
            return Ok(());
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
                if let Some(image_ref) = get_image_ref_for_image_source(&image.source) {
                    builder.push_image(object_bounds, image_ref);
                }
            }
            InlineContent::Shape(shape) => {
                self.paint_inline_shape(builder, object_bounds, shape, bounds)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Paints an inline shape (inline-block background and border)
    fn paint_inline_shape(
        &self,
        builder: &mut DisplayListBuilder,
        object_bounds: LogicalRect,
        shape: &InlineShape,
        bounds: &crate::text3::cache::Rect,
    ) -> Result<()> {
        // Render inline-block backgrounds and borders using their CSS styling
        // The text3 engine positions these correctly in the inline flow
        let Some(node_id) = shape.source_node_id else {
            return Ok(());
        };

        // If this inline-block establishes a stacking context, its background was
        // already painted by paint_node_background_and_border (called from
        // generate_for_stacking_context). Painting again here would cause
        // double-rendering. Skip it.
        if let Some(indices) = self.positioned_tree.tree.dom_to_layout.get(&node_id) {
            if let Some(&idx) = indices.first() {
                if self.establishes_stacking_context(idx) {
                    return Ok(());
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
        let margins = if let Some(indices) = self.positioned_tree.tree.dom_to_layout.get(&node_id) {
            if let Some(&idx) = indices.first() {
                self.positioned_tree.tree.nodes[idx].box_props.margin
            } else {
                Default::default()
            }
        } else {
            Default::default()
        };

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
        );

        // Push hit-test area for this inline-block element
        // This is critical for buttons and other inline-block elements to receive
        // mouse events and display the correct cursor (e.g., cursor: pointer)
        if let Some(tag_id) = get_tag_id(self.ctx.styled_dom, Some(node_id)) {
            builder.push_hit_test_area(border_box_bounds, tag_id);
        }

        Ok(())
    }

    /// Determines if a node establishes a new stacking context based on CSS rules.
    fn establishes_stacking_context(&self, node_index: usize) -> bool {
        let Some(node) = self.positioned_tree.tree.get(node_index) else {
            return false;
        };
        let Some(dom_id) = node.dom_node_id else {
            return false;
        };

        let position = get_position_type(self.ctx.styled_dom, Some(dom_id));
        if position == LayoutPosition::Absolute || position == LayoutPosition::Fixed {
            return true;
        }

        let z_index = get_z_index(self.ctx.styled_dom, Some(dom_id));
        if position == LayoutPosition::Relative && z_index != 0 {
            return true;
        }

        if let Some(styled_node) = self.ctx.styled_dom.styled_nodes.as_container().get(dom_id) {
            let node_data = &self.ctx.styled_dom.node_data.as_container()[dom_id];
            let node_state =
                &self.ctx.styled_dom.styled_nodes.as_container()[dom_id].styled_node_state;

            // Opacity < 1
            let opacity = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_opacity(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| v.inner.normalized())
                .unwrap_or(1.0);

            if opacity < 1.0 {
                return true;
            }

            // Transform != none
            let has_transform = self
                .ctx
                .styled_dom
                .css_property_cache
                .ptr
                .get_transform(node_data, &dom_id, node_state)
                .and_then(|v| v.get_property())
                .map(|v| !v.is_empty())
                .unwrap_or(false);

            if has_transform {
                return true;
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
pub struct PositionedTree<'a> {
    /// The layout tree containing all nodes with their computed sizes
    pub tree: &'a LayoutTree,
    /// Map from node index to its absolute position in the document
    pub calculated_positions: &'a super::PositionVec,
}

/// Describes how overflow content should be handled for an element.
///
/// This maps to the CSS `overflow-x` and `overflow-y` properties and determines
/// whether content that exceeds the element's bounds should be visible, clipped,
/// or scrollable.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverflowBehavior {
    /// Content is not clipped and may render outside the element's box (default)
    Visible,
    /// Content is clipped to the padding box, no scrollbars provided
    Hidden,
    /// Content is clipped to the padding box (CSS `overflow: clip`)
    Clip,
    /// Content is clipped and scrollbars are always shown
    Scroll,
    /// Content is clipped and scrollbars appear only when needed
    Auto,
}

impl OverflowBehavior {
    /// Returns `true` if this overflow behavior clips content.
    ///
    /// All behaviors except `Visible` result in content being clipped
    /// to the element's padding box.
    pub fn is_clipped(&self) -> bool {
        matches!(self, Self::Hidden | Self::Clip | Self::Scroll | Self::Auto)
    }

    /// Returns `true` if this overflow behavior enables scrolling.
    ///
    /// Only `Scroll` and `Auto` allow the user to scroll to see
    /// overflowing content.
    pub fn is_scroll(&self) -> bool {
        matches!(self, Self::Scroll | Self::Auto)
    }
}

fn get_scroll_id(id: Option<NodeId>) -> LocalScrollId {
    id.map(|i| i.index() as u64).unwrap_or(0)
}

/// Calculates the actual content size of a node, including all children and text.
/// This is used to determine if scrollbars should appear for overflow: auto.
fn get_scroll_content_size(node: &LayoutNode) -> LogicalSize {
    // First check if we have a pre-calculated overflow_content_size (for block children)
    if let Some(overflow_size) = node.overflow_content_size {
        return overflow_size;
    }

    // Start with the node's own size
    let mut content_size = node.used_size.unwrap_or_default();

    // If this node has text layout, calculate the bounds of all text items
    if let Some(ref cached_layout) = node.inline_layout_result {
        let text_layout = &cached_layout.layout;
        // Find the maximum extent of all positioned items
        let mut max_x: f32 = 0.0;
        let mut max_y: f32 = 0.0;

        for positioned_item in &text_layout.items {
            let item_bounds = positioned_item.item.bounds();
            let item_right = positioned_item.position.x + item_bounds.width;
            let item_bottom = positioned_item.position.y + item_bounds.height;

            max_x = max_x.max(item_right);
            max_y = max_y.max(item_bottom);
        }

        // Use the maximum extent as content size if it's larger
        content_size.width = content_size.width.max(max_x);
        content_size.height = content_size.height.max(max_y);
    }

    content_size
}

fn get_tag_id(dom: &StyledDom, id: Option<NodeId>) -> Option<DisplayListTagId> {
    let node_id = id?;
    let tag_mapping = dom.tag_ids_to_node_ids.as_ref().iter().find(|m| {
        m.node_id.into_crate_internal() == Some(node_id)
    })?;
    // Use TAG_TYPE_DOM_NODE (0x0100) as namespace marker in u16 field
    // This distinguishes DOM nodes from scrollbars (0x0200) and other tag types
    Some((tag_mapping.tag_id.inner, 0x0100))
}

fn get_image_ref_for_image_source(
    source: &ImageSource,
) -> Option<ImageRef> {
    match source {
        ImageSource::Ref(image_ref) => Some(image_ref.clone()),
        ImageSource::Url(_url) => {
            // TODO: Look up in ImageCache
            // For now, CSS url() images are not yet supported
            None
        }
        ImageSource::Data(_) | ImageSource::Svg(_) | ImageSource::Placeholder(_) => {
            // TODO: Decode raw data / SVG to ImageRef
            None
        }
    }
}

/// Get the bounds of a display list item, if it has spatial extent.
fn get_display_item_bounds(item: &DisplayListItem) -> Option<WindowLogicalRect> {
    match item {
        DisplayListItem::Rect { bounds, .. } => Some(*bounds),
        DisplayListItem::SelectionRect { bounds, .. } => Some(*bounds),
        DisplayListItem::CursorRect { bounds, .. } => Some(*bounds),
        DisplayListItem::Border { bounds, .. } => Some(*bounds),
        DisplayListItem::TextLayout { bounds, .. } => Some(*bounds),
        DisplayListItem::Text { clip_rect, .. } => Some(*clip_rect),
        DisplayListItem::Underline { bounds, .. } => Some(*bounds),
        DisplayListItem::Strikethrough { bounds, .. } => Some(*bounds),
        DisplayListItem::Overline { bounds, .. } => Some(*bounds),
        DisplayListItem::Image { bounds, .. } => Some(*bounds),
        DisplayListItem::ScrollBar { bounds, .. } => Some(*bounds),
        DisplayListItem::ScrollBarStyled { info } => Some(info.bounds),
        DisplayListItem::PushClip { bounds, .. } => Some(*bounds),
        DisplayListItem::PushScrollFrame { clip_bounds, .. } => Some(*clip_bounds),
        DisplayListItem::HitTestArea { bounds, .. } => Some(*bounds),
        DisplayListItem::PushStackingContext { bounds, .. } => Some(*bounds),
        DisplayListItem::IFrame { bounds, .. } => Some(*bounds),
        _ => None,
    }
}

/// Clip a display list item to page bounds and offset to page-relative coordinates.
/// Returns None if the item is completely outside the page bounds.
fn clip_and_offset_display_item(
    item: &DisplayListItem,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    match item {
        DisplayListItem::Rect {
            bounds,
            color,
            border_radius,
        } => clip_rect_item(bounds.into_inner(), *color, *border_radius, page_top, page_bottom),

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
            border_radius.clone(),
            page_top,
            page_bottom,
        ),

        DisplayListItem::SelectionRect {
            bounds,
            border_radius,
            color,
        } => clip_selection_rect_item(bounds.into_inner(), *border_radius, *color, page_top, page_bottom),

        DisplayListItem::CursorRect { bounds, color } => {
            clip_cursor_rect_item(bounds.into_inner(), *color, page_top, page_bottom)
        }

        DisplayListItem::Image { bounds, image } => {
            clip_image_item(bounds.into_inner(), image.clone(), page_top, page_bottom)
        }

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

        DisplayListItem::IFrame {
            child_dom_id,
            bounds,
            clip_rect,
        } => clip_iframe_item(*child_dom_id, bounds.into_inner(), clip_rect.into_inner(), page_top, page_bottom),

        // ScrollBarStyled - clip based on overall bounds
        DisplayListItem::ScrollBarStyled { info } => {
            let bounds = info.bounds;
            if bounds.0.origin.y + bounds.0.size.height < page_top || bounds.0.origin.y > page_bottom {
                None
            } else {
                // Clone and offset all the internal bounds
                let mut clipped_info = (**info).clone();
                let y_offset = -page_top;
                clipped_info.bounds = offset_rect_y(clipped_info.bounds.into_inner(), y_offset).into();
                clipped_info.track_bounds = offset_rect_y(clipped_info.track_bounds.into_inner(), y_offset).into();
                clipped_info.thumb_bounds = offset_rect_y(clipped_info.thumb_bounds.into_inner(), y_offset).into();
                if let Some(b) = clipped_info.button_decrement_bounds {
                    clipped_info.button_decrement_bounds = Some(offset_rect_y(b.into_inner(), y_offset).into());
                }
                if let Some(b) = clipped_info.button_increment_bounds {
                    clipped_info.button_increment_bounds = Some(offset_rect_y(b.into_inner(), y_offset).into());
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
        | DisplayListItem::IFramePlaceholder { .. } => None,

        // Gradient items - simple bounds check
        DisplayListItem::LinearGradient {
            bounds,
            gradient,
            border_radius,
        } => {
            if bounds.0.origin.y + bounds.0.size.height < page_top || bounds.0.origin.y > page_bottom {
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
            if bounds.0.origin.y + bounds.0.size.height < page_top || bounds.0.origin.y > page_bottom {
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
            if bounds.0.origin.y + bounds.0.size.height < page_top || bounds.0.origin.y > page_bottom {
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
            if bounds.0.origin.y + bounds.0.size.height < page_top || bounds.0.origin.y > page_bottom {
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
        | DisplayListItem::PopTextShadow => None,
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
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    if !rect_intersects(&bounds, page_top, page_bottom) {
        return None;
    }
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::Image {
        bounds: clipped.into(),
        image,
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

    // Try to downcast and filter UnifiedLayout items
    #[cfg(feature = "text_layout")]
    if let Some(unified_layout) = layout.downcast_ref::<crate::text3::cache::UnifiedLayout>() {
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

/// Clips a UnifiedLayout by filtering items to those on the current page.
#[cfg(feature = "text_layout")]
fn clip_unified_layout(
    unified_layout: &crate::text3::cache::UnifiedLayout,
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

    let new_layout = crate::text3::cache::UnifiedLayout {
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
        layout: Arc::new(new_layout) as Arc<dyn std::any::Any + Send + Sync>,
        bounds: new_bounds.into(),
        font_hash,
        font_size_px,
        color,
    })
}

/// Checks if an item's center point falls within the page bounds.
#[cfg(feature = "text_layout")]
fn item_center_on_page(
    item: &crate::text3::cache::PositionedItem,
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
/// Returns (items, min_y, max_y, max_width).
#[cfg(feature = "text_layout")]
fn transform_items_to_page_coords(
    items: Vec<crate::text3::cache::PositionedItem>,
    layout_origin_y: f32,
    page_top: f32,
    new_origin_y: f32,
) -> (Vec<crate::text3::cache::PositionedItem>, f32, f32, f32) {
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

/// Clips an iframe to page bounds.
fn clip_iframe_item(
    child_dom_id: DomId,
    bounds: LogicalRect,
    clip_rect: LogicalRect,
    page_top: f32,
    page_bottom: f32,
) -> Option<DisplayListItem> {
    clip_rect_bounds(bounds, page_top, page_bottom).map(|clipped| DisplayListItem::IFrame {
        child_dom_id,
        bounds: clipped.into(),
        clip_rect: offset_rect_y(clip_rect, -page_top).into(),
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
}

impl SlicerConfig {
    /// Create a simple slicer config with no gaps between pages.
    pub fn simple(page_height: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: 0.0,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: 595.0, // Default A4 width in points
            table_headers: TableHeaderTracker::default(),
        }
    }

    /// Create a slicer config with margins/gaps between pages.
    pub fn with_gap(page_height: f32, gap: f32) -> Self {
        Self {
            page_content_height: page_height,
            page_gap: gap,
            allow_clipping: true,
            header_footer: HeaderFooterConfig::default(),
            page_width: 595.0,
            table_headers: TableHeaderTracker::default(),
        }
    }

    /// Add header/footer configuration.
    pub fn with_header_footer(mut self, config: HeaderFooterConfig) -> Self {
        self.header_footer = config;
        self
    }

    /// Set the page width (for header/footer positioning).
    pub fn with_page_width(mut self, width: f32) -> Self {
        self.page_width = width;
        self
    }

    /// Add table headers for repetition.
    pub fn with_table_headers(mut self, tracker: TableHeaderTracker) -> Self {
        self.table_headers = tracker;
        self
    }

    /// Register a single table header.
    pub fn register_table_header(&mut self, info: TableHeaderInfo) {
        self.table_headers.register_table_header(info);
    }

    /// The total height of a page "slot" including the gap.
    pub fn page_slot_height(&self) -> f32 {
        self.page_content_height + self.page_gap
    }

    /// Calculate which page a Y coordinate falls on.
    pub fn page_for_y(&self, y: f32) -> usize {
        if self.page_slot_height() <= 0.0 {
            return 0;
        }
        (y / self.page_slot_height()).floor() as usize
    }

    /// Get the Y range for a specific page (in infinite canvas coordinates).
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
pub fn paginate_display_list_with_slicer_and_breaks(
    full_display_list: DisplayList,
    config: &SlicerConfig,
) -> Result<Vec<DisplayList>> {
    if config.page_content_height <= 0.0 || config.page_content_height >= f32::MAX {
        return Ok(vec![full_display_list]);
    }

    // Calculate base header/footer space (used for pages that show headers/footers)
    let base_header_space = if config.header_footer.show_header {
        config.header_footer.header_height
    } else {
        0.0
    };
    let base_footer_space = if config.header_footer.show_footer {
        config.header_footer.footer_height
    } else {
        0.0
    };

    // Calculate effective heights for different page types
    let normal_page_content_height =
        config.page_content_height - base_header_space - base_footer_space;
    let first_page_content_height = if config.header_footer.skip_first_page {
        // First page has full height when skipping headers/footers
        config.page_content_height
    } else {
        normal_page_content_height
    };

    // Step 1: Calculate page break positions based on CSS properties
    //
    // Instead of using regular intervals, we calculate where page breaks
    // should occur based on:
    //
    // - break-before: always → force break before this item
    // - break-after: always → force break after this item
    // - break-inside: avoid → don't break inside this item (push to next page if needed)

    let page_breaks = calculate_page_break_positions(
        &full_display_list,
        first_page_content_height,
        normal_page_content_height,
    );

    let num_pages = page_breaks.len();

    // Create per-page display lists by slicing the master list
    let mut pages: Vec<DisplayList> = Vec::with_capacity(num_pages);

    for (page_idx, &(content_start_y, content_end_y)) in page_breaks.iter().enumerate() {
        // Generate page info for header/footer content
        let page_info = PageInfo::new(page_idx + 1, num_pages);

        // Calculate per-page header/footer space
        let skip_this_page = config.header_footer.skip_first_page && page_info.is_first;
        let header_space = if config.header_footer.show_header && !skip_this_page {
            config.header_footer.header_height
        } else {
            0.0
        };
        let footer_space = if config.header_footer.show_footer && !skip_this_page {
            config.header_footer.footer_height
        } else {
            0.0
        };

        let _ = footer_space; // Currently unused but reserved for future

        let mut page_items = Vec::new();
        let mut page_node_mapping = Vec::new();

        // 1. Add header if enabled
        if config.header_footer.show_header && !skip_this_page {
            let header_text = config.header_footer.header_text(page_info);
            if !header_text.is_empty() {
                let header_items = generate_text_display_items(
                    &header_text,
                    LogicalRect {
                        origin: LogicalPosition { x: 0.0, y: 0.0 },
                        size: LogicalSize {
                            width: config.page_width,
                            height: config.header_footer.header_height,
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
                );
                for item in header_items {
                    page_items.push(item);
                    page_node_mapping.push(None);
                }
            }
        }

        // 2. Inject repeated table headers (if any)
        let repeated_headers = config.table_headers.get_repeated_headers_for_page(
            page_idx,
            content_start_y,
            content_end_y,
        );

        let mut thead_total_height = 0.0f32;
        for (y_offset_from_page_top, thead_items, thead_height) in repeated_headers {
            let thead_y = header_space + y_offset_from_page_top;
            for item in thead_items {
                let translated_item = offset_display_item_y(item, thead_y);
                page_items.push(translated_item);
                page_node_mapping.push(None);
            }
            thead_total_height = thead_total_height.max(thead_height);
        }

        // 3. Calculate content offset (after header and repeated table headers)
        let content_y_offset = header_space + thead_total_height;

        // 4. Slice and offset content items
        for (item_idx, item) in full_display_list.items.iter().enumerate() {
            if let Some(clipped_item) =
                clip_and_offset_display_item(item, content_start_y, content_end_y)
            {
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

        // 5. Add footer if enabled
        if config.header_footer.show_footer && !skip_this_page {
            let footer_text = config.header_footer.footer_text(page_info);
            if !footer_text.is_empty() {
                let footer_y = config.page_content_height - config.header_footer.footer_height;
                let footer_items = generate_text_display_items(
                    &footer_text,
                    LogicalRect {
                        origin: LogicalPosition {
                            x: 0.0,
                            y: footer_y,
                        },
                        size: LogicalSize {
                            width: config.page_width,
                            height: config.header_footer.footer_height,
                        },
                    },
                    config.header_footer.font_size,
                    config.header_footer.text_color,
                    TextAlignment::Center,
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
            forced_page_breaks: Vec::new(), // Per-page lists don't need this
        });
    }

    // Ensure at least one page
    if pages.is_empty() {
        pages.push(DisplayList::default());
    }

    Ok(pages)
}

/// Calculate page break positions respecting CSS forced page breaks.
///
/// Returns a vector of (start_y, end_y) tuples representing each page's content bounds.
///
/// This function uses the `forced_page_breaks` from the DisplayList to insert
/// page breaks at positions specified by CSS `break-before: always` and `break-after: always`.
/// Regular page breaks still occur at normal intervals when no forced break is present.
fn calculate_page_break_positions(
    display_list: &DisplayList,
    first_page_height: f32,
    normal_page_height: f32,
) -> Vec<(f32, f32)> {
    let total_height = calculate_display_list_height(display_list);

    if total_height <= 0.0 || first_page_height <= 0.0 {
        return vec![(0.0, total_height.max(first_page_height))];
    }

    // Collect all potential break points: forced breaks + regular interval breaks
    let mut break_points: Vec<f32> = Vec::new();

    // Add forced page breaks from the display list (from CSS break-before/break-after)
    for &forced_break_y in &display_list.forced_page_breaks {
        if forced_break_y > 0.0 && forced_break_y < total_height {
            break_points.push(forced_break_y);
        }
    }

    // Generate regular interval break points
    let mut y = first_page_height;
    while y < total_height {
        break_points.push(y);
        y += normal_page_height;
    }

    // Sort and deduplicate break points
    break_points.sort_by(|a, b| a.partial_cmp(b).unwrap());
    break_points.dedup_by(|a, b| (*a - *b).abs() < 1.0); // Merge breaks within 1px

    // Convert break points to page ranges
    let mut page_breaks: Vec<(f32, f32)> = Vec::new();
    let mut page_start = 0.0f32;

    for break_y in break_points {
        if break_y > page_start {
            page_breaks.push((page_start, break_y));
            page_start = break_y;
        }
    }

    // Add final page if there's remaining content
    if page_start < total_height {
        page_breaks.push((page_start, total_height));
    }

    // Ensure at least one page
    if page_breaks.is_empty() {
        page_breaks.push((0.0, total_height.max(first_page_height)));
    }

    page_breaks
}

/// Text alignment for generated header/footer text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlignment {
    Left,
    Center,
    Right,
}

/// Helper to offset all Y coordinates of a display item.
fn offset_display_item_y(item: &DisplayListItem, y_offset: f32) -> DisplayListItem {
    if y_offset == 0.0 {
        return item.clone();
    }

    match item {
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
            widths: widths.clone(),
            colors: *colors,
            styles: *styles,
            border_radius: border_radius.clone(),
        },
        DisplayListItem::Text {
            glyphs,
            font_hash,
            font_size_px,
            color,
            clip_rect,
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
        DisplayListItem::Image { bounds, image } => DisplayListItem::Image {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            image: image.clone(),
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
        DisplayListItem::IFrame {
            child_dom_id,
            bounds,
            clip_rect,
        } => DisplayListItem::IFrame {
            child_dom_id: *child_dom_id,
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            clip_rect: offset_rect_y(clip_rect.into_inner(), y_offset).into(),
        },
        DisplayListItem::IFramePlaceholder {
            node_id,
            bounds,
            clip_rect,
        } => DisplayListItem::IFramePlaceholder {
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
        DisplayListItem::PushOpacity { bounds, opacity } => DisplayListItem::PushOpacity {
            bounds: offset_rect_y(bounds.into_inner(), y_offset).into(),
            opacity: *opacity,
        },
        DisplayListItem::PopOpacity => DisplayListItem::PopOpacity,
        DisplayListItem::ScrollBarStyled { info } => {
            let mut offset_info = (**info).clone();
            offset_info.bounds = offset_rect_y(offset_info.bounds.into_inner(), y_offset).into();
            offset_info.track_bounds = offset_rect_y(offset_info.track_bounds.into_inner(), y_offset).into();
            offset_info.thumb_bounds = offset_rect_y(offset_info.thumb_bounds.into_inner(), y_offset).into();
            if let Some(b) = offset_info.button_decrement_bounds {
                offset_info.button_decrement_bounds = Some(offset_rect_y(b.into_inner(), y_offset).into());
            }
            if let Some(b) = offset_info.button_increment_bounds {
                offset_info.button_increment_bounds = Some(offset_rect_y(b.into_inner(), y_offset).into());
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
        DisplayListItem::PushTextShadow { shadow } => DisplayListItem::PushTextShadow {
            shadow: shadow.clone(),
        },
        DisplayListItem::PopTextShadow => DisplayListItem::PopTextShadow,
    }
}

/// Generate display list items for simple text (headers/footers).
///
/// This creates a simplified text rendering without full text layout.
/// For now, this creates a placeholder that renderers should handle specially.
fn generate_text_display_items(
    text: &str,
    bounds: LogicalRect,
    font_size: f32,
    color: ColorU,
    alignment: TextAlignment,
) -> Vec<DisplayListItem> {
    use crate::font_traits::FontHash;

    if text.is_empty() {
        return Vec::new();
    }

    // Calculate approximate text position based on alignment
    // For now, we estimate character width as 0.5 * font_size (monospace approximation)
    let char_width = font_size * 0.5;
    let text_width = text.len() as f32 * char_width;

    let x_offset = match alignment {
        TextAlignment::Left => bounds.origin.x,
        TextAlignment::Center => bounds.origin.x + (bounds.size.width - text_width) / 2.0,
        TextAlignment::Right => bounds.origin.x + bounds.size.width - text_width,
    };

    // Position text vertically centered in the bounds
    let y_pos = bounds.origin.y + (bounds.size.height + font_size) / 2.0 - font_size * 0.2;

    // Create simple glyph instances for each character
    // Note: This is a simplified approach - proper text rendering should use text3
    let glyphs: Vec<GlyphInstance> = text
        .chars()
        .enumerate()
        .filter(|(_, c)| !c.is_control())
        .map(|(i, c)| GlyphInstance {
            index: c as u32, // Use Unicode codepoint as glyph index (placeholder)
            point: LogicalPosition {
                x: x_offset + i as f32 * char_width,
                y: y_pos,
            },
            size: LogicalSize::new(char_width, font_size),
        })
        .collect();

    if glyphs.is_empty() {
        return Vec::new();
    }

    vec![DisplayListItem::Text {
        glyphs,
        font_hash: FontHash::from_hash(0), // Default font hash - renderer should use default font
        font_size_px: font_size,
        color,
        clip_rect: bounds.into(),
    }]
}

/// Calculate the total height of a display list (max Y + height of all items).
fn calculate_display_list_height(display_list: &DisplayList) -> f32 {
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
pub struct BreakProperties {
    pub break_before: PageBreak,
    pub break_after: PageBreak,
    pub break_inside: BreakInside,
}

use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};
use azul_core::dom::ScrollbarOrientation;

/// Information about scrollbar requirements and dimensions
#[derive(Debug, Clone, Default)]
#[repr(C)]
pub struct ScrollbarRequirements {
    pub needs_horizontal: bool,
    pub needs_vertical: bool,
    pub scrollbar_width: f32,
    pub scrollbar_height: f32,
}

impl ScrollbarRequirements {
    /// Checks if the presence of scrollbars reduces the available inner size,
    /// which would necessitate a reflow of the content.
    pub fn needs_reflow(&self) -> bool {
        self.scrollbar_width > 0.0 || self.scrollbar_height > 0.0
    }

    /// Takes a size (representing a content-box) and returns a new size
    /// reduced by the dimensions of any active scrollbars.
    pub fn shrink_size(&self, size: LogicalSize) -> LogicalSize {
        LogicalSize {
            width: (size.width - self.scrollbar_width).max(0.0),
            height: (size.height - self.scrollbar_height).max(0.0),
        }
    }
}

/// Single source of truth for scrollbar geometry.
///
/// Computed once by [`compute_scrollbar_geometry`], then used by:
/// - Display list painting (`paint_scrollbars`)
/// - GPU transform updates (`update_scrollbar_transforms`)
/// - Hit-testing (`hit_test_component`)
/// - Drag delta conversion (`handle_scrollbar_drag`)
#[derive(Debug, Clone, Copy)]
pub struct ScrollbarGeometry {
    /// Orientation (vertical or horizontal)
    pub orientation: ScrollbarOrientation,
    /// The full track rect in the container's coordinate space
    pub track_rect: LogicalRect,
    /// Button size (square: width = height = scrollbar_width_px)
    pub button_size: f32,
    /// Usable track length after subtracting buttons and corner
    /// = track_total - 2*button_size
    pub usable_track_length: f32,
    /// The thumb length (min-clamped to 2*width_px)
    pub thumb_length: f32,
    /// Thumb size as ratio of viewport / content (0.0–1.0)
    pub thumb_size_ratio: f32,
    /// Scroll ratio (0.0 at top/left, 1.0 at bottom/right)
    pub scroll_ratio: f32,
    /// Thumb offset in pixels from the start of the usable track region
    pub thumb_offset: f32,
    /// Max scroll distance in content pixels
    pub max_scroll: f32,
    /// CSS-specified scrollbar thickness (width for vertical, height for horizontal)
    pub width_px: f32,
}

impl Default for ScrollbarGeometry {
    fn default() -> Self {
        Self {
            orientation: ScrollbarOrientation::Vertical,
            track_rect: LogicalRect::zero(),
            button_size: 0.0,
            usable_track_length: 0.0,
            thumb_length: 0.0,
            thumb_size_ratio: 0.0,
            scroll_ratio: 0.0,
            thumb_offset: 0.0,
            max_scroll: 0.0,
            width_px: 0.0,
        }
    }
}

/// Compute scrollbar geometry for one axis.
///
/// This is the **single source of truth** for all scrollbar calculations.
/// All consumers (display list painting, GPU transforms, hit-testing, drag)
/// must use this function to ensure consistent geometry.
///
/// # Parameters
/// - `orientation`: Vertical or horizontal scrollbar
/// - `inner_rect`: The padding-box (border-box minus borders) of the scroll container,
///   in the container's coordinate space (absolute window coordinates)
/// - `content_size`: Total content size (from `get_content_size()` or `virtual_scroll_size`)
/// - `scroll_offset`: Current scroll offset (y for vertical, x for horizontal; positive = scrolled)
/// - `scrollbar_width_px`: CSS-resolved scrollbar thickness in pixels
/// - `has_other_scrollbar`: Whether the perpendicular scrollbar is also visible
///   (reduces track length by one `scrollbar_width_px` for the corner)
pub fn compute_scrollbar_geometry(
    orientation: ScrollbarOrientation,
    inner_rect: LogicalRect,
    content_size: LogicalSize,
    scroll_offset: f32,
    scrollbar_width_px: f32,
    has_other_scrollbar: bool,
) -> ScrollbarGeometry {
    let button_size = scrollbar_width_px;

    match orientation {
        ScrollbarOrientation::Vertical => {
            // Track runs along the right edge of inner_rect
            let track_total = if has_other_scrollbar {
                inner_rect.size.height - scrollbar_width_px
            } else {
                inner_rect.size.height
            };

            let track_rect = LogicalRect {
                origin: LogicalPosition::new(
                    inner_rect.origin.x + inner_rect.size.width - scrollbar_width_px,
                    inner_rect.origin.y,
                ),
                size: LogicalSize::new(scrollbar_width_px, track_total),
            };

            let usable_track_length = (track_total - 2.0 * button_size).max(0.0);
            let viewport_length = inner_rect.size.height;
            let content_length = content_size.height;

            let thumb_size_ratio = if content_length > 0.0 {
                (viewport_length / content_length).min(1.0)
            } else {
                1.0
            };
            let thumb_length = (usable_track_length * thumb_size_ratio)
                .max(scrollbar_width_px * 2.0)
                .min(usable_track_length);

            let max_scroll = (content_length - viewport_length).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 {
                (scroll_offset.abs() / max_scroll).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let thumb_offset = (usable_track_length - thumb_length) * scroll_ratio;

            ScrollbarGeometry {
                orientation,
                track_rect,
                button_size,
                usable_track_length,
                thumb_length,
                thumb_size_ratio,
                scroll_ratio,
                thumb_offset,
                max_scroll,
                width_px: scrollbar_width_px,
            }
        }
        ScrollbarOrientation::Horizontal => {
            // Track runs along the bottom edge of inner_rect
            let track_total = if has_other_scrollbar {
                inner_rect.size.width - scrollbar_width_px
            } else {
                inner_rect.size.width
            };

            let track_rect = LogicalRect {
                origin: LogicalPosition::new(
                    inner_rect.origin.x,
                    inner_rect.origin.y + inner_rect.size.height - scrollbar_width_px,
                ),
                size: LogicalSize::new(track_total, scrollbar_width_px),
            };

            let usable_track_length = (track_total - 2.0 * button_size).max(0.0);
            let viewport_length = inner_rect.size.width;
            let content_length = content_size.width;

            let thumb_size_ratio = if content_length > 0.0 {
                (viewport_length / content_length).min(1.0)
            } else {
                1.0
            };
            let thumb_length = (usable_track_length * thumb_size_ratio)
                .max(scrollbar_width_px * 2.0)
                .min(usable_track_length);

            let max_scroll = (content_length - viewport_length).max(0.0);
            let scroll_ratio = if max_scroll > 0.0 {
                (scroll_offset.abs() / max_scroll).clamp(0.0, 1.0)
            } else {
                0.0
            };

            let thumb_offset = (usable_track_length - thumb_length) * scroll_ratio;

            ScrollbarGeometry {
                orientation,
                track_rect,
                button_size,
                usable_track_length,
                thumb_length,
                thumb_size_ratio,
                scroll_ratio,
                thumb_offset,
                max_scroll,
                width_px: scrollbar_width_px,
            }
        }
    }
}

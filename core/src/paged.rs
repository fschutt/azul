//! Paged media layout primitives.
//!
//! Provides the [`FragmentationContext`] that the layout solver threads through to
//! distinguish continuous (screen) from paged (print) media.
//!
//! For continuous media (screens), content flows into a single infinitely tall
//! container. For paged media (print), content is laid out on a continuous canvas
//! and afterwards sliced into fixed-size pages by the display-list slicer
//! (`paginate_display_list_with_slicer_and_breaks` in `azul_layout::solver3::display_list`).
//! This lets the layout engine make break decisions while respecting CSS properties
//! like `break-before`, `break-after`, and `break-inside`.
//!
//! Page *decoration* (headers, footers, margin boxes, counters) lives in
//! `azul_layout::solver3::pagination`.

use crate::geom::LogicalSize;

/// Selects how content is fragmented during layout.
///
/// This is the core abstraction for fragmentation support:
/// - Screen rendering: [`Continuous`](Self::Continuous) — a single infinite container.
/// - Print rendering: [`Paged`](Self::Paged) — a series of fixed-size page containers.
#[derive(Debug, Clone, Copy)]
pub enum FragmentationContext {
    /// Continuous media (screen): a single, infinitely tall container.
    ///
    /// Used for normal screen rendering where content can scroll indefinitely;
    /// breaks are never forced.
    Continuous {
        /// Width of the viewport.
        width: f32,
    },

    /// Paged media (print): fixed-size pages.
    ///
    /// Used for PDF generation and print preview. Content flows from one page to
    /// the next when a page is full.
    Paged {
        /// Size of each page.
        page_size: LogicalSize,
    },
}

impl FragmentationContext {
    /// Create a continuous fragmentation context for screen rendering.
    #[must_use] pub const fn new_continuous(width: f32) -> Self {
        Self::Continuous { width }
    }

    /// Create a paged fragmentation context for print rendering.
    #[must_use] pub const fn new_paged(page_size: LogicalSize) -> Self {
        Self::Paged { page_size }
    }

    /// Get the page content height (page height for paged media).
    ///
    /// For continuous media, returns `f32::MAX`.
    #[must_use] pub const fn page_content_height(&self) -> f32 {
        match self {
            Self::Continuous { .. } => f32::MAX,
            Self::Paged { page_size, .. } => page_size.height,
        }
    }

    /// Check if this is paged media.
    #[must_use] pub const fn is_paged(&self) -> bool {
        matches!(self, Self::Paged { .. })
    }
}

/// Page margins in points.
///
/// Canonical paged-media margin type (formerly defined in the now-removed
/// `crate::fragmentation` module). Re-exported from the crate root as
/// `azul_layout::PageMargins`.
#[derive(Debug, Clone, Copy, Default)]
pub struct PageMargins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}

impl PageMargins {
    #[must_use] pub const fn new(top: f32, right: f32, bottom: f32, left: f32) -> Self {
        Self {
            top,
            right,
            bottom,
            left,
        }
    }

    #[must_use] pub const fn uniform(margin: f32) -> Self {
        Self {
            top: margin,
            right: margin,
            bottom: margin,
            left: margin,
        }
    }

    #[must_use] pub fn horizontal(&self) -> f32 {
        self.left + self.right
    }

    #[must_use] pub fn vertical(&self) -> f32 {
        self.top + self.bottom
    }
}

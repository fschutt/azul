//! Paged media layout engine.
//!
//! This module provides infrastructure for multi-page document layout with CSS Paged Media support.
//!
//! The core concept is a **FragmentationContext**, which represents a series of containers
//! (fragmentainers) that content flows into during layout. For continuous media (screens),
//! we use a single infinite container. For paged media (print), we use a series of page-sized
//! containers.
//!
//! This approach allows the layout engine to make break decisions during layout, respecting
//! CSS properties like `break-before`, `break-after`, and `break-inside`.

use azul_core::geom::{LogicalPosition, LogicalSize};

use crate::solver3::display_list::DisplayList;

/// Represents a series of containers that content flows into during layout.
///
/// This is the core abstraction for fragmentation support. Different media types
/// use different fragmentation contexts:
/// - Screen rendering: Continuous (single infinite container)
/// - Print rendering: Paged (series of fixed-size page containers)
/// - Multi-column layout: MultiColumn (series of column containers)
#[derive(Debug, Clone)]
pub enum FragmentationContext {
    /// Continuous media (screen): single infinite container.
    ///
    /// Used for normal screen rendering where content can scroll indefinitely.
    /// The container grows as needed and never forces breaks.
    Continuous {
        /// Width of the viewport
        width: f32,
        /// The single fragmentainer (grows infinitely)
        container: Fragmentainer,
    },

    /// Paged media (print): series of page boxes.
    ///
    /// Used for PDF generation and print preview. Content flows from one
    /// page to the next when a page is full.
    Paged {
        /// Size of each page
        page_size: LogicalSize,
        /// All pages (fragmentainers) that have been created
        pages: Vec<Fragmentainer>,
    },

    /// Multi-column layout: series of column boxes.
    ///
    /// Future support for CSS multi-column layout.
    #[allow(dead_code)]
    MultiColumn {
        /// Width of each column
        column_width: f32,
        /// Height of each column
        column_height: f32,
        /// Gap between columns
        gap: f32,
        /// All columns that have been created
        columns: Vec<Fragmentainer>,
    },

    /// CSS Regions: series of region boxes.
    ///
    /// Future support for CSS Regions specification.
    #[allow(dead_code)]
    Regions {
        /// Pre-defined region boxes
        regions: Vec<Fragmentainer>,
    },
}

/// A single container (fragmentainer) in a fragmentation context.
///
/// Each fragmentainer has a logical size and tracks how much of that space
/// has been used. For continuous media, the fragmentainer can grow infinitely.
/// For paged media, fragmentainers have fixed sizes.
#[derive(Debug, Clone)]
pub struct Fragmentainer {
    /// Logical size of this container (width and height)
    pub size: LogicalSize,

    /// How much block-axis space has been used (typically vertical space)
    pub used_block_size: f32,

    /// Whether this container has a fixed size (true for pages) or can grow (false for continuous)
    pub is_fixed_size: bool,

    /// Content that has been placed in this fragmentainer.
    ///
    /// For Phase 1, this is unused. In later phases, we'll store layout boxes here.
    pub content: Vec<LayoutBox>,
}

/// Placeholder for layout box content (to be implemented in later phases)
#[derive(Debug, Clone)]
pub struct LayoutBox {
    // TODO: Define structure in later phases
}

impl Fragmentainer {
    /// Create a new fragmentainer with the given size.
    ///
    /// # Arguments
    /// * `size` - The logical size (width and height) of this fragmentainer
    /// * `is_fixed_size` - Whether this fragmentainer has a fixed size (true for pages, false for continuous)
    pub fn new(size: LogicalSize, is_fixed_size: bool) -> Self {
        Self {
            size,
            used_block_size: 0.0,
            is_fixed_size,
            content: Vec::new(),
        }
    }

    /// Get the remaining space in this fragmentainer.
    ///
    /// For continuous media, this returns infinity (f32::MAX).
    /// For paged media, this returns the unused space.
    pub fn remaining_space(&self) -> f32 {
        if self.is_fixed_size {
            (self.size.height - self.used_block_size).max(0.0)
        } else {
            f32::MAX // Infinite for continuous media
        }
    }

    /// Check if this fragmentainer is full.
    ///
    /// A fragmentainer is considered full if it has less than 1px of remaining space.
    /// Continuous fragmentainers are never full.
    pub fn is_full(&self) -> bool {
        self.is_fixed_size && self.remaining_space() < 1.0
    }

    /// Check if a block of the given size can fit in this fragmentainer.
    ///
    /// # Arguments
    /// * `block_size` - The height of the block to check
    pub fn can_fit(&self, block_size: f32) -> bool {
        self.remaining_space() >= block_size
    }

    /// Record that space has been used in this fragmentainer.
    ///
    /// # Arguments
    /// * `size` - The amount of block-axis space used
    pub fn use_space(&mut self, size: f32) {
        self.used_block_size += size;
    }
}

impl FragmentationContext {
    /// Create a continuous fragmentation context for screen rendering.
    ///
    /// # Arguments
    /// * `width` - The viewport width
    pub fn new_continuous(width: f32) -> Self {
        Self::Continuous {
            width,
            container: Fragmentainer::new(
                LogicalSize::new(width, f32::MAX),
                false, // Not fixed size
            ),
        }
    }

    /// Create a paged fragmentation context for print rendering.
    ///
    /// # Arguments
    /// * `page_size` - The size of each page
    pub fn new_paged(page_size: LogicalSize) -> Self {
        Self::Paged {
            page_size,
            pages: vec![Fragmentainer::new(page_size, true)],
        }
    }

    /// Get the number of fragmentainers (pages, columns, etc.) in this context.
    pub fn fragmentainer_count(&self) -> usize {
        match self {
            Self::Continuous { .. } => 1,
            Self::Paged { pages, .. } => pages.len(),
            Self::MultiColumn { columns, .. } => columns.len(),
            Self::Regions { regions } => regions.len(),
        }
    }

    /// Get a reference to the current fragmentainer being filled.
    pub fn current(&self) -> &Fragmentainer {
        match self {
            Self::Continuous { container, .. } => container,
            Self::Paged { pages, .. } => pages.last().expect("Paged context must have at least one page"),
            Self::MultiColumn { columns, .. } => columns.last().expect("MultiColumn context must have at least one column"),
            Self::Regions { regions } => regions.last().expect("Regions context must have at least one region"),
        }
    }

    /// Get a mutable reference to the current fragmentainer being filled.
    pub fn current_mut(&mut self) -> &mut Fragmentainer {
        match self {
            Self::Continuous { container, .. } => container,
            Self::Paged { pages, .. } => pages.last_mut().expect("Paged context must have at least one page"),
            Self::MultiColumn { columns, .. } => columns.last_mut().expect("MultiColumn context must have at least one column"),
            Self::Regions { regions } => regions.last_mut().expect("Regions context must have at least one region"),
        }
    }

    /// Advance to the next fragmentainer, creating a new one if necessary.
    ///
    /// For continuous media, this is a no-op (continuous media can't advance).
    /// For paged media, this creates a new page.
    /// For regions, this fails if no more regions are available.
    ///
    /// # Returns
    /// `Ok(())` if the advance succeeded, `Err(String)` if it failed (e.g., no more regions).
    pub fn advance(&mut self) -> Result<(), String> {
        match self {
            Self::Continuous { .. } => {
                // Continuous media doesn't advance, it just grows
                Ok(())
            }
            Self::Paged { page_size, pages } => {
                // Create a new page
                pages.push(Fragmentainer::new(*page_size, true));
                Ok(())
            }
            Self::MultiColumn {
                column_width,
                column_height,
                columns,
                ..
            } => {
                // Create a new column
                columns.push(Fragmentainer::new(
                    LogicalSize::new(*column_width, *column_height),
                    true,
                ));
                Ok(())
            }
            Self::Regions { .. } => {
                // Regions are pre-defined, can't create more
                Err("No more regions available for content overflow".to_string())
            }
        }
    }

    /// Get all fragmentainers in this context.
    pub fn fragmentainers(&self) -> Vec<&Fragmentainer> {
        match self {
            Self::Continuous { container, .. } => vec![container],
            Self::Paged { pages, .. } => pages.iter().collect(),
            Self::MultiColumn { columns, .. } => columns.iter().collect(),
            Self::Regions { regions } => regions.iter().collect(),
        }
    }
}

// Legacy API - keeping for backward compatibility during transition

use std::sync::Arc;
use crate::text3::cache::{ParsedFontTrait, UnifiedLayout};

#[derive(Debug, Clone)]
pub struct Page<T: ParsedFontTrait> {
    pub layout: Arc<UnifiedLayout<T>>,
    pub page_number: usize,
    pub page_size: LogicalSize,
}

#[allow(unused_variables)]
pub fn layout_to_pages<T: ParsedFontTrait + 'static>(page_size: LogicalSize) -> Vec<Page<T>> {
    Vec::new()
}

pub fn generate_display_lists_from_paged_layout<T: ParsedFontTrait>(
    pages: &[Page<T>],
) -> Vec<DisplayList> {
    pages.iter().map(|_| DisplayList::default()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_continuous_context_has_infinite_space() {
        let ctx = FragmentationContext::new_continuous(800.0);
        assert_eq!(ctx.fragmentainer_count(), 1);
        assert_eq!(ctx.current().remaining_space(), f32::MAX);
        assert!(!ctx.current().is_full());
    }

    #[test]
    fn test_paged_context_has_fixed_space() {
        let ctx = FragmentationContext::new_paged(LogicalSize::new(800.0, 1000.0));
        assert_eq!(ctx.fragmentainer_count(), 1);
        assert_eq!(ctx.current().remaining_space(), 1000.0);
        assert!(!ctx.current().is_full());
    }

    #[test]
    fn test_fragmentainer_tracks_used_space() {
        let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, 1000.0), true);
        assert_eq!(fragmentainer.remaining_space(), 1000.0);

        fragmentainer.use_space(300.0);
        assert_eq!(fragmentainer.remaining_space(), 700.0);

        fragmentainer.use_space(600.0);
        assert_eq!(fragmentainer.remaining_space(), 100.0);
    }

    #[test]
    fn test_fragmentainer_can_fit_checks_space() {
        let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, 1000.0), true);
        assert!(fragmentainer.can_fit(500.0));
        assert!(fragmentainer.can_fit(1000.0));
        assert!(!fragmentainer.can_fit(1001.0));

        fragmentainer.use_space(700.0);
        assert!(fragmentainer.can_fit(200.0));
        assert!(fragmentainer.can_fit(300.0));
        assert!(!fragmentainer.can_fit(301.0));
    }

    #[test]
    fn test_fragmentainer_is_full_when_space_exhausted() {
        let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, 1000.0), true);
        assert!(!fragmentainer.is_full());

        // After using 999px, we have 1px remaining - not full yet
        fragmentainer.use_space(999.0);
        assert!(!fragmentainer.is_full()); // Still has exactly 1px

        // After using 0.1px more, we have 0.9px remaining - now it's full (< 1px)
        fragmentainer.use_space(0.1);
        assert!(fragmentainer.is_full()); // Less than 1px remaining
    }

    #[test]
    fn test_paged_context_advances_creates_new_page() {
        let mut ctx = FragmentationContext::new_paged(LogicalSize::new(800.0, 1000.0));
        assert_eq!(ctx.fragmentainer_count(), 1);

        ctx.advance().unwrap();
        assert_eq!(ctx.fragmentainer_count(), 2);

        ctx.advance().unwrap();
        assert_eq!(ctx.fragmentainer_count(), 3);
    }

    #[test]
    fn test_continuous_context_advance_is_noop() {
        let mut ctx = FragmentationContext::new_continuous(800.0);
        assert_eq!(ctx.fragmentainer_count(), 1);

        ctx.advance().unwrap();
        assert_eq!(ctx.fragmentainer_count(), 1); // Still 1, doesn't create new containers
    }

    #[test]
    fn test_fragmentainer_never_full_for_continuous() {
        let mut fragmentainer = Fragmentainer::new(LogicalSize::new(800.0, f32::MAX), false);
        assert_eq!(fragmentainer.remaining_space(), f32::MAX);
        assert!(!fragmentainer.is_full());

        fragmentainer.use_space(10000.0);
        assert_eq!(fragmentainer.remaining_space(), f32::MAX);
        assert!(!fragmentainer.is_full());
    }
}

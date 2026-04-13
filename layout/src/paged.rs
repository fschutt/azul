//! Paged media layout engine.
//!
//! This module provides infrastructure for multi-page document
//! layout with CSS Paged Media support.
//!
//! The core concept is a **FragmentationContext**, which represents
//! a series of containers (fragmentainers) that content flows into
//! during layout. For continuous media (screens), we use a single
//! infinite container. For paged media (print), we use a series of
//! page-sized containers.
//!
//! This approach allows the layout engine to make break decisions
//! during layout, respecting CSS properties like `break-before`,
//! `break-after`, and `break-inside`.

use azul_core::geom::LogicalSize;

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

    /// Whether this container has a fixed size (true for pages) or can
    /// grow (false for continuous)
    pub is_fixed_size: bool,

}

impl Fragmentainer {
    /// Create a new fragmentainer with the given size.
    pub fn new(size: LogicalSize, is_fixed_size: bool) -> Self {
        Self {
            size,
            used_block_size: 0.0,
            is_fixed_size,
        }
    }

    /// Get the remaining block-axis space (infinite for continuous, bounded for paged).
    pub fn remaining_space(&self) -> f32 {
        if self.is_fixed_size {
            (self.size.height - self.used_block_size).max(0.0)
        } else {
            f32::MAX // Infinite for continuous media
        }
    }

    /// Check if this fragmentainer is full (less than 1px remaining).
    pub fn is_full(&self) -> bool {
        self.is_fixed_size && self.remaining_space() < 1.0
    }

    /// Check if a block of the given size can fit in this fragmentainer.
    pub fn can_fit(&self, block_size: f32) -> bool {
        self.remaining_space() >= block_size
    }

    /// Record that block-axis space has been used in this fragmentainer.
    pub fn use_space(&mut self, size: f32) {
        self.used_block_size += size;
    }
}

impl FragmentationContext {
    /// Create a continuous fragmentation context for screen rendering.
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
            Self::Paged { pages, .. } => pages
                .last()
                .expect("Paged context must have at least one page"),
            Self::MultiColumn { columns, .. } => columns
                .last()
                .expect("MultiColumn context must have at least one column"),
            Self::Regions { regions } => regions
                .last()
                .expect("Regions context must have at least one region"),
        }
    }

    /// Get a mutable reference to the current fragmentainer being filled.
    pub fn current_mut(&mut self) -> &mut Fragmentainer {
        match self {
            Self::Continuous { container, .. } => container,
            Self::Paged { pages, .. } => pages
                .last_mut()
                .expect("Paged context must have at least one page"),
            Self::MultiColumn { columns, .. } => columns
                .last_mut()
                .expect("MultiColumn context must have at least one column"),
            Self::Regions { regions } => regions
                .last_mut()
                .expect("Regions context must have at least one region"),
        }
    }

    /// Advance to the next fragmentainer, creating a new one if necessary.
    ///
    /// - For continuous media, this is a no-op (continuous media can't advance).
    /// - For paged media, this creates a new page.
    /// - For regions, this fails if no more regions are available.
    ///
    /// # Returns
    ///
    /// - `Ok(())` if the advance succeeded, `Err(String)` if it failed (e.g., no more regions).
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

    /// Get the page size for paged media, or None for other contexts.
    pub fn page_size(&self) -> Option<LogicalSize> {
        match self {
            Self::Paged { page_size, .. } => Some(*page_size),
            _ => None,
        }
    }

    /// Get the page content height (page height minus margins).
    /// For continuous media, returns f32::MAX.
    pub fn page_content_height(&self) -> f32 {
        match self {
            Self::Continuous { .. } => f32::MAX,
            Self::Paged { page_size, .. } => page_size.height,
            Self::MultiColumn { column_height, .. } => *column_height,
            Self::Regions { regions } => regions.first().map(|r| r.size.height).unwrap_or(f32::MAX),
        }
    }

    /// Check if this is paged media.
    pub fn is_paged(&self) -> bool {
        matches!(self, Self::Paged { .. })
    }
}

//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout state across frames,
//! including caching, incremental updates, and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all the state needed
//! to perform layout and maintain consistency across window resizes and DOM updates.

use std::collections::BTreeMap;

use azul_core::{
    callbacks::DocumentId,
    resources::{Epoch, ImageCache, RenderCallbacks, RendererResources},
    styled_dom::{DomId, StyledDom},
    window::{FullWindowState, LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::LayoutDebugMessage;
use rust_fontconfig::FcFontCache;

use crate::{
    font::parsed::ParsedFont,
    solver3::{self, cache::LayoutCache as Solver3LayoutCache, display_list::DisplayList},
    text3::{
        cache::{FontManager, LayoutCache as TextLayoutCache},
        default::PathLoader,
    },
};

/// A window-level layout manager that encapsulates all layout state and caching.
///
/// This struct owns the layout and text caches, and provides methods to:
/// - Perform initial layout
/// - Incrementally update layout on DOM changes
/// - Generate display lists for rendering
/// - Handle window resizes efficiently
pub struct LayoutWindow {
    /// Layout cache for solver3 (incremental layout tree)
    pub layout_cache: Solver3LayoutCache<ParsedFont>,
    /// Text layout cache for text3 (shaped glyphs, line breaks, etc.)
    pub text_cache: TextLayoutCache<ParsedFont>,
    /// Font manager for loading and caching fonts
    pub font_manager: FontManager<ParsedFont, PathLoader>,
}

impl LayoutWindow {
    /// Create a new layout window with empty caches.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            layout_cache: Solver3LayoutCache {
                tree: None,
                absolute_positions: BTreeMap::new(),
                viewport: None,
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - Display list generation for rendering
    ///
    /// # Arguments
    /// - `styled_dom`: The styled DOM to layout
    /// - `window_state`: Current window dimensions and state
    /// - `renderer_resources`: Resources for image sizing etc.
    /// - `debug_messages`: Optional vector to collect debug/warning messages
    ///
    /// # Returns
    /// The display list ready for rendering, or an error if layout fails.
    pub fn layout_and_generate_display_list(
        &mut self,
        styled_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Prepare viewport from window dimensions
        let viewport = LogicalRect {
            origin: LogicalPosition::new(0.0, 0.0),
            size: window_state.size.dimensions,
        };

        // Prepare scroll offsets (empty for now, TODO: integrate with ScrollStates)
        let scroll_offsets = BTreeMap::new();

        // Prepare selections (empty for now, TODO: integrate with SelectionState)
        let selections = BTreeMap::new();

        // Call the solver3 layout engine
        let display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &selections,
            debug_messages,
        )?;

        Ok(display_list)
    }

    /// Handle a window resize by updating the cached layout.
    ///
    /// This method leverages solver3's incremental layout system to efficiently
    /// relayout only the affected parts of the tree when the window size changes.
    ///
    /// Returns the new display list after the resize.
    pub fn resize_window(
        &mut self,
        styled_dom: StyledDom,
        new_size: LogicalSize,
        renderer_resources: &RendererResources,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Create a temporary FullWindowState with the new size
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = new_size;

        // Reuse the main layout method - solver3 will detect the viewport
        // change and invalidate only what's necessary
        self.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            renderer_resources,
            debug_messages,
        )
    }

    /// Clear all caches (useful for testing or when switching documents).
    pub fn clear_caches(&mut self) {
        self.layout_cache = Solver3LayoutCache {
            tree: None,
            absolute_positions: BTreeMap::new(),
            viewport: None,
        };
        self.text_cache = TextLayoutCache::new();
    }
}

/// Result of a layout operation,包含display list和可能的warnings/debug信息.
pub struct LayoutResult {
    pub display_list: DisplayList,
    pub warnings: Vec<String>,
}

impl LayoutResult {
    pub fn new(display_list: DisplayList, warnings: Vec<String>) -> Self {
        Self {
            display_list,
            warnings,
        }
    }
}

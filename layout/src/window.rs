//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout state across frames,
//! including caching, incremental updates, and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all the state needed
//! to perform layout and maintain consistency across window resizes and DOM updates.

use std::collections::BTreeMap;
use std::collections::BTreeSet;

use azul_core::{
    callbacks::{DocumentId, DomNodeId, ScrollPosition},
    dom::NodeId,
    resources::{Epoch, FontKey, ImageCache, ImageRefHash, RenderCallbacks, RendererResources},
    selection::SelectionState,
    styled_dom::{DomId, NodeHierarchyItemId, StyledDom},
    window::{FullWindowState, LogicalPosition, LogicalRect, LogicalSize},
};
use azul_css::LayoutDebugMessage;
use rust_fontconfig::FcFontCache;

use crate::{
    font::parsed::ParsedFont,
    solver3::{
        self,
        cache::LayoutCache as Solver3LayoutCache,
        display_list::DisplayList,
        layout_tree::LayoutTree,
    },
    text3::{
        cache::{FontManager, LayoutCache as TextLayoutCache},
        default::PathLoader,
    },
};

/// Tracks the state of an IFrame for conditional re-invocation
#[derive(Debug, Clone)]
struct IFrameState {
    /// The bounds of the iframe node at last callback invocation
    bounds: LogicalRect,
    /// The scroll offset at last callback invocation  
    scroll_offset: LogicalPosition,
    /// The DomId assigned to this iframe's content
    dom_id: DomId,
}

/// Result of a layout pass for a single DOM, before display list generation
#[derive(Debug, Clone)]
pub struct DomLayoutResult {
    /// The styled DOM that was laid out
    pub styled_dom: StyledDom,
    /// The layout tree with computed sizes and positions
    pub layout_tree: LayoutTree<ParsedFont>,
    /// Absolute positions of all nodes
    pub absolute_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport used for this layout
    pub viewport: LogicalRect,
}

/// A window-level layout manager that encapsulates all layout state and caching.
///
/// This struct owns the layout and text caches, and provides methods to:
/// - Perform initial layout
/// - Incrementally update layout on DOM changes
/// - Generate display lists for rendering
/// - Handle window resizes efficiently
/// - Manage multiple DOMs (for IFrames)
pub struct LayoutWindow {
    /// Layout cache for solver3 (incremental layout tree) - for the root DOM
    pub layout_cache: Solver3LayoutCache<ParsedFont>,
    /// Text layout cache for text3 (shaped glyphs, line breaks, etc.)
    pub text_cache: TextLayoutCache<ParsedFont>,
    /// Font manager for loading and caching fonts
    pub font_manager: FontManager<ParsedFont, PathLoader>,
    /// Cached layout results for all DOMs (root + iframes)
    /// Maps DomId -> DomLayoutResult
    pub layout_results: BTreeMap<DomId, DomLayoutResult>,
    /// Scroll states for all nodes across all DOMs
    /// Maps (DomId, NodeId) -> ScrollPosition
    pub scroll_states: BTreeMap<(DomId, NodeId), ScrollPosition>,
    /// Selection states for all DOMs
    /// Maps DomId -> SelectionState
    pub selections: BTreeMap<DomId, SelectionState>,
    /// IFrame states for conditional re-invocation
    /// Maps (parent_dom_id, iframe_node_id) -> IFrameState
    pub iframe_states: BTreeMap<(DomId, NodeId), IFrameState>,
    /// Counter for generating unique DomIds for iframes
    next_dom_id: u64,
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
            layout_results: BTreeMap::new(),
            scroll_states: BTreeMap::new(),
            selections: BTreeMap::new(),
            iframe_states: BTreeMap::new(),
            next_dom_id: 1, // Start at 1, 0 is reserved for ROOT_ID
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - IFrame callback invocation and recursive layout
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
        mut styled_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Assign root DomId if not set
        if styled_dom.dom_id.inner == 0 {
            styled_dom.dom_id = DomId::ROOT_ID;
        }
        let dom_id = styled_dom.dom_id;

        // Prepare viewport from window dimensions
        let viewport = LogicalRect {
            origin: LogicalPosition::new(0.0, 0.0),
            size: window_state.size.dimensions,
        };

        // Get scroll offsets for this DOM from our tracked state
        let scroll_offsets = self
            .scroll_states
            .iter()
            .filter(|((d, _), _)| *d == dom_id)
            .map(|((_, node_id), scroll_pos)| (*node_id, scroll_pos.clone()))
            .collect();

        // Clone the styled_dom before moving it
        let styled_dom_clone = styled_dom.clone();

        // Call the solver3 layout engine
        let display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &self.selections,
            debug_messages,
        )?;

        // Store the layout result
        if let Some(tree) = self.layout_cache.tree.clone() {
            self.layout_results.insert(
                dom_id,
                DomLayoutResult {
                    styled_dom: styled_dom_clone,
                    layout_tree: tree,
                    absolute_positions: self.layout_cache.absolute_positions.clone(),
                    viewport,
                },
            );
        }

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
        self.layout_results.clear();
        self.scroll_states.clear();
        self.selections.clear();
        self.iframe_states.clear();
        self.next_dom_id = 1;
    }

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: DomId) -> Option<&DomLayoutResult> {
        self.layout_results.get(&dom_id)
    }

    /// Set scroll position for a node
    pub fn set_scroll_position(&mut self, dom_id: DomId, node_id: NodeId, scroll: ScrollPosition) {
        self.scroll_states.insert((dom_id, node_id), scroll);
    }

    /// Get scroll position for a node
    pub fn get_scroll_position(&self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        self.scroll_states.get(&(dom_id, node_id)).cloned()
    }

    /// Set selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selections.insert(dom_id, selection);
    }

    /// Get selection state for a DOM
    pub fn get_selection(&self, dom_id: DomId) -> Option<&SelectionState> {
        self.selections.get(&dom_id)
    }

    /// Generate a new unique DomId for an iframe
    fn allocate_dom_id(&mut self) -> DomId {
        let id = self.next_dom_id as usize;
        self.next_dom_id += 1;
        DomId { inner: id }
    }

    // Query methods for callbacks

    /// Get the size of a laid-out node
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangle = layout_result.absolute_positions.get(nid)?;
        Some(LogicalSize::new(
            positioned_rectangle.size.width as f32,
            positioned_rectangle.size.height as f32,
        ))
    }

    /// Get the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let positioned_rectangle = layout_result.absolute_positions.get(nid)?;
        Some(LogicalPosition::new(
            positioned_rectangle.origin.x as f32,
            positioned_rectangle.origin.y as f32,
        ))
    }

    /// Get the parent of a node
    pub fn get_parent(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let parent_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .parent_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(parent_id)),
        })
    }

    /// Get the first child of a node
    pub fn get_first_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let hierarchy_item = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?;
        let first_child_id = hierarchy_item.first_child_id(nid)?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(first_child_id)),
        })
    }

    /// Get the next sibling of a node
    pub fn get_next_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let next_sibling_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .next_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(next_sibling_id)),
        })
    }

    /// Get the previous sibling of a node
    pub fn get_previous_sibling(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let prev_sibling_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .previous_sibling_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(prev_sibling_id)),
        })
    }

    /// Get the last child of a node
    pub fn get_last_child(&self, node_id: DomNodeId) -> Option<DomNodeId> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let last_child_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(nid)?
            .last_child_id()?;
        Some(DomNodeId {
            dom: node_id.dom,
            node: NodeHierarchyItemId::from_crate_internal(Some(last_child_id)),
        })
    }

    /// Scan all fonts used in this LayoutWindow (for resource GC)
    pub fn scan_used_fonts(&self) -> BTreeSet<FontKey> {
        let mut fonts = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // TODO: Scan styled_dom for font references
            // This requires accessing the CSS property cache and finding all font-family properties
        }
        fonts
    }

    /// Scan all images used in this LayoutWindow (for resource GC)
    pub fn scan_used_images(&self, _css_image_cache: &ImageCache) -> BTreeSet<ImageRefHash> {
        let mut images = BTreeSet::new();
        for (_dom_id, layout_result) in &self.layout_results {
            // TODO: Scan styled_dom for image references
            // This requires scanning background-image and content properties
        }
        images
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

//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout state across frames,
//! including caching, incremental updates, and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all the state needed
//! to perform layout and maintain consistency across window resizes and DOM updates.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    animation::UpdateImageType,
    callbacks::{FocusTarget, IFrameCallbackReason, Update},
    dom::{Dom, DomId, DomNodeId, NodeId, NodeType},
    events::EasingFunction,
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    gpu::GpuValueCache,
    hit_test::{DocumentId, ScrollPosition, ScrollbarHitId},
    refany::RefAny,
    resources::{
        Epoch, FontKey, GlTextureCache, IdNamespace, ImageCache, ImageMask, ImageRef, ImageRefHash,
        RendererResources,
    },
    selection::SelectionState,
    styled_dom::{NodeHierarchyItemId, StyledDom},
    task::{Duration, Instant, SystemTimeDiff, ThreadId, ThreadSendMsg, TimerId},
    window::{RawWindowHandle, RendererType},
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    parser2::CssApiWrapper,
    props::{basic::FontRef, property::CssProperty},
    AzString, LayoutDebugMessage,
};
use rust_fontconfig::FcFontCache;

use crate::{
    callbacks::{
        CallCallbacksResult, Callback, ExternalSystemCallbacks, FocusUpdateRequest, MenuCallback,
    },
    managers::{
        gpu_state::GpuStateManager,
        iframe::IFrameManager,
        scroll_state::{ScrollManager, ScrollStates},
    },
    solver3::{
        self, cache::LayoutCache as Solver3LayoutCache, display_list::DisplayList,
        layout_tree::LayoutTree,
    },
    text3::{
        cache::{FontManager, LayoutCache as TextLayoutCache, LayoutError},
        default::PathLoader,
    },
    thread::{OptionThreadReceiveMsg, Thread, ThreadReceiveMsg, ThreadWriteBackMsg},
    timer::Timer,
    window_state::{FullWindowState, WindowCreateOptions},
};

// Global atomic counters for generating unique IDs
static DOCUMENT_ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
static ID_NAMESPACE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Helper function to create a unique DocumentId
fn new_document_id() -> DocumentId {
    let namespace_id = new_id_namespace();
    let id = DOCUMENT_ID_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    DocumentId { namespace_id, id }
}

/// Direction for cursor navigation
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum CursorNavigationDirection {
    /// Move cursor up one line
    Up,
    /// Move cursor down one line
    Down,
    /// Move cursor left one character
    Left,
    /// Move cursor right one character
    Right,
    /// Move cursor to start of current line
    LineStart,
    /// Move cursor to end of current line
    LineEnd,
    /// Move cursor to start of document
    DocumentStart,
    /// Move cursor to end of document
    DocumentEnd,
}

/// Result of a cursor movement operation
#[derive(Debug, Clone)]
pub enum CursorMovementResult {
    /// Cursor moved within the same text node
    MovedWithinNode(azul_core::selection::TextCursor),
    /// Cursor moved to a different text node
    MovedToNode {
        dom_id: DomId,
        node_id: NodeId,
        cursor: azul_core::selection::TextCursor,
    },
    /// Cursor is at a boundary and cannot move further
    AtBoundary {
        boundary: crate::text3::cache::TextBoundary,
        cursor: azul_core::selection::TextCursor,
    },
}

/// Error when no cursor destination is available
#[derive(Debug, Clone)]
pub struct NoCursorDestination {
    pub reason: String,
}

/// Helper function to create a unique IdNamespace
fn new_id_namespace() -> IdNamespace {
    let id = ID_NAMESPACE_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    IdNamespace(id)
}

/// Result of a layout pass for a single DOM, before display list generation
#[derive(Debug)]
pub struct DomLayoutResult {
    /// The styled DOM that was laid out
    pub styled_dom: StyledDom,
    /// The layout tree with computed sizes and positions
    pub layout_tree: LayoutTree,
    /// Absolute positions of all nodes
    pub calculated_positions: BTreeMap<usize, LogicalPosition>,
    /// The viewport used for this layout
    pub viewport: LogicalRect,
    /// The generated display list for this DOM.
    pub display_list: DisplayList,
    /// Stable scroll IDs computed from node_data_hash
    /// Maps layout node index -> external scroll ID
    pub scroll_ids: BTreeMap<usize, u64>,
    /// Mapping from scroll IDs to DOM NodeIds for hit testing
    /// This allows us to map WebRender scroll IDs back to DOM nodes
    pub scroll_id_to_node_id: BTreeMap<u64, NodeId>,
}

/// State for tracking scrollbar drag interaction
#[derive(Debug, Clone)]
pub struct ScrollbarDragState {
    pub hit_id: ScrollbarHitId,
    pub initial_mouse_pos: LogicalPosition,
    pub initial_scroll_offset: LogicalPosition,
}

/// Information about the last text edit operation
/// Allows callbacks to query what changed during text input
// Re-export TextChangeset from text_input manager
pub use crate::managers::text_input::TextChangeset;

/// Cached text layout constraints for a node
/// These are the layout parameters that were used to shape the text
#[derive(Debug, Clone)]
pub struct TextConstraintsCache {
    /// Map from (dom_id, node_id) to their layout constraints
    pub constraints: BTreeMap<(DomId, NodeId), crate::text3::cache::UnifiedConstraints>,
}

/// Result of applying callback changes
///
/// This struct consolidates all the outputs from `apply_callback_changes()`,
/// eliminating the need for 18+ mutable reference parameters.
#[derive(Debug, Default)]
pub struct CallbackChangeResult {
    /// Timers to add
    pub timers: FastHashMap<TimerId, crate::timer::Timer>,
    /// Threads to add  
    pub threads: FastHashMap<ThreadId, crate::thread::Thread>,
    /// Timers to remove
    pub timers_removed: FastBTreeSet<TimerId>,
    /// Threads to remove
    pub threads_removed: FastBTreeSet<ThreadId>,
    /// New windows to create
    pub windows_created: Vec<crate::window_state::WindowCreateOptions>,
    /// Menus to open
    pub menus_to_open: Vec<(azul_core::menu::Menu, Option<LogicalPosition>)>,
    /// Tooltips to show
    pub tooltips_to_show: Vec<(AzString, LogicalPosition)>,
    /// Whether to hide tooltip
    pub hide_tooltip: bool,
    /// Whether stopPropagation() was called
    pub stop_propagation: bool,
    /// Whether preventDefault() was called
    pub prevent_default: bool,
    /// Focus target change
    pub focus_target: Option<FocusTarget>,
    /// Text changes that don't require full relayout
    pub words_changed: BTreeMap<DomId, BTreeMap<NodeId, AzString>>,
    /// Image changes (for animated images/video)
    pub images_changed: BTreeMap<DomId, BTreeMap<NodeId, (ImageRef, UpdateImageType)>>,
    /// Image callback nodes that need to be re-rendered (for resize/animations)
    /// Unlike images_changed, this triggers a callback re-invocation
    pub image_callbacks_changed: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    /// IFrame nodes that need to be re-rendered (for content updates)
    /// This triggers the IFrame callback to be called with DomRecreated reason
    pub iframes_to_update: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    /// Clip mask changes (for vector animations)
    pub image_masks_changed: BTreeMap<DomId, BTreeMap<NodeId, ImageMask>>,
    /// CSS property changes from callbacks
    pub css_properties_changed: BTreeMap<DomId, BTreeMap<NodeId, Vec<CssProperty>>>,
    /// Scroll position changes from callbacks
    pub nodes_scrolled: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>,
    /// Modified window state
    pub modified_window_state: FullWindowState,
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
    /// Fragmentation context for this window (continuous for screen, paged for print)
    #[cfg(feature = "pdf")]
    pub fragmentation_context: crate::paged::FragmentationContext,
    /// Layout cache for solver3 (incremental layout tree) - for the root DOM
    pub layout_cache: Solver3LayoutCache,
    /// Text layout cache for text3 (shaped glyphs, line breaks, etc.)
    pub text_cache: TextLayoutCache,
    /// Font manager for loading and caching fonts
    pub font_manager: FontManager<FontRef>,
    /// Cache to store decoded images
    pub image_cache: ImageCache,
    /// Cached layout results for all DOMs (root + iframes)
    pub layout_results: BTreeMap<DomId, DomLayoutResult>,
    /// Scroll state manager for all nodes across all DOMs
    pub scroll_manager: ScrollManager,
    /// Gesture and drag manager for multi-frame interactions (moved from FullWindowState)
    pub gesture_drag_manager: crate::managers::gesture::GestureAndDragManager,
    /// Focus manager for keyboard focus and tab navigation
    pub focus_manager: crate::managers::focus_cursor::FocusManager,
    /// Cursor manager for text cursor position and rendering
    pub cursor_manager: crate::managers::cursor::CursorManager,
    /// File drop manager for cursor state and file drag-drop
    pub file_drop_manager: crate::managers::file_drop::FileDropManager,
    /// Selection manager for text selections across all DOMs
    pub selection_manager: crate::managers::selection::SelectionManager,
    /// Clipboard manager for system clipboard integration
    pub clipboard_manager: crate::managers::clipboard::ClipboardManager,
    /// Drag-drop manager for node and file dragging operations
    pub drag_drop_manager: crate::managers::drag_drop::DragDropManager,
    /// Hover manager for tracking hit test history over multiple frames
    pub hover_manager: crate::managers::hover::HoverManager,
    /// IFrame manager for all nodes across all DOMs
    pub iframe_manager: IFrameManager,
    /// GPU state manager for all nodes across all DOMs
    pub gpu_state_manager: GpuStateManager,
    /// Accessibility manager for screen reader support
    pub a11y_manager: crate::managers::a11y::A11yManager,
    /// Timers associated with this window
    pub timers: BTreeMap<TimerId, Timer>,
    /// Threads running in the background for this window
    pub threads: BTreeMap<ThreadId, Thread>,
    /// Currently loaded fonts and images present in this renderer (window)
    pub renderer_resources: RendererResources,
    /// Renderer type: Hardware-with-software-fallback, pure software or pure hardware renderer?
    pub renderer_type: Option<RendererType>,
    /// Windows state of the window of (current frame - 1): initialized to None on startup
    pub previous_window_state: Option<FullWindowState>,
    /// Window state of this current window (current frame): initialized to the state of
    /// WindowCreateOptions
    pub current_window_state: FullWindowState,
    /// A "document" in WebRender usually corresponds to one tab (i.e. in Azuls case, the whole
    /// window).
    pub document_id: DocumentId,
    /// ID namespace under which every font / image for this window is registered
    pub id_namespace: IdNamespace,
    /// The "epoch" is a frame counter, to remove outdated images, fonts and OpenGL textures when
    /// they're not in use anymore.
    pub epoch: Epoch,
    /// Currently GL textures inside the active CachedDisplayList
    pub gl_texture_cache: GlTextureCache,
    /// State for tracking scrollbar drag interaction
    currently_dragging_thumb: Option<ScrollbarDragState>,
    /// Text input manager - centralizes all text editing logic
    pub text_input_manager: crate::managers::text_input::TextInputManager,
    /// Undo/Redo manager for text editing operations
    pub undo_redo_manager: crate::managers::undo_redo::UndoRedoManager,
    /// Cached text layout constraints for each node
    /// This allows us to re-layout text with the same constraints after edits
    text_constraints_cache: TextConstraintsCache,
    /// Pending IFrame updates from callbacks (processed in next frame)
    /// Map of DomId -> Set of NodeIds that need re-rendering
    pub pending_iframe_updates: BTreeMap<DomId, FastBTreeSet<NodeId>>,
}

fn default_duration_500ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(500))
}

fn default_duration_200ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(200))
}

impl LayoutWindow {
    /// Create a new layout window with empty caches.
    ///
    /// For full initialization with WindowInternal compatibility, use `new_full()`.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            #[cfg(feature = "pdf")]
            fragmentation_context: crate::paged::FragmentationContext::new_continuous(800.0), // Default width, will be updated on first layout
            layout_cache: Solver3LayoutCache {
                tree: None,
                calculated_positions: BTreeMap::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
                counters: BTreeMap::new(),
                float_cache: BTreeMap::new(),
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
            image_cache: ImageCache::default(),
            layout_results: BTreeMap::new(),
            scroll_manager: ScrollManager::new(),
            gesture_drag_manager: crate::managers::gesture::GestureAndDragManager::new(),
            focus_manager: crate::managers::focus_cursor::FocusManager::new(),
            cursor_manager: crate::managers::cursor::CursorManager::new(),
            file_drop_manager: crate::managers::file_drop::FileDropManager::new(),
            selection_manager: crate::managers::selection::SelectionManager::new(),
            clipboard_manager: crate::managers::clipboard::ClipboardManager::new(),
            drag_drop_manager: crate::managers::drag_drop::DragDropManager::new(),
            hover_manager: crate::managers::hover::HoverManager::new(),
            iframe_manager: IFrameManager::new(),
            gpu_state_manager: GpuStateManager::new(
                default_duration_500ms(),
                default_duration_200ms(),
            ),
            a11y_manager: crate::managers::a11y::A11yManager::new(),
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            renderer_resources: RendererResources::default(),
            renderer_type: None,
            previous_window_state: None,
            current_window_state: FullWindowState::default(),
            document_id: new_document_id(),
            id_namespace: new_id_namespace(),
            epoch: Epoch::new(),
            gl_texture_cache: GlTextureCache::default(),
            currently_dragging_thumb: None,
            text_input_manager: crate::managers::text_input::TextInputManager::new(),
            undo_redo_manager: crate::managers::undo_redo::UndoRedoManager::new(),
            text_constraints_cache: TextConstraintsCache {
                constraints: BTreeMap::new(),
            },
            pending_iframe_updates: BTreeMap::new(),
        })
    }

    /// Create a new layout window for paged media (PDF generation).
    ///
    /// This constructor initializes the layout window with a paged fragmentation context,
    /// which will cause content to flow across multiple pages instead of a single continuous
    /// scrollable container.
    ///
    /// # Arguments
    /// - `fc_cache`: Font configuration cache for font loading
    /// - `page_size`: The logical size of each page
    ///
    /// # Returns
    /// A new `LayoutWindow` configured for paged output, or an error if initialization fails.
    #[cfg(feature = "pdf")]
    pub fn new_paged(
        fc_cache: FcFontCache,
        page_size: LogicalSize,
    ) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            fragmentation_context: crate::paged::FragmentationContext::new_paged(page_size),
            layout_cache: Solver3LayoutCache {
                tree: None,
                calculated_positions: BTreeMap::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
                counters: BTreeMap::new(),
                float_cache: BTreeMap::new(),
            },
            text_cache: TextLayoutCache::new(),
            font_manager: FontManager::new(fc_cache)?,
            image_cache: ImageCache::default(),
            layout_results: BTreeMap::new(),
            scroll_manager: ScrollManager::new(),
            gesture_drag_manager: crate::managers::gesture::GestureAndDragManager::new(),
            focus_manager: crate::managers::focus_cursor::FocusManager::new(),
            cursor_manager: crate::managers::cursor::CursorManager::new(),
            file_drop_manager: crate::managers::file_drop::FileDropManager::new(),
            selection_manager: crate::managers::selection::SelectionManager::new(),
            clipboard_manager: crate::managers::clipboard::ClipboardManager::new(),
            drag_drop_manager: crate::managers::drag_drop::DragDropManager::new(),
            hover_manager: crate::managers::hover::HoverManager::new(),
            iframe_manager: IFrameManager::new(),
            gpu_state_manager: GpuStateManager::new(
                default_duration_500ms(),
                default_duration_200ms(),
            ),
            a11y_manager: crate::managers::a11y::A11yManager::new(),
            timers: BTreeMap::new(),
            threads: BTreeMap::new(),
            renderer_resources: RendererResources::default(),
            renderer_type: None,
            previous_window_state: None,
            current_window_state: FullWindowState::default(),
            document_id: new_document_id(),
            id_namespace: new_id_namespace(),
            epoch: Epoch::new(),
            gl_texture_cache: GlTextureCache::default(),
            currently_dragging_thumb: None,
            text_input_manager: crate::managers::text_input::TextInputManager::new(),
            undo_redo_manager: crate::managers::undo_redo::UndoRedoManager::new(),
            text_constraints_cache: TextConstraintsCache {
                constraints: BTreeMap::new(),
            },
            pending_iframe_updates: BTreeMap::new(),
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - IFrame callback invocation and recursive layout
    /// - Display list generation for rendering
    /// - Accessibility tree synchronization
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
        root_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<(), solver3::LayoutError> {
        // Clear previous results for a full relayout
        self.layout_results.clear();

        // Start recursive layout from the root DOM
        let result = self.layout_dom_recursive(
            root_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        );

        // After successful layout, update the accessibility tree
        #[cfg(feature = "a11y")]
        if result.is_ok() {
            let tree_update = crate::managers::a11y::A11yManager::update_tree(
                self.a11y_manager.root_id,
                &self.layout_results,
                &self.current_window_state.title,
                self.current_window_state.size.dimensions,
            );
            // Store the tree_update for platform adapter to consume
            self.a11y_manager.last_tree_update = Some(tree_update);
        }

        // After layout, automatically scroll cursor into view if there's a focused text input
        if result.is_ok() {
            self.scroll_focused_cursor_into_view();
        }

        result
    }

    fn layout_dom_recursive(
        &mut self,
        mut styled_dom: StyledDom,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<(), solver3::LayoutError> {
        if styled_dom.dom_id.inner == 0 {
            styled_dom.dom_id = DomId::ROOT_ID;
        }
        let dom_id = styled_dom.dom_id;

        let viewport = LogicalRect {
            origin: LogicalPosition::zero(),
            size: window_state.size.dimensions,
        };

        // Font Resolution And Loading
        // This must happen BEFORE layout_document() is called
        {
            use crate::solver3::getters::{
                collect_and_resolve_font_chains, 
                collect_font_ids_from_chains,
                compute_fonts_to_load,
                load_fonts_from_disk,
            };
            use crate::text3::default::PathLoader;
            
            // Step 1: Resolve font chains (cached by FontChainKey)
            let chains = collect_and_resolve_font_chains(&styled_dom, &self.font_manager.fc_cache);
            
            // Step 2: Get required font IDs from chains
            let required_fonts = collect_font_ids_from_chains(&chains);
            
            // Step 3: Compute which fonts need to be loaded (diff with already loaded)
            let already_loaded = self.font_manager.get_loaded_font_ids();
            let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
            
            // Step 4: Load missing fonts
            if !fonts_to_load.is_empty() {
                let loader = PathLoader::new();
                let load_result = load_fonts_from_disk(
                    &fonts_to_load,
                    &self.font_manager.fc_cache,
                    |bytes, index| loader.load_font(bytes, index),
                );
                
                // Insert loaded fonts into the font manager
                self.font_manager.insert_fonts(load_result.loaded);
                
                // Log any failures
                for (font_id, error) in &load_result.failed {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "[FontLoading] Failed to load font {:?}: {}", font_id, error
                        )));
                    }
                }
            }
            
            // Step 5: Update font chain cache
            self.font_manager.set_font_chain_cache(chains.into_inner());
        }

        let scroll_offsets = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        let styled_dom_clone = styled_dom.clone();
        let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id).clone();

        let mut display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &self.selection_manager.selections,
            debug_messages,
            Some(&gpu_cache),
            &self.renderer_resources,
            self.id_namespace,
            dom_id,
        )?;

        let tree = self
            .layout_cache
            .tree
            .clone()
            .ok_or(solver3::LayoutError::InvalidTree)?;

        // Get scroll IDs from cache (they were computed during layout_document)
        let scroll_ids = self.layout_cache.scroll_ids.clone();
        let scroll_id_to_node_id = self.layout_cache.scroll_id_to_node_id.clone();

        // Synchronize scrollbar transforms AFTER layout
        self.gpu_state_manager
            .update_scrollbar_transforms(dom_id, &self.scroll_manager, &tree);

        // Scan for IFrames *after* the initial layout pass
        let iframes = self.scan_for_iframes(dom_id, &tree, &self.layout_cache.calculated_positions);

        for (node_id, bounds) in iframes {
            if let Some(child_dom_id) = self.invoke_iframe_callback(
                dom_id,
                node_id,
                bounds,
                window_state,
                renderer_resources,
                system_callbacks,
                debug_messages,
            ) {
                // Insert an IFrame primitive that the renderer will use
                display_list
                    .items
                    .push(crate::solver3::display_list::DisplayListItem::IFrame {
                        child_dom_id,
                        bounds,
                        clip_rect: bounds,
                    });
            }
        }

        // Store the final layout result for this DOM
        self.layout_results.insert(
            dom_id,
            DomLayoutResult {
                styled_dom: styled_dom_clone,
                layout_tree: tree,
                calculated_positions: self.layout_cache.calculated_positions.clone(),
                viewport,
                display_list,
                scroll_ids,
                scroll_id_to_node_id,
            },
        );

        Ok(())
    }

    fn scan_for_iframes(
        &self,
        dom_id: DomId,
        layout_tree: &LayoutTree,
        calculated_positions: &BTreeMap<usize, LogicalPosition>,
    ) -> Vec<(NodeId, LogicalRect)> {
        use azul_core::dom::NodeType;
        layout_tree
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| {
                let node_dom_id = node.dom_node_id?;
                let layout_result = self.layout_results.get(&dom_id)?;
                let node_data = &layout_result.styled_dom.node_data.as_container()[node_dom_id];
                if matches!(node_data.get_node_type(), NodeType::IFrame(_)) {
                    let pos = calculated_positions.get(&idx).copied().unwrap_or_default();
                    let size = node.used_size.unwrap_or_default();
                    Some((node_dom_id, LogicalRect::new(pos, size)))
                } else {
                    None
                }
            })
            .collect()
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
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<DisplayList, crate::solver3::LayoutError> {
        // Create a temporary FullWindowState with the new size
        let mut window_state = FullWindowState::default();
        window_state.size.dimensions = new_size;

        let dom_id = styled_dom.dom_id;

        // Reuse the main layout method - solver3 will detect the viewport
        // change and invalidate only what's necessary
        self.layout_and_generate_display_list(
            styled_dom,
            &window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )?;

        // Retrieve the display list from the layout result
        // We need to take ownership of the display list, so we replace it with an empty one
        self.layout_results
            .get_mut(&dom_id)
            .map(|result| {
                std::mem::replace(&mut result.display_list, DisplayList::default())
            })
            .ok_or(solver3::LayoutError::InvalidTree)
    }

    /// Clear all caches (useful for testing or when switching documents).
    pub fn clear_caches(&mut self) {
        self.layout_cache = Solver3LayoutCache {
            tree: None,
            calculated_positions: BTreeMap::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
            counters: BTreeMap::new(),
            float_cache: BTreeMap::new(),
        };
        self.text_cache = TextLayoutCache::new();
        self.layout_results.clear();
        self.scroll_manager = ScrollManager::new();
        self.selection_manager.clear_all();
    }

    /// Set scroll position for a node
    pub fn set_scroll_position(&mut self, dom_id: DomId, node_id: NodeId, scroll: ScrollPosition) {
        // Convert ScrollPosition to the internal representation
        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        self.scroll_manager.update_node_bounds(
            dom_id,
            node_id,
            scroll.parent_rect,
            scroll.children_rect,
            now.clone(),
        );
        self.scroll_manager
            .set_scroll_position(dom_id, node_id, scroll.children_rect.origin, now);
    }

    /// Get scroll position for a node
    pub fn get_scroll_position(&self, dom_id: DomId, node_id: NodeId) -> Option<ScrollPosition> {
        let states = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        states.get(&node_id).cloned()
    }

    /// Set selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selection_manager.set_selection(dom_id, selection);
    }

    /// Get selection state for a DOM
    pub fn get_selection(&self, dom_id: DomId) -> Option<&SelectionState> {
        self.selection_manager.get_selection(&dom_id)
    }

    /// Invoke an IFrame callback and perform layout on the returned DOM.
    ///
    /// This is the entry point that looks up the necessary `IFrameNode` data before
    /// delegating to the core implementation logic.
    fn invoke_iframe_callback(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "invoke_iframe_callback called for node {:?}",
                node_id
            )));
        }

        // Get the layout result for the parent DOM to access its styled_dom
        let layout_result = self.layout_results.get(&parent_dom_id)?;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Got layout result for parent DOM {:?}",
                parent_dom_id
            )));
        }

        // Get the node data for the IFrame element
        let node_data_container = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id)?;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Got node data at index {}",
                node_id.index()
            )));
        }

        // Extract the IFrame node, cloning it to avoid borrow checker issues
        let iframe_node = match node_data.get_node_type() {
            NodeType::IFrame(iframe) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info("Node is IFrame type".to_string()));
                }
                iframe.clone()
            }
            other => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "Node is NOT IFrame, type = {:?}",
                        other
                    )));
                }
                return None;
            }
        };

        // Call the actual implementation with all necessary data
        self.invoke_iframe_callback_impl(
            parent_dom_id,
            node_id,
            &iframe_node,
            bounds,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
    }

    /// Core implementation for invoking an IFrame callback and managing the recursive layout.
    ///
    /// This method implements the 5 conditional re-invocation rules by coordinating
    /// with the `IFrameManager` and `ScrollManager`.
    ///
    /// # Returns
    ///
    /// `Some(child_dom_id)` if the callback was invoked and the child DOM was laid out.
    /// The parent's display list generator will then use this ID to reference the child's
    /// display list. Returns `None` if the callback was not invoked.
    fn invoke_iframe_callback_impl(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        iframe_node: &azul_core::dom::IFrameNode,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        // Get current time from system callbacks for state updates
        let now = (system_callbacks.get_system_time_fn.cb)();

        // Update node bounds in the scroll manager. This is necessary for the IFrameManager
        // to correctly detect edge scroll conditions.
        self.scroll_manager.update_node_bounds(
            parent_dom_id,
            node_id,
            bounds,
            LogicalRect::new(LogicalPosition::zero(), bounds.size), // Initial content_rect
            now.clone(),
        );

        // Check with the IFrameManager to see if re-invocation is necessary.
        // It handles all 5 conditional rules.
        let reason = match self.iframe_manager.check_reinvoke(
            parent_dom_id,
            node_id,
            &self.scroll_manager,
            bounds,
        ) {
            Some(r) => r,
            None => {
                // No re-invocation needed, but we still need the child_dom_id for the display list.
                return self
                    .iframe_manager
                    .get_nested_dom_id(parent_dom_id, node_id);
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "IFrame ({:?}, {:?}) - Reason: {:?}",
                parent_dom_id, node_id, reason
            )));
        }

        let scroll_offset = self
            .scroll_manager
            .get_current_offset(parent_dom_id, node_id)
            .unwrap_or_default();
        let hidpi_factor = window_state.size.get_hidpi_factor();

        // Create IFrameCallbackInfo with the most up-to-date state
        let mut callback_info = azul_core::callbacks::IFrameCallbackInfo::new(
            reason,
            &*self.font_manager.fc_cache,
            &self.image_cache,
            window_state.theme,
            azul_core::callbacks::HidpiAdjustedBounds {
                logical_size: bounds.size,
                hidpi_factor,
            },
            bounds.size,
            scroll_offset,
            bounds.size,
            LogicalPosition::zero(),
        );

        // Clone the user data for the callback
        let mut callback_data = iframe_node.data.clone();

        // Invoke the user's IFrame callback
        let callback_return = (iframe_node.callback.cb)(&mut callback_data, &mut callback_info);

        // Mark the IFrame as invoked to prevent duplicate InitialRender calls
        self.iframe_manager
            .mark_invoked(parent_dom_id, node_id, reason);

        // Get the child StyledDom from the callback's return value
        let mut child_styled_dom = match callback_return.dom {
            azul_core::styled_dom::OptionStyledDom::Some(dom) => dom,
            azul_core::styled_dom::OptionStyledDom::None => {
                // If the callback returns None, it's an optimization hint.
                if reason == IFrameCallbackReason::InitialRender {
                    // For the very first render, create an empty div as a fallback.
                    let mut empty_dom = Dom::div();
                    let empty_css = CssApiWrapper::empty();
                    empty_dom.style(empty_css)
                } else {
                    // For subsequent calls, returning None means "keep the old DOM".
                    // We just need to update the scroll info and return the existing child ID.
                    self.iframe_manager.update_iframe_info(
                        parent_dom_id,
                        node_id,
                        callback_return.scroll_size,
                        callback_return.virtual_scroll_size,
                    );
                    return self
                        .iframe_manager
                        .get_nested_dom_id(parent_dom_id, node_id);
                }
            }
        };

        // Get or create a unique DomId for the IFrame's content
        let child_dom_id = self
            .iframe_manager
            .get_or_create_nested_dom_id(parent_dom_id, node_id);
        child_styled_dom.dom_id = child_dom_id;

        // Update the IFrameManager with the new scroll sizes from the callback
        self.iframe_manager.update_iframe_info(
            parent_dom_id,
            node_id,
            callback_return.scroll_size,
            callback_return.virtual_scroll_size,
        );

        // **RECURSIVE LAYOUT STEP**
        // Perform a full layout pass on the child DOM. This will recursively handle
        // any IFrames within this IFrame.
        self.layout_dom_recursive(
            child_styled_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
        .ok()?;

        Some(child_dom_id)
    }

    // Query methods for callbacks

    /// Get the size of a laid-out node
    pub fn get_node_size(&self, node_id: DomNodeId) -> Option<LogicalSize> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let layout_node = layout_result.layout_tree.get(nid.index())?;
        layout_node.used_size
    }

    /// Get the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        let position = layout_result.calculated_positions.get(&nid.index())?;
        Some(*position)
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
        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
        let hierarchy_item = node_hierarchy.get(nid)?;
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

    /// Helper function to convert ScrollStates to nested format for CallbackInfo
    fn get_nested_scroll_states(
        &self,
        dom_id: DomId,
    ) -> BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, ScrollPosition>> {
        let mut nested = BTreeMap::new();
        let scroll_states = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        let mut inner = BTreeMap::new();
        for (node_id, scroll_pos) in scroll_states {
            inner.insert(
                NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                scroll_pos,
            );
        }
        nested.insert(dom_id, inner);
        nested
    }

    // Timer Management

    /// Add a timer to this window
    pub fn add_timer(&mut self, timer_id: TimerId, timer: Timer) {
        self.timers.insert(timer_id, timer);
    }

    /// Remove a timer from this window
    pub fn remove_timer(&mut self, timer_id: &TimerId) -> Option<Timer> {
        self.timers.remove(timer_id)
    }

    /// Get a reference to a timer
    pub fn get_timer(&self, timer_id: &TimerId) -> Option<&Timer> {
        self.timers.get(timer_id)
    }

    /// Get a mutable reference to a timer
    pub fn get_timer_mut(&mut self, timer_id: &TimerId) -> Option<&mut Timer> {
        self.timers.get_mut(timer_id)
    }

    /// Get all timer IDs
    pub fn get_timer_ids(&self) -> Vec<TimerId> {
        self.timers.keys().copied().collect()
    }

    /// Tick all timers (called once per frame)
    /// Returns a list of timer IDs that are ready to run
    pub fn tick_timers(&mut self, current_time: azul_core::task::Instant) -> Vec<TimerId> {
        let mut ready_timers = Vec::new();

        for (timer_id, timer) in &mut self.timers {
            // Check if timer is ready to run
            // This logic should match the timer's internal state
            // For now, we'll just collect all timer IDs
            // The actual readiness check will be done when invoking
            ready_timers.push(*timer_id);
        }

        ready_timers
    }

    // Thread Management

    /// Add a thread to this window
    pub fn add_thread(&mut self, thread_id: ThreadId, thread: Thread) {
        self.threads.insert(thread_id, thread);
    }

    /// Remove a thread from this window
    pub fn remove_thread(&mut self, thread_id: &ThreadId) -> Option<Thread> {
        self.threads.remove(thread_id)
    }

    /// Get a reference to a thread
    pub fn get_thread(&self, thread_id: &ThreadId) -> Option<&Thread> {
        self.threads.get(thread_id)
    }

    /// Get a mutable reference to a thread
    pub fn get_thread_mut(&mut self, thread_id: &ThreadId) -> Option<&mut Thread> {
        self.threads.get_mut(thread_id)
    }

    /// Get all thread IDs
    pub fn get_thread_ids(&self) -> Vec<ThreadId> {
        self.threads.keys().copied().collect()
    }

    // CallbackChange Processing

    /// Apply callback changes that were collected during callback execution
    ///
    /// This method processes all changes accumulated in the CallbackChange vector
    /// and applies them to the appropriate state. This is called after a callback
    /// returns to ensure atomic application of all changes.
    ///
    /// Returns a `CallbackChangeResult` containing all the changes to be applied.
    pub fn apply_callback_changes(
        &mut self,
        changes: Vec<crate::callbacks::CallbackChange>,
        current_window_state: &FullWindowState,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
    ) -> CallbackChangeResult {
        use crate::callbacks::CallbackChange;

        let mut result = CallbackChangeResult {
            modified_window_state: current_window_state.clone(),
            ..Default::default()
        };
        for change in changes {
            match change {
                CallbackChange::ModifyWindowState { state } => {
                    result.modified_window_state = state;
                }
                CallbackChange::CreateNewWindow { options } => {
                    result.windows_created.push(options);
                }
                CallbackChange::CloseWindow => {
                    // TODO: Implement window close mechanism
                    // This needs to set a flag that the window should close
                }
                CallbackChange::SetFocusTarget { target } => {
                    result.focus_target = Some(target);
                }
                CallbackChange::StopPropagation => {
                    result.stop_propagation = true;
                }
                CallbackChange::PreventDefault => {
                    result.prevent_default = true;
                }
                CallbackChange::AddTimer { timer_id, timer } => {
                    result.timers.insert(timer_id, timer);
                }
                CallbackChange::RemoveTimer { timer_id } => {
                    result.timers_removed.insert(timer_id);
                }
                CallbackChange::AddThread { thread_id, thread } => {
                    result.threads.insert(thread_id, thread);
                }
                CallbackChange::RemoveThread { thread_id } => {
                    result.threads_removed.insert(thread_id);
                }
                CallbackChange::ChangeNodeText {
                    dom_id,
                    node_id,
                    text,
                } => {
                    result
                        .words_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, text);
                }
                CallbackChange::ChangeNodeImage {
                    dom_id,
                    node_id,
                    image,
                    update_type,
                } => {
                    result
                        .images_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, (image, update_type));
                }
                CallbackChange::UpdateImageCallback { dom_id, node_id } => {
                    result
                        .image_callbacks_changed
                        .entry(dom_id)
                        .or_insert_with(FastBTreeSet::new)
                        .insert(node_id);
                }
                CallbackChange::UpdateIFrame { dom_id, node_id } => {
                    result
                        .iframes_to_update
                        .entry(dom_id)
                        .or_insert_with(FastBTreeSet::new)
                        .insert(node_id);
                }
                CallbackChange::ChangeNodeImageMask {
                    dom_id,
                    node_id,
                    mask,
                } => {
                    result
                        .image_masks_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, mask);
                }
                CallbackChange::ChangeNodeCssProperties {
                    dom_id,
                    node_id,
                    properties,
                } => {
                    result
                        .css_properties_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, properties);
                }
                CallbackChange::ScrollTo {
                    dom_id,
                    node_id,
                    position,
                } => {
                    result
                        .nodes_scrolled
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(node_id, position);
                }
                CallbackChange::AddImageToCache { id, image } => {
                    image_cache.add_css_image_id(id, image);
                }
                CallbackChange::RemoveImageFromCache { id } => {
                    image_cache.delete_css_image_id(&id);
                }
                CallbackChange::ReloadSystemFonts => {
                    *system_fonts = FcFontCache::build();
                }
                CallbackChange::OpenMenu { menu, position } => {
                    result.menus_to_open.push((menu, position));
                }
                CallbackChange::ShowTooltip { text, position } => {
                    result.tooltips_to_show.push((text, position));
                }
                CallbackChange::HideTooltip => {
                    result.hide_tooltip = true;
                }
                CallbackChange::InsertText {
                    dom_id,
                    node_id,
                    text,
                } => {
                    // Record text input for the node
                    use azul_core::styled_dom::NodeHierarchyItemId;
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: hierarchy_id,
                    };

                    // Get old text from node
                    let old_inline_content = self.get_text_before_textinput(dom_id, node_id);
                    let old_text = self.extract_text_from_inline_content(&old_inline_content);

                    // Record the text input
                    use crate::managers::text_input::TextInputSource;
                    self.text_input_manager.record_input(
                        dom_node_id,
                        text.to_string(),
                        old_text,
                        TextInputSource::Programmatic,
                    );
                }
                CallbackChange::DeleteBackward { dom_id, node_id } => {
                    // Get current cursor/selection
                    if let Some(cursor) = self.cursor_manager.get_cursor() {
                        // Get current content
                        let content = self.get_text_before_textinput(dom_id, node_id);

                        // Apply delete backward using text3::edit
                        use azul_core::selection::Selection;

                        use crate::text3::edit::{delete_backward, TextEdit};
                        let mut new_content = content.clone();
                        let (updated_content, new_cursor) =
                            delete_backward(&mut new_content, cursor);

                        // Update cursor position
                        self.cursor_manager
                            .move_cursor_to(new_cursor, dom_id, node_id);

                        // Update text cache
                        self.update_text_cache_after_edit(dom_id, node_id, updated_content);

                        // Mark node as dirty
                        use azul_core::styled_dom::NodeHierarchyItemId;
                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                        let dom_node_id = DomNodeId {
                            dom: dom_id,
                            node: hierarchy_id,
                        };
                        // Note: Dirty marking happens in the caller
                    }
                }
                CallbackChange::DeleteForward { dom_id, node_id } => {
                    // Get current cursor/selection
                    if let Some(cursor) = self.cursor_manager.get_cursor() {
                        // Get current content
                        let content = self.get_text_before_textinput(dom_id, node_id);

                        // Apply delete forward using text3::edit
                        use azul_core::selection::Selection;

                        use crate::text3::edit::{delete_forward, TextEdit};
                        let mut new_content = content.clone();
                        let (updated_content, new_cursor) =
                            delete_forward(&mut new_content, cursor);

                        // Update cursor position
                        self.cursor_manager
                            .move_cursor_to(new_cursor, dom_id, node_id);

                        // Update text cache
                        self.update_text_cache_after_edit(dom_id, node_id, updated_content);
                    }
                }
                CallbackChange::MoveCursor {
                    dom_id,
                    node_id,
                    cursor,
                } => {
                    // Update cursor position in CursorManager
                    self.cursor_manager.move_cursor_to(cursor, dom_id, node_id);
                }
                CallbackChange::SetSelection {
                    dom_id,
                    node_id,
                    selection,
                } => {
                    // Update selection in SelectionManager
                    use azul_core::styled_dom::NodeHierarchyItemId;
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: hierarchy_id,
                    };

                    match selection {
                        azul_core::selection::Selection::Cursor(cursor) => {
                            self.cursor_manager.move_cursor_to(cursor, dom_id, node_id);
                            self.selection_manager.clear_all();
                        }
                        azul_core::selection::Selection::Range(range) => {
                            self.cursor_manager
                                .move_cursor_to(range.start, dom_id, node_id);
                            // TODO: Set selection range in SelectionManager
                            // self.selection_manager.set_selection(dom_node_id, range);
                        }
                    }
                }
                CallbackChange::SetTextChangeset { changeset } => {
                    // Override the current text input changeset
                    // This allows user callbacks to modify what text will be inserted
                    self.text_input_manager.pending_changeset = Some(changeset);
                }
                // Cursor Movement Operations
                CallbackChange::MoveCursorLeft {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_left(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorRight {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_right(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorUp {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_up(*cursor, &mut None, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorDown {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_down(*cursor, &mut None, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorToLineStart {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_to_line_start(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorToLineEnd {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    if let Some(new_cursor) =
                        self.move_cursor_in_node(dom_id, node_id, |layout, cursor| {
                            layout.move_cursor_to_line_end(*cursor, &mut None)
                        })
                    {
                        self.handle_cursor_movement(dom_id, node_id, new_cursor, extend_selection);
                    }
                }
                CallbackChange::MoveCursorToDocumentStart {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    // Document start is the first cluster in the layout
                    if let Some(new_cursor) = self.get_inline_layout_for_node(dom_id, node_id) {
                        if let Some(first_cluster) = new_cursor
                            .items
                            .first()
                            .and_then(|item| item.item.as_cluster())
                        {
                            let doc_start_cursor = azul_core::selection::TextCursor {
                                cluster_id: first_cluster.source_cluster_id,
                                affinity: azul_core::selection::CursorAffinity::Leading,
                            };
                            self.handle_cursor_movement(
                                dom_id,
                                node_id,
                                doc_start_cursor,
                                extend_selection,
                            );
                        }
                    }
                }
                CallbackChange::MoveCursorToDocumentEnd {
                    dom_id,
                    node_id,
                    extend_selection,
                } => {
                    // Document end is the last cluster in the layout
                    if let Some(layout) = self.get_inline_layout_for_node(dom_id, node_id) {
                        if let Some(last_cluster) =
                            layout.items.last().and_then(|item| item.item.as_cluster())
                        {
                            let doc_end_cursor = azul_core::selection::TextCursor {
                                cluster_id: last_cluster.source_cluster_id,
                                affinity: azul_core::selection::CursorAffinity::Trailing,
                            };
                            self.handle_cursor_movement(
                                dom_id,
                                node_id,
                                doc_end_cursor,
                                extend_selection,
                            );
                        }
                    }
                }
                // Clipboard Operations (Override)
                CallbackChange::SetCopyContent { target, content } => {
                    // Store clipboard content to be written to system clipboard
                    // This will be picked up by the platform's sync_clipboard() method
                    self.clipboard_manager.set_copy_content(content);
                }
                CallbackChange::SetCutContent { target, content } => {
                    // Same as copy, but the deletion is handled separately
                    self.clipboard_manager.set_copy_content(content);
                }
                CallbackChange::SetSelectAllRange { target, range } => {
                    // Override selection range for select-all operation
                    // Convert DomNodeId back to internal NodeId
                    if let Some(node_id_internal) = target.node.into_crate_internal() {
                        let dom_node_id = azul_core::dom::DomNodeId {
                            dom: target.dom,
                            node: target.node,
                        };
                        self.selection_manager
                            .set_range(target.dom, dom_node_id, range);
                    }
                }
            }
        }

        // Sync cursor to selection manager for rendering
        // This must happen after all cursor updates
        self.sync_cursor_to_selection_manager();

        result
    }

    /// Helper: Get inline layout for a node
    fn get_inline_layout_for_node(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<&Arc<crate::text3::cache::UnifiedLayout>> {
        let layout_result = self.layout_results.get(&dom_id)?;

        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&node_id)?;
        let layout_index = *layout_indices.first()?;

        let layout_node = layout_result.layout_tree.nodes.get(layout_index)?;
        layout_node.inline_layout_result.as_ref().map(|c| c.get_layout())
    }

    /// Helper: Move cursor using a movement function and return the new cursor if it changed
    fn move_cursor_in_node<F>(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        movement_fn: F,
    ) -> Option<azul_core::selection::TextCursor>
    where
        F: FnOnce(
            &crate::text3::cache::UnifiedLayout,
            &azul_core::selection::TextCursor,
        ) -> azul_core::selection::TextCursor,
    {
        let current_cursor = self.cursor_manager.get_cursor()?;
        let layout = self.get_inline_layout_for_node(dom_id, node_id)?;

        let new_cursor = movement_fn(layout, current_cursor);

        // Only return if cursor actually moved
        if new_cursor != *current_cursor {
            Some(new_cursor)
        } else {
            None
        }
    }

    /// Helper: Handle cursor movement with optional selection extension
    fn handle_cursor_movement(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        new_cursor: azul_core::selection::TextCursor,
        extend_selection: bool,
    ) {
        use azul_core::styled_dom::NodeHierarchyItemId;

        if extend_selection {
            // Get the current cursor as the selection anchor
            if let Some(old_cursor) = self.cursor_manager.get_cursor() {
                // Create DomNodeId for the selection
                let dom_node_id = azul_core::dom::DomNodeId {
                    dom: dom_id,
                    node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
                };

                // Create a selection range from old cursor to new cursor
                let selection_range = if new_cursor.cluster_id.start_byte_in_run
                    < old_cursor.cluster_id.start_byte_in_run
                {
                    // Moving backwards
                    azul_core::selection::SelectionRange {
                        start: new_cursor,
                        end: *old_cursor,
                    }
                } else {
                    // Moving forwards
                    azul_core::selection::SelectionRange {
                        start: *old_cursor,
                        end: new_cursor,
                    }
                };

                // Set the selection range in SelectionManager
                self.selection_manager
                    .set_range(dom_id, dom_node_id, selection_range);
            }

            // Move cursor to new position
            self.cursor_manager
                .move_cursor_to(new_cursor, dom_id, node_id);
        } else {
            // Just move cursor without extending selection
            self.cursor_manager
                .move_cursor_to(new_cursor, dom_id, node_id);

            // Clear any existing selection
            self.selection_manager.clear_selection(&dom_id);
        }
    }

    // Gpu Value Cache Management

    /// Get the GPU value cache for a specific DOM
    pub fn get_gpu_cache(&self, dom_id: &DomId) -> Option<&GpuValueCache> {
        self.gpu_state_manager.caches.get(dom_id)
    }

    /// Get a mutable reference to the GPU value cache for a specific DOM
    pub fn get_gpu_cache_mut(&mut self, dom_id: &DomId) -> Option<&mut GpuValueCache> {
        self.gpu_state_manager.caches.get_mut(dom_id)
    }

    /// Get or create a GPU value cache for a specific DOM
    pub fn get_or_create_gpu_cache(&mut self, dom_id: DomId) -> &mut GpuValueCache {
        self.gpu_state_manager
            .caches
            .entry(dom_id)
            .or_insert_with(GpuValueCache::default)
    }

    // Layout Result Access

    /// Get a layout result for a specific DOM
    pub fn get_layout_result(&self, dom_id: &DomId) -> Option<&DomLayoutResult> {
        self.layout_results.get(dom_id)
    }

    /// Get a mutable layout result for a specific DOM
    pub fn get_layout_result_mut(&mut self, dom_id: &DomId) -> Option<&mut DomLayoutResult> {
        self.layout_results.get_mut(dom_id)
    }

    /// Get all DOM IDs that have layout results
    pub fn get_dom_ids(&self) -> Vec<DomId> {
        self.layout_results.keys().copied().collect()
    }

    // Hit-Test Computation

    /// Compute the cursor type hit-test from a full hit-test
    ///
    /// This determines which mouse cursor to display based on the CSS cursor
    /// properties of the hovered nodes.
    pub fn compute_cursor_type_hit_test(
        &self,
        hit_test: &crate::hit_test::FullHitTest,
    ) -> crate::hit_test::CursorTypeHitTest {
        crate::hit_test::CursorTypeHitTest::new(hit_test, self)
    }

    // TODO: Implement compute_hit_test() once we have the actual hit-testing logic
    // This would involve:
    // 1. Converting screen coordinates to layout coordinates
    // 2. Traversing the layout tree to find nodes under the cursor
    // 3. Handling z-index and stacking contexts
    // 4. Building the FullHitTest structure

    /// Synchronize scrollbar opacity values with the GPU value cache.
    ///
    /// This method updates GPU opacity keys for all scrollbars based on scroll activity
    /// tracked by the ScrollManager. It enables smooth scrollbar fading without
    /// requiring display list regeneration.
    ///
    /// # Arguments
    ///
    /// * `dom_id` - The DOM to synchronize scrollbar opacity for
    /// * `layout_tree` - The layout tree containing scrollbar information
    /// * `now` - Current timestamp for calculating fade progress
    /// * `fade_delay` - Delay before scrollbar starts fading (e.g., 500ms)
    /// * `fade_duration` - Duration of the fade animation (e.g., 200ms)
    ///
    /// # Returns
    ///
    /// A vector of GPU scrollbar opacity change events

    /// Helper function to calculate scrollbar opacity based on activity time
    fn calculate_scrollbar_opacity(
        last_activity: Option<Instant>,
        now: Instant,
        fade_delay: Duration,
        fade_duration: Duration,
    ) -> f32 {
        let Some(last_activity) = last_activity else {
            return 0.0;
        };

        let time_since_activity = now.duration_since(&last_activity);

        // Phase 1: Scrollbar stays fully visible during fade_delay
        if time_since_activity.div(&fade_delay) < 1.0 {
            return 1.0;
        }

        // Phase 2: Fade out over fade_duration
        let time_into_fade_ms = time_since_activity.div(&fade_delay) - 1.0;
        let fade_progress = (time_into_fade_ms * fade_duration.div(&fade_duration)).min(1.0);

        // Phase 3: Fully faded
        (1.0 - fade_progress).max(0.0)
    }

    /// Synchronize scrollbar opacity values with the GPU value cache.
    ///
    /// Static method that takes individual components instead of &mut self to avoid borrow
    /// conflicts.
    pub fn synchronize_scrollbar_opacity(
        gpu_state_manager: &mut GpuStateManager,
        scroll_manager: &ScrollManager,
        dom_id: DomId,
        layout_tree: &LayoutTree,
        system_callbacks: &ExternalSystemCallbacks,
        fade_delay: azul_core::task::Duration,
        fade_duration: azul_core::task::Duration,
    ) -> Vec<azul_core::gpu::GpuScrollbarOpacityEvent> {
        use azul_core::{gpu::GpuScrollbarOpacityEvent, resources::OpacityKey};

        let mut events = Vec::new();
        let gpu_cache = gpu_state_manager.caches.entry(dom_id).or_default();

        // Get current time from system callbacks
        let now = (system_callbacks.get_system_time_fn.cb)();

        // Iterate over all nodes with scrollbar info
        for (node_idx, node) in layout_tree.nodes.iter().enumerate() {
            // Check if node needs scrollbars
            let scrollbar_info = match &node.scrollbar_info {
                Some(info) => info,
                None => continue,
            };

            let node_id = match node.dom_node_id {
                Some(nid) => nid,
                None => continue, // Skip anonymous boxes
            };

            // Calculate current opacity from ScrollManager
            let vertical_opacity = if scrollbar_info.needs_vertical {
                Self::calculate_scrollbar_opacity(
                    scroll_manager.get_last_activity_time(dom_id, node_id),
                    now.clone(),
                    fade_delay,
                    fade_duration,
                )
            } else {
                0.0
            };

            let horizontal_opacity = if scrollbar_info.needs_horizontal {
                Self::calculate_scrollbar_opacity(
                    scroll_manager.get_last_activity_time(dom_id, node_id),
                    now.clone(),
                    fade_delay,
                    fade_duration,
                )
            } else {
                0.0
            };

            // Handle vertical scrollbar
            if scrollbar_info.needs_vertical && vertical_opacity > 0.001 {
                let key = (dom_id, node_id);
                let existing = gpu_cache.scrollbar_v_opacity_values.get(&key);

                match existing {
                    None => {
                        let opacity_key = OpacityKey::unique();
                        gpu_cache.scrollbar_v_opacity_keys.insert(key, opacity_key);
                        gpu_cache
                            .scrollbar_v_opacity_values
                            .insert(key, vertical_opacity);
                        events.push(GpuScrollbarOpacityEvent::VerticalAdded(
                            dom_id,
                            node_id,
                            opacity_key,
                            vertical_opacity,
                        ));
                    }
                    Some(&old_opacity) if (old_opacity - vertical_opacity).abs() > 0.001 => {
                        let opacity_key = gpu_cache.scrollbar_v_opacity_keys[&key];
                        gpu_cache
                            .scrollbar_v_opacity_values
                            .insert(key, vertical_opacity);
                        events.push(GpuScrollbarOpacityEvent::VerticalChanged(
                            dom_id,
                            node_id,
                            opacity_key,
                            old_opacity,
                            vertical_opacity,
                        ));
                    }
                    _ => {}
                }
            } else {
                // Remove if scrollbar no longer needed or fully transparent
                let key = (dom_id, node_id);
                if let Some(opacity_key) = gpu_cache.scrollbar_v_opacity_keys.remove(&key) {
                    gpu_cache.scrollbar_v_opacity_values.remove(&key);
                    events.push(GpuScrollbarOpacityEvent::VerticalRemoved(
                        dom_id,
                        node_id,
                        opacity_key,
                    ));
                }
            }

            // Handle horizontal scrollbar (same logic)
            if scrollbar_info.needs_horizontal && horizontal_opacity > 0.001 {
                let key = (dom_id, node_id);
                let existing = gpu_cache.scrollbar_h_opacity_values.get(&key);

                match existing {
                    None => {
                        let opacity_key = OpacityKey::unique();
                        gpu_cache.scrollbar_h_opacity_keys.insert(key, opacity_key);
                        gpu_cache
                            .scrollbar_h_opacity_values
                            .insert(key, horizontal_opacity);
                        events.push(GpuScrollbarOpacityEvent::HorizontalAdded(
                            dom_id,
                            node_id,
                            opacity_key,
                            horizontal_opacity,
                        ));
                    }
                    Some(&old_opacity) if (old_opacity - horizontal_opacity).abs() > 0.001 => {
                        let opacity_key = gpu_cache.scrollbar_h_opacity_keys[&key];
                        gpu_cache
                            .scrollbar_h_opacity_values
                            .insert(key, horizontal_opacity);
                        events.push(GpuScrollbarOpacityEvent::HorizontalChanged(
                            dom_id,
                            node_id,
                            opacity_key,
                            old_opacity,
                            horizontal_opacity,
                        ));
                    }
                    _ => {}
                }
            } else {
                // Remove if scrollbar no longer needed or fully transparent
                let key = (dom_id, node_id);
                if let Some(opacity_key) = gpu_cache.scrollbar_h_opacity_keys.remove(&key) {
                    gpu_cache.scrollbar_h_opacity_values.remove(&key);
                    events.push(GpuScrollbarOpacityEvent::HorizontalRemoved(
                        dom_id,
                        node_id,
                        opacity_key,
                    ));
                }
            }
        }

        events
    }

    /// Compute stable scroll IDs for all scrollable nodes in a layout tree
    ///
    /// This should be called after layout but before display list generation.
    /// It creates stable IDs based on node_data_hash that persist across frames.
    ///
    /// Returns:
    /// - scroll_ids: Map from layout node index -> external scroll ID
    /// - scroll_id_to_node_id: Map from scroll ID -> DOM NodeId (for hit testing)
    pub fn compute_scroll_ids(
        layout_tree: &LayoutTree,
        styled_dom: &azul_core::styled_dom::StyledDom,
    ) -> (BTreeMap<usize, u64>, BTreeMap<u64, NodeId>) {
        use azul_css::props::layout::LayoutOverflow;

        use crate::solver3::getters::{get_overflow_x, get_overflow_y};

        let mut scroll_ids = BTreeMap::new();
        let mut scroll_id_to_node_id = BTreeMap::new();

        // Iterate through all layout nodes
        for (layout_idx, node) in layout_tree.nodes.iter().enumerate() {
            let Some(dom_node_id) = node.dom_node_id else {
                continue;
            };

            // Get the node state
            let styled_node_state = styled_dom
                .styled_nodes
                .as_container()
                .get(dom_node_id)
                .map(|n| n.state.clone())
                .unwrap_or_default();

            // Check if this node has scroll overflow
            let overflow_x = get_overflow_x(styled_dom, dom_node_id, &styled_node_state);
            let overflow_y = get_overflow_y(styled_dom, dom_node_id, &styled_node_state);

            let is_scrollable = overflow_x.is_scroll() || overflow_y.is_scroll();

            if !is_scrollable {
                continue;
            }

            // Generate stable scroll ID from node_data_hash
            // Use node_data_hash to create a stable ID that persists across frames
            let scroll_id = node.node_data_hash;

            scroll_ids.insert(layout_idx, scroll_id);
            scroll_id_to_node_id.insert(scroll_id, dom_node_id);
        }

        (scroll_ids, scroll_id_to_node_id)
    }

    /// Get the layout rectangle for a specific DOM node in logical coordinates
    ///
    /// This is useful in callbacks to get the position and size of the hit node
    /// for positioning menus, tooltips, or other overlays.
    ///
    /// Returns None if the node is not currently laid out (e.g., display:none)
    pub fn get_node_layout_rect(
        &self,
        node_id: azul_core::dom::DomNodeId,
    ) -> Option<azul_core::geom::LogicalRect> {
        use azul_core::geom::{LogicalPosition, LogicalRect, LogicalSize};

        // Get the layout tree from cache
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Find the layout node index corresponding to this DOM node
        // Convert NodeHierarchyItemId to Option<NodeId> for comparison
        let target_node_id = node_id.node.into_crate_internal();
        let layout_idx = layout_tree
            .nodes
            .iter()
            .position(|node| node.dom_node_id == target_node_id)?;

        // Get the calculated layout position from cache (already in logical units)
        let calc_pos = self.layout_cache.calculated_positions.get(&layout_idx)?;

        // Get the layout node for size information
        let layout_node = layout_tree.nodes.get(layout_idx)?;

        // Get the used size (the actual laid-out size)
        let used_size = layout_node.used_size?;

        // Convert size to logical coordinates
        let hidpi_factor = self
            .current_window_state
            .size
            .get_hidpi_factor()
            .inner
            .get();

        Some(LogicalRect::new(
            LogicalPosition::new(calc_pos.x as f32, calc_pos.y as f32),
            LogicalSize::new(
                used_size.width / hidpi_factor,
                used_size.height / hidpi_factor,
            ),
        ))
    }

    /// Get the cursor rect for the currently focused text input node in ABSOLUTE coordinates.
    ///
    /// This returns the cursor position in absolute window coordinates (not accounting for
    /// scroll offsets). This is used for scroll-into-view calculations where you need to
    /// compare the cursor position with the scrollable container's bounds.
    ///
    /// Returns None if:
    /// - No node is focused
    /// - Focused node has no text cursor
    /// - Focused node has no layout
    /// - Text cache cannot find cursor position
    ///
    /// For IME positioning (viewport-relative coordinates), use
    /// `get_focused_cursor_rect_viewport()`.
    pub fn get_focused_cursor_rect(&self) -> Option<azul_core::geom::LogicalRect> {
        use azul_core::geom::{LogicalPosition, LogicalRect};

        // Get the focused node
        let focused_node = self.focus_manager.focused_node?;

        // Get the text cursor
        let cursor = self.cursor_manager.get_cursor()?;

        // Get the layout tree from cache
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Find the layout node index corresponding to the focused DOM node
        let target_node_id = focused_node.node.into_crate_internal();
        let layout_idx = layout_tree
            .nodes
            .iter()
            .position(|node| node.dom_node_id == target_node_id)?;

        // Get the layout node
        let layout_node = layout_tree.nodes.get(layout_idx)?;

        // Get the text layout result for this node
        let cached_layout = layout_node.inline_layout_result.as_ref()?;
        let inline_layout = &cached_layout.layout;

        // Get the cursor rect in node-relative coordinates
        let mut cursor_rect = inline_layout.get_cursor_rect(cursor)?;

        // Get the calculated layout position from cache (already in logical units)
        let calc_pos = self.layout_cache.calculated_positions.get(&layout_idx)?;

        // Add layout position to cursor rect (both already in logical units)
        cursor_rect.origin.x += calc_pos.x as f32;
        cursor_rect.origin.y += calc_pos.y as f32;

        // Return ABSOLUTE position (no scroll correction)
        Some(cursor_rect)
    }

    /// Get the cursor rect for the currently focused text input node in VIEWPORT coordinates.
    ///
    /// This returns the cursor position accounting for:
    /// 1. Scroll offsets from all scrollable ancestors
    /// 2. GPU transforms (CSS transforms, animations) from all transformed ancestors
    ///
    /// The returned position is viewport-relative (what the user actually sees on screen).
    /// This is used for IME window positioning, where the IME popup needs to appear at the
    /// visible cursor location, not the absolute layout position.
    ///
    /// Returns None if:
    /// - No node is focused
    /// - Focused node has no text cursor
    /// - Focused node has no layout
    /// - Text cache cannot find cursor position
    ///
    /// For scroll-into-view calculations (absolute coordinates), use `get_focused_cursor_rect()`.
    pub fn get_focused_cursor_rect_viewport(&self) -> Option<azul_core::geom::LogicalRect> {
        use azul_core::geom::{LogicalPosition, LogicalRect};

        // Start with absolute position
        let mut cursor_rect = self.get_focused_cursor_rect()?;

        // Get the focused node
        let focused_node = self.focus_manager.focused_node?;

        // Get the layout tree from cache
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Find the layout node index corresponding to the focused DOM node
        let target_node_id = focused_node.node.into_crate_internal();
        let layout_idx = layout_tree
            .nodes
            .iter()
            .position(|node| node.dom_node_id == target_node_id)?;

        // Get the GPU cache for this DOM (if it exists)
        let gpu_cache = self.gpu_state_manager.caches.get(&focused_node.dom);

        // CRITICAL STEP 1: Apply scroll offsets from all scrollable ancestors
        // CRITICAL STEP 2: Apply inverse GPU transforms from all transformed ancestors
        // Walk up the tree and apply both corrections
        let mut current_layout_idx = layout_idx;

        while let Some(parent_idx) = layout_tree.nodes.get(current_layout_idx)?.parent {
            // Get the DOM node ID of the parent (if it's not anonymous)
            if let Some(parent_dom_node_id) = layout_tree.nodes.get(parent_idx)?.dom_node_id {
                // STEP 1: Check if this parent is scrollable and has scroll state
                if let Some(scroll_state) = self
                    .scroll_manager
                    .get_scroll_state(focused_node.dom, parent_dom_node_id)
                {
                    // Subtract scroll offset (scrolling down = positive offset, moves content up)
                    cursor_rect.origin.x -= scroll_state.current_offset.x;
                    cursor_rect.origin.y -= scroll_state.current_offset.y;
                }

                // STEP 2: Check if this parent has a GPU transform applied
                if let Some(cache) = gpu_cache {
                    if let Some(transform) = cache.current_transform_values.get(&parent_dom_node_id)
                    {
                        // Apply the INVERSE transform to get back to viewport coordinates
                        // The transform moves the element, so we need to reverse it for the cursor
                        let inverse = transform.inverse();
                        if let Some(transformed_origin) =
                            inverse.transform_point2d(cursor_rect.origin)
                        {
                            cursor_rect.origin = transformed_origin;
                        }
                        // Note: We don't transform the size, only the position
                    }
                }
            }

            // Move to parent for next iteration
            current_layout_idx = parent_idx;
        }

        Some(cursor_rect)
    }

    /// Find the nearest scrollable ancestor for a given node
    /// Returns (DomId, NodeId) of the scrollable container, or None if no scrollable ancestor
    /// exists
    pub fn find_scrollable_ancestor(
        &self,
        mut node_id: azul_core::dom::DomNodeId,
    ) -> Option<azul_core::dom::DomNodeId> {
        // Get the layout tree
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Convert to internal NodeId
        let mut current_node_id = node_id.node.into_crate_internal();

        // Walk up the tree looking for a scrollable node
        loop {
            // Find layout node index
            let layout_idx = layout_tree
                .nodes
                .iter()
                .position(|node| node.dom_node_id == current_node_id)?;

            let layout_node = layout_tree.nodes.get(layout_idx)?;

            // Check if this node has scrollbar info (meaning it's scrollable)
            if layout_node.scrollbar_info.is_some() {
                // Check if it actually has a scroll state registered
                let check_node_id = current_node_id?;
                if self
                    .scroll_manager
                    .get_scroll_state(node_id.dom, check_node_id)
                    .is_some()
                {
                    // Found a scrollable ancestor
                    return Some(azul_core::dom::DomNodeId {
                        dom: node_id.dom,
                        node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(
                            Some(check_node_id),
                        ),
                    });
                }
            }

            // Move to parent
            let parent_idx = layout_node.parent?;
            let parent_node = layout_tree.nodes.get(parent_idx)?;
            current_node_id = parent_node.dom_node_id;
        }
    }

    /// Scroll selection or cursor into view with distance-based acceleration.
    ///
    /// **Unified Scroll System**: This method handles both cursor (0-size selection)
    /// and full selection scrolling with a single implementation. For drag-to-scroll,
    /// scroll speed increases with distance from container edge.
    ///
    /// ## Algorithm
    /// 1. Get bounds to scroll (cursor rect, selection rect, or mouse position)
    /// 2. Find scrollable ancestor container
    /// 3. Calculate distance from bounds to container edges
    /// 4. Compute scroll delta (instant with padding, or accelerated with zones)
    /// 5. Apply scroll with appropriate animation
    ///
    /// ## Distance-Based Acceleration (ScrollMode::Accelerated)
    /// ```
    /// Distance from edge:  Scroll speed per frame:
    /// 0-20px              Dead zone (no scroll)
    /// 20-50px             Slow (2px/frame)
    /// 50-100px            Medium (4px/frame)
    /// 100-200px           Fast (8px/frame)
    /// 200+px              Very fast (16px/frame)
    /// ```
    ///
    /// ## Returns
    /// `true` if scrolling was applied, `false` if already visible
    pub fn scroll_selection_into_view(
        &mut self,
        scroll_type: SelectionScrollType,
        scroll_mode: ScrollMode,
    ) -> bool {
        // Get bounds to scroll into view
        let bounds = match scroll_type {
            SelectionScrollType::Cursor => {
                // Cursor is 0-size selection at insertion point
                match self.get_focused_cursor_rect() {
                    Some(rect) => rect,
                    None => return false, // No cursor to scroll
                }
            }
            SelectionScrollType::Selection => {
                // Get selection range(s) and compute bounding rect
                // For now, treat as cursor until we implement calculate_selection_bounding_rect
                match self.get_focused_cursor_rect() {
                    Some(rect) => rect,
                    None => return false, // No selection to scroll
                }
                // TODO: Implement calculate_selection_bounding_rect
                // let ranges = self.selection_manager.get_selection();
                // if ranges.is_empty() {
                //     return false;
                // }
                // self.calculate_selection_bounding_rect(ranges)?
            }
            SelectionScrollType::DragSelection { mouse_position } => {
                // For drag: use mouse position to determine scroll direction/speed
                LogicalRect::new(mouse_position, LogicalSize::zero())
            }
        };

        // Get the focused node (or bail if no focus)
        let focused_node = match self.focus_manager.focused_node {
            Some(node) => node,
            None => return false,
        };

        // Find scrollable ancestor
        let scroll_container = match self.find_scrollable_ancestor(focused_node) {
            Some(node) => node,
            None => return false, // No scrollable ancestor
        };

        // Get container bounds and current scroll state
        let layout_tree = match self.layout_cache.tree.as_ref() {
            Some(tree) => tree,
            None => return false,
        };

        let scrollable_node_internal = match scroll_container.node.into_crate_internal() {
            Some(id) => id,
            None => return false,
        };

        let layout_idx = match layout_tree
            .nodes
            .iter()
            .position(|n| n.dom_node_id == Some(scrollable_node_internal))
        {
            Some(idx) => idx,
            None => return false,
        };

        let scrollable_layout_node = match layout_tree.nodes.get(layout_idx) {
            Some(node) => node,
            None => return false,
        };

        let container_pos = self
            .layout_cache
            .calculated_positions
            .get(&layout_idx)
            .copied()
            .unwrap_or_default();

        let container_size = scrollable_layout_node.used_size.unwrap_or_default();

        let container_rect = LogicalRect {
            origin: container_pos,
            size: container_size,
        };

        // Get current scroll state
        let scroll_state = match self
            .scroll_manager
            .get_scroll_state(scroll_container.dom, scrollable_node_internal)
        {
            Some(state) => state,
            None => return false,
        };

        // Calculate visible area (container rect adjusted by scroll offset)
        let visible_area = LogicalRect::new(
            LogicalPosition::new(
                container_rect.origin.x + scroll_state.current_offset.x,
                container_rect.origin.y + scroll_state.current_offset.y,
            ),
            container_rect.size,
        );

        // Calculate scroll delta based on mode
        let scroll_delta = match scroll_mode {
            ScrollMode::Instant => {
                // For typing/clicking: instant scroll with fixed padding
                calculate_instant_scroll_delta(bounds, visible_area)
            }
            ScrollMode::Accelerated => {
                // For drag: accelerated scroll based on distance from edge
                let distance = calculate_edge_distance(bounds, visible_area);
                calculate_accelerated_scroll_delta(distance)
            }
        };

        // Apply scroll if needed
        if scroll_delta.x != 0.0 || scroll_delta.y != 0.0 {
            let duration = match scroll_mode {
                ScrollMode::Instant => Duration::System(SystemTimeDiff { secs: 0, nanos: 0 }),
                ScrollMode::Accelerated => Duration::System(SystemTimeDiff {
                    secs: 0,
                    nanos: 16_666_667,
                }), // 60fps
            };

            let external = ExternalSystemCallbacks::rust_internal();
            let now = (external.get_system_time_fn.cb)();

            // Calculate new scroll target
            let new_target = LogicalPosition {
                x: scroll_state.current_offset.x + scroll_delta.x,
                y: scroll_state.current_offset.y + scroll_delta.y,
            };

            self.scroll_manager.scroll_to(
                scroll_container.dom,
                scrollable_node_internal,
                new_target,
                duration,
                EasingFunction::Linear,
                now.into(),
            );

            true // Scrolled
        } else {
            false // Already visible
        }
    }

    /// Automatically scrolls the focused cursor into view after layout.
    ///
    /// **DEPRECATED**: Use `scroll_selection_into_view(SelectionScrollType::Cursor,
    /// ScrollMode::Instant)` instead. This method is kept for compatibility but redirects to
    /// the unified scroll system.
    ///
    /// This is called after `layout_and_generate_display_list()` to ensure that
    /// text cursors remain visible after text input or cursor movement.
    ///
    /// Algorithm:
    /// 1. Get the focused cursor rect (if any)
    /// 2. Find the scrollable ancestor container
    /// 3. Calculate scroll delta to bring cursor into view
    /// 4. Apply instant scroll (no animation for text input responsiveness)
    fn scroll_focused_cursor_into_view(&mut self) {
        // Redirect to unified scroll system
        self.scroll_selection_into_view(SelectionScrollType::Cursor, ScrollMode::Instant);
    }
}

/// Type of selection bounds to scroll into view
#[derive(Debug, Clone, Copy)]
pub enum SelectionScrollType {
    /// Scroll cursor (0-size selection) into view
    Cursor,
    /// Scroll current selection bounds into view
    Selection,
    /// Scroll for drag selection (use mouse position for direction/speed)
    DragSelection { mouse_position: LogicalPosition },
}

/// Scroll animation mode
#[derive(Debug, Clone, Copy)]
pub enum ScrollMode {
    /// Instant scroll with fixed padding (for typing, arrow keys)
    Instant,
    /// Accelerated scroll based on distance from edge (for drag-to-scroll)
    Accelerated,
}

/// Distance from rect edges to container edges (for acceleration calculation)
#[derive(Debug, Clone, Copy)]
struct EdgeDistance {
    left: f32,
    right: f32,
    top: f32,
    bottom: f32,
}

/// Calculate distance from rect to container edges
fn calculate_edge_distance(rect: LogicalRect, container: LogicalRect) -> EdgeDistance {
    EdgeDistance {
        // Distance from rect's left edge to container's left edge
        left: (rect.origin.x - container.origin.x).max(0.0),
        // Distance from container's right edge to rect's right edge
        right: ((container.origin.x + container.size.width) - (rect.origin.x + rect.size.width))
            .max(0.0),
        // Distance from rect's top edge to container's top edge
        top: (rect.origin.y - container.origin.y).max(0.0),
        // Distance from container's bottom edge to rect's bottom edge
        bottom: ((container.origin.y + container.size.height) - (rect.origin.y + rect.size.height))
            .max(0.0),
    }
}

/// Calculate scroll delta with fixed padding (instant scroll mode)
fn calculate_instant_scroll_delta(
    bounds: LogicalRect,
    visible_area: LogicalRect,
) -> LogicalPosition {
    const PADDING: f32 = 5.0;
    let mut delta = LogicalPosition::zero();

    // Horizontal scrolling
    if bounds.origin.x < visible_area.origin.x + PADDING {
        delta.x = bounds.origin.x - visible_area.origin.x - PADDING;
    } else if bounds.origin.x + bounds.size.width
        > visible_area.origin.x + visible_area.size.width - PADDING
    {
        delta.x = (bounds.origin.x + bounds.size.width)
            - (visible_area.origin.x + visible_area.size.width)
            + PADDING;
    }

    // Vertical scrolling
    if bounds.origin.y < visible_area.origin.y + PADDING {
        delta.y = bounds.origin.y - visible_area.origin.y - PADDING;
    } else if bounds.origin.y + bounds.size.height
        > visible_area.origin.y + visible_area.size.height - PADDING
    {
        delta.y = (bounds.origin.y + bounds.size.height)
            - (visible_area.origin.y + visible_area.size.height)
            + PADDING;
    }

    delta
}

/// Calculate scroll delta with distance-based acceleration (drag-to-scroll mode)
fn calculate_accelerated_scroll_delta(distance: EdgeDistance) -> LogicalPosition {
    // Acceleration zones (in pixels from edge)
    const DEAD_ZONE: f32 = 20.0;
    const SLOW_ZONE: f32 = 50.0;
    const MEDIUM_ZONE: f32 = 100.0;
    const FAST_ZONE: f32 = 200.0;

    // Scroll speeds (pixels per frame at 60fps)
    const SLOW_SPEED: f32 = 2.0;
    const MEDIUM_SPEED: f32 = 4.0;
    const FAST_SPEED: f32 = 8.0;
    const VERY_FAST_SPEED: f32 = 16.0;

    // Helper to calculate speed for one direction
    let speed_for_distance = |dist: f32| -> f32 {
        if dist < DEAD_ZONE {
            0.0
        } else if dist < SLOW_ZONE {
            SLOW_SPEED
        } else if dist < MEDIUM_ZONE {
            MEDIUM_SPEED
        } else if dist < FAST_ZONE {
            FAST_SPEED
        } else {
            VERY_FAST_SPEED
        }
    };

    // Calculate horizontal scroll (left vs right)
    let scroll_x = if distance.left < distance.right {
        // Closer to left edge - scroll left
        -speed_for_distance(distance.left)
    } else {
        // Closer to right edge - scroll right
        speed_for_distance(distance.right)
    };

    // Calculate vertical scroll (top vs bottom)
    let scroll_y = if distance.top < distance.bottom {
        // Closer to top edge - scroll up
        -speed_for_distance(distance.top)
    } else {
        // Closer to bottom edge - scroll down
        speed_for_distance(distance.bottom)
    };

    LogicalPosition::new(scroll_x, scroll_y)
}

/// Result of a layout operation
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

impl LayoutWindow {
    /// Runs a single timer, similar to CallbacksOfHitTest.call()
    ///
    /// NOTE: The timer has to be selected first by the calling code and verified
    /// that it is ready to run
    #[cfg(feature = "std")]
    pub fn run_single_timer(
        &mut self,
        timer_id: usize,
        frame_start: Instant,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use std::collections::BTreeMap;

        use azul_core::{callbacks::Update, task::TerminateTimer, FastBTreeSet, FastHashMap};

        use crate::callbacks::{CallCallbacksResult, CallbackInfo};

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            image_callbacks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            menus_to_open: Vec::new(),
            tooltips_to_show: Vec::new(),
            hide_tooltip: false,
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut should_terminate = TerminateTimer::Continue;

        let current_scroll_states_nested = self.get_nested_scroll_states(DomId::ROOT_ID);

        // Check if timer exists and get node_id before borrowing self mutably
        let timer_exists = self.timers.contains_key(&TimerId { id: timer_id });
        let timer_node_id = self
            .timers
            .get(&TimerId { id: timer_id })
            .and_then(|t| t.node_id.into_option());

        if timer_exists {
            // TODO: store the hit DOM of the timer?
            let hit_dom_node = match timer_node_id {
                Some(s) => s,
                None => DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(None),
                },
            };
            let cursor_relative_to_item = OptionLogicalPosition::None;
            let cursor_in_viewport = OptionLogicalPosition::None;

            // Create changes vector for callback transaction system
            let mut callback_changes = Vec::new();

            // Create reference data container (syntax sugar to reduce parameter count)
            let ref_data = crate::callbacks::CallbackInfoRefData {
                layout_window: self,
                renderer_resources,
                previous_window_state,
                current_window_state,
                gl_context,
                current_scroll_manager: &current_scroll_states_nested,
                current_window_handle,
                system_callbacks,
            };

            let callback_info = CallbackInfo::new(
                &ref_data,
                system_style,
                &mut callback_changes,
                hit_dom_node,
                cursor_relative_to_item,
                cursor_in_viewport,
            ); // Now we can borrow the timer mutably
            let timer = self.timers.get_mut(&TimerId { id: timer_id }).unwrap();
            let tcr = timer.invoke(&callback_info, &system_callbacks.get_system_time_fn);

            ret.callbacks_update_screen = tcr.should_update;
            should_terminate = tcr.should_terminate;

            // Apply callback changes collected during timer execution
            let change_result = self.apply_callback_changes(
                callback_changes,
                current_window_state,
                image_cache,
                system_fonts,
            );

            // Queue IFrame updates for next frame
            if !change_result.iframes_to_update.is_empty() {
                self.queue_iframe_updates(change_result.iframes_to_update.clone());
            }

            // Transfer results from CallbackChangeResult to CallCallbacksResult
            ret.stop_propagation = change_result.stop_propagation;
            ret.prevent_default = change_result.prevent_default;
            ret.tooltips_to_show = change_result.tooltips_to_show;
            ret.hide_tooltip = change_result.hide_tooltip;

            if !change_result.timers.is_empty() {
                ret.timers = Some(change_result.timers);
            }
            if !change_result.threads.is_empty() {
                ret.threads = Some(change_result.threads);
            }
            if change_result.modified_window_state != *current_window_state {
                ret.modified_window_state = Some(change_result.modified_window_state);
            }
            if !change_result.threads_removed.is_empty() {
                ret.threads_removed = Some(change_result.threads_removed);
            }
            if !change_result.timers_removed.is_empty() {
                ret.timers_removed = Some(change_result.timers_removed);
            }
            if !change_result.words_changed.is_empty() {
                ret.words_changed = Some(change_result.words_changed);
            }
            if !change_result.images_changed.is_empty() {
                ret.images_changed = Some(change_result.images_changed);
            }
            if !change_result.image_masks_changed.is_empty() {
                ret.image_masks_changed = Some(change_result.image_masks_changed);
            }
            if !change_result.css_properties_changed.is_empty() {
                ret.css_properties_changed = Some(change_result.css_properties_changed);
            }
            if !change_result.image_callbacks_changed.is_empty() {
                ret.image_callbacks_changed = Some(change_result.image_callbacks_changed);
            }
            if !change_result.nodes_scrolled.is_empty() {
                ret.nodes_scrolled_in_callbacks = Some(change_result.nodes_scrolled);
            }

            // Handle focus target outside the timer block so it's available later
            if let Some(ft) = change_result.focus_target {
                if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                    &ft,
                    &self.layout_results,
                    self.focus_manager.get_focused_node().copied(),
                ) {
                    ret.update_focused_node = match new_focus_node {
                        Some(node) => FocusUpdateRequest::FocusNode(node),
                        None => FocusUpdateRequest::ClearFocus,
                    };
                }
            }
        }

        if should_terminate == TerminateTimer::Terminate {
            ret.timers_removed
                .get_or_insert_with(|| std::collections::BTreeSet::new())
                .insert(TimerId { id: timer_id });
        }

        return ret;
    }

    #[cfg(feature = "std")]
    pub fn run_all_threads(
        &mut self,
        data: &mut RefAny,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use std::collections::BTreeSet;

        use azul_core::{callbacks::Update, refany::RefAny};

        use crate::{
            callbacks::{CallCallbacksResult, CallbackInfo},
            thread::{OptionThreadReceiveMsg, ThreadReceiveMsg, ThreadWriteBackMsg},
        };

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            image_callbacks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            menus_to_open: Vec::new(),
            tooltips_to_show: Vec::new(),
            hide_tooltip: false,
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state = current_window_state.clone();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();
        let mut new_focus_target = None;
        let mut stop_propagation = false;
        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        // Collect thread IDs first to avoid borrowing self.threads while accessing self
        let thread_ids: Vec<ThreadId> = self.threads.keys().copied().collect();

        for thread_id in thread_ids {
            let thread = match self.threads.get_mut(&thread_id) {
                Some(t) => t,
                None => continue,
            };

            let hit_dom_node = DomNodeId {
                dom: DomId::ROOT_ID,
                node: NodeHierarchyItemId::from_crate_internal(None),
            };
            let cursor_relative_to_item = OptionLogicalPosition::None;
            let cursor_in_viewport = OptionLogicalPosition::None;

            // Lock the mutex, extract data, then drop the guard before creating CallbackInfo
            let (msg, writeback_data_ptr, is_finished) = {
                let thread_inner = &mut *match thread.ptr.lock().ok() {
                    Some(s) => s,
                    None => {
                        ret.threads_removed
                            .get_or_insert_with(|| BTreeSet::default())
                            .insert(thread_id);
                        continue;
                    }
                };

                let _ = thread_inner.sender_send(ThreadSendMsg::Tick);
                let update = thread_inner.receiver_try_recv();
                let msg = match update {
                    OptionThreadReceiveMsg::None => continue,
                    OptionThreadReceiveMsg::Some(s) => s,
                };

                let writeback_data_ptr = &mut thread_inner.writeback_data as *mut _;
                let is_finished = thread_inner.is_finished();

                (msg, writeback_data_ptr, is_finished)
                // MutexGuard is dropped here
            };

            let ThreadWriteBackMsg { mut data, callback } = match msg {
                ThreadReceiveMsg::Update(update_screen) => {
                    ret.callbacks_update_screen.max_self(update_screen);
                    continue;
                }
                ThreadReceiveMsg::WriteBack(t) => t,
            };

            // Create changes vector for callback transaction system
            let mut callback_changes = Vec::new();

            // Create reference data container (syntax sugar to reduce parameter count)
            let ref_data = crate::callbacks::CallbackInfoRefData {
                layout_window: self,
                renderer_resources,
                previous_window_state,
                current_window_state,
                gl_context,
                current_scroll_manager: &current_scroll_states,
                current_window_handle,
                system_callbacks,
            };

            let mut callback_info = CallbackInfo::new(
                &ref_data,
                system_style.clone(),
                &mut callback_changes,
                hit_dom_node,
                cursor_relative_to_item,
                cursor_in_viewport,
            );
            let callback_update = (callback.cb)(
                unsafe { &mut *writeback_data_ptr },
                &mut data,
                &mut callback_info,
            );
            ret.callbacks_update_screen.max_self(callback_update);

            // Apply callback changes collected during thread writeback
            let change_result = self.apply_callback_changes(
                callback_changes,
                current_window_state,
                image_cache,
                system_fonts,
            );

            // Queue any IFrame updates from this callback
            self.queue_iframe_updates(change_result.iframes_to_update);

            ret.stop_propagation = ret.stop_propagation || change_result.stop_propagation;
            ret.prevent_default = ret.prevent_default || change_result.prevent_default;
            ret.tooltips_to_show.extend(change_result.tooltips_to_show);
            ret.hide_tooltip = ret.hide_tooltip || change_result.hide_tooltip;

            // Merge changes into accumulated results
            ret_timers.extend(change_result.timers);
            ret_threads.extend(change_result.threads);
            ret_timers_removed.extend(change_result.timers_removed);
            ret_threads_removed.extend(change_result.threads_removed);

            for (dom_id, nodes) in change_result.words_changed {
                ret_words_changed
                    .entry(dom_id)
                    .or_insert_with(BTreeMap::new)
                    .extend(nodes);
            }
            for (dom_id, nodes) in change_result.images_changed {
                ret_images_changed
                    .entry(dom_id)
                    .or_insert_with(BTreeMap::new)
                    .extend(nodes);
            }
            for (dom_id, nodes) in change_result.image_masks_changed {
                ret_image_masks_changed
                    .entry(dom_id)
                    .or_insert_with(BTreeMap::new)
                    .extend(nodes);
            }
            for (dom_id, nodes) in change_result.css_properties_changed {
                ret_css_properties_changed
                    .entry(dom_id)
                    .or_insert_with(BTreeMap::new)
                    .extend(nodes);
            }
            for (dom_id, nodes) in change_result.nodes_scrolled {
                ret_nodes_scrolled_in_callbacks
                    .entry(dom_id)
                    .or_insert_with(BTreeMap::new)
                    .extend(nodes);
            }

            if change_result.modified_window_state != *current_window_state {
                ret_modified_window_state = change_result.modified_window_state;
            }

            if let Some(ft) = change_result.focus_target {
                new_focus_target = Some(ft);
            }

            if is_finished {
                ret.threads_removed
                    .get_or_insert_with(|| BTreeSet::default())
                    .insert(thread_id);
            }
        }

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_window_state {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        return ret;
    }

    /// Invokes a single callback (used for on_window_create, on_window_shutdown, etc.)
    pub fn invoke_single_callback(
        &mut self,
        callback: &mut Callback,
        data: &mut RefAny,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use azul_core::{callbacks::Update, refany::RefAny};

        use crate::callbacks::{CallCallbacksResult, Callback, CallbackInfo};

        let hit_dom_node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(None),
        };

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            image_callbacks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            menus_to_open: Vec::new(),
            tooltips_to_show: Vec::new(),
            hide_tooltip: false,
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state = current_window_state.clone();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();
        let mut new_focus_target = None;
        let mut stop_propagation = false;
        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        let cursor_relative_to_item = OptionLogicalPosition::None;
        let cursor_in_viewport = OptionLogicalPosition::None;

        // Create changes vector for callback transaction system
        let mut callback_changes = Vec::new();

        // Create reference data container (syntax sugar to reduce parameter count)
        let ref_data = crate::callbacks::CallbackInfoRefData {
            layout_window: self,
            renderer_resources,
            previous_window_state,
            current_window_state,
            gl_context,
            current_scroll_manager: &current_scroll_states,
            current_window_handle,
            system_callbacks,
        };

        let mut callback_info = CallbackInfo::new(
            &ref_data,
            system_style,
            &mut callback_changes,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        ret.callbacks_update_screen = (callback.cb)(data, &mut callback_info);

        // Apply callback changes collected during callback execution
        let change_result = self.apply_callback_changes(
            callback_changes,
            current_window_state,
            image_cache,
            system_fonts,
        );

        // Queue any IFrame updates from this callback
        self.queue_iframe_updates(change_result.iframes_to_update);

        ret.stop_propagation = change_result.stop_propagation;
        ret.prevent_default = change_result.prevent_default;
        ret.tooltips_to_show = change_result.tooltips_to_show;
        ret.hide_tooltip = change_result.hide_tooltip;

        ret_timers.extend(change_result.timers);
        ret_threads.extend(change_result.threads);
        ret_timers_removed.extend(change_result.timers_removed);
        ret_threads_removed.extend(change_result.threads_removed);
        ret_words_changed.extend(change_result.words_changed);
        ret_images_changed.extend(change_result.images_changed);
        ret_image_masks_changed.extend(change_result.image_masks_changed);
        ret_css_properties_changed.extend(change_result.css_properties_changed);
        ret_nodes_scrolled_in_callbacks.append(&mut change_result.nodes_scrolled.clone());

        if change_result.modified_window_state != *current_window_state {
            ret_modified_window_state = change_result.modified_window_state;
        }

        new_focus_target = change_result.focus_target.or(new_focus_target);

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_window_state {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        return ret;
    }

    /// Invokes a menu callback
    pub fn invoke_menu_callback(
        &mut self,
        menu_callback: &mut MenuCallback,
        hit_dom_node: DomNodeId,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        image_cache: &mut ImageCache,
        system_fonts: &mut FcFontCache,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> CallCallbacksResult {
        use azul_core::callbacks::Update;

        use crate::callbacks::{CallCallbacksResult, CallbackInfo, MenuCallback};

        let mut ret = CallCallbacksResult {
            should_scroll_render: false,
            callbacks_update_screen: Update::DoNothing,
            modified_window_state: None,
            css_properties_changed: None,
            words_changed: None,
            images_changed: None,
            image_masks_changed: None,
            image_callbacks_changed: None,
            nodes_scrolled_in_callbacks: None,
            update_focused_node: FocusUpdateRequest::NoChange,
            timers: None,
            threads: None,
            timers_removed: None,
            threads_removed: None,
            windows_created: Vec::new(),
            menus_to_open: Vec::new(),
            tooltips_to_show: Vec::new(),
            hide_tooltip: false,
            cursor_changed: false,
            stop_propagation: false,
            prevent_default: false,
        };

        let mut ret_modified_window_state = current_window_state.clone();
        let ret_window_state = ret_modified_window_state.clone();
        let mut ret_timers = FastHashMap::new();
        let mut ret_timers_removed = FastBTreeSet::new();
        let mut ret_threads = FastHashMap::new();
        let mut ret_threads_removed = FastBTreeSet::new();
        let mut ret_words_changed = BTreeMap::new();
        let mut ret_images_changed = BTreeMap::new();
        let mut ret_image_masks_changed = BTreeMap::new();
        let mut ret_css_properties_changed = BTreeMap::new();
        let mut ret_nodes_scrolled_in_callbacks = BTreeMap::new();
        let mut new_focus_target = None;
        let mut stop_propagation = false;
        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        let cursor_relative_to_item = OptionLogicalPosition::None;
        let cursor_in_viewport = OptionLogicalPosition::None;

        // Create changes vector for callback transaction system
        let mut callback_changes = Vec::new();

        // Create reference data container (syntax sugar to reduce parameter count)
        let ref_data = crate::callbacks::CallbackInfoRefData {
            layout_window: self,
            renderer_resources,
            previous_window_state,
            current_window_state,
            gl_context,
            current_scroll_manager: &current_scroll_states,
            current_window_handle,
            system_callbacks,
        };

        let mut callback_info = CallbackInfo::new(
            &ref_data,
            system_style,
            &mut callback_changes,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        ret.callbacks_update_screen =
            (menu_callback.callback.cb)(&mut menu_callback.data, &mut callback_info);

        // Apply callback changes collected during menu callback execution
        let change_result = self.apply_callback_changes(
            callback_changes,
            current_window_state,
            image_cache,
            system_fonts,
        );

        // Queue any IFrame updates from this callback
        self.queue_iframe_updates(change_result.iframes_to_update);

        ret.stop_propagation = change_result.stop_propagation;
        ret.prevent_default = change_result.prevent_default;
        ret.tooltips_to_show = change_result.tooltips_to_show;
        ret.hide_tooltip = change_result.hide_tooltip;

        ret_timers.extend(change_result.timers);
        ret_threads.extend(change_result.threads);
        ret_timers_removed.extend(change_result.timers_removed);
        ret_threads_removed.extend(change_result.threads_removed);
        ret_words_changed.extend(change_result.words_changed);
        ret_images_changed.extend(change_result.images_changed);
        ret_image_masks_changed.extend(change_result.image_masks_changed);
        ret_css_properties_changed.extend(change_result.css_properties_changed);
        ret_nodes_scrolled_in_callbacks.append(&mut change_result.nodes_scrolled.clone());

        if change_result.modified_window_state != *current_window_state {
            ret_modified_window_state = change_result.modified_window_state;
        }

        new_focus_target = change_result.focus_target.or(new_focus_target);

        if !ret_timers.is_empty() {
            ret.timers = Some(ret_timers);
        }
        if !ret_threads.is_empty() {
            ret.threads = Some(ret_threads);
        }
        if ret_modified_window_state != ret_window_state {
            ret.modified_window_state = Some(ret_modified_window_state);
        }
        if !ret_threads_removed.is_empty() {
            ret.threads_removed = Some(ret_threads_removed);
        }
        if !ret_timers_removed.is_empty() {
            ret.timers_removed = Some(ret_timers_removed);
        }
        if !ret_words_changed.is_empty() {
            ret.words_changed = Some(ret_words_changed);
        }
        if !ret_images_changed.is_empty() {
            ret.images_changed = Some(ret_images_changed);
        }
        if !ret_image_masks_changed.is_empty() {
            ret.image_masks_changed = Some(ret_image_masks_changed);
        }
        if !ret_css_properties_changed.is_empty() {
            ret.css_properties_changed = Some(ret_css_properties_changed);
        }
        if !ret_nodes_scrolled_in_callbacks.is_empty() {
            ret.nodes_scrolled_in_callbacks = Some(ret_nodes_scrolled_in_callbacks);
        }

        if let Some(ft) = new_focus_target {
            if let Ok(new_focus_node) = crate::managers::focus_cursor::resolve_focus_target(
                &ft,
                &self.layout_results,
                self.focus_manager.get_focused_node().copied(),
            ) {
                ret.update_focused_node = match new_focus_node {
                    Some(node) => FocusUpdateRequest::FocusNode(node),
                    None => FocusUpdateRequest::ClearFocus,
                };
            }
        }

        return ret;
    }
}

#[cfg(test)]
mod tests {
    use azul_core::{
        dom::DomId,
        gpu::GpuValueCache,
        task::{Instant, ThreadId, TimerId},
    };

    use super::*;
    use crate::{thread::Thread, timer::Timer};

    #[test]
    fn test_timer_add_remove() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let timer_id = TimerId { id: 1 };
        let timer = Timer::default();

        // Add timer
        window.add_timer(timer_id, timer);
        assert!(window.get_timer(&timer_id).is_some());
        assert_eq!(window.get_timer_ids().len(), 1);

        // Remove timer
        let removed = window.remove_timer(&timer_id);
        assert!(removed.is_some());
        assert!(window.get_timer(&timer_id).is_none());
        assert_eq!(window.get_timer_ids().len(), 0);
    }

    #[test]
    fn test_timer_get_mut() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let timer_id = TimerId { id: 1 };
        let timer = Timer::default();

        window.add_timer(timer_id, timer);

        // Get mutable reference
        let timer_mut = window.get_timer_mut(&timer_id);
        assert!(timer_mut.is_some());
    }

    #[test]
    fn test_multiple_timers() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let timer1 = TimerId { id: 1 };
        let timer2 = TimerId { id: 2 };
        let timer3 = TimerId { id: 3 };

        window.add_timer(timer1, Timer::default());
        window.add_timer(timer2, Timer::default());
        window.add_timer(timer3, Timer::default());

        assert_eq!(window.get_timer_ids().len(), 3);

        window.remove_timer(&timer2);
        assert_eq!(window.get_timer_ids().len(), 2);
        assert!(window.get_timer(&timer1).is_some());
        assert!(window.get_timer(&timer2).is_none());
        assert!(window.get_timer(&timer3).is_some());
    }

    // Thread management tests removed - Thread::default() not available
    // and threads require complex setup. Thread management is tested
    // through integration tests instead.

    #[test]
    fn test_gpu_cache_management() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId { inner: 0 };

        // Initially empty
        assert!(window.get_gpu_cache(&dom_id).is_none());

        // Get or create
        let cache = window.get_or_create_gpu_cache(dom_id);
        assert!(cache.transform_keys.is_empty());

        // Now exists
        assert!(window.get_gpu_cache(&dom_id).is_some());

        // Can get mutable reference
        let cache_mut = window.get_gpu_cache_mut(&dom_id);
        assert!(cache_mut.is_some());
    }

    #[test]
    fn test_gpu_cache_multiple_doms() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom1 = DomId { inner: 0 };
        let dom2 = DomId { inner: 1 };

        window.get_or_create_gpu_cache(dom1);
        window.get_or_create_gpu_cache(dom2);

        assert!(window.get_gpu_cache(&dom1).is_some());
        assert!(window.get_gpu_cache(&dom2).is_some());
    }

    #[test]
    fn test_compute_cursor_type_empty_hit_test() {
        use crate::hit_test::FullHitTest;

        let fc_cache = FcFontCache::default();
        let window = LayoutWindow::new(fc_cache).unwrap();

        let empty_hit = FullHitTest::empty(None);
        let cursor_test = window.compute_cursor_type_hit_test(&empty_hit);

        // Empty hit test should result in default cursor
        assert_eq!(
            cursor_test.cursor_icon,
            azul_core::window::MouseCursorType::Default
        );
        assert!(cursor_test.cursor_node.is_none());
    }

    #[test]
    fn test_layout_result_access() {
        let fc_cache = FcFontCache::default();
        let window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId { inner: 0 };

        // Initially no layout results
        assert!(window.get_layout_result(&dom_id).is_none());
        assert_eq!(window.get_dom_ids().len(), 0);
    }

    // ScrollManager and IFrame Integration Tests

    #[test]
    fn test_scroll_manager_initialization() {
        let fc_cache = FcFontCache::default();
        let window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Initially no scroll states
        let scroll_offsets = window.scroll_manager.get_scroll_states_for_dom(dom_id);
        assert!(scroll_offsets.is_empty());

        // No current offset
        let offset = window.scroll_manager.get_current_offset(dom_id, node_id);
        assert_eq!(offset, None);
    }

    #[test]
    fn test_scroll_manager_tick_updates_activity() {
        use azul_core::task::{Duration, Instant, SystemTimeDiff};

        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Create a scroll event
        let scroll_event = crate::managers::scroll_state::ScrollEvent {
            dom_id,
            node_id,
            delta: LogicalPosition::new(10.0, 20.0),
            source: azul_core::events::EventSource::User,
            duration: None,
            easing: EasingFunction::Linear,
        };

        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        let did_scroll = window
            .scroll_manager
            .process_scroll_event(scroll_event, now.clone());

        // process_scroll_event should return true for successful scroll
        assert!(did_scroll);
    }

    #[test]
    fn test_scroll_manager_programmatic_scroll() {
        use azul_core::task::{Duration, Instant, SystemTimeDiff};

        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        // Programmatic scroll with animation
        window.scroll_manager.scroll_to(
            dom_id,
            node_id,
            LogicalPosition::new(100.0, 200.0),
            Duration::System(SystemTimeDiff::from_millis(300)),
            EasingFunction::EaseOut,
            now.clone(),
        );

        let tick_result = window.scroll_manager.tick(now);

        // Programmatic scroll should start animation
        assert!(tick_result.needs_repaint);
    }

    #[test]
    fn test_scroll_manager_iframe_edge_detection() {
        // Note: This test is disabled because the new IFrame architecture
        // moved edge detection logic to IFrameManager. The old ScrollManager
        // API (update_iframe_scroll_info, iframes_to_update) no longer exists.
        // Edge detection is now tested through IFrameManager::check_reinvoke.

        // TODO: Rewrite this test to use the new IFrameManager API once
        // we have a proper test setup for IFrames.
    }

    #[test]
    fn test_scroll_manager_iframe_invocation_tracking() {
        // Note: This test is disabled because IFrame invocation tracking
        // moved to IFrameManager. The ScrollManager no longer tracks
        // which IFrames have been invoked.

        // TODO: Rewrite this test to use IFrameManager::mark_invoked
        // and IFrameManager::check_reinvoke.
    }

    #[test]
    fn test_scrollbar_opacity_fading() {
        // Note: This test is disabled because scrollbar opacity calculation
        // is now done through a helper function in LayoutWindow, not
        // through ScrollManager.get_scrollbar_opacity().

        // The new architecture separates scroll state from opacity calculation.
        // ScrollManager tracks last_activity_time, and LayoutWindow has a
        // calculate_scrollbar_opacity() helper that computes fade based on time.

        // TODO: Rewrite this test to use LayoutWindow::calculate_scrollbar_opacity
        // with ScrollManager::get_last_activity_time.
    }

    #[test]
    fn test_iframe_callback_reason_initial_render() {
        // Note: This test is disabled because the frame lifecycle API
        // (begin_frame, end_frame, had_new_doms) was removed from ScrollManager.

        // IFrame callback reasons are now determined by IFrameManager::check_reinvoke
        // which checks if an IFrame has been invoked before.

        // TODO: Rewrite to test IFrameManager::check_reinvoke with InitialRender.
    }

    #[test]
    fn test_gpu_cache_scrollbar_opacity_keys() {
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Get or create GPU cache
        let gpu_cache = window.get_or_create_gpu_cache(dom_id);

        // Initially no scrollbar opacity keys
        assert!(gpu_cache.scrollbar_v_opacity_keys.is_empty());
        assert!(gpu_cache.scrollbar_h_opacity_keys.is_empty());

        // Add a vertical scrollbar opacity key
        let opacity_key = azul_core::resources::OpacityKey::unique();
        gpu_cache
            .scrollbar_v_opacity_keys
            .insert((dom_id, node_id), opacity_key);
        gpu_cache
            .scrollbar_v_opacity_values
            .insert((dom_id, node_id), 1.0);

        // Verify it was added
        assert_eq!(gpu_cache.scrollbar_v_opacity_keys.len(), 1);
        assert_eq!(
            gpu_cache.scrollbar_v_opacity_values.get(&(dom_id, node_id)),
            Some(&1.0)
        );
    }

    #[test]
    fn test_frame_lifecycle_begin_end() {
        // Note: This test is disabled because begin_frame/end_frame API
        // was removed from ScrollManager. Frame lifecycle is now managed
        // at a higher level.

        // The new ScrollManager focuses purely on scroll state and animations.
        // Frame tracking (had_scroll_activity, had_programmatic_scroll) was
        // removed as it's no longer needed with the new architecture.

        // TODO: If frame lifecycle tracking is needed, it should be
        // implemented at the LayoutWindow level, not in ScrollManager.
    }
}

// --- Cross-Paragraph Cursor Navigation API ---
impl LayoutWindow {
    /// Finds the next text node in the DOM tree after the given node.
    ///
    /// This function performs a depth-first traversal to find the next node
    /// that contains text content and is selectable (user-select != none).
    ///
    /// # Arguments
    /// * `dom_id` - The ID of the DOM containing the current node
    /// * `current_node` - The current node ID to start searching from
    ///
    /// # Returns
    /// * `Some((DomId, NodeId))` - The next text node if found
    /// * `None` - If no next text node exists
    pub fn find_next_text_node(
        &self,
        dom_id: &DomId,
        current_node: NodeId,
    ) -> Option<(DomId, NodeId)> {
        let layout_result = self.get_layout_result(dom_id)?;
        let styled_dom = &layout_result.styled_dom;

        // Start from the next node in document order
        let start_idx = current_node.index() + 1;
        let node_hierarchy = &styled_dom.node_hierarchy;

        for i in start_idx..node_hierarchy.len() {
            let node_id = NodeId::new(i);

            // Check if node has text content
            if self.node_has_text_content(styled_dom, node_id) {
                // Check if text is selectable
                if self.is_text_selectable(styled_dom, node_id) {
                    return Some((*dom_id, node_id));
                }
            }
        }

        None
    }

    /// Finds the previous text node in the DOM tree before the given node.
    ///
    /// This function performs a reverse depth-first traversal to find the previous node
    /// that contains text content and is selectable.
    ///
    /// # Arguments
    /// * `dom_id` - The ID of the DOM containing the current node
    /// * `current_node` - The current node ID to start searching from
    ///
    /// # Returns
    /// * `Some((DomId, NodeId))` - The previous text node if found
    /// * `None` - If no previous text node exists
    pub fn find_prev_text_node(
        &self,
        dom_id: &DomId,
        current_node: NodeId,
    ) -> Option<(DomId, NodeId)> {
        let layout_result = self.get_layout_result(dom_id)?;
        let styled_dom = &layout_result.styled_dom;

        // Start from the previous node in reverse document order
        let current_idx = current_node.index();

        for i in (0..current_idx).rev() {
            let node_id = NodeId::new(i);

            // Check if node has text content
            if self.node_has_text_content(styled_dom, node_id) {
                // Check if text is selectable
                if self.is_text_selectable(styled_dom, node_id) {
                    return Some((*dom_id, node_id));
                }
            }
        }

        None
    }

    /// Checks if a node has text content.
    fn node_has_text_content(&self, styled_dom: &StyledDom, node_id: NodeId) -> bool {
        use azul_core::dom::NodeType;

        // Check if node itself is a text node
        let node_data_container = styled_dom.node_data.as_container();
        let node_type = node_data_container[node_id].get_node_type();
        if matches!(node_type, NodeType::Text(_)) {
            return true;
        }

        // Check if node has text children
        let hierarchy_container = styled_dom.node_hierarchy.as_container();
        let node_item = &hierarchy_container[node_id];

        // Iterate through children
        let mut current_child = node_item.first_child_id(node_id);
        while let Some(child_id) = current_child {
            let child_type = node_data_container[child_id].get_node_type();
            if matches!(child_type, NodeType::Text(_)) {
                return true;
            }

            // Move to next sibling
            current_child = hierarchy_container[child_id].next_sibling_id();
        }

        false
    }

    /// Checks if text in a node is selectable based on CSS user-select property.
    fn is_text_selectable(&self, styled_dom: &StyledDom, node_id: NodeId) -> bool {
        use azul_css::props::style::StyleUserSelect;

        let node_data = &styled_dom.node_data.as_container()[node_id];
        let node_state = &styled_dom.styled_nodes.as_container()[node_id].state;

        styled_dom
            .css_property_cache
            .ptr
            .get_user_select(node_data, &node_id, node_state)
            .and_then(|v| v.get_property())
            .map(|us| *us != StyleUserSelect::None)
            .unwrap_or(true) // Default: text is selectable
    }

    /// Process an accessibility action from an assistive technology.
    ///
    /// This method dispatches actions to the appropriate managers (scroll, focus, etc.)
    /// and returns information about which nodes were affected and how.
    ///
    /// # Arguments
    /// * `dom_id` - The DOM containing the target node
    /// * `node_id` - The target node for the action
    /// * `action` - The accessibility action to perform
    /// * `now` - Current timestamp for animations
    ///
    /// # Returns
    /// A BTreeMap of affected nodes with:
    /// - Key: DomNodeId that was affected
    /// - Value: (Vec<EventFilter> synthetic events to dispatch, bool indicating if node needs
    ///   re-layout)
    ///
    /// Empty map = action was not applicable or nothing changed
    #[cfg(feature = "a11y")]
    pub fn process_accessibility_action(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        action: azul_core::dom::AccessibilityAction,
        now: std::time::Instant,
    ) -> BTreeMap<DomNodeId, (Vec<azul_core::events::EventFilter>, bool)> {
        use azul_core::{
            dom::{AccessibilityAction, AttributeType, DomNodeId, NodeType},
            events::EventFilter,
            geom::LogicalPosition,
            styled_dom::NodeHierarchyItemId,
        };

        use crate::managers::text_input::TextInputSource;

        let mut affected_nodes = BTreeMap::new();

        match action {
            // Focus actions
            AccessibilityAction::Focus => {
                let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: hierarchy_id,
                };
                self.focus_manager.set_focused_node(Some(dom_node_id));

                // Check if node is contenteditable - if so, initialize cursor at end of text
                if let Some(layout_result) = self.layout_results.get(&dom_id) {
                    if let Some(styled_node) = layout_result
                        .styled_dom
                        .node_data
                        .as_ref()
                        .get(node_id.index())
                    {
                        let is_contenteditable =
                            styled_node.attributes.as_ref().iter().any(|attr| {
                                matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
                            });

                        if is_contenteditable {
                            // Initialize cursor at end of text using CursorManager
                            let inline_layout = self.get_node_inline_layout(dom_id, node_id);
                            self.cursor_manager.initialize_cursor_at_end(
                                dom_id,
                                node_id,
                                inline_layout.as_ref(),
                            );

                            // Scroll cursor into view if necessary
                            self.scroll_cursor_into_view_if_needed(dom_id, node_id, now);
                        } else {
                            // Not editable - clear cursor
                            self.cursor_manager.clear();
                        }
                    }
                }

                // Optionally scroll into view
                self.scroll_to_node_if_needed(dom_id, node_id, now);
            }
            AccessibilityAction::Blur => {
                self.focus_manager.clear_focus();
                self.cursor_manager.clear();
            }
            AccessibilityAction::SetSequentialFocusNavigationStartingPoint => {
                let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: hierarchy_id,
                };
                self.focus_manager.set_focused_node(Some(dom_node_id));
                // Clear cursor for focus navigation
                self.cursor_manager.clear();
            }

            // Scroll actions
            AccessibilityAction::ScrollIntoView => {
                self.scroll_to_node_if_needed(dom_id, node_id, now);
            }
            AccessibilityAction::ScrollUp => {
                self.scroll_manager.scroll_by(
                    dom_id,
                    node_id,
                    LogicalPosition { x: 0.0, y: -100.0 },
                    std::time::Duration::from_millis(200).into(),
                    azul_core::events::EasingFunction::EaseOut,
                    now.into(),
                );
            }
            AccessibilityAction::ScrollDown => {
                self.scroll_manager.scroll_by(
                    dom_id,
                    node_id,
                    LogicalPosition { x: 0.0, y: 100.0 },
                    std::time::Duration::from_millis(200).into(),
                    azul_core::events::EasingFunction::EaseOut,
                    now.into(),
                );
            }
            AccessibilityAction::ScrollLeft => {
                self.scroll_manager.scroll_by(
                    dom_id,
                    node_id,
                    LogicalPosition { x: -100.0, y: 0.0 },
                    std::time::Duration::from_millis(200).into(),
                    azul_core::events::EasingFunction::EaseOut,
                    now.into(),
                );
            }
            AccessibilityAction::ScrollRight => {
                self.scroll_manager.scroll_by(
                    dom_id,
                    node_id,
                    LogicalPosition { x: 100.0, y: 0.0 },
                    std::time::Duration::from_millis(200).into(),
                    azul_core::events::EasingFunction::EaseOut,
                    now.into(),
                );
            }
            AccessibilityAction::ScrollUp => {
                // Scroll up by default amount (could use page size for page up)
                if let Some(size) = self.get_node_used_size_a11y(dom_id, node_id) {
                    let scroll_amount = size.height.min(100.0); // Scroll by 100px or page height
                    self.scroll_manager.scroll_by(
                        dom_id,
                        node_id,
                        LogicalPosition {
                            x: 0.0,
                            y: -scroll_amount,
                        },
                        std::time::Duration::from_millis(300).into(),
                        azul_core::events::EasingFunction::EaseInOut,
                        now.into(),
                    );
                }
            }
            AccessibilityAction::ScrollDown => {
                // Scroll down by default amount (could use page size for page down)
                if let Some(size) = self.get_node_used_size_a11y(dom_id, node_id) {
                    let scroll_amount = size.height.min(100.0); // Scroll by 100px or page height
                    self.scroll_manager.scroll_by(
                        dom_id,
                        node_id,
                        LogicalPosition {
                            x: 0.0,
                            y: scroll_amount,
                        },
                        std::time::Duration::from_millis(300).into(),
                        azul_core::events::EasingFunction::EaseInOut,
                        now.into(),
                    );
                }
            }
            AccessibilityAction::SetScrollOffset(pos) => {
                self.scroll_manager.scroll_to(
                    dom_id,
                    node_id,
                    pos,
                    std::time::Duration::from_millis(0).into(),
                    azul_core::events::EasingFunction::Linear,
                    now.into(),
                );
            }
            AccessibilityAction::ScrollToPoint(pos) => {
                self.scroll_manager.scroll_to(
                    dom_id,
                    node_id,
                    pos,
                    std::time::Duration::from_millis(300).into(),
                    azul_core::events::EasingFunction::EaseInOut,
                    now.into(),
                );
            }

            // Actions that should trigger element callbacks if they exist
            // These generate synthetic EventFilters that go through the normal
            // callback system
            AccessibilityAction::Default => {
                // Default action → synthetic Click event
                use azul_core::events::HoverEventFilter;

                let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                let dom_node_id = DomNodeId {
                    dom: dom_id,
                    node: hierarchy_id,
                };

                // Check if node has a Default callback, otherwise fallback to Click
                let event_filter = if let Some(layout_result) = self.layout_results.get(&dom_id) {
                    if let Some(styled_node) = layout_result
                        .styled_dom
                        .node_data
                        .as_ref()
                        .get(node_id.index())
                    {
                        let has_default_callback =
                            styled_node.callbacks.as_ref().iter().any(|cb| {
                                // On::Default converts to HoverEventFilter::MouseUp
                                matches!(cb.event, EventFilter::Hover(HoverEventFilter::MouseUp))
                            });

                        if has_default_callback {
                            EventFilter::Hover(HoverEventFilter::MouseUp)
                        } else {
                            EventFilter::Hover(HoverEventFilter::MouseUp)
                        }
                    } else {
                        EventFilter::Hover(HoverEventFilter::MouseUp)
                    }
                } else {
                    EventFilter::Hover(HoverEventFilter::MouseUp)
                };

                affected_nodes.insert(dom_node_id, (vec![event_filter], false));
            }

            AccessibilityAction::Increment | AccessibilityAction::Decrement => {
                // Increment/Decrement work by:
                // 1. Reading the current value (from "value" attribute or text content)
                // 2. Parsing it as a number
                // 3. Incrementing/decrementing by 1
                // 4. Converting back to string
                // 5. Recording as text input (fires TextInput event)
                //
                // This allows user callbacks to intercept via On::TextInput

                let is_increment = matches!(action, AccessibilityAction::Increment);

                // Get the current value
                let current_value = if let Some(layout_result) = self.layout_results.get(&dom_id) {
                    if let Some(styled_node) = layout_result
                        .styled_dom
                        .node_data
                        .as_ref()
                        .get(node_id.index())
                    {
                        // Try "value" attribute first
                        styled_node
                            .attributes
                            .as_ref()
                            .iter()
                            .find_map(|attr| {
                                if let AttributeType::Value(v) = attr {
                                    Some(v.as_str().to_string())
                                } else {
                                    None
                                }
                            })
                            .or_else(|| {
                                // Fallback to text content
                                if let NodeType::Text(text) = styled_node.get_node_type() {
                                    Some(text.as_str().to_string())
                                } else {
                                    None
                                }
                            })
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Parse as number, increment/decrement, convert back to string
                if let Some(value_str) = current_value {
                    let parsed: Result<f64, _> = value_str.trim().parse();

                    let new_value_str = if let Ok(num) = parsed {
                        // Successfully parsed as number
                        let new_num = if is_increment { num + 1.0 } else { num - 1.0 };
                        // Format with same precision as input if possible
                        if num.fract() == 0.0 {
                            format!("{}", new_num as i64)
                        } else {
                            format!("{}", new_num)
                        }
                    } else {
                        // Not a number - treat as 0 and increment/decrement
                        if is_increment {
                            "1".to_string()
                        } else {
                            "-1".to_string()
                        }
                    };

                    // Record as text input (will fire On::TextInput callbacks)
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: hierarchy_id,
                    };

                    // Get old text for changeset
                    let old_inline_content = self.get_text_before_textinput(dom_id, node_id);
                    let old_text = self.extract_text_from_inline_content(&old_inline_content);

                    // Record the text input
                    self.text_input_manager.record_input(
                        dom_node_id,
                        new_value_str,
                        old_text,
                        TextInputSource::Accessibility,
                    );

                    // Add TextInput event to affected nodes
                    use azul_core::events::FocusEventFilter;
                    affected_nodes.insert(
                        dom_node_id,
                        (vec![EventFilter::Focus(FocusEventFilter::TextInput)], false),
                    );
                }
            }

            AccessibilityAction::Collapse | AccessibilityAction::Expand => {
                // Map to corresponding On:: events
                use azul_core::dom::On;

                let event_type = match action {
                    AccessibilityAction::Collapse => On::Collapse,
                    AccessibilityAction::Expand => On::Expand,
                    _ => unreachable!(),
                };

                // Check if node has a callback for this event type
                if let Some(layout_result) = self.layout_results.get(&dom_id) {
                    if let Some(styled_node) = layout_result
                        .styled_dom
                        .node_data
                        .as_ref()
                        .get(node_id.index())
                    {
                        // Check if any callback matches this event type
                        let has_callback = styled_node
                            .callbacks
                            .as_ref()
                            .iter()
                            .any(|cb| cb.event == event_type.into());

                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                        let dom_node_id = DomNodeId {
                            dom: dom_id,
                            node: hierarchy_id,
                        };

                        if has_callback {
                            // Generate EventFilter for this specific callback
                            affected_nodes.insert(dom_node_id, (vec![event_type.into()], false));
                        } else {
                            // No specific callback - fallback to regular Click
                            use azul_core::events::HoverEventFilter;
                            affected_nodes.insert(
                                dom_node_id,
                                (vec![EventFilter::Hover(HoverEventFilter::MouseUp)], false),
                            );
                        }
                    }
                }
            }

            // Context menu - check if node has a menu and trigger right-click event
            AccessibilityAction::ShowContextMenu => {
                // Check if the node has a context menu attached
                let layout_result = match self.layout_results.get(&dom_id) {
                    Some(lr) => lr,
                    None => {
                        return affected_nodes;
                    }
                };

                // Get the node from the styled DOM
                let styled_node = match layout_result
                    .styled_dom
                    .node_data
                    .as_ref()
                    .get(node_id.index())
                {
                    Some(node) => node,
                    None => {
                        return affected_nodes;
                    }
                };

                // Check if node has context menu
                let has_context_menu = styled_node.get_context_menu().is_some();

                if has_context_menu {
                    // TODO: Generate synthetic right-click event to trigger context menu
                    // This requires access to the event system which is not available here
                } else {
                    // No context menu attached to node - silently ignore
                }
            }

            // Text editing actions - use text3/edit.rs
            AccessibilityAction::ReplaceSelectedText(ref text) => {
                let nodes = self.edit_text_node(
                    dom_id,
                    node_id,
                    TextEditType::ReplaceSelection(text.as_str().to_string()),
                );
                for node in nodes {
                    affected_nodes.insert(node, (Vec::new(), true)); // true = needs re-layout
                }
            }
            AccessibilityAction::SetValue(ref text) => {
                let nodes = self.edit_text_node(
                    dom_id,
                    node_id,
                    TextEditType::SetValue(text.as_str().to_string()),
                );
                for node in nodes {
                    affected_nodes.insert(node, (Vec::new(), true));
                }
            }
            AccessibilityAction::SetNumericValue(value) => {
                let nodes = self.edit_text_node(
                    dom_id,
                    node_id,
                    TextEditType::SetNumericValue(value.get() as f64),
                );
                for node in nodes {
                    affected_nodes.insert(node, (Vec::new(), true));
                }
            }
            AccessibilityAction::SetTextSelection(selection) => {
                // Get the text layout for this node from the layout tree
                let text_layout = self.get_node_inline_layout(dom_id, node_id);

                if let Some(inline_layout) = text_layout {
                    // Convert byte offsets to TextCursor positions
                    let start_cursor =
                        self.byte_offset_to_cursor(inline_layout.as_ref(), selection.start as u32);
                    let end_cursor =
                        self.byte_offset_to_cursor(inline_layout.as_ref(), selection.end as u32);

                    if let (Some(start), Some(end)) = (start_cursor, end_cursor) {
                        use azul_core::{
                            selection::{Selection, SelectionRange, SelectionState},
                            styled_dom::NodeHierarchyItemId,
                        };

                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                        let dom_node_id = DomNodeId {
                            dom: dom_id,
                            node: hierarchy_id,
                        };

                        if start == end {
                            // Same position - just set cursor
                            self.cursor_manager.move_cursor_to(start, dom_id, node_id);

                            // Clear any existing selections
                            self.selection_manager.clear_selection(&dom_id);
                        } else {
                            // Different positions - create selection range
                            let selection = Selection::Range(SelectionRange { start, end });

                            let selection_state = SelectionState {
                                selections: vec![selection],
                                node_id: dom_node_id,
                            };

                            // Set selection in SelectionManager
                            self.selection_manager
                                .set_selection(dom_id, selection_state);

                            // Also set cursor to start of selection
                            self.cursor_manager.move_cursor_to(start, dom_id, node_id);
                        }
                    } else {
                        // Could not convert byte offsets to cursors - silently ignore
                    }
                } else {
                    // No text layout available for node - silently ignore
                }
            }

            // Tooltip actions
            AccessibilityAction::ShowTooltip | AccessibilityAction::HideTooltip => {
                // TODO: Integrate with tooltip manager when implemented
            }

            AccessibilityAction::CustomAction(_id) => {
                // TODO: Allow custom action handlers
            }
        }

        // Sync cursor to selection manager for rendering
        self.sync_cursor_to_selection_manager();

        affected_nodes
    }

    /// Process text input from keyboard using cursor/selection/focus managers.
    ///
    /// This is the new unified text input handling. The framework manages text editing
    /// internally using managers, then fires callbacks (On::TextInput, On::Changed)
    /// after the internal state is already updated.
    ///
    /// ## Workflow
    /// 1. Check if focus manager has a focused contenteditable node
    /// 2. Get cursor/selection from managers
    /// 3. Call edit_text_node to apply the edit and update cache
    /// 4. Collect affected nodes that need dirty marking
    /// 5. Return map for re-layout triggering
    ///
    /// ## Parameters
    /// * `text_input` - The text that was typed (can be multiple chars for IME)
    ///
    /// ## Returns
    /// BTreeMap of affected nodes with:
    /// - Key: DomNodeId that was affected
    /// - Value: (Vec<EventFilter> synthetic events, bool needs_relayout)
    /// - Empty map = no focused contenteditable node
    pub fn record_text_input(
        &mut self,
        text_input: &str,
    ) -> BTreeMap<azul_core::dom::DomNodeId, (Vec<azul_core::events::EventFilter>, bool)> {
        use std::collections::BTreeMap;

        use azul_core::events::{EventFilter, FocusEventFilter};

        use crate::managers::text_input::TextInputSource;

        let mut affected_nodes = BTreeMap::new();

        if text_input.is_empty() {
            return affected_nodes;
        }

        // Get focused node
        let focused_node = match self.focus_manager.get_focused_node().copied() {
            Some(node) => node,
            None => return affected_nodes, // No focused node
        };

        let node_id = match focused_node.node.into_crate_internal() {
            Some(id) => id,
            None => return affected_nodes,
        };

        // Get the OLD text before any changes
        let old_inline_content = self.get_text_before_textinput(focused_node.dom, node_id);
        let old_text = self.extract_text_from_inline_content(&old_inline_content);

        // Record the changeset in TextInputManager (but DON'T apply changes yet)
        self.text_input_manager.record_input(
            focused_node,
            text_input.to_string(),
            old_text,
            TextInputSource::Keyboard, // Assuming keyboard for now
        );

        // Return affected nodes with TextInput event so callbacks can be invoked
        let text_input_event = vec![EventFilter::Focus(FocusEventFilter::TextInput)];

        affected_nodes.insert(focused_node, (text_input_event, false)); // false = no re-layout yet

        affected_nodes
    }

    /// Apply the recorded text changeset to the text cache
    ///
    /// This is called AFTER user callbacks, if preventDefault was not set.
    /// This is where we actually compute the new text and update the cache.
    ///
    /// Also updates the cursor position to reflect the edit.
    ///
    /// Returns the nodes that need to be marked dirty for re-layout.
    pub fn apply_text_changeset(&mut self) -> Vec<azul_core::dom::DomNodeId> {
        // Get the changeset from TextInputManager
        let changeset = match self.text_input_manager.get_pending_changeset() {
            Some(cs) => cs.clone(),
            None => return Vec::new(), // No changeset to apply
        };

        let node_id = match changeset.node.node.into_crate_internal() {
            Some(id) => id,
            None => {
                self.text_input_manager.clear_changeset();
                return Vec::new();
            }
        };

        let dom_id = changeset.node.dom;

        // Check if node is contenteditable
        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => {
                self.text_input_manager.clear_changeset();
                return Vec::new();
            }
        };

        let styled_node = match layout_result
            .styled_dom
            .node_data
            .as_ref()
            .get(node_id.index())
        {
            Some(node) => node,
            None => {
                self.text_input_manager.clear_changeset();
                return Vec::new();
            }
        };

        let is_contenteditable = styled_node
            .attributes
            .as_ref()
            .iter()
            .any(|attr| matches!(attr, azul_core::dom::AttributeType::ContentEditable(_)));

        if !is_contenteditable {
            self.text_input_manager.clear_changeset();
            return Vec::new();
        }

        // Get the current inline content from cache
        let content = self.get_text_before_textinput(dom_id, node_id);

        // Get current cursor/selection from cursor manager
        let current_selection = if let Some(cursor) = self.cursor_manager.get_cursor() {
            vec![azul_core::selection::Selection::Cursor(cursor.clone())]
        } else {
            // No cursor - create one at start of text
            use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};
            vec![azul_core::selection::Selection::Cursor(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Leading,
            })]
        };

        // Capture pre-state for undo/redo BEFORE mutation
        let old_text = self.extract_text_from_inline_content(&content);
        let old_cursor = current_selection.first().and_then(|sel| {
            if let azul_core::selection::Selection::Cursor(c) = sel {
                Some(c.clone())
            } else {
                None
            }
        });
        let old_selection_range = current_selection.first().and_then(|sel| {
            if let azul_core::selection::Selection::Range(r) = sel {
                Some(*r)
            } else {
                None
            }
        });

        let pre_state = crate::managers::undo_redo::NodeStateSnapshot {
            node_id: azul_core::id::NodeId::new(node_id.index()),
            text_content: old_text,
            cursor_position: old_cursor,
            selection_range: old_selection_range,
            timestamp: azul_core::task::Instant::System(std::time::Instant::now().into()),
        };

        // Apply the edit using text3::edit - this is a pure function
        use crate::text3::edit::{edit_text, TextEdit};
        let text_edit = TextEdit::Insert(changeset.inserted_text.clone());
        let (new_content, new_selections) = edit_text(&content, &current_selection, &text_edit);

        // Update the cursor/selection in cursor manager
        // This happens lazily, only when we actually apply the changes
        if let Some(azul_core::selection::Selection::Cursor(new_cursor)) = new_selections.first() {
            self.cursor_manager
                .move_cursor_to(new_cursor.clone(), dom_id, node_id);
        }

        // Update the text cache with the new inline content
        self.update_text_cache_after_edit(dom_id, node_id, new_content);

        // Record this operation to the undo/redo manager AFTER successful mutation
        use azul_core::window::CursorPosition;

        use crate::managers::changeset::{TextChangeset, TextOperation};

        // Get the new cursor position after edit
        let new_cursor = new_selections
            .first()
            .and_then(|sel| {
                if let azul_core::selection::Selection::Cursor(c) = sel {
                    // Convert TextCursor to CursorPosition
                    // For now, we use InWindow with approximate coordinates
                    // TODO: Calculate proper screen coordinates from TextCursor
                    Some(CursorPosition::InWindow(
                        azul_core::geom::LogicalPosition::new(0.0, 0.0),
                    ))
                } else {
                    None
                }
            })
            .unwrap_or(CursorPosition::Uninitialized);

        let old_cursor_pos = old_cursor
            .as_ref()
            .map(|_| {
                // Convert TextCursor to CursorPosition
                CursorPosition::InWindow(azul_core::geom::LogicalPosition::new(0.0, 0.0))
            })
            .unwrap_or(CursorPosition::Uninitialized);

        // Generate a unique changeset ID
        static CHANGESET_COUNTER: std::sync::atomic::AtomicUsize =
            std::sync::atomic::AtomicUsize::new(0);
        let changeset_id = CHANGESET_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

        let undo_changeset = TextChangeset {
            id: changeset_id,
            target: changeset.node,
            operation: TextOperation::InsertText {
                text: changeset.inserted_text.clone(),
                position: old_cursor_pos,
                new_cursor,
            },
            timestamp: azul_core::task::Instant::System(std::time::Instant::now().into()),
        };
        self.undo_redo_manager
            .record_operation(undo_changeset, pre_state);

        // Clear the changeset now that it's been applied
        self.text_input_manager.clear_changeset();

        // Return nodes that need dirty marking
        self.determine_dirty_text_nodes(dom_id, node_id)
    }

    /// Determine which nodes need to be marked dirty after a text edit
    ///
    /// Returns the edited node + its parent (if it exists)
    fn determine_dirty_text_nodes(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Vec<azul_core::dom::DomNodeId> {
        use azul_core::styled_dom::NodeHierarchyItemId;

        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return Vec::new(),
        };

        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        let node_dom_id = azul_core::dom::DomNodeId {
            dom: dom_id,
            node: hierarchy_id,
        };

        // Get parent node ID
        let parent_id = layout_result
            .styled_dom
            .node_hierarchy
            .as_container()
            .get(node_id)
            .and_then(|item| item.parent_id())
            .map(|parent_node_id| {
                let parent_hierarchy_id =
                    NodeHierarchyItemId::from_crate_internal(Some(parent_node_id));
                azul_core::dom::DomNodeId {
                    dom: dom_id,
                    node: parent_hierarchy_id,
                }
            });

        // Return node + parent (if exists)
        if let Some(parent) = parent_id {
            vec![node_dom_id, parent]
        } else {
            vec![node_dom_id]
        }
    }

    /// Legacy name for backward compatibility
    #[inline]
    pub fn process_text_input(
        &mut self,
        text_input: &str,
    ) -> BTreeMap<azul_core::dom::DomNodeId, (Vec<azul_core::events::EventFilter>, bool)> {
        self.record_text_input(text_input)
    }

    /// Get the last text changeset (what was changed in the last text input)
    pub fn get_last_text_changeset(&self) -> Option<&TextChangeset> {
        self.text_input_manager.get_pending_changeset()
    }

    /// Get the current inline content (text before text input is applied)
    ///
    /// This is a query function that retrieves the current text state from the node.
    /// Returns InlineContent vector if the node has text.
    ///
    /// # Implementation Note
    /// This function currently reconstructs InlineContent from the styled DOM.
    /// A future optimization would be to cache the InlineContent during layout
    /// and retrieve it directly from the text cache.
    pub fn get_text_before_textinput(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Vec<crate::text3::cache::InlineContent> {
        use azul_core::dom::NodeType;

        use crate::text3::cache::{InlineContent, StyledRun};

        // Get the layout result for this DOM
        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return Vec::new(),
        };

        // Get the node data
        let node_data = match layout_result
            .styled_dom
            .node_data
            .as_ref()
            .get(node_id.index())
        {
            Some(nd) => nd,
            None => return Vec::new(),
        };

        // Extract text content from the node
        match node_data.get_node_type() {
            NodeType::Text(text) => {
                // Simple text node - create a single StyledRun
                let style = self.get_text_style_for_node(dom_id, node_id);

                vec![InlineContent::Text(StyledRun {
                    text: text.as_str().to_string(),
                    style,
                    logical_start_byte: 0,
                })]
            }
            NodeType::Div | NodeType::Body | NodeType::IFrame(_) => {
                // Container nodes - recursively collect text from children
                self.collect_text_from_children(dom_id, node_id)
            }
            _ => {
                // Other node types (Image, etc.) don't contribute text
                Vec::new()
            }
        }
    }

    /// Get the font style for a text node from CSS
    fn get_text_style_for_node(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> alloc::sync::Arc<crate::text3::cache::StyleProperties> {
        use alloc::sync::Arc;

        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return Arc::new(Default::default()),
        };

        // Try to get font from styled DOM
        let styled_nodes = layout_result.styled_dom.styled_nodes.as_ref();
        let _styled_node = match styled_nodes.get(node_id.index()) {
            Some(sn) => sn,
            None => return Arc::new(Default::default()),
        };

        // Extract font properties from computed style
        // For now, use default - full implementation would query CSS property cache
        // TODO: Query CSS property cache for font-family, font-size, font-weight, etc.
        Arc::new(Default::default())
    }

    /// Recursively collect text content from child nodes
    fn collect_text_from_children(
        &self,
        dom_id: DomId,
        parent_node_id: NodeId,
    ) -> Vec<crate::text3::cache::InlineContent> {
        use crate::text3::cache::InlineContent;

        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => return Vec::new(),
        };

        let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_ref();
        let parent_item = match node_hierarchy.get(parent_node_id.index()) {
            Some(item) => item,
            None => return Vec::new(),
        };

        let mut result = Vec::new();

        // Traverse all children
        let mut current_child = parent_item.first_child_id(parent_node_id);
        while let Some(child_id) = current_child {
            // Get content from this child (recursive)
            let child_content = self.get_text_before_textinput(dom_id, child_id);
            result.extend(child_content);

            // Move to next sibling
            let child_item = match node_hierarchy.get(child_id.index()) {
                Some(item) => item,
                None => break,
            };
            current_child = child_item.next_sibling_id();
        }

        result
    }

    /// Extract plain text string from inline content
    ///
    /// This is a helper for building the changeset's resulting_text field.
    pub fn extract_text_from_inline_content(
        &self,
        content: &[crate::text3::cache::InlineContent],
    ) -> String {
        use crate::text3::cache::InlineContent;

        let mut result = String::new();

        for item in content {
            match item {
                InlineContent::Text(text_run) => {
                    result.push_str(&text_run.text);
                }
                InlineContent::Space(_) => {
                    result.push(' ');
                }
                InlineContent::LineBreak(_) => {
                    result.push('\n');
                }
                InlineContent::Tab => {
                    result.push('\t');
                }
                InlineContent::Ruby { base, .. } => {
                    // For Ruby annotations, include the base text
                    result.push_str(&self.extract_text_from_inline_content(base));
                }
                InlineContent::Marker { run, .. } => {
                    // Markers contribute their text
                    result.push_str(&run.text);
                }
                // Images and shapes don't contribute to plain text
                InlineContent::Image(_) | InlineContent::Shape(_) => {}
            }
        }

        result
    }

    /// Update the text cache after a text edit
    ///
    /// This is the ONLY place where we mutate the text cache.
    /// All other functions are pure queries or transformations.
    pub fn update_text_cache_after_edit(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        new_inline_content: Vec<crate::text3::cache::InlineContent>,
    ) {
        // TODO: Update the text cache with the new inline content
        //
        // Future implementation should:
        // 1. Get or create the text cache entry for this node
        // 2. Clear the existing cache stages (logical, visual, shaped, layout)
        // 3. Store the new InlineContent
        // 4. The next layout pass will re-run the 4-stage pipeline

        let _ = (dom_id, node_id, new_inline_content);
    }

    /// Helper to get node used_size for accessibility actions
    #[cfg(feature = "a11y")]
    fn get_node_used_size_a11y(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<azul_core::geom::LogicalSize> {
        let layout_result = self.layout_results.get(&dom_id)?;
        let node = layout_result.layout_tree.get(node_id.index())?;
        node.used_size
    }

    /// Get the layout bounds (position and size) of a specific node
    fn get_node_bounds(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<azul_css::props::basic::LayoutRect> {
        use azul_css::props::basic::LayoutRect;

        let layout_result = self.layout_results.get(&dom_id)?;
        let node = layout_result.layout_tree.get(node_id.index())?;

        // Get size from used_size
        let size = node.used_size?;

        // Get position from calculated_positions map
        let position = layout_result.calculated_positions.get(&node_id.index())?;

        Some(LayoutRect {
            origin: azul_css::props::basic::LayoutPoint {
                x: position.x as f32 as isize,
                y: position.y as f32 as isize,
            },
            size: azul_css::props::basic::LayoutSize {
                width: size.width as isize,
                height: size.height as isize,
            },
        })
    }

    /// Scroll a node into view if it's not currently visible in the viewport
    #[cfg(feature = "a11y")]
    fn scroll_to_node_if_needed(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        now: std::time::Instant,
    ) {
        use azul_core::geom::LogicalPosition;

        // TODO: This should:
        // 1. Check if node is currently visible in viewport
        // 2. Find the nearest scrollable ancestor
        // 3. Calculate the scroll offset needed to make the node visible
        // 4. Animate the scroll

        // For now, just ensure the node's scroll state is at origin
        if self.get_node_bounds(dom_id, node_id).is_some() {
            self.scroll_manager.scroll_to(
                dom_id,
                node_id,
                LogicalPosition { x: 0.0, y: 0.0 },
                std::time::Duration::from_millis(300).into(),
                azul_core::events::EasingFunction::EaseOut,
                now.into(),
            );
        }
    }

    /// Scroll the cursor into view if it's not currently visible
    ///
    /// This is automatically called when:
    /// - Focus lands on a contenteditable element
    /// - Cursor is moved programmatically
    /// - Text is inserted/deleted
    ///
    /// The function:
    /// 1. Gets the cursor rectangle from the text layout
    /// 2. Checks if the cursor is visible in the current viewport
    /// 3. If not, calculates the minimum scroll offset needed
    /// 4. Animates the scroll to bring the cursor into view
    fn scroll_cursor_into_view_if_needed(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        now: std::time::Instant,
    ) {
        // Get the cursor from CursorManager
        let Some(cursor) = self.cursor_manager.get_cursor() else {
            return;
        };

        // Get the inline layout for this node
        let Some(inline_layout) = self.get_node_inline_layout(dom_id, node_id) else {
            return;
        };

        // Get the cursor rectangle from the text layout
        let Some(cursor_rect) = inline_layout.get_cursor_rect(cursor) else {
            return;
        };

        // Get the node bounds
        let Some(node_bounds) = self.get_node_bounds(dom_id, node_id) else {
            return;
        };

        // Calculate the cursor's absolute position
        let cursor_abs_x = node_bounds.origin.x as f32 + cursor_rect.origin.x;
        let cursor_abs_y = node_bounds.origin.y as f32 + cursor_rect.origin.y;

        // Find the nearest scrollable ancestor
        // For now, just use the node itself if it's scrollable
        // TODO: Walk up the DOM tree to find scrollable ancestor

        // Get current scroll position
        let current_scroll = self
            .scroll_manager
            .get_current_offset(dom_id, node_id)
            .unwrap_or_default();

        // Calculate visible viewport
        let viewport_x = node_bounds.origin.x as f32 + current_scroll.x;
        let viewport_y = node_bounds.origin.y as f32 + current_scroll.y;
        let viewport_width = node_bounds.size.width as f32;
        let viewport_height = node_bounds.size.height as f32;

        // Check if cursor is visible
        let cursor_visible_x = (cursor_abs_x as f32) >= viewport_x
            && (cursor_abs_x as f32) <= viewport_x + viewport_width;
        let cursor_visible_y = (cursor_abs_y as f32) >= viewport_y
            && (cursor_abs_y as f32) <= viewport_y + viewport_height;

        if cursor_visible_x && cursor_visible_y {
            // Cursor is already visible
            return;
        }

        // Calculate scroll offset to make cursor visible
        let mut target_scroll_x = current_scroll.x;
        let mut target_scroll_y = current_scroll.y;

        // Adjust horizontal scroll if needed
        if (cursor_abs_x as f32) < viewport_x {
            // Cursor is to the left of viewport - scroll left
            target_scroll_x = cursor_abs_x as f32 - node_bounds.origin.x as f32;
        } else if (cursor_abs_x as f32) > viewport_x + viewport_width {
            // Cursor is to the right of viewport - scroll right
            target_scroll_x = cursor_abs_x as f32 - node_bounds.origin.x as f32 - viewport_width
                + cursor_rect.size.width;
        }

        // Adjust vertical scroll if needed
        if (cursor_abs_y as f32) < viewport_y {
            // Cursor is above viewport - scroll up
            target_scroll_y = cursor_abs_y as f32 - node_bounds.origin.y as f32;
        } else if (cursor_abs_y as f32) > viewport_y + viewport_height {
            // Cursor is below viewport - scroll down
            target_scroll_y = cursor_abs_y as f32 - node_bounds.origin.y as f32 - viewport_height
                + cursor_rect.size.height;
        }

        // Animate scroll to bring cursor into view
        use azul_core::geom::LogicalPosition;
        self.scroll_manager.scroll_to(
            dom_id,
            node_id,
            LogicalPosition {
                x: target_scroll_x,
                y: target_scroll_y,
            },
            std::time::Duration::from_millis(200).into(),
            azul_core::events::EasingFunction::EaseOut,
            now.into(),
        );
    }

    /// Convert a byte offset in the text to a TextCursor position
    ///
    /// This is used for accessibility SetTextSelection action, which provides
    /// byte offsets rather than grapheme cluster IDs.
    ///
    /// # Arguments
    ///
    /// * `text_layout` - The text layout containing the shaped runs
    /// * `byte_offset` - The byte offset in the UTF-8 text
    ///
    /// # Returns
    ///
    /// A TextCursor positioned at the given byte offset, or None if the offset
    /// is out of bounds.
    fn byte_offset_to_cursor(
        &self,
        text_layout: &crate::text3::cache::UnifiedLayout,
        byte_offset: u32,
    ) -> Option<azul_core::selection::TextCursor> {
        use azul_core::selection::{CursorAffinity, GraphemeClusterId, TextCursor};

        // Handle offset 0 as special case (start of text)
        if byte_offset == 0 {
            // Find first cluster in items
            for item in &text_layout.items {
                if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                    return Some(TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Trailing,
                    });
                }
            }
            // No clusters found - return default
            return Some(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Trailing,
            });
        }

        // Iterate through items to find which cluster contains this byte offset
        let mut current_byte_offset = 0u32;

        for item in &text_layout.items {
            if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                // Calculate byte length of this cluster from its text
                let cluster_byte_length = cluster.text.len() as u32;
                let cluster_end_byte = current_byte_offset + cluster_byte_length;

                // Check if our target byte offset falls within this cluster
                if byte_offset >= current_byte_offset && byte_offset <= cluster_end_byte {
                    // Found the cluster
                    return Some(TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Trailing,
                    });
                }

                current_byte_offset = cluster_end_byte;
            }
        }

        // Offset is beyond the end of all text - return cursor at end of last cluster
        for item in text_layout.items.iter().rev() {
            if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                });
            }
        }

        // No clusters at all - return default position
        Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Trailing,
        })
    }

    /// Get the inline layout result for a specific node
    ///
    /// This looks up the node in the layout tree and returns its inline layout result
    /// if it exists.
    fn get_node_inline_layout(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<alloc::sync::Arc<crate::text3::cache::UnifiedLayout>> {
        // Get the layout tree from cache
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Find the layout node corresponding to the DOM node
        let layout_node = layout_tree
            .nodes
            .iter()
            .find(|node| node.dom_node_id == Some(node_id))?;

        // Return the inline layout result
        layout_node.inline_layout_result.as_ref().map(|c| c.clone_layout())
    }

    /// Sync cursor from CursorManager to SelectionManager for rendering
    ///
    /// The renderer expects cursor and selection data from the SelectionManager,
    /// but we manage the cursor separately in the CursorManager for better separation
    /// of concerns. This function syncs the cursor state so it can be rendered.
    ///
    /// This should be called whenever the cursor changes.
    pub fn sync_cursor_to_selection_manager(&mut self) {
        use azul_core::{
            selection::{Selection, SelectionState},
            styled_dom::NodeHierarchyItemId,
        };

        if let Some(cursor) = self.cursor_manager.get_cursor() {
            if let Some(location) = self.cursor_manager.get_cursor_location() {
                // Convert cursor to Selection
                let selection = Selection::Cursor(cursor.clone());

                // Create SelectionState
                let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(location.node_id));
                let dom_node_id = DomNodeId {
                    dom: location.dom_id,
                    node: hierarchy_id,
                };

                let selection_state = SelectionState {
                    selections: vec![selection],
                    node_id: dom_node_id,
                };

                // Set selection in SelectionManager
                self.selection_manager
                    .set_selection(location.dom_id, selection_state);
            }
        } else {
            // No cursor - clear selections
            // Note: We might want to be more careful here to not clear user selections
            // For now, clearing when cursor is cleared is safe
            self.selection_manager.clear_all();
        }
    }

    /// Edit the text content of a node (used for text input actions)
    ///
    /// This function applies text edits to nodes that contain text content.
    /// The DOM node itself is NOT modified - instead, the text cache is updated
    /// with the new shaped text that reflects the edit, cursor, and selection.
    ///
    /// It handles:
    /// - ReplaceSelectedText: Replaces the current selection with new text
    /// - SetValue: Sets the entire text value
    /// - SetNumericValue: Converts number to string and sets value
    ///
    /// # Returns
    ///
    /// Returns a Vec of DomNodeIds (node + parent) that need to be marked dirty
    /// for re-layout. The caller MUST use this return value to trigger layout.
    #[must_use = "Returned nodes must be marked dirty for re-layout"]
    #[cfg(feature = "a11y")]
    pub fn edit_text_node(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        edit_type: TextEditType,
    ) -> Vec<azul_core::dom::DomNodeId> {
        use azul_core::styled_dom::NodeHierarchyItemId;

        use crate::managers::text_input::TextInputSource;

        // Convert TextEditType to string
        let text_input = match &edit_type {
            TextEditType::ReplaceSelection(text) => text.clone(),
            TextEditType::SetValue(text) => text.clone(),
            TextEditType::SetNumericValue(value) => value.to_string(),
        };

        // Get the OLD text before any changes
        let old_inline_content = self.get_text_before_textinput(dom_id, node_id);
        let old_text = self.extract_text_from_inline_content(&old_inline_content);

        // Create DomNodeId
        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        let dom_node_id = azul_core::dom::DomNodeId {
            dom: dom_id,
            node: hierarchy_id,
        };

        // Record the changeset in TextInputManager
        self.text_input_manager.record_input(
            dom_node_id,
            text_input,
            old_text,
            TextInputSource::Accessibility, // A11y source
        );

        // Immediately apply the changeset (A11y doesn't go through callbacks)
        self.apply_text_changeset()
    }

    #[cfg(not(feature = "a11y"))]
    pub fn process_accessibility_action(
        &mut self,
        _dom_id: DomId,
        _node_id: NodeId,
        _action: azul_core::dom::AccessibilityAction,
        _now: std::time::Instant,
    ) -> BTreeMap<DomNodeId, (Vec<azul_core::events::EventFilter>, bool)> {
        // No-op when accessibility is disabled
        BTreeMap::new()
    }

    /// Process mouse click for text selection.
    ///
    /// This method handles:
    /// - Single click: Place cursor at click position
    /// - Double click: Select word at click position
    /// - Triple click: Select paragraph (line) at click position
    ///
    /// ## Workflow
    /// 1. Update ClickState to determine click count
    /// 2. Hit-test the position to find the text node
    /// 3. Apply appropriate selection based on click count
    /// 4. Update SelectionManager with new selection
    /// 5. Return affected nodes for dirty tracking
    ///
    /// ## Parameters
    /// * `position` - Click position in logical coordinates
    /// * `time_ms` - Current time in milliseconds (for multi-click detection)
    ///
    /// ## Returns
    /// * `Option<Vec<DomNodeId>>` - Affected nodes that need re-rendering, None if click didn't hit
    ///   text
    pub fn process_mouse_click_for_selection(
        &mut self,
        position: azul_core::geom::LogicalPosition,
        time_ms: u64,
    ) -> Option<Vec<azul_core::dom::DomNodeId>> {
        use azul_core::selection::{Selection, SelectionRange};

        use crate::{
            managers::hover::InputPointId,
            text3::selection::{select_paragraph_at_cursor, select_word_at_cursor},
        };

        // Get the current hit test to find which node was clicked
        let hit_test = self.hover_manager.get_current(&InputPointId::Mouse)?;

        // Find the first text node that was hit
        // Text nodes have a text layout in the text cache
        let mut hit_text_node: Option<(DomId, NodeId)> = None;

        for (dom_id, hit) in &hit_test.hovered_nodes {
            for (node_id, _hit_item) in &hit.regular_hit_test_nodes {
                // Check if this node has text layout
                // For now, we'll check if it's in any layout result
                if let Some(layout_result) = self.layout_results.get(dom_id) {
                    if layout_result
                        .styled_dom
                        .node_data
                        .as_container()
                        .get(*node_id)
                        .is_some()
                    {
                        hit_text_node = Some((*dom_id, *node_id));
                        break;
                    }
                }
            }

            if hit_text_node.is_some() {
                break;
            }
        }

        let (dom_id, node_id) = hit_text_node?;

        // Create DomNodeId for click state tracking
        let node_hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
        let dom_node_id = azul_core::dom::DomNodeId {
            dom: dom_id,
            node: node_hierarchy_id,
        };

        // Update click count to determine click count
        let click_count = self
            .selection_manager
            .update_click_count(dom_node_id, position, time_ms);

        // Get the text layout for this node
        // We need to iterate through all cache entries to find ones that match this node
        let mut selection_ranges = Vec::new();

        for cache_id in self.text_cache.get_all_layout_ids() {
            let layout = match self.text_cache.get_layout(&cache_id) {
                Some(l) => l,
                None => continue,
            };

            // Hit-test the layout to find cursor position
            let cursor = match layout.hittest_cursor(position) {
                Some(c) => c,
                None => continue,
            };

            // Apply selection based on click count
            let selection = match click_count {
                1 => {
                    // Single click: place cursor
                    Some(SelectionRange {
                        start: cursor,
                        end: cursor,
                    })
                }
                2 => {
                    // Double click: select word
                    select_word_at_cursor(&cursor, layout.as_ref())
                }
                3 => {
                    // Triple click: select paragraph (line)
                    select_paragraph_at_cursor(&cursor, layout.as_ref())
                }
                _ => None,
            };

            if let Some(sel) = selection {
                selection_ranges.push(sel);
            }
        }

        // Clear existing selections and set new one
        self.selection_manager.clear_selection(&dom_id);

        if !selection_ranges.is_empty() {
            let state = azul_core::selection::SelectionState {
                selections: selection_ranges.into_iter().map(Selection::Range).collect(),
                node_id: dom_node_id,
            };
            self.selection_manager.set_selection(dom_id, state);
        }

        // Return the affected node for dirty tracking
        Some(vec![dom_node_id])
    }

    /// Delete the currently selected text
    ///
    /// Handles Backspace/Delete key when a selection exists. The selection is deleted
    /// and replaced with a single cursor at the deletion point.
    ///
    /// ## Arguments
    /// * `target` - The target node (focused contenteditable element)
    /// * `forward` - true for Delete key (forward), false for Backspace (backward)
    ///
    /// ## Returns
    /// * `Some(Vec<DomNodeId>)` - Affected nodes if selection was deleted
    /// * `None` - If no selection exists or deletion failed
    pub fn delete_selection(
        &mut self,
        target: azul_core::dom::DomNodeId,
        forward: bool,
    ) -> Option<Vec<azul_core::dom::DomNodeId>> {
        use azul_core::selection::SelectionRange;

        let dom_id = target.dom;

        // Get current selection ranges
        let ranges = self.selection_manager.get_ranges(&dom_id);
        if ranges.is_empty() {
            return None; // No selection to delete
        }

        // For each selection range, delete the selected text
        // Note: For now, we just clear the selection and place cursor at start
        // Full implementation would need to modify the underlying text content
        // via the changeset system

        // Find the earliest cursor position from all ranges
        let mut earliest_cursor = None;
        for range in &ranges {
            // Use the start position for backward deletion, end for forward
            let cursor = if forward { range.end } else { range.start };

            if earliest_cursor.is_none() {
                earliest_cursor = Some(cursor);
            } else if let Some(current) = earliest_cursor {
                // Compare cursor positions using cluster_id ordering
                // Earlier cluster_id means earlier position in text
                if cursor < current {
                    earliest_cursor = Some(cursor);
                }
            }
        }

        // Clear selection and place cursor at deletion point
        self.selection_manager.clear_selection(&dom_id);

        if let Some(cursor) = earliest_cursor {
            // Set cursor at deletion point
            let state = azul_core::selection::SelectionState {
                selections: vec![azul_core::selection::Selection::Range(SelectionRange {
                    start: cursor,
                    end: cursor,
                })],
                node_id: target,
            };
            self.selection_manager.set_selection(dom_id, state);
        }

        // Return affected nodes for dirty tracking
        Some(vec![target])
    }

    /// Extract clipboard content from the current selection
    ///
    /// This method extracts both plain text and styled text from the selection ranges.
    /// It iterates through all selected text, extracts the actual characters, and
    /// preserves styling information from the ShapedGlyph's StyleProperties.
    ///
    /// This is NOT reading from the system clipboard - use `clipboard_manager.get_paste_content()`
    /// for that. This extracts content FROM the selection TO be copied.
    ///
    /// ## Arguments
    /// * `dom_id` - The DOM to extract selection from
    ///
    /// ## Returns
    /// * `Some(ClipboardContent)` - If there is a selection with text
    /// * `None` - If no selection or no text layouts found
    pub fn get_selected_content_for_clipboard(
        &self,
        dom_id: &DomId,
    ) -> Option<crate::managers::selection::ClipboardContent> {
        use crate::{
            managers::selection::{ClipboardContent, StyledTextRun},
            text3::cache::ShapedItem,
        };

        // Get selection ranges for this DOM
        let ranges = self.selection_manager.get_ranges(dom_id);
        if ranges.is_empty() {
            return None;
        }

        let mut plain_text = String::new();
        let mut styled_runs = Vec::new();

        // Iterate through all text layouts to find selected content
        for cache_id in self.text_cache.get_all_layout_ids() {
            let layout = self.text_cache.get_layout(&cache_id)?;

            // Process each selection range
            for range in &ranges {
                // Iterate through positioned items in the layout
                for positioned_item in &layout.items {
                    match &positioned_item.item {
                        ShapedItem::Cluster(cluster) => {
                            // Check if this cluster is within the selection range
                            let cluster_id = cluster.source_cluster_id;

                            // Simple check: is this cluster between start and end?
                            let in_range = if range.start.cluster_id <= range.end.cluster_id {
                                cluster_id >= range.start.cluster_id
                                    && cluster_id <= range.end.cluster_id
                            } else {
                                cluster_id >= range.end.cluster_id
                                    && cluster_id <= range.start.cluster_id
                            };

                            if in_range {
                                // Extract text from cluster
                                plain_text.push_str(&cluster.text);

                                // Extract styling from first glyph (they share styling)
                                if let Some(first_glyph) = cluster.glyphs.first() {
                                    let style = &first_glyph.style;

                                    // Extract font family from font selector (use first from stack)
                                    let default_font = crate::text3::cache::FontSelector::default();
                                    let first_font = style.font_stack.first().unwrap_or(&default_font);
                                    let font_family = Some(first_font.family.clone());

                                    // Check if bold/italic from font selector
                                    use rust_fontconfig::FcWeight;
                                    let is_bold = matches!(
                                        first_font.weight,
                                        FcWeight::Bold | FcWeight::ExtraBold | FcWeight::Black
                                    );
                                    let is_italic = matches!(
                                        first_font.style,
                                        crate::text3::cache::FontStyle::Italic
                                            | crate::text3::cache::FontStyle::Oblique
                                    );

                                    styled_runs.push(StyledTextRun {
                                        text: cluster.text.clone(),
                                        font_family,
                                        font_size_px: style.font_size_px,
                                        color: style.color,
                                        is_bold,
                                        is_italic,
                                    });
                                }
                            }
                        }
                        // For now, skip non-cluster items (objects, breaks, etc.)
                        _ => {}
                    }
                }
            }
        }

        if plain_text.is_empty() {
            None
        } else {
            Some(ClipboardContent {
                plain_text,
                styled_runs,
            })
        }
    }

    /// Process image callback updates from CallbackChangeResult
    ///
    /// This function re-invokes image callbacks for nodes that requested updates
    /// (typically from timer callbacks or resize events). It returns the updated
    /// textures along with their metadata for the rendering pipeline to process.
    ///
    /// # Arguments
    ///
    /// * `image_callbacks_changed` - Map of DomId -> Set of NodeIds that need re-rendering
    /// * `gl_context` - OpenGL context pointer for rendering
    ///
    /// # Returns
    ///
    /// Vector of (DomId, NodeId, Texture) tuples for textures that were updated
    pub fn process_image_callback_updates(
        &mut self,
        image_callbacks_changed: &BTreeMap<DomId, FastBTreeSet<NodeId>>,
        gl_context: &OptionGlContextPtr,
    ) -> Vec<(DomId, NodeId, azul_core::gl::Texture)> {
        use azul_core::{callbacks::HidpiAdjustedBounds, dom::NodeType};

        use crate::callbacks::{RenderImageCallback, RenderImageCallbackInfo};

        let mut updated_textures = Vec::new();

        for (dom_id, node_ids) in image_callbacks_changed {
            let layout_result = match self.layout_results.get_mut(dom_id) {
                Some(lr) => lr,
                None => continue,
            };

            for node_id in node_ids {
                // Get the node data - store container ref to extend lifetime
                let node_data_container = layout_result.styled_dom.node_data.as_container();
                let node_data = match node_data_container.get(*node_id) {
                    Some(nd) => nd,
                    None => continue,
                };

                // Check if this is an Image node with a callback
                let has_callback = matches!(node_data.get_node_type(), NodeType::Image(img_ref)
                    if img_ref.get_image_callback().is_some());

                if !has_callback {
                    continue;
                }

                // Get layout indices for this DOM node (can have multiple due to text splitting,
                // etc.)
                let layout_indices = match layout_result.layout_tree.dom_to_layout.get(node_id) {
                    Some(indices) if !indices.is_empty() => indices,
                    _ => continue,
                };

                // Use the first layout index (primary node)
                let layout_index = layout_indices[0];

                // Get the position from calculated_positions
                let position = match layout_result.calculated_positions.get(&layout_index) {
                    Some(pos) => *pos,
                    None => continue,
                };

                // Get the layout node to determine size
                let layout_node = match layout_result.layout_tree.get(layout_index) {
                    Some(ln) => ln,
                    None => continue,
                };

                // Get the size from the layout node (used_size is the computed size from layout)
                let (width, height) = match layout_node.used_size {
                    Some(size) => (size.width, size.height),
                    None => continue, // Node hasn't been laid out yet
                };

                let callback_domnode_id = DomNodeId {
                    dom: *dom_id,
                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(
                        *node_id,
                    )),
                };

                let bounds = HidpiAdjustedBounds::from_bounds(
                    azul_css::props::basic::LayoutSize {
                        width: width as isize,
                        height: height as isize,
                    },
                    self.current_window_state.size.get_hidpi_factor(),
                );

                // Create callback info
                let mut gl_callback_info = RenderImageCallbackInfo::new(
                    callback_domnode_id,
                    bounds,
                    gl_context,
                    &self.image_cache,
                    &self.font_manager.fc_cache,
                );

                // Invoke the callback
                let new_image_ref = {
                    let mut node_data_mut = layout_result.styled_dom.node_data.as_container_mut();
                    match node_data_mut.get_mut(*node_id) {
                        Some(nd) => {
                            match &mut nd.node_type {
                                NodeType::Image(img_ref) => {
                                    img_ref.get_image_callback_mut().map(|core_callback| {
                                        // Convert from CoreImageCallback (cb: usize) to
                                        // RenderImageCallback (cb: fn pointer)
                                        let callback =
                                            RenderImageCallback::from_core(&core_callback.callback);
                                        (callback.cb)(
                                            &mut core_callback.data,
                                            &mut gl_callback_info,
                                        )
                                    })
                                }
                                _ => None,
                            }
                        }
                        None => None,
                    }
                };

                // Reset GL state after callback
                #[cfg(feature = "gl_context_loader")]
                if let Some(gl) = gl_context.as_ref() {
                    use gl_context_loader::gl;
                    gl.bind_framebuffer(gl::FRAMEBUFFER, 0);
                    gl.disable(gl::FRAMEBUFFER_SRGB);
                    gl.disable(gl::MULTISAMPLE);
                }

                // Extract the texture from the returned ImageRef
                if let Some(image_ref) = new_image_ref {
                    if let Some(decoded_image) = image_ref.into_inner() {
                        if let azul_core::resources::DecodedImage::Gl(texture) = decoded_image {
                            updated_textures.push((*dom_id, *node_id, texture));
                        }
                    }
                }
            }
        }

        updated_textures
    }

    /// Process IFrame updates requested by callbacks
    ///
    /// This method handles manual IFrame re-rendering triggered by `trigger_iframe_rerender()`.
    /// It invokes the IFrame callback with `DomRecreated` reason and performs layout on the
    /// returned DOM, then submits a new display list to WebRender for that pipeline.
    ///
    /// # Arguments
    ///
    /// * `iframes_to_update` - Map of DomId -> Set of NodeIds that need re-rendering
    /// * `window_state` - Current window state
    /// * `renderer_resources` - Renderer resources
    /// * `system_callbacks` - External system callbacks
    ///
    /// # Returns
    ///
    /// Vector of (DomId, NodeId) tuples for IFrames that were successfully updated
    pub fn process_iframe_updates(
        &mut self,
        iframes_to_update: &BTreeMap<DomId, FastBTreeSet<NodeId>>,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
    ) -> Vec<(DomId, NodeId)> {
        let mut updated_iframes = Vec::new();

        for (dom_id, node_ids) in iframes_to_update {
            for node_id in node_ids {
                // Extract iframe bounds from layout result
                let bounds = match Self::get_iframe_bounds_from_layout(
                    &self.layout_results,
                    *dom_id,
                    *node_id,
                ) {
                    Some(b) => b,
                    None => continue,
                };

                // Force re-invocation by clearing the "was_invoked" flag
                self.iframe_manager.force_reinvoke(*dom_id, *node_id);

                // Invoke the IFrame callback
                if let Some(_child_dom_id) = self.invoke_iframe_callback(
                    *dom_id,
                    *node_id,
                    bounds,
                    window_state,
                    renderer_resources,
                    system_callbacks,
                    &mut None,
                ) {
                    updated_iframes.push((*dom_id, *node_id));
                }
            }
        }

        updated_iframes
    }

    /// Queue IFrame updates to be processed in the next frame
    ///
    /// This is called after callbacks to store the iframes_to_update from CallbackChangeResult
    pub fn queue_iframe_updates(
        &mut self,
        iframes_to_update: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    ) {
        for (dom_id, node_ids) in iframes_to_update {
            self.pending_iframe_updates
                .entry(dom_id)
                .or_insert_with(FastBTreeSet::new)
                .extend(node_ids);
        }
    }

    /// Process and clear pending IFrame updates
    ///
    /// This is called during frame generation to re-render updated IFrames
    pub fn process_pending_iframe_updates(
        &mut self,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
    ) -> Vec<(DomId, NodeId)> {
        if self.pending_iframe_updates.is_empty() {
            return Vec::new();
        }

        // Take ownership of pending updates
        let iframes_to_update = core::mem::take(&mut self.pending_iframe_updates);

        // Process them
        self.process_iframe_updates(
            &iframes_to_update,
            window_state,
            renderer_resources,
            system_callbacks,
        )
    }

    /// Helper: Extract IFrame bounds from layout results
    ///
    /// Returns None if the node is not an IFrame or doesn't have layout info
    fn get_iframe_bounds_from_layout(
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<LogicalRect> {
        use azul_core::dom::NodeType;

        let layout_result = layout_results.get(&dom_id)?;

        // Check if this is an IFrame node
        let node_data_container = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id)?;

        if !matches!(node_data.get_node_type(), NodeType::IFrame(_)) {
            return None;
        }

        // Get layout indices
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&node_id)?;
        if layout_indices.is_empty() {
            return None;
        }

        let layout_index = layout_indices[0];

        // Get position
        let position = *layout_result.calculated_positions.get(&layout_index)?;

        // Get size
        let layout_node = layout_result.layout_tree.get(layout_index)?;
        let size = layout_node.used_size?;

        Some(LogicalRect::new(
            position,
            LogicalSize::new(size.width as f32, size.height as f32),
        ))
    }
}

#[cfg(feature = "a11y")]
#[derive(Debug, Clone)]
pub enum TextEditType {
    ReplaceSelection(String),
    SetValue(String),
    SetNumericValue(f64),
}

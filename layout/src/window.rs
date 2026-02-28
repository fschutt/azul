//! Window layout management for solver3/text3
//!
//! This module provides the high-level API for managing layout
//! state across frames, including caching, incremental updates,
//! and display list generation.
//!
//! The main entry point is `LayoutWindow`, which encapsulates all
//! the state needed to perform layout and maintain consistency
//! across window resizes and DOM updates.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use azul_core::{
    animation::UpdateImageType,
    callbacks::{FocusTarget, HidpiAdjustedBounds, VirtualizedViewCallbackReason, Update},
    dom::{
        AccessibilityAction, AttributeType, Dom, DomId, DomIdVec, DomNodeId, NodeId, NodeType, On,
    },
    events::{EasingFunction, EventFilter, FocusEventFilter, HoverEventFilter},
    geom::{LogicalPosition, LogicalRect, LogicalSize, OptionLogicalPosition},
    gl::OptionGlContextPtr,
    gpu::{GpuScrollbarOpacityEvent, GpuValueCache},
    hit_test::{DocumentId, ScrollPosition, ScrollbarHitId},
    refany::{OptionRefAny, RefAny},
    resources::{
        Epoch, FontKey, GlTextureCache, IdNamespace, ImageCache, ImageMask, ImageRef, ImageRefHash,
        OpacityKey, RendererResources,
    },
    selection::{
        CursorAffinity, GraphemeClusterId, Selection, SelectionAnchor, SelectionFocus,
        SelectionRange, SelectionState, TextCursor, TextSelection,
    },
    styled_dom::{
        collect_nodes_in_document_order, is_before_in_document_order, NodeHierarchyItemId,
        StyledDom,
    },
    task::{
        Duration, Instant, SystemTickDiff, SystemTimeDiff, TerminateTimer, ThreadId, ThreadIdVec,
        ThreadSendMsg, TimerId, TimerIdVec,
    },
    window::{CursorPosition, MonitorVec, RawWindowHandle, RendererType},
    FastBTreeSet, FastHashMap,
};
use azul_css::{
    css::Css,
    props::{
        basic::FontRef,
        property::{CssProperty, CssPropertyVec},
    },
    AzString, LayoutDebugMessage, OptionString,
};
use rust_fontconfig::FcFontCache;

#[cfg(feature = "icu")]
use crate::icu::IcuLocalizerHandle;
use crate::{
    callbacks::{
        Callback, ExternalSystemCallbacks, MenuCallback,
    },
    managers::{
        gpu_state::GpuStateManager,
        virtualized_view::VirtualizedViewManager,
        scroll_state::{ScrollManager, ScrollStates},
    },
    solver3::{
        self, cache::LayoutCache as Solver3LayoutCache, display_list::DisplayList,
        layout_tree::LayoutTree,
    },
    text3::{
        cache::{
            FontManager, FontSelector, FontStyle, InlineContent, LayoutCache as TextLayoutCache,
            LayoutError, ShapedItem, StyleProperties, StyledRun, TextBoundary, UnifiedConstraints,
            UnifiedLayout,
        },
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
    MovedWithinNode(TextCursor),
    /// Cursor moved to a different text node
    MovedToNode {
        dom_id: DomId,
        node_id: NodeId,
        cursor: TextCursor,
    },
    /// Cursor is at a boundary and cannot move further
    AtBoundary {
        boundary: TextBoundary,
        cursor: TextCursor,
    },
}

/// Error when no cursor destination is available
#[derive(Debug, Clone)]
pub struct NoCursorDestination {
    pub reason: String,
}

/// Action to take for the cursor blink timer when focus changes
///
/// This enum is returned by `LayoutWindow::handle_focus_change_for_cursor_blink()`
/// to tell the platform layer what timer action to take.
#[derive(Debug, Clone)]
pub enum CursorBlinkTimerAction {
    /// Start the cursor blink timer with the given timer configuration
    Start(crate::timer::Timer),
    /// Stop the cursor blink timer
    Stop,
    /// No change needed (timer already in correct state)
    NoChange,
}

/// Helper function to create a unique IdNamespace
fn new_id_namespace() -> IdNamespace {
    let id = ID_NAMESPACE_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
    IdNamespace(id)
}

// ============================================================================
// Cursor Blink Timer Callback
// ============================================================================

/// Destructor for cursor blink timer RefAny (no-op since we use null pointer)
extern "C" fn cursor_blink_timer_destructor(_: RefAny) {
    // No cleanup needed - we use a null pointer RefAny
}

/// Callback for the cursor blink timer
///
/// This function is called every ~530ms to toggle cursor visibility.
/// It checks if enough time has passed since the last user input before blinking,
/// to avoid blinking while the user is actively typing.
///
/// The callback returns:
/// - `TerminateTimer::Continue` + `Update::RefreshDom` if cursor toggled
/// - `TerminateTimer::Terminate` if focus is no longer on a contenteditable element
pub extern "C" fn cursor_blink_timer_callback(
    _data: RefAny,
    mut info: crate::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::callbacks::{TimerCallbackReturn, Update};
    use azul_core::task::TerminateTimer;

    // Get current time
    let now = info.get_current_time();

    // We need to access the LayoutWindow through the info
    // The timer callback needs to:
    // 1. Check if focus is still on a contenteditable element
    // 2. Check time since last input
    // 3. Toggle visibility or keep solid

    // For now, we'll queue changes via the CallbackInfo system
    // The actual state modification happens in apply_user_change

    // Check if we should blink or stay solid
    // This is done by checking CursorManager.should_blink(now) in the layout window

    // Since we can't access LayoutWindow directly here (it's not passed to timer callbacks),
    // we use a different approach: the timer callback always toggles, and the visibility
    // check is done in display_list.rs based on CursorManager state.

    // Simply toggle cursor visibility
    info.set_cursor_visibility_toggle();

    // Continue the timer and request a redraw
    TimerCallbackReturn {
        should_update: Update::RefreshDom,
        should_terminate: TerminateTimer::Continue,
    }
}

/// Result of a layout pass for a single DOM, before display list generation
#[derive(Debug)]
pub struct DomLayoutResult {
    /// The styled DOM that was laid out
    pub styled_dom: StyledDom,
    /// The layout tree with computed sizes and positions
    pub layout_tree: LayoutTree,
    /// Absolute positions of all nodes
    pub calculated_positions: crate::solver3::PositionVec,
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
// Re-export PendingTextEdit from text_input manager
pub use crate::managers::text_input::PendingTextEdit;

/// Cached text layout constraints for a node
/// These are the layout parameters that were used to shape the text
#[derive(Debug, Clone)]
pub struct TextConstraintsCache {
    /// Map from (dom_id, node_id) to their layout constraints
    pub constraints: BTreeMap<(DomId, NodeId), UnifiedConstraints>,
}

impl Default for TextConstraintsCache {
    fn default() -> Self {
        Self {
            constraints: BTreeMap::new(),
        }
    }
}

/// A text node that has been edited since the last full layout.
/// This allows us to perform lightweight relayout without rebuilding the entire DOM.
#[derive(Debug, Clone)]
pub struct DirtyTextNode {
    /// The new inline content (text + images) after editing
    pub content: Vec<InlineContent>,
    /// The new cursor position after editing
    pub cursor: Option<TextCursor>,
    /// Whether this edit requires ancestor relayout (e.g., text grew taller)
    pub needs_ancestor_relayout: bool,
}

/// A window-level layout manager that encapsulates all layout state and caching.
///
/// This struct owns the layout and text caches, and provides methods dir_to:
/// - Perform initial layout
/// - Incrementally update layout on DOM changes
/// - Generate display lists for rendering
/// - Handle window resizes efficiently
/// - Manage multiple DOMs (for VirtualizedViews)
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
    /// Cached layout results for all DOMs (root + virtualized views)
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
    /// VirtualizedView manager for all nodes across all DOMs
    pub virtualized_view_manager: VirtualizedViewManager,
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
    /// Tracks which nodes have been edited since last full layout.
    /// Key: (DomId, NodeId of IFC root)
    /// Value: The edited inline content that should be used for relayout
    dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>,
    /// Pending VirtualizedView updates from callbacks (processed in next frame)
    /// Map of DomId -> Set of NodeIds that need re-rendering
    pub pending_virtualized_view_updates: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    /// System style (colors, fonts, metrics) for resolving system color keywords
    /// Set via `set_system_style()` from the shell after window creation
    pub system_style: Option<std::sync::Arc<azul_css::system::SystemStyle>>,
    /// Shared monitor list — initialized once at app start, updated by the platform
    /// layer on monitor topology changes. Arc<Mutex> allows zero-cost sharing
    /// across all CallbackInfoRefData without cloning the Vec each time.
    pub monitors: std::sync::Arc<std::sync::Mutex<MonitorVec>>,
    /// ICU4X localizer handle for internationalized formatting (numbers, dates, lists, plurals)
    /// Initialized from system language at startup, can be overridden
    #[cfg(feature = "icu")]
    pub icu_localizer: IcuLocalizerHandle,
}

fn default_duration_500ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(500))
}

fn default_duration_200ms() -> Duration {
    Duration::System(SystemTimeDiff::from_millis(200))
}

/// Helper function to convert Duration to milliseconds
///
/// Duration is an enum with System (std::time::Duration) and Tick variants.
/// We need to handle both cases for proper time calculations.
fn duration_to_millis(duration: Duration) -> u64 {
    match duration {
        #[cfg(feature = "std")]
        Duration::System(system_diff) => {
            let std_duration: std::time::Duration = system_diff.into();
            std_duration.as_millis() as u64
        }
        #[cfg(not(feature = "std"))]
        Duration::System(system_diff) => {
            // Manual calculation: secs * 1000 + nanos / 1_000_000
            system_diff.secs * 1000 + (system_diff.nanos / 1_000_000) as u64
        }
        Duration::Tick(tick_diff) => {
            // Assume tick = 1ms for simplicity (platform-specific)
            tick_diff.tick_diff
        }
    }
}

impl LayoutWindow {
    /// Create a new layout window with empty caches.
    ///
    /// For full initialization with WindowInternal compatibility, use `new_full()`.
    pub fn new(fc_cache: FcFontCache) -> Result<Self, crate::solver3::LayoutError> {
        Ok(Self {
            // Default width, will be updated on first layout
            #[cfg(feature = "pdf")]
            fragmentation_context: crate::paged::FragmentationContext::new_continuous(800.0),
            layout_cache: Solver3LayoutCache {
                tree: None,
                calculated_positions: Vec::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
                counters: BTreeMap::new(),
                float_cache: BTreeMap::new(),
                cache_map: Default::default(),
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
            virtualized_view_manager: VirtualizedViewManager::new(),
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
            dirty_text_nodes: BTreeMap::new(),
            pending_virtualized_view_updates: BTreeMap::new(),
            system_style: None,
            monitors: std::sync::Arc::new(std::sync::Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
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
                calculated_positions: Vec::new(),
                viewport: None,
                scroll_ids: BTreeMap::new(),
                scroll_id_to_node_id: BTreeMap::new(),
                counters: BTreeMap::new(),
                float_cache: BTreeMap::new(),
                cache_map: Default::default(),
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
            virtualized_view_manager: VirtualizedViewManager::new(),
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
            dirty_text_nodes: BTreeMap::new(),
            pending_virtualized_view_updates: BTreeMap::new(),
            system_style: None,
            monitors: std::sync::Arc::new(std::sync::Mutex::new(MonitorVec::from_const_slice(&[]))),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
        })
    }

    /// Perform layout on a styled DOM and generate a display list.
    ///
    /// This is the main entry point for layout. It handles:
    /// - Incremental layout updates using the cached layout tree
    /// - Text shaping and line breaking
    /// - VirtualizedView callback invocation and recursive layout
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

        // CRITICAL: Reset VirtualizedView invocation flags so check_reinvoke() returns
        // InitialRender for every tracked VirtualizedView. Without this, the VirtualizedViewManager
        // still has was_invoked=true from the previous frame, so it skips
        // re-invocation — but the child DOM was just destroyed by clear().
        self.virtualized_view_manager.reset_all_invocation_flags();

        if let Some(msgs) = debug_messages.as_mut() {
            msgs.push(LayoutDebugMessage::info(format!(
                "[layout_and_generate_display_list] Starting layout for DOM with {} nodes",
                root_dom.node_data.len()
            )));
        }

        // Start recursive layout from the root DOM
        let result = self.layout_dom_recursive(
            root_dom,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        );

        if let Err(ref e) = result {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::error(format!(
                    "[layout_and_generate_display_list] Layout FAILED: {:?}",
                    e
                )));
            }
        } else {
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[layout_and_generate_display_list] Layout SUCCESS, layout_results count: {}",
                    self.layout_results.len()
                )));
            }
        }

        // After successful layout, update the accessibility tree
        // Note: This is wrapped in catch_unwind to prevent a11y issues from crashing the app
        #[cfg(feature = "a11y")]
        if result.is_ok() {
            // Use catch_unwind to prevent a11y panics from crashing the main application
            let a11y_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::managers::a11y::A11yManager::update_tree(
                    self.a11y_manager.root_id,
                    &self.layout_results,
                    &self.current_window_state.title,
                    self.current_window_state.size.dimensions,
                )
            }));

            match a11y_result {
                Ok(tree_update) => {
                    // Store the tree_update for platform adapter to consume
                    self.a11y_manager.last_tree_update = Some(tree_update);
                }
                Err(_) => {
                    // A11y update failed - log and continue without a11y
                }
            }
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

        // Get the platform from system_style, falling back to compile-time detection
        let platform = self.system_style.as_ref()
            .map(|s| s.platform.clone())
            .unwrap_or_else(azul_css::system::Platform::current);

        // Font Resolution And Loading
        // This must happen BEFORE layout_document() is called
        {
            use crate::{
                solver3::getters::{
                    collect_and_resolve_font_chains, collect_font_ids_from_chains,
                    compute_fonts_to_load, load_fonts_from_disk, register_embedded_fonts_from_styled_dom,
                },
                text3::default::PathLoader,
            };

            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(
                    "[FontLoading] Starting font resolution for DOM".to_string(),
                ));
            }

            // Step 0: Register embedded FontRefs (e.g. Material Icons)
            // These fonts bypass fontconfig and are used directly
            register_embedded_fonts_from_styled_dom(&styled_dom, &self.font_manager, &platform);

            // Step 1: Resolve font chains (cached by FontChainKey)
            let chains = collect_and_resolve_font_chains(&styled_dom, &self.font_manager.fc_cache, &platform);
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[FontLoading] Resolved {} font chains",
                    chains.len()
                )));
            }

            // Step 2: Get required font IDs from chains
            let required_fonts = collect_font_ids_from_chains(&chains);
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[FontLoading] Required fonts: {} unique fonts",
                    required_fonts.len()
                )));
            }

            // Step 3: Compute which fonts need to be loaded (diff with already loaded)
            let already_loaded = self.font_manager.get_loaded_font_ids();
            let fonts_to_load = compute_fonts_to_load(&required_fonts, &already_loaded);
            if let Some(msgs) = debug_messages.as_mut() {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[FontLoading] Already loaded: {}, need to load: {}",
                    already_loaded.len(),
                    fonts_to_load.len()
                )));
            }

            // Step 4: Load missing fonts
            if !fonts_to_load.is_empty() {
                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[FontLoading] Loading {} fonts from disk...",
                        fonts_to_load.len()
                    )));
                }
                let loader = PathLoader::new();
                let load_result = load_fonts_from_disk(
                    &fonts_to_load,
                    &self.font_manager.fc_cache,
                    |bytes, index| loader.load_font(bytes, index),
                );

                if let Some(msgs) = debug_messages.as_mut() {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[FontLoading] Loaded {} fonts, {} failed",
                        load_result.loaded.len(),
                        load_result.failed.len()
                    )));
                }

                // Insert loaded fonts into the font manager
                self.font_manager.insert_fonts(load_result.loaded);

                // Log any failures
                for (font_id, error) in &load_result.failed {
                    if let Some(msgs) = debug_messages.as_mut() {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "[FontLoading] Failed to load font {:?}: {}",
                            font_id, error
                        )));
                    }
                }
            }

            // Step 5: Update font chain cache
            self.font_manager.set_font_chain_cache(chains.into_fontconfig_chains());
        }

        let scroll_offsets = self.scroll_manager.get_scroll_states_for_dom(dom_id);
        let styled_dom_clone = styled_dom.clone();
        let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id).clone();

        // Get cursor visibility from cursor manager for display list generation
        let cursor_is_visible = self.cursor_manager.should_draw_cursor();

        // Get cursor location from cursor manager for independent cursor rendering
        let cursor_location = self.cursor_manager.get_cursor_location().and_then(|loc| {
            self.cursor_manager.get_cursor().map(|cursor| {
                (loc.dom_id, loc.node_id, cursor.clone())
            })
        });

        let mut display_list = solver3::layout_document(
            &mut self.layout_cache,
            &mut self.text_cache,
            styled_dom,
            viewport,
            &self.font_manager,
            &scroll_offsets,
            &self.selection_manager.selections,
            &self.selection_manager.text_selections,
            debug_messages,
            Some(&gpu_cache),
            &self.renderer_resources,
            self.id_namespace,
            dom_id,
            cursor_is_visible,
            cursor_location,
            self.system_style.clone(),
            system_callbacks.get_system_time_fn,
        )?;

        let tree = self
            .layout_cache
            .tree
            .clone()
            .ok_or(solver3::LayoutError::InvalidTree)?;

        // Get scroll IDs from cache (they were computed during layout_document)
        let scroll_ids = self.layout_cache.scroll_ids.clone();
        let scroll_id_to_node_id = self.layout_cache.scroll_id_to_node_id.clone();

        // Register scrollbar thumb TransformKeys from the display list into the GPU cache.
        // paint_scrollbars() creates TransformKey::unique() for each thumb. We need to
        // register those keys in the GPU cache so that update_scrollbar_transforms() can
        // update the values during GPU-only scroll (without display list rebuild).
        // Also register opacity keys from the display list the same way.
        {
            use crate::solver3::display_list::{DisplayListItem, ScrollbarDrawInfo};
            let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id);
            for item in &display_list.items {
                if let DisplayListItem::ScrollBarStyled { info } = item {
                    if let Some(hit_id) = &info.hit_id {
                        // Register transform keys
                        if let Some(transform_key) = info.thumb_transform_key {
                            match hit_id {
                                azul_core::hit_test::ScrollbarHitId::VerticalThumb(_, nid) => {
                                    if !gpu_cache.transform_keys.contains_key(nid) {
                                        gpu_cache.transform_keys.insert(*nid, transform_key);
                                        gpu_cache.current_transform_values.insert(*nid, info.thumb_initial_transform);
                                    }
                                }
                                azul_core::hit_test::ScrollbarHitId::HorizontalThumb(_, nid) => {
                                    if !gpu_cache.h_transform_keys.contains_key(nid) {
                                        gpu_cache.h_transform_keys.insert(*nid, transform_key);
                                        gpu_cache.h_current_transform_values.insert(*nid, info.thumb_initial_transform);
                                    }
                                }
                                _ => {}
                            }
                        }

                        // Register opacity keys (same pattern as transform keys).
                        // The display list always generates an OpacityKey for each
                        // scrollbar. We mirror these into the GPU cache so that
                        // synchronize_scrollbar_opacity can update the values and
                        // synchronize_gpu_values can push them to WebRender.
                        //
                        // Initial opacity depends on visibility mode:
                        //   Always       → 1.0 (legacy scrollbar, always visible)
                        //   WhenScrolling → 0.0 (overlay scrollbar, hidden until scroll)
                        //   Auto         → 0.0 (same as WhenScrolling)
                        let initial_opacity = if info.visibility == azul_css::props::style::scrollbar::ScrollbarVisibilityMode::Always {
                            1.0
                        } else {
                            0.0
                        };
                        if let Some(opacity_key) = info.opacity_key {
                            match hit_id {
                                azul_core::hit_test::ScrollbarHitId::VerticalThumb(_, nid) => {
                                    let key = (dom_id, *nid);
                                    if !gpu_cache.scrollbar_v_opacity_keys.contains_key(&key) {
                                        gpu_cache.scrollbar_v_opacity_keys.insert(key, opacity_key);
                                        gpu_cache.scrollbar_v_opacity_values.insert(key, initial_opacity);
                                    }
                                }
                                azul_core::hit_test::ScrollbarHitId::HorizontalThumb(_, nid) => {
                                    let key = (dom_id, *nid);
                                    if !gpu_cache.scrollbar_h_opacity_keys.contains_key(&key) {
                                        gpu_cache.scrollbar_h_opacity_keys.insert(key, opacity_key);
                                        gpu_cache.scrollbar_h_opacity_values.insert(key, initial_opacity);
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }

        // Synchronize scrollbar transforms AFTER layout
        self.gpu_state_manager
            .update_scrollbar_transforms(dom_id, &self.scroll_manager, &tree);

        // Scan for VirtualizedViews *after* the initial layout pass
        // Pass styled_dom directly — layout_results isn't populated yet at this point
        let vviews = self.scan_for_virtualized_views(&styled_dom_clone, &tree, &self.layout_cache.calculated_positions);

        for (node_id, bounds) in vviews {
            if let Some(child_dom_id) = self.invoke_virtualized_view_callback_with_dom(
                dom_id,
                node_id,
                bounds,
                Some(&styled_dom_clone),
                window_state,
                renderer_resources,
                system_callbacks,
                debug_messages,
            ) {
                // Replace the VirtualizedViewPlaceholder with the real VirtualizedView item.
                // The placeholder was emitted by generate_display_list() at the
                // correct position (outside any scroll frame, inside the parent clip).
                let mut replaced = false;
                for item in display_list.items.iter_mut() {
                    if let crate::solver3::display_list::DisplayListItem::VirtualizedViewPlaceholder {
                        node_id: ref placeholder_nid,
                        bounds: ref placeholder_bounds,
                        clip_rect: ref placeholder_clip,
                        ..
                    } = item
                    {
                        if *placeholder_nid == node_id {
                            *item = crate::solver3::display_list::DisplayListItem::VirtualizedView {
                                child_dom_id,
                                bounds: *placeholder_bounds,
                                clip_rect: *placeholder_clip,
                            };
                            replaced = true;
                            break;
                        }
                    }
                }

                if !replaced {
                    // Fallback: if no placeholder found (shouldn't happen), append at end
                    display_list
                        .items
                        .push(crate::solver3::display_list::DisplayListItem::VirtualizedView {
                            child_dom_id,
                            bounds: bounds.into(),
                            clip_rect: bounds.into(),
                        });
                }
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

    fn scan_for_virtualized_views(
        &self,
        styled_dom: &StyledDom,
        layout_tree: &LayoutTree,
        calculated_positions: &crate::solver3::PositionVec,
    ) -> Vec<(NodeId, LogicalRect)> {
        let node_data_container = styled_dom.node_data.as_container();
        layout_tree
            .nodes
            .iter()
            .enumerate()
            .filter_map(|(idx, node)| {
                let node_dom_id = node.dom_node_id?;
                let node_data = node_data_container.get(node_dom_id)?;
                if matches!(node_data.get_node_type(), NodeType::VirtualizedView) {
                    let pos = calculated_positions.get(idx).copied().unwrap_or_default();
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
            .map(|result| std::mem::replace(&mut result.display_list, DisplayList::default()))
            .ok_or(solver3::LayoutError::InvalidTree)
    }

    /// Clear all caches (useful for testing or when switching documents).
    pub fn clear_caches(&mut self) {
        self.layout_cache = Solver3LayoutCache {
            tree: None,
            calculated_positions: Vec::new(),
            viewport: None,
            scroll_ids: BTreeMap::new(),
            scroll_id_to_node_id: BTreeMap::new(),
            counters: BTreeMap::new(),
            float_cache: BTreeMap::new(),
            cache_map: Default::default(),
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

    /// Invoke a VirtualizedView callback and perform layout on the returned DOM.
    ///
    /// This is the entry point that looks up the necessary `VirtualizedViewNode` data before
    /// delegating to the core implementation logic.
    /// Invoke a VirtualizedView callback for a node. Returns the child DomId if the
    /// callback was invoked and the child DOM was laid out.
    ///
    /// This calls the VirtualizedView's own RefAny callback (NOT the main layout() callback),
    /// swaps the child StyledDom, and re-layouts only the VirtualizedView sub-tree.
    pub fn invoke_virtualized_view_callback(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        self.invoke_virtualized_view_callback_with_dom(
            parent_dom_id, node_id, bounds, None,
            window_state, renderer_resources, system_callbacks, debug_messages,
        )
    }

    /// Invoke a VirtualizedView callback. If `styled_dom_override` is provided, use it
    /// instead of reading from `self.layout_results` (needed during initial
    /// layout when layout_results isn't populated yet).
    fn invoke_virtualized_view_callback_with_dom(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        bounds: LogicalRect,
        styled_dom_override: Option<&StyledDom>,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "invoke_virtualized_view_callback called for node {:?}",
                node_id
            )));
        }

        // Use the override styled_dom if provided, otherwise read from layout_results
        let virtualized_view_node = if let Some(styled_dom) = styled_dom_override {
            let node_data_container = styled_dom.node_data.as_container();
            let node_data = node_data_container.get(node_id)?;
            node_data.get_virtualized_view_node_ref()?.clone()
        } else {
            let layout_result = self.layout_results.get(&parent_dom_id)?;
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    "Got layout result for parent DOM {:?}",
                    parent_dom_id
                )));
            }
            let node_data_container = layout_result.styled_dom.node_data.as_container();
            let node_data = node_data_container.get(node_id)?;
            match node_data.get_virtualized_view_node_ref() {
                Some(vv) => vv.clone(),
                None => {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "Node is NOT VirtualizedView, type = {:?}",
                            node_data.get_node_type()
                        )));
                    }
                    return None;
                }
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info("Node is VirtualizedView type".to_string()));
        }

        // Call the actual implementation with all necessary data
        self.invoke_virtualized_view_callback_impl(
            parent_dom_id,
            node_id,
            &virtualized_view_node,
            bounds,
            window_state,
            renderer_resources,
            system_callbacks,
            debug_messages,
        )
    }

    /// Core implementation for invoking a VirtualizedView callback and managing the recursive layout.
    ///
    /// This method implements the 5 conditional re-invocation rules by coordinating
    /// with the `VirtualizedViewManager` and `ScrollManager`.
    ///
    /// # Returns
    ///
    /// `Some(child_dom_id)` if the callback was invoked and the child DOM was laid out.
    /// The parent's display list generator will then use this ID to reference the child's
    /// display list. Returns `None` if the callback was not invoked.
    fn invoke_virtualized_view_callback_impl(
        &mut self,
        parent_dom_id: DomId,
        node_id: NodeId,
        virtualized_view_node: &azul_core::dom::VirtualizedViewNode,
        bounds: LogicalRect,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Option<DomId> {
        // Get current time from system callbacks for state updates
        let now = (system_callbacks.get_system_time_fn.cb)();

        // Update node bounds in the scroll manager. This is necessary for the VirtualizedViewManager
        // to correctly detect edge scroll conditions.
        self.scroll_manager.update_node_bounds(
            parent_dom_id,
            node_id,
            bounds,
            LogicalRect::new(LogicalPosition::zero(), bounds.size), // Initial content_rect
            now.clone(),
        );

        // Check with the VirtualizedViewManager to see if re-invocation is necessary.
        // It handles all 5 conditional rules.
        let reason = match self.virtualized_view_manager.check_reinvoke(
            parent_dom_id,
            node_id,
            &self.scroll_manager,
            bounds,
        ) {
            Some(r) => r,
            None => {
                // No re-invocation needed, but we still need the child_dom_id for the display list.
                return self
                    .virtualized_view_manager
                    .get_nested_dom_id(parent_dom_id, node_id);
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "VirtualizedView ({:?}, {:?}) - Reason: {:?}",
                parent_dom_id, node_id, reason
            )));
        }

        let scroll_offset = self
            .scroll_manager
            .get_current_offset(parent_dom_id, node_id)
            .unwrap_or_default();

        let hidpi_factor = window_state.size.get_hidpi_factor();

        // Create VirtualizedViewCallbackInfo with the most up-to-date state
        let mut callback_info = azul_core::callbacks::VirtualizedViewCallbackInfo::new(
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
        let callback_data = virtualized_view_node.refany.clone();

        // Invoke the user's VirtualizedView callback
        let callback_return = (virtualized_view_node.callback.cb)(callback_data, callback_info);

        // Mark the VirtualizedView as invoked to prevent duplicate InitialRender calls
        self.virtualized_view_manager
            .mark_invoked(parent_dom_id, node_id, reason);

        // Get the child StyledDom from the callback's return value
        let mut child_styled_dom = match callback_return.dom {
            azul_core::styled_dom::OptionStyledDom::Some(dom) => dom,
            azul_core::styled_dom::OptionStyledDom::None => {
                // If the callback returns None, it's an optimization hint.
                if reason == VirtualizedViewCallbackReason::InitialRender {
                    // For the very first render, create an empty div as a fallback.
                    let mut empty_dom = Dom::create_div();
                    let empty_css = Css::empty();
                    empty_dom.style(empty_css)
                } else {
                    // For subsequent calls, returning None means "keep the old DOM".
                    // We just need to update the scroll info and return the existing child ID.
                    self.virtualized_view_manager.update_virtualized_view_info(
                        parent_dom_id,
                        node_id,
                        callback_return.scroll_size,
                        callback_return.virtual_scroll_size,
                    );
                    // Propagate virtual scroll bounds to ScrollManager
                    self.scroll_manager.update_virtual_scroll_bounds(
                        parent_dom_id,
                        node_id,
                        callback_return.virtual_scroll_size,
                        Some(callback_return.scroll_offset),
                    );
                    return self
                        .virtualized_view_manager
                        .get_nested_dom_id(parent_dom_id, node_id);
                }
            }
        };

        // Get or create a unique DomId for the VirtualizedView's content
        let child_dom_id = self
            .virtualized_view_manager
            .get_or_create_nested_dom_id(parent_dom_id, node_id);
        child_styled_dom.dom_id = child_dom_id;

        // Update the VirtualizedViewManager with the new scroll sizes from the callback
        self.virtualized_view_manager.update_virtualized_view_info(
            parent_dom_id,
            node_id,
            callback_return.scroll_size,
            callback_return.virtual_scroll_size,
        );
        // Propagate virtual scroll bounds to ScrollManager
        self.scroll_manager.update_virtual_scroll_bounds(
            parent_dom_id,
            node_id,
            callback_return.virtual_scroll_size,
            Some(callback_return.scroll_offset),
        );

        // **RECURSIVE LAYOUT STEP**
        // Perform a full layout pass on the child DOM. This will recursively handle
        // any VirtualizedViews within this VirtualizedView.
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
        // Use dom_to_layout mapping since layout tree indices differ from DOM indices
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;
        let layout_index = *layout_indices.first()?;
        let layout_node = layout_result.layout_tree.get(layout_index)?;
        layout_node.used_size
    }

    /// Get the position of a laid-out node
    pub fn get_node_position(&self, node_id: DomNodeId) -> Option<LogicalPosition> {
        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;
        // Use dom_to_layout mapping since layout tree indices differ from DOM indices
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&nid)?;
        let layout_index = *layout_indices.first()?;
        let position = layout_result.calculated_positions.get(layout_index)?;
        Some(*position)
    }

    /// Get the hit test bounds of a node from the display list
    ///
    /// This is more reliable than get_node_position + get_node_size because
    /// the display list always contains the correct final rendered positions,
    /// including for nodes that may not have entries in calculated_positions.
    pub fn get_node_hit_test_bounds(&self, node_id: DomNodeId) -> Option<LogicalRect> {
        use crate::solver3::display_list::DisplayListItem;

        let layout_result = self.layout_results.get(&node_id.dom)?;
        let nid = node_id.node.into_crate_internal()?;

        // Look up tag_id from the authoritative tag_ids_to_node_ids mapping
        let nid_encoded = NodeHierarchyItemId::from_crate_internal(Some(nid));
        let tag_id = layout_result.styled_dom.tag_ids_to_node_ids.iter()
            .find(|m| m.node_id == nid_encoded)?
            .tag_id
            .inner;

        // Search the display list for a HitTestArea with matching tag
        // Note: tag is now (u64, u16) tuple where tag.0 is the TagId.inner
        for item in &layout_result.display_list.items {
            if let DisplayListItem::HitTestArea { bounds, tag } = item {
                if tag.0 == tag_id && bounds.0.size.width > 0.0 && bounds.0.size.height > 0.0 {
                    return Some(bounds.0);
                }
            }
        }
        None
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

    // Scroll Into View

    /// Scroll a DOM node into view
    ///
    /// This is the main API for scrolling elements into view. It handles:
    /// - Finding scroll ancestors
    /// - Calculating scroll deltas
    /// - Applying scroll animations
    ///
    /// # Arguments
    ///
    /// * `node_id` - The DOM node to scroll into view
    /// * `options` - Scroll alignment and animation options
    /// * `now` - Current timestamp for animations
    ///
    /// # Returns
    ///
    /// A vector of scroll adjustments that were applied
    pub fn scroll_node_into_view(
        &mut self,
        node_id: DomNodeId,
        options: crate::managers::scroll_into_view::ScrollIntoViewOptions,
        now: azul_core::task::Instant,
    ) -> Vec<crate::managers::scroll_into_view::ScrollAdjustment> {
        crate::managers::scroll_into_view::scroll_node_into_view(
            node_id,
            &self.layout_results,
            &mut self.scroll_manager,
            options,
            now,
        )
    }

    /// Scroll a text cursor into view
    ///
    /// Used when the cursor moves within a contenteditable element.
    /// The cursor rect should be in node-local coordinates.
    pub fn scroll_cursor_into_view(
        &mut self,
        cursor_rect: LogicalRect,
        node_id: DomNodeId,
        options: crate::managers::scroll_into_view::ScrollIntoViewOptions,
        now: azul_core::task::Instant,
    ) -> Vec<crate::managers::scroll_into_view::ScrollAdjustment> {
        crate::managers::scroll_into_view::scroll_cursor_into_view(
            cursor_rect,
            node_id,
            &self.layout_results,
            &mut self.scroll_manager,
            options,
            now,
        )
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
    pub fn get_timer_ids(&self) -> TimerIdVec {
        self.timers.keys().copied().collect::<Vec<_>>().into()
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

    /// Calculate milliseconds until the next timer needs to fire.
    ///
    /// Returns `None` if there are no timers, meaning the caller can block indefinitely.
    /// Returns `Some(0)` if a timer is already overdue.
    /// Otherwise returns the minimum time in milliseconds until any timer fires.
    ///
    /// This is used by Linux (X11/Wayland) to set an efficient poll/select timeout
    /// instead of always polling every 16ms.
    pub fn time_until_next_timer_ms(
        &self,
        get_system_time_fn: &azul_core::task::GetSystemTimeCallback,
    ) -> Option<u64> {
        if self.timers.is_empty() {
            return None; // No timers - can block indefinitely
        }

        let now = (get_system_time_fn.cb)();
        let mut min_ms: Option<u64> = None;

        for timer in self.timers.values() {
            let next_run = timer.instant_of_next_run();

            // Calculate time difference in milliseconds
            let ms_until = if next_run < now {
                0 // Timer is overdue
            } else {
                duration_to_millis(next_run.duration_since(&now))
            };

            min_ms = Some(match min_ms {
                Some(current_min) => current_min.min(ms_until),
                None => ms_until,
            });
        }

        min_ms
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
    pub fn get_thread_ids(&self) -> ThreadIdVec {
        self.threads.keys().copied().collect::<Vec<_>>().into()
    }

    // Cursor Blinking Timer

    /// Create the cursor blink timer
    ///
    /// This timer toggles cursor visibility at ~530ms intervals.
    /// It checks if enough time has passed since the last user input before blinking,
    /// to avoid blinking while the user is actively typing.
    pub fn create_cursor_blink_timer(&self, _window_state: &FullWindowState) -> crate::timer::Timer {
        use azul_core::task::{Duration, SystemTimeDiff};
        use crate::timer::{Timer, TimerCallback};
        use azul_core::refany::RefAny;

        let interval_ms = crate::managers::cursor::CURSOR_BLINK_INTERVAL_MS;

        // Create a RefAny with a unit type - the timer callback doesn't need any data
        // The actual cursor state is in LayoutWindow.cursor_manager
        let refany = RefAny::new(());

        Timer {
            refany,
            node_id: None.into(),
            created: azul_core::task::Instant::now(),
            run_count: 0,
            last_run: azul_core::task::OptionInstant::None,
            delay: azul_core::task::OptionDuration::None,
            interval: azul_core::task::OptionDuration::Some(Duration::System(SystemTimeDiff::from_millis(interval_ms))),
            timeout: azul_core::task::OptionDuration::None,
            callback: TimerCallback::create(cursor_blink_timer_callback),
        }
    }

    /// Check if a node is contenteditable (internal version using NodeId)
    fn is_node_contenteditable_internal(&self, dom_id: DomId, node_id: NodeId) -> bool {
        use crate::solver3::getters::is_node_contenteditable;

        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return false;
        };

        is_node_contenteditable(&layout_result.styled_dom, node_id)
    }

    /// Check if a node is contenteditable with W3C-conformant inheritance.
    ///
    /// This traverses up the DOM tree to check if the node or any ancestor
    /// has `contenteditable="true"` set, respecting `contenteditable="false"`
    /// to stop inheritance.
    fn is_node_contenteditable_inherited_internal(&self, dom_id: DomId, node_id: NodeId) -> bool {
        use crate::solver3::getters::is_node_contenteditable_inherited;

        let Some(layout_result) = self.layout_results.get(&dom_id) else {
            return false;
        };

        is_node_contenteditable_inherited(&layout_result.styled_dom, node_id)
    }

    /// Handle focus change for cursor blink timer management (W3C "flag and defer" pattern)
    ///
    /// This method implements the W3C focus/selection model:
    /// 1. Focus change is handled immediately (timer start/stop)
    /// 2. Cursor initialization is DEFERRED until after layout (via flag)
    ///
    /// The cursor is NOT initialized here because text layout may not be available
    /// during focus event handling. Instead, we set a flag that is consumed by
    /// `finalize_pending_focus_changes()` after the layout pass.
    ///
    /// # Parameters
    ///
    /// * `new_focus` - The newly focused node (None if focus is being cleared)
    /// * `current_window_state` - Current window state for timer creation
    ///
    /// # Returns
    ///
    /// A `CursorBlinkTimerAction` indicating what timer action the platform
    /// layer should take.
    pub fn handle_focus_change_for_cursor_blink(
        &mut self,
        new_focus: Option<azul_core::dom::DomNodeId>,
        current_window_state: &FullWindowState,
    ) -> CursorBlinkTimerAction {
        // Check if the new focus is on a contenteditable element
        // Use the inherited check for W3C conformance
        let contenteditable_info = match new_focus {
            Some(focus_node) => {
                if let Some(node_id) = focus_node.node.into_crate_internal() {
                    // Check if this node or any ancestor is contenteditable
                    if self.is_node_contenteditable_inherited_internal(focus_node.dom, node_id) {
                        // Find the text node where the cursor should be placed
                        let text_node_id = self.find_last_text_child(focus_node.dom, node_id)
                            .unwrap_or(node_id);
                        Some((focus_node.dom, node_id, text_node_id))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            None => None,
        };

        // Determine the action based on current state and new focus
        let timer_was_active = self.cursor_manager.is_blink_timer_active();

        if let Some((dom_id, container_node_id, text_node_id)) = contenteditable_info {

            // W3C "flag and defer" pattern:
            // Set flag for cursor initialization AFTER layout pass
            self.focus_manager.set_pending_contenteditable_focus(
                dom_id,
                container_node_id,
                text_node_id,
            );

            // Make cursor visible and record current time (even before actual initialization)
            let now = azul_core::task::Instant::now();
            self.cursor_manager.reset_blink_on_input(now);
            self.cursor_manager.set_blink_timer_active(true);

            if !timer_was_active {
                // Need to start the timer
                let timer = self.create_cursor_blink_timer(current_window_state);
                return CursorBlinkTimerAction::Start(timer);
            } else {
                // Timer already active, just continue
                return CursorBlinkTimerAction::NoChange;
            }
        } else {
            // Focus is moving away from contenteditable or being cleared

            // Clear the cursor AND the pending focus flag
            self.cursor_manager.clear();
            self.focus_manager.clear_pending_contenteditable_focus();

            if timer_was_active {
                // Need to stop the timer
                self.cursor_manager.set_blink_timer_active(false);
                return CursorBlinkTimerAction::Stop;
            } else {
                return CursorBlinkTimerAction::NoChange;
            }
        }
    }

    /// Finalize pending focus changes after layout pass (W3C "flag and defer" pattern)
    ///
    /// This method should be called AFTER the layout pass completes. It checks if
    /// there's a pending contenteditable focus and initializes the cursor now that
    /// text layout information is available.
    ///
    /// # W3C Conformance
    ///
    /// In the W3C model:
    /// 1. Focus event fires during event handling (layout may not be ready)
    /// 2. Selection/cursor placement happens after layout is computed
    /// 3. The cursor is drawn at the position specified by the Selection
    ///
    /// This function implements step 2+3 by:
    /// - Checking the `cursor_needs_initialization` flag
    /// - Getting the (now available) text layout
    /// - Initializing the cursor at the correct position
    ///
    /// # Returns
    ///
    /// `true` if cursor was initialized, `false` if no pending focus or initialization failed.
    pub fn finalize_pending_focus_changes(&mut self) -> bool {
        // Take the pending focus info (this clears the flag)
        let pending = match self.focus_manager.take_pending_contenteditable_focus() {
            Some(p) => p,
            None => return false,
        };

        // Now we can safely get the text layout (layout pass has completed)
        let text_layout = self.get_inline_layout_for_node(pending.dom_id, pending.text_node_id).cloned();

        // Initialize cursor at end of text
        self.cursor_manager.initialize_cursor_at_end(
            pending.dom_id,
            pending.text_node_id,
            text_layout.as_ref(),
        )
    }

    /// Helper: Get inline layout for a node
    ///
    /// For text nodes that participate in an IFC, the inline layout is stored
    /// on the IFC root node (the block container), not on the text node itself.
    /// This method handles both cases:
    /// 1. The node has its own `inline_layout_result` (IFC root)
    /// 2. The node has `ifc_membership` pointing to the IFC root
    ///
    /// This is a thin wrapper around `LayoutTree::get_inline_layout_for_node`.
    pub fn get_inline_layout_for_node(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<&Arc<UnifiedLayout>> {
        let layout_result = self.layout_results.get(&dom_id)?;

        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&node_id)?;
        let layout_index = *layout_indices.first()?;

        // Use the centralized LayoutTree method that handles IFC membership
        layout_result.layout_tree.get_inline_layout_for_node(layout_index)
    }

    /// Helper: Move cursor using a movement function and return the new cursor if it changed
    pub fn move_cursor_in_node<F>(
        &self,
        dom_id: DomId,
        node_id: NodeId,
        movement_fn: F,
    ) -> Option<TextCursor>
    where
        F: FnOnce(&UnifiedLayout, &TextCursor) -> TextCursor,
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
    pub fn handle_cursor_movement(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        new_cursor: TextCursor,
        extend_selection: bool,
    ) {
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
                    SelectionRange {
                        start: new_cursor,
                        end: *old_cursor,
                    }
                } else {
                    // Moving forwards
                    SelectionRange {
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
    pub fn get_dom_ids(&self) -> DomIdVec {
        self.layout_results
            .keys()
            .copied()
            .collect::<Vec<_>>()
            .into()
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
        // time_into_fade is (elapsed / fade_delay) - 1.0, a dimensionless ratio
        // To convert to a ratio of fade_duration, multiply by (fade_delay / fade_duration)
        // giving (elapsed - fade_delay) / fade_duration
        let time_into_fade = time_since_activity.div(&fade_delay) - 1.0;
        let fade_progress = (time_into_fade * fade_delay.div(&fade_duration)).min(1.0);

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
        let mut events = Vec::new();
        let mut any_opacity_nonzero = false;
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

            // Track whether any scrollbar is actively fading (0 < opacity < 1).
            // We do NOT count fully-visible scrollbars (opacity == 1.0) because
            // those are driven by the scroll physics timer already. We only need
            // extra frames for the fade-out interpolation phase.
            if (vertical_opacity > 0.0 && vertical_opacity < 1.0)
                || (horizontal_opacity > 0.0 && horizontal_opacity < 1.0)
            {
                any_opacity_nonzero = true;
            }

            // Handle vertical scrollbar
            // IMPORTANT: Always pre-register the opacity key when the node needs a
            // vertical scrollbar, even if the current opacity is 0.  The display list
            // generator reads the key from the GPU cache to embed a PropertyBinding
            // in the ScrollBarStyled item.  If we only create the key when opacity > 0,
            // the first display list won't have the binding, and GPU-only scroll
            // updates (build_image_only_transaction) can never make the scrollbar
            // visible because WebRender doesn't know about the binding.
            if scrollbar_info.needs_vertical {
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
                // Remove if scrollbar no longer needed
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

            // Handle horizontal scrollbar (same logic as vertical above)
            if scrollbar_info.needs_horizontal {
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
                // Remove if scrollbar no longer needed
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

        // Signal to the platform render loop that more frames are needed
        // to complete the scrollbar fade animation. The caller should
        // schedule a redraw while this flag is true.
        if any_opacity_nonzero {
            gpu_state_manager.scrollbar_fade_active = true;
        } else {
            gpu_state_manager.scrollbar_fade_active = false;
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
                .map(|n| n.styled_node_state.clone())
                .unwrap_or_default();

            // Check if this node has scroll overflow
            let overflow_x = get_overflow_x(styled_dom, dom_node_id, &styled_node_state);
            let overflow_y = get_overflow_y(styled_dom, dom_node_id, &styled_node_state);

            let is_scrollable = overflow_x.is_scroll() || overflow_y.is_scroll();

            if !is_scrollable {
                continue;
            }

            // Generate stable scroll ID from node_data_fingerprint
            // Use a combined hash of the fingerprint fields to create a stable ID
            let scroll_id = {
                use std::hash::{Hash, Hasher, DefaultHasher};
                let mut h = DefaultHasher::new();
                node.node_data_fingerprint.hash(&mut h);
                h.finish()
            };

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
        let calc_pos = self.layout_cache.calculated_positions.get(layout_idx)?;

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
        let calc_pos = self.layout_cache.calculated_positions.get(layout_idx)?;

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
    /// ```text
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
            .get(layout_idx)
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
    /// Run a single timer callback and return raw changes + update.
    ///
    /// If the timer should terminate, a `RemoveTimer` change is appended.
    pub fn run_single_timer(
        &mut self,
        timer_id: usize,
        frame_start: Instant,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> (Vec<crate::callbacks::CallbackChange>, Update) {
        use crate::callbacks::{CallbackInfo, CallbackChange};

        let mut update = Update::DoNothing;
        let mut all_changes = Vec::new();
        let mut should_terminate = TerminateTimer::Continue;

        let current_scroll_states_nested = self.get_nested_scroll_states(DomId::ROOT_ID);

        let timer_exists = self.timers.contains_key(&TimerId { id: timer_id });
        let timer_node_id = self
            .timers
            .get(&TimerId { id: timer_id })
            .and_then(|t| t.node_id.into_option());

        if timer_exists {
            let hit_dom_node = match timer_node_id {
                Some(s) => s,
                None => DomNodeId {
                    dom: DomId::ROOT_ID,
                    node: NodeHierarchyItemId::from_crate_internal(None),
                },
            };
            let cursor_relative_to_item = OptionLogicalPosition::None;
            let cursor_in_viewport = OptionLogicalPosition::None;

            let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

            let timer_ctx = self
                .timers
                .get(&TimerId { id: timer_id })
                .map(|t| t.callback.ctx.clone())
                .unwrap_or(OptionRefAny::None);

            let ref_data = crate::callbacks::CallbackInfoRefData {
                layout_window: self,
                renderer_resources,
                previous_window_state,
                current_window_state,
                gl_context,
                current_scroll_manager: &current_scroll_states_nested,
                current_window_handle,
                system_callbacks,
                system_style,
                monitors: self.monitors.clone(),
                #[cfg(feature = "icu")]
                icu_localizer: self.icu_localizer.clone(),
                ctx: timer_ctx,
            };

            let callback_info = CallbackInfo::new(
                &ref_data,
                &callback_changes,
                hit_dom_node,
                cursor_relative_to_item,
                cursor_in_viewport,
            );

            let timer = self.timers.get_mut(&TimerId { id: timer_id }).unwrap();
            let tcr = timer.invoke(&callback_info, &system_callbacks.get_system_time_fn);

            update = tcr.should_update;
            should_terminate = tcr.should_terminate;

            all_changes = callback_changes
                .lock()
                .map(|mut guard| core::mem::take(&mut *guard))
                .unwrap_or_default();
        }

        if should_terminate == TerminateTimer::Terminate {
            all_changes.push(CallbackChange::RemoveTimer {
                timer_id: TimerId { id: timer_id },
            });
        }

        (all_changes, update)
    }

    #[cfg(feature = "std")]
    /// Run all thread writeback callbacks and return raw changes + update.
    pub fn run_all_threads(
        &mut self,
        data: &mut RefAny,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> (Vec<crate::callbacks::CallbackChange>, Update) {
        use std::collections::BTreeSet;

        use crate::{
            callbacks::{CallbackInfo, CallbackChange},
            thread::{OptionThreadReceiveMsg, ThreadReceiveMsg, ThreadWriteBackMsg},
        };

        let mut update = Update::DoNothing;
        let mut all_changes = Vec::new();

        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

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

            let (msg, writeback_data_ptr, is_finished) = {
                let thread_inner = &mut *match thread.ptr.lock().ok() {
                    Some(s) => s,
                    None => {
                        all_changes.push(CallbackChange::RemoveThread { thread_id });
                        continue;
                    }
                };

                let _ = thread_inner.sender_send(ThreadSendMsg::Tick);
                let recv = thread_inner.receiver_try_recv();
                let msg = match recv {
                    OptionThreadReceiveMsg::None => continue,
                    OptionThreadReceiveMsg::Some(s) => s,
                };

                let writeback_data_ptr: *mut RefAny = &mut thread_inner.writeback_data as *mut _;
                let is_finished = thread_inner.is_finished();

                (msg, writeback_data_ptr, is_finished)
            };

            let ThreadWriteBackMsg {
                refany: mut data_inner,
                callback,
            } = match msg {
                ThreadReceiveMsg::Update(update_screen) => {
                    update.max_self(update_screen);
                    continue;
                }
                ThreadReceiveMsg::WriteBack(t) => t,
            };

            let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

            let ref_data = crate::callbacks::CallbackInfoRefData {
                layout_window: self,
                renderer_resources,
                previous_window_state,
                current_window_state,
                gl_context,
                current_scroll_manager: &current_scroll_states,
                current_window_handle,
                system_callbacks,
                system_style: system_style.clone(),
                monitors: self.monitors.clone(),
                #[cfg(feature = "icu")]
                icu_localizer: self.icu_localizer.clone(),
                ctx: callback.ctx.clone(),
            };

            let callback_info = CallbackInfo::new(
                &ref_data,
                &callback_changes,
                hit_dom_node,
                cursor_relative_to_item,
                cursor_in_viewport,
            );

            let callback_update = (callback.cb)(
                unsafe { (*writeback_data_ptr).clone() },
                data_inner.clone(),
                callback_info,
            );
            update.max_self(callback_update);

            let collected_changes = callback_changes
                .lock()
                .map(|mut guard| core::mem::take(&mut *guard))
                .unwrap_or_default();

            all_changes.extend(collected_changes);

            if is_finished {
                all_changes.push(CallbackChange::RemoveThread { thread_id });
            }
        }

        (all_changes, update)
    }

    /// Invokes a single callback and returns the raw changes + update signal.
    ///
    /// Caller is responsible for processing each `CallbackChange` via
    /// `PlatformWindowV2::apply_user_change()`.
    pub fn invoke_single_callback(
        &mut self,
        callback: &mut Callback,
        data: &mut RefAny,
        current_window_handle: &RawWindowHandle,
        gl_context: &OptionGlContextPtr,
        system_style: std::sync::Arc<azul_css::system::SystemStyle>,
        system_callbacks: &ExternalSystemCallbacks,
        previous_window_state: &Option<FullWindowState>,
        current_window_state: &FullWindowState,
        renderer_resources: &RendererResources,
    ) -> (Vec<crate::callbacks::CallbackChange>, Update) {
        use crate::callbacks::{CallbackInfo, CallbackChange};

        let hit_dom_node = DomNodeId {
            dom: DomId::ROOT_ID,
            node: NodeHierarchyItemId::from_crate_internal(None),
        };

        let current_scroll_states = self.get_nested_scroll_states(DomId::ROOT_ID);

        let cursor_relative_to_item = OptionLogicalPosition::None;
        let cursor_in_viewport = match current_window_state.mouse_state.cursor_position.get_position() {
            Some(pos) => OptionLogicalPosition::Some(pos),
            None => OptionLogicalPosition::None,
        };

        // Create changes container for callback transaction system
        let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

        // Create reference data container
        let ref_data = crate::callbacks::CallbackInfoRefData {
            layout_window: self,
            renderer_resources,
            previous_window_state,
            current_window_state,
            gl_context,
            current_scroll_manager: &current_scroll_states,
            current_window_handle,
            system_callbacks,
            system_style,
            monitors: self.monitors.clone(),
            #[cfg(feature = "icu")]
            icu_localizer: self.icu_localizer.clone(),
            ctx: OptionRefAny::None,
        };

        let callback_info = CallbackInfo::new(
            &ref_data,
            &callback_changes,
            hit_dom_node,
            cursor_relative_to_item,
            cursor_in_viewport,
        );

        let update = (callback.cb)(data.clone(), callback_info);

        // Extract changes from the Arc<Mutex>
        let collected_changes = callback_changes
            .lock()
            .map(|mut guard| core::mem::take(&mut *guard))
            .unwrap_or_default();

        (collected_changes, update)
    }

    /// Set the system style for resolving system color keywords in CSS.
    ///
    /// This should be called during window initialization and whenever the system
    /// theme changes (dark/light mode switch, accent color change).
    ///
    /// The system style is used to resolve CSS system colors like `selection-background`,
    /// `selection-text`, `accent`, etc. If not set, hard-coded fallback values are used.
    pub fn set_system_style(&mut self, system_style: std::sync::Arc<azul_css::system::SystemStyle>) {
        #[cfg(feature = "icu")]
        {
            self.icu_localizer = crate::icu::IcuLocalizerHandle::from_system_language(&system_style.language);
        }
        self.system_style = Some(system_style);
    }
}

// --- ICU4X Internationalization API ---

#[cfg(feature = "icu")]
impl LayoutWindow {
    /// Initialize the ICU localizer with the system's detected language.
    ///
    /// This should be called during window initialization, passing the language
    /// from `SystemStyle::language`.
    ///
    /// # Arguments
    /// * `locale` - The BCP 47 language tag (e.g., "en-US", "de-DE")
    pub fn set_icu_locale(&mut self, locale: &str) {
        self.icu_localizer.set_locale(locale);
    }

    /// Initialize the ICU localizer from a SystemStyle.
    ///
    /// This is a convenience method that extracts the language from the system style.
    pub fn init_icu_from_system_style(&mut self, system_style: &azul_css::system::SystemStyle) {
        self.icu_localizer = IcuLocalizerHandle::from_system_language(&system_style.language);
    }

    /// Get a clone of the ICU localizer handle.
    ///
    /// This can be used to perform locale-aware formatting outside of callbacks.
    pub fn get_icu_localizer(&self) -> IcuLocalizerHandle {
        self.icu_localizer.clone()
    }

    /// Load additional ICU locale data from a binary blob.
    ///
    /// The blob should be generated using `icu4x-datagen` with the `--format blob` flag.
    /// This allows supporting locales that aren't compiled into the binary.
    pub fn load_icu_data_blob(&mut self, data: Vec<u8>) -> bool {
        self.icu_localizer.load_data_blob(&data)
    }
}

#[cfg(test)]
mod tests {
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

    // ScrollManager and VirtualizedView Integration Tests

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
        let fc_cache = FcFontCache::default();
        let mut window = LayoutWindow::new(fc_cache).unwrap();

        let dom_id = DomId::ROOT_ID;
        let node_id = NodeId::new(0);

        // Create a scroll input
        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 });

        let scroll_input = crate::managers::scroll_state::ScrollInput {
            dom_id,
            node_id,
            delta: LogicalPosition::new(10.0, 20.0),
            timestamp: now.clone(),
            source: crate::managers::scroll_state::ScrollInputSource::WheelDiscrete,
        };

        let should_start_timer = window
            .scroll_manager
            .record_scroll_input(scroll_input);

        // record_scroll_input should return true (timer was not running)
        assert!(should_start_timer);
    }

    #[test]
    fn test_scroll_manager_programmatic_scroll() {
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

    /// Find the last text child node of a given node.
    ///
    /// For contenteditable elements, the text is usually in a child Text node,
    /// not the contenteditable div itself. This function finds the last Text node
    /// so the cursor defaults to the end position.
    fn find_last_text_child(&self, dom_id: DomId, parent_node_id: NodeId) -> Option<NodeId> {
        let layout_result = self.layout_results.get(&dom_id)?;
        let styled_dom = &layout_result.styled_dom;
        let node_data_container = styled_dom.node_data.as_container();
        let hierarchy_container = styled_dom.node_hierarchy.as_container();

        // Check if parent itself is a text node
        let parent_type = node_data_container[parent_node_id].get_node_type();
        if matches!(parent_type, NodeType::Text(_)) {
            return Some(parent_node_id);
        }

        // Find the last text child by iterating through all children
        let parent_item = &hierarchy_container[parent_node_id];
        let mut last_text_child: Option<NodeId> = None;
        let mut current_child = parent_item.first_child_id(parent_node_id);
        while let Some(child_id) = current_child {
            let child_type = node_data_container[child_id].get_node_type();
            if matches!(child_type, NodeType::Text(_)) {
                last_text_child = Some(child_id);
            }
            current_child = hierarchy_container[child_id].next_sibling_id();
        }

        last_text_child
    }

    /// Checks if a node has text content.
    fn node_has_text_content(&self, styled_dom: &StyledDom, node_id: NodeId) -> bool {
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
        let node_state = &styled_dom.styled_nodes.as_container()[node_id].styled_node_state;
        crate::solver3::getters::is_text_selectable(styled_dom, node_id, node_state)
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
                        // Check BOTH: the contenteditable boolean field AND the attribute
                        // NodeData has a direct `contenteditable: bool` field that should be
                        // checked in addition to the attribute for robustness
                        let is_contenteditable = styled_node.is_contenteditable()
                            || styled_node.attributes.as_ref().iter().any(|attr| {
                                matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
                            });

                        if is_contenteditable {
                            // Get inline layout for cursor positioning
                            // Clone the Arc to avoid borrow conflict
                            let inline_layout = self.get_inline_layout_for_node(dom_id, node_id).cloned();
                            if inline_layout.is_some() {
                                self.cursor_manager.initialize_cursor_at_end(
                                    dom_id,
                                    node_id,
                                    inline_layout.as_ref(),
                                );

                                // Scroll cursor into view if necessary
                                self.scroll_cursor_into_view_if_needed(dom_id, node_id, now);
                            }
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
                    affected_nodes.insert(
                        dom_node_id,
                        (vec![EventFilter::Focus(FocusEventFilter::TextInput)], false),
                    );
                }
            }

            AccessibilityAction::Collapse | AccessibilityAction::Expand => {
                // Map to corresponding On:: events
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
                    let start_cursor = self.byte_offset_to_cursor(
                        inline_layout.as_ref(),
                        selection.selection_start as u32,
                    );
                    let end_cursor = self.byte_offset_to_cursor(
                        inline_layout.as_ref(),
                        selection.selection_end as u32,
                    );

                    if let (Some(start), Some(end)) = (start_cursor, end_cursor) {
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
                                selections: vec![selection].into(),
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

        use crate::managers::text_input::TextInputSource;

        let mut affected_nodes = BTreeMap::new();

        if text_input.is_empty() {
            return affected_nodes;
        }

        // Get focused node
        let focused_node = match self.focus_manager.get_focused_node().copied() {
            Some(node) => node,
            None => {
                return affected_nodes;
            }
        };

        let node_id = match focused_node.node.into_crate_internal() {
            Some(id) => id,
            None => {
                return affected_nodes;
            }
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
            None => {
                return Vec::new();
            }
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

        // Check BOTH: the contenteditable boolean field AND the attribute
        // NodeData has a direct `contenteditable: bool` field that should be
        // checked in addition to the attribute for robustness
        let is_contenteditable = styled_node.is_contenteditable()
            || styled_node.attributes.as_ref().iter().any(|attr| {
                matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
            });

        if !is_contenteditable {
            self.text_input_manager.clear_changeset();
            return Vec::new();
        }

        // Get the current inline content from cache
        let content = self.get_text_before_textinput(dom_id, node_id);

        // Get current cursor/selection from cursor manager
        let current_selection = if let Some(cursor) = self.cursor_manager.get_cursor() {
            vec![Selection::Cursor(cursor.clone())]
        } else {
            // No cursor - create one at start of text
            vec![Selection::Cursor(TextCursor {
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
            if let Selection::Cursor(c) = sel {
                Some(c.clone())
            } else {
                None
            }
        });
        let old_selection_range = current_selection.first().and_then(|sel| {
            if let Selection::Range(r) = sel {
                Some(*r)
            } else {
                None
            }
        });

        let pre_state = crate::managers::undo_redo::NodeStateSnapshot {
            node_id: azul_core::id::NodeId::new(node_id.index()),
            text_content: old_text.into(),
            cursor_position: old_cursor.into(),
            selection_range: old_selection_range.into(),
            #[cfg(feature = "std")]
            timestamp: azul_core::task::Instant::System(std::time::Instant::now().into()),
            #[cfg(not(feature = "std"))]
            timestamp: azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 }),
        };

        // Apply the edit using text3::edit - this is a pure function
        use crate::text3::edit::{edit_text, TextEdit};
        let text_edit = TextEdit::Insert(changeset.inserted_text.as_str().to_string());
        let (new_content, new_selections) = edit_text(&content, &current_selection, &text_edit);

        // Update the cursor/selection in cursor manager
        // This happens lazily, only when we actually apply the changes
        if let Some(Selection::Cursor(new_cursor)) = new_selections.first() {
            self.cursor_manager
                .move_cursor_to(new_cursor.clone(), dom_id, node_id);
        }

        // Update the text cache with the new inline content
        self.update_text_cache_after_edit(dom_id, node_id, new_content);

        // Record this operation to the undo/redo manager AFTER successful mutation

        use crate::managers::changeset::{TextChangeset, TextOpInsertText, TextOperation};

        // Get the new cursor position after edit
        let new_cursor = new_selections
            .first()
            .and_then(|sel| {
                if let Selection::Cursor(c) = sel {
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
            operation: TextOperation::InsertText(TextOpInsertText {
                text: changeset.inserted_text.clone(),
                position: old_cursor_pos,
                new_cursor,
            }),
            #[cfg(feature = "std")]
            timestamp: azul_core::task::Instant::System(std::time::Instant::now().into()),
            #[cfg(not(feature = "std"))]
            timestamp: azul_core::task::Instant::Tick(azul_core::task::SystemTick { tick_counter: 0 }),
        };
        self.undo_redo_manager
            .record_operation(undo_changeset, pre_state);

        // Clear the changeset now that it's been applied
        self.text_input_manager.clear_changeset();

        // Return nodes that need dirty marking
        let dirty_nodes = self.determine_dirty_text_nodes(dom_id, node_id);
        dirty_nodes
    }

    /// Determine which nodes need to be marked dirty after a text edit
    ///
    /// Returns the edited node + its parent (if it exists)
    fn determine_dirty_text_nodes(
        &self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Vec<azul_core::dom::DomNodeId> {
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
    pub fn get_last_text_changeset(&self) -> Option<&PendingTextEdit> {
        self.text_input_manager.get_pending_changeset()
    }

    /// Get the current inline content (text before text input is applied)
    ///
    /// This is a query function that retrieves the current text state from the node.
    /// Returns InlineContent vector if the node has text.
    ///
    /// # Implementation Note
    /// This function FIRST checks `dirty_text_nodes` for optimistic state (edits not yet
    /// committed to StyledDom), then falls back to the StyledDom. This is critical for
    /// correct text input handling - without this, each keystroke would read stale state.
    pub fn get_text_before_textinput(&self, dom_id: DomId, node_id: NodeId) -> Vec<InlineContent> {
        // CRITICAL FIX: Check dirty_text_nodes first!
        // If the node has been edited since last full layout, its most up-to-date
        // content is in dirty_text_nodes, NOT in the StyledDom.
        // Without this check, every keystroke reads the ORIGINAL text instead of
        // the accumulated edits, causing bugs like double-input and wrong node affected.
        if let Some(dirty_node) = self.dirty_text_nodes.get(&(dom_id, node_id)) {
            return dirty_node.content.clone();
        }

        // Fallback to committed state from StyledDom
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
                    source_node_id: Some(node_id),
                })]
            }
            NodeType::Div | NodeType::Body | NodeType::VirtualizedView => {
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
    ) -> alloc::sync::Arc<StyleProperties> {
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
    ) -> Vec<InlineContent> {
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
    pub fn extract_text_from_inline_content(&self, content: &[InlineContent]) -> String {
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
                InlineContent::Tab { .. } => {
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
    ///
    /// This function:
    /// 1. Stores the new content in `dirty_text_nodes` for tracking
    /// 2. Re-runs the text3 layout pipeline (create_logical_items -> reorder -> shape -> fragment)
    /// 3. Updates the inline_layout_result on the IFC root node in the layout tree
    pub fn update_text_cache_after_edit(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        new_inline_content: Vec<InlineContent>,
    ) {
        use crate::solver3::layout_tree::CachedInlineLayout;

        // 1. Store the new content in dirty_text_nodes for tracking
        let cursor = self.cursor_manager.get_cursor().cloned();
        self.dirty_text_nodes.insert(
            (dom_id, node_id),
            DirtyTextNode {
                content: new_inline_content.clone(),
                cursor,
                needs_ancestor_relayout: false, // Will be set if size changes
            },
        );

        // 2. Get the cached constraints from the existing inline layout result
        // We need to find the IFC root node and extract its constraints
        let constraints = {
            let layout_result = match self.layout_results.get(&dom_id) {
                Some(r) => r,
                None => {
                    return;
                }
            };

            let layout_node = match layout_result.layout_tree.get(node_id.index()) {
                Some(n) => n,
                None => {
                    return;
                }
            };

            let cached_layout = match &layout_node.inline_layout_result {
                Some(c) => c,
                None => {
                    return;
                }
            };

            match &cached_layout.constraints {
                Some(c) => c.clone(),
                None => {
                    return;
                }
            }
        };

        // 3. Re-run the text3 layout pipeline
        let new_layout = self.relayout_text_node_internal(&new_inline_content, &constraints);

        let Some(new_layout) = new_layout else {
            return;
        };

        // 4. Update the layout cache with the new layout
        // Find the IFC root node in the layout tree and update its inline_layout_result
        if let Some(layout_result) = self.layout_results.get_mut(&dom_id) {
            // Find the node in the layout tree
            if let Some(layout_node) = layout_result.layout_tree.get_mut(node_id.index()) {
                // Check if size changed (needs repaint)
                let old_size = layout_node.used_size;
                let new_bounds = new_layout.bounds();
                let new_size = Some(LogicalSize {
                    width: new_bounds.width,
                    height: new_bounds.height,
                });

                // Check if we need to propagate layout shift
                if let (Some(old), Some(new)) = (old_size, new_size) {
                    if (old.height - new.height).abs() > 0.5 || (old.width - new.width).abs() > 0.5 {
                        // Mark that ancestor relayout is needed
                        if let Some(dirty_node) = self.dirty_text_nodes.get_mut(&(dom_id, node_id)) {
                            dirty_node.needs_ancestor_relayout = true;
                        }
                    }
                }

                // Update the inline layout result with the new layout but preserve constraints
                layout_node.inline_layout_result = Some(CachedInlineLayout::new_with_constraints(
                    Arc::new(new_layout),
                    constraints.available_width,
                    false, // No floats in quick relayout
                    constraints,
                ));
            }
        }

        // CRITICAL: Regenerate the display list after updating the inline layout.
        // Without this, the old display list (with old text glyphs) is sent to WebRender,
        // so the screen still shows the old text even though the layout tree is updated.
        self.regenerate_display_list_for_dom(dom_id);
    }

    /// Regenerate the display list for a specific DOM from the current layout tree.
    ///
    /// This is the critical missing piece for text input: after `update_text_cache_after_edit`
    /// updates the `inline_layout_result` on layout tree nodes, the `DomLayoutResult.display_list`
    /// must be regenerated. Otherwise, `generate_frame()` sends the OLD display list to WebRender
    /// and the screen shows stale text.
    ///
    /// This method creates a temporary `LayoutContext` from the existing `LayoutWindow` state
    /// and calls `generate_display_list` on the already-computed layout tree and positions.
    fn regenerate_display_list_for_dom(&mut self, dom_id: DomId) {
        use crate::solver3::{
            display_list::generate_display_list,
            LayoutContext,
        };

        // Get all the data we need from the layout result
        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => { return; }
        };

        let tree = &layout_result.layout_tree;
        let calculated_positions = &layout_result.calculated_positions;
        let scroll_ids = &layout_result.scroll_ids;
        let styled_dom = &layout_result.styled_dom;
        let viewport = layout_result.viewport;

        // Get scroll offsets from scroll manager
        let scroll_offsets = self.scroll_manager.get_scroll_states_for_dom(dom_id);

        // Get GPU cache for this DOM
        let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id).clone();

        // Get cursor state for display list generation
        let cursor_is_visible = self.cursor_manager.should_draw_cursor();
        let cursor_location = self.cursor_manager.get_cursor_location().and_then(|loc| {
            self.cursor_manager.get_cursor().map(|cursor| {
                (loc.dom_id, loc.node_id, cursor.clone())
            })
        });

        // Build a temporary LayoutContext with all the state we need
        let mut counter_values = BTreeMap::new();
        let mut debug_messages: Option<Vec<LayoutDebugMessage>> = None;
        let cache_map = std::mem::take(&mut self.layout_cache.cache_map);

        let mut ctx = LayoutContext {
            styled_dom,
            font_manager: &self.font_manager,
            selections: &self.selection_manager.selections,
            text_selections: &self.selection_manager.text_selections,
            debug_messages: &mut debug_messages,
            counters: &mut counter_values,
            viewport_size: viewport.size,
            fragmentation_context: None,
            cursor_is_visible,
            cursor_location,
            cache_map,
            system_style: self.system_style.clone(),
            get_system_time_fn: azul_core::task::GetSystemTimeCallback {
                cb: azul_core::task::get_system_time_libstd,
            },
        };

        // Generate the new display list from the existing layout tree
        let new_display_list = generate_display_list(
            &mut ctx,
            tree,
            calculated_positions,
            &scroll_offsets,
            scroll_ids,
            Some(&gpu_cache),
            &self.renderer_resources,
            self.id_namespace,
            dom_id,
        );

        // Restore the cache_map back to layout_cache
        self.layout_cache.cache_map = std::mem::take(&mut ctx.cache_map);

        match new_display_list {
            Ok(display_list) => {
                if let Some(layout_result) = self.layout_results.get_mut(&dom_id) {
                    layout_result.display_list = display_list;
                }
            }
            Err(_e) => { }
        }
    }

    /// Internal helper to re-run the text3 layout pipeline on new content
    fn relayout_text_node_internal(
        &self,
        content: &[InlineContent],
        constraints: &UnifiedConstraints,
    ) -> Option<UnifiedLayout> {
        use crate::text3::cache::{
            create_logical_items, perform_fragment_layout, reorder_logical_items,
            shape_visual_items, BidiDirection, BreakCursor,
        };

        // Stage 1: Create logical items from InlineContent
        let logical_items = create_logical_items(content, &[], &mut None);

        if logical_items.is_empty() {
            // Empty text - return empty layout
            return Some(UnifiedLayout {
                items: Vec::new(),
                overflow: crate::text3::cache::OverflowInfo::default(),
            });
        }

        // Stage 2: Bidi reordering
        let base_direction = constraints.direction.unwrap_or(BidiDirection::Ltr);
        let visual_items = reorder_logical_items(&logical_items, base_direction, &mut None).ok()?;

        // Stage 3: Shape text (resolve fonts, create glyphs)
        let loaded_fonts = self.font_manager.get_loaded_fonts();
        let shaped_items = shape_visual_items(
            &visual_items,
            self.font_manager.get_font_chain_cache(),
            &self.font_manager.fc_cache,
            &loaded_fonts,
            &mut None,
        )
        .ok()?;

        // Stage 4: Fragment layout (line breaking, positioning)
        let mut cursor = BreakCursor::new(&shaped_items);
        perform_fragment_layout(&mut cursor, &logical_items, constraints, &mut None, &loaded_fonts)
            .ok()
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
        let position = layout_result.calculated_positions.get(node_id.index())?;

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
        text_layout: &UnifiedLayout,
        byte_offset: u32,
    ) -> Option<TextCursor> {
        // Handle offset 0 as special case (start of text)
        if byte_offset == 0 {
            // Find first cluster in items
            for item in &text_layout.items {
                if let ShapedItem::Cluster(cluster) = &item.item {
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
            if let ShapedItem::Cluster(cluster) = &item.item {
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
            if let ShapedItem::Cluster(cluster) = &item.item {
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
    ) -> Option<alloc::sync::Arc<UnifiedLayout>> {
        // Get the layout tree from cache
        let layout_tree = self.layout_cache.tree.as_ref()?;

        // Find the layout node corresponding to the DOM node
        let layout_node = layout_tree
            .nodes
            .iter()
            .find(|node| node.dom_node_id == Some(node_id))?;

        // Return the inline layout result
        layout_node
            .inline_layout_result
            .as_ref()
            .map(|c| c.clone_layout())
    }

    /// Sync cursor from CursorManager to SelectionManager for rendering
    ///
    /// The renderer expects cursor and selection data from the SelectionManager,
    /// but we manage the cursor separately in the CursorManager for better separation
    /// of concerns. This function syncs the cursor state so it can be rendered.
    ///
    /// This should be called whenever the cursor changes.
    pub fn sync_cursor_to_selection_manager(&mut self) {
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
                    selections: vec![selection].into(),
                    node_id: dom_node_id,
                };

                // Set selection in SelectionManager
                self.selection_manager
                    .set_selection(location.dom_id, selection_state);
            }
        }
        // NOTE: We intentionally do NOT clear selections when there's no cursor.
        // Text selections from mouse clicks should persist independently of cursor state.
        // Only explicit user actions (clicking elsewhere, Escape, etc.) should clear selections.
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
    /// 1. Use HoverManager's hit test to find hit nodes
    /// 2. Find the IFC layout via `inline_layout_result` (IFC root) or `ifc_membership` (text node)
    /// 3. Use point_relative_to_item for local cursor position
    /// 4. Hit-test the text layout to get logical cursor
    /// 5. Apply appropriate selection based on click count
    /// 6. Update SelectionManager with new selection
    ///
    /// ## IFC Architecture
    /// Text nodes don't store `inline_layout_result` directly. Instead:
    /// - IFC root nodes (e.g., `<p>`) have `inline_layout_result` with the complete text layout
    /// - Text nodes have `ifc_membership` pointing back to their IFC root
    /// - This allows efficient lookup without iterating all nodes
    ///
    /// ## Parameters
    /// * `position` - Click position in logical coordinates (for click count tracking)
    /// * `time_ms` - Current time in milliseconds (for multi-click detection)
    ///
    /// ## Returns
    /// * `Option<Vec<DomNodeId>>` - Affected nodes that need re-rendering, None if click didn't hit text
    pub fn process_mouse_click_for_selection(
        &mut self,
        position: azul_core::geom::LogicalPosition,
        time_ms: u64,
    ) -> Option<Vec<azul_core::dom::DomNodeId>> {
        use crate::managers::hover::InputPointId;
        use crate::text3::selection::{select_paragraph_at_cursor, select_word_at_cursor};

        // found_selection stores: (dom_id, ifc_root_node_id, selection_range, local_pos)
        // IMPORTANT: We always store the IFC root NodeId, not the text node NodeId,
        // because selections are rendered via inline_layout_result which lives on the IFC root.
        let mut found_selection: Option<(DomId, NodeId, SelectionRange, azul_core::geom::LogicalPosition)> = None;

        // Try to get hit test from HoverManager first (fast path, uses WebRender's point_relative_to_item)
        if let Some(hit_test) = self.hover_manager.get_current(&InputPointId::Mouse) {
            // Iterate through hit nodes from the HoverManager
            for (dom_id, hit) in &hit_test.hovered_nodes {
                let layout_result = match self.layout_results.get(dom_id) {
                    Some(lr) => lr,
                    None => continue,
                };
                // Use layout tree from layout_result, not layout_cache
                let tree = &layout_result.layout_tree;

                // Sort by DOM depth (deepest first) to prefer specific text nodes over containers.
                // We count the actual number of parents to determine DOM depth properly.
                // Secondary sort by NodeId for deterministic ordering within the same depth.
                let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
                let get_dom_depth = |node_id: &NodeId| -> usize {
                    let mut depth = 0;
                    let mut current = *node_id;
                    while let Some(parent) = node_hierarchy.get(current).and_then(|h| h.parent_id()) {
                        depth += 1;
                        current = parent;
                    }
                    depth
                };

                let mut sorted_hits: Vec<_> = hit.regular_hit_test_nodes.iter().collect();
                sorted_hits.sort_by(|(a_id, _), (b_id, _)| {
                    let depth_a = get_dom_depth(a_id);
                    let depth_b = get_dom_depth(b_id);
                    // Higher depth = deeper in DOM = should come first
                    // Then sort by NodeId for deterministic order within same depth
                    depth_b.cmp(&depth_a).then_with(|| a_id.index().cmp(&b_id.index()))
                });

                for (node_id, hit_item) in sorted_hits {
                    // Check if text is selectable
                    if !self.is_text_selectable(&layout_result.styled_dom, *node_id) {
                        continue;
                    }

                    // Find the layout node for this DOM node
                    let layout_node_idx = tree.nodes.iter().position(|n| n.dom_node_id == Some(*node_id));
                    let layout_node_idx = match layout_node_idx {
                        Some(idx) => idx,
                        None => continue,
                    };
                    let layout_node = &tree.nodes[layout_node_idx];

                    // Get the IFC layout and IFC root NodeId
                    // Selection must be stored on the IFC root, not on text nodes
                    let (cached_layout, ifc_root_node_id) = if let Some(ref cached) = layout_node.inline_layout_result {
                        // This node IS an IFC root - use its own NodeId
                        (cached, *node_id)
                    } else if let Some(ref membership) = layout_node.ifc_membership {
                        // This node participates in an IFC - get layout and NodeId from IFC root
                        match tree.nodes.get(membership.ifc_root_layout_index) {
                            Some(ifc_root) => match (ifc_root.inline_layout_result.as_ref(), ifc_root.dom_node_id) {
                                (Some(cached), Some(root_dom_id)) => (cached, root_dom_id),
                                _ => continue,
                            },
                            None => continue,
                        }
                    } else {
                        // No IFC involvement - not a text node
                        continue;
                    };

                    let layout = &cached_layout.layout;

                    // Use point_relative_to_item - this is the local position within the hit node
                    // provided by WebRender's hit test
                    let local_pos = hit_item.point_relative_to_item;

                    // Hit-test the cursor in this text layout
                    if let Some(cursor) = layout.hittest_cursor(local_pos) {
                        // Store selection with IFC root NodeId, not the hit text node
                        found_selection = Some((*dom_id, ifc_root_node_id, SelectionRange {
                            start: cursor.clone(),
                            end: cursor,
                        }, local_pos));
                        break;
                    }
                }

                if found_selection.is_some() {
                    break;
                }
            }
        }

        // Fallback: If HoverManager has no hit test (e.g., debug server),
        // search through IFC roots using global position
        if found_selection.is_none() {
            for (dom_id, layout_result) in &self.layout_results {
                // Use the layout tree from layout_result, not layout_cache
                // layout_cache.tree is for the root DOM only; layout_result.layout_tree
                // is the correct tree for each DOM (including virtualized views)
                let tree = &layout_result.layout_tree;

                // Only iterate IFC roots (nodes with inline_layout_result)
                for (node_idx, layout_node) in tree.nodes.iter().enumerate() {
                    let cached_layout = match layout_node.inline_layout_result.as_ref() {
                        Some(c) => c,
                        None => continue, // Skip non-IFC-root nodes
                    };

                    let node_id = match layout_node.dom_node_id {
                        Some(n) => n,
                        None => continue,
                    };

                    // Check if text is selectable
                    if !self.is_text_selectable(&layout_result.styled_dom, node_id) {
                        continue;
                    }

                    // Get the node's absolute position
                    // Use layout_result.calculated_positions for the correct DOM
                    let node_pos = layout_result.calculated_positions
            .get(node_idx)
                        .copied()
                        .unwrap_or_default();

                    // Check if position is within node bounds
                    let node_size = layout_node.used_size.unwrap_or_else(|| {
                        let bounds = cached_layout.layout.bounds();
                        azul_core::geom::LogicalSize::new(bounds.width, bounds.height)
                    });

                    if position.x < node_pos.x || position.x > node_pos.x + node_size.width ||
                       position.y < node_pos.y || position.y > node_pos.y + node_size.height {
                        continue;
                    }

                    // Convert global position to node-local coordinates
                    let local_pos = azul_core::geom::LogicalPosition {
                        x: position.x - node_pos.x,
                        y: position.y - node_pos.y,
                    };

                    let layout = &cached_layout.layout;

                    // Hit-test the cursor in this text layout
                    if let Some(cursor) = layout.hittest_cursor(local_pos) {
                        found_selection = Some((*dom_id, node_id, SelectionRange {
                            start: cursor.clone(),
                            end: cursor,
                        }, local_pos));
                        break;
                    }
                }

                if found_selection.is_some() {
                    break;
                }
            }
        }

        let (dom_id, ifc_root_node_id, initial_range, _local_pos) = found_selection?;

        // Create DomNodeId for click state tracking - use IFC root's NodeId
        // Selection state is keyed by IFC root because that's where inline_layout_result lives
        let node_hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(ifc_root_node_id));
        let dom_node_id = azul_core::dom::DomNodeId {
            dom: dom_id,
            node: node_hierarchy_id,
        };

        // Update click count to determine selection type
        let click_count = self
            .selection_manager
            .update_click_count(dom_node_id, position, time_ms);

        // Get the text layout again for word/paragraph selection
        let final_range = if click_count > 1 {
            // Use layout_results for the correct DOM's tree
            let layout_result = self.layout_results.get(&dom_id)?;
            let tree = &layout_result.layout_tree;

            // Find layout node - ifc_root_node_id is always the IFC root, so it has inline_layout_result
            let layout_node = tree.nodes.iter().find(|n| n.dom_node_id == Some(ifc_root_node_id))?;
            let cached_layout = layout_node.inline_layout_result.as_ref()?;
            let layout = &cached_layout.layout;

            match click_count {
                2 => select_word_at_cursor(&initial_range.start, layout.as_ref())
                    .unwrap_or(initial_range),
                3 => select_paragraph_at_cursor(&initial_range.start, layout.as_ref())
                    .unwrap_or(initial_range),
                _ => initial_range,
            }
        } else {
            initial_range
        };

        // Clear existing selections and set new one using the NEW anchor/focus model
        // First, get the cursor bounds for the anchor
        let char_bounds = {
            let layout_result = self.layout_results.get(&dom_id)?;
            let tree = &layout_result.layout_tree;
            let layout_node = tree.nodes.iter().find(|n| n.dom_node_id == Some(ifc_root_node_id))?;
            let cached_layout = layout_node.inline_layout_result.as_ref()?;
            cached_layout.layout.get_cursor_rect(&final_range.start)
                .unwrap_or(azul_core::geom::LogicalRect {
                    origin: position,
                    size: azul_core::geom::LogicalSize { width: 1.0, height: 16.0 },
                })
        };

        // Clear any existing text selection for this DOM
        self.selection_manager.clear_text_selection(&dom_id);

        // Start a new selection with the anchor at the clicked position
        self.selection_manager.start_selection(
            dom_id,
            ifc_root_node_id,
            final_range.start,
            char_bounds,
            position,
        );

        // Also update the legacy selection state for backward compatibility with rendering
        self.selection_manager.clear_selection(&dom_id);

        let state = SelectionState {
            selections: vec![Selection::Range(final_range)].into(),
            node_id: dom_node_id,
        };

        self.selection_manager.set_selection(dom_id, state);

        // CRITICAL FIX 1: Set focus on the clicked node
        // Without this, clicking on a contenteditable element shows a cursor but
        // text input doesn't work because record_text_input() checks focus_manager.get_focused_node()
        // and returns early if there's no focus.
        //
        // Check if the node OR ANY ANCESTOR is contenteditable before setting focus
        // The contenteditable attribute is typically on a parent div, not on the IFC root or text node
        let is_contenteditable = self.layout_results.get(&dom_id)
            .map(|lr| {
                let node_hierarchy = lr.styled_dom.node_hierarchy.as_container();
                let node_data = lr.styled_dom.node_data.as_ref();

                // Walk up the DOM tree to check if any ancestor has contenteditable
                let mut current_node = Some(ifc_root_node_id);
                while let Some(node_id) = current_node {
                    if let Some(styled_node) = node_data.get(node_id.index()) {
                        // Check BOTH: the contenteditable boolean field AND the attribute
                        // NodeData has a direct `contenteditable: bool` field that should be
                        // checked in addition to the attribute for robustness
                        if styled_node.is_contenteditable() {
                            return true;
                        }

                        // Also check the attribute (for backwards compatibility)
                        let has_contenteditable_attr = styled_node.attributes.as_ref().iter().any(|attr| {
                            matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
                        });
                        if has_contenteditable_attr {
                            return true;
                        }
                    }
                    // Move to parent
                    current_node = node_hierarchy.get(node_id).and_then(|h| h.parent_id());
                }
                false
            })
            .unwrap_or(false);

        // W3C conformance: contenteditable elements are implicitly focusable
        if is_contenteditable {
            self.focus_manager.set_focused_node(Some(dom_node_id));
        }

        // CRITICAL FIX 2: Initialize the CursorManager with the clicked position
        // Without this, clicking on a contenteditable element sets focus (blue outline)
        // but the text cursor doesn't appear because CursorManager is never told where to draw it.
        let now = azul_core::task::Instant::now();
        self.cursor_manager.move_cursor_to(
            final_range.start.clone(),
            dom_id,
            ifc_root_node_id,
        );
        // Reset the blink timer so the cursor is immediately visible
        self.cursor_manager.reset_blink_on_input(now);
        self.cursor_manager.set_blink_timer_active(true);

        // Return the affected node for dirty tracking
        Some(vec![dom_node_id])
    }

    /// Process mouse drag for text selection extension.
    ///
    /// This method handles drag-to-select by extending the selection from
    /// the anchor (mousedown position) to the current focus (drag position).
    ///
    /// Uses the anchor/focus model:
    /// - Anchor is fixed at the initial click position (set by process_mouse_click_for_selection)
    /// - Focus moves with the mouse during drag
    /// - Affected nodes between anchor and focus are computed in DOM order
    ///
    /// ## Parameters
    /// * `start_position` - Initial click position in logical coordinates (unused, anchor is stored)
    /// * `current_position` - Current mouse position in logical coordinates
    ///
    /// ## Returns
    /// * `Option<Vec<DomNodeId>>` - Affected nodes that need re-rendering
    pub fn process_mouse_drag_for_selection(
        &mut self,
        _start_position: azul_core::geom::LogicalPosition,
        current_position: azul_core::geom::LogicalPosition,
    ) -> Option<Vec<azul_core::dom::DomNodeId>> {
        use crate::managers::hover::InputPointId;

        // Find which DOM has an active text selection with an anchor
        let dom_id = self.selection_manager.get_all_text_selections()
            .keys()
            .next()
            .copied()?;

        // Get the existing anchor from the text selection
        let anchor = {
            let text_selection = self.selection_manager.get_text_selection(&dom_id)?;
            text_selection.anchor.clone()
        };

        // Get the current hit test from HoverManager
        let hit_test = self.hover_manager.get_current(&InputPointId::Mouse)?;

        // Find the focus position (current cursor under mouse)
        let mut focus_info: Option<(NodeId, TextCursor, azul_core::geom::LogicalPosition)> = None;

        for (hit_dom_id, hit) in &hit_test.hovered_nodes {
            if *hit_dom_id != dom_id {
                continue;
            }

            let layout_result = match self.layout_results.get(hit_dom_id) {
                Some(lr) => lr,
                None => continue,
            };
            let tree = &layout_result.layout_tree;

            for (node_id, hit_item) in &hit.regular_hit_test_nodes {
                if !self.is_text_selectable(&layout_result.styled_dom, *node_id) {
                    continue;
                }

                let layout_node_idx = tree.nodes.iter().position(|n| n.dom_node_id == Some(*node_id));
                let layout_node_idx = match layout_node_idx {
                    Some(idx) => idx,
                    None => continue,
                };
                let layout_node = &tree.nodes[layout_node_idx];

                // Get the IFC layout and IFC root NodeId
                let (cached_layout, ifc_root_node_id) = if let Some(ref cached) = layout_node.inline_layout_result {
                    (cached, *node_id)
                } else if let Some(ref membership) = layout_node.ifc_membership {
                    match tree.nodes.get(membership.ifc_root_layout_index) {
                        Some(ifc_root) => match (ifc_root.inline_layout_result.as_ref(), ifc_root.dom_node_id) {
                            (Some(cached), Some(root_dom_id)) => (cached, root_dom_id),
                            _ => continue,
                        },
                        None => continue,
                    }
                } else {
                    continue;
                };

                let local_pos = hit_item.point_relative_to_item;

                if let Some(cursor) = cached_layout.layout.hittest_cursor(local_pos) {
                    focus_info = Some((ifc_root_node_id, cursor, current_position));
                    break;
                }
            }

            if focus_info.is_some() {
                break;
            }
        }

        let (focus_ifc_root, focus_cursor, focus_mouse_pos) = focus_info?;

        // Compute affected nodes between anchor and focus
        let layout_result = self.layout_results.get(&dom_id)?;
        let hierarchy = &layout_result.styled_dom.node_hierarchy;

        // Determine document order
        let is_forward = if anchor.ifc_root_node_id == focus_ifc_root {
            // Same IFC - compare cursors
            anchor.cursor <= focus_cursor
        } else {
            // Different IFCs - use document order
            is_before_in_document_order(hierarchy, anchor.ifc_root_node_id, focus_ifc_root)
        };

        let (start_node, end_node) = if is_forward {
            (anchor.ifc_root_node_id, focus_ifc_root)
        } else {
            (focus_ifc_root, anchor.ifc_root_node_id)
        };

        // Collect all IFC roots between start and end
        let nodes_in_range = collect_nodes_in_document_order(hierarchy, start_node, end_node);

        // Build the affected_nodes map with SelectionRanges for each IFC root
        let mut affected_nodes_map = std::collections::BTreeMap::new();
        let tree = &layout_result.layout_tree;

        for node_id in &nodes_in_range {
            // Check if this node is an IFC root (has inline_layout_result)
            let layout_node = tree.nodes.iter().find(|n| n.dom_node_id == Some(*node_id));
            let layout_node = match layout_node {
                Some(ln) if ln.inline_layout_result.is_some() => ln,
                _ => continue, // Skip non-IFC-root nodes
            };

            let cached_layout = layout_node.inline_layout_result.as_ref()?;
            let layout = &cached_layout.layout;

            let range = if *node_id == anchor.ifc_root_node_id && *node_id == focus_ifc_root {
                // Both anchor and focus in same IFC
                SelectionRange {
                    start: if is_forward { anchor.cursor } else { focus_cursor },
                    end: if is_forward { focus_cursor } else { anchor.cursor },
                }
            } else if *node_id == anchor.ifc_root_node_id {
                // Anchor node - select from anchor to end (if forward) or start to anchor (if backward)
                if is_forward {
                    let end_cursor = layout.get_last_cluster_cursor()
                        .unwrap_or(anchor.cursor);
                    SelectionRange { start: anchor.cursor, end: end_cursor }
                } else {
                    let start_cursor = layout.get_first_cluster_cursor()
                        .unwrap_or(anchor.cursor);
                    SelectionRange { start: start_cursor, end: anchor.cursor }
                }
            } else if *node_id == focus_ifc_root {
                // Focus node - select from start to focus (if forward) or focus to end (if backward)
                if is_forward {
                    let start_cursor = layout.get_first_cluster_cursor()
                        .unwrap_or(focus_cursor);
                    SelectionRange { start: start_cursor, end: focus_cursor }
                } else {
                    let end_cursor = layout.get_last_cluster_cursor()
                        .unwrap_or(focus_cursor);
                    SelectionRange { start: focus_cursor, end: end_cursor }
                }
            } else {
                // Middle node - fully selected
                let start_cursor = layout.get_first_cluster_cursor()?;
                let end_cursor = layout.get_last_cluster_cursor()?;
                SelectionRange { start: start_cursor, end: end_cursor }
            };

            affected_nodes_map.insert(*node_id, range);
        }

        // Update the text selection with new focus and affected nodes
        // This does NOT clear the anchor!
        self.selection_manager.update_selection_focus(
            &dom_id,
            focus_ifc_root,
            focus_cursor,
            focus_mouse_pos,
            affected_nodes_map.clone(),
            is_forward,
        );

        // Also update the legacy selection state for backward compatibility with rendering
        // For now, we just update the anchor's IFC root with the visible range
        if let Some(anchor_range) = affected_nodes_map.get(&anchor.ifc_root_node_id) {
            let node_hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(anchor.ifc_root_node_id));
            let dom_node_id = azul_core::dom::DomNodeId {
                dom: dom_id,
                node: node_hierarchy_id,
            };

            let state = SelectionState {
                selections: vec![Selection::Range(*anchor_range)].into(),
                node_id: dom_node_id,
            };
            self.selection_manager.set_selection(dom_id, state);
        }

        // Return affected nodes for dirty tracking
        let affected_dom_nodes: Vec<azul_core::dom::DomNodeId> = affected_nodes_map.keys()
            .map(|node_id| azul_core::dom::DomNodeId {
                dom: dom_id,
                node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)),
            })
            .collect();

        if affected_dom_nodes.is_empty() {
            None
        } else {
            Some(affected_dom_nodes)
        }
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
            let state = SelectionState {
                selections: vec![Selection::Range(SelectionRange {
                    start: cursor.clone(),
                    end: cursor,
                })]
                .into(),
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

                                    // Extract font family from font stack
                                    let default_font = FontSelector::default();
                                    let first_font = style.font_stack.first_selector()
                                        .unwrap_or(&default_font);
                                    let font_family: OptionString =
                                        Some(AzString::from(first_font.family.as_str())).into();

                                    // Check if bold/italic from font selector
                                    use rust_fontconfig::FcWeight;
                                    let is_bold = matches!(
                                        first_font.weight,
                                        FcWeight::Bold | FcWeight::ExtraBold | FcWeight::Black
                                    );
                                    let is_italic = matches!(
                                        first_font.style,
                                        FontStyle::Italic | FontStyle::Oblique
                                    );

                                    styled_runs.push(StyledTextRun {
                                        text: cluster.text.clone().into(),
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
                plain_text: plain_text.into(),
                styled_runs: styled_runs.into(),
            })
        }
    }

    /// Process image callback updates from callback changes
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
                let position = match layout_result.calculated_positions.get(layout_index) {
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
                                    // Try get_image_callback_mut first (requires exclusive access)
                                    let callback_result = img_ref.get_image_callback_mut();
                                    
                                    if callback_result.is_none() {
                                        // The ImageRef has multiple copies (Arc refcount > 1),
                                        // so get_image_callback_mut returns None. Fall back to
                                        // read-only access + clone to invoke the callback.
                                        match img_ref.get_data() {
                                            azul_core::resources::DecodedImage::Callback(core_callback) => {
                                                if core_callback.callback.cb == 0 {
                                                    None
                                                } else {
                                                    let callback = RenderImageCallback::from_core(&core_callback.callback);
                                                    let refany_clone = core_callback.refany.clone();
                                                    use std::panic;
                                                    let result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
                                                        (callback.cb)(refany_clone, gl_callback_info)
                                                    }));
                                                    result.ok()
                                                }
                                            }
                                            _ => None,
                                        }
                                    } else {
                                        callback_result.map(|core_callback| {
                                            // Convert from CoreImageCallback (cb: usize) to
                                            // RenderImageCallback (cb: fn pointer)
                                            let callback =
                                                RenderImageCallback::from_core(&core_callback.callback);
                                            (callback.cb)(
                                                core_callback.refany.clone(),
                                                gl_callback_info,
                                            )
                                        })
                                    }
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

    /// Check if a scrolled node is a VirtualizedView that needs re-invocation. If so,
    /// queue it in `pending_virtualized_view_updates` for processing before the next frame.
    ///
    /// This is the bridge between the scroll system and the VirtualizedView lifecycle:
    ///   ScrollTo → scroll_manager.scroll_to() → check_and_queue_virtualized_view_reinvoke()
    ///
    /// Returns `true` if a VirtualizedView update was queued (caller should trigger a
    /// display list rebuild instead of a lightweight repaint).
    pub fn check_and_queue_virtualized_view_reinvoke(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
    ) -> bool {
        // Get the VirtualizedView's current layout bounds (needed for check_reinvoke)
        let bounds = match Self::get_virtualized_view_bounds_from_layout(
            &self.layout_results,
            dom_id,
            node_id,
        ) {
            Some(b) => b,
            None => return false, // Not a VirtualizedView or no layout info
        };

        // Ask the VirtualizedViewManager whether this VirtualizedView needs re-invocation
        let reason = self.virtualized_view_manager.check_reinvoke(
            dom_id, node_id, &self.scroll_manager, bounds,
        );

        if reason.is_some() {
            // Queue the VirtualizedView for re-invocation in the next render pass
            self.pending_virtualized_view_updates
                .entry(dom_id)
                .or_insert_with(FastBTreeSet::new)
                .insert(node_id);
            true
        } else {
            false
        }
    }

    /// Process VirtualizedView updates requested by callbacks
    ///
    /// This method handles manual VirtualizedView re-rendering triggered by `trigger_virtualized_view_rerender()`.
    /// It invokes the VirtualizedView callback with `DomRecreated` reason and performs layout on the
    /// returned DOM, then submits a new display list to WebRender for that pipeline.
    ///
    /// # Arguments
    ///
    /// * `vviews_to_update` - Map of DomId -> Set of NodeIds that need re-rendering
    /// * `window_state` - Current window state
    /// * `renderer_resources` - Renderer resources
    /// * `system_callbacks` - External system callbacks
    ///
    /// # Returns
    ///
    /// Vector of (DomId, NodeId) tuples for VirtualizedViews that were successfully updated
    pub fn process_virtualized_view_updates(
        &mut self,
        vviews_to_update: &BTreeMap<DomId, FastBTreeSet<NodeId>>,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
    ) -> Vec<(DomId, NodeId)> {
        let mut updated_vviews = Vec::new();

        for (dom_id, node_ids) in vviews_to_update {
            for node_id in node_ids {
                // Extract virtualized view bounds from layout result
                let bounds = match Self::get_virtualized_view_bounds_from_layout(
                    &self.layout_results,
                    *dom_id,
                    *node_id,
                ) {
                    Some(b) => b,
                    None => continue,
                };

                // Force re-invocation by clearing the "was_invoked" flag
                self.virtualized_view_manager.force_reinvoke(*dom_id, *node_id);

                // Invoke the VirtualizedView callback
                if let Some(_child_dom_id) = self.invoke_virtualized_view_callback(
                    *dom_id,
                    *node_id,
                    bounds,
                    window_state,
                    renderer_resources,
                    system_callbacks,
                    &mut None,
                ) {
                    updated_vviews.push((*dom_id, *node_id));
                }
            }
        }

        updated_vviews
    }

    /// Queue VirtualizedView updates to be processed in the next frame
    ///
    /// This is called after callbacks to store the vviews_to_update from callback changes
    pub fn queue_virtualized_view_updates(
        &mut self,
        vviews_to_update: BTreeMap<DomId, FastBTreeSet<NodeId>>,
    ) {
        for (dom_id, node_ids) in vviews_to_update {
            self.pending_virtualized_view_updates
                .entry(dom_id)
                .or_insert_with(FastBTreeSet::new)
                .extend(node_ids);
        }
    }

    /// Process and clear pending VirtualizedView updates
    ///
    /// This is called during frame generation to re-render updated VirtualizedViews
    pub fn process_pending_virtualized_view_updates(
        &mut self,
        window_state: &FullWindowState,
        renderer_resources: &RendererResources,
        system_callbacks: &ExternalSystemCallbacks,
    ) -> Vec<(DomId, NodeId)> {
        if self.pending_virtualized_view_updates.is_empty() {
            return Vec::new();
        }

        // Take ownership of pending updates
        let vviews_to_update = core::mem::take(&mut self.pending_virtualized_view_updates);

        // Process them
        self.process_virtualized_view_updates(
            &vviews_to_update,
            window_state,
            renderer_resources,
            system_callbacks,
        )
    }

    /// Helper: Extract VirtualizedView bounds from layout results
    ///
    /// Returns None if the node is not a VirtualizedView or doesn't have layout info
    fn get_virtualized_view_bounds_from_layout(
        layout_results: &BTreeMap<DomId, DomLayoutResult>,
        dom_id: DomId,
        node_id: NodeId,
    ) -> Option<LogicalRect> {
        let layout_result = layout_results.get(&dom_id)?;

        // Check if this is a VirtualizedView node
        let node_data_container = layout_result.styled_dom.node_data.as_container();
        let node_data = node_data_container.get(node_id)?;

        if !matches!(node_data.get_node_type(), NodeType::VirtualizedView) {
            return None;
        }

        // Get layout indices
        let layout_indices = layout_result.layout_tree.dom_to_layout.get(&node_id)?;
        if layout_indices.is_empty() {
            return None;
        }

        let layout_index = layout_indices[0];

        // Get position
        let position = *layout_result.calculated_positions.get(layout_index)?;

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

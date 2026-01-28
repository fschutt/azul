
# CRITICAL BUGS TO FIX

## Bug 1: TEXT INPUT STOPPED WORKING (HIGHEST PRIORITY)
- Clicking on contenteditable element now positions cursor (visible blinking cursor)
- But typing does NOTHING - no text appears
- This worked before the current diff changes
- The diff added:
  - Focus setting on click in process_mouse_click_for_selection
  - dirty_text_nodes check in get_text_before_textinput
  - scroll_selection_into_view after text edit
  - Removed duplicate record_text_input from macOS handle_key_down

Expected flow:
1. Click -> process_mouse_click_for_selection -> sets focus + cursor
2. Type -> macOS insertText: -> handle_text_input -> record_text_input
3. record_text_input checks focus_manager.get_focused_node()
4. If focused, records changeset, returns affected nodes
5. Callback fires, text appears

Current behavior:
- Step 1 works (cursor appears)
- Step 2-5: Nothing happens, no text appears

## Bug 2: Border/Scrollbar Offset (~10px detached)
- The border around elements is rendered ~10px away from the actual element
- The scrollbar at bottom is also offset, not at the window edge
- This suggests incorrect position calculation during display list building
- Probably related to padding/margin not being accounted for in border rect calculation

## Bug 3: white-space: nowrap Ignored
- CSS sets white-space: nowrap on .editor
- But text still wraps to multiple lines
- The text layout ignores the white-space constraint

## Bug 4: Missing Glyphs
- Some characters render as white boxes instead of glyphs
- Font loading or glyph caching issue
- Possibly related to font-family: monospace fallback

## Bug 5: Scrollbar Sizing/Position Wrong
- Scrollbar track size should be (width - 2*button_width), not just width
- Scrollbar should be hidden when overflow: auto and content fits
- Scrollbar is painted at wrong Y position (should be at bottom of scroll container)


# CURRENT GIT DIFF

```diff
diff --git a/dll/src/desktop/shell2/common/event_v2.rs b/dll/src/desktop/shell2/common/event_v2.rs
index 49911bb9..e6b95f64 100644
--- a/dll/src/desktop/shell2/common/event_v2.rs
+++ b/dll/src/desktop/shell2/common/event_v2.rs
@@ -2919,6 +2919,14 @@ pub trait PlatformWindowV2 {
                 if !dirty_nodes.is_empty() {
                     println!("[process_callback_result_v2] Applied text changeset, {} dirty nodes", dirty_nodes.len());
                     event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
+                    
+                    // CRITICAL FIX: Scroll cursor into view after text edit
+                    // Without this, typing at the end of a long text doesn't scroll
+                    // the view to keep the cursor visible.
+                    layout_window.scroll_selection_into_view(
+                        azul_layout::window::SelectionScrollType::Cursor,
+                        azul_layout::window::ScrollMode::Instant,
+                    );
                 }
             }
         }
diff --git a/dll/src/desktop/shell2/macos/events.rs b/dll/src/desktop/shell2/macos/events.rs
index c0d1260c..35c553e9 100644
--- a/dll/src/desktop/shell2/macos/events.rs
+++ b/dll/src/desktop/shell2/macos/events.rs
@@ -373,13 +373,13 @@ impl MacOSWindow {
         // Update keyboard state with keycode
         self.update_keyboard_state(key_code, modifiers, true);
 
-        // Record text input if character is available
-        if let Some(ch) = character {
-            if let Some(layout_window) = self.get_layout_window_mut() {
-                let text_input = ch.to_string();
-                layout_window.record_text_input(&text_input);
-            }
-        }
+        // NOTE: We do NOT call record_text_input here!
+        // On macOS, text input comes through the IME system via insertText:
+        // which calls handle_text_input(). Calling record_text_input here
+        // would cause DOUBLE text input because both keyDown and insertText
+        // are called by the system for normal key presses.
+        // The character from keyDown is only used for VirtualKeyDown events,
+        // not for text input.
 
         // V2 system will detect VirtualKeyDown and TextInput from state diff
         let result = self.process_window_events_recursive_v2(0);
diff --git a/layout/src/window.rs b/layout/src/window.rs
index 0016befd..49d9b05c 100644
--- a/layout/src/window.rs
+++ b/layout/src/window.rs
@@ -5482,10 +5482,22 @@ impl LayoutWindow {
     /// Returns InlineContent vector if the node has text.
     ///
     /// # Implementation Note
-    /// This function currently reconstructs InlineContent from the styled DOM.
-    /// A future optimization would be to cache the InlineContent during layout
-    /// and retrieve it directly from the text cache.
+    /// This function FIRST checks `dirty_text_nodes` for optimistic state (edits not yet
+    /// committed to StyledDom), then falls back to the StyledDom. This is critical for
+    /// correct text input handling - without this, each keystroke would read stale state.
     pub fn get_text_before_textinput(&self, dom_id: DomId, node_id: NodeId) -> Vec<InlineContent> {
+        // CRITICAL FIX: Check dirty_text_nodes first!
+        // If the node has been edited since last full layout, its most up-to-date
+        // content is in dirty_text_nodes, NOT in the StyledDom.
+        // Without this check, every keystroke reads the ORIGINAL text instead of
+        // the accumulated edits, causing bugs like double-input and wrong node affected.
+        if let Some(dirty_node) = self.dirty_text_nodes.get(&(dom_id, node_id)) {
+            #[cfg(feature = "std")]
+            eprintln!("[get_text_before_textinput] Using dirty_text_nodes content for ({:?}, {:?})", dom_id, node_id);
+            return dirty_node.content.clone();
+        }
+
+        // Fallback to committed state from StyledDom
         // Get the layout result for this DOM
         let layout_result = match self.layout_results.get(&dom_id) {
             Some(lr) => lr,
@@ -6486,6 +6498,43 @@ impl LayoutWindow {
         
         self.selection_manager.set_selection(dom_id, state);
 
+        // CRITICAL FIX 1: Set focus on the clicked node
+        // Without this, clicking on a contenteditable element shows a cursor but
+        // text input doesn't work because record_text_input() checks focus_manager.get_focused_node()
+        // and returns early if there's no focus.
+        //
+        // Check if the node is contenteditable before setting focus
+        let is_contenteditable = self.layout_results.get(&dom_id)
+            .and_then(|lr| lr.styled_dom.node_data.as_ref().get(ifc_root_node_id.index()))
+            .map(|styled_node| {
+                styled_node.attributes.as_ref().iter().any(|attr| {
+                    matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
+                })
+            })
+            .unwrap_or(false);
+        
+        if is_contenteditable {
+            self.focus_manager.set_focused_node(Some(dom_node_id));
+            #[cfg(feature = "std")]
+            eprintln!("[DEBUG] Set focus on contenteditable node {:?}", ifc_root_node_id);
+        }
+
+        // CRITICAL FIX 2: Initialize the CursorManager with the clicked position
+        // Without this, clicking on a contenteditable element sets focus (blue outline)
+        // but the text cursor doesn't appear because CursorManager is never told where to draw it.
+        let now = azul_core::task::Instant::now();
+        self.cursor_manager.move_cursor_to(
+            final_range.start.clone(),
+            dom_id,
+            ifc_root_node_id,
+        );
+        // Reset the blink timer so the cursor is immediately visible
+        self.cursor_manager.reset_blink_on_input(now);
+        self.cursor_manager.set_blink_timer_active(true);
+        
+        #[cfg(feature = "std")]
+        eprintln!("[DEBUG] Initialized cursor at {:?} for node {:?}", final_range.start, ifc_root_node_id);
+
         // Return the affected node for dirty tracking
         Some(vec![dom_node_id])
     }
diff --git a/tests/e2e/contenteditable.c b/tests/e2e/contenteditable.c
index c76d5ba4..3e9e6b7d 100644
--- a/tests/e2e/contenteditable.c
+++ b/tests/e2e/contenteditable.c
@@ -53,39 +53,10 @@ AzUpdate on_text_input(AzRefAny data, AzCallbackInfo info) {
         return AzUpdate_DoNothing;
     }
     
-    // Get the text changeset from the callback info
-    AzOptionPendingTextEdit changeset = AzCallbackInfo_getTextChangeset(&info);
-    
-    if (changeset.Some.tag == AzOptionPendingTextEdit_Tag_Some) {
-        AzPendingTextEdit* edit = &changeset.Some.payload;
-        
-        // Print the changeset for debugging
-        printf("[TextInput] Changeset received:\n");
-        printf("  inserted_text: '%.*s'\n", 
-               (int)edit->inserted_text.vec.len, 
-               (const char*)edit->inserted_text.vec.ptr);
-        printf("  old_text: '%.*s' (len=%zu)\n", 
-               (int)(edit->old_text.vec.len > 50 ? 50 : edit->old_text.vec.len),
-               (const char*)edit->old_text.vec.ptr,
-               edit->old_text.vec.len);
-        
-        // Append the inserted text to our data model
-        // For single-line, we just append to the existing text
-        size_t current_len = strlen(ref.ptr->single_line_text);
-        size_t insert_len = edit->inserted_text.vec.len;
-        
-        if (current_len + insert_len < sizeof(ref.ptr->single_line_text) - 1) {
-            memcpy(ref.ptr->single_line_text + current_len, 
-                   edit->inserted_text.vec.ptr, 
-                   insert_len);
-            ref.ptr->single_line_text[current_len + insert_len] = '\0';
-            printf("  Updated single_line_text: '%s'\n", ref.ptr->single_line_text);
-        }
-        
-        ref.ptr->text_change_count++;
-    } else {
-        printf("[TextInput] No changeset available\n");
-    }
+    // Just count the text input event - the framework handles the actual text update
+    // The contenteditable system uses its internal state for visual updates
+    ref.ptr->text_change_count++;
+    printf("[TextInput] Event received (count: %d)\n", ref.ptr->text_change_count);
     
     ContentEditableDataRefMut_delete(&ref);
     

```

# SOURCE FILES

## layout/src/window.rs
// Main window logic - text input, focus, cursor management
// 7238 lines

```rust
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
    callbacks::{FocusTarget, HidpiAdjustedBounds, IFrameCallbackReason, Update},
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
    window::{CursorPosition, RawWindowHandle, RendererType},
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
    // The actual state modification happens in apply_callback_changes
    
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
    pub css_properties_changed: BTreeMap<DomId, BTreeMap<NodeId, CssPropertyVec>>,
    /// Scroll position changes from callbacks
    pub nodes_scrolled: BTreeMap<DomId, BTreeMap<NodeHierarchyItemId, LogicalPosition>>,
    /// Modified window state
    pub modified_window_state: FullWindowState,
    /// Queued window states to apply in sequence (for simulating clicks, etc.)
    /// Each state will trigger separate event processing to detect state changes.
    pub queued_window_states: Vec<FullWindowState>,
    /// Hit test update requested at this position (for Debug API)
    /// When set, the shell layer should perform a hit test update before processing events
    pub hit_test_update_requested: Option<LogicalPosition>,
    /// Text input events triggered by CreateTextInput
    /// These need to be processed by the recursive event loop to invoke user callbacks
    pub text_input_triggered: Vec<(azul_core::dom::DomNodeId, Vec<azul_core::events::EventFilter>)>,
}

/// A window-level layout manager that encapsulates all layout state and caching.
///
/// This struct owns the layout and text caches, and provides methods dir_to:
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
    /// Tracks which nodes have been edited since last full layout.
    /// Key: (DomId, NodeId of IFC root)
    /// Value: The edited inline content that should be used for relayout
    dirty_text_nodes: BTreeMap<(DomId, NodeId), DirtyTextNode>,
    /// Pending IFrame updates from callbacks (processed in next frame)
    /// Map of DomId -> Set of NodeIds that need re-rendering
    pub pending_iframe_updates: BTreeMap<DomId, FastBTreeSet<NodeId>>,
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
            dirty_text_nodes: BTreeMap::new(),
            pending_iframe_updates: BTreeMap::new(),
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
            dirty_text_nodes: BTreeMap::new(),
            pending_iframe_updates: BTreeMap::new(),
            #[cfg(feature = "icu")]
            icu_localizer: IcuLocalizerHandle::default(),
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
            eprintln!("[layout_and_generate_display_list] Layout FAILED: {:?}", e);
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
            register_embedded_fonts_from_styled_dom(&styled_dom, &self.font_manager);

            // Step 1: Resolve font chains (cached by FontChainKey)
            let chains = collect_and_resolve_font_chains(&styled_dom, &self.font_manager.fc_cache);
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
            .map(|result| std::mem::replace(&mut result.display_list, DisplayList::default()))
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
        let callback_data = iframe_node.refany.clone();

        // Invoke the user's IFrame callback
        let callback_return = (iframe_node.callback.cb)(callback_data, callback_info);

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
                    let mut empty_dom = Dom::create_div();
                    let empty_css = Css::empty();
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
        let position = layout_result.calculated_positions.get(&layout_index)?;
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

        // Get the actual tag_id from styled_nodes (matches what get_tag_id in display_list.rs uses)
        let styled_nodes = layout_result.styled_dom.styled_nodes.as_container();
        let tag_id = styled_nodes.get(nid)?.tag_id.into_option()?.inner;

        // Search the display list for a HitTestArea with matching tag
        // Note: tag is now (u64, u16) tuple where tag.0 is the TagId.inner
        for item in &layout_result.display_list.items {
            if let DisplayListItem::HitTestArea { bounds, tag } = item {
                if tag.0 == tag_id && bounds.size.width > 0.0 && bounds.size.height > 0.0 {
                    return Some(*bounds);
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
    
    /// Scroll the active text cursor into view within its scrollable container
    ///
    /// This finds the focused contenteditable node, gets the cursor rectangle,
    /// and scrolls any scrollable ancestor to ensure the cursor is visible.
    pub fn scroll_active_cursor_into_view(&mut self, result: &mut CallbackChangeResult) {
        use crate::managers::scroll_into_view;
        
        // Get the focused node
        let focused_node = match self.focus_manager.get_focused_node() {
            Some(node) => *node,
            None => return,
        };
        
        let Some(node_id_internal) = focused_node.node.into_crate_internal() else {
            return;
        };
        
        // Check if node is contenteditable
        if !self.is_node_contenteditable_internal(focused_node.dom, node_id_internal) {
            return;
        }
        
        // Get the cursor location
        let cursor_location = match self.cursor_manager.get_cursor_location() {
            Some(loc) if loc.dom_id == focused_node.dom && loc.node_id == node_id_internal => loc,
            _ => return,
        };
        
        // Get the cursor position
        let cursor = match self.cursor_manager.get_cursor() {
            Some(c) => c.clone(),
            None => return,
        };
        
        // Get the inline layout to find the cursor rectangle
        let layout = match self.get_inline_layout_for_node(focused_node.dom, node_id_internal) {
            Some(l) => l,
            None => return,
        };
        
        // Get cursor rectangle (node-local coordinates)
        let cursor_rect = match layout.get_cursor_rect(&cursor) {
            Some(r) => r,
            None => return,
        };
        
        // Use scroll_into_view to scroll the cursor rect into view
        let now = azul_core::task::Instant::now();
        let options = scroll_into_view::ScrollIntoViewOptions::nearest();
        
        // Calculate scroll adjustments
        let adjustments = scroll_into_view::scroll_rect_into_view(
            cursor_rect,
            focused_node.dom,
            node_id_internal,
            &self.layout_results,
            &mut self.scroll_manager,
            options,
            now,
        );
        
        // Record the scroll changes
        for adj in adjustments {
            let current_pos = self.scroll_manager
                .get_current_offset(adj.scroll_container_dom_id, adj.scroll_container_node_id)
                .unwrap_or(LogicalPosition::zero());
            
            let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(adj.scroll_container_node_id));
            result
                .nodes_scrolled
                .entry(adj.scroll_container_dom_id)
                .or_insert_with(BTreeMap::new)
                .insert(hierarchy_id, current_pos);
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
                CallbackChange::QueueWindowStateSequence { states } => {
                    // Queue the states to be processed in sequence.
                    // The first state is applied immediately, subsequent states
                    // are stored for processing in future frames.
                    result.queued_window_states.extend(states);
                }
                CallbackChange::CreateNewWindow { options } => {
                    result.windows_created.push(options);
                }
                CallbackChange::CloseWindow => {
                    // Set the close_requested flag to trigger window close
                    result.modified_window_state.flags.close_requested = true;
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
                CallbackChange::ChangeNodeText { node_id, text } => {
                    let dom_id = node_id.dom;
                    let internal_node_id = match node_id.node.into_crate_internal() {
                        Some(id) => id,
                        None => continue,
                    };
                    result
                        .words_changed
                        .entry(dom_id)
                        .or_insert_with(BTreeMap::new)
                        .insert(internal_node_id, text);
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
                CallbackChange::ScrollIntoView { node_id, options } => {
                    // Use the scroll_into_view module to calculate and apply scroll adjustments
                    use crate::managers::scroll_into_view;
                    let now = azul_core::task::Instant::now();
                    let adjustments = scroll_into_view::scroll_node_into_view(
                        node_id,
                        &self.layout_results,
                        &mut self.scroll_manager,
                        options,
                        now,
                    );
                    // Record the scroll changes in nodes_scrolled
                    // The scroll_manager was already updated by scroll_node_into_view,
                    // but we need to report the new absolute positions for event processing
                    for adj in adjustments {
                        // Get the current scroll position from scroll_manager (now updated)
                        let current_pos = self.scroll_manager
                            .get_current_offset(adj.scroll_container_dom_id, adj.scroll_container_node_id)
                            .unwrap_or(LogicalPosition::zero());
                        
                        let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(adj.scroll_container_node_id));
                        result
                            .nodes_scrolled
                            .entry(adj.scroll_container_dom_id)
                            .or_insert_with(BTreeMap::new)
                            .insert(hierarchy_id, current_pos);
                    }
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
                    let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(node_id));
                    let dom_node_id = DomNodeId {
                        dom: dom_id,
                        node: hierarchy_id,
                    };

                    match selection {
                        Selection::Cursor(cursor) => {
                            self.cursor_manager.move_cursor_to(cursor, dom_id, node_id);
                            self.selection_manager.clear_all();
                        }
                        Selection::Range(range) => {
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
                            let doc_start_cursor = TextCursor {
                                cluster_id: first_cluster.source_cluster_id,
                                affinity: CursorAffinity::Leading,
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
                            let doc_end_cursor = TextCursor {
                                cluster_id: last_cluster.source_cluster_id,
                                affinity: CursorAffinity::Trailing,
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
                CallbackChange::RequestHitTestUpdate { position } => {
                    // Mark that a hit test update is requested
                    // This will be processed by the shell layer which has access to WebRender
                    result.hit_test_update_requested = Some(position);
                }
                CallbackChange::ProcessTextSelectionClick { position, time_ms } => {
                    // Process text selection click at position
                    // This is used by the Debug API to trigger text selection directly
                    // The selection update will cause the display list to be regenerated
                    let _ = self.process_mouse_click_for_selection(position, time_ms);
                }
                CallbackChange::SetCursorVisibility { visible: _ } => {
                    // Timer callback sets visibility - check if we should blink or stay solid
                    let now = azul_core::task::Instant::now();
                    if self.cursor_manager.should_blink(&now) {
                        // Enough time has passed since last input - toggle visibility
                        self.cursor_manager.toggle_visibility();
                    } else {
                        // User is actively typing - keep cursor visible
                        self.cursor_manager.set_visibility(true);
                    }
                }
                CallbackChange::ResetCursorBlink => {
                    // Reset cursor blink state on user input
                    let now = azul_core::task::Instant::now();
                    self.cursor_manager.reset_blink_on_input(now);
                }
                CallbackChange::StartCursorBlinkTimer => {
                    // Start the cursor blink timer if not already active
                    if !self.cursor_manager.is_blink_timer_active() {
                        let timer = self.create_cursor_blink_timer(current_window_state);
                        result.timers.insert(azul_core::task::CURSOR_BLINK_TIMER_ID, timer);
                        self.cursor_manager.set_blink_timer_active(true);
                    }
                }
                CallbackChange::StopCursorBlinkTimer => {
                    // Stop the cursor blink timer
                    if self.cursor_manager.is_blink_timer_active() {
                        result.timers_removed.insert(azul_core::task::CURSOR_BLINK_TIMER_ID);
                        self.cursor_manager.set_blink_timer_active(false);
                    }
                }
                CallbackChange::ScrollActiveCursorIntoView => {
                    // Scroll the active text cursor into view
                    self.scroll_active_cursor_into_view(&mut result);
                }
                CallbackChange::CreateTextInput { text } => {
                    // Create a synthetic text input event
                    // This simulates receiving text input from the OS
                    println!("[CreateTextInput] Processing text: '{}'", text.as_str());
                    
                    // Process the text input - this records the changeset in TextInputManager
                    let affected_nodes = self.process_text_input(text.as_str());
                    println!("[CreateTextInput] process_text_input returned {} affected nodes", affected_nodes.len());
                    
                    // Mark that we need to trigger text input callbacks
                    // The affected nodes and their events will be processed by the recursive event loop
                    for (node, (events, _)) in affected_nodes {
                        result.text_input_triggered.push((node, events));
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
    /// 
    /// For text nodes that participate in an IFC, the inline layout is stored
    /// on the IFC root node (the block container), not on the text node itself.
    /// This method handles both cases:
    /// 1. The node has its own `inline_layout_result` (IFC root)
    /// 2. The node has `ifc_membership` pointing to the IFC root
    ///
    /// This is a thin wrapper around `LayoutTree::get_inline_layout_for_node`.
    fn get_inline_layout_for_node(
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
    fn move_cursor_in_node<F>(
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
    fn handle_cursor_movement(
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
                .map(|n| n.styled_node_state.clone())
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
            hit_test_update_requested: None,
            queued_window_states: Vec::new(),
            text_input_triggered: Vec::new(),
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

            // Create changes container for callback transaction system
            // Uses Arc<Mutex> so that cloned CallbackInfo (e.g., in timer callbacks)
            // still push to the same collection
            let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

            // Create reference data container (syntax sugar to reduce parameter count)
            // First get the ctx from the timer's callback before we borrow timer again
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
            ); // Now we can borrow the timer mutably
            let timer = self.timers.get_mut(&TimerId { id: timer_id }).unwrap();
            let tcr = timer.invoke(&callback_info, &system_callbacks.get_system_time_fn);

            ret.callbacks_update_screen = tcr.should_update;
            should_terminate = tcr.should_terminate;

            // Extract changes from the Arc<Mutex> - they may have been pushed by
            // cloned CallbackInfo instances (e.g., in timer callbacks)
            let collected_changes = callback_changes
                .lock()
                .map(|mut guard| core::mem::take(&mut *guard))
                .unwrap_or_default();

            // Apply callback changes collected during timer execution
            let change_result = self.apply_callback_changes(
                collected_changes,
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

            // Forward hit test update request to shell layer
            if change_result.hit_test_update_requested.is_some() {
                ret.hit_test_update_requested = change_result.hit_test_update_requested;
            }

            // Forward queued window states to shell layer for sequential processing
            if !change_result.queued_window_states.is_empty() {
                ret.queued_window_states = change_result.queued_window_states;
            }

            // Forward text_input_triggered to shell layer for recursive callback processing
            if !change_result.text_input_triggered.is_empty() {
                println!("[run_single_timer] Forwarding {} text_input_triggered events", change_result.text_input_triggered.len());
                ret.text_input_triggered = change_result.text_input_triggered;
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
            hit_test_update_requested: None,
            queued_window_states: Vec::new(),
            text_input_triggered: Vec::new(),
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

                let writeback_data_ptr: *mut RefAny = &mut thread_inner.writeback_data as *mut _;
                let is_finished = thread_inner.is_finished();

                (msg, writeback_data_ptr, is_finished)
                // MutexGuard is dropped here
            };

            let ThreadWriteBackMsg {
                refany: mut data,
                callback,
            } = match msg {
                ThreadReceiveMsg::Update(update_screen) => {
                    ret.callbacks_update_screen.max_self(update_screen);
                    continue;
                }
                ThreadReceiveMsg::WriteBack(t) => t,
            };

            // Create changes container for callback transaction system
            let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

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
                system_style: system_style.clone(),
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
                data.clone(),
                callback_info,
            );
            ret.callbacks_update_screen.max_self(callback_update);

            // Extract changes from the Arc<Mutex>
            let collected_changes = callback_changes
                .lock()
                .map(|mut guard| core::mem::take(&mut *guard))
                .unwrap_or_default();

            // Apply callback changes collected during thread writeback
            let change_result = self.apply_callback_changes(
                collected_changes,
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

            // Forward hit test update request
            if change_result.hit_test_update_requested.is_some() {
                ret.hit_test_update_requested = change_result.hit_test_update_requested;
            }

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
            hit_test_update_requested: None,
            queued_window_states: Vec::new(),
            text_input_triggered: Vec::new(),
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

        // Create changes container for callback transaction system
        let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

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
            system_style,
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

        ret.callbacks_update_screen = (callback.cb)(data.clone(), callback_info);

        // Extract changes from the Arc<Mutex>
        let collected_changes = callback_changes
            .lock()
            .map(|mut guard| core::mem::take(&mut *guard))
            .unwrap_or_default();

        // Apply callback changes collected during callback execution
        let change_result = self.apply_callback_changes(
            collected_changes,
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

        // Forward hit test update request (invoke_single_callback)
        if change_result.hit_test_update_requested.is_some() {
            ret.hit_test_update_requested = change_result.hit_test_update_requested;
        }

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
            hit_test_update_requested: None,
            queued_window_states: Vec::new(),
            text_input_triggered: Vec::new(),
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

        // Create changes container for callback transaction system
        let callback_changes = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));

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
            system_style,
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

        ret.callbacks_update_screen =
            (menu_callback.callback.cb)(menu_callback.refany.clone(), callback_info);

        // Extract changes from the Arc<Mutex>
        let collected_changes = callback_changes
            .lock()
            .map(|mut guard| core::mem::take(&mut *guard))
            .unwrap_or_default();

        // Apply callback changes collected during menu callback execution
        let change_result = self.apply_callback_changes(
            collected_changes,
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

        // Forward hit test update request (invoke_menu_callback)
        if change_result.hit_test_update_requested.is_some() {
            ret.hit_test_update_requested = change_result.hit_test_update_requested;
        }

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

        println!("[record_text_input] Called with text: '{}'", text_input);

        let mut affected_nodes = BTreeMap::new();

        if text_input.is_empty() {
            println!("[record_text_input] Empty text, returning empty");
            return affected_nodes;
        }

        // Get focused node
        let focused_node = match self.focus_manager.get_focused_node().copied() {
            Some(node) => {
                println!("[record_text_input] Focused node: {:?}", node);
                node
            },
            None => {
                println!("[record_text_input] ERROR: No focused node!");
                return affected_nodes;
            }
        };

        let node_id = match focused_node.node.into_crate_internal() {
            Some(id) => {
                println!("[record_text_input] Node ID: {:?}", id);
                id
            },
            None => {
                println!("[record_text_input] ERROR: Invalid node ID");
                return affected_nodes;
            }
        };

        // Get the OLD text before any changes
        let old_inline_content = self.get_text_before_textinput(focused_node.dom, node_id);
        let old_text = self.extract_text_from_inline_content(&old_inline_content);
        println!("[record_text_input] Old text: '{}' ({} bytes)", old_text, old_text.len());

        // Record the changeset in TextInputManager (but DON'T apply changes yet)
        println!("[record_text_input] Recording input in TextInputManager...");
        self.text_input_manager.record_input(
            focused_node,
            text_input.to_string(),
            old_text,
            TextInputSource::Keyboard, // Assuming keyboard for now
        );
        println!("[record_text_input] Input recorded successfully");

        // Return affected nodes with TextInput event so callbacks can be invoked
        let text_input_event = vec![EventFilter::Focus(FocusEventFilter::TextInput)];

        affected_nodes.insert(focused_node, (text_input_event, false)); // false = no re-layout yet
        println!("[record_text_input] Returning {} affected nodes", affected_nodes.len());

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
        println!("[apply_text_changeset] Starting...");
        
        // Get the changeset from TextInputManager
        let changeset = match self.text_input_manager.get_pending_changeset() {
            Some(cs) => {
                println!("[apply_text_changeset] Got changeset for node {:?}, inserted='{}', old_len={}", 
                    cs.node, cs.inserted_text.as_str(), cs.old_text.as_str().len());
                cs.clone()
            },
            None => {
                println!("[apply_text_changeset] ERROR: No pending changeset!");
                return Vec::new();
            }
        };

        let node_id = match changeset.node.node.into_crate_internal() {
            Some(id) => {
                println!("[apply_text_changeset] Node ID: {:?}", id);
                id
            },
            None => {
                println!("[apply_text_changeset] ERROR: Invalid node ID");
                self.text_input_manager.clear_changeset();
                return Vec::new();
            }
        };

        let dom_id = changeset.node.dom;
        println!("[apply_text_changeset] DOM ID: {:?}", dom_id);

        // Check if node is contenteditable
        let layout_result = match self.layout_results.get(&dom_id) {
            Some(lr) => lr,
            None => {
                println!("[apply_text_changeset] ERROR: No layout result for DOM {:?}", dom_id);
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
                println!("[apply_text_changeset] ERROR: No styled node at index {}", node_id.index());
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
        println!("[apply_text_changeset] Got content, {} inline items", content.len());

        // Get current cursor/selection from cursor manager
        let current_selection = if let Some(cursor) = self.cursor_manager.get_cursor() {
            println!("[apply_text_changeset] Cursor: run={}, byte={}", 
                cursor.cluster_id.source_run, cursor.cluster_id.start_byte_in_run);
            vec![Selection::Cursor(cursor.clone())]
        } else {
            println!("[apply_text_changeset] No cursor, creating at position 0");
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
            timestamp: azul_core::task::Instant::System(std::time::Instant::now().into()),
        };

        // Apply the edit using text3::edit - this is a pure function
        use crate::text3::edit::{edit_text, TextEdit};
        let text_edit = TextEdit::Insert(changeset.inserted_text.as_str().to_string());
        println!("[apply_text_changeset] Calling edit_text() with Insert('{}')", changeset.inserted_text.as_str());
        let (new_content, new_selections) = edit_text(&content, &current_selection, &text_edit);
        println!("[apply_text_changeset] edit_text returned {} inline items, {} selections", 
            new_content.len(), new_selections.len());

        // Update the cursor/selection in cursor manager
        // This happens lazily, only when we actually apply the changes
        if let Some(Selection::Cursor(new_cursor)) = new_selections.first() {
            println!("[apply_text_changeset] Updating cursor to run={}, byte={}", 
                new_cursor.cluster_id.source_run, new_cursor.cluster_id.start_byte_in_run);
            self.cursor_manager
                .move_cursor_to(new_cursor.clone(), dom_id, node_id);
        }

        // Update the text cache with the new inline content
        println!("[apply_text_changeset] Calling update_text_cache_after_edit()");
        self.update_text_cache_after_edit(dom_id, node_id, new_content);
        println!("[apply_text_changeset] Text cache updated successfully");

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
            timestamp: azul_core::task::Instant::System(std::time::Instant::now().into()),
        };
        self.undo_redo_manager
            .record_operation(undo_changeset, pre_state);

        // Clear the changeset now that it's been applied
        self.text_input_manager.clear_changeset();
        println!("[apply_text_changeset] Changeset cleared");

        // Return nodes that need dirty marking
        let dirty_nodes = self.determine_dirty_text_nodes(dom_id, node_id);
        println!("[apply_text_changeset] Dirty nodes: {:?}", dirty_nodes);
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
        println!("[process_text_input] Called with text: '{}'", text_input);
        let result = self.record_text_input(text_input);
        println!("[process_text_input] record_text_input returned {} affected nodes", result.len());
        for (node, (filters, has_text)) in &result {
            println!("[process_text_input]   Node {:?}: {} filters, has_text={}", node, filters.len(), has_text);
        }
        result
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
            #[cfg(feature = "std")]
            eprintln!("[get_text_before_textinput] Using dirty_text_nodes content for ({:?}, {:?})", dom_id, node_id);
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

        println!("[update_text_cache_after_edit] Starting for DOM {:?}, node {:?}", dom_id, node_id);
        println!("[update_text_cache_after_edit] New content has {} inline items", new_inline_content.len());
        for (i, item) in new_inline_content.iter().enumerate() {
            match item {
                crate::text3::cache::InlineContent::Text(run) => {
                    println!("[update_text_cache_after_edit]   Item {}: Text('{}')", i, run.text);
                }
                _ => {
                    println!("[update_text_cache_after_edit]   Item {}: Non-text", i);
                }
            }
        }

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
        println!("[update_text_cache_after_edit] Stored in dirty_text_nodes");

        // 2. Get the cached constraints from the existing inline layout result
        // We need to find the IFC root node and extract its constraints
        let constraints = {
            let layout_result = match self.layout_results.get(&dom_id) {
                Some(r) => r,
                None => {
                    println!("[update_text_cache_after_edit] ERROR: No layout result for DOM");
                    return;
                }
            };
            
            let layout_node = match layout_result.layout_tree.get(node_id.index()) {
                Some(n) => n,
                None => {
                    println!("[update_text_cache_after_edit] ERROR: Node {} not found in layout tree", node_id.index());
                    return;
                }
            };
            
            let cached_layout = match &layout_node.inline_layout_result {
                Some(c) => c,
                None => {
                    println!("[update_text_cache_after_edit] ERROR: No inline layout cached for node");
                    return;
                }
            };
            
            match &cached_layout.constraints {
                Some(c) => {
                    println!("[update_text_cache_after_edit] Got cached constraints");
                    c.clone()
                },
                None => {
                    println!("[update_text_cache_after_edit] ERROR: No constraints cached");
                    return;
                }
            }
        };

        // 3. Re-run the text3 layout pipeline
        println!("[update_text_cache_after_edit] Re-running text3 layout pipeline...");
        let new_layout = self.relayout_text_node_internal(&new_inline_content, &constraints);

        let Some(new_layout) = new_layout else {
            println!("[update_text_cache_after_edit] ERROR: relayout_text_node_internal returned None");
            return;
        };
        println!("[update_text_cache_after_edit] Text3 layout complete, {} items", new_layout.items.len());

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
                println!("[update_text_cache_after_edit] Old size: {:?}, new size: {:?}", old_size, new_size);

                // Check if we need to propagate layout shift
                if let (Some(old), Some(new)) = (old_size, new_size) {
                    if (old.height - new.height).abs() > 0.5 || (old.width - new.width).abs() > 0.5 {
                        // Mark that ancestor relayout is needed
                        println!("[update_text_cache_after_edit] Size changed, marking for ancestor relayout");
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
                println!("[update_text_cache_after_edit] Layout cache updated successfully");
            } else {
                println!("[update_text_cache_after_edit] ERROR: Layout node {} not found for update", node_id.index());
            }
        } else {
            println!("[update_text_cache_after_edit] ERROR: Layout result not found for update");
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

        #[cfg(feature = "std")]
        eprintln!("[DEBUG] process_mouse_click_for_selection: position=({:.1},{:.1}), time_ms={}", 
            position.x, position.y, time_ms);

        // found_selection stores: (dom_id, ifc_root_node_id, selection_range, local_pos)
        // IMPORTANT: We always store the IFC root NodeId, not the text node NodeId,
        // because selections are rendered via inline_layout_result which lives on the IFC root.
        let mut found_selection: Option<(DomId, NodeId, SelectionRange, azul_core::geom::LogicalPosition)> = None;

        // Try to get hit test from HoverManager first (fast path, uses WebRender's point_relative_to_item)
        if let Some(hit_test) = self.hover_manager.get_current(&InputPointId::Mouse) {
            #[cfg(feature = "std")]
            eprintln!("[DEBUG] HoverManager has hit test with {} doms", hit_test.hovered_nodes.len());
            
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
            #[cfg(feature = "std")]
            eprintln!("[DEBUG] Fallback path: layout_results count = {}", self.layout_results.len());
            
            for (dom_id, layout_result) in &self.layout_results {
                // Use the layout tree from layout_result, not layout_cache
                // layout_cache.tree is for the root DOM only; layout_result.layout_tree
                // is the correct tree for each DOM (including iframes)
                let tree = &layout_result.layout_tree;
                
                #[cfg(feature = "std")]
                {
                    let ifc_root_count = tree.nodes.iter()
                        .filter(|n| n.inline_layout_result.is_some())
                        .count();
                    eprintln!("[DEBUG] DOM {:?}: tree has {} nodes, {} IFC roots", 
                        dom_id, tree.nodes.len(), ifc_root_count);
                }
                
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
                        #[cfg(feature = "std")]
                        eprintln!("[DEBUG]   IFC root node_idx={} node_id={:?}: NOT selectable", node_idx, node_id);
                        continue;
                    }
                    
                    // Get the node's absolute position
                    // Use layout_result.calculated_positions for the correct DOM
                    let node_pos = layout_result.calculated_positions
                        .get(&node_idx)
                        .copied()
                        .unwrap_or_default();
                    
                    // Check if position is within node bounds
                    let node_size = layout_node.used_size.unwrap_or_else(|| {
                        let bounds = cached_layout.layout.bounds();
                        azul_core::geom::LogicalSize::new(bounds.width, bounds.height)
                    });
                    
                    #[cfg(feature = "std")]
                    eprintln!("[DEBUG]   IFC root node_idx={} node_id={:?}: pos=({:.1},{:.1}) size=({:.1},{:.1}), click=({:.1},{:.1})",
                        node_idx, node_id, node_pos.x, node_pos.y, node_size.width, node_size.height, position.x, position.y);
                    
                    if position.x < node_pos.x || position.x > node_pos.x + node_size.width ||
                       position.y < node_pos.y || position.y > node_pos.y + node_size.height {
                        #[cfg(feature = "std")]
                        eprintln!("[DEBUG]     -> OUT OF BOUNDS");
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
        
        #[cfg(feature = "std")]
        eprintln!("[DEBUG] Setting selection on dom_id={:?}, node_id={:?}", dom_id, ifc_root_node_id);
        
        self.selection_manager.set_selection(dom_id, state);

        // CRITICAL FIX 1: Set focus on the clicked node
        // Without this, clicking on a contenteditable element shows a cursor but
        // text input doesn't work because record_text_input() checks focus_manager.get_focused_node()
        // and returns early if there's no focus.
        //
        // Check if the node is contenteditable before setting focus
        let is_contenteditable = self.layout_results.get(&dom_id)
            .and_then(|lr| lr.styled_dom.node_data.as_ref().get(ifc_root_node_id.index()))
            .map(|styled_node| {
                styled_node.attributes.as_ref().iter().any(|attr| {
                    matches!(attr, azul_core::dom::AttributeType::ContentEditable(_))
                })
            })
            .unwrap_or(false);
        
        if is_contenteditable {
            self.focus_manager.set_focused_node(Some(dom_node_id));
            #[cfg(feature = "std")]
            eprintln!("[DEBUG] Set focus on contenteditable node {:?}", ifc_root_node_id);
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
        
        #[cfg(feature = "std")]
        eprintln!("[DEBUG] Initialized cursor at {:?} for node {:?}", final_range.start, ifc_root_node_id);

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

        #[cfg(feature = "std")]
        eprintln!("[DEBUG] process_mouse_drag_for_selection: current=({:.1},{:.1})",
            current_position.x, current_position.y);

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
        
        #[cfg(feature = "std")]
        eprintln!("[DEBUG] Found anchor at IFC root {:?}, cursor {:?}", 
            anchor.ifc_root_node_id, anchor.cursor.cluster_id);

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
        
        #[cfg(feature = "std")]
        eprintln!("[DEBUG] Found focus at IFC root {:?}, cursor {:?}", 
            focus_ifc_root, focus_cursor.cluster_id);

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
        
        #[cfg(feature = "std")]
        eprintln!("[DEBUG] Nodes in range: {:?}, is_forward: {}", 
            nodes_in_range.iter().map(|n| n.index()).collect::<Vec<_>>(), is_forward);
        
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
        
        #[cfg(feature = "std")]
        eprintln!("[DEBUG] Affected IFC roots: {:?}", 
            affected_nodes_map.keys().map(|n| n.index()).collect::<Vec<_>>());
        
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
                                            core_callback.refany.clone(),
                                            gl_callback_info,
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

```

## dll/src/desktop/shell2/macos/events.rs
// macOS event handling - keyDown, insertText
// 1160 lines

```rust
//! macOS Event handling - converts NSEvent to Azul events and dispatches callbacks.

use super::super::common::debug_server::LogCategory;
use crate::{log_debug, log_error, log_info, log_trace, log_warn};

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId, ScrollbarOrientation},
    events::{EventFilter, MouseButton, ProcessEventResult, SyntheticEvent},
    geom::{LogicalPosition, PhysicalPositionI32},
    hit_test::{CursorTypeHitTest, FullHitTest},
    window::{
        CursorPosition, KeyboardState, MouseCursorType, MouseState, OptionMouseCursorType,
        VirtualKeyCode, WindowFrame,
    },
};
use azul_layout::{
    callbacks::CallbackInfo,
    managers::{
        hover::InputPointId,
        scroll_state::{ScrollbarComponent, ScrollbarHit},
    },
    solver3::display_list::DisplayList,
    window::LayoutWindow,
    window_state::FullWindowState,
};
use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType};
use objc2_foundation::NSPoint;

use super::MacOSWindow;
// Re-export common types
pub use crate::desktop::shell2::common::event_v2::HitTestNode;
// Import V2 cross-platform event processing trait
use crate::desktop::shell2::common::event_v2::PlatformWindowV2;

/// Convert macOS window coordinates to Azul logical coordinates.
///
/// macOS uses a bottom-left origin coordinate system where Y=0 is at the bottom.
/// Azul/WebRender uses a top-left origin coordinate system where Y=0 is at the top.
/// This function converts from macOS to Azul coordinates.
#[inline]
fn macos_to_azul_coords(location: NSPoint, window_height: f32) -> LogicalPosition {
    LogicalPosition::new(location.x as f32, window_height - location.y as f32)
}

/// Extension trait for Callback to convert from CoreCallback
trait CallbackExt {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self;
}

impl CallbackExt for azul_layout::callbacks::Callback {
    fn from_core(core_callback: azul_core::callbacks::CoreCallback) -> Self {
        // Use the existing safe wrapper method from Callback
        azul_layout::callbacks::Callback::from_core(core_callback)
    }
}

/// Result of processing an event - determines whether to redraw, update layout, etc.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EventProcessResult {
    /// No action needed
    DoNothing,
    /// Request redraw (present() will be called)
    RequestRedraw,
    /// Layout changed, need full rebuild
    RegenerateDisplayList,
    /// Window should close
    CloseWindow,
}

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events,
    /// keys)
    RootNodes,
}

impl MacOSWindow {
    /// Convert ProcessEventResult to platform-specific EventProcessResult
    #[inline]
    fn convert_process_result(result: azul_core::events::ProcessEventResult) -> EventProcessResult {
        use azul_core::events::ProcessEventResult as PER;
        match result {
            PER::DoNothing => EventProcessResult::DoNothing,
            PER::ShouldReRenderCurrentWindow => EventProcessResult::RequestRedraw,
            PER::ShouldUpdateDisplayListCurrentWindow => EventProcessResult::RegenerateDisplayList,
            PER::UpdateHitTesterAndProcessAgain => EventProcessResult::RegenerateDisplayList,
            PER::ShouldRegenerateDomCurrentWindow => EventProcessResult::RegenerateDisplayList,
            PER::ShouldRegenerateDomAllWindows => EventProcessResult::RegenerateDisplayList,
        }
    }

    // NOTE: perform_scrollbar_hit_test(), handle_scrollbar_click(), and handle_scrollbar_drag()
    // are now provided by the PlatformWindowV2 trait as default methods.
    // The trait methods are cross-platform and work identically.
    // See dll/src/desktop/shell2/common/event_v2.rs for the implementation.

    /// Process a mouse button down event.
    pub fn handle_mouse_down(
        &mut self,
        event: &NSEvent,
        button: MouseButton,
    ) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Check for scrollbar hit FIRST (before state changes)
        // Use trait method from PlatformWindowV2
        if let Some(scrollbar_hit_id) = PlatformWindowV2::perform_scrollbar_hit_test(self, position)
        {
            let result = PlatformWindowV2::handle_scrollbar_click(self, scrollbar_hit_id, position);
            return Self::convert_process_result(result);
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Set appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = true,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = true,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = true,
            _ => {}
        }

        // Record input sample for gesture detection (button down starts new session)
        let button_state = match button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            _ => 0x00,
        };
        self.record_input_sample(position, button_state, true, false);

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Use V2 cross-platform event system - it will automatically:
        // - Detect MouseDown event (left/right/middle)
        // - Dispatch to hovered nodes (including CSD buttons with callbacks)
        // - Handle event propagation
        // - Process callback results recursively
        let result = self.process_window_events_recursive_v2(0);

        Self::convert_process_result(result)
    }

    /// Process a mouse button up event.
    pub fn handle_mouse_up(&mut self, event: &NSEvent, button: MouseButton) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // End scrollbar drag if active (before state changes)
        if self.scrollbar_drag_state.is_some() {
            self.scrollbar_drag_state = None;
            return EventProcessResult::RequestRedraw;
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state - clear appropriate button flag
        match button {
            MouseButton::Left => self.current_window_state.mouse_state.left_down = false,
            MouseButton::Right => self.current_window_state.mouse_state.right_down = false,
            MouseButton::Middle => self.current_window_state.mouse_state.middle_down = false,
            _ => {}
        }

        // Record input sample for gesture detection (button up ends session)
        let button_state = match button {
            MouseButton::Left => 0x01,
            MouseButton::Right => 0x02,
            MouseButton::Middle => 0x04,
            _ => 0x00,
        };
        self.record_input_sample(position, button_state, false, true);

        // Perform hit testing and update last_hit_test
        self.update_hit_test(position);

        // Check for right-click context menu (before event processing)
        if button == MouseButton::Right {
            if let Some(hit_node) = self.get_first_hovered_node() {
                if self
                    .try_show_context_menu(hit_node, position, event)
                    .is_some()
                {
                    return EventProcessResult::DoNothing;
                }
            }
        }

        // Use V2 cross-platform event system - automatically detects MouseUp
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process a mouse move event.
    pub fn handle_mouse_move(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Handle active scrollbar drag (special case - not part of normal event system)
        // Use trait method from PlatformWindowV2
        if self.scrollbar_drag_state.is_some() {
            let result = PlatformWindowV2::handle_scrollbar_drag(self, position);
            return Self::convert_process_result(result);
        }

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Record input sample for gesture detection (movement during button press)
        let button_state = if self.current_window_state.mouse_state.left_down {
            0x01
        } else {
            0x00
        } | if self.current_window_state.mouse_state.right_down {
            0x02
        } else {
            0x00
        } | if self.current_window_state.mouse_state.middle_down {
            0x04
        } else {
            0x00
        };
        self.record_input_sample(position, button_state, false, false);

        // Update hit test
        self.update_hit_test(position);

        // Update cursor based on CSS cursor properties
        // This is done BEFORE callbacks so callbacks can override the cursor
        if let Some(layout_window) = self.layout_window.as_ref() {
            if let Some(hit_test) = layout_window
                .hover_manager
                .get_current(&InputPointId::Mouse)
            {
                let cursor_test = layout_window.compute_cursor_type_hit_test(hit_test);
                // Update the window state cursor type
                self.current_window_state.mouse_state.mouse_cursor_type =
                    Some(cursor_test.cursor_icon).into();
                // Set the actual OS cursor
                let cursor_name = self.map_cursor_type_to_macos(cursor_test.cursor_icon);
                self.set_cursor(cursor_name);
            }
        }

        // V2 system will detect MouseOver/MouseEnter/MouseLeave/Drag from state diff
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process mouse entered window event.
    pub fn handle_mouse_entered(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state - cursor is now in window
        self.current_window_state.mouse_state.cursor_position = CursorPosition::InWindow(position);

        // Update hit test
        self.update_hit_test(position);

        // V2 system will detect MouseEnter events from state diff
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process mouse exited window event.
    pub fn handle_mouse_exited(&mut self, event: &NSEvent) -> EventProcessResult {
        let location = unsafe { event.locationInWindow() };
        let window_height = self.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update mouse state - cursor left window
        self.current_window_state.mouse_state.cursor_position =
            CursorPosition::OutOfWindow(position);

        // Clear last hit test since mouse is out
        use azul_layout::managers::hover::InputPointId;
        if let Some(ref mut layout_window) = self.layout_window {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, FullHitTest::empty(None));
        }

        // V2 system will detect MouseLeave events from state diff
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process a scroll wheel event.
    pub fn handle_scroll_wheel(&mut self, event: &NSEvent) -> EventProcessResult {
        let delta_x = unsafe { event.scrollingDeltaX() };
        let delta_y = unsafe { event.scrollingDeltaY() };
        let _has_precise = unsafe { event.hasPreciseScrollingDeltas() };

        let location = unsafe { event.locationInWindow() };
        let window_height = self.current_window_state.size.dimensions.height;
        let position = macos_to_azul_coords(location, window_height);

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update hit test FIRST (required for scroll manager)
        self.update_hit_test(position);

        // Record scroll sample using ScrollManager (if delta is significant)
        // The ScrollManager will update its internal state, and during the next render,
        // scroll_all_nodes() will synchronize the offsets to WebRender automatically.
        if (delta_x.abs() > 0.01 || delta_y.abs() > 0.01) {
            if let Some(layout_window) = self.get_layout_window_mut() {
                use azul_core::task::Instant;
                use azul_layout::managers::hover::InputPointId;

                let now = Instant::from(std::time::Instant::now());
                let _scroll_result = layout_window.scroll_manager.record_sample(
                    -delta_x as f32, // Invert for natural scrolling
                    -delta_y as f32,
                    &layout_window.hover_manager,
                    &InputPointId::Mouse,
                    now,
                );

                // Note: We do NOT call gpu_scroll() here - it would cause double-scrolling!
                // The scroll state will be automatically synchronized to WebRender during
                // the next render_and_present() call via scroll_all_nodes().
            }
        }

        // V2 system will detect Scroll event from ScrollManager state
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process a key down event.
    pub fn handle_key_down(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Extract Unicode character from event
        let character = unsafe {
            event.characters().and_then(|s| {
                let s_str = s.to_string();
                s_str.chars().next()
            })
        };

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update keyboard state with keycode
        self.update_keyboard_state(key_code, modifiers, true);

        // NOTE: We do NOT call record_text_input here!
        // On macOS, text input comes through the IME system via insertText:
        // which calls handle_text_input(). Calling record_text_input here
        // would cause DOUBLE text input because both keyDown and insertText
        // are called by the system for normal key presses.
        // The character from keyDown is only used for VirtualKeyDown events,
        // not for text input.

        // V2 system will detect VirtualKeyDown and TextInput from state diff
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process a key up event.
    pub fn handle_key_up(&mut self, event: &NSEvent) -> EventProcessResult {
        let key_code = unsafe { event.keyCode() };
        let modifiers = unsafe { event.modifierFlags() };

        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update keyboard state
        self.update_keyboard_state(key_code, modifiers, false);

        // Clear current character on key up
        self.update_keyboard_state_with_char(None);

        // V2 system will detect VirtualKeyUp from state diff
        let result = self.process_window_events_recursive_v2(0);
        Self::convert_process_result(result)
    }

    /// Process text input from IME (called from insertText:replacementRange:)
    ///
    /// This is the proper way to handle text input on macOS, as it respects
    /// the IME composition system for non-ASCII characters (accents, CJK, etc.)
    pub fn handle_text_input(&mut self, text: &str) {
        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Record text input - V2 system will detect TextInput event from state diff
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.record_text_input(text);
        }

        // Process V2 events
        let _ = self.process_window_events_recursive_v2(0);

        // Request redraw if needed
        self.frame_needs_regeneration = true;
    }

    /// Process a flags changed event (modifier keys).
    pub fn handle_flags_changed(&mut self, event: &NSEvent) -> EventProcessResult {
        let modifiers = unsafe { event.modifierFlags() };

        // Determine which modifier keys are currently pressed
        let shift_pressed = modifiers.contains(NSEventModifierFlags::Shift);
        let ctrl_pressed = modifiers.contains(NSEventModifierFlags::Control);
        let alt_pressed = modifiers.contains(NSEventModifierFlags::Option);
        let cmd_pressed = modifiers.contains(NSEventModifierFlags::Command);

        // Track previous state to detect what changed
        let keyboard_state = &self.current_window_state.keyboard_state;
        let was_shift_down = keyboard_state.shift_down();
        let was_ctrl_down = keyboard_state.ctrl_down();
        let was_alt_down = keyboard_state.alt_down();
        let was_cmd_down = keyboard_state.super_down();

        // Update keyboard state based on changes
        use azul_core::window::VirtualKeyCode;

        // Shift key changed
        if shift_pressed != was_shift_down {
            if shift_pressed {
                self.update_keyboard_state(0x38, modifiers, true); // LShift keycode
            } else {
                self.update_keyboard_state(0x38, modifiers, false);
            }
        }

        // Control key changed
        if ctrl_pressed != was_ctrl_down {
            if ctrl_pressed {
                self.update_keyboard_state(0x3B, modifiers, true); // LControl keycode
            } else {
                self.update_keyboard_state(0x3B, modifiers, false);
            }
        }

        // Alt/Option key changed
        if alt_pressed != was_alt_down {
            if alt_pressed {
                self.update_keyboard_state(0x3A, modifiers, true); // LAlt keycode
            } else {
                self.update_keyboard_state(0x3A, modifiers, false);
            }
        }

        // Command key changed
        if cmd_pressed != was_cmd_down {
            if cmd_pressed {
                self.update_keyboard_state(0x37, modifiers, true); // LWin (Command) keycode
            } else {
                self.update_keyboard_state(0x37, modifiers, false);
            }
        }

        // Dispatch modifier changed callbacks if any modifier changed
        if shift_pressed != was_shift_down
            || ctrl_pressed != was_ctrl_down
            || alt_pressed != was_alt_down
            || cmd_pressed != was_cmd_down
        {
            // For now, just return DoNothing - could dispatch specific callbacks later
            EventProcessResult::DoNothing
        } else {
            EventProcessResult::DoNothing
        }
    }

    /// Process a window resize event.
    pub fn handle_resize(&mut self, new_width: f64, new_height: f64) -> EventProcessResult {
        use azul_core::geom::LogicalSize;

        let new_size = LogicalSize {
            width: new_width as f32,
            height: new_height as f32,
        };

        // Store old context for comparison
        let old_context = self.dynamic_selector_context.clone();

        // Update window state
        self.current_window_state.size.dimensions = new_size;

        // Update dynamic selector context with new viewport dimensions
        self.dynamic_selector_context.viewport_width = new_width as f32;
        self.dynamic_selector_context.viewport_height = new_height as f32;
        self.dynamic_selector_context.orientation = if new_width > new_height {
            azul_css::dynamic_selector::OrientationType::Landscape
        } else {
            azul_css::dynamic_selector::OrientationType::Portrait
        };

        // Check if DPI changed (window may have moved to different display)
        let current_hidpi = self.get_hidpi_factor();
        let old_hidpi = self.current_window_state.size.get_hidpi_factor();

        if (current_hidpi.inner.get() - old_hidpi.inner.get()).abs() > 0.001 {
            log_info!(
                LogCategory::Window,
                "[Resize] DPI changed: {} -> {}",
                old_hidpi.inner.get(),
                current_hidpi.inner.get()
            );
            self.current_window_state.size.dpi = (current_hidpi.inner.get() * 96.0) as u32;
        }

        // Notify compositor of resize (this is private in mod.rs, so we inline it here)
        if let Err(e) = self.handle_compositor_resize() {
            log_error!(LogCategory::Rendering, "Compositor resize failed: {}", e);
        }

        // Check if viewport dimensions actually changed (debounce rapid resize events)
        let viewport_changed =
            (old_context.viewport_width - self.dynamic_selector_context.viewport_width).abs() > 0.5
                || (old_context.viewport_height - self.dynamic_selector_context.viewport_height)
                    .abs()
                    > 0.5;

        if !viewport_changed {
            // No significant change, just update compositor
            return EventProcessResult::RequestRedraw;
        }

        // Check if any CSS breakpoints were crossed
        // Common breakpoints: 320, 480, 640, 768, 1024, 1280, 1440, 1920
        let breakpoints = [320.0, 480.0, 640.0, 768.0, 1024.0, 1280.0, 1440.0, 1920.0];
        let breakpoint_crossed =
            old_context.viewport_breakpoint_changed(&self.dynamic_selector_context, &breakpoints);

        if breakpoint_crossed {
            log_debug!(
                LogCategory::Layout,
                "[Resize] Breakpoint crossed: {}x{} -> {}x{}",
                old_context.viewport_width,
                old_context.viewport_height,
                self.dynamic_selector_context.viewport_width,
                self.dynamic_selector_context.viewport_height
            );
        }

        // Resize requires full display list rebuild
        EventProcessResult::RegenerateDisplayList
    }

    /// Process a file drop event.
    pub fn handle_file_drop(&mut self, paths: Vec<String>) -> EventProcessResult {
        // Save previous state BEFORE making changes
        self.previous_window_state = Some(self.current_window_state.clone());

        // Update cursor manager with dropped file
        if let Some(first_path) = paths.first() {
            if let Some(layout_window) = self.layout_window.as_mut() {
                layout_window
                    .file_drop_manager
                    .set_dropped_file(Some(first_path.clone().into()));
            }
        }

        // Update hit test at current cursor position
        if let CursorPosition::InWindow(pos) = self.current_window_state.mouse_state.cursor_position
        {
            self.update_hit_test(pos);
        }

        // V2 system will detect FileDrop event from state diff
        let result = self.process_window_events_recursive_v2(0);

        // Clear dropped file after processing (one-shot event)
        if let Some(layout_window) = self.layout_window.as_mut() {
            layout_window.file_drop_manager.set_dropped_file(None);
        }

        Self::convert_process_result(result)
    }

    /// Perform hit testing at given position using WebRender hit-testing API.
    fn perform_hit_test(&mut self, position: LogicalPosition) -> Option<HitTestNode> {
        use azul_core::window::CursorPosition;

        let layout_window = self.layout_window.as_ref()?;

        // Early return if no layout results
        if layout_window.layout_results.is_empty() {
            return None;
        }

        let cursor_position = CursorPosition::InWindow(position);

        // Get focused node from FocusManager
        let focused_node = layout_window.focus_manager.get_focused_node().copied();

        // Use layout_results directly (BTreeMap)
        let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
            &*self.hit_tester.resolve(),
            self.document_id,
            focused_node,
            &layout_window.layout_results,
            &cursor_position,
            self.current_window_state.size.get_hidpi_factor(),
        );

        // Extract first hovered node from hit test result
        hit_test
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes.keys().next().map(|node_id| {
                    let node_id_value = node_id.index();
                    HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id_value as u64,
                    }
                })
            })
            .next()
    }

    /// Convert macOS keycode to VirtualKeyCode.
    fn convert_keycode(&self, keycode: u16) -> Option<VirtualKeyCode> {
        // macOS keycodes: https://eastmanreference.com/complete-list-of-applescript-key-codes
        match keycode {
            0x00 => Some(VirtualKeyCode::A),
            0x01 => Some(VirtualKeyCode::S),
            0x02 => Some(VirtualKeyCode::D),
            0x03 => Some(VirtualKeyCode::F),
            0x04 => Some(VirtualKeyCode::H),
            0x05 => Some(VirtualKeyCode::G),
            0x06 => Some(VirtualKeyCode::Z),
            0x07 => Some(VirtualKeyCode::X),
            0x08 => Some(VirtualKeyCode::C),
            0x09 => Some(VirtualKeyCode::V),
            0x0B => Some(VirtualKeyCode::B),
            0x0C => Some(VirtualKeyCode::Q),
            0x0D => Some(VirtualKeyCode::W),
            0x0E => Some(VirtualKeyCode::E),
            0x0F => Some(VirtualKeyCode::R),
            0x10 => Some(VirtualKeyCode::Y),
            0x11 => Some(VirtualKeyCode::T),
            0x12 => Some(VirtualKeyCode::Key1),
            0x13 => Some(VirtualKeyCode::Key2),
            0x14 => Some(VirtualKeyCode::Key3),
            0x15 => Some(VirtualKeyCode::Key4),
            0x16 => Some(VirtualKeyCode::Key6),
            0x17 => Some(VirtualKeyCode::Key5),
            0x18 => Some(VirtualKeyCode::Equals),
            0x19 => Some(VirtualKeyCode::Key9),
            0x1A => Some(VirtualKeyCode::Key7),
            0x1B => Some(VirtualKeyCode::Minus),
            0x1C => Some(VirtualKeyCode::Key8),
            0x1D => Some(VirtualKeyCode::Key0),
            0x1E => Some(VirtualKeyCode::RBracket),
            0x1F => Some(VirtualKeyCode::O),
            0x20 => Some(VirtualKeyCode::U),
            0x21 => Some(VirtualKeyCode::LBracket),
            0x22 => Some(VirtualKeyCode::I),
            0x23 => Some(VirtualKeyCode::P),
            0x24 => Some(VirtualKeyCode::Return),
            0x25 => Some(VirtualKeyCode::L),
            0x26 => Some(VirtualKeyCode::J),
            0x27 => Some(VirtualKeyCode::Apostrophe),
            0x28 => Some(VirtualKeyCode::K),
            0x29 => Some(VirtualKeyCode::Semicolon),
            0x2A => Some(VirtualKeyCode::Backslash),
            0x2B => Some(VirtualKeyCode::Comma),
            0x2C => Some(VirtualKeyCode::Slash),
            0x2D => Some(VirtualKeyCode::N),
            0x2E => Some(VirtualKeyCode::M),
            0x2F => Some(VirtualKeyCode::Period),
            0x30 => Some(VirtualKeyCode::Tab),
            0x31 => Some(VirtualKeyCode::Space),
            0x32 => Some(VirtualKeyCode::Grave),
            0x33 => Some(VirtualKeyCode::Back),
            0x35 => Some(VirtualKeyCode::Escape),
            0x37 => Some(VirtualKeyCode::LWin), // Command
            0x38 => Some(VirtualKeyCode::LShift),
            0x39 => Some(VirtualKeyCode::Capital), // Caps Lock
            0x3A => Some(VirtualKeyCode::LAlt),    // Option
            0x3B => Some(VirtualKeyCode::LControl),
            0x3C => Some(VirtualKeyCode::RShift),
            0x3D => Some(VirtualKeyCode::RAlt),
            0x3E => Some(VirtualKeyCode::RControl),
            0x7B => Some(VirtualKeyCode::Left),
            0x7C => Some(VirtualKeyCode::Right),
            0x7D => Some(VirtualKeyCode::Down),
            0x7E => Some(VirtualKeyCode::Up),
            _ => None,
        }
    }

    /// Update keyboard state from event.
    fn update_keyboard_state(
        &mut self,
        keycode: u16,
        modifiers: NSEventModifierFlags,
        is_down: bool,
    ) {
        use azul_core::window::VirtualKeyCode;

        // Convert keycode to VirtualKeyCode first (before borrowing)
        let vk = match self.convert_keycode(keycode) {
            Some(k) => k,
            None => return,
        };

        let keyboard_state = &mut self.current_window_state.keyboard_state;

        if is_down {
            // Add to pressed keys if not already present
            let mut already_pressed = false;
            for pressed_key in keyboard_state.pressed_virtual_keycodes.as_ref() {
                if *pressed_key == vk {
                    already_pressed = true;
                    break;
                }
            }
            if !already_pressed {
                // Convert to Vec, add, convert back
                let mut pressed_vec: Vec<VirtualKeyCode> =
                    keyboard_state.pressed_virtual_keycodes.as_ref().to_vec();
                pressed_vec.push(vk);
                keyboard_state.pressed_virtual_keycodes =
                    azul_core::window::VirtualKeyCodeVec::from_vec(pressed_vec);
            }
            keyboard_state.current_virtual_keycode =
                azul_core::window::OptionVirtualKeyCode::Some(vk);
        } else {
            // Remove from pressed keys
            let pressed_vec: Vec<VirtualKeyCode> = keyboard_state
                .pressed_virtual_keycodes
                .as_ref()
                .iter()
                .copied()
                .filter(|k| *k != vk)
                .collect();
            keyboard_state.pressed_virtual_keycodes =
                azul_core::window::VirtualKeyCodeVec::from_vec(pressed_vec);
            keyboard_state.current_virtual_keycode = azul_core::window::OptionVirtualKeyCode::None;
        }
    }

    /// Update keyboard state with character from event
    /// NOTE: This method is deprecated and should not set current_char anymore.
    /// Text input is now handled by process_text_input() which receives the
    /// composed text directly from NSTextInputClient.
    fn update_keyboard_state_with_char(&mut self, _character: Option<char>) {
        // current_char field has been removed from KeyboardState
        // KeyboardState now only tracks virtual keys and scancodes
        // Text input is handled separately by LayoutWindow::process_text_input()
    }

    /// Handle compositor resize notification.
    fn handle_compositor_resize(&mut self) -> Result<(), String> {
        use webrender::api::units::{DeviceIntRect, DeviceIntSize, DevicePixelScale};

        // Get new physical size
        let physical_size = self.current_window_state.size.get_physical_size();
        let new_size = DeviceIntSize::new(physical_size.width as i32, physical_size.height as i32);
        let hidpi_factor = self.current_window_state.size.get_hidpi_factor();

        // Update WebRender document size
        let mut txn = webrender::Transaction::new();
        let device_rect = DeviceIntRect::from_size(new_size);
        // NOTE: azul_layout outputs coordinates in CSS pixels (logical pixels).
        txn.set_document_view(device_rect, DevicePixelScale::new(hidpi_factor.inner.get()));

        // Send transaction
        if let Some(ref layout_window) = self.layout_window {
            let document_id =
                crate::desktop::wr_translate2::wr_translate_document_id(layout_window.document_id);
            self.render_api.send_transaction(document_id, txn);
        }

        // Resize GL viewport (if OpenGL backend)
        if let Some(ref gl_context) = self.gl_context {
            // Make context current
            unsafe {
                gl_context.makeCurrentContext();
            }

            // Resize viewport
            if let Some(ref gl) = self.gl_functions {
                use azul_core::gl as gl_types;
                gl.functions.viewport(
                    0,
                    0,
                    physical_size.width as gl_types::GLint,
                    physical_size.height as gl_types::GLint,
                );
            }
        }

        // Resize CPU framebuffer if using CPU backend
        if let Some(cpu_view) = &self.cpu_view {
            unsafe {
                // Force the CPU view to resize its framebuffer on next draw
                // The actual resize happens in CPUView::drawRect when bounds change
                cpu_view.setNeedsDisplay(true);
            }
        }

        Ok(())
    }

    /// Try to show context menu for the given node at position.
    /// Returns Some if a menu was shown, None otherwise.
    fn try_show_context_menu(
        &mut self,
        node: HitTestNode,
        position: LogicalPosition,
        event: &NSEvent,
    ) -> Option<()> {
        use azul_core::dom::DomId;

        let layout_window = self.layout_window.as_ref()?;
        let dom_id = DomId {
            inner: node.dom_id as usize,
        };

        // Get layout result for this DOM
        let layout_result = layout_window.layout_results.get(&dom_id)?;

        // Check if this node has a context menu
        let node_id = azul_core::id::NodeId::from_usize(node.node_id as usize)?;
        let binding = layout_result.styled_dom.node_data.as_container();
        let node_data = binding.get(node_id)?;

        // Context menus are stored directly on NodeData, not as callbacks
        // Clone the menu to avoid borrow conflicts
        let context_menu = node_data.get_context_menu()?.clone();

        log_debug!(
            LogCategory::Input,
            "[Context Menu] Showing context menu at ({}, {}) for node {:?} with {} items",
            position.x,
            position.y,
            node,
            context_menu.items.as_slice().len()
        );

        // Check if native context menus are enabled
        if self.current_window_state.flags.use_native_context_menus {
            self.show_native_context_menu_at_position(&context_menu, position, event);
        } else {
            self.show_window_based_context_menu(&context_menu, position);
        }

        Some(())
    }

    /// Show an NSMenu as a context menu at the given screen position.
    fn show_native_context_menu_at_position(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
        event: &NSEvent,
    ) {
        use objc2_app_kit::{NSMenu, NSMenuItem};
        use objc2_foundation::{MainThreadMarker, NSPoint, NSString};

        let mtm = match MainThreadMarker::new() {
            Some(m) => m,
            None => {
                log_warn!(
                    LogCategory::Platform,
                    "[Context Menu] Not on main thread, cannot show menu"
                );
                return;
            }
        };

        let ns_menu = NSMenu::new(mtm);

        // Build menu items recursively from Azul menu structure
        Self::recursive_build_nsmenu(&ns_menu, menu.items.as_slice(), &mtm, &mut self.menu_state);

        // Show the menu at the specified position
        let view_point = NSPoint {
            x: position.x as f64,
            y: position.y as f64,
        };

        let view = if let Some(ref gl_view) = self.gl_view {
            Some(&**gl_view as &objc2::runtime::AnyObject)
        } else if let Some(ref cpu_view) = self.cpu_view {
            Some(&**cpu_view as &objc2::runtime::AnyObject)
        } else {
            None
        };

        if let Some(view) = view {
            log_debug!(
                LogCategory::Input,
                "[Context Menu] Showing native menu at position ({}, {}) with {} items",
                position.x,
                position.y,
                menu.items.as_slice().len()
            );

            unsafe {
                use objc2::{msg_send_id, rc::Retained, runtime::AnyObject, sel};

                let _: () = msg_send_id![
                    &ns_menu,
                    popUpMenuPositioningItem: Option::<&AnyObject>::None,
                    atLocation: view_point,
                    inView: view
                ];
            }
        }
    }

    /// Show a context menu using Azul window-based menu system
    ///
    /// This uses the same unified menu system as regular menus (crate::desktop::menu::show_menu)
    /// but spawns at cursor position instead of below a trigger rect.
    ///
    /// The menu window creation is queued and will be processed in Phase 3 of the event loop.
    fn show_window_based_context_menu(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: LogicalPosition,
    ) {
        // Get parent window position
        let parent_pos = match self.current_window_state.position {
            azul_core::window::WindowPosition::Initialized(pos) => {
                LogicalPosition::new(pos.x as f32, pos.y as f32)
            }
            _ => LogicalPosition::new(0.0, 0.0),
        };

        // Create menu window options using the unified menu system
        // This is identical to how menu bar menus work, but with cursor_pos instead of trigger_rect
        let menu_options = crate::desktop::menu::show_menu(
            menu.clone(),
            self.system_style.clone(),
            parent_pos,
            None,           // No trigger rect for context menus (they spawn at cursor)
            Some(position), // Cursor position for menu positioning
            None,           // No parent menu
        );

        // Queue window creation request for processing in Phase 3 of the event loop
        // The event loop will create the window with MacOSWindow::new_with_fc_cache()
        log_debug!(
            LogCategory::Window,
            "[macOS] Queuing window-based context menu at screen ({}, {}) - will be created in \
             event loop Phase 3",
            position.x,
            position.y
        );

        self.pending_window_creates.push(menu_options);
    }

    /// Recursively builds an NSMenu from Azul MenuItem array
    ///
    /// This mirrors the Win32 recursive_construct_menu() logic:
    /// - Leaf items (no children) -> addItem with callback
    /// - Items with children -> create submenu and recurse
    /// - Separators -> add separator item
    pub(crate) fn recursive_build_nsmenu(
        menu: &objc2_app_kit::NSMenu,
        items: &[azul_core::menu::MenuItem],
        mtm: &objc2::MainThreadMarker,
        menu_state: &mut crate::desktop::shell2::macos::menu::MenuState,
    ) {
        use objc2_app_kit::{NSMenu, NSMenuItem};
        use objc2_foundation::NSString;

        for item in items {
            match item {
                azul_core::menu::MenuItem::String(string_item) => {
                    let menu_item = NSMenuItem::new(*mtm);
                    let title = NSString::from_str(&string_item.label);
                    menu_item.setTitle(&title);

                    // Set enabled/disabled state based on MenuItemState
                    let enabled = match string_item.menu_item_state {
                        azul_core::menu::MenuItemState::Normal => true,
                        azul_core::menu::MenuItemState::Disabled => false,
                        azul_core::menu::MenuItemState::Greyed => false,
                    };
                    menu_item.setEnabled(enabled);

                    // Check if this item has children (submenu)
                    if !string_item.children.as_ref().is_empty() {
                        // Create submenu and recurse
                        let submenu = NSMenu::new(*mtm);
                        let submenu_title = NSString::from_str(&string_item.label);
                        submenu.setTitle(&submenu_title);

                        // Recursively build submenu items
                        Self::recursive_build_nsmenu(
                            &submenu,
                            string_item.children.as_ref(),
                            mtm,
                            menu_state,
                        );

                        // Attach submenu to menu item
                        menu_item.setSubmenu(Some(&submenu));

                        log_debug!(
                            LogCategory::Input,
                            "[Context Menu] Created submenu '{}' with {} items",
                            string_item.label,
                            string_item.children.as_ref().len()
                        );
                    } else {
                        use crate::desktop::shell2::macos::menu;
                        // Leaf item - wire up callback using the same system as menu bar
                        if let Some(callback) = string_item.callback.as_option() {
                            let tag = menu_state.register_callback(callback.clone());
                            menu_item.setTag(tag as isize);

                            // Use shared AzulMenuTarget for callback dispatch
                            let target = menu::AzulMenuTarget::shared_instance(*mtm);
                            unsafe {
                                menu_item.setTarget(Some(&target));
                                menu_item.setAction(Some(objc2::sel!(menuItemAction:)));
                            }
                        }

                        // Set keyboard shortcut if present
                        if let Some(ref accelerator) = string_item.accelerator.into_option() {
                            menu::set_menu_item_accelerator(&menu_item, accelerator);
                        }
                    }

                    menu.addItem(&menu_item);
                }

                azul_core::menu::MenuItem::Separator => {
                    let separator = unsafe { NSMenuItem::separatorItem(*mtm) };
                    menu.addItem(&separator);
                }

                azul_core::menu::MenuItem::BreakLine => {
                    // BreakLine is for horizontal menu layouts, not supported in NSMenu
                    // Just add a separator as a visual indication
                    let separator = unsafe { NSMenuItem::separatorItem(*mtm) };
                    menu.addItem(&separator);
                }
            }
        }
    }

    // Helper Functions for V2 Event System

    /// Update hit test at given position and store in current_window_state.
    fn update_hit_test(&mut self, position: LogicalPosition) {
        if let Some(layout_window) = self.layout_window.as_mut() {
            let cursor_position = CursorPosition::InWindow(position);
            // Get focused node from FocusManager
            let focused_node = layout_window.focus_manager.get_focused_node().copied();
            let hit_test = crate::desktop::wr_translate2::fullhittest_new_webrender(
                &*self.hit_tester.resolve(),
                self.document_id,
                focused_node,
                &layout_window.layout_results,
                &cursor_position,
                self.current_window_state.size.get_hidpi_factor(),
            );
            use azul_layout::managers::hover::InputPointId;
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    /// Get the first hovered node from current mouse hit test.
    fn get_first_hovered_node(&self) -> Option<HitTestNode> {
        use azul_layout::managers::hover::InputPointId;
        self.layout_window
            .as_ref()?
            .hover_manager
            .get_current(&InputPointId::Mouse)?
            .hovered_nodes
            .iter()
            .flat_map(|(dom_id, ht)| {
                ht.regular_hit_test_nodes
                    .keys()
                    .next()
                    .map(|node_id| HitTestNode {
                        dom_id: dom_id.inner as u64,
                        node_id: node_id.index() as u64,
                    })
            })
            .next()
    }

    /// Convert ProcessEventResult to EventProcessResult for old API compatibility.
    fn process_callback_result_to_event_result_v2(
        &self,
        result: ProcessEventResult,
    ) -> EventProcessResult {
        Self::convert_process_result(result)
    }

    // V2 Cross-Platform Event Processing
    // NOTE: All V2 event processing methods are now provided by the
    // PlatformWindowV2 trait in common/event_v2.rs. The trait provides:
    // - process_window_events_v2() - Entry point (public API)
    // - process_window_events_recursive_v2() - Recursive processing
    // - invoke_callbacks_v2() - Required method (implemented in mod.rs)
    // - process_callback_result_v2() - Result handling
    // This eliminates ~336 lines of platform-specific duplicated code.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keycode_conversion() {
        // Test some basic keycodes
        assert_eq!(Some(VirtualKeyCode::A), convert_keycode_test(0x00));
        assert_eq!(Some(VirtualKeyCode::Return), convert_keycode_test(0x24));
        assert_eq!(Some(VirtualKeyCode::Space), convert_keycode_test(0x31));
        assert_eq!(None, convert_keycode_test(0xFF)); // Invalid
    }

    fn convert_keycode_test(keycode: u16) -> Option<VirtualKeyCode> {
        // Helper for testing keycode conversion without MacOSWindow instance
        match keycode {
            0x00 => Some(VirtualKeyCode::A),
            0x24 => Some(VirtualKeyCode::Return),
            0x31 => Some(VirtualKeyCode::Space),
            _ => None,
        }
    }
}

```

## dll/src/desktop/shell2/common/event_v2.rs
// Event processing v2 - callbacks, text input
// 3448 lines

```rust
//! Cross-platform V2 event processing system
//!
//! This module contains the **complete unified event processing logic** that is shared across all
//! platforms (macOS, Windows, X11, Wayland). The V2 system uses state-diffing between frames to
//! detect events, eliminating platform-specific event handling differences.
//!
//! ## Architecture
//!
//! The `PlatformWindowV2` trait provides **default implementations** for all complex logic:
//! - Event processing (state diffing via `process_window_events()`)
//! - Callback invocation (`invoke_callbacks_v2()`)
//! - Callback result handling (`process_callback_result_v2()`)
//! - Hit testing (`perform_scrollbar_hit_test()`)
//! - Scrollbar interaction (`handle_scrollbar_click()`, `handle_scrollbar_drag()`)
//!
//! Platform implementations only need to:
//! 1. Implement simple getter methods to access their window state
//! 2. Call `process_window_events()` after updating platform state
//! 3. Update the screen based on the returned `ProcessEventResult`
//!
//! ## Event Processing Flow
//!
//! ```text
//! Platform Input → Update Window State → Update Hit Tests → process_window_events()
//!                                                                      ↓
//!                                      ┌───────────────────────────────┘
//!                                      ↓
//!                          PRE-EVENT-DISPATCH PROCESSING
//! =
//!                          1. Scroll: record_sample() on ScrollManager
//!                          2. Text: process_text_input() on LayoutWindow
//!                          3. A11y: record_state_changes() on A11yManager
//!                          ↓
//!                          EVENT FILTERING & DISPATCH
//! =
//!                          4. State diffing (window_state::create_events_from_states)
//!                          5. Event filtering (dispatch_events)
//!                          6. Callback invocation (invoke_callbacks_v2)
//!                          ↓
//!                          POST-CALLBACK PROCESSING
//! =
//!                          7. Process callback results (update DOM, layout, etc.)
//!                          8. Re-layout if necessary
//!                          9. Mark dirty nodes for re-render
//! ```
//!
//! ## Platform Integration Points
//!
//! ### macOS (dll/src/desktop/shell2/macos/events.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In each native event handler AFTER updating `current_window_state`
//! - Examples:
//!   - `handle_mouse_down()` - After setting mouse button state and hit test
//!   - `handle_mouse_up()` - After clearing mouse button state
//!   - `handle_mouse_moved()` - After updating cursor position and hit test
//!   - `handle_key_down()` - After updating keyboard state
//!   - `handle_scroll()` - After calling scroll_manager.record_sample()
//!   - `handle_text_input()` - Platform should provide text_input: &str to process_text_input()
//!   - `handle_window_resize()` - After updating size in window state
//!
//! **Hit-Testing Requirements:**
//! - Call `update_hit_test()` before `process_window_events()` for mouse/touch events
//! - Hit test updates `hover_manager.push_hit_test(InputPointId::Mouse, hit_test)`
//! - For multi-touch: call for each touch with `InputPointId::Touch(id)`
//!
//! **Scroll Integration:**
//! - Get scroll delta from NSEvent
//! - Call `scroll_manager.record_sample(delta_x, delta_y, hover_manager, input_id, now)`
//! - ScrollManager finds scrollable node via hit test and applies scroll
//! - Then call `process_window_events()` which will generate scroll events
//!
//! **Text Input Integration:**
//! - Get composed text from NSTextInputClient (insertText/setMarkedText)
//! - Platform should store text_input string temporarily
//! - `process_window_events()` will call `process_text_input(text_input)`
//! - Framework applies edit, updates cursor, marks nodes dirty
//!
//! **Peculiarities:**
//! - Uses NSEvent for native input
//! - Hit-testing done via `update_hit_test()` before processing
//! - Scrollbar drag state stored in window struct
//! - Must call `present()` for RequestRedraw results
//!
//! ### Windows (dll/src/desktop/shell2/windows/mod.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In WndProc message handlers AFTER updating `current_window_state`
//! - Examples:
//!   - `WM_LBUTTONDOWN/WM_RBUTTONDOWN` - After setting mouse state
//!   - `WM_LBUTTONUP/WM_RBUTTONUP` - After clearing mouse state
//!   - `WM_MOUSEMOVE` - After updating cursor position
//!   - `WM_KEYDOWN/WM_KEYUP` - After updating keyboard state
//!   - `WM_MOUSEWHEEL` - After updating scroll delta
//!   - `WM_SIZE` - After updating window size
//!
//! **Peculiarities:**
//! - Uses Win32 message loop (WndProc)
//! - Hit-testing via WebRender on every mouse move
//! - Must handle WM_PAINT separately for rendering
//! - DPI scaling handled via GetDpiForWindow
//!
//! ### X11 (dll/src/desktop/shell2/linux/x11/events.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In event loop AFTER processing XEvent and updating `current_window_state`
//! - Examples:
//!   - `ButtonPress/ButtonRelease` - After setting mouse button state
//!   - `MotionNotify` - After updating cursor position and hit test
//!   - `KeyPress/KeyRelease` - After XIM processing and keyboard state update
//!   - `ConfigureNotify` - After updating window size/position
//!   - `EnterNotify/LeaveNotify` - After updating cursor in/out state
//!
//! **Peculiarities:**
//! - XIM (X Input Method) for international text input
//! - XFilterEvent must be called before processing for IME
//! - Manual coordinate translation (relative to root window)
//! - Expose events trigger redraw separately
//!
//! ### Wayland (dll/src/desktop/shell2/linux/wayland/mod.rs)
//!
//! **Where to call `process_window_events()`:**
//! - In Wayland event handlers AFTER updating `current_window_state`
//! - Examples:
//!   - `wl_pointer::button` - After setting mouse button state
//!   - `wl_pointer::motion` - After updating cursor position
//!   - `wl_keyboard::key` - After updating keyboard state
//!   - `xdg_toplevel::configure` - After updating window size
//!
//! **Peculiarities:**
//! - Compositor-driven (no XY coordinates, uses surface-local coords)
//! - Frame callbacks for rendering synchronization
//! - Client-side decorations (CSD) always enabled
//! - Seat-based input (single seat assumption for now)
//!
//! When migrating a platform to use `PlatformWindowV2`.

use alloc::sync::Arc;
use core::cell::RefCell;
use std::collections::BTreeMap;

use azul_core::{
    callbacks::LayoutCallbackInfo,
    dom::{DomId, NodeId},
    events::{
        CallbackTarget as CoreCallbackTarget, EventFilter, FocusEventFilter, PreCallbackFilterResult,
        ProcessEventResult, SyntheticEvent,
    },
    geom::LogicalPosition,
    gl::*,
    hit_test::{DocumentId, PipelineId},
    id::NodeId as CoreNodeId,
    refany::RefAny,
    resources::{IdNamespace, ImageCache, RendererResources},
    window::RawWindowHandle,
};
use azul_layout::{
    callbacks::{
        CallCallbacksResult, Callback as LayoutCallback, CallbackInfo, ExternalSystemCallbacks,
    },
    event_determination::determine_all_events,
    hit_test::FullHitTest,
    window::{LayoutWindow, ScrollbarDragState},
    window_state::{self, FullWindowState},
};
use rust_fontconfig::FcFontCache;

use crate::desktop::wr_translate2::{self, AsyncHitTester, WrRenderApi};
use crate::{log_debug, log_warn};

/// Maximum depth for recursive event processing (prevents infinite loops from callbacks)
// Event Processing Configuration

/// Maximum recursion depth for event processing.
///
/// Events can trigger callbacks that regenerate the DOM, which triggers new events.
/// This limit prevents infinite loops.
const MAX_EVENT_RECURSION_DEPTH: usize = 7;

/// Unique timer ID for auto-scroll during drag selection.
///
/// This ID is reserved for the framework's auto-scroll timer and should not
/// be used by user code. Value chosen to avoid conflicts with typical timer IDs.
const AUTO_SCROLL_TIMER_ID: usize = 0xABCD_1234;

// Platform-specific Clipboard Helpers

/// Get clipboard text content (platform-specific)
#[inline]
fn get_system_clipboard() -> Option<String> {
    #[cfg(target_os = "windows")]
    {
        crate::desktop::shell2::windows::clipboard::get_clipboard_content()
    }
    #[cfg(target_os = "macos")]
    {
        crate::desktop::shell2::macos::clipboard::get_clipboard_content()
    }
    #[cfg(all(target_os = "linux", feature = "x11"))]
    {
        crate::desktop::shell2::linux::x11::clipboard::get_clipboard_content()
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", feature = "x11")
    )))]
    {
        None
    }
}

/// Set clipboard text content (platform-specific)
#[inline]
fn set_system_clipboard(text: String) -> bool {
    #[cfg(target_os = "windows")]
    {
        use clipboard_win::{formats, set_clipboard};
        set_clipboard(formats::Unicode, &text).is_ok()
    }
    #[cfg(target_os = "macos")]
    {
        crate::desktop::shell2::macos::clipboard::write_to_clipboard(&text).is_ok()
    }
    #[cfg(all(target_os = "linux", feature = "x11"))]
    {
        crate::desktop::shell2::linux::x11::clipboard::write_to_clipboard(&text).is_ok()
    }
    #[cfg(not(any(
        target_os = "windows",
        target_os = "macos",
        all(target_os = "linux", feature = "x11")
    )))]
    {
        false
    }
}

/// Timer callback for auto-scroll during drag selection.
///
/// This callback fires at the monitor's refresh rate during drag-to-scroll operations.
/// It checks if dragging is still active, calculates scroll delta based on mouse distance
/// from container edges, and applies accelerated scrolling.
///
/// The callback terminates automatically when:
/// - Mouse button is released (no longer dragging)
/// - Mouse returns to within container bounds (no scroll needed)
extern "C" fn auto_scroll_timer_callback(
    _data: azul_core::refany::RefAny,
    timer_info: azul_layout::timer::TimerCallbackInfo,
) -> azul_core::callbacks::TimerCallbackReturn {
    use azul_core::task::TerminateTimer;
    use azul_layout::window::SelectionScrollType;

    // Access window state through callback_info
    let callback_info = &timer_info.callback_info;

    // Access window state through callback_info
    let callback_info = &timer_info.callback_info;

    // Get current mouse position from window state (safe access via public getter)
    let full_window_state = callback_info.get_current_window_state();

    // Check if still dragging (left mouse button is down)
    if !full_window_state.mouse_state.left_down {
        // Mouse released - stop timer
        return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
    }

    // Get mouse position - if mouse is outside window, terminate timer
    let mouse_position = match full_window_state.mouse_state.cursor_position.get_position() {
        Some(pos) => pos,
        None => {
            // Mouse outside window - stop auto-scroll
            return azul_core::callbacks::TimerCallbackReturn::terminate_unchanged();
        }
    };

    // TODO: Scroll based on mouse distance from container edge
    // The issue is that scroll_selection_into_view requires &mut LayoutWindow,
    // but we only have &CallbackInfo which has *const LayoutWindow.
    // We need to either:
    // 1. Make scroll_selection_into_view work via CallbackChange transaction
    // 2. Provide a different API for timer callbacks to access mutable state
    // For now, just continue the timer without scrolling
    //
    // let layout_window = timer_info.callback_info.get_layout_window();
    // if layout_window.scroll_selection_into_view(
    //     SelectionScrollType::DragSelection { mouse_position },
    //     ScrollMode::Accelerated,
    // ) {
    //     return azul_core::callbacks::TimerCallbackReturn::continue_and_update();
    // }

    // No scroll needed (mouse within container or no scrollable ancestor)
    // Continue timer in case mouse moves outside again
    azul_core::callbacks::TimerCallbackReturn::continue_unchanged()
}

// Focus Restyle Helper

/// Apply focus change restyle and determine the ProcessEventResult.
///
/// This helper function consolidates the duplicated restyle logic that was
/// previously repeated for FocusNext/Previous/First/Last and ClearFocus handlers.
///
/// # Arguments
/// * `layout_window` - Mutable reference to the layout window
/// * `old_focus` - The node that is losing focus (if any)
/// * `new_focus` - The node that is gaining focus (if any)
///
/// # Returns
/// The appropriate ProcessEventResult based on what CSS properties changed.
fn apply_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<NodeId>,
    new_focus: Option<NodeId>,
) -> ProcessEventResult {
    use azul_core::styled_dom::FocusChange;
    
    // Get the first (primary) layout result
    let Some((_, layout_result)) = layout_window.layout_results.iter_mut().next() else {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    };
    
    // Apply restyle for focus change
    let restyle_result = layout_result.styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: old_focus,
            gained_focus: new_focus,
        }),
        None, // hover
        None, // active
    );
    
    log_debug!(
        super::debug_server::LogCategory::Input,
        "[Event V2] Focus restyle: needs_layout={}, needs_display_list={}, changed_nodes={}",
        restyle_result.needs_layout,
        restyle_result.needs_display_list,
        restyle_result.changed_nodes.len()
    );
    
    // Determine ProcessEventResult based on what changed
    if restyle_result.needs_layout {
        ProcessEventResult::ShouldRegenerateDomCurrentWindow
    } else if restyle_result.needs_display_list {
        ProcessEventResult::ShouldUpdateDisplayListCurrentWindow
    } else {
        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}

// Platform-Specific Timer Management

/// Hit test node structure for event routing.
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct HitTestNode {
    pub dom_id: u64,
    pub node_id: u64,
}

/// Target for callback dispatch - either a specific node or all root nodes.
#[derive(Debug, Clone, Copy)]
pub enum CallbackTarget {
    /// Dispatch to callbacks on a specific node (e.g., mouse events, hover)
    Node(HitTestNode),
    /// Dispatch to callbacks on root nodes (NodeId::ZERO) across all DOMs (e.g., window events,
    /// keys)
    RootNodes,
}

/// Borrowed resources needed for `invoke_single_callback`.
///
/// This struct borrows individual fields from the window, allowing the borrow checker
/// to see that we're borrowing distinct fields rather than `&mut self` multiple times.
/// This avoids borrow checker conflicts when calling trait methods.
pub struct InvokeSingleCallbackBorrows<'a> {
    /// Mutable layout window for callback invocation
    pub layout_window: &'a mut LayoutWindow,
    /// Raw window handle for platform identification
    pub window_handle: RawWindowHandle,
    /// OpenGL context pointer
    pub gl_context_ptr: &'a OptionGlContextPtr,
    /// Mutable image cache
    pub image_cache: &'a mut ImageCache,
    /// Cloned font cache (FcFontCache doesn't support &mut access)
    pub fc_cache_clone: FcFontCache,
    /// System style (Arc, cheap to clone)
    pub system_style: Arc<azul_css::system::SystemStyle>,
    /// Previous window state
    pub previous_window_state: &'a Option<FullWindowState>,
    /// Current window state
    pub current_window_state: &'a FullWindowState,
    /// Renderer resources
    pub renderer_resources: &'a mut RendererResources,
}

/// Trait that platform-specific window types must implement to use the unified V2 event system.
///
/// This trait provides **default implementations** for all complex cross-platform logic.
/// Platform implementations only need to implement the simple getter methods (27 methods).
///
/// ## Required Methods (Simple Getters - 27 total)
///
/// Platforms must implement these methods to expose their internal state:
/// - Layout window access (`get_layout_window`, `get_layout_window_mut`)
/// - Window state access (`get_current_window_state`, `get_previous_window_state`, etc.)
/// - Resource access (`get_image_cache_mut`, `get_renderer_resources_mut`, etc.)
/// - Hit testing state (`get_hit_tester`, `get_scrollbar_drag_state`, etc.)
/// - Frame regeneration (`needs_frame_regeneration`, `mark_frame_needs_regeneration`, etc.)
/// - Raw window handle (`get_raw_window_handle`)
/// - **Callback preparation (`prepare_callback_invocation`)** - Returns all borrows needed for
///   callbacks
///
/// ## Provided Methods (Complete Logic - All Cross-Platform!)
///
/// These methods have default implementations with the full cross-platform logic:
/// - `invoke_callbacks_v2()` - **FULLY CROSS-PLATFORM!** Callback dispatch using
///   `prepare_callback_invocation()`
/// - `process_window_events_recursive_v2()` - Main event processing with recursion
/// - `process_callback_result_v2()` - Handle callback results
/// - `perform_scrollbar_hit_test()` - Scrollbar interaction
/// - `handle_scrollbar_click()` - Scrollbar click handling
/// - `handle_scrollbar_drag()` - Scrollbar drag handling
/// - `gpu_scroll()` - GPU-accelerated smooth scrolling
///
/// ## Platform Implementation Checklist
///
/// To integrate a new platform:
/// 1. Implement the 26 required getter methods
/// 2. Import the trait: `use crate::desktop::shell2::common::event_v2::PlatformWindowV2;`
/// 3. Call `self.process_window_events_recursive_v2(0)` after updating window state
/// 4. Done! All event processing is now unified.
pub trait PlatformWindowV2 {
    // REQUIRED: Simple Getter Methods (Platform Must Implement)

    // Layout Window Access

    /// Get mutable access to the layout window
    fn get_layout_window_mut(&mut self) -> Option<&mut LayoutWindow>;

    /// Get immutable access to the layout window
    fn get_layout_window(&self) -> Option<&LayoutWindow>;

    // Window State Access

    /// Get the current window state
    fn get_current_window_state(&self) -> &FullWindowState;

    /// Get mutable access to the current window state
    fn get_current_window_state_mut(&mut self) -> &mut FullWindowState;

    /// Get the previous window state (if available)
    fn get_previous_window_state(&self) -> &Option<FullWindowState>;

    /// Set the previous window state
    fn set_previous_window_state(&mut self, state: FullWindowState);

    // Resource Access

    /// Get mutable access to image cache
    fn get_image_cache_mut(&mut self) -> &mut ImageCache;

    /// Get mutable access to renderer resources
    fn get_renderer_resources_mut(&mut self) -> &mut RendererResources;

    /// Get the font cache
    fn get_fc_cache(&self) -> &Arc<FcFontCache>;

    /// Get the OpenGL context pointer
    fn get_gl_context_ptr(&self) -> &OptionGlContextPtr;

    /// Get the system style
    fn get_system_style(&self) -> &Arc<azul_css::system::SystemStyle>;

    /// Get the shared application data
    fn get_app_data(&self) -> &Arc<RefCell<RefAny>>;

    // Scrollbar State

    /// Get the current scrollbar drag state
    fn get_scrollbar_drag_state(&self) -> Option<&ScrollbarDragState>;

    /// Get mutable access to scrollbar drag state
    fn get_scrollbar_drag_state_mut(&mut self) -> &mut Option<ScrollbarDragState>;

    /// Set scrollbar drag state
    fn set_scrollbar_drag_state(&mut self, state: Option<ScrollbarDragState>);

    // Hit Testing

    /// Get the async hit tester
    fn get_hit_tester(&self) -> &AsyncHitTester;

    /// Get mutable access to hit tester
    fn get_hit_tester_mut(&mut self) -> &mut AsyncHitTester;

    /// Get the last hovered node
    fn get_last_hovered_node(&self) -> Option<&HitTestNode>;

    /// Set the last hovered node
    fn set_last_hovered_node(&mut self, node: Option<HitTestNode>);

    // WebRender Infrastructure

    /// Get the document ID
    fn get_document_id(&self) -> DocumentId;

    /// Get the ID namespace
    fn get_id_namespace(&self) -> IdNamespace;

    /// Get the render API
    fn get_render_api(&self) -> &WrRenderApi;

    /// Get mutable access to render API
    fn get_render_api_mut(&mut self) -> &mut WrRenderApi;

    /// Get the renderer (if available)
    fn get_renderer(&self) -> Option<&webrender::Renderer>;

    /// Get mutable access to renderer
    fn get_renderer_mut(&mut self) -> Option<&mut webrender::Renderer>;

    // Timers and Threads

    /// Get raw window handle for spawning child windows
    fn get_raw_window_handle(&self) -> RawWindowHandle;

    // Frame Regeneration

    /// Check if frame needs regeneration
    fn needs_frame_regeneration(&self) -> bool;

    /// Mark that the frame needs regeneration
    fn mark_frame_needs_regeneration(&mut self);

    /// Clear frame regeneration flag
    fn clear_frame_regeneration_flag(&mut self);

    // Callback Invocation Preparation

    /// Borrow all resources needed for `invoke_single_callback` in one call.
    ///
    /// This method returns a struct with individual field borrows, allowing the borrow
    /// checker to see that we're borrowing distinct fields rather than `&mut self` multiple times.
    ///
    /// ## Returns
    /// * `InvokeSingleCallbackBorrows` - All borrowed resources needed for callback invocation
    fn prepare_callback_invocation(&mut self) -> InvokeSingleCallbackBorrows;

    // REQUIRED: Timer Management (Platform-Specific Implementation)

    /// Start a timer with the given ID and interval.
    ///
    /// When the timer fires, the platform should tick timers in the layout window
    /// and trigger event processing to invoke timer callbacks.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `SetTimer(hwnd, timer_id, interval_ms, NULL)`
    /// - **macOS**: Use `NSTimer::scheduledTimerWithTimeInterval` with userInfo containing timer_id
    /// - **X11**: Add timer to internal manager, use select() timeout to check expiration
    /// - **Wayland**: Create timerfd with timerfd_create(), add to event loop poll
    ///
    /// ## Parameters
    /// * `timer_id` - Unique timer identifier (from TimerId.id)
    /// * `timer` - Timer configuration with interval and callback info
    fn start_timer(&mut self, timer_id: usize, timer: azul_layout::timer::Timer);

    /// Stop a timer with the given ID.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `KillTimer(hwnd, timer_id)`
    /// - **macOS**: Call `[timer invalidate]` on stored NSTimer
    /// - **X11**: Remove timer from internal manager
    /// - **Wayland**: Close timerfd with close(fd)
    ///
    /// ## Parameters
    /// * `timer_id` - Timer identifier to stop
    fn stop_timer(&mut self, timer_id: usize);

    // REQUIRED: Thread Management (Platform-Specific Implementation)

    /// Start the thread polling timer (typically 16ms interval).
    ///
    /// This timer should check all active threads for completed work and trigger
    /// event processing if any threads have finished.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `SetTimer(hwnd, 0xFFFF, 16, NULL)` with reserved ID 0xFFFF
    /// - **macOS**: Use `NSTimer::scheduledTimerWithTimeInterval` with 0.016 interval
    /// - **X11**: Add 16ms timeout to select() when threads exist
    /// - **Wayland**: Create 16ms timerfd for thread polling
    fn start_thread_poll_timer(&mut self);

    /// Stop the thread polling timer.
    ///
    /// Called when the last thread is removed from the thread pool.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use `KillTimer(hwnd, 0xFFFF)`
    /// - **macOS**: Call `[timer invalidate]` on thread_timer_running
    /// - **X11**: Stop using 16ms timeout in select()
    /// - **Wayland**: Close thread polling timerfd
    fn stop_thread_poll_timer(&mut self);

    /// Add threads to the thread pool.
    ///
    /// Threads are stored in `layout_window.threads` and polled periodically by
    /// the thread polling timer to check for completion.
    ///
    /// ## Parameters
    /// * `threads` - Threads to add to the pool (BTreeMap from CallCallbacksResult)
    fn add_threads(
        &mut self,
        threads: std::collections::BTreeMap<azul_core::task::ThreadId, azul_layout::thread::Thread>,
    );

    /// Remove threads from the thread pool.
    ///
    /// ## Parameters  
    /// * `thread_ids` - Thread IDs to remove
    fn remove_threads(
        &mut self,
        thread_ids: &std::collections::BTreeSet<azul_core::task::ThreadId>,
    );

    // REQUIRED: Menu Display (Platform-Specific Implementation)

    /// Show a menu at the specified position.
    ///
    /// This method is called when a callback uses `info.open_menu()` or `info.open_menu_at()`.
    /// The platform should display the menu either as a native menu or a fallback DOM-based menu
    /// depending on the window's `use_native_context_menus` flag.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **macOS**: Use NSMenu with popUpMenuPositioningItem or show fallback window
    /// - **Windows**: Use TrackPopupMenu or show fallback window
    /// - **X11**: Create GTK popup menu or show fallback window
    /// - **Wayland**: Use xdg_popup protocol or show fallback window
    ///
    /// ## Parameters
    /// * `menu` - The menu structure to display
    /// * `position` - The position where the menu should appear (logical coordinates)
    fn show_menu_from_callback(
        &mut self,
        menu: &azul_core::menu::Menu,
        position: azul_core::geom::LogicalPosition,
    );

    // REQUIRED: Tooltip Display (Platform-Specific Implementation)

    /// Show a tooltip with the given text at the specified position.
    ///
    /// This method is called when a callback uses `info.show_tooltip()` or
    /// `info.show_tooltip_at()`. The platform should display a native tooltip at the given
    /// position.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use TOOLTIPS_CLASS with TTM_TRACKACTIVATE
    /// - **macOS**: Use NSPopover with NSViewController
    /// - **X11**: Create transient window with override_redirect
    /// - **Wayland**: Use zwlr_layer_shell_v1 for tooltip surface
    ///
    /// ## Parameters
    /// * `text` - The tooltip text to display
    /// * `position` - The position where the tooltip should appear (logical coordinates)
    fn show_tooltip_from_callback(
        &mut self,
        text: &str,
        position: azul_core::geom::LogicalPosition,
    );

    /// Hide the currently displayed tooltip.
    ///
    /// This method is called when a callback uses `info.hide_tooltip()`.
    /// The platform should hide any currently displayed tooltip.
    ///
    /// ## Platform Implementation Notes
    ///
    /// - **Windows**: Use TTM_TRACKACTIVATE with FALSE
    /// - **macOS**: Call [popover close]
    /// - **X11**: Unmap the tooltip window
    /// - **Wayland**: Destroy the tooltip surface
    fn hide_tooltip_from_callback(&mut self);

    // PROVIDED: Hit Testing (Cross-Platform Implementation)

    /// Update hit test at given position and store in hover manager.
    ///
    /// This method performs WebRender hit testing at the given logical position
    /// and updates the HoverManager with the results. This is needed for:
    /// - Normal mouse movement events (platform calls this)
    /// - Synthetic mouse events from debug API (process_callback_result_v2 calls this)
    ///
    /// ## Parameters
    /// * `position` - The logical position to hit test at
    fn update_hit_test_at(&mut self, position: azul_core::geom::LogicalPosition) {
        use azul_core::window::CursorPosition;
        use azul_layout::managers::hover::InputPointId;

        let document_id = self.get_document_id();
        let hidpi_factor = self.get_current_window_state().size.get_hidpi_factor();

        // Get focused node before borrowing layout_window
        let focused_node = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        // Check if layout window exists
        let has_layout_window = self.get_layout_window().is_some();
        if !has_layout_window {
            return;
        }

        // Resolve hit tester first (this mutates self.hit_tester from Requested to Resolved)
        let resolved_hit_tester = self.get_hit_tester_mut().resolve();

        // Now get layout window immutably for hit testing
        let hit_test = {
            let layout_window = self.get_layout_window().unwrap();

            crate::desktop::wr_translate2::fullhittest_new_webrender(
                &*resolved_hit_tester,
                document_id,
                focused_node,
                &layout_window.layout_results,
                &CursorPosition::InWindow(position),
                hidpi_factor,
            )
        };

        // Store hit test in hover manager
        if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window
                .hover_manager
                .push_hit_test(InputPointId::Mouse, hit_test);
        }
    }

    // PROVIDED: Callback Invocation (Cross-Platform Implementation)

    /// Invoke callbacks for a given target and event filter.
    ///
    /// This method is now **provided** (cross-platform) because all required state
    /// is accessible through trait getter methods. No platform-specific code needed!
    ///
    /// ## Workflow
    /// 1. Collect callbacks from NodeData based on target (Node or RootNodes)
    /// 2. Filter callbacks by event type
    /// 3. Build an event chain from target node up to root (JS-style bubbling)
    /// 4. Invoke callbacks in bubbling order, stopping if stopPropagation() is called
    /// 5. Return all callback results
    ///
    /// ## Event Bubbling
    /// For hover events (clicks, mouse moves, etc.), this implements JavaScript-style
    /// event bubbling:
    /// 1. Find the deepest (target) node that was hit
    /// 2. Build a chain: target → parent → grandparent → ... → root
    /// 3. Invoke callbacks at each level in order
    /// 4. Stop propagation if a callback calls `stop_propagation()`
    ///
    /// ## Returns
    /// * `Vec<CallCallbacksResult>` - Results from all invoked callbacks
    fn invoke_callbacks_v2(
        &mut self,
        target: CallbackTarget,
        event_filter: EventFilter,
    ) -> Vec<CallCallbacksResult> {
        use azul_core::{
            callbacks::CoreCallbackData,
            dom::{DomId, NodeId},
            id::NodeId as CoreNodeId,
        };

        // Internal struct to track callback with its source node for bubbling
        #[derive(Clone)]
        struct NodeCallback {
            dom_id: DomId,
            node_id: NodeId,
            depth: usize, // 0 = target (deepest), higher = closer to root
            callback: CoreCallbackData,
        }

        // Collect callbacks based on target, now with node info for bubbling
        let node_callbacks: Vec<NodeCallback> = match target {
            CallbackTarget::Node(node) => {
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return Vec::new(),
                };

                let dom_id = DomId {
                    inner: node.dom_id as usize,
                };
                // Note: node.node_id is 0-based, use NodeId::new() directly instead of from_usize
                // from_usize expects 1-based encoding (0=None, n=NodeId(n-1))
                let node_id = NodeId::new(node.node_id as usize);

                let layout_result = match layout_window.layout_results.get(&dom_id) {
                    Some(lr) => lr,
                    None => return Vec::new(),
                };

                let binding = layout_result.styled_dom.node_data.as_container();
                let node_data = match binding.get(node_id) {
                    Some(nd) => nd,
                    None => return Vec::new(),
                };

                // For targeted node, just collect its callbacks (no bubbling for explicit target)
                node_data
                    .get_callbacks()
                    .as_container()
                    .iter()
                    .filter(|cd| cd.event == event_filter)
                    .map(|cb| NodeCallback {
                        dom_id,
                        node_id,
                        depth: 0,
                        callback: cb.clone(),
                    })
                    .collect()
            }
            CallbackTarget::RootNodes => {
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return Vec::new(),
                };

                let mut node_callbacks = Vec::new();

                // Check if this is a HoverEventFilter - if so, implement event bubbling
                let is_hover_event = matches!(event_filter, EventFilter::Hover(_));

                if is_hover_event {
                    // For hover events, implement JS-style event bubbling:
                    // Find deepest hit node, then bubble up to root
                    use azul_layout::managers::hover::InputPointId;

                    if let Some(hit_test) = layout_window
                        .hover_manager
                        .get_current(&InputPointId::Mouse)
                    {
                        for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                            if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                                let node_data_container =
                                    layout_result.styled_dom.node_data.as_container();
                                let node_hierarchy =
                                    layout_result.styled_dom.node_hierarchy.as_container();

                                // Find the deepest hit node (target)
                                // In regular_hit_test_nodes, the last node is typically the deepest
                                // but we should find the one with the maximum depth
                                let deepest_node = hit_test_data
                                    .regular_hit_test_nodes
                                    .iter()
                                    .max_by_key(|(node_id, _)| {
                                        // Count depth by traversing to root
                                        let mut depth = 0usize;
                                        let mut current = Some(**node_id);
                                        while let Some(nid) = current {
                                            depth += 1;
                                            current =
                                                node_hierarchy.get(nid).and_then(|h| h.parent_id());
                                        }
                                        depth
                                    });

                                if let Some((target_node_id, _)) = deepest_node {
                                    // Build event chain: target → parent → ... → root
                                    let mut current_node = Some(*target_node_id);
                                    let mut depth = 0usize;

                                    while let Some(node_id) = current_node {
                                        // Collect callbacks from this node
                                        if let Some(node_data) = node_data_container.get(node_id) {
                                            for callback in node_data.get_callbacks().iter() {
                                                if callback.event == event_filter {
                                                    node_callbacks.push(NodeCallback {
                                                        dom_id: *dom_id,
                                                        node_id,
                                                        depth,
                                                        callback: callback.clone(),
                                                    });
                                                }
                                            }
                                        }

                                        // Move to parent
                                        current_node =
                                            node_hierarchy.get(node_id).and_then(|h| h.parent_id());
                                        depth += 1;
                                    }
                                }
                            }
                        }
                    }
                } else {
                    // For non-hover events (window events, etc.), search only root nodes
                    for (dom_id, layout_result) in &layout_window.layout_results {
                        if let Some(root_node) = layout_result
                            .styled_dom
                            .node_data
                            .as_container()
                            .get(CoreNodeId::ZERO)
                        {
                            for callback in root_node.get_callbacks().iter() {
                                if callback.event == event_filter {
                                    let node_id = match NodeId::from_usize(0) {
                                        Some(nid) => nid,
                                        None => continue,
                                    };
                                    node_callbacks.push(NodeCallback {
                                        dom_id: *dom_id,
                                        node_id,
                                        depth: 0,
                                        callback: callback.clone(),
                                    });
                                }
                            }
                        }
                    }
                }
                node_callbacks
            }
        };

        if node_callbacks.is_empty() {
            return Vec::new();
        }

        // Sort by depth (0 = target first, then parents)
        // This ensures JS-style bubbling order: target → parent → grandparent → root
        let mut sorted_callbacks = node_callbacks;
        sorted_callbacks.sort_by_key(|nc| nc.depth);

        // Prepare all borrows in one call - avoids multiple &mut self borrows
        let mut borrows = self.prepare_callback_invocation();

        let mut results = Vec::new();

        for node_callback in sorted_callbacks {
            let mut callback = LayoutCallback::from_core(node_callback.callback.callback);

            let callback_result = borrows.layout_window.invoke_single_callback(
                &mut callback,
                &mut node_callback.callback.refany.clone(),
                &borrows.window_handle,
                borrows.gl_context_ptr,
                borrows.image_cache,
                &mut borrows.fc_cache_clone,
                borrows.system_style.clone(),
                &ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            // Check if stopPropagation() was called - if so, stop bubbling
            let should_stop = callback_result.stop_propagation;

            results.push(callback_result);

            if should_stop {
                // Stop event propagation - don't invoke callbacks on parent nodes
                break;
            }
        }

        results
    }

    // PROVIDED: Complete Logic (Default Implementations)

    /// GPU-accelerated smooth scrolling.
    ///
    /// This applies a scroll delta to a node and updates WebRender's display list
    /// for smooth GPU-based scrolling.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM ID containing the scrollable node
    /// * `node_id` - The scrollable node ID
    /// * `delta_x` - Horizontal scroll delta (pixels)
    /// * `delta_y` - Vertical scroll delta (pixels)
    ///
    /// ## Returns
    /// * `Ok(())` - Scroll applied successfully
    /// * `Err(msg)` - Error message if scroll failed
    fn gpu_scroll(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        delta_x: f32,
        delta_y: f32,
    ) -> Result<(), String> {
        use azul_core::{
            events::{EasingFunction, EventSource},
            geom::LogicalPosition,
        };
        use azul_layout::managers::scroll_state::ScrollEvent;

        let layout_window = self.get_layout_window_mut().ok_or("No layout window")?;

        // Create scroll event
        let scroll_event = ScrollEvent {
            dom_id,
            node_id,
            delta: LogicalPosition::new(delta_x, delta_y),
            source: EventSource::User,
            duration: None, // Instant scroll
            easing: EasingFunction::Linear,
        };

        let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();

        // Apply scroll
        layout_window.scroll_manager.scroll_by(
            scroll_event.dom_id,
            scroll_event.node_id,
            scroll_event.delta,
            scroll_event
                .duration
                .unwrap_or(azul_core::task::Duration::System(
                    azul_core::task::SystemTimeDiff { secs: 0, nanos: 0 },
                )),
            scroll_event.easing,
            (external.get_system_time_fn.cb)(),
        );

        self.mark_frame_needs_regeneration();
        Ok(())
    }

    // PROVIDED: Input Recording for Gesture Detection

    /// Record input sample for gesture detection.
    ///
    /// Call this from platform event handlers to feed input data into the gesture manager:
    /// - On mouse button down: Start new session
    /// - On mouse move (while button down): Record movement
    /// - On mouse button up: End session
    ///
    /// The gesture manager will analyze these samples to detect:
    /// - Drags (movement beyond threshold)
    /// - Double-clicks (two clicks within time/distance)
    /// - Long-presses (button held down without much movement)
    ///
    /// ## Parameters
    /// - `position`: Current mouse position in logical coordinates
    /// - `button_state`: Button state bitfield (0x01=left, 0x02=right, 0x04=middle)
    /// - `is_button_down`: Whether a button was just pressed (starts new session)
    /// - `is_button_up`: Whether a button was just released (ends session)
    fn record_input_sample(
        &mut self,
        position: azul_core::geom::LogicalPosition,
        button_state: u8,
        is_button_down: bool,
        is_button_up: bool,
    ) {
        // Get access to gesture manager
        let layout_window = match self.get_layout_window_mut() {
            Some(lw) => lw,
            None => return,
        };

        // Get current time (platform-specific, use system clock)
        #[cfg(feature = "std")]
        let current_time = azul_core::task::Instant::from(std::time::Instant::now());

        #[cfg(not(feature = "std"))]
        let current_time = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));

        let manager = &mut layout_window.gesture_drag_manager;

        // Record based on event type
        if is_button_down {
            // Start new input session
            manager.start_input_session(position, current_time.clone(), button_state);
        } else if is_button_up {
            // End current session
            manager.end_current_session();
        } else {
            // Record ongoing movement
            manager.record_input_sample(position, current_time.clone(), button_state);
        }

        // Periodically clear old samples (every frame is fine)
        manager.clear_old_sessions(current_time);
    }

    // PROVIDED: Event Processing (Cross-Platform Implementation)

    /// V2: Record accessibility action and return affected nodes.
    ///
    /// Similar to `record_input_sample()` for gestures, this method takes an incoming
    /// accessibility action from assistive technologies (screen readers), applies
    /// necessary state changes to managers (scroll, focus, cursor, selection), and
    /// returns information about which nodes were affected.
    ///
    /// ## Workflow
    /// 1. Apply manager state changes (focus, scroll, cursor, selection)
    /// 2. Generate synthetic EventFilters for callback actions
    /// 3. Return map of affected nodes with events and dirty flags
    ///
    /// ## Parameters
    /// * `dom_id` - DOM containing the target node
    /// * `node_id` - Target node for the action
    /// * `action` - Accessibility action from screen reader
    ///
    /// ## Returns
    /// * `BTreeMap<DomNodeId, (Vec<EventFilter>, bool)>` - Map of:
    ///   - Key: Affected node
    ///   - Value: (Synthetic events to dispatch, needs_relayout flag)
    ///   - Empty map = action not applicable or nothing changed
    #[cfg(feature = "a11y")]
    fn record_accessibility_action(
        &mut self,
        dom_id: azul_core::dom::DomId,
        node_id: azul_core::dom::NodeId,
        action: azul_core::dom::AccessibilityAction,
    ) -> BTreeMap<azul_core::dom::DomNodeId, (Vec<EventFilter>, bool)> {
        use std::collections::BTreeMap;

        let layout_window = match self.get_layout_window_mut() {
            Some(lw) => lw,
            None => return BTreeMap::new(),
        };

        let now = std::time::Instant::now();

        // Delegate to LayoutWindow's process_accessibility_action
        // This has direct mutable access to all managers and returns affected nodes
        layout_window.process_accessibility_action(dom_id, node_id, action, now)
    }

    /// Process all window events using the V2 state-diffing system.
    ///
    /// V2: Main entry point for processing window events.
    ///
    /// This is the **main entry point** for event processing. Call this after updating
    /// the current window state with platform events.
    ///
    /// ## Workflow
    /// 1. Compare current vs previous window state to detect events
    /// 2. Use `dispatch_events()` to determine which callbacks to invoke
    /// 3. Invoke callbacks and collect results
    /// 4. Handle callback results (regenerate DOM, update display list, etc.)
    /// 5. Recurse if needed (DOM was regenerated)
    ///
    /// ## Returns
    /// * `ProcessEventResult` - Tells the platform what action to take (redraw, close, etc.)
    ///
    /// ## Implementation
    /// Recursively processes events with depth limiting (max 5 levels) to prevent
    /// infinite loops from callbacks that regenerate the DOM.
    fn process_window_events_recursive_v2(&mut self, depth: usize) -> ProcessEventResult {
        
        if depth >= MAX_EVENT_RECURSION_DEPTH {
            log_warn!(
                super::debug_server::LogCategory::EventLoop,
                "[PlatformWindowV2] Max event recursion depth {} reached",
                MAX_EVENT_RECURSION_DEPTH
            );
            return ProcessEventResult::DoNothing;
        }

        // Get previous state (or use current as fallback for first frame)
        let has_previous = self.get_previous_window_state().is_some();
        let previous_state = self
            .get_previous_window_state()
            .as_ref()
            .unwrap_or(self.get_current_window_state());

        let current_state = self.get_current_window_state();

        // DEBUG: Print state comparison for mouse buttons

        // Get gesture manager for gesture detection (if available)
        let gesture_manager = self.get_layout_window().map(|lw| &lw.gesture_drag_manager);

        // Detect all events that occurred by comparing states
        // Using new SyntheticEvent architecture with determine_all_events()

        // Get managers for event detection
        let focus_manager = self.get_layout_window().map(|w| &w.focus_manager);
        let file_drop_manager = self.get_layout_window().map(|w| &w.file_drop_manager);
        let hover_manager = self.get_layout_window().map(|w| &w.hover_manager);

        // Get EventProvider managers (scroll, text input, etc.)
        let scroll_manager_ref = self.get_layout_window().map(|w| &w.scroll_manager);
        let text_manager_ref = self.get_layout_window().map(|w| &w.text_input_manager);

        // Build list of EventProvider managers
        let mut event_providers: Vec<&dyn azul_core::events::EventProvider> = Vec::new();
        if let Some(sm) = scroll_manager_ref.as_ref() {
            event_providers.push(*sm as &dyn azul_core::events::EventProvider);
        }
        if let Some(tm) = text_manager_ref.as_ref() {
            event_providers.push(*tm as &dyn azul_core::events::EventProvider);
        }

        // Get current timestamp
        #[cfg(feature = "std")]
        let timestamp = azul_core::task::Instant::from(std::time::Instant::now());
        #[cfg(not(feature = "std"))]
        let timestamp = azul_core::task::Instant::Tick(azul_core::task::SystemTick::new(0));

        // Determine all events (returns Vec<SyntheticEvent>)
        let synthetic_events = if let (Some(fm), Some(fdm), Some(hm)) =
            (focus_manager, file_drop_manager, hover_manager)
        {
            determine_all_events(
                self.get_current_window_state(),
                previous_state,
                hm,
                fm,
                fdm,
                gesture_manager,
                &event_providers,
                timestamp,
            )
        } else {
            // Fallback: no events if managers not available
            Vec::new()
        };

        for (i, ev) in synthetic_events.iter().enumerate() {
        }

        if synthetic_events.is_empty() {
            return ProcessEventResult::DoNothing;
        }

        // Get mouse hit test if available (clone early to avoid borrow conflicts)
        use azul_layout::managers::hover::InputPointId;
        let hit_test_for_dispatch = self
            .get_layout_window()
            .and_then(|lw| lw.hover_manager.get_current(&InputPointId::Mouse))
            .cloned();

        // If DragStart event occurred and we have a hit test, save it in the manager
        // This allows callbacks to query which nodes were hit at drag start
        if synthetic_events
            .iter()
            .any(|e| matches!(e.event_type, azul_core::events::EventType::DragStart))
        {
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Extract first hit from current state (the hovered DOM node)
                // NOTE: With the unified DragContext system, hit tests are stored
                // directly in the DragContext when the drag is activated.
                // No need for separate update_*_hit_test() calls.
                let _hit_test_clone = hit_test_for_dispatch.as_ref().and_then(|ht| {
                    // Get first hovered node's hit test
                    ht.hovered_nodes.values().next().cloned()
                });
            }
        }

        // PRE-EVENT-DISPATCH PROCESSING
        // Process input BEFORE event filtering and callback invocation.
        // This ensures framework state (scroll, text, a11y) is updated before
        // callbacks see the events.
        //
        // IMPORTANT: Hit tests must already be done by platform layer!
        // Platform code should call update_hit_test() before calling this function.
        //
        // IMPLEMENTATION STATUS:
        // [ OK ] Scroll: Platform calls scroll_manager.record_sample() in handle_scroll_wheel()
        // [ OK ] Text: Platform calls process_text_input() in handle_key_down()
        // [ WAIT ] A11y: Not yet implemented (needs a11y_manager.record_state_changes())

        // Process text input BEFORE event dispatch
        // If there's a focused contenteditable node and text input occurred,
        // apply the edit using cursor/selection managers and mark nodes dirty
        //
        // NOTE: Debug server text input is now handled via CallbackChange::CreateTextInput
        // which triggers text_input_triggered in CallCallbacksResult, processed in
        // process_callback_result_v2()
        let text_input_affected_nodes: BTreeMap<azul_core::dom::DomNodeId, (Vec<azul_core::events::EventFilter>, bool)> = if let Some(_layout_window) = self.get_layout_window_mut() {
            // TODO: Get actual text input from platform (IME, composed chars, etc.)
            // Platform layer needs to provide text_input: &str when available
            // Example integration:
            // - macOS: NSTextInputClient::insertText / setMarkedText
            // - Windows: WM_CHAR / WM_UNICHAR messages
            // - X11: XIM XLookupString with UTF-8
            // - Wayland: text-input protocol
            BTreeMap::new()
        } else {
            BTreeMap::new()
        };
        // TODO: Process accessibility events
        // if let Some(layout_window) = self.get_layout_window_mut() {
        //     layout_window.a11y_manager.record_state_changes(...);
        // }

        // PRE-CALLBACK INTERNAL EVENT FILTERING
        // Analyze events BEFORE user callbacks to extract internal system events
        // (text selection, etc.) that the framework handles.
        //
        // Managers have already been updated with current state (hit test, clicks, etc.)
        // Now we query them to detect multi-frame event patterns.

        let current_window_state = self.get_current_window_state();

        // Filter events to separate internal system events from user events
        // Query managers for state-based analysis (no local tracking needed)
        let pre_filter = if let Some(layout_window) = self.get_layout_window() {
            azul_core::events::pre_callback_filter_internal_events(
                &synthetic_events,
                hit_test_for_dispatch.as_ref(),
                &current_window_state.keyboard_state,
                &current_window_state.mouse_state,
                &layout_window.selection_manager,
                &layout_window.focus_manager,
            )
        } else {
            // No layout window - no internal events possible
            PreCallbackFilterResult {
                internal_events: Vec::new(),
                user_events: synthetic_events.clone(),
            }
        };

        // Track overall processing result
        let mut result = ProcessEventResult::DoNothing;

        // IFrame Integration: Check if any Scroll events occurred
        // If scrolling happened, we need to regenerate layout so IFrameManager can check
        // for edge detection and trigger re-invocation if needed
        let has_scroll_events = synthetic_events
            .iter()
            .any(|e| matches!(e.event_type, azul_core::events::EventType::Scroll));

        if has_scroll_events {
            // Mark frame for regeneration to enable IFrame edge detection
            self.mark_frame_needs_regeneration();
            result = result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
        }

        // Get external callbacks for system time
        let external = ExternalSystemCallbacks::rust_internal();

        // Process internal system events (text selection) BEFORE user callbacks
        let mut text_selection_affected_nodes = Vec::new();
        for internal_event in &pre_filter.internal_events {
            use azul_core::events::PreCallbackSystemEvent;

            match internal_event {
                PreCallbackSystemEvent::TextClick {
                    target,
                    position,
                    click_count,
                    timestamp,
                } => {
                    // Get current time using system callbacks
                    let current_instant = (external.get_system_time_fn.cb)();

                    // Calculate milliseconds since event timestamp
                    let duration_since_event = current_instant.duration_since(timestamp);
                    let current_time_ms = match duration_since_event {
                        azul_core::task::Duration::System(d) => {
                            #[cfg(feature = "std")]
                            {
                                let std_duration: std::time::Duration = d.into();
                                std_duration.as_millis() as u64
                            }
                            #[cfg(not(feature = "std"))]
                            {
                                0u64
                            }
                        }
                        azul_core::task::Duration::Tick(t) => t.tick_diff as u64,
                    };

                    // Process text selection click
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if let Some(affected_nodes) = layout_window
                            .process_mouse_click_for_selection(*position, current_time_ms)
                        {
                            text_selection_affected_nodes.extend(affected_nodes);
                        }
                    }
                }
                PreCallbackSystemEvent::TextDragSelection {
                    start_position,
                    current_position,
                    is_dragging,
                    ..
                } => {
                    if *is_dragging {
                        // Extend selection from start to current position
                        if let Some(layout_window) = self.get_layout_window_mut() {
                            if let Some(affected_nodes) = layout_window
                                .process_mouse_drag_for_selection(*start_position, *current_position)
                            {
                                text_selection_affected_nodes.extend(affected_nodes);
                            }
                        }
                    }
                }
                PreCallbackSystemEvent::ArrowKeyNavigation { .. } => {
                    // TODO: Implement arrow key navigation
                }
                PreCallbackSystemEvent::KeyboardShortcut { target, shortcut } => {
                    use azul_core::events::KeyboardShortcut;

                    match shortcut {
                        KeyboardShortcut::Copy => {
                            // Handle Ctrl+C: Copy selected text to clipboard
                            if let Some(layout_window) = self.get_layout_window() {
                                // TODO: Map target to correct DOM
                                let dom_id = azul_core::dom::DomId { inner: 0 };
                                if let Some(clipboard_content) =
                                    layout_window.get_selected_content_for_clipboard(&dom_id)
                                {
                                    // Copy text to system clipboard
                                    set_system_clipboard(
                                        clipboard_content.plain_text.as_str().to_string(),
                                    );
                                }
                            }
                        }
                        KeyboardShortcut::Cut => {
                            // Handle Ctrl+X: Copy to clipboard and delete selection
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                // TODO: Map target to correct DOM
                                let dom_id = azul_core::dom::DomId { inner: 0 };

                                // First, copy to clipboard
                                if let Some(clipboard_content) =
                                    layout_window.get_selected_content_for_clipboard(&dom_id)
                                {
                                    if set_system_clipboard(
                                        clipboard_content.plain_text.as_str().to_string(),
                                    ) {
                                        // Then delete the selection
                                        if let Some(affected_nodes) =
                                            layout_window.delete_selection(*target, false)
                                        {
                                            text_selection_affected_nodes.extend(affected_nodes);
                                        }
                                    }
                                }
                            }
                        }
                        KeyboardShortcut::Paste => {
                            // Handle Ctrl+V: Insert clipboard text at cursor
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                if let Some(clipboard_text) = get_system_clipboard() {
                                    // Insert text at current cursor position
                                    // TODO: Implement paste operation through TextInputManager
                                    // For now, treat it like text input
                                    let affected_nodes =
                                        layout_window.process_text_input(&clipboard_text);
                                    for (node_id, _) in affected_nodes {
                                        text_selection_affected_nodes.push(node_id);
                                    }
                                }
                            }
                        }
                        KeyboardShortcut::SelectAll => {
                            // Handle Ctrl+A: Select all text in focused node
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                // TODO: Implement select_all operation
                                // This should select all text in the focused contenteditable node
                            }
                        }
                        KeyboardShortcut::Undo | KeyboardShortcut::Redo => {
                            // Handle Ctrl+Z (Undo) / Ctrl+Y or Ctrl+Shift+Z (Redo)
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                // Convert DomNodeId to NodeId using proper decoding
                                let node_id = match target.node.into_crate_internal() {
                                    Some(id) => id,
                                    None => continue,
                                };

                                // Get external callbacks for system time
                                let external = ExternalSystemCallbacks::rust_internal();
                                let timestamp = (external.get_system_time_fn.cb)().into();

                                if *shortcut == KeyboardShortcut::Undo {
                                    // Pop from undo stack
                                    if let Some(operation) =
                                        layout_window.undo_redo_manager.pop_undo(node_id)
                                    {
                                        // Create revert changeset
                                        use azul_layout::managers::undo_redo::create_revert_changeset;
                                        let revert_changeset =
                                            create_revert_changeset(&operation, timestamp);

                                        // TODO: Allow user callback to preventDefault

                                        // Apply the revert - restore pre-state text completely
                                        let node_id_internal = target.node.into_crate_internal();
                                        if let Some(node_id_internal) = node_id_internal {
                                            // Create InlineContent from pre-state text
                                            use std::sync::Arc;

                                            use azul_layout::text3::cache::{
                                                InlineContent, StyleProperties, StyledRun,
                                            };

                                            let new_content =
                                                vec![InlineContent::Text(StyledRun {
                                                    text: operation
                                                        .pre_state
                                                        .text_content
                                                        .as_str()
                                                        .to_string(),
                                                    // TODO: Preserve original style
                                                    style: Arc::new(StyleProperties::default()),
                                                    logical_start_byte: 0,
                                                    source_node_id: None, // Undo operation - node context not available
                                                })];

                                            // Update text cache with pre-state content
                                            layout_window.update_text_cache_after_edit(
                                                target.dom,
                                                node_id_internal,
                                                new_content,
                                            );

                                            // Restore cursor position
                                            if let Some(cursor) =
                                                operation.pre_state.cursor_position.into_option()
                                            {
                                                layout_window.cursor_manager.move_cursor_to(
                                                    cursor,
                                                    target.dom,
                                                    node_id_internal,
                                                );
                                            }
                                        }

                                        // Push to redo stack after successful undo
                                        layout_window.undo_redo_manager.push_redo(operation);

                                        // Mark node for re-render
                                        text_selection_affected_nodes.push(*target);
                                    }
                                } else {
                                    // Redo operation
                                    if let Some(operation) =
                                        layout_window.undo_redo_manager.pop_redo(node_id)
                                    {
                                        // TODO: Allow user callback to preventDefault

                                        // Re-apply the original changeset by re-executing text
                                        // input
                                        let node_id_internal = target.node.into_crate_internal();
                                        if let Some(node_id_internal) = node_id_internal {
                                            // For redo, we use the text input system to re-apply
                                            // the change
                                            use azul_layout::managers::changeset::TextOperation;

                                            // Determine what to re-apply based on the operation
                                            match &operation.changeset.operation {
                                                TextOperation::InsertText(op) => {
                                                    // Re-insert the text via process_text_input
                                                    let affected =
                                                        layout_window.process_text_input(&op.text);
                                                    for (node, _) in affected {
                                                        text_selection_affected_nodes.push(node);
                                                    }
                                                }
                                                _ => {
                                                    // For other operations, just mark for re-render
                                                    // Full implementation would handle each
                                                    // operation type
                                                }
                                            }
                                        }

                                        // Push to undo stack after successful redo
                                        layout_window.undo_redo_manager.push_undo(operation);

                                        // Mark node for re-render
                                        text_selection_affected_nodes.push(*target);
                                    }
                                }
                            }
                        }
                    }
                }
                PreCallbackSystemEvent::DeleteSelection { target, forward } => {
                    // Handle Backspace/Delete key
                    // For now, we directly call delete_selection
                    // TODO: Integrate with TextInputManager changeset system
                    // This should:
                    // 1. Create DeleteText changeset
                    // 2. Fire On::TextInput callback with preventDefault support
                    // 3. Apply deletion if !preventDefault
                    // 4. Record to undo stack
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if let Some(affected_nodes) =
                            layout_window.delete_selection(*target, *forward)
                        {
                            text_selection_affected_nodes.extend(affected_nodes);
                        }
                    }
                }
            }
        }

        // If text selection changed, mark for re-render
        if !text_selection_affected_nodes.is_empty() {
            result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // EVENT FILTERING AND CALLBACK DISPATCH

        // DEBUG: Log user events
        for (i, ev) in pre_filter.user_events.iter().enumerate() {
        }
        
        // DEBUG: Check hit test
        if let Some(ref ht) = hit_test_for_dispatch {
            for (dom_id, dom_ht) in &ht.hovered_nodes {
                for (node_id, _) in dom_ht.regular_hit_test_nodes.iter() {
                }
            }
        } else {
        }

        // Dispatch user events to callbacks (internal events already processed)
        let dispatch_result = azul_core::events::dispatch_synthetic_events(
            &pre_filter.user_events,
            hit_test_for_dispatch.as_ref(),
        );

        for (i, cb) in dispatch_result.callbacks.iter().enumerate() {
        }

        if dispatch_result.is_empty() {
            // Return accumulated result from internal processing, not DoNothing
            // Internal events (text selection, keyboard shortcuts) may have set
            // result to ShouldReRenderCurrentWindow even if no user callbacks exist.
            return result;
        }

        // Filter out system internal events as a safety check
        // (They shouldn't appear since user events shouldn't contain them,
        //  but we filter anyway to be safe)
        let user_callbacks: Vec<_> = dispatch_result
            .callbacks
            .iter()
            .filter(|cb| {
                if let azul_core::events::EventFilter::Hover(hover_filter) = cb.event_filter {
                    !hover_filter.is_system_internal()
                } else {
                    true
                }
            })
            .collect();


        // USER CALLBACK INVOCATION

        // Capture focus state before callbacks for post-callback filtering
        let old_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        // Invoke all user callbacks and collect results
        let mut should_stop_propagation = false;
        let mut should_recurse = false;
        let mut focus_changed = false;
        let mut prevent_default = false; // Track if any callback prevented default

        for callback_to_invoke in user_callbacks {
            if should_stop_propagation {
                break;
            }

            // Convert core CallbackTarget to shell CallbackTarget
            let target = match &callback_to_invoke.target {
                CoreCallbackTarget::Node { dom_id, node_id } => CallbackTarget::Node(HitTestNode {
                    dom_id: dom_id.inner as u64,
                    node_id: node_id.index() as u64,
                }),
                CoreCallbackTarget::RootNodes => CallbackTarget::RootNodes,
            };

            // Invoke callbacks and collect results
            let callback_results =
                self.invoke_callbacks_v2(target, callback_to_invoke.event_filter);

            for callback_result in callback_results {
                let event_result = self.process_callback_result_v2(&callback_result);
                result = result.max(event_result);

                // Check if callback prevented default
                if callback_result.prevent_default {
                    prevent_default = true;
                }

                // Check if we should stop propagation
                if callback_result.stop_propagation {
                    should_stop_propagation = true;
                    break;
                }

                // Check if we need to recurse (DOM was regenerated)
                use azul_core::callbacks::Update;
                if matches!(
                    callback_result.callbacks_update_screen,
                    Update::RefreshDom | Update::RefreshDomAllWindows
                ) {
                    should_recurse = true;
                }
            }
        }

        // POST-CALLBACK INTERNAL EVENT FILTERING
        // Process callback results to determine what internal processing continues

        let new_focus = self
            .get_layout_window()
            .and_then(|lw| lw.focus_manager.get_focused_node().copied());

        let post_filter = azul_core::events::post_callback_filter_internal_events(
            prevent_default,
            &pre_filter.internal_events,
            old_focus,
            new_focus,
        );

        // Process system events returned from post-callback filter
        for system_event in &post_filter.system_events {
            match system_event {
                azul_core::events::PostCallbackSystemEvent::FocusChanged => {
                    focus_changed = true;
                }
                azul_core::events::PostCallbackSystemEvent::ApplyTextInput => {
                    // Text input will be applied below
                }
                azul_core::events::PostCallbackSystemEvent::ApplyTextChangeset => {
                    // TODO: Apply text changesets from Phase 2 refactoring
                    // This will be implemented when changesets are fully integrated
                }
                azul_core::events::PostCallbackSystemEvent::ScrollIntoView => {
                    // Scroll cursor/selection into view after text change
                    if let Some(layout_window) = self.get_layout_window_mut() {
                        use azul_layout::window::{ScrollMode, SelectionScrollType};

                        // Determine what to scroll based on focus manager state
                        let scroll_type =
                            if let Some(focused_node) = layout_window.focus_manager.focused_node {
                                // Check if focused node has a text cursor or selection
                                if layout_window
                                    .selection_manager
                                    .get_selection(&focused_node.dom)
                                    .is_some()
                                {
                                    SelectionScrollType::Selection
                                } else {
                                    SelectionScrollType::Cursor
                                }
                            } else {
                                // No focus, nothing to scroll
                                continue;
                            };

                        // Scroll with instant mode (user-initiated action, not auto-scroll)
                        layout_window.scroll_selection_into_view(scroll_type, ScrollMode::Instant);

                        // Mark for re-render since scrolling changed viewport
                        result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                    }
                }
                azul_core::events::PostCallbackSystemEvent::StartAutoScrollTimer => {
                    // Start auto-scroll timer for drag-to-scroll (Phase 5)
                    // Timer frequency matches monitor refresh rate for smooth scrolling

                    if let Some(layout_window) = self.get_layout_window() {
                        let timer_id = azul_core::task::TimerId {
                            id: AUTO_SCROLL_TIMER_ID,
                        };

                        // Check if timer already running (avoid duplicate timers)
                        if !layout_window.timers.contains_key(&timer_id) {
                            use azul_core::{
                                refany::RefAny,
                                task::{Duration as AzulDuration, SystemTimeDiff},
                            };
                            use azul_layout::timer::{Timer, TimerCallbackType};

                            // TODO: Get actual monitor refresh rate from platform
                            // For now, default to 60Hz (16.67ms per frame)
                            // Platform implementations should query:
                            // - macOS: [[NSScreen mainScreen] maximumFramesPerSecond]
                            // - Windows: DwmGetCompositionTimingInfo
                            // - X11: XRRGetScreenInfo
                            // - Wayland: wl_output refresh field
                            const DEFAULT_REFRESH_RATE_HZ: u32 = 60;
                            let frame_time_nanos = 1_000_000_000 / DEFAULT_REFRESH_RATE_HZ;

                            // Get system time function for timer creation
                            let external = ExternalSystemCallbacks::rust_internal();

                            // Create timer with monitor refresh rate interval
                            let timer = Timer::create(
                                RefAny::new(()), // Empty data
                                auto_scroll_timer_callback as TimerCallbackType,
                                external.get_system_time_fn,
                            )
                            .with_interval(AzulDuration::System(SystemTimeDiff {
                                secs: 0,
                                nanos: frame_time_nanos,
                            }));

                            // Add timer to layout window
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                layout_window.add_timer(timer_id, timer.clone());

                                // Start platform-specific native timer
                                // This will create NSTimer/SetTimer/timerfd depending on platform
                                self.start_timer(AUTO_SCROLL_TIMER_ID, timer);

                                result =
                                    result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                            }
                        }
                    }
                }
                azul_core::events::PostCallbackSystemEvent::CancelAutoScrollTimer => {
                    // Cancel auto-scroll timer (Phase 5)
                    // This stops both the framework timer and the native platform timer

                    let timer_id = azul_core::task::TimerId {
                        id: AUTO_SCROLL_TIMER_ID,
                    };

                    if let Some(layout_window) = self.get_layout_window_mut() {
                        if layout_window.timers.contains_key(&timer_id) {
                            // Remove from layout window timer map
                            layout_window.remove_timer(&timer_id);

                            // Stop native platform timer (NSTimer/SetTimer/timerfd)
                            // Platform implementations handle cleanup:
                            // - macOS: [timer invalidate]
                            // - Windows: KillTimer(hwnd, timer_id)
                            // - X11: Remove from internal timer manager
                            // - Wayland: close(timerfd)
                            self.stop_timer(AUTO_SCROLL_TIMER_ID);
                        }
                    }
                }
            }
        }

        // POST-CALLBACK TEXT INPUT PROCESSING
        // Apply text changeset if preventDefault was not set.
        // This is where we:
        // 1. Compute and cache the text changes (reshape glyphs)
        // 2. Scroll cursor into view if needed
        // 3. Mark dirty nodes for re-layout
        // 4. Potentially trigger another event cycle if scrolling occurred

        let should_apply_text_input = post_filter
            .system_events
            .contains(&azul_core::events::PostCallbackSystemEvent::ApplyTextInput);

        if should_apply_text_input && !text_input_affected_nodes.is_empty() {
            if let Some(layout_window) = self.get_layout_window_mut() {
                // Apply text changes and get list of dirty nodes
                let dirty_nodes = layout_window.apply_text_changeset();

                // Mark dirty nodes for re-layout
                for node in dirty_nodes {
                    // TODO: Mark node as needing re-layout
                    // This will be handled by the existing dirty tracking system
                    let _ = node;
                }

                // Request re-render since text changed
                result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }

            // After text changes, scroll cursor into view if we have a focused text input
            // Note: This needs to happen AFTER relayout to get accurate cursor position
            if let Some(layout_window) = self.get_layout_window() {
                if let Some(cursor_rect) = layout_window.get_focused_cursor_rect() {
                    // Get the focused node to find its scroll container
                    if let Some(focused_node_id) = layout_window.focus_manager.focused_node {
                        // Find the nearest scrollable ancestor
                        if let Some(scroll_container) =
                            layout_window.find_scrollable_ancestor(focused_node_id)
                        {
                            // Get the scroll state for this container
                            let scroll_node_id = scroll_container.node.into_crate_internal();
                            if let Some(scroll_node_id) = scroll_node_id {
                                if let Some(scroll_state) = layout_window
                                    .scroll_manager
                                    .get_scroll_state(scroll_container.dom, scroll_node_id)
                                {
                                    // Get the container's layout rect
                                    if let Some(container_rect) =
                                        layout_window.get_node_layout_rect(scroll_container)
                                    {
                                        // Calculate the visible area (container rect adjusted by
                                        // scroll offset)
                                        let visible_area = azul_core::geom::LogicalRect::new(
                                            azul_core::geom::LogicalPosition::new(
                                                container_rect.origin.x
                                                    + scroll_state.current_offset.x,
                                                container_rect.origin.y
                                                    + scroll_state.current_offset.y,
                                            ),
                                            container_rect.size,
                                        );

                                        // Add padding around cursor for comfortable visibility
                                        const SCROLL_PADDING: f32 = 5.0;

                                        // Calculate how much to scroll to bring cursor into view
                                        let mut scroll_delta =
                                            azul_core::geom::LogicalPosition::zero();

                                        // Check horizontal overflow
                                        if cursor_rect.origin.x
                                            < visible_area.origin.x + SCROLL_PADDING
                                        {
                                            // Cursor is too far left
                                            scroll_delta.x = cursor_rect.origin.x
                                                - (visible_area.origin.x + SCROLL_PADDING);
                                        } else if cursor_rect.origin.x + cursor_rect.size.width
                                            > visible_area.origin.x + visible_area.size.width
                                                - SCROLL_PADDING
                                        {
                                            // Cursor is too far right
                                            scroll_delta.x = (cursor_rect.origin.x
                                                + cursor_rect.size.width)
                                                - (visible_area.origin.x + visible_area.size.width
                                                    - SCROLL_PADDING);
                                        }

                                        // Check vertical overflow
                                        if cursor_rect.origin.y
                                            < visible_area.origin.y + SCROLL_PADDING
                                        {
                                            // Cursor is too far up
                                            scroll_delta.y = cursor_rect.origin.y
                                                - (visible_area.origin.y + SCROLL_PADDING);
                                        } else if cursor_rect.origin.y + cursor_rect.size.height
                                            > visible_area.origin.y + visible_area.size.height
                                                - SCROLL_PADDING
                                        {
                                            // Cursor is too far down
                                            scroll_delta.y = (cursor_rect.origin.y
                                                + cursor_rect.size.height)
                                                - (visible_area.origin.y
                                                    + visible_area.size.height
                                                    - SCROLL_PADDING);
                                        }

                                        // Apply scroll if needed
                                        if scroll_delta.x != 0.0 || scroll_delta.y != 0.0 {
                                            // Get current time from system callbacks
                                            let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                                            let now = (external.get_system_time_fn.cb)();

                                            if let Some(layout_window_mut) =
                                                self.get_layout_window_mut()
                                            {
                                                // Instant scroll (duration = 0) for cursor
                                                // scrolling
                                                layout_window_mut.scroll_manager.scroll_by(
                                                    scroll_container.dom,
                                                    scroll_node_id,
                                                    scroll_delta,
                                                    std::time::Duration::from_millis(0).into(),
                                                    azul_core::events::EasingFunction::Linear,
                                                    now.into(),
                                                );
                                                // Scrolling may trigger more events, so recurse
                                                result = result.max(
                                                    ProcessEventResult::ShouldReRenderCurrentWindow,
                                                );
                                                should_recurse = true;
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // MOUSE CLICK-TO-FOCUS (W3C default behavior)
        // When the user clicks on a focusable element, focus should move to that element.
        // We check for MouseDown events and find the deepest focusable ancestor.
        let mut mouse_click_focus_changed = false;
        if !prevent_default {
            let has_mouse_down = synthetic_events.iter().any(|e| {
                matches!(e.event_type, azul_core::events::EventType::MouseDown)
            });
            
            if has_mouse_down {
                // Get the hit test data to find which node was clicked
                if let Some(ref hit_test) = hit_test_for_dispatch {
                    // Find the deepest focusable node in the hit chain
                    let mut clicked_focusable_node: Option<azul_core::dom::DomNodeId> = None;
                    
                    for (dom_id, hit_test_data) in &hit_test.hovered_nodes {
                        // Find deepest hit node first
                        let deepest = hit_test_data.regular_hit_test_nodes
                            .iter()
                            .max_by_key(|(_, hit_item)| {
                                // Higher hit_depth = further from camera, so we want lowest
                                // But we actually want the topmost (frontmost) which is depth 0
                                std::cmp::Reverse(hit_item.hit_depth)
                            });
                        
                        if let Some((node_id, _)) = deepest {
                            if let Some(layout_window) = self.get_layout_window() {
                                if let Some(layout_result) = layout_window.layout_results.get(dom_id) {
                                    let node_data = layout_result.styled_dom.node_data.as_container();
                                    let node_hierarchy = layout_result.styled_dom.node_hierarchy.as_container();
                                    
                                    // Walk from clicked node to root, find first focusable
                                    let mut current = Some(*node_id);
                                    while let Some(nid) = current {
                                        if let Some(nd) = node_data.get(nid) {
                                            if nd.is_focusable() {
                                                clicked_focusable_node = Some(azul_core::dom::DomNodeId {
                                                    dom: *dom_id,
                                                    node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(nid)),
                                                });
                                                break;
                                            }
                                        }
                                        current = node_hierarchy.get(nid).and_then(|h| h.parent_id());
                                    }
                                }
                            }
                        }
                    }
                    
                    // If we found a focusable node, set focus to it
                    if let Some(new_focus) = clicked_focusable_node {
                        let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                        let new_focus_node_id = new_focus.node.into_crate_internal();
                        
                        // Only change focus if clicking on a different node
                        if old_focus_node_id != new_focus_node_id {
                            if let Some(layout_window) = self.get_layout_window_mut() {
                                layout_window.focus_manager.set_focused_node(Some(new_focus));
                                mouse_click_focus_changed = true;
                                
                                // SCROLL INTO VIEW: Scroll newly focused node into visible area
                                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                                let now = azul_core::task::Instant::now();
                                layout_window.scroll_node_into_view(
                                    new_focus,
                                    ScrollIntoViewOptions::nearest(),
                                    now,
                                );
                                
                                // RESTYLE: Update StyledNodeState and compute CSS changes
                                let restyle_result = apply_focus_restyle(
                                    layout_window,
                                    old_focus_node_id,
                                    new_focus_node_id,
                                );
                                result = result.max(restyle_result);
                            }
                            
                            log_debug!(
                                super::debug_server::LogCategory::Input,
                                "[Event V2] Click-to-focus: {:?} -> {:?}",
                                old_focus,
                                new_focus
                            );
                        }
                    }
                }
            }
        }

        // KEYBOARD DEFAULT ACTIONS (Tab navigation, Enter/Space activation, Escape)
        // Process keyboard default actions if not prevented by callbacks
        // This implements W3C focus navigation and element activation behavior
        let mut default_action_focus_changed = false;
        let mut synthetic_click_target: Option<azul_core::dom::DomNodeId> = None;
        
        if !prevent_default {
            // Check if we have a keyboard event (KeyDown specifically)
            let has_key_event = pre_filter.user_events.iter().any(|e| {
                matches!(e.event_type, azul_core::events::EventType::KeyDown)
            });

            if has_key_event {
                // Get keyboard state and focused node for default action determination
                let keyboard_state = &self.get_current_window_state().keyboard_state;
                let focused_node = old_focus;

                // Get layout results for querying node properties
                let layout_results = self.get_layout_window()
                    .map(|lw| &lw.layout_results);

                if let Some(layout_results) = layout_results {
                    // Determine what default action should occur
                    let default_action_result = azul_layout::default_actions::determine_keyboard_default_action(
                        keyboard_state,
                        focused_node,
                        layout_results,
                        prevent_default,
                    );

                    // Process the default action if not prevented
                    if default_action_result.has_action() {
                        use azul_core::events::DefaultAction;
                        use azul_core::callbacks::FocusTarget;
                        use azul_layout::managers::focus_cursor::resolve_focus_target;

                        match &default_action_result.action {
                            DefaultAction::FocusNext | DefaultAction::FocusPrevious |
                            DefaultAction::FocusFirst | DefaultAction::FocusLast => {
                                // Convert DefaultAction to FocusTarget
                                let focus_target = azul_layout::default_actions::default_action_to_focus_target(&default_action_result.action);
                                
                                if let Some(focus_target) = focus_target {
                                    // Resolve the focus target to an actual node
                                    let resolve_result = resolve_focus_target(
                                        &focus_target,
                                        layout_results,
                                        focused_node,
                                    );
                                    
                                    if let Ok(new_focus_node) = resolve_result {
                                        // Get the old focus node ID for restyle
                                        let old_focus_node_id = focused_node.and_then(|f| f.node.into_crate_internal());
                                        let new_focus_node_id = new_focus_node.and_then(|f| f.node.into_crate_internal());
                                        
                                        // Update focus manager and get timer action
                                        let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                                            layout_window.focus_manager.set_focused_node(new_focus_node);
                                            default_action_focus_changed = true;
                                            
                                            // SCROLL INTO VIEW: Scroll newly focused node into visible area
                                            if let Some(focus_node) = new_focus_node {
                                                use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                                                let now = azul_core::task::Instant::now();
                                                layout_window.scroll_node_into_view(
                                                    focus_node,
                                                    ScrollIntoViewOptions::nearest(),
                                                    now,
                                                );
                                            }
                                            
                                            // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
                                            let window_state = layout_window.current_window_state.clone();
                                            let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                                                new_focus_node,
                                                &window_state,
                                            );
                                            
                                            // RESTYLE: Update StyledNodeState and compute CSS changes
                                            if old_focus_node_id != new_focus_node_id {
                                                let restyle_result = apply_focus_restyle(
                                                    layout_window,
                                                    old_focus_node_id,
                                                    new_focus_node_id,
                                                );
                                                result = result.max(restyle_result);
                                            } else {
                                                result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                                            }
                                            
                                            Some(timer_action)
                                        } else {
                                            None
                                        };
                                        
                                        // Apply timer action outside the layout_window borrow
                                        if let Some(timer_action) = timer_action {
                                            match timer_action {
                                                azul_layout::CursorBlinkTimerAction::Start(timer) => {
                                                    self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                                                }
                                                azul_layout::CursorBlinkTimerAction::Stop => {
                                                    self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                                                }
                                                azul_layout::CursorBlinkTimerAction::NoChange => {}
                                            }
                                        }

                                        log_debug!(
                                            super::debug_server::LogCategory::Input,
                                            "[Event V2] Default action: {:?} -> {:?}",
                                            default_action_result.action,
                                            new_focus_node
                                        );
                                    }
                                }
                            }

                            DefaultAction::ClearFocus => {
                                // Clear focus (Escape key)
                                // Get old focus before clearing
                                let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                                
                                let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                                    layout_window.focus_manager.set_focused_node(None);
                                    default_action_focus_changed = true;
                                    
                                    // CURSOR BLINK TIMER: Stop timer when focus is cleared
                                    let window_state = layout_window.current_window_state.clone();
                                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                                        None,
                                        &window_state,
                                    );
                                    
                                    // RESTYLE: Update StyledNodeState when focus is cleared
                                    if old_focus_node_id.is_some() {
                                        let restyle_result = apply_focus_restyle(
                                            layout_window,
                                            old_focus_node_id,
                                            None,
                                        );
                                        result = result.max(restyle_result);
                                    } else {
                                        result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                                    }
                                    
                                    Some(timer_action)
                                } else {
                                    None
                                };
                                
                                // Apply timer action outside the layout_window borrow
                                if let Some(timer_action) = timer_action {
                                    match timer_action {
                                        azul_layout::CursorBlinkTimerAction::Start(_) => {}
                                        azul_layout::CursorBlinkTimerAction::Stop => {
                                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                                        }
                                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                                    }
                                }

                                log_debug!(
                                    super::debug_server::LogCategory::Input,
                                    "[Event V2] Default action: ClearFocus"
                                );
                            }

                            DefaultAction::ActivateFocusedElement { target } => {
                                // Queue synthetic click for later dispatch
                                synthetic_click_target = Some(target.clone());

                                log_debug!(
                                    super::debug_server::LogCategory::Input,
                                    "[Event V2] Default action: ActivateFocusedElement -> {:?}",
                                    target
                                );
                            }

                            DefaultAction::ScrollFocusedContainer { direction, amount } => {
                                // TODO: Implement keyboard scrolling
                                log_debug!(
                                    super::debug_server::LogCategory::Input,
                                    "[Event V2] Default action: ScrollFocusedContainer {:?} {:?} (not yet implemented)",
                                    direction,
                                    amount
                                );
                            }

                            DefaultAction::None => {}
                            
                            // Additional default actions not yet implemented
                            DefaultAction::SubmitForm { .. } |
                            DefaultAction::CloseModal { .. } |
                            DefaultAction::SelectAllText => {
                                // These are placeholder for future implementation
                            }
                        }
                    }
                }
            }
        }

        // SYNTHETIC CLICK DISPATCH (for Enter/Space activation)
        // Process synthetic clicks from keyboard activation
        if let Some(click_target) = synthetic_click_target {
            if depth + 1 < MAX_EVENT_RECURSION_DEPTH {
                // Dispatch the synthetic click event directly to callbacks
                if let Some(internal_node_id) = click_target.node.into_crate_internal() {
                    let target = CallbackTarget::Node(HitTestNode {
                        dom_id: click_target.dom.inner as u64,
                        node_id: internal_node_id.index() as u64,
                    });

                    // Invoke click callbacks on the target
                    // Note: In Azul, "click" is typically LeftMouseUp
                    let click_results = self.invoke_callbacks_v2(target, EventFilter::Hover(
                        azul_core::events::HoverEventFilter::LeftMouseUp
                    ));

                    for callback_result in click_results {
                        let event_result = self.process_callback_result_v2(&callback_result);
                        result = result.max(event_result);

                        // Check if we need to recurse (DOM was regenerated)
                        use azul_core::callbacks::Update;
                        if matches!(
                            callback_result.callbacks_update_screen,
                            Update::RefreshDom | Update::RefreshDomAllWindows
                        ) {
                            should_recurse = true;
                        }
                    }
                }

                log_debug!(
                    super::debug_server::LogCategory::Input,
                    "[Event V2] Dispatched synthetic click for element activation: {:?}",
                    click_target
                );
            }
        }

        // Handle focus changes: generate synthetic FocusIn/FocusOut events
        log_debug!(
            super::debug_server::LogCategory::Input,
            "[Event V2] Focus check: focus_changed={}, default_action_focus_changed={}, mouse_click_focus_changed={}, depth={}, old_focus={:?}",
            focus_changed,
            default_action_focus_changed,
            mouse_click_focus_changed,
            depth,
            old_focus
        );
        
        if (focus_changed || default_action_focus_changed || mouse_click_focus_changed) && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            // Get the new focus BEFORE clearing selections
            let new_focus = self
                .get_layout_window()
                .and_then(|lw| lw.focus_manager.get_focused_node().copied());
            
            log_debug!(
                super::debug_server::LogCategory::Input,
                "[Event V2] Focus changed! old_focus={:?}, new_focus={:?}",
                old_focus,
                new_focus
            );
            
            // Clear selections when focus changes (standard UI behavior)
            if let Some(layout_window) = self.get_layout_window_mut() {
                layout_window.selection_manager.clear_all();
            }
            
            // DISPATCH FOCUS CALLBACKS: FocusLost on old node, FocusReceived on new node
            // This is where we actually invoke the user-registered FocusReceived/FocusLost callbacks
            
            // Dispatch FocusLost to old node (if any)
            if let Some(old_node) = old_focus {
                if let Some(internal_node_id) = old_node.node.into_crate_internal() {
                    let target = CallbackTarget::Node(HitTestNode {
                        dom_id: old_node.dom.inner as u64,
                        node_id: internal_node_id.index() as u64,
                    });
                    
                    log_debug!(
                        super::debug_server::LogCategory::Input,
                        "[Event V2] Dispatching FocusLost to node {:?}",
                        old_node
                    );
                    
                    let focus_lost_results = self.invoke_callbacks_v2(
                        target,
                        EventFilter::Focus(FocusEventFilter::FocusLost)
                    );
                    
                    for callback_result in focus_lost_results {
                        let event_result = self.process_callback_result_v2(&callback_result);
                        result = result.max(event_result);
                    }
                }
            }
            
            // Dispatch FocusReceived to new node (if any)
            if let Some(new_node) = new_focus {
                if let Some(internal_node_id) = new_node.node.into_crate_internal() {
                    let target = CallbackTarget::Node(HitTestNode {
                        dom_id: new_node.dom.inner as u64,
                        node_id: internal_node_id.index() as u64,
                    });
                    
                    log_debug!(
                        super::debug_server::LogCategory::Input,
                        "[Event V2] Dispatching FocusReceived to node {:?}",
                        new_node
                    );
                    
                    let focus_received_results = self.invoke_callbacks_v2(
                        target,
                        EventFilter::Focus(FocusEventFilter::FocusReceived)
                    );
                    
                    for callback_result in focus_received_results {
                        let event_result = self.process_callback_result_v2(&callback_result);
                        result = result.max(event_result);
                    }
                }
            }

            // CRITICAL: Update previous_state BEFORE recursing to prevent the same
            // keyboard events from being detected again. Without this, a Tab key
            // would trigger FocusNext on every recursion level.
            let current = self.get_current_window_state().clone();
            self.set_previous_window_state(current);

            // Recurse to process any further events that may have been triggered
            let focus_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(focus_result);
        }

        // Recurse if needed (DOM regeneration)
        if should_recurse && depth + 1 < MAX_EVENT_RECURSION_DEPTH {
            // CRITICAL: Update previous_state BEFORE recursing to prevent the same
            // mouse/keyboard events from being detected again. Without this, a MouseUp
            // event would trigger the callback on every recursion level, causing
            // the callback to fire multiple times for a single click.
            let current = self.get_current_window_state().clone();
            self.set_previous_window_state(current);

            let recursive_result = self.process_window_events_recursive_v2(depth + 1);
            result = result.max(recursive_result);
        }

        // Auto-activate window drag if DragStart occurred on titlebar
        // This allows titlebar dragging to work even when mouse leaves window
        if synthetic_events
            .iter()
            .any(|e| matches!(e.event_type, azul_core::events::EventType::DragStart))
        {
            // Get current window position before mutable borrow
            let current_pos = self.get_current_window_state().position;

            // Check if drag was on a titlebar element (class="csd-title" or "csd-titlebar")
            if let Some(hit_test) = hit_test_for_dispatch.as_ref() {
                if let Some(layout_window) = self.get_layout_window_mut() {
                    let is_titlebar_drag = is_hit_on_titlebar(hit_test, layout_window);

                    if is_titlebar_drag && !layout_window.gesture_drag_manager.is_window_dragging()
                    {
                        // Activate window drag with current window position
                        let hit_test_clone = hit_test.hovered_nodes.values().next().cloned();

                        layout_window
                            .gesture_drag_manager
                            .activate_window_drag(current_pos, hit_test_clone);

                        log_debug!(
                            super::debug_server::LogCategory::Input,
                            "[Event V2] Auto-activated window drag on titlebar DragStart"
                        );
                    }
                }
            }
        }

        // W3C "flag and defer" pattern: Finalize pending focus changes after all events processed
        // 
        // This is called at the end of event processing to initialize the cursor for
        // contenteditable elements. The cursor wasn't initialized during focus event handling
        // because text layout may not have been available. Now that all events have been
        // processed and layout has had a chance to update, we can safely initialize the cursor.
        //
        // After successful cursor initialization, we also start the cursor blink timer.
        // NOTE: We need to carefully manage borrows here - first do all layout_window work,
        // then create the timer separately if needed.
        let timer_creation_needed = if let Some(layout_window) = self.get_layout_window_mut() {
            let needs_init = layout_window.focus_manager.needs_cursor_initialization();
            if needs_init {
                let cursor_initialized = layout_window.finalize_pending_focus_changes();
                if cursor_initialized {
                    log_debug!(
                        super::debug_server::LogCategory::Input,
                        "[Event V2] Cursor initialized via finalize_pending_focus_changes"
                    );
                    result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                    
                    // Check if blink timer is not already active
                    if !layout_window.cursor_manager.is_blink_timer_active() {
                        layout_window.cursor_manager.set_blink_timer_active(true);
                        true // Signal that we need to create and start the timer
                    } else {
                        false
                    }
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            false
        };
        
        // Create and start the blink timer outside of the mutable layout_window borrow
        if timer_creation_needed {
            // Now we can safely get both window_state and layout_window
            let timer = if let Some(layout_window) = self.get_layout_window() {
                let current_window_state = self.get_current_window_state();
                Some(layout_window.create_cursor_blink_timer(current_window_state))
            } else {
                None
            };
            
            if let Some(timer) = timer {
                self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                log_debug!(
                    super::debug_server::LogCategory::Input,
                    "[Event V2] Started cursor blink timer after focus finalization"
                );
            }
        }

        result
    }

    /// V2: Process callback result and determine what action to take.
    ///
    /// This converts the callback result into a `ProcessEventResult` that tells
    /// the platform what to do next (redraw, regenerate layout, etc.).
    ///
    /// This method handles:
    /// - Window state modifications (title, size, position, flags)
    /// - Focus changes
    /// - Image/image mask updates
    /// - Timer/thread management
    /// - New window creation
    /// - DOM regeneration triggering
    fn process_callback_result_v2(&mut self, result: &CallCallbacksResult) -> ProcessEventResult {
        use azul_core::callbacks::Update;

        let mut event_result = ProcessEventResult::DoNothing;
        let mut mouse_state_changed = false;
        let mut keyboard_state_changed = false;

        // Handle window state modifications
        if let Some(ref modified_state) = result.modified_window_state {
            // Check if mouse_state changed (for synthetic event injection)
            // NOTE: We must save previous state BEFORE modifying current state
            // so that process_window_events_recursive_v2 can detect the change
            let old_mouse_state = self.get_current_window_state().mouse_state.clone();
            if old_mouse_state != modified_state.mouse_state {
                mouse_state_changed = true;
                // Save current state as previous BEFORE updating
                // This is critical for synthetic events from debug API
                let old_state = self.get_current_window_state().clone();
                self.set_previous_window_state(old_state);
            }
            
            // Check if keyboard_state changed (for synthetic keyboard events)
            let old_keyboard_state = self.get_current_window_state().keyboard_state.clone();
            if old_keyboard_state != modified_state.keyboard_state {
                keyboard_state_changed = true;
                // Save current state as previous BEFORE updating (if not already saved for mouse)
                if !mouse_state_changed {
                    let old_state = self.get_current_window_state().clone();
                    self.set_previous_window_state(old_state);
                }
            }

            // Now update current state
            let current_state = self.get_current_window_state_mut();
            current_state.title = modified_state.title.clone();
            current_state.size = modified_state.size;
            current_state.position = modified_state.position;
            current_state.flags = modified_state.flags;
            current_state.background_color = modified_state.background_color;
            // Also copy mouse_state for synthetic event injection
            current_state.mouse_state = modified_state.mouse_state.clone();
            // Also copy keyboard_state for synthetic keyboard events
            current_state.keyboard_state = modified_state.keyboard_state.clone();

            // Check if window should close
            if modified_state.flags.close_requested {
                // Platform should handle window destruction
                return ProcessEventResult::DoNothing;
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // If mouse_state changed, trigger event processing to invoke callbacks
        // This enables synthetic mouse events from debug API and automation
        if mouse_state_changed {
            // First, update hit testing at the new mouse position
            // This is critical for synthetic events - without hit testing,
            // dispatch_synthetic_events won't know which nodes are under the mouse
            let mouse_pos = self
                .get_current_window_state()
                .mouse_state
                .cursor_position
                .get_position();
            if let Some(pos) = mouse_pos {
                self.update_hit_test_at(pos);
            }

            // Re-process events with the new mouse state
            // This will detect the mouse state change and invoke appropriate callbacks
            let nested_result = self.process_window_events_recursive_v2(0);
            event_result = event_result.max(nested_result);
        }
        
        // If keyboard_state changed, trigger event processing to invoke callbacks
        // This enables synthetic keyboard events from debug API (Tab, Enter, etc.)
        if keyboard_state_changed && !mouse_state_changed {
            // Re-process events with the new keyboard state
            // This will detect the keyboard state change and invoke appropriate callbacks
            let nested_result = self.process_window_events_recursive_v2(0);
            event_result = event_result.max(nested_result);
        }

        // Handle queued window state sequence (for simulating clicks, etc.)
        // Each state is applied in order, with event processing between states
        // to detect the transitions (e.g., mouse down → mouse up)
        if !result.queued_window_states.is_empty() {
            for (i, queued_state) in result.queued_window_states.iter().enumerate() {
                // Save current state as previous
                let old_state = self.get_current_window_state().clone();
                self.set_previous_window_state(old_state.clone());

                // Apply the queued state
                let current_state = self.get_current_window_state_mut();
                current_state.mouse_state = queued_state.mouse_state.clone();
                current_state.keyboard_state = queued_state.keyboard_state.clone();
                current_state.title = queued_state.title.clone();
                current_state.size = queued_state.size;
                current_state.position = queued_state.position;
                current_state.flags = queued_state.flags;

                // Update hit testing at the new mouse position
                let mouse_pos = queued_state.mouse_state.cursor_position.get_position();
                if let Some(pos) = mouse_pos {
                    self.update_hit_test_at(pos);
                }

                // Process events with this state (will detect state changes)
                let nested_result = self.process_window_events_recursive_v2(0);
                event_result = event_result.max(nested_result);
            }
        }

        // Handle focus changes
        use azul_layout::callbacks::FocusUpdateRequest;
        match result.update_focused_node {
            FocusUpdateRequest::FocusNode(new_focus) => {
                // Update focus in the FocusManager (in LayoutWindow)
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window
                        .focus_manager
                        .set_focused_node(Some(new_focus));
                    
                    // SCROLL INTO VIEW: Scroll newly focused node into visible area
                    use azul_layout::managers::scroll_into_view::ScrollIntoViewOptions;
                    let now = azul_core::task::Instant::now();
                    layout_window.scroll_node_into_view(
                        new_focus,
                        ScrollIntoViewOptions::nearest(),
                        now,
                    );
                    
                    // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
                    let window_state = layout_window.current_window_state.clone();
                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                        Some(new_focus),
                        &window_state,
                    );
                    
                    match timer_action {
                        azul_layout::CursorBlinkTimerAction::Start(timer) => {
                            self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
                        }
                        azul_layout::CursorBlinkTimerAction::Stop => {
                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                        }
                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                    }
                }
                event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
            FocusUpdateRequest::ClearFocus => {
                // Clear focus in the FocusManager (in LayoutWindow)
                if let Some(layout_window) = self.get_layout_window_mut() {
                    layout_window.focus_manager.set_focused_node(None);
                    
                    // CURSOR BLINK TIMER: Stop timer when focus is cleared
                    let window_state = layout_window.current_window_state.clone();
                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                        None,
                        &window_state,
                    );
                    
                    match timer_action {
                        azul_layout::CursorBlinkTimerAction::Start(_timer) => {
                            // Shouldn't happen when clearing focus, but handle it
                        }
                        azul_layout::CursorBlinkTimerAction::Stop => {
                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
                        }
                        azul_layout::CursorBlinkTimerAction::NoChange => {}
                    }
                }
                event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
            }
            FocusUpdateRequest::NoChange => {
                // No focus change requested
            }
        }

        // Handle scroll position changes from callbacks (e.g., scroll_node_by API)
        if let Some(ref nodes_scrolled) = result.nodes_scrolled_in_callbacks {
            if !nodes_scrolled.is_empty() {
                // Get current time for scroll animation
                let external = azul_layout::callbacks::ExternalSystemCallbacks::rust_internal();
                let now = (external.get_system_time_fn.cb)();

                if let Some(layout_window) = self.get_layout_window_mut() {
                    for (dom_id, node_map) in nodes_scrolled {
                        for (hierarchy_id, target_position) in node_map {
                            // Convert NodeHierarchyItemId to NodeId
                            if let Some(node_id) = hierarchy_id.into_crate_internal() {
                                // Use instant scroll (duration = 0) for programmatic scrolling
                                layout_window.scroll_manager.scroll_to(
                                    *dom_id,
                                    node_id,
                                    *target_position,
                                    std::time::Duration::from_millis(0).into(),
                                    azul_core::events::EasingFunction::Linear,
                                    now.clone().into(),
                                );
                            }
                        }
                    }
                    // Scrolling changes require re-render
                    event_result =
                        event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                }
            }
        }

        // Handle image updates
        if result.images_changed.is_some() || result.image_masks_changed.is_some() {
            event_result =
                event_result.max(ProcessEventResult::ShouldUpdateDisplayListCurrentWindow);
        }

        // Handle timers and threads
        if result.timers.is_some()
            || result.timers_removed.is_some()
            || result.threads.is_some()
            || result.threads_removed.is_some()
        {
            // Process timers - call platform-specific start/stop methods
            if let Some(timers) = &result.timers {
                for (timer_id, timer) in timers {
                    self.start_timer(timer_id.id, timer.clone());
                }
            }

            if let Some(timers_removed) = &result.timers_removed {
                for timer_id in timers_removed {
                    self.stop_timer(timer_id.id);
                }
            }

            // Process threads - add/remove from layout_window and manage polling timer
            let should_start_thread_timer;
            let should_stop_thread_timer;

            // First, check if we had threads before
            let had_threads = if let Some(layout_window) = self.get_layout_window() {
                !layout_window.threads.is_empty()
            } else {
                false
            };

            // Add new threads
            if let Some(threads) = result.threads.clone() {
                self.add_threads(threads);
            }

            // Remove old threads
            if let Some(threads_removed) = &result.threads_removed {
                self.remove_threads(threads_removed);
            }

            // Now check if we have threads after modifications
            let has_threads = if let Some(layout_window) = self.get_layout_window() {
                !layout_window.threads.is_empty()
            } else {
                false
            };

            // Determine if we need to start/stop the thread polling timer
            should_start_thread_timer = !had_threads && has_threads;
            should_stop_thread_timer = had_threads && !has_threads;

            // Start thread polling timer if we now have threads
            if should_start_thread_timer {
                self.start_thread_poll_timer();
            }

            // Stop thread polling timer if we no longer have threads
            if should_stop_thread_timer {
                self.stop_thread_poll_timer();
            }

            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle new windows spawned in callbacks
        if !result.windows_created.is_empty() {
            // TODO: Signal to event loop to create new windows
            // For now, just log
            log_debug!(
                super::debug_server::LogCategory::Window,
                "[PlatformWindowV2] {} new windows requested (not yet implemented)",
                result.windows_created.len()
            );
        }

        // Handle menus requested to be opened
        if !result.menus_to_open.is_empty() {
            for (menu, position_override) in &result.menus_to_open {
                // Use override position if provided, otherwise use (0, 0) as default
                // The Menu.position field is a MenuPopupPosition enum (AutoCursor, etc.),
                // not a specific coordinate. For callback-opened menus, the position_override
                // specifies where to show it.
                let position = position_override.unwrap_or(LogicalPosition::new(0.0, 0.0));

                // Show menu (native or fallback based on flags)
                self.show_menu_from_callback(menu, position);
            }
            event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
        }

        // Handle tooltip show requests
        if !result.tooltips_to_show.is_empty() {
            // Show only the last tooltip requested (if multiple were requested in one callback)
            if let Some((text, position)) = result.tooltips_to_show.last() {
                self.show_tooltip_from_callback(text.as_str(), *position);
            }
        }

        // Handle tooltip hide request
        if result.hide_tooltip {
            self.hide_tooltip_from_callback();
        }

        // Handle explicit hit test update request (from Debug API)
        // This is separate from mouse_state_changed to allow explicit hit test updates
        // without modifying mouse position
        if let Some(position) = result.hit_test_update_requested {
            self.update_hit_test_at(position);
        }

        // Process text_input_triggered from CreateTextInput
        // This is how debug server text input flows:
        // 1. debug_timer_callback calls callback_info.create_text_input(text)
        // 2. apply_callback_changes processes CreateTextInput
        // 3. process_text_input() is called, returning affected nodes
        // 4. text_input_triggered is populated and forwarded here
        // 5. We trigger recursive event processing to invoke user callbacks
        if !result.text_input_triggered.is_empty() {
            println!("[process_callback_result_v2] Processing {} text_input_triggered events", result.text_input_triggered.len());
            
            // For each affected node, invoke OnTextInput callbacks
            // User callbacks can intercept via preventDefault
            for (dom_node_id, event_filters) in &result.text_input_triggered {
                println!("[process_callback_result_v2] Node {:?} triggered {} event filters", dom_node_id, event_filters.len());
                
                // Convert DomNodeId to CallbackTarget
                if let Some(node_id) = dom_node_id.node.into_crate_internal() {
                    let callback_target = CallbackTarget::Node(HitTestNode {
                        dom_id: dom_node_id.dom.inner as u64,
                        node_id: node_id.index() as u64,
                    });
                    
                    // Invoke callbacks for each event filter (typically OnTextInput)
                    for event_filter in event_filters {
                        println!("[process_callback_result_v2] Invoking callback for {:?}", event_filter);
                        let callback_results = self.invoke_callbacks_v2(callback_target.clone(), event_filter.clone());
                        
                        // Process callback results
                        for callback_result in &callback_results {
                            if callback_result.prevent_default {
                                println!("[process_callback_result_v2] preventDefault called - text input will be rejected");
                                // TODO: Clear the pending changeset if rejected
                            }
                            
                            // Check if we need to update the screen
                            if matches!(callback_result.callbacks_update_screen, Update::RefreshDom | Update::RefreshDomAllWindows) {
                                event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
                            }
                        }
                    }
                }
            }
            
            // After processing callbacks, apply the text changeset if not rejected
            // This updates the visual cache
            if let Some(layout_window) = self.get_layout_window_mut() {
                let dirty_nodes = layout_window.apply_text_changeset();
                if !dirty_nodes.is_empty() {
                    println!("[process_callback_result_v2] Applied text changeset, {} dirty nodes", dirty_nodes.len());
                    event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                    
                    // CRITICAL FIX: Scroll cursor into view after text edit
                    // Without this, typing at the end of a long text doesn't scroll
                    // the view to keep the cursor visible.
                    layout_window.scroll_selection_into_view(
                        azul_layout::window::SelectionScrollType::Cursor,
                        azul_layout::window::ScrollMode::Instant,
                    );
                }
            }
        }

        // Process Update screen command
        match result.callbacks_update_screen {
            Update::DoNothing => {}
            Update::RefreshDom => {
                self.mark_frame_needs_regeneration();
                event_result =
                    event_result.max(ProcessEventResult::ShouldRegenerateDomCurrentWindow);
            }
            Update::RefreshDomAllWindows => {
                self.mark_frame_needs_regeneration();
                event_result = event_result.max(ProcessEventResult::ShouldRegenerateDomAllWindows);
            }
        }

        event_result
    }

    /// Perform scrollbar hit-test at the given position.
    ///
    /// Returns `Some(ScrollbarHitId)` if a scrollbar was hit, `None` otherwise.
    ///
    /// This uses WebRender's hit-tester to check for scrollbar tags.
    fn perform_scrollbar_hit_test(
        &self,
        position: azul_core::geom::LogicalPosition,
    ) -> Option<azul_core::hit_test::ScrollbarHitId> {
        use webrender::api::units::WorldPoint;

        let hit_tester = match self.get_hit_tester() {
            AsyncHitTester::Resolved(ht) => ht,
            _ => return None,
        };

        let world_point = WorldPoint::new(position.x, position.y);
        let hit_result = hit_tester.hit_test(world_point);

        // Check each hit item for scrollbar tag
        for item in hit_result.items.iter() {
            if let Some(scrollbar_id) =
                wr_translate2::translate_item_tag_to_scrollbar_hit_id(item.tag)
            {
                return Some(scrollbar_id);
            }
        }

        None
    }

    /// Handle scrollbar click (thumb or track).
    ///
    /// Returns `ProcessEventResult` indicating whether to redraw.
    fn handle_scrollbar_click(
        &mut self,
        hit_id: azul_core::hit_test::ScrollbarHitId,
        position: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::hit_test::ScrollbarHitId;

        match hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id)
            | ScrollbarHitId::HorizontalThumb(dom_id, node_id) => {
                // Start drag
                let layout_window = match self.get_layout_window() {
                    Some(lw) => lw,
                    None => return ProcessEventResult::DoNothing,
                };

                let scroll_offset = layout_window
                    .scroll_manager
                    .get_current_offset(dom_id, node_id)
                    .unwrap_or_default();

                self.set_scrollbar_drag_state(Some(ScrollbarDragState {
                    hit_id,
                    initial_mouse_pos: position,
                    initial_scroll_offset: scroll_offset,
                }));

                ProcessEventResult::ShouldReRenderCurrentWindow
            }

            ScrollbarHitId::VerticalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, true)
            }

            ScrollbarHitId::HorizontalTrack(dom_id, node_id) => {
                self.handle_track_click(dom_id, node_id, position, false)
            }
        }
    }

    /// Handle track click - jump scroll to clicked position.
    fn handle_track_click(
        &mut self,
        dom_id: DomId,
        node_id: CoreNodeId,
        click_position: azul_core::geom::LogicalPosition,
        is_vertical: bool,
    ) -> ProcessEventResult {
        use azul_core::dom::ScrollbarOrientation;

        // Get scrollbar state to calculate target position
        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Get current scrollbar geometry
        let scrollbar_state = if is_vertical {
            layout_window.scroll_manager.get_scrollbar_state(
                dom_id,
                node_id,
                ScrollbarOrientation::Vertical,
            )
        } else {
            layout_window.scroll_manager.get_scrollbar_state(
                dom_id,
                node_id,
                ScrollbarOrientation::Horizontal,
            )
        };

        let scrollbar_state = match scrollbar_state {
            Some(s) if s.visible => s,
            _ => return ProcessEventResult::DoNothing,
        };

        // Get current scroll state
        let scroll_state = match layout_window
            .scroll_manager
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate which position on the track was clicked (0.0 = top/left, 1.0 = bottom/right)
        let click_ratio = if is_vertical {
            let track_top = scrollbar_state.track_rect.origin.y;
            let track_height = scrollbar_state.track_rect.size.height;
            ((click_position.y - track_top) / track_height).clamp(0.0, 1.0)
        } else {
            let track_left = scrollbar_state.track_rect.origin.x;
            let track_width = scrollbar_state.track_rect.size.width;
            ((click_position.x - track_left) / track_width).clamp(0.0, 1.0)
        };

        // Calculate target scroll position
        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);
        let target_scroll = click_ratio * max_scroll;

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let scroll_delta = target_scroll - current_scroll;

        // Apply scroll using gpu_scroll
        if let Err(e) = self.gpu_scroll(
            dom_id,
            node_id,
            if is_vertical { 0.0 } else { scroll_delta },
            if is_vertical { scroll_delta } else { 0.0 },
        ) {
            log_warn!(
                super::debug_server::LogCategory::Input,
                "Track click scroll failed: {}",
                e
            );
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }

    // PROVIDED: Timer Invocation (Cross-Platform Implementation)

    /// Invoke all expired timer callbacks.
    ///
    /// This method checks for expired timers via `tick_timers()` and invokes
    /// the callback for each expired timer using `run_single_timer()`.
    ///
    /// ## Returns
    /// * `Vec<CallCallbacksResult>` - Results from all invoked timer callbacks
    ///
    /// ## Platform Usage
    /// Call this from platform event loops when:
    /// - **Windows**: In `WM_TIMER` handler
    /// - **macOS**: In `performSelector:withObject:afterDelay:` callback
    /// - **X11**: After `select()` timeout
    /// - **Wayland**: After `timerfd` read
    fn invoke_expired_timers(&mut self) -> Vec<azul_layout::callbacks::CallCallbacksResult> {
        use azul_core::callbacks::Update;
        use azul_core::task::TimerId;
        use azul_layout::callbacks::{CallCallbacksResult, ExternalSystemCallbacks};

        // Get current system time
        let system_callbacks = ExternalSystemCallbacks::rust_internal();
        let current_time = (system_callbacks.get_system_time_fn.cb)();
        let frame_start: azul_core::task::Instant = current_time.clone().into();

        // First, get expired timer IDs without borrowing self
        let expired_timer_ids: Vec<TimerId> = {
            let layout_window = match self.get_layout_window_mut() {
                Some(lw) => lw,
                None => return Vec::new(),
            };
            layout_window.tick_timers(current_time)
        };

        if expired_timer_ids.is_empty() {
            return Vec::new();
        }

        let mut all_results = Vec::new();

        // Process each expired timer
        for timer_id in expired_timer_ids {
            // Prepare borrows fresh for each timer invocation
            let mut borrows = self.prepare_callback_invocation();

            let result = borrows.layout_window.run_single_timer(
                timer_id.id,
                frame_start.clone(),
                &borrows.window_handle,
                borrows.gl_context_ptr,
                borrows.image_cache,
                &mut borrows.fc_cache_clone,
                borrows.system_style.clone(),
                &ExternalSystemCallbacks::rust_internal(),
                borrows.previous_window_state,
                borrows.current_window_state,
                borrows.renderer_resources,
            );

            // Apply results: add new timers/threads, remove terminated ones
            if let Some(ref new_timers) = result.timers {
                for (timer_id, timer) in new_timers {
                    borrows
                        .layout_window
                        .timers
                        .insert(*timer_id, timer.clone());
                }
            }
            if let Some(ref removed_timers) = result.timers_removed {
                for timer_id in removed_timers {
                    borrows.layout_window.timers.remove(timer_id);
                }
            }
            if let Some(ref new_threads) = result.threads {
                for (thread_id, thread) in new_threads {
                    borrows
                        .layout_window
                        .threads
                        .insert(*thread_id, thread.clone());
                }
            }
            if let Some(ref removed_threads) = result.threads_removed {
                for thread_id in removed_threads {
                    borrows.layout_window.threads.remove(thread_id);
                }
            }

            // Mark frame for redraw if callback requested it
            if result.callbacks_update_screen == Update::RefreshDom
                || result.callbacks_update_screen == Update::RefreshDomAllWindows
            {
                self.mark_frame_needs_regeneration();
            }

            all_results.push(result);
        }

        all_results
    }

    // PROVIDED: Thread Callback Invocation (Cross-Platform Implementation)

    /// Invoke all pending thread callbacks (writeback messages).
    ///
    /// This method polls all active threads for completed work and invokes
    /// the writeback callbacks for any threads that have finished.
    ///
    /// ## Returns
    /// * `Option<CallCallbacksResult>` - Combined result from all thread writeback callbacks, or None if no threads processed
    ///
    /// ## Platform Usage
    /// Call this from platform event loops when:
    /// - **Windows**: In `WM_TIMER` handler with thread timer ID (0xFFFF)
    /// - **macOS**: In thread poll timer callback (NSTimer every 16ms)
    /// - **X11**: After `select()` timeout when threads exist
    /// - **Wayland**: After thread timerfd read
    fn invoke_thread_callbacks(&mut self) -> Option<azul_layout::callbacks::CallCallbacksResult> {
        use azul_layout::callbacks::ExternalSystemCallbacks;

        // Check if we have threads to poll
        let has_threads = {
            let layout_window = match self.get_layout_window() {
                Some(lw) => lw,
                None => return None,
            };
            !layout_window.threads.is_empty()
        };

        if !has_threads {
            return None;
        }

        // Get app_data from the platform window (shared across all windows)
        let app_data_arc = self.get_app_data().clone();

        // Prepare borrows for thread invocation
        let mut borrows = self.prepare_callback_invocation();

        // Call run_all_threads on the layout_window
        let mut app_data = app_data_arc.borrow_mut();
        let result = borrows.layout_window.run_all_threads(
            &mut *app_data,
            &borrows.window_handle,
            borrows.gl_context_ptr,
            borrows.image_cache,
            &mut borrows.fc_cache_clone,
            borrows.system_style.clone(),
            &ExternalSystemCallbacks::rust_internal(),
            borrows.previous_window_state,
            borrows.current_window_state,
            borrows.renderer_resources,
        );

        Some(result)
    }

    /// Handle scrollbar drag - update scroll position based on mouse delta.
    fn handle_scrollbar_drag(
        &mut self,
        current_pos: azul_core::geom::LogicalPosition,
    ) -> ProcessEventResult {
        use azul_core::dom::ScrollbarOrientation;
        use azul_core::hit_test::ScrollbarHitId;

        let drag_state = match self.get_scrollbar_drag_state() {
            Some(ds) => ds.clone(),
            None => return ProcessEventResult::DoNothing,
        };

        let layout_window = match self.get_layout_window() {
            Some(lw) => lw,
            None => return ProcessEventResult::DoNothing,
        };

        // Calculate delta
        let (dom_id, node_id, is_vertical) = match drag_state.hit_id {
            ScrollbarHitId::VerticalThumb(dom_id, node_id) => (dom_id, node_id, true),
            ScrollbarHitId::HorizontalThumb(dom_id, node_id) => (dom_id, node_id, false),
            _ => return ProcessEventResult::DoNothing,
        };

        let pixel_delta = if is_vertical {
            current_pos.y - drag_state.initial_mouse_pos.y
        } else {
            current_pos.x - drag_state.initial_mouse_pos.x
        };

        // Get scrollbar geometry
        let orientation = if is_vertical {
            ScrollbarOrientation::Vertical
        } else {
            ScrollbarOrientation::Horizontal
        };

        let scrollbar_state =
            match layout_window
                .scroll_manager
                .get_scrollbar_state(dom_id, node_id, orientation)
            {
                Some(s) if s.visible => s,
                _ => return ProcessEventResult::DoNothing,
            };

        let scroll_state = match layout_window
            .scroll_manager
            .get_scroll_state(dom_id, node_id)
        {
            Some(s) => s,
            None => return ProcessEventResult::DoNothing,
        };

        // Convert pixel delta to scroll delta
        // pixel_delta / track_size = scroll_delta / max_scroll
        let track_size = if is_vertical {
            scrollbar_state.track_rect.size.height
        } else {
            scrollbar_state.track_rect.size.width
        };

        let container_size = if is_vertical {
            scroll_state.container_rect.size.height
        } else {
            scroll_state.container_rect.size.width
        };

        let content_size = if is_vertical {
            scroll_state.content_rect.size.height
        } else {
            scroll_state.content_rect.size.width
        };

        let max_scroll = (content_size - container_size).max(0.0);

        // Account for thumb size: usable track size is track_size - thumb_size
        let thumb_size = scrollbar_state.thumb_size_ratio * track_size;
        let usable_track_size = (track_size - thumb_size).max(1.0);

        // Calculate scroll delta
        let scroll_delta = if usable_track_size > 0.0 {
            (pixel_delta / usable_track_size) * max_scroll
        } else {
            0.0
        };

        // Calculate target scroll position (initial + delta from drag start)
        let target_scroll = if is_vertical {
            drag_state.initial_scroll_offset.y + scroll_delta
        } else {
            drag_state.initial_scroll_offset.x + scroll_delta
        };

        // Clamp to valid range
        let target_scroll = target_scroll.clamp(0.0, max_scroll);

        // Calculate delta from current position
        let current_scroll = if is_vertical {
            scroll_state.current_offset.y
        } else {
            scroll_state.current_offset.x
        };

        let delta_from_current = target_scroll - current_scroll;

        // Use gpu_scroll to update scroll position
        if let Err(e) = self.gpu_scroll(
            dom_id,
            node_id,
            if is_vertical { 0.0 } else { delta_from_current },
            if is_vertical { delta_from_current } else { 0.0 },
        ) {
            log_warn!(
                super::debug_server::LogCategory::Input,
                "Scrollbar drag failed: {}",
                e
            );
            return ProcessEventResult::DoNothing;
        }

        ProcessEventResult::ShouldReRenderCurrentWindow
    }
}

/// Checks if any hit node has the CSD titlebar class ("csd-title" or "csd-titlebar").
///
/// This is used to determine if a drag gesture should activate window dragging.
fn is_hit_on_titlebar(
    hit_test: &azul_core::hit_test::FullHitTest,
    layout_window: &LayoutWindow,
) -> bool {
    use azul_core::dom::IdOrClass;

    for (dom_id, hit) in hit_test.hovered_nodes.iter() {
        let (node_id, _hit_item) = match hit.regular_hit_test_nodes.iter().next() {
            Some(n) => n,
            None => continue,
        };

        let layout_result = match layout_window.layout_results.get(dom_id) {
            Some(lr) => lr,
            None => continue,
        };

        let binding = layout_result.styled_dom.node_data.as_container();
        let node_data = match binding.get(*node_id) {
            Some(nd) => nd,
            None => continue,
        };

        let has_titlebar_class = node_data.get_ids_and_classes().as_ref().iter().any(
            |id_or_class| {
                matches!(
                    id_or_class,
                    IdOrClass::Class(c) if c.as_str() == "csd-title" || c.as_str() == "csd-titlebar"
                )
            },
        );

        if has_titlebar_class {
            return true;
        }
    }

    false
}

```

## layout/src/managers/text_input.rs
// Text input manager - records changesets
// 256 lines

```rust
//! Text Input Manager
//!
//! Centralizes all text editing logic for contenteditable nodes.
//!
//! This manager handles text input from multiple sources:
//!
//! - Keyboard input (character insertion, backspace, etc.)
//! - IME composition (multi-character input for Asian languages)
//! - Accessibility actions (screen readers, voice control)
//! - Programmatic edits (from callbacks)
//!
//! ## Architecture
//!
//! The text input system uses a two-phase approach:
//!
//! 1. **Record Phase**: When text input occurs, record what changed (old_text + inserted_text)
//!
//!    - Store in `pending_changeset`
//!    - Do NOT modify any caches yet
//!    - Return affected nodes so callbacks can be invoked
//!
//! 2. **Apply Phase**: After callbacks, if preventDefault was not set:
//!
//!    - Compute new text using text3::edit
//!    - Update cursor position
//!    - Update text cache
//!    - Mark nodes dirty for re-layout
//!
//! This separation allows:
//!
//! - User callbacks to inspect the changeset before it's applied
//! - preventDefault to cancel the edit
//! - Consistent behavior across keyboard/IME/A11y sources

use std::collections::BTreeMap;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    events::{EventData, EventProvider, EventSource as CoreEventSource, EventType, SyntheticEvent},
    selection::TextCursor,
    task::Instant,
};
use azul_css::corety::AzString;

/// Information about a pending text edit that hasn't been applied yet
#[derive(Debug, Clone)]
#[repr(C)]
pub struct PendingTextEdit {
    /// The node that was edited
    pub node: DomNodeId,
    /// The text that was inserted
    pub inserted_text: AzString,
    /// The old text before the edit (plain text extracted from InlineContent)
    pub old_text: AzString,
}

impl PendingTextEdit {
    /// Compute the resulting text after applying the edit
    ///
    /// This is a pure function that applies the inserted_text to old_text
    /// using the current cursor position.
    ///
    /// NOTE: Actual text application is handled by apply_text_changeset() in window.rs
    /// which uses text3::edit::insert_text() for proper cursor-based insertion.
    /// This method is for preview/inspection purposes only.
    pub fn resulting_text(&self, cursor: Option<&TextCursor>) -> AzString {
        // For preview: append the inserted text
        // Actual insertion at cursor is done by text3::edit::insert_text()
        let mut result = self.old_text.as_str().to_string();
        result.push_str(self.inserted_text.as_str());

        let _ = cursor; // Preview doesn't need cursor - actual insert does

        result.into()
    }
}

/// C-compatible Option type for PendingTextEdit
#[derive(Debug, Clone)]
#[repr(C, u8)]
pub enum OptionPendingTextEdit {
    None,
    Some(PendingTextEdit),
}

impl OptionPendingTextEdit {
    pub fn into_option(self) -> Option<PendingTextEdit> {
        match self {
            OptionPendingTextEdit::None => None,
            OptionPendingTextEdit::Some(t) => Some(t),
        }
    }
}

impl From<Option<PendingTextEdit>> for OptionPendingTextEdit {
    fn from(o: Option<PendingTextEdit>) -> Self {
        match o {
            Some(v) => OptionPendingTextEdit::Some(v),
            None => OptionPendingTextEdit::None,
        }
    }
}

impl<'a> From<Option<&'a PendingTextEdit>> for OptionPendingTextEdit {
    fn from(o: Option<&'a PendingTextEdit>) -> Self {
        match o {
            Some(v) => OptionPendingTextEdit::Some(v.clone()),
            None => OptionPendingTextEdit::None,
        }
    }
}

/// Source of a text input event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextInputSource {
    /// Regular keyboard input
    Keyboard,
    /// IME composition (multi-character input)
    Ime,
    /// Accessibility action from assistive technology
    Accessibility,
    /// Programmatic edit from user callback
    Programmatic,
}

/// Text Input Manager
///
/// Centralizes all text editing logic. This is the single source of truth
/// for text input state.
pub struct TextInputManager {
    /// The pending text changeset that hasn't been applied yet.
    /// This is set during the "record" phase and cleared after the "apply" phase.
    pub pending_changeset: Option<PendingTextEdit>,
    /// Source of the current text input
    pub input_source: Option<TextInputSource>,
}

impl TextInputManager {
    /// Create a new TextInputManager
    pub fn new() -> Self {
        Self {
            pending_changeset: None,
            input_source: None,
        }
    }

    /// Record a text input event (Phase 1)
    ///
    /// This ONLY records what text was inserted. It does NOT apply the changes yet.
    /// The changes are applied later in `apply_changeset()` if preventDefault is not set.
    ///
    /// # Arguments
    ///
    /// - `node` - The DOM node being edited
    /// - `inserted_text` - The text being inserted
    /// - `old_text` - The current text before the edit
    /// - `source` - Where the input came from (keyboard, IME, A11y, etc.)
    ///
    /// Returns the affected node for event generation.
    pub fn record_input(
        &mut self,
        node: DomNodeId,
        inserted_text: String,
        old_text: String,
        source: TextInputSource,
    ) -> DomNodeId {
        println!("[TextInputManager::record_input] Recording input for node {:?}", node);
        println!("[TextInputManager::record_input] Inserted text: '{}', old_text len: {}", inserted_text, old_text.len());
        println!("[TextInputManager::record_input] Source: {:?}", source);

        // Clear any previous changeset
        self.pending_changeset = None;

        // Store the new changeset
        self.pending_changeset = Some(PendingTextEdit {
            node,
            inserted_text: inserted_text.into(),
            old_text: old_text.into(),
        });

        self.input_source = Some(source);
        println!("[TextInputManager::record_input] Changeset stored successfully");

        node
    }

    /// Get the pending changeset (if any)
    pub fn get_pending_changeset(&self) -> Option<&PendingTextEdit> {
        let result = self.pending_changeset.as_ref();
        if result.is_some() {
            println!("[TextInputManager::get_pending_changeset] Returning pending changeset");
        } else {
            println!("[TextInputManager::get_pending_changeset] No pending changeset!");
        }
        result
    }

    /// Clear the pending changeset
    ///
    /// This is called after applying the changeset or if preventDefault was set.
    pub fn clear_changeset(&mut self) {
        println!("[TextInputManager::clear_changeset] Clearing changeset");
        self.pending_changeset = None;
        self.input_source = None;
    }

    /// Check if there's a pending changeset that needs to be applied
    pub fn has_pending_changeset(&self) -> bool {
        self.pending_changeset.is_some()
    }
}

impl Default for TextInputManager {
    fn default() -> Self {
        Self::new()
    }
}

impl EventProvider for TextInputManager {
    /// Get pending text input events.
    ///
    /// If there's a pending changeset, returns an Input event for the affected node.
    /// The event data includes the old text and inserted text so callbacks can
    /// query the changeset.
    fn get_pending_events(&self, timestamp: Instant) -> Vec<SyntheticEvent> {
        let mut events = Vec::new();

        if let Some(changeset) = &self.pending_changeset {
            let event_source = match self.input_source {
                Some(TextInputSource::Keyboard) | Some(TextInputSource::Ime) => {
                    CoreEventSource::User
                }
                Some(TextInputSource::Accessibility) => CoreEventSource::User, /* A11y is still */
                // user input
                Some(TextInputSource::Programmatic) => CoreEventSource::Programmatic,
                None => CoreEventSource::User,
            };

            // Generate Input event (fires on every keystroke)
            events.push(SyntheticEvent::new(
                EventType::Input,
                event_source,
                changeset.node,
                timestamp,
                // Callbacks can query changeset via
                // text_input_manager.get_pending_changeset()
                EventData::None,
            ));

            // Note: We don't generate Change events here - those are generated
            // when focus is lost or Enter is pressed (handled elsewhere)
        }

        events
    }
}

```

## layout/src/managers/focus_cursor.rs
// Focus and cursor manager
// 679 lines

```rust
//! Focus and tab navigation management.
//!
//! Manages keyboard focus, tab navigation, and programmatic focus changes
//! with a recursive event system for focus/blur callbacks (max depth: 5).

use alloc::{collections::BTreeMap, vec::Vec};

use azul_core::{
    callbacks::{FocusTarget, FocusTargetPath},
    dom::{DomId, DomNodeId, NodeId},
    style::matches_html_element,
    styled_dom::NodeHierarchyItemId,
};

use crate::window::DomLayoutResult;

/// CSS path for selecting elements (placeholder - needs proper implementation)
pub type CssPathString = alloc::string::String;

/// Information about a pending contenteditable focus that needs cursor initialization
/// after layout is complete (W3C "flag and defer" pattern).
///
/// This is set during focus event handling and consumed after layout pass.
#[derive(Debug, Clone, PartialEq)]
pub struct PendingContentEditableFocus {
    /// The DOM where the contenteditable element is
    pub dom_id: DomId,
    /// The contenteditable container node that received focus
    pub container_node_id: NodeId,
    /// The text node where the cursor should be placed (often a child of the container)
    pub text_node_id: NodeId,
}

/// Manager for keyboard focus and tab navigation
///
/// Note: Text cursor management is now handled by the separate `CursorManager`.
///
/// The `FocusManager` only tracks which node has focus, while `CursorManager`
/// tracks the cursor position within that node (if it's contenteditable).
///
/// ## W3C Focus/Selection Model
///
/// The W3C model maintains a strict separation between **keyboard focus** and **selection**:
///
/// 1. **Focus** lands on the contenteditable container (`document.activeElement`)
/// 2. **Selection/Cursor** is placed in a descendant text node (`Selection.focusNode`)
///
/// This separation requires a "flag and defer" pattern:
/// - During focus event: Set `cursor_needs_initialization = true`
/// - After layout pass: Call `finalize_pending_focus_changes()` to actually initialize the cursor
///
/// This is necessary because cursor positioning requires text layout information,
/// which isn't available during the focus event handling phase.
#[derive(Debug, Clone, PartialEq)]
pub struct FocusManager {
    /// Currently focused node (if any)
    pub focused_node: Option<DomNodeId>,
    /// Pending focus request from callback
    pub pending_focus_request: Option<FocusTarget>,
    
    // --- W3C "flag and defer" pattern fields ---
    
    /// Flag indicating that cursor initialization is pending (set during focus, consumed after layout)
    pub cursor_needs_initialization: bool,
    /// Information about the pending contenteditable focus
    pub pending_contenteditable_focus: Option<PendingContentEditableFocus>,
}

impl Default for FocusManager {
    fn default() -> Self {
        Self::new()
    }
}

impl FocusManager {
    /// Create a new focus manager
    pub fn new() -> Self {
        Self {
            focused_node: None,
            pending_focus_request: None,
            cursor_needs_initialization: false,
            pending_contenteditable_focus: None,
        }
    }

    /// Get the currently focused node
    pub fn get_focused_node(&self) -> Option<&DomNodeId> {
        self.focused_node.as_ref()
    }

    /// Set the focused node directly (used by event system)
    ///
    /// Note: Cursor initialization/clearing is now handled by `CursorManager`.
    /// The event system should check if the newly focused node is contenteditable
    /// and call `CursorManager::initialize_cursor_at_end()` if needed.
    pub fn set_focused_node(&mut self, node: Option<DomNodeId>) {
        self.focused_node = node;
    }

    /// Request a focus change (to be processed by event system)
    pub fn request_focus_change(&mut self, target: FocusTarget) {
        self.pending_focus_request = Some(target);
    }

    /// Take the pending focus request (one-shot)
    pub fn take_focus_request(&mut self) -> Option<FocusTarget> {
        self.pending_focus_request.take()
    }

    /// Clear focus
    pub fn clear_focus(&mut self) {
        self.focused_node = None;
    }

    /// Check if a specific node has focus
    pub fn has_focus(&self, node: &DomNodeId) -> bool {
        self.focused_node.as_ref() == Some(node)
    }
    
    // --- W3C "flag and defer" pattern methods ---
    
    /// Mark that cursor initialization is needed for a contenteditable element.
    ///
    /// This is called during focus event handling. The actual cursor initialization
    /// happens later in `finalize_pending_focus_changes()` after layout is complete.
    ///
    /// # W3C Conformance
    ///
    /// In the W3C model, when focus lands on a contenteditable element:
    /// 1. The focus event fires on the container element
    /// 2. The browser's editing engine modifies the Selection to place a caret
    /// 3. The Selection's anchorNode/focusNode point to the child text node
    ///
    /// Since we need layout information to position the cursor, we defer step 2+3.
    pub fn set_pending_contenteditable_focus(
        &mut self,
        dom_id: DomId,
        container_node_id: NodeId,
        text_node_id: NodeId,
    ) {
        self.cursor_needs_initialization = true;
        self.pending_contenteditable_focus = Some(PendingContentEditableFocus {
            dom_id,
            container_node_id,
            text_node_id,
        });
    }
    
    /// Clear the pending contenteditable focus (when focus moves away or is cleared).
    pub fn clear_pending_contenteditable_focus(&mut self) {
        self.cursor_needs_initialization = false;
        self.pending_contenteditable_focus = None;
    }
    
    /// Take the pending contenteditable focus (consumes the flag).
    ///
    /// Returns `Some(info)` if cursor initialization is pending, `None` otherwise.
    /// After calling this, `cursor_needs_initialization` is set to `false`.
    pub fn take_pending_contenteditable_focus(&mut self) -> Option<PendingContentEditableFocus> {
        if self.cursor_needs_initialization {
            self.cursor_needs_initialization = false;
            self.pending_contenteditable_focus.take()
        } else {
            None
        }
    }
    
    /// Check if cursor initialization is pending.
    pub fn needs_cursor_initialization(&self) -> bool {
        self.cursor_needs_initialization
    }
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

/// Error returned when cursor navigation cannot find a valid destination.
///
/// This occurs when attempting to move the cursor (e.g., arrow keys in a
/// contenteditable element) but no valid target position exists, such as
/// when already at the start/end of text content.
#[derive(Debug, Clone)]
pub struct NoCursorDestination {
    /// Human-readable explanation of why navigation failed
    pub reason: String,
}

/// Warning/error type for focus resolution failures.
///
/// Returned by `resolve_focus_target` when the requested focus target
/// cannot be resolved to a valid focusable node.
#[derive(Debug, Clone, PartialEq)]
pub enum UpdateFocusWarning {
    /// The specified DOM ID does not exist in the layout results
    FocusInvalidDomId(DomId),
    /// The specified node ID does not exist within its DOM
    FocusInvalidNodeId(NodeHierarchyItemId),
    /// CSS path selector did not match any focusable node (includes the path for debugging)
    CouldNotFindFocusNode(String),
}

/// Direction for searching focusable nodes in the DOM tree.
///
/// Used by `search_focusable_node` to traverse nodes either forward
/// (towards higher indices / next DOM) or backward (towards lower indices / previous DOM).
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
enum SearchDirection {
    /// Search forward: increment node index, move to next DOM when at end
    Forward,
    /// Search backward: decrement node index, move to previous DOM when at start
    Backward,
}

impl SearchDirection {
    /// Compute the next node index in this direction.
    ///
    /// Uses saturating arithmetic to avoid overflow/underflow.
    fn step_node(&self, index: usize) -> usize {
        match self {
            Self::Forward => index.saturating_add(1),
            Self::Backward => index.saturating_sub(1),
        }
    }

    /// Advance the DOM ID in this direction (mutates in place).
    fn step_dom(&self, dom_id: &mut DomId) {
        match self {
            Self::Forward => dom_id.inner += 1,
            Self::Backward => dom_id.inner -= 1,
        }
    }

    /// Check if we've hit a node boundary and need to switch DOMs.
    ///
    /// Returns `true` if:
    ///
    /// - Backward: at min node and current < start (wrapped around)
    /// - Forward: at max node and current > start (wrapped around)
    fn is_at_boundary(&self, current: NodeId, start: NodeId, min: NodeId, max: NodeId) -> bool {
        match self {
            Self::Backward => current == min && current < start,
            Self::Forward => current == max && current > start,
        }
    }

    /// Check if we've hit a DOM boundary (first or last DOM in the layout).
    fn is_at_dom_boundary(&self, dom_id: DomId, min: DomId, max: DomId) -> bool {
        match self {
            Self::Backward => dom_id == min,
            Self::Forward => dom_id == max,
        }
    }

    /// Get the starting node ID when entering a new DOM.
    ///
    /// - Forward: start at first node (index 0)
    /// - Backward: start at last node
    fn initial_node_for_next_dom(&self, layout: &DomLayoutResult) -> NodeId {
        match self {
            Self::Forward => NodeId::ZERO,
            Self::Backward => NodeId::new(layout.styled_dom.node_data.len() - 1),
        }
    }
}

/// Context for focusable node search operations.
///
/// Holds shared state and provides helper methods for traversing
/// the DOM tree to find focusable nodes. This avoids passing
/// multiple parameters through the search functions.
struct FocusSearchContext<'a> {
    /// Reference to all DOM layouts in the window
    layout_results: &'a BTreeMap<DomId, DomLayoutResult>,
    /// First DOM ID (always `ROOT_ID`)
    min_dom_id: DomId,
    /// Last DOM ID in the layout results
    max_dom_id: DomId,
}

impl<'a> FocusSearchContext<'a> {
    /// Create a new search context from layout results.
    fn new(layout_results: &'a BTreeMap<DomId, DomLayoutResult>) -> Self {
        Self {
            layout_results,
            min_dom_id: DomId::ROOT_ID,
            max_dom_id: DomId {
                inner: layout_results.len() - 1,
            },
        }
    }

    /// Get the layout for a DOM ID, or return an error if invalid.
    fn get_layout(&self, dom_id: &DomId) -> Result<&'a DomLayoutResult, UpdateFocusWarning> {
        self.layout_results
            .get(dom_id)
            .ok_or_else(|| UpdateFocusWarning::FocusInvalidDomId(dom_id.clone()))
    }

    /// Validate that a node exists in the given layout.
    ///
    /// Returns an error if the node ID is out of bounds or the DOM is empty.
    fn validate_node(
        &self,
        layout: &DomLayoutResult,
        node_id: NodeId,
        dom_id: DomId,
    ) -> Result<(), UpdateFocusWarning> {
        let is_valid = layout
            .styled_dom
            .node_data
            .as_container()
            .get(node_id)
            .is_some();
        if !is_valid {
            return Err(UpdateFocusWarning::FocusInvalidNodeId(
                NodeHierarchyItemId::from_crate_internal(Some(node_id)),
            ));
        }
        if layout.styled_dom.node_data.is_empty() {
            return Err(UpdateFocusWarning::FocusInvalidDomId(dom_id));
        }
        Ok(())
    }

    /// Get the valid node ID range for a layout: `(min, max)`.
    fn node_bounds(&self, layout: &DomLayoutResult) -> (NodeId, NodeId) {
        (
            NodeId::ZERO,
            NodeId::new(layout.styled_dom.node_data.len() - 1),
        )
    }

    /// Check if a node can receive keyboard focus.
    fn is_focusable(&self, layout: &DomLayoutResult, node_id: NodeId) -> bool {
        layout.styled_dom.node_data.as_container()[node_id].is_focusable()
    }

    /// Construct a `DomNodeId` from DOM and node IDs.
    fn make_dom_node_id(&self, dom_id: DomId, node_id: NodeId) -> DomNodeId {
        DomNodeId {
            dom: dom_id,
            node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
        }
    }
}

/// Search for the next focusable node in a given direction.
///
/// Traverses nodes within the current DOM, then moves to adjacent DOMs
/// if no focusable node is found. Returns `Ok(None)` if no focusable
/// node exists in the entire layout in the given direction.
///
/// # Termination guarantee
///
/// The function is guaranteed to terminate because:
///
/// - The inner loop advances `node_id` by 1 each iteration (via `step_node`)
/// - When hitting a node boundary, we either return `None` (at DOM boundary) or move to the next
///   DOM and break to the outer loop
/// - The outer loop only continues when we switch DOMs, which is bounded by the finite number of
///   DOMs in `layout_results`
/// - Each DOM is visited at most once per search direction
///
/// # Returns
///
/// * `Ok(Some(node))` - Found a focusable node
/// * `Ok(None)` - No focusable node exists in the search direction
/// * `Err(_)` - Invalid DOM or node ID encountered
fn search_focusable_node(
    ctx: &FocusSearchContext,
    mut dom_id: DomId,
    mut node_id: NodeId,
    direction: SearchDirection,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    loop {
        let layout = ctx.get_layout(&dom_id)?;
        ctx.validate_node(layout, node_id, dom_id)?;

        let (min_node, max_node) = ctx.node_bounds(layout);

        loop {
            let next_node = NodeId::new(direction.step_node(node_id.index()))
                .max(min_node)
                .min(max_node);

            // If we couldn't make progress (next_node == node_id due to clamping),
            // we've hit the boundary of this DOM
            if next_node == node_id {
                if direction.is_at_dom_boundary(dom_id, ctx.min_dom_id, ctx.max_dom_id) {
                    return Ok(None); // Reached end of all DOMs
                }
                direction.step_dom(&mut dom_id);
                let next_layout = ctx.get_layout(&dom_id)?;
                node_id = direction.initial_node_for_next_dom(next_layout);
                break; // Continue outer loop with new DOM
            }

            // Check for focusable node (we made progress, so this is a different node)
            if ctx.is_focusable(layout, next_node) {
                return Ok(Some(ctx.make_dom_node_id(dom_id, next_node)));
            }

            // Detect if we've hit the boundary (at min/max node)
            let at_boundary = direction.is_at_boundary(next_node, node_id, min_node, max_node);

            if at_boundary {
                if direction.is_at_dom_boundary(dom_id, ctx.min_dom_id, ctx.max_dom_id) {
                    return Ok(None); // Reached end of all DOMs
                }
                direction.step_dom(&mut dom_id);
                let next_layout = ctx.get_layout(&dom_id)?;
                node_id = direction.initial_node_for_next_dom(next_layout);
                break; // Continue outer loop with new DOM
            }

            node_id = next_node;
        }
    }
}

/// Get starting position for Previous focus search
fn get_previous_start(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<(DomId, NodeId), UpdateFocusWarning> {
    let last_dom_id = DomId {
        inner: layout_results.len() - 1,
    };

    let Some(focus) = current_focus else {
        let layout = layout_results
            .get(&last_dom_id)
            .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_dom_id))?;
        return Ok((
            last_dom_id,
            NodeId::new(layout.styled_dom.node_data.len() - 1),
        ));
    };

    let Some(node) = focus.node.into_crate_internal() else {
        if let Some(layout) = layout_results.get(&focus.dom) {
            return Ok((
                focus.dom,
                NodeId::new(layout.styled_dom.node_data.len() - 1),
            ));
        }
        let layout = layout_results
            .get(&last_dom_id)
            .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_dom_id))?;
        return Ok((
            last_dom_id,
            NodeId::new(layout.styled_dom.node_data.len() - 1),
        ));
    };

    Ok((focus.dom, node))
}

/// Get starting position for Next focus search
fn get_next_start(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> (DomId, NodeId) {
    let Some(focus) = current_focus else {
        return (DomId::ROOT_ID, NodeId::ZERO);
    };

    match focus.node.into_crate_internal() {
        Some(node) => (focus.dom, node),
        None if layout_results.contains_key(&focus.dom) => (focus.dom, NodeId::ZERO),
        None => (DomId::ROOT_ID, NodeId::ZERO),
    }
}

/// Get starting position for Last focus search
fn get_last_start(
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
) -> Result<(DomId, NodeId), UpdateFocusWarning> {
    let last_dom_id = DomId {
        inner: layout_results.len() - 1,
    };
    let layout = layout_results
        .get(&last_dom_id)
        .ok_or(UpdateFocusWarning::FocusInvalidDomId(last_dom_id))?;
    Ok((
        last_dom_id,
        NodeId::new(layout.styled_dom.node_data.len() - 1),
    ))
}

/// Find the first focusable node matching a CSS path selector.
///
/// Iterates through all nodes in the DOM in document order (index 0..n),
/// and returns the first node that:
///
/// 1. Matches the CSS path selector
/// 2. Is focusable (has `tabindex` or is naturally focusable)
///
/// # Returns
///
/// * `Ok(Some(node))` - Found a matching focusable node
/// * `Ok(None)` - No matching focusable node exists
/// * `Err(_)` - CSS path could not be matched (malformed selector)
fn find_first_matching_focusable_node(
    layout: &DomLayoutResult,
    dom_id: &DomId,
    css_path: &azul_css::css::CssPath,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    let styled_dom = &layout.styled_dom;
    let node_hierarchy = styled_dom.node_hierarchy.as_container();
    let node_data = styled_dom.node_data.as_container();
    let cascade_info = styled_dom.cascade_info.as_container();

    // Iterate through all nodes in document order
    let matching_node = (0..node_data.len())
        .map(NodeId::new)
        .filter(|&node_id| {
            // Check if node matches the CSS path (no pseudo-selector requirement)
            matches_html_element(
                css_path,
                node_id,
                &node_hierarchy,
                &node_data,
                &cascade_info,
                None, // No expected pseudo-selector ending like :hover/:focus
            )
        })
        .find(|&node_id| {
            // Among matching nodes, find first that is focusable
            node_data[node_id].is_focusable()
        });

    Ok(matching_node.map(|node_id| DomNodeId {
        dom: *dom_id,
        node: NodeHierarchyItemId::from_crate_internal(Some(node_id)),
    }))
}

/// Resolve a FocusTarget to an actual DomNodeId
pub fn resolve_focus_target(
    focus_target: &FocusTarget,
    layout_results: &BTreeMap<DomId, DomLayoutResult>,
    current_focus: Option<DomNodeId>,
) -> Result<Option<DomNodeId>, UpdateFocusWarning> {
    use azul_core::callbacks::FocusTarget::*;

    if layout_results.is_empty() {
        return Ok(None);
    }

    let ctx = FocusSearchContext::new(layout_results);

    match focus_target {
        Path(FocusTargetPath { dom, css_path }) => {
            let layout = ctx.get_layout(dom)?;
            find_first_matching_focusable_node(layout, dom, css_path)
        }

        Id(dom_node_id) => {
            let layout = ctx.get_layout(&dom_node_id.dom)?;
            let is_valid = dom_node_id
                .node
                .into_crate_internal()
                .map(|n| layout.styled_dom.node_data.as_container().get(n).is_some())
                .unwrap_or(false);

            if is_valid {
                Ok(Some(dom_node_id.clone()))
            } else {
                Err(UpdateFocusWarning::FocusInvalidNodeId(
                    dom_node_id.node.clone(),
                ))
            }
        }

        Previous => {
            let (dom_id, node_id) = get_previous_start(layout_results, current_focus)?;
            let result = search_focusable_node(&ctx, dom_id, node_id, SearchDirection::Backward)?;
            // Wrap around: if no previous focusable found, go to last focusable
            if result.is_none() {
                let (last_dom_id, last_node_id) = get_last_start(layout_results)?;
                // First check if the last node itself is focusable
                let last_layout = ctx.get_layout(&last_dom_id)?;
                if ctx.is_focusable(last_layout, last_node_id) {
                    Ok(Some(ctx.make_dom_node_id(last_dom_id, last_node_id)))
                } else {
                    // Otherwise search backward from last node
                    search_focusable_node(&ctx, last_dom_id, last_node_id, SearchDirection::Backward)
                }
            } else {
                Ok(result)
            }
        }

        Next => {
            let (dom_id, node_id) = get_next_start(layout_results, current_focus);
            let result = search_focusable_node(&ctx, dom_id, node_id, SearchDirection::Forward)?;
            // Wrap around: if no next focusable found, go to first focusable
            if result.is_none() {
                // First check if the first node itself is focusable
                let first_layout = ctx.get_layout(&DomId::ROOT_ID)?;
                if ctx.is_focusable(first_layout, NodeId::ZERO) {
                    Ok(Some(ctx.make_dom_node_id(DomId::ROOT_ID, NodeId::ZERO)))
                } else {
                    search_focusable_node(&ctx, DomId::ROOT_ID, NodeId::ZERO, SearchDirection::Forward)
                }
            } else {
                Ok(result)
            }
        }

        First => {
            // First check if the first node itself is focusable
            let first_layout = ctx.get_layout(&DomId::ROOT_ID)?;
            if ctx.is_focusable(first_layout, NodeId::ZERO) {
                Ok(Some(ctx.make_dom_node_id(DomId::ROOT_ID, NodeId::ZERO)))
            } else {
                search_focusable_node(&ctx, DomId::ROOT_ID, NodeId::ZERO, SearchDirection::Forward)
            }
        }

        Last => {
            let (dom_id, node_id) = get_last_start(layout_results)?;
            // First check if the last node itself is focusable
            let last_layout = ctx.get_layout(&dom_id)?;
            if ctx.is_focusable(last_layout, node_id) {
                Ok(Some(ctx.make_dom_node_id(dom_id, node_id)))
            } else {
                search_focusable_node(&ctx, dom_id, node_id, SearchDirection::Backward)
            }
        }

        NoFocus => Ok(None),
    }
}

// Trait Implementations for Event Filtering

impl azul_core::events::FocusManagerQuery for FocusManager {
    fn get_focused_node_id(&self) -> Option<azul_core::dom::DomNodeId> {
        self.focused_node
    }
}

```

## layout/src/managers/selection.rs
// Selection manager
// 632 lines

```rust
//! Text selection state management
//!
//! Manages text selection ranges across all DOMs using the browser-style
//! anchor/focus model for multi-node selection.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;
use core::time::Duration;

use azul_core::{
    dom::{DomId, DomNodeId, NodeId},
    events::SelectionManagerQuery,
    geom::{LogicalPosition, LogicalRect},
    selection::{
        Selection, SelectionAnchor, SelectionFocus, SelectionRange, SelectionState, SelectionVec,
        TextCursor, TextSelection,
    },
};
use azul_css::{impl_option, impl_option_inner, AzString, OptionString};

/// Click state for detecting double/triple clicks
#[derive(Debug, Clone, PartialEq)]
pub struct ClickState {
    /// Last clicked node
    pub last_node: Option<DomNodeId>,
    /// Last click position
    pub last_position: LogicalPosition,
    /// Last click time (as milliseconds since some epoch)
    pub last_time_ms: u64,
    /// Current click count (1=single, 2=double, 3=triple)
    pub click_count: u8,
}

impl Default for ClickState {
    fn default() -> Self {
        Self {
            last_node: None,
            last_position: LogicalPosition { x: 0.0, y: 0.0 },
            last_time_ms: 0,
            click_count: 0,
        }
    }
}

/// Manager for text selections across all DOMs
///
/// This manager supports both the legacy per-node selection model and the new
/// browser-style anchor/focus model for multi-node selection.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionManager {
    /// Legacy selection state for each DOM (per-node model)
    /// Maps DomId -> SelectionState
    /// TODO: Deprecate once multi-node selection is fully implemented
    pub selections: BTreeMap<DomId, SelectionState>,
    
    /// New multi-node selection state using anchor/focus model
    /// Maps DomId -> TextSelection
    pub text_selections: BTreeMap<DomId, TextSelection>,
    
    /// Click state for multi-click detection
    pub click_state: ClickState,
}

impl Default for SelectionManager {
    fn default() -> Self {
        Self::new()
    }
}

impl SelectionManager {
    /// Multi-click timeout in milliseconds
    pub const MULTI_CLICK_TIMEOUT_MS: u64 = 500;
    /// Multi-click maximum distance in pixels
    pub const MULTI_CLICK_DISTANCE_PX: f32 = 5.0;

    /// Create a new selection manager
    pub fn new() -> Self {
        Self {
            selections: BTreeMap::new(),
            text_selections: BTreeMap::new(),
            click_state: ClickState::default(),
        }
    }

    /// Update click count based on position and time
    /// Returns the new click count (1=single, 2=double, 3=triple)
    pub fn update_click_count(
        &mut self,
        node_id: DomNodeId,
        position: LogicalPosition,
        current_time_ms: u64,
    ) -> u8 {
        // Check if this is part of multi-click sequence
        let should_increment = if let Some(last_node) = self.click_state.last_node {
            if last_node != node_id {
                return self.reset_click_count(node_id, position, current_time_ms);
            }

            let time_delta = current_time_ms.saturating_sub(self.click_state.last_time_ms);
            if time_delta >= Self::MULTI_CLICK_TIMEOUT_MS {
                return self.reset_click_count(node_id, position, current_time_ms);
            }

            let dx = position.x - self.click_state.last_position.x;
            let dy = position.y - self.click_state.last_position.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance >= Self::MULTI_CLICK_DISTANCE_PX {
                return self.reset_click_count(node_id, position, current_time_ms);
            }

            true
        } else {
            false
        };

        let click_count = if should_increment {
            // Cycle: 1 -> 2 -> 3 -> 1
            let new_count = self.click_state.click_count + 1;
            if new_count > 3 {
                1
            } else {
                new_count
            }
        } else {
            1
        };

        self.click_state = ClickState {
            last_node: Some(node_id),
            last_position: position,
            last_time_ms: current_time_ms,
            click_count,
        };

        click_count
    }

    /// Reset click count to 1 (new click sequence)
    fn reset_click_count(
        &mut self,
        node_id: DomNodeId,
        position: LogicalPosition,
        current_time_ms: u64,
    ) -> u8 {
        self.click_state = ClickState {
            last_node: Some(node_id),
            last_position: position,
            last_time_ms: current_time_ms,
            click_count: 1,
        };
        1
    }

    /// Get the selection state for a DOM
    pub fn get_selection(&self, dom_id: &DomId) -> Option<&SelectionState> {
        self.selections.get(dom_id)
    }

    /// Get mutable selection state for a DOM
    pub fn get_selection_mut(&mut self, dom_id: &DomId) -> Option<&mut SelectionState> {
        self.selections.get_mut(dom_id)
    }

    /// Set the selection state for a DOM
    pub fn set_selection(&mut self, dom_id: DomId, selection: SelectionState) {
        self.selections.insert(dom_id, selection);
    }

    /// Set a single cursor for a DOM, replacing all existing selections
    pub fn set_cursor(&mut self, dom_id: DomId, node_id: DomNodeId, cursor: TextCursor) {
        let state = SelectionState {
            selections: vec![Selection::Cursor(cursor)].into(),
            node_id,
        };
        self.selections.insert(dom_id, state);
    }

    /// Set a selection range for a DOM, replacing all existing selections
    pub fn set_range(&mut self, dom_id: DomId, node_id: DomNodeId, range: SelectionRange) {
        let state = SelectionState {
            selections: vec![Selection::Range(range)].into(),
            node_id,
        };
        self.selections.insert(dom_id, state);
    }

    /// Add a selection to an existing selection state (for multi-cursor support)
    pub fn add_selection(&mut self, dom_id: DomId, node_id: DomNodeId, selection: Selection) {
        self.selections
            .entry(dom_id)
            .or_insert_with(|| SelectionState {
                selections: SelectionVec::from_const_slice(&[]),
                node_id,
            })
            .add(selection);
    }

    /// Clear the selection for a DOM
    pub fn clear_selection(&mut self, dom_id: &DomId) {
        self.selections.remove(dom_id);
    }

    /// Clear all selections
    pub fn clear_all(&mut self) {
        self.selections.clear();
    }

    /// Get all selections
    pub fn get_all_selections(&self) -> &BTreeMap<DomId, SelectionState> {
        &self.selections
    }

    /// Check if any DOM has an active selection
    pub fn has_any_selection(&self) -> bool {
        !self.selections.is_empty()
    }

    /// Check if a specific DOM has a selection
    pub fn has_selection(&self, dom_id: &DomId) -> bool {
        self.selections.contains_key(dom_id)
    }

    /// Get the primary cursor for a DOM (first cursor in selection list)
    pub fn get_primary_cursor(&self, dom_id: &DomId) -> Option<TextCursor> {
        self.selections
            .get(dom_id)?
            .selections
            .as_slice()
            .first()
            .and_then(|s| match s {
                Selection::Cursor(c) => Some(c.clone()),
                // Primary cursor is at the end of selection
                Selection::Range(r) => Some(r.end.clone()),
            })
    }

    /// Get all selection ranges for a DOM (excludes plain cursors)
    pub fn get_ranges(&self, dom_id: &DomId) -> alloc::vec::Vec<SelectionRange> {
        self.selections
            .get(dom_id)
            .map(|state| {
                state
                    .selections
                    .as_slice()
                    .iter()
                    .filter_map(|s| match s {
                        Selection::Range(r) => Some(r.clone()),
                        Selection::Cursor(_) => None,
                    })
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Analyze a click event and return what type of text selection should be performed
    ///
    /// This is used by the event system to determine if a click should trigger
    /// text selection (single/double/triple click).
    ///
    /// ## Returns
    ///
    /// - `Some(1)` - Single click (place cursor)
    /// - `Some(2)` - Double click (select word)
    /// - `Some(3)` - Triple click (select paragraph/line)
    /// - `None` - Not a text selection click (click count > 3 or timeout/distance exceeded)
    pub fn analyze_click_for_selection(
        &self,
        node_id: DomNodeId,
        position: LogicalPosition,
        current_time_ms: u64,
    ) -> Option<u8> {
        let click_state = &self.click_state;

        // Check if this continues a multi-click sequence
        if let Some(last_node) = click_state.last_node {
            if last_node != node_id {
                return Some(1); // Different node = new single click
            }

            let time_delta = current_time_ms.saturating_sub(click_state.last_time_ms);
            if time_delta >= Self::MULTI_CLICK_TIMEOUT_MS {
                return Some(1); // Timeout = new single click
            }

            let dx = position.x - click_state.last_position.x;
            let dy = position.y - click_state.last_position.y;
            let distance = (dx * dx + dy * dy).sqrt();
            if distance >= Self::MULTI_CLICK_DISTANCE_PX {
                return Some(1); // Too far = new single click
            }
        } else {
            return Some(1); // No previous click = single click
        }

        // Continue multi-click sequence
        let next_count = click_state.click_count + 1;
        if next_count > 3 {
            Some(1) // Cycle back to single click
        } else {
            Some(next_count)
        }
    }
    
    // ========================================================================
    // NEW: Anchor/Focus model for multi-node selection
    // ========================================================================
    
    /// Start a new text selection with an anchor point.
    ///
    /// This is called on MouseDown. It creates a collapsed selection (cursor)
    /// at the anchor position. The focus will be updated during drag.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM this selection belongs to
    /// * `ifc_root_node_id` - The IFC root node where the click occurred
    /// * `cursor` - The cursor position within the IFC's UnifiedLayout
    /// * `char_bounds` - Visual bounds of the clicked character
    /// * `mouse_position` - Mouse position in viewport coordinates
    pub fn start_selection(
        &mut self,
        dom_id: DomId,
        ifc_root_node_id: NodeId,
        cursor: TextCursor,
        char_bounds: LogicalRect,
        mouse_position: LogicalPosition,
    ) {
        let selection = TextSelection::new_collapsed(
            dom_id,
            ifc_root_node_id,
            cursor,
            char_bounds,
            mouse_position,
        );
        self.text_selections.insert(dom_id, selection);
    }
    
    /// Update the focus point of an ongoing selection.
    ///
    /// This is called during MouseMove/Drag. It updates the focus position
    /// and recomputes the affected nodes between anchor and focus.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM this selection belongs to
    /// * `ifc_root_node_id` - The IFC root node where the focus is now
    /// * `cursor` - The cursor position within the IFC's UnifiedLayout
    /// * `mouse_position` - Current mouse position in viewport coordinates
    /// * `affected_nodes` - Pre-computed map of affected IFC roots to their SelectionRanges
    /// * `is_forward` - Whether anchor comes before focus in document order
    ///
    /// ## Returns
    /// * `true` if the selection was updated
    /// * `false` if no selection exists for this DOM
    pub fn update_selection_focus(
        &mut self,
        dom_id: &DomId,
        ifc_root_node_id: NodeId,
        cursor: TextCursor,
        mouse_position: LogicalPosition,
        affected_nodes: BTreeMap<NodeId, SelectionRange>,
        is_forward: bool,
    ) -> bool {
        if let Some(selection) = self.text_selections.get_mut(dom_id) {
            selection.focus = SelectionFocus {
                ifc_root_node_id,
                cursor,
                mouse_position,
            };
            selection.affected_nodes = affected_nodes;
            selection.is_forward = is_forward;
            true
        } else {
            false
        }
    }
    
    /// Get the current text selection for a DOM.
    pub fn get_text_selection(&self, dom_id: &DomId) -> Option<&TextSelection> {
        self.text_selections.get(dom_id)
    }
    
    /// Get mutable reference to the current text selection for a DOM.
    pub fn get_text_selection_mut(&mut self, dom_id: &DomId) -> Option<&mut TextSelection> {
        self.text_selections.get_mut(dom_id)
    }
    
    /// Check if a DOM has an active text selection (new model).
    pub fn has_text_selection(&self, dom_id: &DomId) -> bool {
        self.text_selections.contains_key(dom_id)
    }
    
    /// Get the selection range for a specific IFC root node.
    ///
    /// This is used by the renderer to quickly look up if a node is selected
    /// and get its selection range for `get_selection_rects()`.
    ///
    /// ## Parameters
    /// * `dom_id` - The DOM to check
    /// * `ifc_root_node_id` - The IFC root node to look up
    ///
    /// ## Returns
    /// * `Some(&SelectionRange)` if this node is part of the selection
    /// * `None` if not selected
    pub fn get_range_for_ifc_root(
        &self,
        dom_id: &DomId,
        ifc_root_node_id: &NodeId,
    ) -> Option<&SelectionRange> {
        self.text_selections
            .get(dom_id)?
            .get_range_for_node(ifc_root_node_id)
    }
    
    /// Clear the text selection for a DOM (new model).
    pub fn clear_text_selection(&mut self, dom_id: &DomId) {
        self.text_selections.remove(dom_id);
    }
    
    /// Clear all text selections (new model).
    pub fn clear_all_text_selections(&mut self) {
        self.text_selections.clear();
    }
    
    /// Get all text selections.
    pub fn get_all_text_selections(&self) -> &BTreeMap<DomId, TextSelection> {
        &self.text_selections
    }
}

// Clipboard Content Extraction

/// Styled text run for rich clipboard content
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct StyledTextRun {
    /// The actual text content
    pub text: AzString,
    /// Font family name
    pub font_family: OptionString,
    /// Font size in pixels
    pub font_size_px: f32,
    /// Text color
    pub color: azul_css::props::basic::ColorU,
    /// Whether text is bold
    pub is_bold: bool,
    /// Whether text is italic
    pub is_italic: bool,
}

azul_css::impl_vec!(
    StyledTextRun,
    StyledTextRunVec,
    StyledTextRunVecDestructor,
    StyledTextRunVecDestructorType
);
azul_css::impl_vec_debug!(StyledTextRun, StyledTextRunVec);
azul_css::impl_vec_clone!(StyledTextRun, StyledTextRunVec, StyledTextRunVecDestructor);
azul_css::impl_vec_partialeq!(StyledTextRun, StyledTextRunVec);

/// Clipboard content with both plain text and styled (HTML) representation
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct ClipboardContent {
    /// Plain text representation (UTF-8)
    pub plain_text: AzString,
    /// Rich text runs with styling information
    pub styled_runs: StyledTextRunVec,
}

impl_option!(
    ClipboardContent,
    OptionClipboardContent,
    copy = false,
    [Debug, Clone, PartialEq]
);

impl ClipboardContent {
    /// Convert styled runs to HTML for rich clipboard formats
    pub fn to_html(&self) -> String {
        let mut html = String::from("<div>");

        for run in self.styled_runs.as_slice() {
            html.push_str("<span style=\"");

            if let Some(font_family) = run.font_family.as_ref() {
                html.push_str(&format!("font-family: {}; ", font_family.as_str()));
            }
            html.push_str(&format!("font-size: {}px; ", run.font_size_px));
            html.push_str(&format!(
                "color: rgba({}, {}, {}, {}); ",
                run.color.r,
                run.color.g,
                run.color.b,
                run.color.a as f32 / 255.0
            ));
            if run.is_bold {
                html.push_str("font-weight: bold; ");
            }
            if run.is_italic {
                html.push_str("font-style: italic; ");
            }

            html.push_str("\">");
            // Escape HTML entities
            let escaped = run
                .text
                .as_str()
                .replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;");
            html.push_str(&escaped);
            html.push_str("</span>");
        }

        html.push_str("</div>");
        html
    }
}

// Trait Implementations for Event Filtering

impl SelectionManagerQuery for SelectionManager {
    fn get_click_count(&self) -> u8 {
        self.click_state.click_count
    }

    fn get_drag_start_position(&self) -> Option<LogicalPosition> {
        // If left mouse button is down and we have a last click position,
        // that's our drag start position
        if self.click_state.click_count > 0 {
            Some(self.click_state.last_position)
        } else {
            None
        }
    }

    fn has_selection(&self) -> bool {
        // Check if any selection exists via:
        //
        // 1. Click count > 0 (single/double/triple click created selection)
        // 2. Drag start position exists (drag selection in progress)
        // 3. Any DOM has non-empty selection state

        if self.click_state.click_count > 0 {
            return true;
        }

        // Check if any DOM has an active selection
        for (_dom_id, selection_state) in &self.selections {
            if !selection_state.selections.is_empty() {
                return true;
            }
        }

        false
    }
}

impl SelectionManager {
    /// Remap NodeIds after DOM reconciliation
    ///
    /// When the DOM is regenerated, NodeIds can change. This method updates all
    /// internal state to use the new NodeIds based on the provided mapping.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &std::collections::BTreeMap<azul_core::dom::NodeId, azul_core::dom::NodeId>,
    ) {
        use azul_core::styled_dom::NodeHierarchyItemId;
        
        // Update legacy selection state
        if let Some(selection_state) = self.selections.get_mut(&dom_id) {
            if let Some(old_node_id) = selection_state.node_id.node.into_crate_internal() {
                if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                    selection_state.node_id.node = NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
                } else {
                    // Node was removed, clear selection for this DOM
                    self.selections.remove(&dom_id);
                    return;
                }
            }
        }
        
        // Update text_selections (new multi-node model)
        if let Some(text_selection) = self.text_selections.get_mut(&dom_id) {
            // Update anchor ifc_root_node_id
            let old_anchor_id = text_selection.anchor.ifc_root_node_id;
            if let Some(&new_node_id) = node_id_map.get(&old_anchor_id) {
                text_selection.anchor.ifc_root_node_id = new_node_id;
            } else {
                // Anchor node removed, clear selection
                self.text_selections.remove(&dom_id);
                return;
            }
            
            // Update focus ifc_root_node_id
            let old_focus_id = text_selection.focus.ifc_root_node_id;
            if let Some(&new_node_id) = node_id_map.get(&old_focus_id) {
                text_selection.focus.ifc_root_node_id = new_node_id;
            } else {
                // Focus node removed, clear selection
                self.text_selections.remove(&dom_id);
                return;
            }
            
            // Update affected_nodes map with remapped NodeIds
            let old_affected: Vec<_> = text_selection.affected_nodes.keys().cloned().collect();
            let mut new_affected = std::collections::BTreeMap::new();
            for old_node_id in old_affected {
                if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                    if let Some(range) = text_selection.affected_nodes.remove(&old_node_id) {
                        new_affected.insert(new_node_id, range);
                    }
                }
            }
            text_selection.affected_nodes = new_affected;
        }
        
        // Update click_state last_node if it's in the affected DOM
        if let Some(last_node) = &mut self.click_state.last_node {
            if last_node.dom == dom_id {
                if let Some(old_node_id) = last_node.node.into_crate_internal() {
                    if let Some(&new_node_id) = node_id_map.get(&old_node_id) {
                        last_node.node = NodeHierarchyItemId::from_crate_internal(Some(new_node_id));
                    } else {
                        // Node removed, reset click state
                        self.click_state = ClickState::default();
                    }
                }
            }
        }
    }
}

```

## layout/src/managers/cursor.rs
// Cursor blink and position
// 352 lines

```rust
//! Text cursor management
//!
//! Manages text cursor position and state for contenteditable elements.
//!
//! # Cursor Lifecycle
//!
//! The cursor is automatically managed in response to focus changes:
//!
//! 1. **Focus lands on contenteditable node**: Cursor initialized at end of text
//! 2. **Focus moves to non-editable node**: Cursor automatically cleared
//! 3. **Focus clears entirely**: Cursor automatically cleared
//!
//! ## Automatic Cursor Initialization
//!
//! When focus is set to a contenteditable node via `FocusManager::set_focused_node()`,
//! the event system (in `window.rs`) checks if the node is contenteditable and calls
//! `CursorManager::initialize_cursor_at_end()` to place the cursor at the end of the text.
//!
//! This happens for:
//!
//! - User clicks on contenteditable element
//! - Tab navigation to contenteditable element
//! - Programmatic focus via `AccessibilityAction::Focus`
//! - Focus from screen reader commands
//!
//! ## Cursor Blinking
//!
//! The cursor blinks at ~530ms intervals when a contenteditable element has focus.
//! Blinking is managed by a system timer (`CURSOR_BLINK_TIMER_ID`) that:
//!
//! - Starts when focus lands on a contenteditable element
//! - Stops when focus moves away
//! - Resets (cursor becomes visible) on any user input (keyboard, mouse)
//! - After ~530ms of no input, the cursor toggles visibility
//!
//! ## Integration with Text Layout
//!
//! The cursor manager uses the `TextLayoutCache` to determine:
//!
//! - Total number of grapheme clusters in the text
//! - Position of the last grapheme cluster (for cursor-at-end)
//! - Bounding rectangles for scroll-into-view
//!
//! ## Scroll-Into-View
//!
//! When a cursor is set, the system automatically checks if it's visible in the
//! viewport. If not, it uses the `ScrollManager` to scroll the minimum amount
//! needed to bring the cursor into view.
//!
//! ## Multi-Cursor Support
//!
//! While the core `TextCursor` type supports multi-cursor editing (used in
//! `text3::edit`), the `CursorManager` currently manages a single cursor for
//! accessibility and user interaction. Multi-cursor scenarios are handled at
//! the `SelectionManager` level with multiple `Selection::Cursor` items.

use azul_core::{
    dom::{DomId, NodeId},
    selection::{CursorAffinity, GraphemeClusterId, TextCursor},
    task::Instant,
};

/// Default cursor blink interval in milliseconds
pub const CURSOR_BLINK_INTERVAL_MS: u64 = 530;

/// Manager for text cursor position and rendering
#[derive(Debug, Clone)]
pub struct CursorManager {
    /// Current cursor position (if any)
    pub cursor: Option<TextCursor>,
    /// DOM and node where the cursor is located
    pub cursor_location: Option<CursorLocation>,
    /// Whether the cursor is currently visible (toggled by blink timer)
    pub is_visible: bool,
    /// Timestamp of the last user input event (keyboard, mouse click in text)
    /// Used to determine whether to blink or stay solid while typing
    pub last_input_time: Option<Instant>,
    /// Whether the cursor blink timer is currently active
    pub blink_timer_active: bool,
}

/// Location of a cursor within the DOM
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorLocation {
    pub dom_id: DomId,
    pub node_id: NodeId,
}

impl PartialEq for CursorManager {
    fn eq(&self, other: &Self) -> bool {
        // Ignore is_visible and last_input_time for equality - they're transient state
        self.cursor == other.cursor && self.cursor_location == other.cursor_location
    }
}

impl Default for CursorManager {
    fn default() -> Self {
        Self::new()
    }
}

impl CursorManager {
    /// Create a new cursor manager with no cursor
    pub fn new() -> Self {
        Self {
            cursor: None,
            cursor_location: None,
            is_visible: false,
            last_input_time: None,
            blink_timer_active: false,
        }
    }

    /// Get the current cursor position
    pub fn get_cursor(&self) -> Option<&TextCursor> {
        self.cursor.as_ref()
    }

    /// Get the current cursor location
    pub fn get_cursor_location(&self) -> Option<&CursorLocation> {
        self.cursor_location.as_ref()
    }

    /// Set the cursor position manually
    ///
    /// This is used for programmatic cursor positioning. For automatic
    /// initialization when focusing a contenteditable element, use
    /// `initialize_cursor_at_end()`.
    pub fn set_cursor(&mut self, cursor: Option<TextCursor>, location: Option<CursorLocation>) {
        self.cursor = cursor;
        self.cursor_location = location;
        // Make cursor visible when set
        if cursor.is_some() {
            self.is_visible = true;
        }
    }
    
    /// Set the cursor position with timestamp for blink reset
    pub fn set_cursor_with_time(&mut self, cursor: Option<TextCursor>, location: Option<CursorLocation>, now: Instant) {
        self.cursor = cursor;
        self.cursor_location = location;
        if cursor.is_some() {
            self.is_visible = true;
            self.last_input_time = Some(now);
        }
    }

    /// Clear the cursor
    ///
    /// This is automatically called when focus moves to a non-editable node
    /// or when focus is cleared entirely.
    pub fn clear(&mut self) {
        self.cursor = None;
        self.cursor_location = None;
        self.is_visible = false;
        self.last_input_time = None;
        self.blink_timer_active = false;
    }

    /// Check if there is an active cursor
    pub fn has_cursor(&self) -> bool {
        self.cursor.is_some()
    }
    
    /// Check if the cursor should be drawn (has cursor AND is visible)
    pub fn should_draw_cursor(&self) -> bool {
        self.cursor.is_some() && self.is_visible
    }
    
    /// Reset the blink state on user input
    ///
    /// This makes the cursor visible and records the input time.
    /// The blink timer will keep the cursor visible until `CURSOR_BLINK_INTERVAL_MS`
    /// has passed since this time.
    pub fn reset_blink_on_input(&mut self, now: Instant) {
        self.is_visible = true;
        self.last_input_time = Some(now);
    }
    
    /// Toggle cursor visibility (called by blink timer)
    ///
    /// Returns the new visibility state.
    pub fn toggle_visibility(&mut self) -> bool {
        self.is_visible = !self.is_visible;
        self.is_visible
    }
    
    /// Set cursor visibility directly
    pub fn set_visibility(&mut self, visible: bool) {
        self.is_visible = visible;
    }
    
    /// Check if enough time has passed since last input to start blinking
    ///
    /// Returns true if the cursor should blink (toggle visibility),
    /// false if it should stay solid (user is actively typing).
    pub fn should_blink(&self, now: &Instant) -> bool {
        use azul_core::task::{Duration, SystemTimeDiff};
        
        match &self.last_input_time {
            Some(last_input) => {
                let elapsed = now.duration_since(last_input);
                let blink_interval = Duration::System(SystemTimeDiff::from_millis(CURSOR_BLINK_INTERVAL_MS));
                // If elapsed time is greater than blink interval, allow blinking
                elapsed.greater_than(&blink_interval)
            }
            None => true, // No input recorded, allow blinking
        }
    }
    
    /// Mark the blink timer as active
    pub fn set_blink_timer_active(&mut self, active: bool) {
        self.blink_timer_active = active;
    }
    
    /// Check if the blink timer is active
    pub fn is_blink_timer_active(&self) -> bool {
        self.blink_timer_active
    }

    /// Initialize cursor at the end of the text in the given node
    ///
    /// This is called automatically when focus lands on a contenteditable element.
    /// It queries the text layout to find the position of the last grapheme
    /// cluster and places the cursor there.
    ///
    /// # Returns
    ///
    /// `true` if cursor was successfully initialized, `false` if the node has no text
    /// or text layout is not available.
    pub fn initialize_cursor_at_end(
        &mut self,
        dom_id: DomId,
        node_id: NodeId,
        text_layout: Option<&alloc::sync::Arc<crate::text3::cache::UnifiedLayout>>,
    ) -> bool {
        // Get the text layout for this node
        let Some(layout) = text_layout else {
            // No text layout - set cursor at start
            self.cursor = Some(TextCursor {
                cluster_id: GraphemeClusterId {
                    source_run: 0,
                    start_byte_in_run: 0,
                },
                affinity: CursorAffinity::Trailing,
            });
            self.cursor_location = Some(CursorLocation { dom_id, node_id });
            self.is_visible = true; // Make cursor visible immediately
            return true;
        };

        // Find the last grapheme cluster in items
        let mut last_cluster_id: Option<GraphemeClusterId> = None;

        // Iterate through all items to find the last cluster
        for item in layout.items.iter().rev() {
            if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                last_cluster_id = Some(cluster.source_cluster_id);
                break;
            }
        }

        // Set cursor at the end of the text
        self.cursor = Some(TextCursor {
            cluster_id: last_cluster_id.unwrap_or(GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            }),
            affinity: CursorAffinity::Trailing,
        });

        self.cursor_location = Some(CursorLocation { dom_id, node_id });
        self.is_visible = true; // Make cursor visible immediately

        true
    }

    /// Initialize cursor at the start of the text in the given node
    ///
    /// This can be used for specific navigation scenarios (e.g., Ctrl+Home).
    pub fn initialize_cursor_at_start(&mut self, dom_id: DomId, node_id: NodeId) {
        self.cursor = Some(TextCursor {
            cluster_id: GraphemeClusterId {
                source_run: 0,
                start_byte_in_run: 0,
            },
            affinity: CursorAffinity::Trailing,
        });

        self.cursor_location = Some(CursorLocation { dom_id, node_id });
    }

    /// Move the cursor to a specific position
    ///
    /// This is used by text editing operations and keyboard navigation.
    pub fn move_cursor_to(&mut self, cursor: TextCursor, dom_id: DomId, node_id: NodeId) {
        self.cursor = Some(cursor);
        self.cursor_location = Some(CursorLocation { dom_id, node_id });
    }

    /// Check if the cursor is in a specific node
    pub fn is_cursor_in_node(&self, dom_id: DomId, node_id: NodeId) -> bool {
        self.cursor_location
            .as_ref()
            .map(|loc| loc.dom_id == dom_id && loc.node_id == node_id)
            .unwrap_or(false)
    }
    
    /// Get the DomNodeId where the cursor is located (for cross-frame tracking)
    pub fn get_cursor_node(&self) -> Option<azul_core::dom::DomNodeId> {
        self.cursor_location.as_ref().map(|loc| {
            azul_core::dom::DomNodeId {
                dom: loc.dom_id,
                node: azul_core::styled_dom::NodeHierarchyItemId::from_crate_internal(Some(loc.node_id)),
            }
        })
    }
    
    /// Update the NodeId for the cursor location (after DOM reconciliation)
    ///
    /// This is called when the DOM is regenerated and NodeIds change.
    /// The cursor position within the text is preserved.
    pub fn update_node_id(&mut self, new_node: azul_core::dom::DomNodeId) {
        if let Some(ref mut loc) = self.cursor_location {
            if let Some(new_id) = new_node.node.into_crate_internal() {
                loc.dom_id = new_node.dom;
                loc.node_id = new_id;
            }
        }
    }
    
    /// Remap NodeIds after DOM reconciliation
    ///
    /// When the DOM is regenerated, NodeIds can change. This method updates
    /// the cursor location to use the new NodeId based on the provided mapping.
    pub fn remap_node_ids(
        &mut self,
        dom_id: DomId,
        node_id_map: &std::collections::BTreeMap<NodeId, NodeId>,
    ) {
        if let Some(ref mut loc) = self.cursor_location {
            if loc.dom_id == dom_id {
                if let Some(&new_node_id) = node_id_map.get(&loc.node_id) {
                    loc.node_id = new_node_id;
                } else {
                    // Node was removed, clear cursor location
                    self.cursor_location = None;
                }
            }
        }
    }
}

```

## layout/src/text3/cache.rs
// Text layout cache - hittest_cursor, selection rects
// 7248 lines

```rust
use std::{
    any::{Any, TypeId},
    cmp::Ordering,
    collections::{
        hash_map::{DefaultHasher, Entry, HashMap},
        BTreeSet,
    },
    hash::{Hash, Hasher},
    mem::discriminant,
    num::NonZeroUsize,
    sync::{Arc, Mutex},
};

pub use azul_core::selection::{ContentIndex, GraphemeClusterId};
use azul_core::{
    dom::NodeId,
    geom::{LogicalPosition, LogicalRect, LogicalSize},
    resources::ImageRef,
    selection::{CursorAffinity, SelectionRange, TextCursor},
    ui_solver::GlyphInstance,
};
use azul_css::{
    corety::LayoutDebugMessage, props::basic::ColorU, props::style::StyleBackgroundContent,
};
#[cfg(feature = "text_layout_hyphenation")]
use hyphenation::{Hyphenator, Language as HyphenationLanguage, Load, Standard};
use rust_fontconfig::{FcFontCache, FcPattern, FcWeight, FontId, PatternMatch, UnicodeRange};
use unicode_bidi::{BidiInfo, Level, TextSource};
use unicode_segmentation::UnicodeSegmentation;

// Stub type when hyphenation is disabled
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct Standard;

#[cfg(not(feature = "text_layout_hyphenation"))]
impl Standard {
    /// Stub hyphenate method that returns no breaks
    pub fn hyphenate<'a>(&'a self, _word: &'a str) -> StubHyphenationBreaks {
        StubHyphenationBreaks { breaks: Vec::new() }
    }
}

/// Result of hyphenation (stub when feature is disabled)
#[cfg(not(feature = "text_layout_hyphenation"))]
pub struct StubHyphenationBreaks {
    pub breaks: alloc::vec::Vec<usize>,
}

// Always import Language from script module
use crate::text3::script::{script_to_language, Language, Script};

/// Available space for layout, similar to Taffy's AvailableSpace.
///
/// This type explicitly represents the three possible states for available space:
///
/// - `Definite(f32)`: A specific pixel width is available
/// - `MinContent`: Layout should use minimum content width (shrink-wrap)
/// - `MaxContent`: Layout should use maximum content width (no line breaks unless necessary)
///
/// This is critical for proper handling of intrinsic sizing in Flexbox/Grid
/// where the available space may be indefinite during the measure phase.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AvailableSpace {
    /// A specific amount of space is available (in pixels)
    Definite(f32),
    /// The node should be laid out under a min-content constraint
    MinContent,
    /// The node should be laid out under a max-content constraint  
    MaxContent,
}

impl Default for AvailableSpace {
    fn default() -> Self {
        AvailableSpace::Definite(0.0)
    }
}

impl AvailableSpace {
    /// Returns true if this is a definite (finite, known) amount of space
    pub fn is_definite(&self) -> bool {
        matches!(self, AvailableSpace::Definite(_))
    }

    /// Returns true if this is an indefinite (min-content or max-content) constraint
    pub fn is_indefinite(&self) -> bool {
        !self.is_definite()
    }

    /// Returns the definite value if available, or a fallback for indefinite constraints
    pub fn unwrap_or(self, fallback: f32) -> f32 {
        match self {
            AvailableSpace::Definite(v) => v,
            _ => fallback,
        }
    }

    /// Returns the definite value, or 0.0 for min-content, or f32::MAX for max-content
    pub fn to_f32_for_layout(self) -> f32 {
        match self {
            AvailableSpace::Definite(v) => v,
            AvailableSpace::MinContent => 0.0,
            AvailableSpace::MaxContent => f32::MAX,
        }
    }

    /// Create from an f32 value, recognizing special sentinel values.
    ///
    /// This function provides backwards compatibility with code that uses f32 for constraints:
    /// - `f32::INFINITY` or `f32::MAX` → `MaxContent` (no line wrapping)
    /// - `0.0` → `MinContent` (maximum line wrapping, return longest word width)
    /// - Other values → `Definite(value)`
    ///
    /// Note: Using sentinel values like 0.0 for MinContent is fragile. Prefer using
    /// `AvailableSpace::MinContent` directly when possible.
    pub fn from_f32(value: f32) -> Self {
        if value.is_infinite() || value >= f32::MAX / 2.0 {
            // Treat very large values (including f32::MAX) as MaxContent
            AvailableSpace::MaxContent
        } else if value <= 0.0 {
            // Treat zero or negative as MinContent (shrink-wrap)
            AvailableSpace::MinContent
        } else {
            AvailableSpace::Definite(value)
        }
    }
}

impl Hash for AvailableSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        if let AvailableSpace::Definite(v) = self {
            (v.round() as usize).hash(state);
        }
    }
}

// Re-export traits for backwards compatibility
pub use crate::font_traits::{ParsedFontTrait, ShallowClone};

// --- Core Data Structures for the New Architecture ---

/// Key for caching font chains - based only on CSS properties, not text content
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FontChainKey {
    pub font_families: Vec<String>,
    pub weight: FcWeight,
    pub italic: bool,
    pub oblique: bool,
}

/// Either a FontChainKey (resolved via fontconfig) or a direct FontRef hash.
/// 
/// This enum cleanly separates:
/// - `Chain`: Fonts resolved through fontconfig with fallback support
/// - `Ref`: Direct FontRef that bypasses fontconfig entirely (e.g., embedded icon fonts)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FontChainKeyOrRef {
    /// Regular font chain resolved via fontconfig
    Chain(FontChainKey),
    /// Direct FontRef identified by pointer address (covers entire Unicode range, no fallbacks)
    Ref(usize),
}

impl FontChainKeyOrRef {
    /// Create from a FontStack enum
    pub fn from_font_stack(font_stack: &FontStack) -> Self {
        match font_stack {
            FontStack::Stack(selectors) => FontChainKeyOrRef::Chain(FontChainKey::from_selectors(selectors)),
            FontStack::Ref(font_ref) => FontChainKeyOrRef::Ref(font_ref.parsed as usize),
        }
    }
    
    /// Returns true if this is a direct FontRef
    pub fn is_ref(&self) -> bool {
        matches!(self, FontChainKeyOrRef::Ref(_))
    }
    
    /// Returns the FontRef pointer if this is a Ref variant
    pub fn as_ref_ptr(&self) -> Option<usize> {
        match self {
            FontChainKeyOrRef::Ref(ptr) => Some(*ptr),
            _ => None,
        }
    }
    
    /// Returns the FontChainKey if this is a Chain variant
    pub fn as_chain(&self) -> Option<&FontChainKey> {
        match self {
            FontChainKeyOrRef::Chain(key) => Some(key),
            _ => None,
        }
    }
}

impl FontChainKey {
    /// Create a FontChainKey from a slice of font selectors
    pub fn from_selectors(font_stack: &[FontSelector]) -> Self {
        let font_families: Vec<String> = font_stack
            .iter()
            .map(|s| s.family.clone())
            .filter(|f| !f.is_empty())
            .collect();

        let font_families = if font_families.is_empty() {
            vec!["serif".to_string()]
        } else {
            font_families
        };

        let weight = font_stack
            .first()
            .map(|s| s.weight)
            .unwrap_or(FcWeight::Normal);
        let is_italic = font_stack
            .first()
            .map(|s| s.style == FontStyle::Italic)
            .unwrap_or(false);
        let is_oblique = font_stack
            .first()
            .map(|s| s.style == FontStyle::Oblique)
            .unwrap_or(false);

        FontChainKey {
            font_families,
            weight,
            italic: is_italic,
            oblique: is_oblique,
        }
    }
}

/// A map of pre-loaded fonts, keyed by FontId (from rust-fontconfig)
///
/// This is passed to the shaper - no font loading happens during shaping
/// The fonts are loaded BEFORE layout based on the font chains and text content.
///
/// Provides both FontId and hash-based lookup for efficient glyph operations.
#[derive(Debug, Clone)]
pub struct LoadedFonts<T> {
    /// Primary storage: FontId -> Font
    pub fonts: HashMap<FontId, T>,
    /// Reverse index: font_hash -> FontId for fast hash-based lookups
    hash_to_id: HashMap<u64, FontId>,
}

impl<T: ParsedFontTrait> LoadedFonts<T> {
    pub fn new() -> Self {
        Self {
            fonts: HashMap::new(),
            hash_to_id: HashMap::new(),
        }
    }

    /// Insert a font with its FontId
    pub fn insert(&mut self, font_id: FontId, font: T) {
        let hash = font.get_hash();
        self.hash_to_id.insert(hash, font_id.clone());
        self.fonts.insert(font_id, font);
    }

    /// Get a font by FontId
    pub fn get(&self, font_id: &FontId) -> Option<&T> {
        self.fonts.get(font_id)
    }

    /// Get a font by its hash
    pub fn get_by_hash(&self, hash: u64) -> Option<&T> {
        self.hash_to_id.get(&hash).and_then(|id| self.fonts.get(id))
    }

    /// Get the FontId for a hash
    pub fn get_font_id_by_hash(&self, hash: u64) -> Option<&FontId> {
        self.hash_to_id.get(&hash)
    }

    /// Check if a FontId is present
    pub fn contains_key(&self, font_id: &FontId) -> bool {
        self.fonts.contains_key(font_id)
    }

    /// Check if a hash is present
    pub fn contains_hash(&self, hash: u64) -> bool {
        self.hash_to_id.contains_key(&hash)
    }

    /// Iterate over all fonts
    pub fn iter(&self) -> impl Iterator<Item = (&FontId, &T)> {
        self.fonts.iter()
    }

    /// Get the number of loaded fonts
    pub fn len(&self) -> usize {
        self.fonts.len()
    }

    /// Check if empty
    pub fn is_empty(&self) -> bool {
        self.fonts.is_empty()
    }
}

impl<T: ParsedFontTrait> Default for LoadedFonts<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: ParsedFontTrait> FromIterator<(FontId, T)> for LoadedFonts<T> {
    fn from_iter<I: IntoIterator<Item = (FontId, T)>>(iter: I) -> Self {
        let mut loaded = LoadedFonts::new();
        for (id, font) in iter {
            loaded.insert(id, font);
        }
        loaded
    }
}

/// Enum that wraps either a fontconfig-resolved font (T) or a direct FontRef.
///
/// This allows the shaping code to handle both fontconfig-resolved fonts
/// and embedded fonts (FontRef) uniformly through the ParsedFontTrait interface.
#[derive(Debug, Clone)]
pub enum FontOrRef<T> {
    /// A font loaded via fontconfig
    Font(T),
    /// A direct FontRef (embedded font, bypasses fontconfig)
    Ref(azul_css::props::basic::FontRef),
}

impl<T: ParsedFontTrait> ShallowClone for FontOrRef<T> {
    fn shallow_clone(&self) -> Self {
        match self {
            FontOrRef::Font(f) => FontOrRef::Font(f.shallow_clone()),
            FontOrRef::Ref(r) => FontOrRef::Ref(r.clone()),
        }
    }
}

impl<T: ParsedFontTrait> ParsedFontTrait for FontOrRef<T> {
    fn shape_text(
        &self,
        text: &str,
        script: Script,
        language: Language,
        direction: BidiDirection,
        style: &StyleProperties,
    ) -> Result<Vec<Glyph>, LayoutError> {
        match self {
            FontOrRef::Font(f) => f.shape_text(text, script, language, direction, style),
            FontOrRef::Ref(r) => r.shape_text(text, script, language, direction, style),
        }
    }

    fn get_hash(&self) -> u64 {
        match self {
            FontOrRef::Font(f) => f.get_hash(),
            FontOrRef::Ref(r) => r.get_hash(),
        }
    }

    fn get_glyph_size(&self, glyph_id: u16, font_size: f32) -> Option<LogicalSize> {
        match self {
            FontOrRef::Font(f) => f.get_glyph_size(glyph_id, font_size),
            FontOrRef::Ref(r) => r.get_glyph_size(glyph_id, font_size),
        }
    }

    fn get_hyphen_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            FontOrRef::Font(f) => f.get_hyphen_glyph_and_advance(font_size),
            FontOrRef::Ref(r) => r.get_hyphen_glyph_and_advance(font_size),
        }
    }

    fn get_kashida_glyph_and_advance(&self, font_size: f32) -> Option<(u16, f32)> {
        match self {
            FontOrRef::Font(f) => f.get_kashida_glyph_and_advance(font_size),
            FontOrRef::Ref(r) => r.get_kashida_glyph_and_advance(font_size),
        }
    }

    fn has_glyph(&self, codepoint: u32) -> bool {
        match self {
            FontOrRef::Font(f) => f.has_glyph(codepoint),
            FontOrRef::Ref(r) => r.has_glyph(codepoint),
        }
    }

    fn get_vertical_metrics(&self, glyph_id: u16) -> Option<VerticalMetrics> {
        match self {
            FontOrRef::Font(f) => f.get_vertical_metrics(glyph_id),
            FontOrRef::Ref(r) => r.get_vertical_metrics(glyph_id),
        }
    }

    fn get_font_metrics(&self) -> LayoutFontMetrics {
        match self {
            FontOrRef::Font(f) => f.get_font_metrics(),
            FontOrRef::Ref(r) => r.get_font_metrics(),
        }
    }

    fn num_glyphs(&self) -> u16 {
        match self {
            FontOrRef::Font(f) => f.num_glyphs(),
            FontOrRef::Ref(r) => r.num_glyphs(),
        }
    }
}

#[derive(Debug)]
pub struct FontManager<T> {
    ///  Cache that holds the **file paths** of the fonts (not any font data itself)
    pub fc_cache: Arc<FcFontCache>,
    /// Holds the actual parsed font (usually with the font bytes attached)
    pub parsed_fonts: Mutex<HashMap<FontId, T>>,
    // Cache for font chains - populated by resolve_all_font_chains() before layout
    // This is read-only during layout - no locking needed for reads
    pub font_chain_cache: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    /// Cache for direct FontRefs (embedded fonts like Material Icons)
    /// These are fonts referenced via FontStack::Ref that bypass fontconfig
    pub embedded_fonts: Mutex<HashMap<u64, azul_css::props::basic::FontRef>>,
}

impl<T: ParsedFontTrait> FontManager<T> {
    pub fn new(fc_cache: FcFontCache) -> Result<Self, LayoutError> {
        Ok(Self {
            fc_cache: Arc::new(fc_cache),
            parsed_fonts: Mutex::new(HashMap::new()),
            font_chain_cache: HashMap::new(), // Populated via set_font_chain_cache()
            embedded_fonts: Mutex::new(HashMap::new()),
        })
    }

    /// Set the font chain cache from externally resolved chains
    ///
    /// This should be called with the result of `resolve_font_chains()` or
    /// `collect_and_resolve_font_chains()` from `solver3::getters`.
    pub fn set_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache = chains;
    }

    /// Merge additional font chains into the existing cache
    ///
    /// Useful when processing multiple DOMs that may have different font requirements.
    pub fn merge_font_chain_cache(
        &mut self,
        chains: HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    ) {
        self.font_chain_cache.extend(chains);
    }

    /// Get a reference to the font chain cache
    pub fn get_font_chain_cache(
        &self,
    ) -> &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain> {
        &self.font_chain_cache
    }

    /// Get an embedded font by its hash (used for WebRender registration)
    /// Returns the FontRef if it exists in the embedded_fonts cache.
    pub fn get_embedded_font_by_hash(&self, font_hash: u64) -> Option<azul_css::props::basic::FontRef> {
        let embedded = self.embedded_fonts.lock().unwrap();
        embedded.get(&font_hash).cloned()
    }

    /// Get a parsed font by its hash (used for WebRender registration)
    /// Returns the parsed font if it exists in the parsed_fonts cache.
    pub fn get_font_by_hash(&self, font_hash: u64) -> Option<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        // Linear search through all cached fonts to find one with matching hash
        for (_, font) in parsed.iter() {
            if font.get_hash() == font_hash {
                return Some(font.clone());
            }
        }
        None
    }

    /// Register an embedded FontRef for later lookup by hash
    /// This is called when using FontStack::Ref during shaping
    pub fn register_embedded_font(&self, font_ref: &azul_css::props::basic::FontRef) {
        let hash = font_ref.get_hash();
        let mut embedded = self.embedded_fonts.lock().unwrap();
        embedded.insert(hash, font_ref.clone());
    }

    /// Get a snapshot of all currently loaded fonts
    ///
    /// This returns a copy of all parsed fonts, which can be passed to the shaper.
    /// No locking is required after this call - the returned HashMap is independent.
    ///
    /// NOTE: This should be called AFTER loading all required fonts for a layout pass.
    pub fn get_loaded_fonts(&self) -> LoadedFonts<T> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed
            .iter()
            .map(|(id, font)| (id.clone(), font.shallow_clone()))
            .collect()
    }

    /// Get the set of FontIds that are currently loaded
    ///
    /// This is useful for computing which fonts need to be loaded
    /// (diff with required fonts).
    pub fn get_loaded_font_ids(&self) -> std::collections::HashSet<FontId> {
        let parsed = self.parsed_fonts.lock().unwrap();
        parsed.keys().cloned().collect()
    }

    /// Insert a loaded font into the cache
    ///
    /// Returns the old font if one was already present for this FontId.
    pub fn insert_font(&self, font_id: FontId, font: T) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.insert(font_id, font)
    }

    /// Insert multiple loaded fonts into the cache
    ///
    /// This is more efficient than calling `insert_font` multiple times
    /// because it only acquires the lock once.
    pub fn insert_fonts(&self, fonts: impl IntoIterator<Item = (FontId, T)>) {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        for (font_id, font) in fonts {
            parsed.insert(font_id, font);
        }
    }

    /// Remove a font from the cache
    ///
    /// Returns the removed font if it was present.
    pub fn remove_font(&self, font_id: &FontId) -> Option<T> {
        let mut parsed = self.parsed_fonts.lock().unwrap();
        parsed.remove(font_id)
    }
}

// Error handling
#[derive(Debug, thiserror::Error)]
pub enum LayoutError {
    #[error("Bidi analysis failed: {0}")]
    BidiError(String),
    #[error("Shaping failed: {0}")]
    ShapingError(String),
    #[error("Font not found: {0:?}")]
    FontNotFound(FontSelector),
    #[error("Invalid text input: {0}")]
    InvalidText(String),
    #[error("Hyphenation failed: {0}")]
    HyphenationError(String),
}

/// Text boundary types for cursor movement
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextBoundary {
    /// Reached top of text (first line)
    Top,
    /// Reached bottom of text (last line)
    Bottom,
    /// Reached start of text (first character)
    Start,
    /// Reached end of text (last character)
    End,
}

/// Error returned when cursor movement hits a boundary
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorBoundsError {
    /// The boundary that was hit
    pub boundary: TextBoundary,
    /// The cursor position (unchanged from input)
    pub cursor: TextCursor,
}

/// Unified constraints combining all layout features
///
/// # CSS Inline Layout Module Level 3: Constraint Mapping
///
/// This structure maps CSS properties to layout constraints:
///
/// ## \u00a7 2.1 Layout of Line Boxes
/// - `available_width`: \u26a0\ufe0f CRITICAL - Should equal containing block's inner width
///   * Currently defaults to 0.0 which causes immediate line breaking
///   * Per spec: "logical width of a line box is equal to the inner logical width of its containing
///     block"
/// - `available_height`: For block-axis constraints (max-height)
///
/// ## \u00a7 2.2 Layout Within Line Boxes
/// - `text_align`: \u2705 Horizontal alignment (start, end, center, justify)
/// - `vertical_align`: \u26a0\ufe0f PARTIAL - Only baseline supported, missing:
///   * top, bottom, middle, text-top, text-bottom
///   * <length>, <percentage> values
///   * sub, super positions
/// - `line_height`: \u2705 Distance between baselines
///
/// ## \u00a7 3 Baselines and Alignment Metrics
/// - `text_orientation`: \u2705 For vertical writing (sideways, upright)
/// - `writing_mode`: \u2705 horizontal-tb, vertical-rl, vertical-lr
/// - `direction`: \u2705 ltr, rtl for BiDi
///
/// ## \u00a7 4 Baseline Alignment (vertical-align property)
/// \u26a0\ufe0f INCOMPLETE: Only basic baseline alignment implemented
///
/// ## \u00a7 5 Line Spacing (line-height property)
/// - `line_height`: \u2705 Implemented
/// - \u274c MISSING: line-fit-edge for controlling which edges contribute to line height
///
/// ## \u00a7 6 Trimming Leading (text-box-trim)
/// - \u274c NOT IMPLEMENTED: text-box-trim property
/// - \u274c NOT IMPLEMENTED: text-box-edge property
///
/// ## CSS Text Module Level 3
/// - `text_indent`: \u2705 First line indentation
/// - `text_justify`: \u2705 Justification algorithm (auto, inter-word, inter-character)
/// - `hyphenation`: \u2705 Automatic hyphenation
/// - `hanging_punctuation`: \u2705 Hanging punctuation at line edges
///
/// ## CSS Text Level 4
/// - `text_wrap`: \u2705 balance, pretty, stable
/// - `line_clamp`: \u2705 Max number of lines
///
/// ## CSS Writing Modes Level 4
/// - `text_combine_upright`: \u2705 Tate-chu-yoko for vertical text
///
/// ## CSS Shapes Module
/// - `shape_boundaries`: \u2705 Custom line box shapes
/// - `shape_exclusions`: \u2705 Exclusion areas (float-like behavior)
/// - `exclusion_margin`: \u2705 Margin around exclusions
///
/// ## Multi-column Layout
/// - `columns`: \u2705 Number of columns
/// - `column_gap`: \u2705 Gap between columns
///
/// # Known Issues:
/// 1. [ISSUE] available_width defaults to Definite(0.0) instead of containing block width
/// 2. [ISSUE] vertical_align only supports baseline
/// 3. [TODO] initial-letter (drop caps) not implemented
#[derive(Debug, Clone)]
pub struct UnifiedConstraints {
    // Shape definition
    pub shape_boundaries: Vec<ShapeBoundary>,
    pub shape_exclusions: Vec<ShapeBoundary>,

    // Basic layout - using AvailableSpace for proper indefinite handling
    pub available_width: AvailableSpace,
    pub available_height: Option<f32>,

    // Text layout
    pub writing_mode: Option<WritingMode>,
    // Base direction from CSS, overrides auto-detection
    pub direction: Option<BidiDirection>,
    pub text_orientation: TextOrientation,
    pub text_align: TextAlign,
    pub text_justify: JustifyContent,
    pub line_height: f32,
    pub vertical_align: VerticalAlign,

    // Overflow handling
    pub overflow: OverflowBehavior,
    pub segment_alignment: SegmentAlignment,

    // Advanced features
    pub text_combine_upright: Option<TextCombineUpright>,
    pub exclusion_margin: f32,
    pub hyphenation: bool,
    pub hyphenation_language: Option<Language>,
    pub text_indent: f32,
    pub initial_letter: Option<InitialLetter>,
    pub line_clamp: Option<NonZeroUsize>,

    // text-wrap: balance
    pub text_wrap: TextWrap,
    pub columns: u32,
    pub column_gap: f32,
    pub hanging_punctuation: bool,
}

impl Default for UnifiedConstraints {
    fn default() -> Self {
        Self {
            shape_boundaries: Vec::new(),
            shape_exclusions: Vec::new(),

            // IMPORTANT: This should be set to the containing block's inner width
            // per CSS Inline-3 § 2.1, but defaults to Definite(0.0) which causes immediate line
            // breaking. This value should be passed from the box layout solver (fc.rs)
            // when creating UnifiedConstraints for text layout.
            available_width: AvailableSpace::Definite(0.0),
            available_height: None,
            writing_mode: None,
            direction: None, // Will default to LTR if not specified
            text_orientation: TextOrientation::default(),
            text_align: TextAlign::default(),
            text_justify: JustifyContent::default(),
            line_height: 16.0, // A more sensible default
            vertical_align: VerticalAlign::default(),
            overflow: OverflowBehavior::default(),
            segment_alignment: SegmentAlignment::default(),
            text_combine_upright: None,
            exclusion_margin: 0.0,
            hyphenation: false,
            hyphenation_language: None,
            columns: 1,
            column_gap: 0.0,
            hanging_punctuation: false,
            text_indent: 0.0,
            initial_letter: None,
            line_clamp: None,
            text_wrap: TextWrap::default(),
        }
    }
}

// UnifiedConstraints
impl Hash for UnifiedConstraints {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_boundaries.hash(state);
        self.shape_exclusions.hash(state);
        self.available_width.hash(state);
        self.available_height
            .map(|h| h.round() as usize)
            .hash(state);
        self.writing_mode.hash(state);
        self.direction.hash(state);
        self.text_orientation.hash(state);
        self.text_align.hash(state);
        self.text_justify.hash(state);
        (self.line_height.round() as usize).hash(state);
        self.vertical_align.hash(state);
        self.overflow.hash(state);
        self.text_combine_upright.hash(state);
        (self.exclusion_margin.round() as usize).hash(state);
        self.hyphenation.hash(state);
        self.hyphenation_language.hash(state);
        self.columns.hash(state);
        (self.column_gap.round() as usize).hash(state);
        self.hanging_punctuation.hash(state);
    }
}

impl PartialEq for UnifiedConstraints {
    fn eq(&self, other: &Self) -> bool {
        self.shape_boundaries == other.shape_boundaries
            && self.shape_exclusions == other.shape_exclusions
            && self.available_width == other.available_width
            && match (self.available_height, other.available_height) {
                (None, None) => true,
                (Some(h1), Some(h2)) => round_eq(h1, h2),
                _ => false,
            }
            && self.writing_mode == other.writing_mode
            && self.direction == other.direction
            && self.text_orientation == other.text_orientation
            && self.text_align == other.text_align
            && self.text_justify == other.text_justify
            && round_eq(self.line_height, other.line_height)
            && self.vertical_align == other.vertical_align
            && self.overflow == other.overflow
            && self.text_combine_upright == other.text_combine_upright
            && round_eq(self.exclusion_margin, other.exclusion_margin)
            && self.hyphenation == other.hyphenation
            && self.hyphenation_language == other.hyphenation_language
            && self.columns == other.columns
            && round_eq(self.column_gap, other.column_gap)
            && self.hanging_punctuation == other.hanging_punctuation
    }
}

impl Eq for UnifiedConstraints {}

impl UnifiedConstraints {
    fn direction(&self, fallback: BidiDirection) -> BidiDirection {
        match self.writing_mode {
            Some(s) => s.get_direction().unwrap_or(fallback),
            None => fallback,
        }
    }
    fn is_vertical(&self) -> bool {
        matches!(
            self.writing_mode,
            Some(WritingMode::VerticalRl) | Some(WritingMode::VerticalLr)
        )
    }
}

/// Line constraints with multi-segment support
#[derive(Debug, Clone)]
pub struct LineConstraints {
    pub segments: Vec<LineSegment>,
    pub total_available: f32,
}

impl WritingMode {
    fn get_direction(&self) -> Option<BidiDirection> {
        match self {
            // determined by text content
            WritingMode::HorizontalTb => None,
            WritingMode::VerticalRl => Some(BidiDirection::Rtl),
            WritingMode::VerticalLr => Some(BidiDirection::Ltr),
            WritingMode::SidewaysRl => Some(BidiDirection::Rtl),
            WritingMode::SidewaysLr => Some(BidiDirection::Ltr),
        }
    }
}

// Stage 1: Collection - Styled runs from DOM traversal
#[derive(Debug, Clone, Hash)]
pub struct StyledRun {
    pub text: String,
    pub style: Arc<StyleProperties>,
    /// Byte index in the original logical paragraph text
    pub logical_start_byte: usize,
    /// The DOM NodeId of the Text node this run came from.
    /// None for generated content (e.g., list markers, ::before/::after).
    pub source_node_id: Option<NodeId>,
}

// Stage 2: Bidi Analysis - Visual runs in display order
#[derive(Debug, Clone)]
pub struct VisualRun<'a> {
    pub text_slice: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start_byte: usize,
    pub bidi_level: BidiLevel,
    pub script: Script,
    pub language: Language,
}

// Font and styling types

/// A selector for loading fonts from the font cache.
/// Used by FontManager to query fontconfig and load font files.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontSelector {
    pub family: String,
    pub weight: FcWeight,
    pub style: FontStyle,
    pub unicode_ranges: Vec<UnicodeRange>,
}

impl Default for FontSelector {
    fn default() -> Self {
        Self {
            family: "serif".to_string(),
            weight: FcWeight::Normal,
            style: FontStyle::Normal,
            unicode_ranges: Vec::new(),
        }
    }
}

/// Font stack that can be either a list of font selectors (resolved via fontconfig)
/// or a direct FontRef (bypasses fontconfig entirely).
///
/// When a `FontRef` is used, it bypasses fontconfig resolution entirely
/// and uses the pre-parsed font data directly. This is used for embedded
/// fonts like Material Icons.
#[derive(Debug, Clone)]
pub enum FontStack {
    /// A stack of font selectors to be resolved via fontconfig
    /// First font is primary, rest are fallbacks
    Stack(Vec<FontSelector>),
    /// A direct reference to a pre-parsed font (e.g., embedded icon fonts)
    /// This font covers the entire Unicode range and has no fallbacks.
    Ref(azul_css::props::basic::font::FontRef),
}

impl Default for FontStack {
    fn default() -> Self {
        FontStack::Stack(vec![FontSelector::default()])
    }
}

impl FontStack {
    /// Returns true if this is a direct FontRef
    pub fn is_ref(&self) -> bool {
        matches!(self, FontStack::Ref(_))
    }

    /// Returns the FontRef if this is a Ref variant
    pub fn as_ref(&self) -> Option<&azul_css::props::basic::font::FontRef> {
        match self {
            FontStack::Ref(r) => Some(r),
            _ => None,
        }
    }

    /// Returns the font selectors if this is a Stack variant
    pub fn as_stack(&self) -> Option<&[FontSelector]> {
        match self {
            FontStack::Stack(s) => Some(s),
            _ => None,
        }
    }

    /// Returns the first FontSelector if this is a Stack variant, None if Ref
    pub fn first_selector(&self) -> Option<&FontSelector> {
        match self {
            FontStack::Stack(s) => s.first(),
            FontStack::Ref(_) => None,
        }
    }

    /// Returns the first font family name (for Stack) or a placeholder (for Ref)
    pub fn first_family(&self) -> &str {
        match self {
            FontStack::Stack(s) => s.first().map(|f| f.family.as_str()).unwrap_or("serif"),
            FontStack::Ref(_) => "<embedded-font>",
        }
    }
}

impl PartialEq for FontStack {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (FontStack::Stack(a), FontStack::Stack(b)) => a == b,
            (FontStack::Ref(a), FontStack::Ref(b)) => a.parsed == b.parsed,
            _ => false,
        }
    }
}

impl Eq for FontStack {}

impl Hash for FontStack {
    fn hash<H: Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            FontStack::Stack(s) => s.hash(state),
            FontStack::Ref(r) => (r.parsed as usize).hash(state),
        }
    }
}

/// A reference to a font for rendering, identified by its hash.
/// This hash corresponds to ParsedFont::hash and is used to look up
/// the actual font data in the renderer's font cache.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontHash {
    /// The hash of the ParsedFont. 0 means invalid/unknown font.
    pub font_hash: u64,
}

impl FontHash {
    pub fn invalid() -> Self {
        Self { font_hash: 0 }
    }

    pub fn from_hash(font_hash: u64) -> Self {
        Self { font_hash }
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

/// Defines how text should be aligned when a line contains multiple disjoint segments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum SegmentAlignment {
    /// Align text within the first available segment on the line.
    #[default]
    First,
    /// Align text relative to the total available width of all
    /// segments on the line combined.
    Total,
}

#[derive(Debug, Clone)]
pub struct VerticalMetrics {
    pub advance: f32,
    pub bearing_x: f32,
    pub bearing_y: f32,
    pub origin_y: f32,
}

/// Layout-specific font metrics extracted from FontMetrics
/// Contains only the metrics needed for text layout and rendering
#[derive(Debug, Clone)]
pub struct LayoutFontMetrics {
    pub ascent: f32,
    pub descent: f32,
    pub line_gap: f32,
    pub units_per_em: u16,
}

impl LayoutFontMetrics {
    pub fn baseline_scaled(&self, font_size: f32) -> f32 {
        let scale = font_size / self.units_per_em as f32;
        self.ascent * scale
    }

    /// Convert from full FontMetrics to layout-specific metrics
    pub fn from_font_metrics(metrics: &azul_css::props::basic::FontMetrics) -> Self {
        Self {
            ascent: metrics.ascender as f32,
            descent: metrics.descender as f32,
            line_gap: metrics.line_gap as f32,
            units_per_em: metrics.units_per_em,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LineSegment {
    pub start_x: f32,
    pub width: f32,
    // For choosing best segment when multiple available
    pub priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextWrap {
    #[default]
    Wrap,
    Balance,
    NoWrap,
}

// initial-letter
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct InitialLetter {
    /// How many lines tall the initial letter should be.
    pub size: f32,
    /// How many lines the letter should sink into.
    pub sink: u32,
    /// How many characters to apply this styling to.
    pub count: NonZeroUsize,
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// This is a marker trait, indicating that `a == b` is a true equivalence
// relation. The derived `PartialEq` already satisfies this.
impl Eq for InitialLetter {}

impl Hash for InitialLetter {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Per the request, round the f32 to a usize for hashing.
        // This is a lossy conversion; values like 2.3 and 2.4 will produce
        // the same hash value for this field. This is acceptable as long as
        // the `PartialEq` implementation correctly distinguishes them.
        (self.size.round() as usize).hash(state);
        self.sink.hash(state);
        self.count.hash(state);
    }
}

// Path and shape definitions
#[derive(Debug, Clone, PartialOrd)]
pub enum PathSegment {
    MoveTo(Point),
    LineTo(Point),
    CurveTo {
        control1: Point,
        control2: Point,
        end: Point,
    },
    QuadTo {
        control: Point,
        end: Point,
    },
    Arc {
        center: Point,
        radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
    Close,
}

// PathSegment
impl Hash for PathSegment {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Hash the enum variant's discriminant first to distinguish them
        discriminant(self).hash(state);

        match self {
            PathSegment::MoveTo(p) => p.hash(state),
            PathSegment::LineTo(p) => p.hash(state),
            PathSegment::CurveTo {
                control1,
                control2,
                end,
            } => {
                control1.hash(state);
                control2.hash(state);
                end.hash(state);
            }
            PathSegment::QuadTo { control, end } => {
                control.hash(state);
                end.hash(state);
            }
            PathSegment::Arc {
                center,
                radius,
                start_angle,
                end_angle,
            } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
                (start_angle.round() as usize).hash(state);
                (end_angle.round() as usize).hash(state);
            }
            PathSegment::Close => {} // No data to hash
        }
    }
}

impl PartialEq for PathSegment {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (PathSegment::MoveTo(a), PathSegment::MoveTo(b)) => a == b,
            (PathSegment::LineTo(a), PathSegment::LineTo(b)) => a == b,
            (
                PathSegment::CurveTo {
                    control1: c1a,
                    control2: c2a,
                    end: ea,
                },
                PathSegment::CurveTo {
                    control1: c1b,
                    control2: c2b,
                    end: eb,
                },
            ) => c1a == c1b && c2a == c2b && ea == eb,
            (
                PathSegment::QuadTo {
                    control: ca,
                    end: ea,
                },
                PathSegment::QuadTo {
                    control: cb,
                    end: eb,
                },
            ) => ca == cb && ea == eb,
            (
                PathSegment::Arc {
                    center: ca,
                    radius: ra,
                    start_angle: sa_a,
                    end_angle: ea_a,
                },
                PathSegment::Arc {
                    center: cb,
                    radius: rb,
                    start_angle: sa_b,
                    end_angle: ea_b,
                },
            ) => ca == cb && round_eq(*ra, *rb) && round_eq(*sa_a, *sa_b) && round_eq(*ea_a, *ea_b),
            (PathSegment::Close, PathSegment::Close) => true,
            _ => false, // Variants are different
        }
    }
}

impl Eq for PathSegment {}

// Enhanced content model supporting mixed inline content
#[derive(Debug, Clone, Hash)]
pub enum InlineContent {
    Text(StyledRun),
    Image(InlineImage),
    Shape(InlineShape),
    Space(InlineSpace),
    LineBreak(InlineBreak),
    Tab,
    /// List marker (::marker pseudo-element)
    /// Markers with list-style-position: outside are positioned
    /// in the padding gutter of the list container
    Marker {
        run: StyledRun,
        /// Whether marker is positioned outside (in padding) or inside (inline)
        position_outside: bool,
    },
    // Ruby annotation
    Ruby {
        base: Vec<InlineContent>,
        text: Vec<InlineContent>,
        // Style for the ruby text itself
        style: Arc<StyleProperties>,
    },
}

#[derive(Debug, Clone)]
pub struct InlineImage {
    pub source: ImageSource,
    pub intrinsic_size: Size,
    pub display_size: Option<Size>,
    // How much to shift baseline
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub object_fit: ObjectFit,
}

impl PartialEq for InlineImage {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.source == other.source
            && self.intrinsic_size == other.intrinsic_size
            && self.display_size == other.display_size
            && self.alignment == other.alignment
            && self.object_fit == other.object_fit
    }
}

impl Eq for InlineImage {}

impl Hash for InlineImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.intrinsic_size.hash(state);
        self.display_size.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.alignment.hash(state);
        self.object_fit.hash(state);
    }
}

impl PartialOrd for InlineImage {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineImage {
    fn cmp(&self, other: &Self) -> Ordering {
        self.source
            .cmp(&other.source)
            .then_with(|| self.intrinsic_size.cmp(&other.intrinsic_size))
            .then_with(|| self.display_size.cmp(&other.display_size))
            .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
            .then_with(|| self.alignment.cmp(&other.alignment))
            .then_with(|| self.object_fit.cmp(&other.object_fit))
    }
}

/// Enhanced glyph with all features
#[derive(Debug, Clone)]
pub struct Glyph {
    // Core glyph data
    pub glyph_id: u16,
    pub codepoint: char,
    /// Hash of the font - use LoadedFonts to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
    pub style: Arc<StyleProperties>,
    pub source: GlyphSource,

    // Text mapping
    pub logical_byte_index: usize,
    pub logical_byte_len: usize,
    pub content_index: usize,
    pub cluster: u32,

    // Metrics
    pub advance: f32,
    pub kerning: f32,
    pub offset: Point,

    // Vertical text support
    pub vertical_advance: f32,
    pub vertical_origin_y: f32, // from VORG
    pub vertical_bearing: Point,
    pub orientation: GlyphOrientation,

    // Layout properties
    pub script: Script,
    pub bidi_level: BidiLevel,
}

impl Glyph {
    #[inline]
    fn bounds(&self) -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: self.advance,
            height: self.style.line_height,
        }
    }

    #[inline]
    fn character_class(&self) -> CharacterClass {
        classify_character(self.codepoint as u32)
    }

    #[inline]
    fn is_whitespace(&self) -> bool {
        self.character_class() == CharacterClass::Space
    }

    #[inline]
    fn can_justify(&self) -> bool {
        !self.codepoint.is_whitespace() && self.character_class() != CharacterClass::Combining
    }

    #[inline]
    fn justification_priority(&self) -> u8 {
        get_justification_priority(self.character_class())
    }

    #[inline]
    fn break_opportunity_after(&self) -> bool {
        let is_whitespace = self.codepoint.is_whitespace();
        let is_soft_hyphen = self.codepoint == '\u{00AD}';
        is_whitespace || is_soft_hyphen
    }
}

// Information about text runs after initial analysis
#[derive(Debug, Clone)]
pub struct TextRunInfo<'a> {
    pub text: &'a str,
    pub style: Arc<StyleProperties>,
    pub logical_start: usize,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub enum ImageSource {
    /// Direct reference to decoded image (from DOM NodeType::Image)
    Ref(ImageRef),
    /// CSS url reference (from background-image, needs ImageCache lookup)
    Url(String),
    /// Raw image data
    Data(Arc<[u8]>),
    /// SVG source
    Svg(Arc<str>),
    /// Placeholder for layout without actual image
    Placeholder(Size),
}

impl PartialEq for ImageSource {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ImageSource::Ref(a), ImageSource::Ref(b)) => a.get_hash() == b.get_hash(),
            (ImageSource::Url(a), ImageSource::Url(b)) => a == b,
            (ImageSource::Data(a), ImageSource::Data(b)) => Arc::ptr_eq(a, b),
            (ImageSource::Svg(a), ImageSource::Svg(b)) => Arc::ptr_eq(a, b),
            (ImageSource::Placeholder(a), ImageSource::Placeholder(b)) => {
                a.width.to_bits() == b.width.to_bits() && a.height.to_bits() == b.height.to_bits()
            }
            _ => false,
        }
    }
}

impl Eq for ImageSource {}

impl std::hash::Hash for ImageSource {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        match self {
            ImageSource::Ref(r) => r.get_hash().hash(state),
            ImageSource::Url(s) => s.hash(state),
            ImageSource::Data(d) => (Arc::as_ptr(d) as *const u8 as usize).hash(state),
            ImageSource::Svg(s) => (Arc::as_ptr(s) as *const u8 as usize).hash(state),
            ImageSource::Placeholder(sz) => {
                sz.width.to_bits().hash(state);
                sz.height.to_bits().hash(state);
            }
        }
    }
}

impl PartialOrd for ImageSource {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ImageSource {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        fn variant_index(s: &ImageSource) -> u8 {
            match s {
                ImageSource::Ref(_) => 0,
                ImageSource::Url(_) => 1,
                ImageSource::Data(_) => 2,
                ImageSource::Svg(_) => 3,
                ImageSource::Placeholder(_) => 4,
            }
        }
        match (self, other) {
            (ImageSource::Ref(a), ImageSource::Ref(b)) => a.get_hash().cmp(&b.get_hash()),
            (ImageSource::Url(a), ImageSource::Url(b)) => a.cmp(b),
            (ImageSource::Data(a), ImageSource::Data(b)) => {
                (Arc::as_ptr(a) as *const u8 as usize).cmp(&(Arc::as_ptr(b) as *const u8 as usize))
            }
            (ImageSource::Svg(a), ImageSource::Svg(b)) => {
                (Arc::as_ptr(a) as *const u8 as usize).cmp(&(Arc::as_ptr(b) as *const u8 as usize))
            }
            (ImageSource::Placeholder(a), ImageSource::Placeholder(b)) => {
                (a.width.to_bits(), a.height.to_bits())
                    .cmp(&(b.width.to_bits(), b.height.to_bits()))
            }
            // Different variants: compare by variant index
            _ => variant_index(self).cmp(&variant_index(other)),
        }
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum VerticalAlign {
    // Align image baseline with text baseline
    #[default]
    Baseline,
    // Align image bottom with line bottom
    Bottom,
    // Align image top with line top
    Top,
    // Align image middle with text middle
    Middle,
    // Align with tallest text in line
    TextTop,
    // Align with lowest text in line
    TextBottom,
    // Subscript alignment
    Sub,
    // Superscript alignment
    Super,
    // Custom offset from baseline
    Offset(f32),
}

impl std::hash::Hash for VerticalAlign {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        core::mem::discriminant(self).hash(state);
        if let VerticalAlign::Offset(f) = self {
            f.to_bits().hash(state);
        }
    }
}

impl Eq for VerticalAlign {}

impl Ord for VerticalAlign {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.partial_cmp(other).unwrap_or(std::cmp::Ordering::Equal)
    }
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum ObjectFit {
    // Stretch to fit display size
    Fill,
    // Scale to fit within display size
    Contain,
    // Scale to cover display size
    Cover,
    // Use intrinsic size
    None,
    // Like contain but never scale up
    ScaleDown,
}

/// Border information for inline elements (display: inline, inline-block)
///
/// This stores the resolved border properties needed for rendering inline element borders.
/// Unlike block elements which render borders via paint_node_background_and_border(),
/// inline element borders must be rendered per glyph-run to handle line breaks correctly.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineBorderInfo {
    /// Border widths in pixels for each side
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
    /// Border colors for each side
    pub top_color: ColorU,
    pub right_color: ColorU,
    pub bottom_color: ColorU,
    pub left_color: ColorU,
    /// Border radius (if any)
    pub radius: Option<f32>,
}

impl Default for InlineBorderInfo {
    fn default() -> Self {
        Self {
            top: 0.0,
            right: 0.0,
            bottom: 0.0,
            left: 0.0,
            top_color: ColorU::TRANSPARENT,
            right_color: ColorU::TRANSPARENT,
            bottom_color: ColorU::TRANSPARENT,
            left_color: ColorU::TRANSPARENT,
            radius: None,
        }
    }
}

impl InlineBorderInfo {
    /// Returns true if any border has a non-zero width
    pub fn has_border(&self) -> bool {
        self.top > 0.0 || self.right > 0.0 || self.bottom > 0.0 || self.left > 0.0
    }
}

#[derive(Debug, Clone)]
pub struct InlineShape {
    pub shape_def: ShapeDefinition,
    pub fill: Option<ColorU>,
    pub stroke: Option<Stroke>,
    pub baseline_offset: f32,
    /// The NodeId of the element that created this shape
    /// (e.g., inline-block) - this allows us to look up
    /// styling information (background, border) when rendering
    pub source_node_id: Option<azul_core::dom::NodeId>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OverflowBehavior {
    // Content extends outside shape
    Visible,
    // Content is clipped to shape
    Hidden,
    // Scrollable overflow
    Scroll,
    // Browser/system decides
    #[default]
    Auto,
    // Break into next shape/page
    Break,
}

#[derive(Debug, Clone)]
pub struct MeasuredImage {
    pub source: ImageSource,
    pub size: Size,
    pub baseline_offset: f32,
    pub alignment: VerticalAlign,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct MeasuredShape {
    pub shape_def: ShapeDefinition,
    pub size: Size,
    pub baseline_offset: f32,
    pub content_index: usize,
}

#[derive(Debug, Clone)]
pub struct InlineSpace {
    pub width: f32,
    pub is_breaking: bool, // Can line break here
    pub is_stretchy: bool, // Can be expanded for justification
}

impl PartialEq for InlineSpace {
    fn eq(&self, other: &Self) -> bool {
        self.width.to_bits() == other.width.to_bits()
            && self.is_breaking == other.is_breaking
            && self.is_stretchy == other.is_stretchy
    }
}

impl Eq for InlineSpace {}

impl Hash for InlineSpace {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.width.to_bits().hash(state);
        self.is_breaking.hash(state);
        self.is_stretchy.hash(state);
    }
}

impl PartialOrd for InlineSpace {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for InlineSpace {
    fn cmp(&self, other: &Self) -> Ordering {
        self.width
            .total_cmp(&other.width)
            .then_with(|| self.is_breaking.cmp(&other.is_breaking))
            .then_with(|| self.is_stretchy.cmp(&other.is_stretchy))
    }
}

impl PartialEq for InlineShape {
    fn eq(&self, other: &Self) -> bool {
        self.baseline_offset.to_bits() == other.baseline_offset.to_bits()
            && self.shape_def == other.shape_def
            && self.fill == other.fill
            && self.stroke == other.stroke
            && self.source_node_id == other.source_node_id
    }
}

impl Eq for InlineShape {}

impl Hash for InlineShape {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.shape_def.hash(state);
        self.fill.hash(state);
        self.stroke.hash(state);
        self.baseline_offset.to_bits().hash(state);
        self.source_node_id.hash(state);
    }
}

impl PartialOrd for InlineShape {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(
            self.shape_def
                .partial_cmp(&other.shape_def)?
                .then_with(|| self.fill.cmp(&other.fill))
                .then_with(|| {
                    self.stroke
                        .partial_cmp(&other.stroke)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| self.baseline_offset.total_cmp(&other.baseline_offset))
                .then_with(|| self.source_node_id.cmp(&other.source_node_id)),
        )
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PartialEq for Rect {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x)
            && round_eq(self.y, other.y)
            && round_eq(self.width, other.width)
            && round_eq(self.height, other.height)
    }
}
impl Eq for Rect {}

impl Hash for Rect {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // The order in which you hash the fields matters.
        // A consistent order is crucial.
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Size {
    pub width: f32,
    pub height: f32,
}

impl Ord for Size {
    fn cmp(&self, other: &Self) -> Ordering {
        (self.width.round() as usize)
            .cmp(&(other.width.round() as usize))
            .then_with(|| (self.height.round() as usize).cmp(&(other.height.round() as usize)))
    }
}

// Size
impl Hash for Size {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.width.round() as usize).hash(state);
        (self.height.round() as usize).hash(state);
    }
}
impl PartialEq for Size {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.width, other.width) && round_eq(self.height, other.height)
    }
}
impl Eq for Size {}

impl Size {
    pub const fn zero() -> Self {
        Self::new(0.0, 0.0)
    }
    pub const fn new(width: f32, height: f32) -> Self {
        Self { width, height }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialOrd)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}

// Point
impl Hash for Point {
    fn hash<H: Hasher>(&self, state: &mut H) {
        (self.x.round() as usize).hash(state);
        (self.y.round() as usize).hash(state);
    }
}

impl PartialEq for Point {
    fn eq(&self, other: &Self) -> bool {
        round_eq(self.x, other.x) && round_eq(self.y, other.y)
    }
}

impl Eq for Point {}

#[derive(Debug, Clone, PartialOrd)]
pub enum ShapeDefinition {
    Rectangle {
        size: Size,
        corner_radius: Option<f32>,
    },
    Circle {
        radius: f32,
    },
    Ellipse {
        radii: Size,
    },
    Polygon {
        points: Vec<Point>,
    },
    Path {
        segments: Vec<PathSegment>,
    },
}

// ShapeDefinition
impl Hash for ShapeDefinition {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeDefinition::Rectangle {
                size,
                corner_radius,
            } => {
                size.hash(state);
                corner_radius.map(|r| r.round() as usize).hash(state);
            }
            ShapeDefinition::Circle { radius } => {
                (radius.round() as usize).hash(state);
            }
            ShapeDefinition::Ellipse { radii } => {
                radii.hash(state);
            }
            ShapeDefinition::Polygon { points } => {
                // Since Point implements Hash, we can hash the Vec directly.
                points.hash(state);
            }
            ShapeDefinition::Path { segments } => {
                // Same for Vec<PathSegment>
                segments.hash(state);
            }
        }
    }
}

impl PartialEq for ShapeDefinition {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                ShapeDefinition::Rectangle {
                    size: s1,
                    corner_radius: r1,
                },
                ShapeDefinition::Rectangle {
                    size: s2,
                    corner_radius: r2,
                },
            ) => {
                s1 == s2
                    && match (r1, r2) {
                        (None, None) => true,
                        (Some(v1), Some(v2)) => round_eq(*v1, *v2),
                        _ => false,
                    }
            }
            (ShapeDefinition::Circle { radius: r1 }, ShapeDefinition::Circle { radius: r2 }) => {
                round_eq(*r1, *r2)
            }
            (ShapeDefinition::Ellipse { radii: r1 }, ShapeDefinition::Ellipse { radii: r2 }) => {
                r1 == r2
            }
            (ShapeDefinition::Polygon { points: p1 }, ShapeDefinition::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeDefinition::Path { segments: s1 }, ShapeDefinition::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeDefinition {}

impl ShapeDefinition {
    /// Calculates the bounding box size for the shape.
    pub fn get_size(&self) -> Size {
        match self {
            // The size is explicitly defined.
            ShapeDefinition::Rectangle { size, .. } => *size,

            // The bounding box of a circle is a square with sides equal to the diameter.
            ShapeDefinition::Circle { radius } => {
                let diameter = radius * 2.0;
                Size::new(diameter, diameter)
            }

            // The bounding box of an ellipse has width and height equal to twice its radii.
            ShapeDefinition::Ellipse { radii } => Size::new(radii.width * 2.0, radii.height * 2.0),

            // For a polygon, we must find the min/max coordinates to get the bounds.
            ShapeDefinition::Polygon { points } => calculate_bounding_box_size(points),

            // For a path, we find the bounding box of all its anchor and control points.
            //
            // NOTE: This is a common and fast approximation. The true bounding box of
            // bezier curves can be slightly smaller than the box containing their control
            // points. For pixel-perfect results, one would need to calculate the
            // curve's extrema.
            ShapeDefinition::Path { segments } => {
                let mut points = Vec::new();
                let mut current_pos = Point { x: 0.0, y: 0.0 };

                for segment in segments {
                    match segment {
                        PathSegment::MoveTo(p) | PathSegment::LineTo(p) => {
                            points.push(*p);
                            current_pos = *p;
                        }
                        PathSegment::QuadTo { control, end } => {
                            points.push(current_pos);
                            points.push(*control);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::CurveTo {
                            control1,
                            control2,
                            end,
                        } => {
                            points.push(current_pos);
                            points.push(*control1);
                            points.push(*control2);
                            points.push(*end);
                            current_pos = *end;
                        }
                        PathSegment::Arc {
                            center,
                            radius,
                            start_angle,
                            end_angle,
                        } => {
                            // 1. Calculate and add the arc's start and end points to the list.
                            let start_point = Point {
                                x: center.x + radius * start_angle.cos(),
                                y: center.y + radius * start_angle.sin(),
                            };
                            let end_point = Point {
                                x: center.x + radius * end_angle.cos(),
                                y: center.y + radius * end_angle.sin(),
                            };
                            points.push(start_point);
                            points.push(end_point);

                            // 2. Normalize the angles to handle cases where the arc crosses the
                            //    0-radian line.
                            // This ensures we can iterate forward from a start to an end angle.
                            let mut normalized_end = *end_angle;
                            while normalized_end < *start_angle {
                                normalized_end += 2.0 * std::f32::consts::PI;
                            }

                            // 3. Find the first cardinal point (multiples of PI/2) at or after the
                            //    start angle.
                            let mut check_angle = (*start_angle / std::f32::consts::FRAC_PI_2)
                                .ceil()
                                * std::f32::consts::FRAC_PI_2;

                            // 4. Iterate through all cardinal points that fall within the arc's
                            //    sweep and add them.
                            // These points define the maximum extent of the arc's bounding box.
                            while check_angle < normalized_end {
                                points.push(Point {
                                    x: center.x + radius * check_angle.cos(),
                                    y: center.y + radius * check_angle.sin(),
                                });
                                check_angle += std::f32::consts::FRAC_PI_2;
                            }

                            // 5. The end of the arc is the new current position for subsequent path
                            //    segments.
                            current_pos = end_point;
                        }
                        PathSegment::Close => {
                            // No new points are added for closing the path
                        }
                    }
                }
                calculate_bounding_box_size(&points)
            }
        }
    }
}

/// Helper function to calculate the size of the bounding box enclosing a set of points.
fn calculate_bounding_box_size(points: &[Point]) -> Size {
    if points.is_empty() {
        return Size::zero();
    }

    let mut min_x = f32::MAX;
    let mut max_x = f32::MIN;
    let mut min_y = f32::MAX;
    let mut max_y = f32::MIN;

    for point in points {
        min_x = min_x.min(point.x);
        max_x = max_x.max(point.x);
        min_y = min_y.min(point.y);
        max_y = max_y.max(point.y);
    }

    // Handle case where points might be collinear or a single point
    if min_x > max_x || min_y > max_y {
        return Size::zero();
    }

    Size::new(max_x - min_x, max_y - min_y)
}

#[derive(Debug, Clone, PartialOrd)]
pub struct Stroke {
    pub color: ColorU,
    pub width: f32,
    pub dash_pattern: Option<Vec<f32>>,
}

// Stroke
impl Hash for Stroke {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.color.hash(state);
        (self.width.round() as usize).hash(state);

        // Manual hashing for Option<Vec<f32>>
        match &self.dash_pattern {
            None => 0u8.hash(state), // Hash a discriminant for None
            Some(pattern) => {
                1u8.hash(state); // Hash a discriminant for Some
                pattern.len().hash(state); // Hash the length
                for &val in pattern {
                    (val.round() as usize).hash(state); // Hash each rounded value
                }
            }
        }
    }
}

impl PartialEq for Stroke {
    fn eq(&self, other: &Self) -> bool {
        if self.color != other.color || !round_eq(self.width, other.width) {
            return false;
        }
        match (&self.dash_pattern, &other.dash_pattern) {
            (None, None) => true,
            (Some(p1), Some(p2)) => {
                p1.len() == p2.len() && p1.iter().zip(p2.iter()).all(|(a, b)| round_eq(*a, *b))
            }
            _ => false,
        }
    }
}

impl Eq for Stroke {}

// Helper function to round f32 for comparison
fn round_eq(a: f32, b: f32) -> bool {
    (a.round() as isize) == (b.round() as isize)
}

#[derive(Debug, Clone)]
pub enum ShapeBoundary {
    Rectangle(Rect),
    Circle { center: Point, radius: f32 },
    Ellipse { center: Point, radii: Size },
    Polygon { points: Vec<Point> },
    Path { segments: Vec<PathSegment> },
}

impl ShapeBoundary {
    pub fn inflate(&self, margin: f32) -> Self {
        if margin == 0.0 {
            return self.clone();
        }
        match self {
            Self::Rectangle(rect) => Self::Rectangle(Rect {
                x: rect.x - margin,
                y: rect.y - margin,
                width: (rect.width + margin * 2.0).max(0.0),
                height: (rect.height + margin * 2.0).max(0.0),
            }),
            Self::Circle { center, radius } => Self::Circle {
                center: *center,
                radius: radius + margin,
            },
            // For simplicity, Polygon and Path inflation is not implemented here.
            // A full implementation would require a geometry library to offset the path.
            _ => self.clone(),
        }
    }
}

// ShapeBoundary
impl Hash for ShapeBoundary {
    fn hash<H: Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            ShapeBoundary::Rectangle(rect) => rect.hash(state),
            ShapeBoundary::Circle { center, radius } => {
                center.hash(state);
                (radius.round() as usize).hash(state);
            }
            ShapeBoundary::Ellipse { center, radii } => {
                center.hash(state);
                radii.hash(state);
            }
            ShapeBoundary::Polygon { points } => points.hash(state),
            ShapeBoundary::Path { segments } => segments.hash(state),
        }
    }
}
impl PartialEq for ShapeBoundary {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (ShapeBoundary::Rectangle(r1), ShapeBoundary::Rectangle(r2)) => r1 == r2,
            (
                ShapeBoundary::Circle {
                    center: c1,
                    radius: r1,
                },
                ShapeBoundary::Circle {
                    center: c2,
                    radius: r2,
                },
            ) => c1 == c2 && round_eq(*r1, *r2),
            (
                ShapeBoundary::Ellipse {
                    center: c1,
                    radii: r1,
                },
                ShapeBoundary::Ellipse {
                    center: c2,
                    radii: r2,
                },
            ) => c1 == c2 && r1 == r2,
            (ShapeBoundary::Polygon { points: p1 }, ShapeBoundary::Polygon { points: p2 }) => {
                p1 == p2
            }
            (ShapeBoundary::Path { segments: s1 }, ShapeBoundary::Path { segments: s2 }) => {
                s1 == s2
            }
            _ => false,
        }
    }
}
impl Eq for ShapeBoundary {}

impl ShapeBoundary {
    /// Converts a CSS shape (from azul-css) to a layout engine ShapeBoundary
    ///
    /// # Arguments
    /// * `css_shape` - The parsed CSS shape from azul-css
    /// * `reference_box` - The containing box for resolving coordinates (from layout solver)
    ///
    /// # Returns
    /// A ShapeBoundary ready for use in the text layout engine
    pub fn from_css_shape(
        css_shape: &azul_css::shape::CssShape,
        reference_box: Rect,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Self {
        use azul_css::shape::CssShape;

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Input CSS shape: {:?}",
                css_shape
            )));
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Reference box: {:?}",
                reference_box
            )));
        }

        let result = match css_shape {
            CssShape::Circle(circle) => {
                let center = Point {
                    x: reference_box.x + circle.center.x,
                    y: reference_box.y + circle.center.y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - CSS center: ({}, {}), radius: {}",
                        circle.center.x, circle.center.y, circle.radius
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Circle - Absolute center: ({}, {}), \
                         radius: {}",
                        center.x, center.y, circle.radius
                    )));
                }
                ShapeBoundary::Circle {
                    center,
                    radius: circle.radius,
                }
            }

            CssShape::Ellipse(ellipse) => {
                let center = Point {
                    x: reference_box.x + ellipse.center.x,
                    y: reference_box.y + ellipse.center.y,
                };
                let radii = Size {
                    width: ellipse.radius_x,
                    height: ellipse.radius_y,
                };
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Ellipse - center: ({}, {}), radii: ({}, \
                         {})",
                        center.x, center.y, radii.width, radii.height
                    )));
                }
                ShapeBoundary::Ellipse { center, radii }
            }

            CssShape::Polygon(polygon) => {
                let points = polygon
                    .points
                    .as_ref()
                    .iter()
                    .map(|pt| Point {
                        x: reference_box.x + pt.x,
                        y: reference_box.y + pt.y,
                    })
                    .collect();
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Polygon - {} points",
                        polygon.points.as_ref().len()
                    )));
                }
                ShapeBoundary::Polygon { points }
            }

            CssShape::Inset(inset) => {
                // Inset defines distances from reference box edges
                let x = reference_box.x + inset.inset_left;
                let y = reference_box.y + inset.inset_top;
                let width = reference_box.width - inset.inset_left - inset.inset_right;
                let height = reference_box.height - inset.inset_top - inset.inset_bottom;

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - insets: ({}, {}, {}, {})",
                        inset.inset_top, inset.inset_right, inset.inset_bottom, inset.inset_left
                    )));
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[ShapeBoundary::from_css_shape] Inset - resulting rect: x={}, y={}, \
                         w={}, h={}",
                        x, y, width, height
                    )));
                }

                ShapeBoundary::Rectangle(Rect {
                    x,
                    y,
                    width: width.max(0.0),
                    height: height.max(0.0),
                })
            }

            CssShape::Path(path) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "[ShapeBoundary::from_css_shape] Path - fallback to rectangle".to_string(),
                    ));
                }
                // TODO: Parse SVG path data into PathSegments
                // For now, fall back to rectangle
                ShapeBoundary::Rectangle(reference_box)
            }
        };

        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[ShapeBoundary::from_css_shape] Result: {:?}",
                result
            )));
        }
        result
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct InlineBreak {
    pub break_type: BreakType,
    pub clear: ClearType,
    pub content_index: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BreakType {
    Soft,   // Preferred break (like <wbr>)
    Hard,   // Forced break (like <br>)
    Page,   // Page break
    Column, // Column break
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ClearType {
    None,
    Left,
    Right,
    Both,
}

// Complex shape constraints for non-rectangular text flow
#[derive(Debug, Clone)]
pub struct ShapeConstraints {
    pub boundaries: Vec<ShapeBoundary>,
    pub exclusions: Vec<ShapeBoundary>,
    pub writing_mode: WritingMode,
    pub text_align: TextAlign,
    pub line_height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum WritingMode {
    #[default]
    HorizontalTb, // horizontal-tb (normal horizontal)
    VerticalRl, // vertical-rl (vertical right-to-left)
    VerticalLr, // vertical-lr (vertical left-to-right)
    SidewaysRl, // sideways-rl (rotated horizontal in vertical context)
    SidewaysLr, // sideways-lr (rotated horizontal in vertical context)
}

impl WritingMode {
    /// Necessary to determine if the glyphs are advancing in a horizontal direction
    pub fn is_advance_horizontal(&self) -> bool {
        matches!(
            self,
            WritingMode::HorizontalTb | WritingMode::SidewaysRl | WritingMode::SidewaysLr
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum JustifyContent {
    #[default]
    None,
    InterWord,      // Expand spaces between words
    InterCharacter, // Expand spaces between all characters (for CJK)
    Distribute,     // Distribute space evenly including start/end
    Kashida,        // Stretch Arabic text using kashidas
}

// Enhanced text alignment with logical directions
#[derive(Debug, Clone, Copy, PartialEq, Default, Hash, Eq, PartialOrd, Ord)]
pub enum TextAlign {
    #[default]
    Left,
    Right,
    Center,
    Justify,
    Start,
    End,        // Logical start/end
    JustifyAll, // Justify including last line
}

// Vertical text orientation for individual characters
#[derive(Debug, Clone, Copy, PartialEq, Default, Eq, PartialOrd, Ord, Hash)]
pub enum TextOrientation {
    #[default]
    Mixed, // Default: upright for scripts, rotated for others
    Upright,  // All characters upright
    Sideways, // All characters rotated 90 degrees
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextDecoration {
    pub underline: bool,
    pub strikethrough: bool,
    pub overline: bool,
}

impl Default for TextDecoration {
    fn default() -> Self {
        TextDecoration {
            underline: false,
            overline: false,
            strikethrough: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum TextTransform {
    #[default]
    None,
    Uppercase,
    Lowercase,
    Capitalize,
}

// Type alias for OpenType feature tags
pub type FourCc = [u8; 4];

// Enum for relative or absolute spacing
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Spacing {
    Px(i32), // Use integer pixels to simplify hashing and equality
    Em(f32),
}

// A type that implements `Hash` must also implement `Eq`.
// Since f32 does not implement `Eq`, we provide a manual implementation.
// The derived `PartialEq` is sufficient for this marker trait.
impl Eq for Spacing {}

impl Hash for Spacing {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // First, hash the enum variant to distinguish between Px and Em.
        discriminant(self).hash(state);
        match self {
            Spacing::Px(val) => val.hash(state),
            // For hashing floats, convert them to their raw bit representation.
            // This ensures that identical float values produce identical hashes.
            Spacing::Em(val) => val.to_bits().hash(state),
        }
    }
}

impl Default for Spacing {
    fn default() -> Self {
        Spacing::Px(0)
    }
}

impl Default for FontHash {
    fn default() -> Self {
        Self::invalid()
    }
}

/// Style properties with vertical text support
#[derive(Debug, Clone, PartialEq)]
pub struct StyleProperties {
    /// Font stack for fallback support (priority order)
    /// Can be either a list of FontSelectors (resolved via fontconfig)
    /// or a direct FontRef (bypasses fontconfig entirely).
    pub font_stack: FontStack,
    pub font_size_px: f32,
    pub color: ColorU,
    /// Background color for inline elements (e.g., `<span style="background-color: yellow">`)
    ///
    /// This is propagated from CSS through the style system and eventually used by
    /// the PDF renderer to draw filled rectangles behind text. The value is `None`
    /// for transparent backgrounds (the default).
    ///
    /// The propagation chain is:
    /// CSS -> `get_style_properties()` -> `StyleProperties` -> `ShapedGlyph` -> `PdfGlyphRun`
    ///
    /// See `PdfGlyphRun::background_color` for how this is used in PDF rendering.
    pub background_color: Option<ColorU>,
    /// Full background content layers (for gradients, images, etc.)
    /// This extends background_color to support CSS gradients on inline elements.
    pub background_content: Vec<StyleBackgroundContent>,
    /// Border information for inline elements
    pub border: Option<InlineBorderInfo>,
    pub letter_spacing: Spacing,
    pub word_spacing: Spacing,

    pub line_height: f32,
    pub text_decoration: TextDecoration,

    // Represents CSS font-feature-settings like `"liga"`, `"smcp=1"`.
    pub font_features: Vec<String>,

    // Variable fonts
    pub font_variations: Vec<(FourCc, f32)>,
    // Multiplier of the space width
    pub tab_size: f32,
    // text-transform
    pub text_transform: TextTransform,
    // Vertical text properties
    pub writing_mode: WritingMode,
    pub text_orientation: TextOrientation,
    // Tate-chu-yoko
    pub text_combine_upright: Option<TextCombineUpright>,

    // Variant handling
    pub font_variant_caps: FontVariantCaps,
    pub font_variant_numeric: FontVariantNumeric,
    pub font_variant_ligatures: FontVariantLigatures,
    pub font_variant_east_asian: FontVariantEastAsian,
}

impl Default for StyleProperties {
    fn default() -> Self {
        const FONT_SIZE: f32 = 16.0;
        const TAB_SIZE: f32 = 8.0;
        Self {
            font_stack: FontStack::default(),
            font_size_px: FONT_SIZE,
            color: ColorU::default(),
            background_color: None,
            background_content: Vec::new(),
            border: None,
            letter_spacing: Spacing::default(), // Px(0)
            word_spacing: Spacing::default(),   // Px(0)
            line_height: FONT_SIZE * 1.2,
            text_decoration: TextDecoration::default(),
            font_features: Vec::new(),
            font_variations: Vec::new(),
            tab_size: TAB_SIZE, // CSS default
            text_transform: TextTransform::default(),
            writing_mode: WritingMode::default(),
            text_orientation: TextOrientation::default(),
            text_combine_upright: None,
            font_variant_caps: FontVariantCaps::default(),
            font_variant_numeric: FontVariantNumeric::default(),
            font_variant_ligatures: FontVariantLigatures::default(),
            font_variant_east_asian: FontVariantEastAsian::default(),
        }
    }
}

impl Hash for StyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.color.hash(state);
        self.background_color.hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);

        // For f32 fields, round and cast to usize before hashing.
        (self.font_size_px.round() as usize).hash(state);
        (self.line_height.round() as usize).hash(state);
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Eq, PartialOrd, Ord)]
pub enum TextCombineUpright {
    None,
    All,        // Combine all characters in horizontal layout
    Digits(u8), // Combine up to N digits
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlyphSource {
    /// Glyph generated from a character in the source text.
    Char,
    /// Glyph inserted dynamically by the layout engine (e.g., a hyphen).
    Hyphen,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CharacterClass {
    Space,       // Regular spaces - highest justification priority
    Punctuation, // Can sometimes be adjusted
    Letter,      // Normal letters
    Ideograph,   // CJK characters - can be justified between
    Symbol,      // Symbols, emojis
    Combining,   // Combining marks - never justified
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphOrientation {
    Horizontal, // Keep horizontal (normal in horizontal text)
    Vertical,   // Rotate to vertical (normal in vertical text)
    Upright,    // Keep upright regardless of writing mode
    Mixed,      // Use script-specific default orientation
}

// Bidi and script detection
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BidiDirection {
    Ltr,
    Rtl,
}

impl BidiDirection {
    pub fn is_rtl(&self) -> bool {
        matches!(self, BidiDirection::Rtl)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantCaps {
    #[default]
    Normal,
    SmallCaps,
    AllSmallCaps,
    PetiteCaps,
    AllPetiteCaps,
    Unicase,
    TitlingCaps,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantNumeric {
    #[default]
    Normal,
    LiningNums,
    OldstyleNums,
    ProportionalNums,
    TabularNums,
    DiagonalFractions,
    StackedFractions,
    Ordinal,
    SlashedZero,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantLigatures {
    #[default]
    Normal,
    None,
    Common,
    NoCommon,
    Discretionary,
    NoDiscretionary,
    Historical,
    NoHistorical,
    Contextual,
    NoContextual,
}

#[derive(Debug, Clone, Copy, PartialEq, Hash, Eq, PartialOrd, Ord, Default)]
pub enum FontVariantEastAsian {
    #[default]
    Normal,
    Jis78,
    Jis83,
    Jis90,
    Jis04,
    Simplified,
    Traditional,
    FullWidth,
    ProportionalWidth,
    Ruby,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BidiLevel(u8);

impl BidiLevel {
    pub fn new(level: u8) -> Self {
        Self(level)
    }
    pub fn is_rtl(&self) -> bool {
        self.0 % 2 == 1
    }
    pub fn level(&self) -> u8 {
        self.0
    }
}

// Add this new struct for style overrides
#[derive(Debug, Clone)]
pub struct StyleOverride {
    /// The specific character this override applies to.
    pub target: ContentIndex,
    /// The style properties to apply.
    /// Any `None` value means "inherit from the base style".
    pub style: PartialStyleProperties,
}

#[derive(Debug, Clone, Default)]
pub struct PartialStyleProperties {
    pub font_stack: Option<FontStack>,
    pub font_size_px: Option<f32>,
    pub color: Option<ColorU>,
    pub letter_spacing: Option<Spacing>,
    pub word_spacing: Option<Spacing>,
    pub line_height: Option<f32>,
    pub text_decoration: Option<TextDecoration>,
    pub font_features: Option<Vec<String>>,
    pub font_variations: Option<Vec<(FourCc, f32)>>,
    pub tab_size: Option<f32>,
    pub text_transform: Option<TextTransform>,
    pub writing_mode: Option<WritingMode>,
    pub text_orientation: Option<TextOrientation>,
    pub text_combine_upright: Option<Option<TextCombineUpright>>,
    pub font_variant_caps: Option<FontVariantCaps>,
    pub font_variant_numeric: Option<FontVariantNumeric>,
    pub font_variant_ligatures: Option<FontVariantLigatures>,
    pub font_variant_east_asian: Option<FontVariantEastAsian>,
}

impl Hash for PartialStyleProperties {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.font_stack.hash(state);
        self.font_size_px.map(|f| f.to_bits()).hash(state);
        self.color.hash(state);
        self.letter_spacing.hash(state);
        self.word_spacing.hash(state);
        self.line_height.map(|f| f.to_bits()).hash(state);
        self.text_decoration.hash(state);
        self.font_features.hash(state);

        // Manual hashing for Vec<(FourCc, f32)>
        self.font_variations.as_ref().map(|v| {
            for (tag, val) in v {
                tag.hash(state);
                val.to_bits().hash(state);
            }
        });

        self.tab_size.map(|f| f.to_bits()).hash(state);
        self.text_transform.hash(state);
        self.writing_mode.hash(state);
        self.text_orientation.hash(state);
        self.text_combine_upright.hash(state);
        self.font_variant_caps.hash(state);
        self.font_variant_numeric.hash(state);
        self.font_variant_ligatures.hash(state);
        self.font_variant_east_asian.hash(state);
    }
}

impl PartialEq for PartialStyleProperties {
    fn eq(&self, other: &Self) -> bool {
        self.font_stack == other.font_stack &&
        self.font_size_px.map(|f| f.to_bits()) == other.font_size_px.map(|f| f.to_bits()) &&
        self.color == other.color &&
        self.letter_spacing == other.letter_spacing &&
        self.word_spacing == other.word_spacing &&
        self.line_height.map(|f| f.to_bits()) == other.line_height.map(|f| f.to_bits()) &&
        self.text_decoration == other.text_decoration &&
        self.font_features == other.font_features &&
        self.font_variations == other.font_variations && // Vec<(FourCc, f32)> is PartialEq
        self.tab_size.map(|f| f.to_bits()) == other.tab_size.map(|f| f.to_bits()) &&
        self.text_transform == other.text_transform &&
        self.writing_mode == other.writing_mode &&
        self.text_orientation == other.text_orientation &&
        self.text_combine_upright == other.text_combine_upright &&
        self.font_variant_caps == other.font_variant_caps &&
        self.font_variant_numeric == other.font_variant_numeric &&
        self.font_variant_ligatures == other.font_variant_ligatures &&
        self.font_variant_east_asian == other.font_variant_east_asian
    }
}

impl Eq for PartialStyleProperties {}

impl StyleProperties {
    fn apply_override(&self, partial: &PartialStyleProperties) -> Self {
        let mut new_style = self.clone();
        if let Some(val) = &partial.font_stack {
            new_style.font_stack = val.clone();
        }
        if let Some(val) = partial.font_size_px {
            new_style.font_size_px = val;
        }
        if let Some(val) = &partial.color {
            new_style.color = val.clone();
        }
        if let Some(val) = partial.letter_spacing {
            new_style.letter_spacing = val;
        }
        if let Some(val) = partial.word_spacing {
            new_style.word_spacing = val;
        }
        if let Some(val) = partial.line_height {
            new_style.line_height = val;
        }
        if let Some(val) = &partial.text_decoration {
            new_style.text_decoration = val.clone();
        }
        if let Some(val) = &partial.font_features {
            new_style.font_features = val.clone();
        }
        if let Some(val) = &partial.font_variations {
            new_style.font_variations = val.clone();
        }
        if let Some(val) = partial.tab_size {
            new_style.tab_size = val;
        }
        if let Some(val) = partial.text_transform {
            new_style.text_transform = val;
        }
        if let Some(val) = partial.writing_mode {
            new_style.writing_mode = val;
        }
        if let Some(val) = partial.text_orientation {
            new_style.text_orientation = val;
        }
        if let Some(val) = &partial.text_combine_upright {
            new_style.text_combine_upright = val.clone();
        }
        if let Some(val) = partial.font_variant_caps {
            new_style.font_variant_caps = val;
        }
        if let Some(val) = partial.font_variant_numeric {
            new_style.font_variant_numeric = val;
        }
        if let Some(val) = partial.font_variant_ligatures {
            new_style.font_variant_ligatures = val;
        }
        if let Some(val) = partial.font_variant_east_asian {
            new_style.font_variant_east_asian = val;
        }
        new_style
    }
}

/// The kind of a glyph, used to distinguish characters from layout-inserted items.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum GlyphKind {
    /// A standard glyph representing one or more characters from the source text.
    Character,
    /// A hyphen glyph inserted by the line breaking algorithm.
    Hyphen,
    /// A `.notdef` glyph, indicating a character that could not be found in any font.
    NotDef,
    /// A Kashida justification glyph, inserted to stretch Arabic text.
    Kashida {
        /// The target width of the kashida.
        width: f32,
    },
}

// --- Stage 1: Logical Representation ---

#[derive(Debug, Clone)]
pub enum LogicalItem {
    Text {
        /// A stable ID pointing back to the original source character.
        source: ContentIndex,
        /// The text of this specific logical item (often a single grapheme cluster).
        text: String,
        style: Arc<StyleProperties>,
        /// If this text is a list marker: whether it should be positioned outside
        /// (in the padding gutter) or inside (inline with content).
        /// None for non-marker content.
        marker_position_outside: Option<bool>,
        /// The DOM NodeId of the Text node this item originated from.
        /// None for generated content (list markers, ::before/::after, etc.)
        source_node_id: Option<NodeId>,
    },
    /// Tate-chu-yoko: Run of text to be laid out horizontally within a vertical context.
    CombinedText {
        source: ContentIndex,
        text: String,
        style: Arc<StyleProperties>,
    },
    Ruby {
        source: ContentIndex,
        // For the stub, we simplify to strings. A full implementation
        // would need to handle Vec<LogicalItem> for both.
        base_text: String,
        ruby_text: String,
        style: Arc<StyleProperties>,
    },
    Object {
        /// A stable ID pointing back to the original source object.
        source: ContentIndex,
        /// The original non-text object.
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        style: Arc<StyleProperties>,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl Hash for LogicalItem {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        discriminant(self).hash(state);
        match self {
            LogicalItem::Text {
                source,
                text,
                style,
                marker_position_outside,
                source_node_id,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state); // Hash the content, not the Arc pointer
                marker_position_outside.hash(state);
                source_node_id.hash(state);
            }
            LogicalItem::CombinedText {
                source,
                text,
                style,
            } => {
                source.hash(state);
                text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                source.hash(state);
                base_text.hash(state);
                ruby_text.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Object { source, content } => {
                source.hash(state);
                content.hash(state);
            }
            LogicalItem::Tab { source, style } => {
                source.hash(state);
                style.as_ref().hash(state);
            }
            LogicalItem::Break { source, break_info } => {
                source.hash(state);
                break_info.hash(state);
            }
        }
    }
}

// --- Stage 2: Visual Representation ---

#[derive(Debug, Clone)]
pub struct VisualItem {
    /// A reference to the logical item this visual item originated from.
    /// A single LogicalItem can be split into multiple VisualItems.
    pub logical_source: LogicalItem,
    /// The Bidi embedding level for this item.
    pub bidi_level: BidiLevel,
    /// The script detected for this run, crucial for shaping.
    pub script: Script,
    /// The text content for this specific visual run.
    pub text: String,
}

// --- Stage 3: Shaped Representation ---

#[derive(Debug, Clone)]
pub enum ShapedItem {
    Cluster(ShapedCluster),
    /// A block of combined text (tate-chu-yoko) that is laid out
    // as a single unbreakable object.
    CombinedBlock {
        source: ContentIndex,
        /// The glyphs to be rendered horizontally within the vertical line.
        glyphs: Vec<ShapedGlyph>,
        bounds: Rect,
        baseline_offset: f32,
    },
    Object {
        source: ContentIndex,
        bounds: Rect,
        baseline_offset: f32,
        // Store original object for rendering
        content: InlineContent,
    },
    Tab {
        source: ContentIndex,
        bounds: Rect,
    },
    Break {
        source: ContentIndex,
        break_info: InlineBreak,
    },
}

impl ShapedItem {
    pub fn as_cluster(&self) -> Option<&ShapedCluster> {
        match self {
            ShapedItem::Cluster(c) => Some(c),
            _ => None,
        }
    }
    /// Returns the bounding box of the item, relative to its own origin.
    ///
    /// The origin of the returned `Rect` is `(0,0)`, representing the top-left corner
    /// of the item's layout space before final positioning. The size represents the
    /// item's total advance (width in horizontal mode) and its line height (ascent + descent).
    pub fn bounds(&self) -> Rect {
        match self {
            ShapedItem::Cluster(cluster) => {
                // The width of a text cluster is its total advance.
                let width = cluster.advance;

                // The height is the sum of its ascent and descent, which defines its line box.
                // We use the existing helper function which correctly calculates this from font
                // metrics.
                let (ascent, descent) = get_item_vertical_metrics(self);
                let height = ascent + descent;

                Rect {
                    x: 0.0,
                    y: 0.0,
                    width,
                    height,
                }
            }
            // For atomic inline items like objects, combined blocks, and tabs,
            // their bounds have already been calculated during the shaping or measurement phase.
            ShapedItem::CombinedBlock { bounds, .. } => *bounds,
            ShapedItem::Object { bounds, .. } => *bounds,
            ShapedItem::Tab { bounds, .. } => *bounds,

            // Breaks are control characters and have no visual geometry.
            ShapedItem::Break { .. } => Rect::default(), // A zero-sized rectangle.
        }
    }
}

/// A group of glyphs that corresponds to one or more source characters (a cluster).
#[derive(Debug, Clone)]
pub struct ShapedCluster {
    /// The original text that this cluster was shaped from.
    /// This is crucial for correct hyphenation.
    pub text: String,
    /// The ID of the grapheme cluster this glyph cluster represents.
    pub source_cluster_id: GraphemeClusterId,
    /// The source `ContentIndex` for mapping back to logical items.
    pub source_content_index: ContentIndex,
    /// The DOM NodeId of the Text node this cluster originated from.
    /// None for generated content (list markers, ::before/::after, etc.)
    pub source_node_id: Option<NodeId>,
    /// The glyphs that make up this cluster.
    pub glyphs: Vec<ShapedGlyph>,
    /// The total advance width (horizontal) or height (vertical) of the cluster.
    pub advance: f32,
    /// The direction of this cluster, inherited from its `VisualItem`.
    pub direction: BidiDirection,
    /// Font style of this cluster
    pub style: Arc<StyleProperties>,
    /// If this cluster is a list marker: whether it should be positioned outside
    /// (in the padding gutter) or inside (inline with content).
    /// None for non-marker content.
    pub marker_position_outside: Option<bool>,
}

/// A single, shaped glyph with its essential metrics.
#[derive(Debug, Clone)]
pub struct ShapedGlyph {
    /// The kind of glyph this is (character, hyphen, etc.).
    pub kind: GlyphKind,
    /// Glyph ID inside of the font
    pub glyph_id: u16,
    /// The byte offset of this glyph's source character(s) within its cluster text.
    pub cluster_offset: u32,
    /// The horizontal advance for this glyph (for horizontal text) - this is the BASE advance
    /// from the font metrics, WITHOUT kerning applied
    pub advance: f32,
    /// The kerning adjustment for this glyph (positive = more space, negative = less space)
    /// This is separate from advance so we can position glyphs absolutely
    pub kerning: f32,
    /// The horizontal offset/bearing for this glyph
    pub offset: Point,
    /// The vertical advance for this glyph (for vertical text).
    pub vertical_advance: f32,
    /// The vertical offset/bearing for this glyph.
    pub vertical_offset: Point,
    pub script: Script,
    pub style: Arc<StyleProperties>,
    /// Hash of the font - use LoadedFonts to look up the actual font when needed
    pub font_hash: u64,
    /// Cached font metrics to avoid font lookup for common operations
    pub font_metrics: LayoutFontMetrics,
}

impl ShapedGlyph {
    pub fn into_glyph_instance<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        let position = if writing_mode.is_advance_horizontal() {
            LogicalPosition {
                x: self.offset.x,
                y: self.offset.y,
            }
        } else {
            LogicalPosition {
                x: self.vertical_offset.x,
                y: self.vertical_offset.y,
            }
        };

        GlyphInstance {
            index: self.glyph_id as u32,
            point: position,
            size,
        }
    }

    /// Convert this ShapedGlyph into a GlyphInstance with an absolute position.
    /// This is used for display list generation where glyphs need their final page coordinates.
    pub fn into_glyph_instance_at<T: ParsedFontTrait>(
        &self,
        writing_mode: WritingMode,
        absolute_position: LogicalPosition,
        loaded_fonts: &LoadedFonts<T>,
    ) -> GlyphInstance {
        let size = loaded_fonts
            .get_by_hash(self.font_hash)
            .and_then(|font| font.get_glyph_size(self.glyph_id, self.style.font_size_px))
            .unwrap_or_default();

        GlyphInstance {
            index: self.glyph_id as u32,
            point: absolute_position,
            size,
        }
    }

    /// Convert this ShapedGlyph into a GlyphInstance with an absolute position.
    /// This version doesn't require fonts - it uses a default size.
    /// Use this when you don't need precise glyph bounds (e.g., display list generation).
    pub fn into_glyph_instance_at_simple(
        &self,
        _writing_mode: WritingMode,
        absolute_position: LogicalPosition,
    ) -> GlyphInstance {
        // Use font metrics to estimate size, or default to zero
        // The actual rendering will use the font directly
        GlyphInstance {
            index: self.glyph_id as u32,
            point: absolute_position,
            size: LogicalSize::default(),
        }
    }
}

// --- Stage 4: Positioned Representation (Final Layout) ---

#[derive(Debug, Clone)]
pub struct PositionedItem {
    pub item: ShapedItem,
    pub position: Point,
    pub line_index: usize,
}

#[derive(Debug, Clone)]
pub struct UnifiedLayout {
    pub items: Vec<PositionedItem>,
    /// Information about content that did not fit.
    pub overflow: OverflowInfo,
}

impl UnifiedLayout {
    /// Calculate the bounding box of all positioned items.
    /// This is computed on-demand rather than cached.
    pub fn bounds(&self) -> Rect {
        if self.items.is_empty() {
            return Rect::default();
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for item in &self.items {
            let item_x = item.position.x;
            let item_y = item.position.y;

            // Get item dimensions
            let item_bounds = item.item.bounds();
            let item_width = item_bounds.width;
            let item_height = item_bounds.height;

            min_x = min_x.min(item_x);
            min_y = min_y.min(item_y);
            max_x = max_x.max(item_x + item_width);
            max_y = max_y.max(item_y + item_height);
        }

        Rect {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
    pub fn last_baseline(&self) -> Option<f32> {
        self.items
            .iter()
            .rev()
            .find_map(|item| get_baseline_for_item(&item.item))
    }

    /// Takes a point relative to the layout's origin and returns the closest
    /// logical cursor position.
    ///
    /// This is the unified hit-testing implementation. The old `hit_test_to_cursor`
    /// method is deprecated in favor of this one.
    pub fn hittest_cursor(&self, point: LogicalPosition) -> Option<TextCursor> {
        if self.items.is_empty() {
            return None;
        }

        // Find the closest cluster vertically and horizontally
        let mut closest_item_idx = 0;
        let mut closest_distance = f32::MAX;

        for (idx, item) in self.items.iter().enumerate() {
            // Only consider cluster items for cursor placement
            if !matches!(item.item, ShapedItem::Cluster(_)) {
                continue;
            }

            let item_bounds = item.item.bounds();
            let item_center_y = item.position.y + item_bounds.height / 2.0;

            // Distance from click position to item center
            let vertical_distance = (point.y - item_center_y).abs();

            // For horizontal distance, check if we're within the cluster bounds
            let horizontal_distance = if point.x < item.position.x {
                item.position.x - point.x
            } else if point.x > item.position.x + item_bounds.width {
                point.x - (item.position.x + item_bounds.width)
            } else {
                0.0 // Inside the cluster horizontally
            };

            // Combined distance (prioritize vertical proximity)
            let distance = vertical_distance * 2.0 + horizontal_distance;

            if distance < closest_distance {
                closest_distance = distance;
                closest_item_idx = idx;
            }
        }

        // Get the closest cluster
        let closest_item = &self.items[closest_item_idx];
        let cluster = match &closest_item.item {
            ShapedItem::Cluster(c) => c,
            // Objects are treated as a single cluster for selection
            ShapedItem::Object { source, .. } | ShapedItem::CombinedBlock { source, .. } => {
                return Some(TextCursor {
                    cluster_id: GraphemeClusterId {
                        source_run: source.run_index,
                        start_byte_in_run: source.item_index,
                    },
                    affinity: if point.x
                        < closest_item.position.x + (closest_item.item.bounds().width / 2.0)
                    {
                        CursorAffinity::Leading
                    } else {
                        CursorAffinity::Trailing
                    },
                });
            }
            _ => return None,
        };

        // Determine affinity based on which half of the cluster was clicked
        let cluster_mid_x = closest_item.position.x + cluster.advance / 2.0;
        let affinity = if point.x < cluster_mid_x {
            CursorAffinity::Leading
        } else {
            CursorAffinity::Trailing
        };

        Some(TextCursor {
            cluster_id: cluster.source_cluster_id,
            affinity,
        })
    }

    /// Given a logical selection range, returns a vector of visual rectangles
    /// that cover the selected text, in the layout's coordinate space.
    pub fn get_selection_rects(&self, range: &SelectionRange) -> Vec<LogicalRect> {
        // 1. Build a map from the logical cluster ID to the visual PositionedItem for fast lookups.
        let mut cluster_map: HashMap<GraphemeClusterId, &PositionedItem> = HashMap::new();
        for item in &self.items {
            if let Some(cluster) = item.item.as_cluster() {
                cluster_map.insert(cluster.source_cluster_id, item);
            }
        }

        // 2. Normalize the range to ensure start always logically precedes end.
        let (start_cursor, end_cursor) = if range.start.cluster_id > range.end.cluster_id
            || (range.start.cluster_id == range.end.cluster_id
                && range.start.affinity > range.end.affinity)
        {
            (range.end, range.start)
        } else {
            (range.start, range.end)
        };

        // 3. Find the positioned items corresponding to the start and end of the selection.
        let Some(start_item) = cluster_map.get(&start_cursor.cluster_id) else {
            return Vec::new();
        };
        let Some(end_item) = cluster_map.get(&end_cursor.cluster_id) else {
            return Vec::new();
        };

        let mut rects = Vec::new();

        // Helper to get the absolute visual X coordinate of a cursor.
        let get_cursor_x = |item: &PositionedItem, affinity: CursorAffinity| -> f32 {
            match affinity {
                CursorAffinity::Leading => item.position.x,
                CursorAffinity::Trailing => item.position.x + get_item_measure(&item.item, false),
            }
        };

        // Helper to get the visual bounding box of all content on a specific line index.
        let get_line_bounds = |line_index: usize| -> Option<LogicalRect> {
            let items_on_line = self.items.iter().filter(|i| i.line_index == line_index);

            let mut min_x: Option<f32> = None;
            let mut max_x: Option<f32> = None;
            let mut min_y: Option<f32> = None;
            let mut max_y: Option<f32> = None;

            for item in items_on_line {
                // Skip items that don't take up space (like hard breaks)
                let item_bounds = item.item.bounds();
                if item_bounds.width <= 0.0 && item_bounds.height <= 0.0 {
                    continue;
                }

                let item_x_end = item.position.x + item_bounds.width;
                let item_y_end = item.position.y + item_bounds.height;

                min_x = Some(min_x.map_or(item.position.x, |mx| mx.min(item.position.x)));
                max_x = Some(max_x.map_or(item_x_end, |mx| mx.max(item_x_end)));
                min_y = Some(min_y.map_or(item.position.y, |my| my.min(item.position.y)));
                max_y = Some(max_y.map_or(item_y_end, |my| my.max(item_y_end)));
            }

            if let (Some(min_x), Some(max_x), Some(min_y), Some(max_y)) =
                (min_x, max_x, min_y, max_y)
            {
                Some(LogicalRect {
                    origin: LogicalPosition { x: min_x, y: min_y },
                    size: LogicalSize {
                        width: max_x - min_x,
                        height: max_y - min_y,
                    },
                })
            } else {
                None
            }
        };

        // 4. Handle single-line selection.
        if start_item.line_index == end_item.line_index {
            if let Some(line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let end_x = get_cursor_x(end_item, end_cursor.affinity);

                // Use min/max and abs to correctly handle selections made from right-to-left.
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: start_x.min(end_x),
                        y: line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: (end_x - start_x).abs(),
                        height: line_bounds.size.height,
                    },
                });
            }
        }
        // 5. Handle multi-line selection.
        else {
            // Rectangle for the start line (from cursor to end of line).
            if let Some(start_line_bounds) = get_line_bounds(start_item.line_index) {
                let start_x = get_cursor_x(start_item, start_cursor.affinity);
                let line_end_x = start_line_bounds.origin.x + start_line_bounds.size.width;
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: start_x,
                        y: start_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: line_end_x - start_x,
                        height: start_line_bounds.size.height,
                    },
                });
            }

            // Rectangles for all full lines in between.
            for line_idx in (start_item.line_index + 1)..end_item.line_index {
                if let Some(line_bounds) = get_line_bounds(line_idx) {
                    rects.push(line_bounds);
                }
            }

            // Rectangle for the end line (from start of line to cursor).
            if let Some(end_line_bounds) = get_line_bounds(end_item.line_index) {
                let line_start_x = end_line_bounds.origin.x;
                let end_x = get_cursor_x(end_item, end_cursor.affinity);
                rects.push(LogicalRect {
                    origin: LogicalPosition {
                        x: line_start_x,
                        y: end_line_bounds.origin.y,
                    },
                    size: LogicalSize {
                        width: end_x - line_start_x,
                        height: end_line_bounds.size.height,
                    },
                });
            }
        }

        rects
    }

    /// Calculates the visual rectangle for a cursor at a given logical position.
    pub fn get_cursor_rect(&self, cursor: &TextCursor) -> Option<LogicalRect> {
        // Find the item and glyph corresponding to the cursor's cluster ID.
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                if cluster.source_cluster_id == cursor.cluster_id {
                    // This is the correct cluster. Now find the position.
                    let line_height = item.item.bounds().height;
                    let cursor_x = match cursor.affinity {
                        CursorAffinity::Leading => item.position.x,
                        CursorAffinity::Trailing => item.position.x + cluster.advance,
                    };
                    return Some(LogicalRect {
                        origin: LogicalPosition {
                            x: cursor_x,
                            y: item.position.y,
                        },
                        size: LogicalSize {
                            width: 1.0,
                            height: line_height,
                        }, // 1px wide cursor
                    });
                }
            }
        }
        None
    }

    /// Get a cursor at the first cluster (leading edge) in the layout.
    pub fn get_first_cluster_cursor(&self) -> Option<TextCursor> {
        for item in &self.items {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                });
            }
        }
        None
    }

    /// Get a cursor at the last cluster (trailing edge) in the layout.
    pub fn get_last_cluster_cursor(&self) -> Option<TextCursor> {
        for item in self.items.iter().rev() {
            if let ShapedItem::Cluster(cluster) = &item.item {
                return Some(TextCursor {
                    cluster_id: cluster.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                });
            }
        }
        None
    }

    /// Moves a cursor one visual unit to the left, handling line wrapping and Bidi text.
    pub fn move_cursor_left(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        // Find current item
        let current_item_pos = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        });

        let Some(current_pos) = current_item_pos else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        // If we're at trailing edge, move to leading edge of same cluster
        if cursor.affinity == CursorAffinity::Trailing {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: moving from trailing to leading edge of byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return TextCursor {
                cluster_id: cursor.cluster_id,
                affinity: CursorAffinity::Leading,
            };
        }

        // We're at leading edge, move to previous cluster's trailing edge
        // Search backwards for a cluster on the same line, or any cluster if at line start
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at leading edge, current line {}",
                current_line
            ));
        }

        // First, try to find previous item on same line
        for i in (0..current_pos).rev() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == current_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_left: found previous cluster on same line, byte \
                             {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Trailing,
                    };
                }
            }
        }

        // If no previous item on same line, try to move to end of previous line
        if current_line > 0 {
            let prev_line = current_line - 1;
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_left: trying previous line {}",
                    prev_line
                ));
            }
            for i in (0..current_pos).rev() {
                if let Some(cluster) = self.items[i].item.as_cluster() {
                    if self.items[i].line_index == prev_line {
                        if let Some(d) = debug {
                            d.push(format!(
                                "[Cursor] move_cursor_left: found cluster on previous line, byte \
                                 {}",
                                cluster.source_cluster_id.start_byte_in_run
                            ));
                        }
                        return TextCursor {
                            cluster_id: cluster.source_cluster_id,
                            affinity: CursorAffinity::Trailing,
                        };
                    }
                }
            }
        }

        // At start of text, can't move further
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_left: at start of text, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor one visual unit to the right.
    pub fn move_cursor_right(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        // Find current item
        let current_item_pos = self.items.iter().position(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        });

        let Some(current_pos) = current_item_pos else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_right: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        // If we're at leading edge, move to trailing edge of same cluster
        if cursor.affinity == CursorAffinity::Leading {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_right: moving from leading to trailing edge of byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return TextCursor {
                cluster_id: cursor.cluster_id,
                affinity: CursorAffinity::Trailing,
            };
        }

        // We're at trailing edge, move to next cluster's leading edge
        let current_line = self.items[current_pos].line_index;

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at trailing edge, current line {}",
                current_line
            ));
        }

        // First, try to find next item on same line
        for i in (current_pos + 1)..self.items.len() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == current_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_right: found next cluster on same line, byte {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
        }

        // If no next item on same line, try to move to start of next line
        let next_line = current_line + 1;
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: trying next line {}",
                next_line
            ));
        }
        for i in (current_pos + 1)..self.items.len() {
            if let Some(cluster) = self.items[i].item.as_cluster() {
                if self.items[i].line_index == next_line {
                    if let Some(d) = debug {
                        d.push(format!(
                            "[Cursor] move_cursor_right: found cluster on next line, byte {}",
                            cluster.source_cluster_id.start_byte_in_run
                        ));
                    }
                    return TextCursor {
                        cluster_id: cluster.source_cluster_id,
                        affinity: CursorAffinity::Leading,
                    };
                }
            }
        }

        // At end of text, can't move further
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_right: at end of text, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor up one line, attempting to preserve the horizontal column.
    pub fn move_cursor_up(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let target_line_idx = current_item.line_index.saturating_sub(1);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_up: already at top line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        // Find the Y coordinate of the middle of the target line
        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map(|i| i.position.y + (i.item.bounds().height / 2.0))
            .unwrap_or(current_item.position.y);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: target line {}, hittesting at ({}, {})",
                target_line_idx, current_x, target_y
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_up: result byte {} (affinity {:?})",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor down one line, attempting to preserve the horizontal column.
    pub fn move_cursor_down(
        &self,
        cursor: TextCursor,
        goal_x: &mut Option<f32>,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: from byte {} (affinity {:?})",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: cursor not found in items, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let max_line = self.items.iter().map(|i| i.line_index).max().unwrap_or(0);
        let target_line_idx = (current_item.line_index + 1).min(max_line);
        if current_item.line_index == target_line_idx {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_down: already at bottom line {}, staying put",
                    current_item.line_index
                ));
            }
            return cursor;
        }

        let current_x = goal_x.unwrap_or_else(|| {
            let x = match cursor.affinity {
                CursorAffinity::Leading => current_item.position.x,
                CursorAffinity::Trailing => {
                    current_item.position.x + get_item_measure(&current_item.item, false)
                }
            };
            *goal_x = Some(x);
            x
        });

        let target_y = self
            .items
            .iter()
            .find(|i| i.line_index == target_line_idx)
            .map(|i| i.position.y + (i.item.bounds().height / 2.0))
            .unwrap_or(current_item.position.y);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: hit testing at ({}, {})",
                current_x, target_y
            ));
        }

        let result = self
            .hittest_cursor(LogicalPosition {
                x: current_x,
                y: target_y,
            })
            .unwrap_or(cursor);

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_down: result byte {}, affinity {:?}",
                result.cluster_id.start_byte_in_run, result.affinity
            ));
        }

        result
    }

    /// Moves a cursor to the visual start of its current line.
    pub fn move_cursor_to_line_start(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_start: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let first_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .min_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = first_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Leading,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_start: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_start: no first item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }

    /// Moves a cursor to the visual end of its current line.
    pub fn move_cursor_to_line_end(
        &self,
        cursor: TextCursor,
        debug: &mut Option<Vec<String>>,
    ) -> TextCursor {
        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: starting at byte {}, affinity {:?}",
                cursor.cluster_id.start_byte_in_run, cursor.affinity
            ));
        }

        let Some(current_item) = self.items.iter().find(|i| {
            i.item
                .as_cluster()
                .map_or(false, |c| c.source_cluster_id == cursor.cluster_id)
        }) else {
            if let Some(d) = debug {
                d.push(format!(
                    "[Cursor] move_cursor_to_line_end: cursor not found, staying at byte {}",
                    cursor.cluster_id.start_byte_in_run
                ));
            }
            return cursor;
        };

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: current line {}, position ({}, {})",
                current_item.line_index, current_item.position.x, current_item.position.y
            ));
        }

        let last_item_on_line = self
            .items
            .iter()
            .filter(|i| i.line_index == current_item.line_index)
            .max_by(|a, b| {
                a.position
                    .x
                    .partial_cmp(&b.position.x)
                    .unwrap_or(Ordering::Equal)
            });

        if let Some(item) = last_item_on_line {
            if let ShapedItem::Cluster(c) = &item.item {
                let result = TextCursor {
                    cluster_id: c.source_cluster_id,
                    affinity: CursorAffinity::Trailing,
                };
                if let Some(d) = debug {
                    d.push(format!(
                        "[Cursor] move_cursor_to_line_end: result byte {}, affinity {:?}",
                        result.cluster_id.start_byte_in_run, result.affinity
                    ));
                }
                return result;
            }
        }

        if let Some(d) = debug {
            d.push(format!(
                "[Cursor] move_cursor_to_line_end: no last item found, staying at byte {}",
                cursor.cluster_id.start_byte_in_run
            ));
        }
        cursor
    }
}

fn get_baseline_for_item(item: &ShapedItem) -> Option<f32> {
    match item {
        ShapedItem::CombinedBlock {
            baseline_offset, ..
        } => Some(*baseline_offset),
        ShapedItem::Object {
            baseline_offset, ..
        } => Some(*baseline_offset),
        // We have to get the clusters font from the last glyph
        ShapedItem::Cluster(ref cluster) => {
            if let Some(last_glyph) = cluster.glyphs.last() {
                Some(
                    last_glyph
                        .font_metrics
                        .baseline_scaled(last_glyph.style.font_size_px),
                )
            } else {
                None
            }
        }
        ShapedItem::Break { source, break_info } => {
            // Breaks do not contribute to baseline
            None
        }
        ShapedItem::Tab { source, bounds } => {
            // Tabs do not contribute to baseline
            None
        }
    }
}

/// Stores information about content that exceeded the available layout space.
#[derive(Debug, Clone, Default)]
pub struct OverflowInfo {
    /// The items that did not fit within the constraints.
    pub overflow_items: Vec<ShapedItem>,
    /// The total bounds of all content, including overflowing items.
    /// This is useful for `OverflowBehavior::Visible` or `Scroll`.
    pub unclipped_bounds: Rect,
}

impl OverflowInfo {
    pub fn has_overflow(&self) -> bool {
        !self.overflow_items.is_empty()
    }
}

/// Intermediate structure carrying information from the line breaker to the positioner.
#[derive(Debug, Clone)]
pub struct UnifiedLine {
    pub items: Vec<ShapedItem>,
    /// The y-position (for horizontal) or x-position (for vertical) of the line's baseline.
    pub cross_axis_position: f32,
    /// The geometric segments this line must fit into.
    pub constraints: LineConstraints,
    pub is_last: bool,
}

// --- Caching Infrastructure ---

pub type CacheId = u64;

/// Defines a single area for layout, with its own shape and properties.
#[derive(Debug, Clone)]
pub struct LayoutFragment {
    /// A unique identifier for this fragment (e.g., "main-content", "sidebar").
    pub id: String,
    /// The geometric and style constraints for this specific fragment.
    pub constraints: UnifiedConstraints,
}

/// Represents the final layout distributed across multiple fragments.
#[derive(Debug, Clone)]
pub struct FlowLayout {
    /// A map from a fragment's unique ID to the layout it contains.
    pub fragment_layouts: HashMap<String, Arc<UnifiedLayout>>,
    /// Any items that did not fit into the last fragment in the flow chain.
    /// This is useful for pagination or determining if more layout space is needed.
    pub remaining_items: Vec<ShapedItem>,
}

pub struct LayoutCache {
    // Stage 1 Cache: InlineContent -> LogicalItems
    logical_items: HashMap<CacheId, Arc<Vec<LogicalItem>>>,
    // Stage 2 Cache: LogicalItems -> VisualItems
    visual_items: HashMap<CacheId, Arc<Vec<VisualItem>>>,
    // Stage 3 Cache: VisualItems -> ShapedItems (now strongly typed)
    shaped_items: HashMap<CacheId, Arc<Vec<ShapedItem>>>,
    // Stage 4 Cache: ShapedItems + Constraints -> Final Layout (now strongly typed)
    layouts: HashMap<CacheId, Arc<UnifiedLayout>>,
}

impl LayoutCache {
    pub fn new() -> Self {
        Self {
            logical_items: HashMap::new(),
            visual_items: HashMap::new(),
            shaped_items: HashMap::new(),
            layouts: HashMap::new(),
        }
    }

    /// Get a layout from the cache by its ID
    pub fn get_layout(&self, cache_id: &CacheId) -> Option<&Arc<UnifiedLayout>> {
        self.layouts.get(cache_id)
    }

    /// Get all layout cache IDs (for iteration/debugging)
    pub fn get_all_layout_ids(&self) -> Vec<CacheId> {
        self.layouts.keys().copied().collect()
    }
}

impl Default for LayoutCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Key for caching the conversion from `InlineContent` to `LogicalItem`s.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LogicalItemsKey<'a> {
    pub inline_content_hash: u64, // Pre-hash the content for efficiency
    pub default_font_size: u32,   // Affects space widths
    // Add other relevant properties from constraints if they affect this stage
    pub _marker: std::marker::PhantomData<&'a ()>,
}

/// Key for caching the Bidi reordering stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct VisualItemsKey {
    pub logical_items_id: CacheId,
    pub base_direction: BidiDirection,
}

/// Key for caching the shaping stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct ShapedItemsKey {
    pub visual_items_id: CacheId,
    pub style_hash: u64, // Represents a hash of all font/style properties
}

impl ShapedItemsKey {
    pub fn new(visual_items_id: CacheId, visual_items: &[VisualItem]) -> Self {
        let style_hash = {
            let mut hasher = DefaultHasher::new();
            for item in visual_items.iter() {
                // Hash the style from the logical source, as this is what determines the font.
                match &item.logical_source {
                    LogicalItem::Text { style, .. } | LogicalItem::CombinedText { style, .. } => {
                        style.as_ref().hash(&mut hasher);
                    }
                    _ => {}
                }
            }
            hasher.finish()
        };

        Self {
            visual_items_id,
            style_hash,
        }
    }
}

/// Key for the final layout stage.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct LayoutKey {
    pub shaped_items_id: CacheId,
    pub constraints: UnifiedConstraints,
}

/// Helper to create a `CacheId` from any `Hash`able type.
fn calculate_id<T: Hash>(item: &T) -> CacheId {
    let mut hasher = DefaultHasher::new();
    item.hash(&mut hasher);
    hasher.finish()
}

// --- Main Layout Pipeline Implementation ---

impl LayoutCache {
    /// New top-level entry point for flowing layout across multiple regions.
    ///
    /// This function orchestrates the entire layout pipeline, but instead of fitting
    /// content into a single set of constraints, it flows the content through an
    /// ordered sequence of `LayoutFragment`s.
    ///
    /// # CSS Inline Layout Module Level 3: Pipeline Implementation
    ///
    /// This implements the inline formatting context with 5 stages:
    ///
    /// ## Stage 1: Logical Analysis (InlineContent -> LogicalItem)
    /// \u2705 IMPLEMENTED: Parses raw content into logical units
    /// - Handles text runs, inline-blocks, replaced elements
    /// - Applies style overrides at character level
    /// - Implements \u00a7 2.2: Content size contribution calculation
    ///
    /// ## Stage 2: BiDi Reordering (LogicalItem -> VisualItem)
    /// \u2705 IMPLEMENTED: Uses CSS 'direction' property per CSS Writing Modes
    /// - Reorders items for right-to-left text (Arabic, Hebrew)
    /// - Respects containing block direction (not auto-detection)
    /// - Conforms to Unicode BiDi Algorithm (UAX #9)
    ///
    /// ## Stage 3: Shaping (VisualItem -> ShapedItem)
    /// \u2705 IMPLEMENTED: Converts text to glyphs
    /// - Uses HarfBuzz for OpenType shaping
    /// - Handles ligatures, kerning, contextual forms
    /// - Caches shaped results for performance
    ///
    /// ## Stage 4: Text Orientation Transformations
    /// \u26a0\ufe0f PARTIAL: Applies text-orientation for vertical text
    /// - Uses constraints from *first* fragment only
    /// - \u274c TODO: Should re-orient if fragments have different writing modes
    ///
    /// ## Stage 5: Flow Loop (ShapedItem -> PositionedItem)
    /// \u2705 IMPLEMENTED: Breaks lines and positions content
    /// - Calls perform_fragment_layout for each fragment
    /// - Uses BreakCursor to flow content across fragments
    /// - Implements \u00a7 5: Line breaking and hyphenation
    ///
    /// # Missing Features from CSS Inline-3:
    /// - \u00a7 3.3: initial-letter (drop caps)
    /// - \u00a7 4: vertical-align (only baseline supported)
    /// - \u00a7 6: text-box-trim (leading trim)
    /// - \u00a7 7: inline-sizing (aspect-ratio for inline-blocks)
    ///
    /// # Arguments
    /// * `content` - The raw `InlineContent` to be laid out.
    /// * `style_overrides` - Character-level style changes.
    /// * `flow_chain` - An ordered slice of `LayoutFragment` defining the regions (e.g., columns,
    ///   pages) that the content should flow through.
    /// * `font_chain_cache` - Pre-resolved font chains (from FontManager.font_chain_cache)
    /// * `fc_cache` - The fontconfig cache for font lookups
    /// * `loaded_fonts` - Pre-loaded fonts, keyed by FontId
    ///
    /// # Returns
    /// A `FlowLayout` struct containing the positioned items for each fragment that
    /// was filled, and any content that did not fit in the final fragment.
    pub fn layout_flow<T: ParsedFontTrait>(
        &mut self,
        content: &[InlineContent],
        style_overrides: &[StyleOverride],
        flow_chain: &[LayoutFragment],
        font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
        fc_cache: &FcFontCache,
        loaded_fonts: &LoadedFonts<T>,
        debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    ) -> Result<FlowLayout, LayoutError> {
        // --- Stages 1-3: Preparation ---
        // These stages are independent of the final geometry. We perform them once
        // on the entire content block before flowing. Caching is used at each stage.

        // Stage 1: Logical Analysis (InlineContent -> LogicalItem)
        let logical_items_id = calculate_id(&content);
        let logical_items = self
            .logical_items
            .entry(logical_items_id)
            .or_insert_with(|| {
                Arc::new(create_logical_items(
                    content,
                    style_overrides,
                    debug_messages,
                ))
            })
            .clone();

        // Get the first fragment's constraints to extract the CSS direction property.
        // This is used for BiDi reordering in Stage 2.
        let default_constraints = UnifiedConstraints::default();
        let first_constraints = flow_chain
            .first()
            .map(|f| &f.constraints)
            .unwrap_or(&default_constraints);

        // Stage 2: Bidi Reordering (LogicalItem -> VisualItem)
        // Use CSS direction property from constraints instead of auto-detecting from text content.
        // This fixes issues with mixed-direction text (e.g., "Arabic - Latin") where auto-detection
        // would treat the entire paragraph as RTL if the first strong character is Arabic.
        // Per HTML/CSS spec, base direction should come from the 'direction' CSS property,
        // defaulting to LTR if not specified.
        let base_direction = first_constraints.direction.unwrap_or(BidiDirection::Ltr);
        let visual_key = VisualItemsKey {
            logical_items_id,
            base_direction,
        };
        let visual_items_id = calculate_id(&visual_key);
        let visual_items = self
            .visual_items
            .entry(visual_items_id)
            .or_insert_with(|| {
                Arc::new(
                    reorder_logical_items(&logical_items, base_direction, debug_messages).unwrap(),
                )
            })
            .clone();

        // Stage 3: Shaping (VisualItem -> ShapedItem)
        let shaped_key = ShapedItemsKey::new(visual_items_id, &visual_items);
        let shaped_items_id = calculate_id(&shaped_key);
        let shaped_items = match self.shaped_items.get(&shaped_items_id) {
            Some(cached) => cached.clone(),
            None => {
                let items = Arc::new(shape_visual_items(
                    &visual_items,
                    font_chain_cache,
                    fc_cache,
                    loaded_fonts,
                    debug_messages,
                )?);
                self.shaped_items.insert(shaped_items_id, items.clone());
                items
            }
        };

        // --- Stage 4: Apply Vertical Text Transformations ---

        // Note: first_constraints was already extracted above for BiDi reordering (Stage 2).
        // This orients all text based on the constraints of the *first* fragment.
        // A more advanced system could defer orientation until inside the loop if
        // fragments can have different writing modes.
        let oriented_items = apply_text_orientation(shaped_items, first_constraints)?;

        // --- Stage 5: The Flow Loop ---

        let mut fragment_layouts = HashMap::new();
        // The cursor now manages the stream of items for the entire flow.
        let mut cursor = BreakCursor::new(&oriented_items);

        for fragment in flow_chain {
            // Perform layout for this single fragment, consuming items from the cursor.
            let fragment_layout = perform_fragment_layout(
                &mut cursor,
                &logical_items,
                &fragment.constraints,
                debug_messages,
                loaded_fonts,
            )?;

            fragment_layouts.insert(fragment.id.clone(), Arc::new(fragment_layout));
            if cursor.is_done() {
                break; // All content has been laid out.
            }
        }

        Ok(FlowLayout {
            fragment_layouts,
            remaining_items: cursor.drain_remaining(),
        })
    }
}

// --- Stage 1 Implementation ---
pub fn create_logical_items(
    content: &[InlineContent],
    style_overrides: &[StyleOverride],
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Vec<LogicalItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering create_logical_items (Refactored) ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input content length: {}",
            content.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input overrides length: {}",
            style_overrides.len()
        )));
    }

    let mut items = Vec::new();
    let mut style_cache: HashMap<u64, Arc<StyleProperties>> = HashMap::new();

    // 1. Organize overrides for fast lookup per run.
    let mut run_overrides: HashMap<u32, HashMap<u32, &PartialStyleProperties>> = HashMap::new();
    for override_item in style_overrides {
        run_overrides
            .entry(override_item.target.run_index)
            .or_default()
            .insert(override_item.target.item_index, &override_item.style);
    }

    for (run_idx, inline_item) in content.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Processing content run #{}",
                run_idx
            )));
        }

        // Extract marker information if this is a marker
        let marker_position_outside = match inline_item {
            InlineContent::Marker {
                position_outside, ..
            } => Some(*position_outside),
            _ => None,
        };

        match inline_item {
            InlineContent::Text(run) | InlineContent::Marker { run, .. } => {
                let text = &run.text;
                if text.is_empty() {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(
                            "  Run is empty, skipping.".to_string(),
                        ));
                    }
                    continue;
                }
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!("  Run text: '{}'", text)));
                }

                let current_run_overrides = run_overrides.get(&(run_idx as u32));
                let mut boundaries = BTreeSet::new();
                boundaries.insert(0);
                boundaries.insert(text.len());

                // --- Stateful Boundary Generation ---
                let mut scan_cursor = 0;
                while scan_cursor < text.len() {
                    let style_at_cursor = if let Some(partial) =
                        current_run_overrides.and_then(|o| o.get(&(scan_cursor as u32)))
                    {
                        // Create a temporary, full style to check its properties
                        run.style.apply_override(partial)
                    } else {
                        (*run.style).clone()
                    };

                    let current_char = text[scan_cursor..].chars().next().unwrap();

                    // Rule 1: Multi-character features take precedence.
                    if let Some(TextCombineUpright::Digits(max_digits)) =
                        style_at_cursor.text_combine_upright
                    {
                        if max_digits > 0 && current_char.is_ascii_digit() {
                            let digit_chunk: String = text[scan_cursor..]
                                .chars()
                                .take(max_digits as usize)
                                .take_while(|c| c.is_ascii_digit())
                                .collect();

                            let end_of_chunk = scan_cursor + digit_chunk.len();
                            boundaries.insert(scan_cursor);
                            boundaries.insert(end_of_chunk);
                            scan_cursor = end_of_chunk; // Jump past the entire sequence
                            continue;
                        }
                    }

                    // Rule 2: If no multi-char feature, check for a normal single-grapheme
                    // override.
                    if current_run_overrides
                        .and_then(|o| o.get(&(scan_cursor as u32)))
                        .is_some()
                    {
                        let grapheme_len = text[scan_cursor..]
                            .graphemes(true)
                            .next()
                            .unwrap_or("")
                            .len();
                        boundaries.insert(scan_cursor);
                        boundaries.insert(scan_cursor + grapheme_len);
                        scan_cursor += grapheme_len;
                        continue;
                    }

                    // Rule 3: No special features or overrides at this point, just advance one
                    // char.
                    scan_cursor += current_char.len_utf8();
                }

                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  Boundaries: {:?}",
                        boundaries
                    )));
                }

                // --- Chunk Processing ---
                for (start, end) in boundaries.iter().zip(boundaries.iter().skip(1)) {
                    let (start, end) = (*start, *end);
                    if start >= end {
                        continue;
                    }

                    let text_slice = &text[start..end];
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Processing chunk from {} to {}: '{}'",
                            start, end, text_slice
                        )));
                    }

                    let style_to_use = if let Some(partial_style) =
                        current_run_overrides.and_then(|o| o.get(&(start as u32)))
                    {
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "  -> Applying override at byte {}",
                                start
                            )));
                        }
                        let mut hasher = DefaultHasher::new();
                        Arc::as_ptr(&run.style).hash(&mut hasher);
                        partial_style.hash(&mut hasher);
                        style_cache
                            .entry(hasher.finish())
                            .or_insert_with(|| Arc::new(run.style.apply_override(partial_style)))
                            .clone()
                    } else {
                        run.style.clone()
                    };

                    let is_combinable_chunk = if let Some(TextCombineUpright::Digits(max_digits)) =
                        &style_to_use.text_combine_upright
                    {
                        *max_digits > 0
                            && !text_slice.is_empty()
                            && text_slice.chars().all(|c| c.is_ascii_digit())
                            && text_slice.chars().count() <= *max_digits as usize
                    } else {
                        false
                    };

                    if is_combinable_chunk {
                        items.push(LogicalItem::CombinedText {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: text_slice.to_string(),
                            style: style_to_use,
                        });
                    } else {
                        items.push(LogicalItem::Text {
                            source: ContentIndex {
                                run_index: run_idx as u32,
                                item_index: start as u32,
                            },
                            text: text_slice.to_string(),
                            style: style_to_use,
                            marker_position_outside,
                            source_node_id: run.source_node_id,
                        });
                    }
                }
            }
            // Handle explicit line breaks (from white-space: pre or <br>)
            InlineContent::LineBreak(break_info) => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  LineBreak: {:?}",
                        break_info
                    )));
                }
                items.push(LogicalItem::Break {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    break_info: break_info.clone(),
                });
            }
            // Other cases (Image, Shape, Space, Tab, Ruby)
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Run is not text, creating generic LogicalItem.".to_string(),
                    ));
                }
                items.push(LogicalItem::Object {
                    source: ContentIndex {
                        run_index: run_idx as u32,
                        item_index: 0,
                    },
                    content: inline_item.clone(),
                });
            }
        }
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting create_logical_items, created {} items ---",
            items.len()
        )));
    }
    items
}

// --- Stage 2 Implementation ---

pub fn get_base_direction_from_logical(logical_items: &[LogicalItem]) -> BidiDirection {
    let first_strong = logical_items.iter().find_map(|item| {
        if let LogicalItem::Text { text, .. } = item {
            Some(unicode_bidi::get_base_direction(text.as_str()))
        } else {
            None
        }
    });

    match first_strong {
        Some(unicode_bidi::Direction::Rtl) => BidiDirection::Rtl,
        _ => BidiDirection::Ltr,
    }
}

pub fn reorder_logical_items(
    logical_items: &[LogicalItem],
    base_direction: BidiDirection,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<VisualItem>, LayoutError> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering reorder_logical_items ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Input logical items count: {}",
            logical_items.len()
        )));
        msgs.push(LayoutDebugMessage::info(format!(
            "Base direction: {:?}",
            base_direction
        )));
    }

    let mut bidi_str = String::new();
    let mut item_map = Vec::new();
    for (idx, item) in logical_items.iter().enumerate() {
        let text = match item {
            LogicalItem::Text { text, .. } => text.as_str(),
            LogicalItem::CombinedText { text, .. } => text.as_str(),
            _ => "\u{FFFC}",
        };
        let start_byte = bidi_str.len();
        bidi_str.push_str(text);
        for _ in start_byte..bidi_str.len() {
            item_map.push(idx);
        }
    }

    if bidi_str.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Bidi string is empty, returning.".to_string(),
            ));
        }
        return Ok(Vec::new());
    }
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Constructed bidi string: '{}'",
            bidi_str
        )));
    }

    let bidi_level = if base_direction == BidiDirection::Rtl {
        Some(Level::rtl())
    } else {
        Some(Level::ltr())
    };
    let bidi_info = BidiInfo::new(&bidi_str, bidi_level);
    let para = &bidi_info.paragraphs[0];
    let (levels, visual_runs) = bidi_info.visual_runs(para, para.range.clone());

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Bidi visual runs generated:".to_string(),
        ));
        for (i, run_range) in visual_runs.iter().enumerate() {
            let level = levels[run_range.start].number();
            let slice = &bidi_str[run_range.start..run_range.end];
            msgs.push(LayoutDebugMessage::info(format!(
                "  Run {}: range={:?}, level={}, text='{}'",
                i, run_range, level, slice
            )));
        }
    }

    let mut visual_items = Vec::new();
    for run_range in visual_runs {
        let bidi_level = BidiLevel::new(levels[run_range.start].number());
        let mut sub_run_start = run_range.start;

        for i in (run_range.start + 1)..run_range.end {
            if item_map[i] != item_map[sub_run_start] {
                let logical_idx = item_map[sub_run_start];
                let logical_item = &logical_items[logical_idx];
                let text_slice = &bidi_str[sub_run_start..i];
                visual_items.push(VisualItem {
                    logical_source: logical_item.clone(),
                    bidi_level,
                    script: crate::text3::script::detect_script(text_slice)
                        .unwrap_or(Script::Latin),
                    text: text_slice.to_string(),
                });
                sub_run_start = i;
            }
        }

        let logical_idx = item_map[sub_run_start];
        let logical_item = &logical_items[logical_idx];
        let text_slice = &bidi_str[sub_run_start..run_range.end];
        visual_items.push(VisualItem {
            logical_source: logical_item.clone(),
            bidi_level,
            script: crate::text3::script::detect_script(text_slice).unwrap_or(Script::Latin),
            text: text_slice.to_string(),
        });
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "Final visual items produced:".to_string(),
        ));
        for (i, item) in visual_items.iter().enumerate() {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Item {}: level={}, text='{}'",
                i,
                item.bidi_level.level(),
                item.text
            )));
        }
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting reorder_logical_items ---".to_string(),
        ));
    }
    Ok(visual_items)
}

// --- Stage 3 Implementation ---

/// Shape visual items into ShapedItems using pre-loaded fonts.
///
/// This function does NOT load any fonts - all fonts must be pre-loaded and passed in.
/// If a required font is not in `loaded_fonts`, the text will be skipped with a warning.
pub fn shape_visual_items<T: ParsedFontTrait>(
    visual_items: &[VisualItem],
    font_chain_cache: &HashMap<FontChainKey, rust_fontconfig::FontFallbackChain>,
    fc_cache: &FcFontCache,
    loaded_fonts: &LoadedFonts<T>,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> Result<Vec<ShapedItem>, LayoutError> {
    let mut shaped = Vec::new();

    for item in visual_items {
        match &item.logical_source {
            LogicalItem::Text {
                style,
                source,
                marker_position_outside,
                source_node_id,
                ..
            } => {
                let direction = if item.bidi_level.is_rtl() {
                    BidiDirection::Rtl
                } else {
                    BidiDirection::Ltr
                };

                let language = script_to_language(item.script, &item.text);

                // Shape text using either FontRef directly or fontconfig-resolved font
                let shaped_clusters_result: Result<Vec<ShapedCluster>, LayoutError> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for text: '{}'",
                                item.text.chars().take(30).collect::<String>()
                            )));
                        }
                        shape_text_correctly(
                            &item.text,
                            item.script,
                            language,
                            direction,
                            font_ref,
                            style,
                            *source,
                            *source_node_id,
                        )
                    }
                    FontStack::Stack(selectors) => {
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);

                        // Look up pre-resolved font chain
                        let font_chain = match font_chain_cache.get(&cache_key) {
                            Some(chain) => chain,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font chain not pre-resolved for {:?} - text will \
                                         not be rendered",
                                        cache_key.font_families
                                    )));
                                }
                                continue;
                            }
                        };

                        // Use the font chain to resolve which font to use for the first character
                        let first_char = item.text.chars().next().unwrap_or('A');
                        let font_id = match font_chain.resolve_char(fc_cache, first_char) {
                            Some((id, _css_source)) => id,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] No font in chain can render character '{}' \
                                         (U+{:04X})",
                                        first_char, first_char as u32
                                    )));
                                }
                                continue;
                            }
                        };

                        // Look up the pre-loaded font
                        match loaded_fonts.get(&font_id) {
                            Some(font) => {
                                shape_text_correctly(
                                    &item.text,
                                    item.script,
                                    language,
                                    direction,
                                    font,
                                    style,
                                    *source,
                                    *source_node_id,
                                )
                            }
                            None => {
                                if let Some(msgs) = debug_messages {
                                    let truncated_text = item.text.chars().take(50).collect::<String>();
                                    let display_text = if item.text.chars().count() > 50 {
                                        format!("{}...", truncated_text)
                                    } else {
                                        truncated_text
                                    };

                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font {:?} not pre-loaded for text: '{}'",
                                        font_id, display_text
                                    )));
                                }
                                continue;
                            }
                        }
                    }
                };

                let mut shaped_clusters = shaped_clusters_result?;

                // Set marker flag on all clusters if this is a marker
                if let Some(is_outside) = marker_position_outside {
                    for cluster in &mut shaped_clusters {
                        cluster.marker_position_outside = Some(*is_outside);
                    }
                }

                shaped.extend(shaped_clusters.into_iter().map(ShapedItem::Cluster));
            }
            LogicalItem::Tab { source, style } => {
                // TODO: To get the space width accurately, we would need to shape
                // a space character with the current font.
                // For now, we approximate it as a fraction of the font size.
                let space_advance = style.font_size_px * 0.33;
                let tab_width = style.tab_size * space_advance;
                shaped.push(ShapedItem::Tab {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: tab_width,
                        height: 0.0,
                    },
                });
            }
            LogicalItem::Ruby {
                source,
                base_text,
                ruby_text,
                style,
            } => {
                // TODO: Implement Ruby layout. This is a major feature.
                // 1. Recursively call layout for the `base_text` to get its size.
                // 2. Recursively call layout for the `ruby_text` (with a smaller font from
                //    `style`).
                // 3. Position the ruby text bounds above/beside the base text bounds.
                // 4. Create a single `ShapedItem::Object` or `ShapedItem::CombinedBlock` that
                //    represents the combined metric bounds of the group, which will be used for
                //    line breaking and positioning on the main line.
                // For now, create a placeholder object.
                let placeholder_width = base_text.chars().count() as f32 * style.font_size_px * 0.6;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds: Rect {
                        x: 0.0,
                        y: 0.0,
                        width: placeholder_width,
                        height: style.line_height * 1.5,
                    },
                    baseline_offset: 0.0,
                    content: InlineContent::Text(StyledRun {
                        text: base_text.clone(),
                        style: style.clone(),
                        logical_start_byte: 0,
                        source_node_id: None, // Ruby text is generated, not from DOM
                    }),
                });
            }
            LogicalItem::CombinedText {
                style,
                source,
                text,
            } => {
                let language = script_to_language(item.script, &item.text);

                // Shape CombinedText using either FontRef directly or fontconfig-resolved font
                let glyphs: Vec<Glyph> = match &style.font_stack {
                    FontStack::Ref(font_ref) => {
                        // For FontRef, use the font directly without fontconfig
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[TextLayout] Using direct FontRef for CombinedText: '{}'",
                                text.chars().take(30).collect::<String>()
                            )));
                        }
                        font_ref.shape_text(
                            text,
                            item.script,
                            language,
                            BidiDirection::Ltr,
                            style.as_ref(),
                        )?
                    }
                    FontStack::Stack(selectors) => {
                        // Build FontChainKey and resolve through fontconfig
                        let cache_key = FontChainKey::from_selectors(selectors);

                        let font_chain = match font_chain_cache.get(&cache_key) {
                            Some(chain) => chain,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font chain not pre-resolved for CombinedText {:?}",
                                        cache_key.font_families
                                    )));
                                }
                                continue;
                            }
                        };

                        let first_char = text.chars().next().unwrap_or('A');
                        let font_id = match font_chain.resolve_char(fc_cache, first_char) {
                            Some((id, _)) => id,
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] No font for CombinedText char '{}'",
                                        first_char
                                    )));
                                }
                                continue;
                            }
                        };

                        match loaded_fonts.get(&font_id) {
                            Some(font) => {
                                font.shape_text(
                                    text,
                                    item.script,
                                    language,
                                    BidiDirection::Ltr,
                                    style.as_ref(),
                                )?
                            }
                            None => {
                                if let Some(msgs) = debug_messages {
                                    msgs.push(LayoutDebugMessage::warning(format!(
                                        "[TextLayout] Font {:?} not pre-loaded for CombinedText",
                                        font_id
                                    )));
                                }
                                continue;
                            }
                        }
                    }
                };

                let shaped_glyphs = glyphs
                    .into_iter()
                    .map(|g| ShapedGlyph {
                        kind: GlyphKind::Character,
                        glyph_id: g.glyph_id,
                        script: g.script,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics,
                        style: g.style,
                        cluster_offset: 0,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                    })
                    .collect::<Vec<_>>();

                let total_width: f32 = shaped_glyphs.iter().map(|g| g.advance + g.kerning).sum();
                let bounds = Rect {
                    x: 0.0,
                    y: 0.0,
                    width: total_width,
                    height: style.line_height,
                };

                shaped.push(ShapedItem::CombinedBlock {
                    source: *source,
                    glyphs: shaped_glyphs,
                    bounds,
                    baseline_offset: 0.0,
                });
            }
            LogicalItem::Object {
                content, source, ..
            } => {
                let (bounds, baseline) = measure_inline_object(content)?;
                shaped.push(ShapedItem::Object {
                    source: *source,
                    bounds,
                    baseline_offset: baseline,
                    content: content.clone(),
                });
            }
            LogicalItem::Break { source, break_info } => {
                shaped.push(ShapedItem::Break {
                    source: *source,
                    break_info: break_info.clone(),
                });
            }
        }
    }
    Ok(shaped)
}

/// Helper to check if a cluster contains only hanging punctuation.
fn is_hanging_punctuation(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        if c.glyphs.len() == 1 {
            match c.text.as_str() {
                "." | "," | ":" | ";" => true,
                _ => false,
            }
        } else {
            false
        }
    } else {
        false
    }
}

fn shape_text_correctly<T: ParsedFontTrait>(
    text: &str,
    script: Script,
    language: crate::text3::script::Language,
    direction: BidiDirection,
    font: &T, // Changed from &Arc<T>
    style: &Arc<StyleProperties>,
    source_index: ContentIndex,
    source_node_id: Option<NodeId>,
) -> Result<Vec<ShapedCluster>, LayoutError> {
    let glyphs = font.shape_text(text, script, language, direction, style.as_ref())?;

    if glyphs.is_empty() {
        return Ok(Vec::new());
    }

    let mut clusters = Vec::new();

    // Group glyphs by cluster ID from the shaper.
    let mut current_cluster_glyphs = Vec::new();
    let mut cluster_id = glyphs[0].cluster;
    let mut cluster_start_byte_in_text = glyphs[0].logical_byte_index;

    for glyph in glyphs {
        if glyph.cluster != cluster_id {
            // Finalize previous cluster
            let advance = current_cluster_glyphs
                .iter()
                .map(|g: &Glyph| g.advance)
                .sum();

            // Safely extract cluster text - handle cases where byte indices may be out of order
            // (can happen with RTL text or complex GSUB reordering)
            let (start, end) = if cluster_start_byte_in_text <= glyph.logical_byte_index {
                (cluster_start_byte_in_text, glyph.logical_byte_index)
            } else {
                (glyph.logical_byte_index, cluster_start_byte_in_text)
            };
            let cluster_text = text.get(start..end).unwrap_or("");

            clusters.push(ShapedCluster {
                text: cluster_text.to_string(), // Store original text for hyphenation
                source_cluster_id: GraphemeClusterId {
                    source_run: source_index.run_index,
                    start_byte_in_run: cluster_id,
                },
                source_content_index: source_index,
                source_node_id,
                glyphs: current_cluster_glyphs
                    .iter()
                    .map(|g| {
                        let source_char = text
                            .get(g.logical_byte_index..)
                            .and_then(|s| s.chars().next())
                            .unwrap_or('\u{FFFD}');
                        // Calculate cluster_offset safely
                        let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                            (g.logical_byte_index - cluster_start_byte_in_text) as u32
                        } else {
                            0
                        };
                        ShapedGlyph {
                            kind: if g.glyph_id == 0 {
                                GlyphKind::NotDef
                            } else {
                                GlyphKind::Character
                            },
                            glyph_id: g.glyph_id,
                            script: g.script,
                            font_hash: g.font_hash,
                            font_metrics: g.font_metrics.clone(),
                            style: g.style.clone(),
                            cluster_offset,
                            advance: g.advance,
                            kerning: g.kerning,
                            vertical_advance: g.vertical_advance,
                            vertical_offset: g.vertical_bearing,
                            offset: g.offset,
                        }
                    })
                    .collect(),
                advance,
                direction,
                style: style.clone(),
                marker_position_outside: None,
            });
            current_cluster_glyphs.clear();
            cluster_id = glyph.cluster;
            cluster_start_byte_in_text = glyph.logical_byte_index;
        }
        current_cluster_glyphs.push(glyph);
    }

    // Finalize the last cluster
    if !current_cluster_glyphs.is_empty() {
        let advance = current_cluster_glyphs
            .iter()
            .map(|g: &Glyph| g.advance)
            .sum();
        let cluster_text = text.get(cluster_start_byte_in_text..).unwrap_or("");
        clusters.push(ShapedCluster {
            text: cluster_text.to_string(), // Store original text
            source_cluster_id: GraphemeClusterId {
                source_run: source_index.run_index,
                start_byte_in_run: cluster_id,
            },
            source_content_index: source_index,
            source_node_id,
            glyphs: current_cluster_glyphs
                .iter()
                .map(|g| {
                    let source_char = text
                        .get(g.logical_byte_index..)
                        .and_then(|s| s.chars().next())
                        .unwrap_or('\u{FFFD}');
                    // Calculate cluster_offset safely
                    let cluster_offset = if g.logical_byte_index >= cluster_start_byte_in_text {
                        (g.logical_byte_index - cluster_start_byte_in_text) as u32
                    } else {
                        0
                    };
                    ShapedGlyph {
                        kind: if g.glyph_id == 0 {
                            GlyphKind::NotDef
                        } else {
                            GlyphKind::Character
                        },
                        glyph_id: g.glyph_id,
                        font_hash: g.font_hash,
                        font_metrics: g.font_metrics.clone(),
                        style: g.style.clone(),
                        script: g.script,
                        vertical_advance: g.vertical_advance,
                        vertical_offset: g.vertical_bearing,
                        cluster_offset,
                        advance: g.advance,
                        kerning: g.kerning,
                        offset: g.offset,
                    }
                })
                .collect(),
            advance,
            direction,
            style: style.clone(),
            marker_position_outside: None,
        });
    }

    Ok(clusters)
}

/// Measures a non-text object, returning its bounds and baseline offset.
fn measure_inline_object(item: &InlineContent) -> Result<(Rect, f32), LayoutError> {
    match item {
        InlineContent::Image(img) => {
            let size = img.display_size.unwrap_or(img.intrinsic_size);
            Ok((
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                img.baseline_offset,
            ))
        }
        InlineContent::Shape(shape) => Ok({
            let size = shape.shape_def.get_size();
            (
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: size.width,
                    height: size.height,
                },
                shape.baseline_offset,
            )
        }),
        InlineContent::Space(space) => Ok((
            Rect {
                x: 0.0,
                y: 0.0,
                width: space.width,
                height: 0.0,
            },
            0.0,
        )),
        InlineContent::Marker { .. } => {
            // Markers are treated as text content, not measurable objects
            Err(LayoutError::InvalidText(
                "Marker is text content, not a measurable object".into(),
            ))
        }
        _ => Err(LayoutError::InvalidText("Not a measurable object".into())),
    }
}

// --- Stage 4 Implementation: Vertical Text ---

/// Applies orientation and vertical metrics to glyphs if the writing mode is vertical.
fn apply_text_orientation(
    items: Arc<Vec<ShapedItem>>,
    constraints: &UnifiedConstraints,
) -> Result<Arc<Vec<ShapedItem>>, LayoutError> {
    if !constraints.is_vertical() {
        return Ok(items);
    }

    let mut oriented_items = Vec::with_capacity(items.len());
    let writing_mode = constraints.writing_mode.unwrap_or_default();

    for item in items.iter() {
        match item {
            ShapedItem::Cluster(cluster) => {
                let mut new_cluster = cluster.clone();
                let mut total_vertical_advance = 0.0;

                for glyph in &mut new_cluster.glyphs {
                    // Use the vertical metrics already computed during shaping
                    // If they're zero, use fallback values
                    if glyph.vertical_advance > 0.0 {
                        total_vertical_advance += glyph.vertical_advance;
                    } else {
                        // Fallback: use line height for vertical advance
                        let fallback_advance = cluster.style.line_height;
                        glyph.vertical_advance = fallback_advance;
                        // Center the glyph horizontally as a fallback
                        glyph.vertical_offset = Point {
                            x: -glyph.advance / 2.0,
                            y: 0.0,
                        };
                        total_vertical_advance += fallback_advance;
                    }
                }
                // The cluster's `advance` now represents vertical advance.
                new_cluster.advance = total_vertical_advance;
                oriented_items.push(ShapedItem::Cluster(new_cluster));
            }
            // Non-text objects also need their advance axis swapped.
            ShapedItem::Object {
                source,
                bounds,
                baseline_offset,
                content,
            } => {
                let mut new_bounds = *bounds;
                std::mem::swap(&mut new_bounds.width, &mut new_bounds.height);
                oriented_items.push(ShapedItem::Object {
                    source: *source,
                    bounds: new_bounds,
                    baseline_offset: *baseline_offset,
                    content: content.clone(),
                });
            }
            _ => oriented_items.push(item.clone()),
        }
    }

    Ok(Arc::new(oriented_items))
}

// --- Stage 5 & 6 Implementation: Combined Layout Pass ---
// This section replaces the previous simple line breaking and positioning logic.

/// Gets the ascent (distance from baseline to top) and descent (distance from baseline to bottom)
/// for a single item.
pub fn get_item_vertical_metrics(item: &ShapedItem) -> (f32, f32) {
    // (ascent, descent)
    match item {
        ShapedItem::Cluster(c) => {
            if c.glyphs.is_empty() {
                // For an empty text cluster, use the line height from its style as a fallback.
                return (c.style.line_height, 0.0);
            }
            // CORRECTED: Iterate through ALL glyphs in the cluster to find the true max
            // ascent/descent.
            c.glyphs
                .iter()
                .fold((0.0f32, 0.0f32), |(max_asc, max_desc), glyph| {
                    let metrics = &glyph.font_metrics;
                    if metrics.units_per_em == 0 {
                        return (max_asc, max_desc);
                    }
                    let scale = glyph.style.font_size_px / metrics.units_per_em as f32;
                    let item_asc = metrics.ascent * scale;
                    // Descent in OpenType is typically negative, so we negate it to get a positive
                    // distance.
                    let item_desc = (-metrics.descent * scale).max(0.0);
                    (max_asc.max(item_asc), max_desc.max(item_desc))
                })
        }
        ShapedItem::Object {
            bounds,
            baseline_offset,
            ..
        } => {
            // Per analysis, `baseline_offset` is the distance from the bottom.
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent.max(0.0), descent.max(0.0))
        }
        ShapedItem::CombinedBlock {
            bounds,
            baseline_offset,
            ..
        } => {
            // CORRECTED: Treat baseline_offset consistently as distance from the bottom (descent).
            let ascent = bounds.height - *baseline_offset;
            let descent = *baseline_offset;
            (ascent.max(0.0), descent.max(0.0))
        }
        _ => (0.0, 0.0), // Breaks and other non-visible items don't affect line height.
    }
}

/// Calculates the maximum ascent and descent for an entire line of items.
/// This determines the "line box" used for vertical alignment.
fn calculate_line_metrics(items: &[ShapedItem]) -> (f32, f32) {
    // (max_ascent, max_descent)
    items
        .iter()
        .fold((0.0f32, 0.0f32), |(max_asc, max_desc), item| {
            let (item_asc, item_desc) = get_item_vertical_metrics(item);
            (max_asc.max(item_asc), max_desc.max(item_desc))
        })
}

/// Performs layout for a single fragment, consuming items from a `BreakCursor`.
///
/// This function contains the core line-breaking and positioning logic, but is
/// designed to operate on a portion of a larger content stream and within the
/// constraints of a single geometric area (a fragment).
///
/// The loop terminates when either the fragment is filled (e.g., runs out of
/// vertical space) or the content stream managed by the `cursor` is exhausted.
///
/// # CSS Inline Layout Module Level 3 Implementation
///
/// This function implements the inline formatting context as described in:
/// https://www.w3.org/TR/css-inline-3/#inline-formatting-context
///
/// ## § 2.1 Layout of Line Boxes
/// "In general, the line-left edge of a line box touches the line-left edge of its
/// containing block and the line-right edge touches the line-right edge of its
/// containing block, and thus the logical width of a line box is equal to the inner
/// logical width of its containing block."
///
/// [ISSUE] available_width should be set to the containing block's inner width,
/// but is currently defaulting to 0.0 in UnifiedConstraints::default().
/// This causes premature line breaking.
///
/// ## § 2.2 Layout Within Line Boxes
/// The layout process follows these steps:
/// 1. Baseline Alignment: All inline-level boxes are aligned by their baselines
/// 2. Content Size Contribution: Calculate layout bounds for each box
/// 3. Line Box Sizing: Size line box to fit aligned layout bounds
/// 4. Content Positioning: Position boxes within the line box
///
/// ## Missing Features:
/// - § 3 Baselines and Alignment Metrics: Only basic baseline alignment implemented
/// - § 4 Baseline Alignment: vertical-align property not fully supported
/// - § 5 Line Spacing: line-height implemented, but line-fit-edge missing
/// - § 6 Trimming Leading: text-box-trim not implemented
pub fn perform_fragment_layout<T: ParsedFontTrait>(
    cursor: &mut BreakCursor,
    logical_items: &[LogicalItem],
    fragment_constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Result<UnifiedLayout, LayoutError> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering perform_fragment_layout ---".to_string(),
        ));
        msgs.push(LayoutDebugMessage::info(format!(
            "Constraints: available_width={:?}, available_height={:?}, columns={}, text_wrap={:?}",
            fragment_constraints.available_width,
            fragment_constraints.available_height,
            fragment_constraints.columns,
            fragment_constraints.text_wrap
        )));
    }

    // For TextWrap::Balance, use Knuth-Plass algorithm for optimal line breaking
    // This produces more visually balanced lines at the cost of more computation
    if fragment_constraints.text_wrap == TextWrap::Balance {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "Using Knuth-Plass algorithm for text-wrap: balance".to_string(),
            ));
        }

        // Get the shaped items from the cursor
        let shaped_items: Vec<ShapedItem> = cursor.drain_remaining();

        let hyphenator = if fragment_constraints.hyphenation {
            fragment_constraints
                .hyphenation_language
                .and_then(|lang| get_hyphenator(lang).ok())
        } else {
            None
        };

        // Use the Knuth-Plass algorithm for optimal line breaking
        return crate::text3::knuth_plass::kp_layout(
            &shaped_items,
            logical_items,
            fragment_constraints,
            hyphenator.as_ref(),
            fonts,
        );
    }

    let hyphenator = if fragment_constraints.hyphenation {
        fragment_constraints
            .hyphenation_language
            .and_then(|lang| get_hyphenator(lang).ok())
    } else {
        None
    };

    let mut positioned_items = Vec::new();
    let mut layout_bounds = Rect::default();

    let num_columns = fragment_constraints.columns.max(1);
    let total_column_gap = fragment_constraints.column_gap * (num_columns - 1) as f32;

    // CSS Inline Layout § 2.1: "the logical width of a line box is equal to the inner
    // logical width of its containing block"
    //
    // Handle the different available space modes:
    // - Definite(width): Use the specified width for column calculation
    // - MinContent: Use 0.0 to force line breaks at every opportunity
    // - MaxContent: Use a large value to allow content to expand naturally
    let column_width = match fragment_constraints.available_width {
        AvailableSpace::Definite(width) => (width - total_column_gap) / num_columns as f32,
        AvailableSpace::MinContent => {
            // Min-content: effectively 0 width forces immediate line breaks
            0.0
        }
        AvailableSpace::MaxContent => {
            // Max-content: very large width allows content to expand
            // Using f32::MAX / 2.0 to avoid overflow issues
            f32::MAX / 2.0
        }
    };
    let mut current_column = 0;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Column width calculated: {}",
            column_width
        )));
    }

    // Use the CSS direction from constraints instead of auto-detecting from text
    // This ensures that mixed-direction text (e.g., "مرحبا - Hello") uses the
    // correct paragraph-level direction for alignment purposes
    let base_direction = fragment_constraints.direction.unwrap_or(BidiDirection::Ltr);

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[PFLayout] Base direction: {:?} (from CSS), Text align: {:?}",
            base_direction, fragment_constraints.text_align
        )));
    }

    'column_loop: while current_column < num_columns {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "\n-- Starting Column {} --",
                current_column
            )));
        }
        let column_start_x =
            (column_width + fragment_constraints.column_gap) * current_column as f32;
        let mut line_top_y = 0.0;
        let mut line_index = 0;
        let mut empty_segment_count = 0; // Failsafe counter for infinite loops
        const MAX_EMPTY_SEGMENTS: usize = 1000; // Maximum allowed consecutive empty segments

        while !cursor.is_done() {
            if let Some(max_height) = fragment_constraints.available_height {
                if line_top_y >= max_height {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::info(format!(
                            "  Column full (pen {} >= height {}), breaking to next column.",
                            line_top_y, max_height
                        )));
                    }
                    break;
                }
            }

            if let Some(clamp) = fragment_constraints.line_clamp {
                if line_index >= clamp.get() {
                    break;
                }
            }

            // Create constraints specific to the current column for the line breaker.
            let mut column_constraints = fragment_constraints.clone();
            column_constraints.available_width = AvailableSpace::Definite(column_width);
            let line_constraints = get_line_constraints(
                line_top_y,
                fragment_constraints.line_height,
                &column_constraints,
                debug_messages,
            );

            if line_constraints.segments.is_empty() {
                empty_segment_count += 1;
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "  No available segments at y={}, skipping to next line. (empty count: \
                         {}/{})",
                        line_top_y, empty_segment_count, MAX_EMPTY_SEGMENTS
                    )));
                }

                // Failsafe: If we've skipped too many lines without content, break out
                if empty_segment_count >= MAX_EMPTY_SEGMENTS {
                    if let Some(msgs) = debug_messages {
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  [WARN] Reached maximum empty segment count ({}). Breaking to \
                             prevent infinite loop.",
                            MAX_EMPTY_SEGMENTS
                        )));
                        msgs.push(LayoutDebugMessage::warning(
                            "  This likely means the shape constraints are too restrictive or \
                             positioned incorrectly."
                                .to_string(),
                        ));
                        msgs.push(LayoutDebugMessage::warning(format!(
                            "  Current y={}, shape boundaries might be outside this range.",
                            line_top_y
                        )));
                    }
                    break;
                }

                // Additional check: If we have shapes and are far beyond the expected height,
                // also break to avoid infinite loops
                if !fragment_constraints.shape_boundaries.is_empty() && empty_segment_count > 50 {
                    // Calculate maximum shape height
                    let max_shape_y: f32 = fragment_constraints
                        .shape_boundaries
                        .iter()
                        .map(|shape| {
                            match shape {
                                ShapeBoundary::Circle { center, radius } => center.y + radius,
                                ShapeBoundary::Ellipse { center, radii } => center.y + radii.height,
                                ShapeBoundary::Polygon { points } => {
                                    points.iter().map(|p| p.y).fold(0.0, f32::max)
                                }
                                ShapeBoundary::Rectangle(rect) => rect.y + rect.height,
                                ShapeBoundary::Path { .. } => f32::MAX, // Can't determine for path
                            }
                        })
                        .fold(0.0, f32::max);

                    if line_top_y > max_shape_y + 100.0 {
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "  [INFO] Current y={} is far beyond maximum shape extent y={}. \
                                 Breaking layout.",
                                line_top_y, max_shape_y
                            )));
                            msgs.push(LayoutDebugMessage::info(
                                "  Shape boundaries exist but no segments available - text cannot \
                                 fit in shape."
                                    .to_string(),
                            ));
                        }
                        break;
                    }
                }

                line_top_y += fragment_constraints.line_height;
                continue;
            }

            // Reset counter when we find valid segments
            empty_segment_count = 0;

            // CSS Text Module Level 3 § 5 Line Breaking and Word Boundaries
            // https://www.w3.org/TR/css-text-3/#line-breaking
            // "When an inline box exceeds the logical width of a line box, it is split
            // into several fragments, which are partitioned across multiple line boxes."
            let (mut line_items, was_hyphenated) =
                break_one_line(cursor, &line_constraints, false, hyphenator.as_ref(), fonts);
            if line_items.is_empty() {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "  Break returned no items. Ending column.".to_string(),
                    ));
                }
                break;
            }

            let line_text_before_rev: String = line_items
                .iter()
                .filter_map(|i| i.as_cluster())
                .map(|c| c.text.as_str())
                .collect();
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    // FIX: The log message was misleading. Items are in visual order.
                    "[PFLayout] Line items from breaker (visual order): [{}]",
                    line_text_before_rev
                )));
            }

            let (mut line_pos_items, line_height) = position_one_line(
                line_items,
                &line_constraints,
                line_top_y,
                line_index,
                fragment_constraints.text_align,
                base_direction,
                cursor.is_done() && !was_hyphenated,
                fragment_constraints,
                debug_messages,
                fonts,
            );

            for item in &mut line_pos_items {
                item.position.x += column_start_x;
            }

            line_top_y += line_height.max(fragment_constraints.line_height);
            line_index += 1;
            positioned_items.extend(line_pos_items);
        }
        current_column += 1;
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting perform_fragment_layout, positioned {} items ---",
            positioned_items.len()
        )));
    }

    let layout = UnifiedLayout {
        items: positioned_items,
        overflow: OverflowInfo::default(),
    };

    // Calculate bounds on demand via the bounds() method
    let calculated_bounds = layout.bounds();
    
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Calculated bounds: width={}, height={} ---",
            calculated_bounds.width, calculated_bounds.height
        )));
    }

    Ok(layout)
}

/// Breaks a single line of items to fit within the given geometric constraints,
/// handling multi-segment lines and hyphenation.
/// Break a single line from the current cursor position.
///
/// # CSS Text Module Level 3 \u00a7 5 Line Breaking and Word Boundaries
/// https://www.w3.org/TR/css-text-3/#line-breaking
///
/// Implements the line breaking algorithm:
/// 1. "When an inline box exceeds the logical width of a line box, it is split into several
///    fragments, which are partitioned across multiple line boxes."
///
/// ## \u2705 Implemented Features:
/// - **Break Opportunities**: Identifies word boundaries and break points
/// - **Soft Wraps**: Wraps at spaces between words
/// - **Hard Breaks**: Handles explicit line breaks (\\n)
/// - **Overflow**: If a word is too long, places it anyway to avoid infinite loop
/// - **Hyphenation**: Tries to break long words at hyphenation points (\u00a7 5.4)
///
/// ## \u26a0\ufe0f Known Issues:
/// - If `line_constraints.total_available` is 0.0 (from `available_width: 0.0` bug), every word
///   will overflow, causing single-word lines
/// - This is the symptom visible in the PDF: "List items break extremely early"
///
/// ## \u00a7 5.2 Breaking Rules for Letters
/// \u2705 IMPLEMENTED: Uses Unicode line breaking algorithm
/// - Relies on UAX #14 for break opportunities
/// - Respects non-breaking spaces and zero-width joiners
///
/// ## \u00a7 5.3 Breaking Rules for Punctuation
/// \u26a0\ufe0f PARTIAL: Basic punctuation handling
/// - \u274c TODO: hanging-punctuation is declared in UnifiedConstraints but not used here
/// - \u274c TODO: Should implement punctuation trimming at line edges
///
/// ## \u00a7 5.4 Hyphenation
/// \u2705 IMPLEMENTED: Automatic hyphenation with hyphenator library
/// - Tries to hyphenate words that overflow
/// - Inserts hyphen glyph at break point
/// - Carries remainder to next line
///
/// ## \u00a7 5.5 Overflow Wrapping
/// \u2705 IMPLEMENTED: Emergency breaking
/// - If line is empty and word doesn't fit, forces at least one item
/// - Prevents infinite loop
/// - This is "overflow-wrap: break-word" behavior
///
/// # Missing Features:
/// - \u274c word-break property (normal, break-all, keep-all)
/// - \u274c line-break property (auto, loose, normal, strict, anywhere)
/// - \u274c overflow-wrap: anywhere vs break-word distinction
/// - \u274c white-space: break-spaces handling
pub fn break_one_line<T: ParsedFontTrait>(
    cursor: &mut BreakCursor,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    hyphenator: Option<&Standard>,
    fonts: &LoadedFonts<T>,
) -> (Vec<ShapedItem>, bool) {
    let mut line_items = Vec::new();
    let mut current_width = 0.0;

    if cursor.is_done() {
        return (Vec::new(), false);
    }

    // CSS Text Module Level 3 § 4.1.1: At the beginning of a line, white space
    // is collapsed away. Skip leading whitespace at line start.
    // https://www.w3.org/TR/css-text-3/#white-space-phase-2
    while !cursor.is_done() {
        let next_unit = cursor.peek_next_unit();
        if next_unit.is_empty() {
            break;
        }
        // Check if the first item is whitespace-only
        if next_unit.len() == 1 && is_word_separator(&next_unit[0]) {
            // Skip this whitespace at line start
            cursor.consume(1);
        } else {
            break;
        }
    }

    loop {
        // 1. Identify the next unbreakable unit (word) or break opportunity.
        let next_unit = cursor.peek_next_unit();
        if next_unit.is_empty() {
            break; // End of content
        }

        // Handle hard breaks immediately.
        if let Some(ShapedItem::Break { .. }) = next_unit.first() {
            line_items.push(next_unit[0].clone());
            cursor.consume(1);
            return (line_items, false);
        }

        let unit_width: f32 = next_unit
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();
        let available_width = line_constraints.total_available - current_width;

        // 2. Can the whole unit fit on the current line?
        if unit_width <= available_width {
            line_items.extend_from_slice(&next_unit);
            current_width += unit_width;
            cursor.consume(next_unit.len());
        } else {
            // 3. The unit overflows. Can we hyphenate it?
            if let Some(hyphenator) = hyphenator {
                // We only try to hyphenate if the unit is a word (not a space).
                if !is_break_opportunity(next_unit.last().unwrap()) {
                    if let Some(hyphenation_result) = try_hyphenate_word_cluster(
                        &next_unit,
                        available_width,
                        is_vertical,
                        hyphenator,
                        fonts,
                    ) {
                        line_items.extend(hyphenation_result.line_part);
                        // Consume the original full word from the cursor.
                        cursor.consume(next_unit.len());
                        // Put the remainder back for the next line.
                        cursor.partial_remainder = hyphenation_result.remainder_part;
                        return (line_items, true);
                    }
                }
            }

            // 4. Cannot hyphenate or fit. The line is finished.
            // If the line is empty, we must force at least one item to avoid an infinite loop.
            if line_items.is_empty() {
                line_items.push(next_unit[0].clone());
                cursor.consume(1);
            }
            break;
        }
    }

    (line_items, false)
}

/// Represents a single valid hyphenation point within a word.
#[derive(Clone)]
pub struct HyphenationBreak {
    /// The number of characters from the original word string included on the line.
    pub char_len_on_line: usize,
    /// The total advance width of the line part + the hyphen.
    pub width_on_line: f32,
    /// The cluster(s) that will remain on the current line.
    pub line_part: Vec<ShapedItem>,
    /// The cluster that represents the hyphen character itself.
    pub hyphen_item: ShapedItem,
    /// The cluster(s) that will be carried over to the next line.
    /// CRITICAL FIX: Changed from ShapedItem to Vec<ShapedItem>
    pub remainder_part: Vec<ShapedItem>,
}

/// A "word" is defined as a sequence of one or more adjacent ShapedClusters.
pub fn find_all_hyphenation_breaks<T: ParsedFontTrait>(
    word_clusters: &[ShapedCluster],
    hyphenator: &Standard,
    is_vertical: bool, // Pass this in to use correct metrics
    fonts: &LoadedFonts<T>,
) -> Option<Vec<HyphenationBreak>> {
    if word_clusters.is_empty() {
        return None;
    }

    // --- 1. Concatenate the TRUE text and build a robust map ---
    let mut word_string = String::new();
    let mut char_map = Vec::new();
    let mut current_width = 0.0;

    for (cluster_idx, cluster) in word_clusters.iter().enumerate() {
        for (char_byte_offset, _ch) in cluster.text.char_indices() {
            let glyph_idx = cluster
                .glyphs
                .iter()
                .rposition(|g| g.cluster_offset as usize <= char_byte_offset)
                .unwrap_or(0);
            let glyph = &cluster.glyphs[glyph_idx];

            let num_chars_in_glyph = cluster.text[glyph.cluster_offset as usize..]
                .chars()
                .count();
            let advance_per_char = if is_vertical {
                glyph.vertical_advance
            } else {
                glyph.advance
            } / (num_chars_in_glyph as f32).max(1.0);

            current_width += advance_per_char;
            char_map.push((cluster_idx, glyph_idx, current_width));
        }
        word_string.push_str(&cluster.text);
    }

    // --- 2. Get hyphenation opportunities ---
    let opportunities = hyphenator.hyphenate(&word_string);
    if opportunities.breaks.is_empty() {
        return None;
    }

    let last_cluster = word_clusters.last().unwrap();
    let last_glyph = last_cluster.glyphs.last().unwrap();
    let style = last_cluster.style.clone();

    // Look up font from hash
    let font = fonts.get_by_hash(last_glyph.font_hash)?;
    let (hyphen_glyph_id, hyphen_advance) =
        font.get_hyphen_glyph_and_advance(style.font_size_px)?;

    let mut possible_breaks = Vec::new();

    // --- 3. Generate a HyphenationBreak for each valid opportunity ---
    for &break_char_idx in &opportunities.breaks {
        // The break is *before* the character at this index.
        // So the last character on the line is at `break_char_idx - 1`.
        if break_char_idx == 0 || break_char_idx > char_map.len() {
            continue;
        }

        let (_, _, width_at_break) = char_map[break_char_idx - 1];

        // The line part is all clusters *before* the break index.
        let line_part: Vec<ShapedItem> = word_clusters[..break_char_idx]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        // The remainder is all clusters *from* the break index onward.
        let remainder_part: Vec<ShapedItem> = word_clusters[break_char_idx..]
            .iter()
            .map(|c| ShapedItem::Cluster(c.clone()))
            .collect();

        let hyphen_item = ShapedItem::Cluster(ShapedCluster {
            text: "-".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Hyphen is generated, not from DOM
            glyphs: vec![ShapedGlyph {
                kind: GlyphKind::Hyphen,
                glyph_id: hyphen_glyph_id,
                font_hash: last_glyph.font_hash,
                font_metrics: last_glyph.font_metrics.clone(),
                cluster_offset: 0,
                script: Script::Latin,
                advance: hyphen_advance,
                kerning: 0.0,
                offset: Point::default(),
                style: style.clone(),
                vertical_advance: hyphen_advance,
                vertical_offset: Point::default(),
            }],
            advance: hyphen_advance,
            direction: BidiDirection::Ltr,
            style: style.clone(),
            marker_position_outside: None,
        });

        possible_breaks.push(HyphenationBreak {
            char_len_on_line: break_char_idx,
            width_on_line: width_at_break + hyphen_advance,
            line_part,
            hyphen_item,
            remainder_part,
        });
    }

    Some(possible_breaks)
}

/// Tries to find a hyphenation point within a word, returning the line part and remainder.
fn try_hyphenate_word_cluster<T: ParsedFontTrait>(
    word_items: &[ShapedItem],
    remaining_width: f32,
    is_vertical: bool,
    hyphenator: &Standard,
    fonts: &LoadedFonts<T>,
) -> Option<HyphenationResult> {
    let word_clusters: Vec<ShapedCluster> = word_items
        .iter()
        .filter_map(|item| item.as_cluster().cloned())
        .collect();

    if word_clusters.is_empty() {
        return None;
    }

    let all_breaks = find_all_hyphenation_breaks(&word_clusters, hyphenator, is_vertical, fonts)?;

    if let Some(best_break) = all_breaks
        .into_iter()
        .rfind(|b| b.width_on_line <= remaining_width)
    {
        let mut line_part = best_break.line_part;
        line_part.push(best_break.hyphen_item);

        return Some(HyphenationResult {
            line_part,
            remainder_part: best_break.remainder_part,
        });
    }

    None
}

/// Positions a single line of items, handling alignment and justification within segments.
///
/// This function is architecturally critical for cache safety. It does not mutate the
/// `advance` or `bounds` of the input `ShapedItem`s. Instead, it applies justification
/// spacing by adjusting the drawing pen's position (`main_axis_pen`).
///
/// # Returns
/// A tuple containing the `Vec` of positioned items and the calculated height of the line box.
/// Position items on a single line after breaking.
///
/// # CSS Inline Layout Module Level 3 \u00a7 2.2 Layout Within Line Boxes
/// https://www.w3.org/TR/css-inline-3/#layout-within-line-boxes
///
/// Implements the positioning algorithm:
/// 1. "All inline-level boxes are aligned by their baselines"
/// 2. "Calculate layout bounds for each inline box"
/// 3. "Size the line box to fit the aligned layout bounds"
/// 4. "Position all inline boxes within the line box"
///
/// ## \u2705 Implemented Features:
///
/// ### \u00a7 4 Baseline Alignment (vertical-align)
/// \u26a0\ufe0f PARTIAL IMPLEMENTATION:
/// - \u2705 `baseline`: Aligns box baseline with parent baseline (default)
/// - \u2705 `top`: Aligns top of box with top of line box
/// - \u2705 `middle`: Centers box within line box
/// - \u2705 `bottom`: Aligns bottom of box with bottom of line box
/// - \u274c MISSING: `text-top`, `text-bottom`, `sub`, `super`
/// - \u274c MISSING: `<length>`, `<percentage>` values for custom offset
///
/// ### \u00a7 2.2.1 Text Alignment (text-align)
/// \u2705 IMPLEMENTED:
/// - `left`, `right`, `center`: Physical alignment
/// - `start`, `end`: Logical alignment (respects direction: ltr/rtl)
/// - `justify`: Distributes space between words/characters
/// - `justify-all`: Justifies last line too
///
/// ### \u00a7 7.3 Text Justification (text-justify)
/// \u2705 IMPLEMENTED:
/// - `inter-word`: Adds space between words
/// - `inter-character`: Adds space between characters
/// - `kashida`: Arabic kashida elongation
/// - \u274c MISSING: `distribute` (CJK justification)
///
/// ### CSS Text \u00a7 8.1 Text Indentation (text-indent)
/// \u2705 IMPLEMENTED: First line indentation
///
/// ### CSS Text \u00a7 4.1 Word Spacing (word-spacing)
/// \u2705 IMPLEMENTED: Additional space between words
///
/// ### CSS Text \u00a7 4.2 Letter Spacing (letter-spacing)
/// \u2705 IMPLEMENTED: Additional space between characters
///
/// ## Segment-Aware Layout:
/// \u2705 Handles CSS Shapes and multi-column layouts
/// - Breaks line into segments (for shape boundaries)
/// - Calculates justification per segment
/// - Applies alignment within each segment's bounds
///
/// ## Known Issues:
/// - \u26a0\ufe0f If segment.width is infinite (from intrinsic sizing), sets alignment_offset=0 to
///   avoid infinite positioning. This is correct for measurement but documented for clarity.
/// - The function assumes `line_index == 0` means first line for text-indent. A more robust system
///   would track paragraph boundaries.
///
/// # Missing Features:
/// - \u274c \u00a7 6 Trimming Leading (text-box-trim, text-box-edge)
/// - \u274c \u00a7 3.3 Initial Letters (drop caps)
/// - \u274c Full vertical-align support (sub, super, lengths, percentages)
/// - \u274c white-space: break-spaces alignment behavior
pub fn position_one_line<T: ParsedFontTrait>(
    line_items: Vec<ShapedItem>,
    line_constraints: &LineConstraints,
    line_top_y: f32,
    line_index: usize,
    text_align: TextAlign,
    base_direction: BidiDirection,
    is_last_line: bool,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> (Vec<PositionedItem>, f32) {
    let line_text: String = line_items
        .iter()
        .filter_map(|i| i.as_cluster())
        .map(|c| c.text.as_str())
        .collect();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering position_one_line for line: [{}] ---",
            line_text
        )));
    }
    // NEW: Resolve the final physical alignment here, inside the function.
    let physical_align = match (text_align, base_direction) {
        (TextAlign::Start, BidiDirection::Ltr) => TextAlign::Left,
        (TextAlign::Start, BidiDirection::Rtl) => TextAlign::Right,
        (TextAlign::End, BidiDirection::Ltr) => TextAlign::Right,
        (TextAlign::End, BidiDirection::Rtl) => TextAlign::Left,
        // Physical alignments are returned as-is, regardless of direction.
        (other, _) => other,
    };
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "[Pos1Line] Physical align: {:?}",
            physical_align
        )));
    }

    if line_items.is_empty() {
        return (Vec::new(), 0.0);
    }
    let mut positioned = Vec::new();
    let is_vertical = constraints.is_vertical();

    // The line box is calculated once for all items on the line, regardless of segment.
    let (line_ascent, line_descent) = calculate_line_metrics(&line_items);
    let line_box_height = line_ascent + line_descent;

    // The baseline for the entire line is determined by its tallest item.
    let line_baseline_y = line_top_y + line_ascent;

    // --- Segment-Aware Positioning ---
    let mut item_cursor = 0;
    let is_first_line_of_para = line_index == 0; // Simplified assumption

    for (segment_idx, segment) in line_constraints.segments.iter().enumerate() {
        if item_cursor >= line_items.len() {
            break;
        }

        // 1. Collect all items that fit into the current segment.
        let mut segment_items = Vec::new();
        let mut current_segment_width = 0.0;
        while item_cursor < line_items.len() {
            let item = &line_items[item_cursor];
            let item_measure = get_item_measure(item, is_vertical);
            // Put at least one item in the segment to avoid getting stuck.
            if current_segment_width + item_measure > segment.width && !segment_items.is_empty() {
                break;
            }
            segment_items.push(item.clone());
            current_segment_width += item_measure;
            item_cursor += 1;
        }

        if segment_items.is_empty() {
            continue;
        }

        // 2. Calculate justification spacing *for this segment only*.
        let (extra_word_spacing, extra_char_spacing) = if constraints.text_justify
            != JustifyContent::None
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
            && constraints.text_justify != JustifyContent::Kashida
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![segment.clone()],
                total_available: segment.width,
            };
            calculate_justification_spacing(
                &segment_items,
                &segment_line_constraints,
                constraints.text_justify,
                is_vertical,
            )
        } else {
            (0.0, 0.0)
        };

        // Kashida justification needs to be segment-aware if used.
        let justified_segment_items = if constraints.text_justify == JustifyContent::Kashida
            && (!is_last_line || constraints.text_align == TextAlign::JustifyAll)
        {
            let segment_line_constraints = LineConstraints {
                segments: vec![segment.clone()],
                total_available: segment.width,
            };
            justify_kashida_and_rebuild(
                segment_items,
                &segment_line_constraints,
                is_vertical,
                debug_messages,
                fonts,
            )
        } else {
            segment_items
        };

        // Recalculate width in case kashida changed the item list
        let final_segment_width: f32 = justified_segment_items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        // 3. Calculate alignment offset *within this segment*.
        let remaining_space = segment.width - final_segment_width;

        // Handle MaxContent/indefinite width: when available_width is MaxContent (for intrinsic
        // sizing), segment.width will be f32::MAX / 2.0. Alignment calculations would
        // produce huge offsets. In this case, treat as left-aligned (offset = 0) since
        // we're measuring natural content width. We check for both infinite AND very large
        // values (> 1e30) to catch the MaxContent case.
        let is_indefinite_width = segment.width.is_infinite() || segment.width > 1e30;
        let alignment_offset = if is_indefinite_width {
            0.0 // No alignment offset for indefinite width
        } else {
            match physical_align {
                TextAlign::Center => remaining_space / 2.0,
                TextAlign::Right => remaining_space,
                _ => 0.0, // Left, Justify
            }
        };

        let mut main_axis_pen = segment.start_x + alignment_offset;
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "[Pos1Line] Segment width: {}, Item width: {}, Remaining space: {}, Initial pen: \
                 {}",
                segment.width, final_segment_width, remaining_space, main_axis_pen
            )));
        }

        // Apply text-indent only to the very first segment of the first line.
        if is_first_line_of_para && segment_idx == 0 {
            main_axis_pen += constraints.text_indent;
        }

        // Calculate total marker width for proper outside marker positioning
        // We need to position all marker clusters together in the padding gutter
        let total_marker_width: f32 = justified_segment_items
            .iter()
            .filter_map(|item| {
                if let ShapedItem::Cluster(c) = item {
                    if c.marker_position_outside == Some(true) {
                        return Some(get_item_measure(item, is_vertical));
                    }
                }
                None
            })
            .sum();

        // Track marker pen separately - starts at negative position for outside markers
        let marker_spacing = 4.0; // Small gap between marker and content
        let mut marker_pen = if total_marker_width > 0.0 {
            -(total_marker_width + marker_spacing)
        } else {
            0.0
        };

        // 4. Position the items belonging to this segment.
        //
        // Vertical alignment positioning (CSS vertical-align)
        //
        // Currently, we use `constraints.vertical_align` for ALL items on the line.
        // This is the GLOBAL vertical alignment set on the containing block.
        //
        // KNOWN LIMITATION / TODO:
        //
        // Per-item vertical-align (stored in `InlineImage.alignment`) is NOT used here.
        // According to CSS, each inline element can have its own vertical-align value:
        //   <img style="vertical-align: top"> would align to line top
        //   <img style="vertical-align: middle"> would center in line box
        //   <img style="vertical-align: bottom"> would align to line bottom
        //
        // To fix this, we would need dir_to:
        // 1. Add a helper function `get_item_vertical_align(&item)` that extracts the alignment
        //    from ShapedItem::Object -> InlineContent::Image -> alignment
        // 2. Use that alignment instead of `constraints.vertical_align` for Objects
        //
        // For now, all items use the global alignment which works correctly for
        // text-only content or when all images have the same alignment.
        //
        // Reference: CSS Inline Layout Level 3 § 4 Baseline Alignment
        // https://www.w3.org/TR/css-inline-3/#baseline-alignment
        for item in justified_segment_items {
            let (item_ascent, item_descent) = get_item_vertical_metrics(&item);
            let item_baseline_pos = match constraints.vertical_align {
                VerticalAlign::Top => line_top_y + item_ascent,
                VerticalAlign::Middle => {
                    line_top_y + (line_box_height / 2.0) - ((item_ascent + item_descent) / 2.0)
                        + item_ascent
                }
                VerticalAlign::Bottom => line_top_y + line_box_height - item_descent,
                _ => line_baseline_y, // Baseline
            };

            // Calculate item measure (needed for both positioning and pen advance)
            let item_measure = get_item_measure(&item, is_vertical);

            let position = if is_vertical {
                Point {
                    x: item_baseline_pos - item_ascent,
                    y: main_axis_pen,
                }
            } else {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "[Pos1Line] is_vertical=false, main_axis_pen={}, item_baseline_pos={}, \
                         item_ascent={}",
                        main_axis_pen, item_baseline_pos, item_ascent
                    )));
                }

                // Check if this is an outside marker - if so, position it in the padding gutter
                let x_position = if let ShapedItem::Cluster(cluster) = &item {
                    if cluster.marker_position_outside == Some(true) {
                        // Use marker_pen for sequential marker positioning
                        let marker_width = item_measure;
                        if let Some(msgs) = debug_messages {
                            msgs.push(LayoutDebugMessage::info(format!(
                                "[Pos1Line] Outside marker detected! width={}, positioning at \
                                 marker_pen={}",
                                marker_width, marker_pen
                            )));
                        }
                        let pos = marker_pen;
                        marker_pen += marker_width; // Advance marker pen for next marker cluster
                        pos
                    } else {
                        main_axis_pen
                    }
                } else {
                    main_axis_pen
                };

                Point {
                    y: item_baseline_pos - item_ascent,
                    x: x_position,
                }
            };

            // item_measure is calculated above for marker positioning
            let item_text = item
                .as_cluster()
                .map(|c| c.text.as_str())
                .unwrap_or("[OBJ]");
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(format!(
                    "[Pos1Line] Positioning item '{}' at pen_x={}",
                    item_text, main_axis_pen
                )));
            }
            positioned.push(PositionedItem {
                item: item.clone(),
                position,
                line_index,
            });

            // Outside markers don't advance the pen - they're positioned in the padding gutter
            let is_outside_marker = if let ShapedItem::Cluster(c) = &item {
                c.marker_position_outside == Some(true)
            } else {
                false
            };

            if !is_outside_marker {
                main_axis_pen += item_measure;
            }

            // Apply calculated spacing to the pen (skip for outside markers)
            if !is_outside_marker && extra_char_spacing > 0.0 && can_justify_after(&item) {
                main_axis_pen += extra_char_spacing;
            }
            if let ShapedItem::Cluster(c) = &item {
                if !is_outside_marker {
                    let letter_spacing_px = match c.style.letter_spacing {
                        Spacing::Px(px) => px as f32,
                        Spacing::Em(em) => em * c.style.font_size_px,
                    };
                    main_axis_pen += letter_spacing_px;
                    if is_word_separator(&item) {
                        let word_spacing_px = match c.style.word_spacing {
                            Spacing::Px(px) => px as f32,
                            Spacing::Em(em) => em * c.style.font_size_px,
                        };
                        main_axis_pen += word_spacing_px;
                        main_axis_pen += extra_word_spacing;
                    }
                }
            }
        }
    }

    (positioned, line_box_height)
}

/// Calculates the starting pen offset to achieve the desired text alignment.
fn calculate_alignment_offset(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    align: TextAlign,
    is_vertical: bool,
    constraints: &UnifiedConstraints,
) -> f32 {
    // Simplified to use the first segment for alignment.
    if let Some(segment) = line_constraints.segments.first() {
        let total_width: f32 = items
            .iter()
            .map(|item| get_item_measure(item, is_vertical))
            .sum();

        let available_width = if constraints.segment_alignment == SegmentAlignment::Total {
            line_constraints.total_available
        } else {
            segment.width
        };

        if total_width >= available_width {
            return 0.0; // No alignment needed if line is full or overflows
        }

        let remaining_space = available_width - total_width;

        match align {
            TextAlign::Center => remaining_space / 2.0,
            TextAlign::Right => remaining_space,
            _ => 0.0, // Left, Justify, Start, End
        }
    } else {
        0.0
    }
}

/// Calculates the extra spacing needed for justification without modifying the items.
///
/// This function is pure and does not mutate any state, making it safe to use
/// with cached `ShapedItem` data.
///
/// # Arguments
/// * `items` - A slice of items on the line.
/// * `line_constraints` - The geometric constraints for the line.
/// * `text_justify` - The type of justification to calculate.
/// * `is_vertical` - Whether the layout is vertical.
///
/// # Returns
/// A tuple `(extra_per_word, extra_per_char)` containing the extra space in pixels
/// to add at each word or character justification opportunity.
fn calculate_justification_spacing(
    items: &[ShapedItem],
    line_constraints: &LineConstraints,
    text_justify: JustifyContent,
    is_vertical: bool,
) -> (f32, f32) {
    // (extra_per_word, extra_per_char)
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;

    if total_width >= available_width || available_width <= 0.0 {
        return (0.0, 0.0);
    }

    let extra_space = available_width - total_width;

    match text_justify {
        JustifyContent::InterWord => {
            // Count justification opportunities (spaces).
            let space_count = items.iter().filter(|item| is_word_separator(item)).count();
            if space_count > 0 {
                (extra_space / space_count as f32, 0.0)
            } else {
                (0.0, 0.0) // No spaces to expand, do nothing.
            }
        }
        JustifyContent::InterCharacter | JustifyContent::Distribute => {
            // Count justification opportunities (between non-combining characters).
            let gap_count = items
                .iter()
                .enumerate()
                .filter(|(i, item)| *i < items.len() - 1 && can_justify_after(item))
                .count();
            if gap_count > 0 {
                (0.0, extra_space / gap_count as f32)
            } else {
                (0.0, 0.0) // No gaps to expand, do nothing.
            }
        }
        // Kashida justification modifies the item list and is handled by a separate function.
        _ => (0.0, 0.0),
    }
}

/// Rebuilds a line of items, inserting Kashida glyphs for justification.
///
/// This function is non-mutating with respect to its inputs. It takes ownership of the
/// original items and returns a completely new `Vec`. This is necessary because Kashida
/// justification changes the number of items on the line, and must not modify cached data.
pub fn justify_kashida_and_rebuild<T: ParsedFontTrait>(
    items: Vec<ShapedItem>,
    line_constraints: &LineConstraints,
    is_vertical: bool,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
    fonts: &LoadedFonts<T>,
) -> Vec<ShapedItem> {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(
            "\n--- Entering justify_kashida_and_rebuild ---".to_string(),
        ));
    }
    let total_width: f32 = items
        .iter()
        .map(|item| get_item_measure(item, is_vertical))
        .sum();
    let available_width = line_constraints.total_available;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Total item width: {}, Available width: {}",
            total_width, available_width
        )));
    }

    if total_width >= available_width || available_width <= 0.0 {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No justification needed (line is full or invalid).".to_string(),
            ));
        }
        return items;
    }

    let extra_space = available_width - total_width;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Extra space to fill: {}",
            extra_space
        )));
    }

    let font_info = items.iter().find_map(|item| {
        if let ShapedItem::Cluster(c) = item {
            if let Some(glyph) = c.glyphs.first() {
                if glyph.script == Script::Arabic {
                    // Look up font from hash
                    if let Some(font) = fonts.get_by_hash(glyph.font_hash) {
                        return Some((
                            font.clone(),
                            glyph.font_hash,
                            glyph.font_metrics.clone(),
                            glyph.style.clone(),
                        ));
                    }
                }
            }
        }
        None
    });

    let (font, font_hash, font_metrics, style) = match font_info {
        Some(info) => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(
                    "Found Arabic font for kashida.".to_string(),
                ));
            }
            info
        }
        None => {
            if let Some(msgs) = debug_messages {
                msgs.push(LayoutDebugMessage::info(
                    "No Arabic font found on line. Cannot insert kashidas.".to_string(),
                ));
            }
            return items;
        }
    };

    let (kashida_glyph_id, kashida_advance) =
        match font.get_kashida_glyph_and_advance(style.font_size_px) {
            Some((id, adv)) if adv > 0.0 => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(format!(
                        "Font provides kashida glyph with advance {}",
                        adv
                    )));
                }
                (id, adv)
            }
            _ => {
                if let Some(msgs) = debug_messages {
                    msgs.push(LayoutDebugMessage::info(
                        "Font does not support kashida justification.".to_string(),
                    ));
                }
                return items;
            }
        };

    let opportunity_indices: Vec<usize> = items
        .windows(2)
        .enumerate()
        .filter_map(|(i, window)| {
            if let (ShapedItem::Cluster(cur), ShapedItem::Cluster(next)) = (&window[0], &window[1])
            {
                if is_arabic_cluster(cur)
                    && is_arabic_cluster(next)
                    && !is_word_separator(&window[1])
                {
                    return Some(i + 1);
                }
            }
            None
        })
        .collect();

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Found {} kashida insertion opportunities at indices: {:?}",
            opportunity_indices.len(),
            opportunity_indices
        )));
    }

    if opportunity_indices.is_empty() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(
                "No opportunities found. Exiting.".to_string(),
            ));
        }
        return items;
    }

    let num_kashidas_to_insert = (extra_space / kashida_advance).floor() as usize;
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Calculated number of kashidas to insert: {}",
            num_kashidas_to_insert
        )));
    }

    if num_kashidas_to_insert == 0 {
        return items;
    }

    let kashidas_per_point = num_kashidas_to_insert / opportunity_indices.len();
    let mut remainder = num_kashidas_to_insert % opportunity_indices.len();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Distributing kashidas: {} per point, with {} remainder.",
            kashidas_per_point, remainder
        )));
    }

    let kashida_item = {
        /* ... as before ... */
        let kashida_glyph = ShapedGlyph {
            kind: GlyphKind::Kashida {
                width: kashida_advance,
            },
            glyph_id: kashida_glyph_id,
            font_hash,
            font_metrics: font_metrics.clone(),
            style: style.clone(),
            script: Script::Arabic,
            advance: kashida_advance,
            kerning: 0.0,
            cluster_offset: 0,
            offset: Point::default(),
            vertical_advance: 0.0,
            vertical_offset: Point::default(),
        };
        ShapedItem::Cluster(ShapedCluster {
            text: "\u{0640}".to_string(),
            source_cluster_id: GraphemeClusterId {
                source_run: u32::MAX,
                start_byte_in_run: u32::MAX,
            },
            source_content_index: ContentIndex {
                run_index: u32::MAX,
                item_index: u32::MAX,
            },
            source_node_id: None, // Kashida is generated, not from DOM
            glyphs: vec![kashida_glyph],
            advance: kashida_advance,
            direction: BidiDirection::Ltr,
            style,
            marker_position_outside: None,
        })
    };

    let mut new_items = Vec::with_capacity(items.len() + num_kashidas_to_insert);
    let mut last_copy_idx = 0;
    for &point in &opportunity_indices {
        new_items.extend_from_slice(&items[last_copy_idx..point]);
        let mut num_to_insert = kashidas_per_point;
        if remainder > 0 {
            num_to_insert += 1;
            remainder -= 1;
        }
        for _ in 0..num_to_insert {
            new_items.push(kashida_item.clone());
        }
        last_copy_idx = point;
    }
    new_items.extend_from_slice(&items[last_copy_idx..]);

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "--- Exiting justify_kashida_and_rebuild, new item count: {} ---",
            new_items.len()
        )));
    }
    new_items
}

/// Helper to determine if a cluster belongs to the Arabic script.
fn is_arabic_cluster(cluster: &ShapedCluster) -> bool {
    // A cluster is considered Arabic if its first non-NotDef glyph is from the Arabic script.
    // This is a robust heuristic for mixed-script lines.
    cluster.glyphs.iter().any(|g| g.script == Script::Arabic)
}

/// Helper to identify if an item is a word separator (like a space).
pub fn is_word_separator(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        // A cluster is a word separator if its text is whitespace.
        // This is a simplification; a single glyph might be whitespace.
        c.text.chars().any(|g| g.is_whitespace())
    } else {
        false
    }
}

/// Helper to identify if space can be added after an item.
fn can_justify_after(item: &ShapedItem) -> bool {
    if let ShapedItem::Cluster(c) = item {
        c.text.chars().last().map_or(false, |g| {
            !g.is_whitespace() && classify_character(g as u32) != CharacterClass::Combining
        })
    } else {
        // Can generally justify after inline objects unless they are followed by a break.
        !matches!(item, ShapedItem::Break { .. })
    }
}

/// Classifies a character for layout purposes (e.g., justification behavior).
/// Copied from `mod.rs`.
fn classify_character(codepoint: u32) -> CharacterClass {
    match codepoint {
        0x0020 | 0x00A0 | 0x3000 => CharacterClass::Space,
        0x0021..=0x002F | 0x003A..=0x0040 | 0x005B..=0x0060 | 0x007B..=0x007E => {
            CharacterClass::Punctuation
        }
        0x4E00..=0x9FFF | 0x3400..=0x4DBF => CharacterClass::Ideograph,
        0x0300..=0x036F | 0x1AB0..=0x1AFF => CharacterClass::Combining,
        // Mongolian script range
        0x1800..=0x18AF => CharacterClass::Letter,
        _ => CharacterClass::Letter,
    }
}

/// Helper to get the primary measure (width or height) of a shaped item.
pub fn get_item_measure(item: &ShapedItem, is_vertical: bool) -> f32 {
    match item {
        ShapedItem::Cluster(c) => {
            // Total width = base advance + kerning adjustments
            // Kerning is stored separately in glyphs for inspection, but the total
            // cluster width must include it for correct layout positioning
            let total_kerning: f32 = c.glyphs.iter().map(|g| g.kerning).sum();
            c.advance + total_kerning
        }
        ShapedItem::Object { bounds, .. }
        | ShapedItem::CombinedBlock { bounds, .. }
        | ShapedItem::Tab { bounds, .. } => {
            if is_vertical {
                bounds.height
            } else {
                bounds.width
            }
        }
        ShapedItem::Break { .. } => 0.0,
    }
}

/// Helper to get the final positioned bounds of an item.
fn get_item_bounds(item: &PositionedItem) -> Rect {
    let measure = get_item_measure(&item.item, false); // for simplicity, use horizontal
    let cross_measure = match &item.item {
        ShapedItem::Object { bounds, .. } => bounds.height,
        _ => 20.0, // placeholder line height
    };
    Rect {
        x: item.position.x,
        y: item.position.y,
        width: measure,
        height: cross_measure,
    }
}

/// Calculates the available horizontal segments for a line at a given vertical position,
/// considering both shape boundaries and exclusions.
fn get_line_constraints(
    line_y: f32,
    line_height: f32,
    constraints: &UnifiedConstraints,
    debug_messages: &mut Option<Vec<LayoutDebugMessage>>,
) -> LineConstraints {
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "\n--- Entering get_line_constraints for y={} ---",
            line_y
        )));
    }

    let mut available_segments = Vec::new();
    if constraints.shape_boundaries.is_empty() {
        // The segment_width is determined by available_width, NOT by TextWrap.
        // TextWrap::NoWrap only affects whether the LineBreaker can insert soft breaks,
        // it should NOT override a definite width constraint from CSS.
        // CSS Text Level 3: For 'white-space: pre/nowrap', text overflows horizontally
        // if it doesn't fit, rather than expanding the container.
        let segment_width = match constraints.available_width {
            AvailableSpace::Definite(w) => w, // Respect definite width from CSS
            AvailableSpace::MaxContent => f32::MAX / 2.0, // For intrinsic max-content sizing
            AvailableSpace::MinContent => 0.0, // For intrinsic min-content sizing
        };
        // Note: TextWrap::NoWrap is handled by the LineBreaker in break_one_line()
        // to prevent soft wraps. The text will simply overflow if it exceeds segment_width.
        available_segments.push(LineSegment {
            start_x: 0.0,
            width: segment_width,
            priority: 0,
        });
    } else {
        // ... complex boundary logic ...
    }

    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Initial available segments: {:?}",
            available_segments
        )));
    }

    for (idx, exclusion) in constraints.shape_exclusions.iter().enumerate() {
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "Applying exclusion #{}: {:?}",
                idx, exclusion
            )));
        }
        let exclusion_spans =
            get_shape_horizontal_spans(exclusion, line_y, line_height).unwrap_or_default();
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Exclusion spans at y={}: {:?}",
                line_y, exclusion_spans
            )));
        }

        if exclusion_spans.is_empty() {
            continue;
        }

        let mut next_segments = Vec::new();
        for (excl_start, excl_end) in exclusion_spans {
            for segment in &available_segments {
                let seg_start = segment.start_x;
                let seg_end = segment.start_x + segment.width;

                // Create new segments by subtracting the exclusion
                if seg_end > excl_start && seg_start < excl_end {
                    if seg_start < excl_start {
                        // Left part
                        next_segments.push(LineSegment {
                            start_x: seg_start,
                            width: excl_start - seg_start,
                            priority: segment.priority,
                        });
                    }
                    if seg_end > excl_end {
                        // Right part
                        next_segments.push(LineSegment {
                            start_x: excl_end,
                            width: seg_end - excl_end,
                            priority: segment.priority,
                        });
                    }
                } else {
                    next_segments.push(segment.clone()); // No overlap
                }
            }
            available_segments = merge_segments(next_segments);
            next_segments = Vec::new();
        }
        if let Some(msgs) = debug_messages {
            msgs.push(LayoutDebugMessage::info(format!(
                "  Segments after exclusion #{}: {:?}",
                idx, available_segments
            )));
        }
    }

    let total_width = available_segments.iter().map(|s| s.width).sum();
    if let Some(msgs) = debug_messages {
        msgs.push(LayoutDebugMessage::info(format!(
            "Final segments: {:?}, total available width: {}",
            available_segments, total_width
        )));
        msgs.push(LayoutDebugMessage::info(
            "--- Exiting get_line_constraints ---".to_string(),
        ));
    }

    LineConstraints {
        segments: available_segments,
        total_available: total_width,
    }
}

/// Helper function to get the horizontal spans of any shape at a given y-coordinate.
/// Returns a list of (start_x, end_x) tuples.
fn get_shape_horizontal_spans(
    shape: &ShapeBoundary,
    y: f32,
    line_height: f32,
) -> Result<Vec<(f32, f32)>, LayoutError> {
    match shape {
        ShapeBoundary::Rectangle(rect) => {
            // Check for any overlap between the line box [y, y + line_height]
            // and the rectangle's vertical span [rect.y, rect.y + rect.height].
            let line_start = y;
            let line_end = y + line_height;
            let rect_start = rect.y;
            let rect_end = rect.y + rect.height;

            if line_start < rect_end && line_end > rect_start {
                Ok(vec![(rect.x, rect.x + rect.width)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Circle { center, radius } => {
            let line_center_y = y + line_height / 2.0;
            let dy = (line_center_y - center.y).abs();
            if dy <= *radius {
                let dx = (radius.powi(2) - dy.powi(2)).sqrt();
                Ok(vec![(center.x - dx, center.x + dx)])
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Ellipse { center, radii } => {
            let line_center_y = y + line_height / 2.0;
            let dy = line_center_y - center.y;
            if dy.abs() <= radii.height {
                // Formula: (x-h)^2/a^2 + (y-k)^2/b^2 = 1
                let y_term = dy / radii.height;
                let x_term_squared = 1.0 - y_term.powi(2);
                if x_term_squared >= 0.0 {
                    let dx = radii.width * x_term_squared.sqrt();
                    Ok(vec![(center.x - dx, center.x + dx)])
                } else {
                    Ok(vec![])
                }
            } else {
                Ok(vec![])
            }
        }
        ShapeBoundary::Polygon { points } => {
            let segments = polygon_line_intersection(points, y, line_height)?;
            Ok(segments
                .iter()
                .map(|s| (s.start_x, s.start_x + s.width))
                .collect())
        }
        ShapeBoundary::Path { .. } => Ok(vec![]), // TODO!
    }
}

/// Merges overlapping or adjacent line segments into larger ones.
fn merge_segments(mut segments: Vec<LineSegment>) -> Vec<LineSegment> {
    if segments.len() <= 1 {
        return segments;
    }
    segments.sort_by(|a, b| a.start_x.partial_cmp(&b.start_x).unwrap());
    let mut merged = vec![segments[0].clone()];
    for next_seg in segments.iter().skip(1) {
        let last = merged.last_mut().unwrap();
        if next_seg.start_x <= last.start_x + last.width {
            let new_width = (next_seg.start_x + next_seg.width) - last.start_x;
            last.width = last.width.max(new_width);
        } else {
            merged.push(next_seg.clone());
        }
    }
    merged
}

// TODO: Dummy polygon function to make it compile
fn polygon_line_intersection(
    points: &[Point],
    y: f32,
    line_height: f32,
) -> Result<Vec<LineSegment>, LayoutError> {
    if points.len() < 3 {
        return Ok(vec![]);
    }

    let line_center_y = y + line_height / 2.0;
    let mut intersections = Vec::new();

    // Use winding number algorithm for robustness with complex polygons.
    for i in 0..points.len() {
        let p1 = points[i];
        let p2 = points[(i + 1) % points.len()];

        // Skip horizontal edges as they don't intersect a horizontal scanline in a meaningful way.
        if (p2.y - p1.y).abs() < f32::EPSILON {
            continue;
        }

        // Check if our horizontal scanline at `line_center_y` crosses this polygon edge.
        let crosses = (p1.y <= line_center_y && p2.y > line_center_y)
            || (p1.y > line_center_y && p2.y <= line_center_y);

        if crosses {
            // Calculate intersection x-coordinate using linear interpolation.
            let t = (line_center_y - p1.y) / (p2.y - p1.y);
            let x = p1.x + t * (p2.x - p1.x);
            intersections.push(x);
        }
    }

    // Sort intersections by x-coordinate to form spans.
    intersections.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    // Build segments from paired intersection points.
    let mut segments = Vec::new();
    for chunk in intersections.chunks_exact(2) {
        let start_x = chunk[0];
        let end_x = chunk[1];
        if end_x > start_x {
            segments.push(LineSegment {
                start_x,
                width: end_x - start_x,
                priority: 0,
            });
        }
    }

    Ok(segments)
}

// ADDITION: A helper function to get a hyphenator.
/// Helper to get a hyphenator for a given language.
/// TODO: In a real app, this would be cached.
#[cfg(feature = "text_layout_hyphenation")]
fn get_hyphenator(language: HyphenationLanguage) -> Result<Standard, LayoutError> {
    Standard::from_embedded(language).map_err(|e| LayoutError::HyphenationError(e.to_string()))
}

/// Stub when hyphenation is disabled - always returns an error
#[cfg(not(feature = "text_layout_hyphenation"))]
fn get_hyphenator(_language: Language) -> Result<Standard, LayoutError> {
    Err(LayoutError::HyphenationError("Hyphenation feature not enabled".to_string()))
}

fn is_break_opportunity(item: &ShapedItem) -> bool {
    // Break after spaces or explicit break items.
    if is_word_separator(item) {
        return true;
    }
    if let ShapedItem::Break { .. } = item {
        return true;
    }
    // Also consider soft hyphens as opportunities.
    if let ShapedItem::Cluster(c) = item {
        if c.text.starts_with('\u{00AD}') {
            return true;
        }
    }
    false
}

// A cursor to manage the state of the line breaking process.
// This allows us to handle items that are partially consumed by hyphenation.
pub struct BreakCursor<'a> {
    /// A reference to the complete list of shaped items.
    pub items: &'a [ShapedItem],
    /// The index of the next *full* item to be processed from the `items` slice.
    pub next_item_index: usize,
    /// The remainder of an item that was split by hyphenation on the previous line.
    /// This will be the very first piece of content considered for the next line.
    pub partial_remainder: Vec<ShapedItem>,
}

impl<'a> BreakCursor<'a> {
    pub fn new(items: &'a [ShapedItem]) -> Self {
        Self {
            items,
            next_item_index: 0,
            partial_remainder: Vec::new(),
        }
    }

    /// Checks if the cursor is at the very beginning of the content stream.
    pub fn is_at_start(&self) -> bool {
        self.next_item_index == 0 && self.partial_remainder.is_empty()
    }

    /// Consumes the cursor and returns all remaining items as a `Vec`.
    pub fn drain_remaining(&mut self) -> Vec<ShapedItem> {
        let mut remaining = std::mem::take(&mut self.partial_remainder);
        if self.next_item_index < self.items.len() {
            remaining.extend_from_slice(&self.items[self.next_item_index..]);
        }
        self.next_item_index = self.items.len();
        remaining
    }

    /// Checks if all content, including any partial remainders, has been processed.
    pub fn is_done(&self) -> bool {
        self.next_item_index >= self.items.len() && self.partial_remainder.is_empty()
    }

    /// Consumes a number of items from the cursor's stream.
    pub fn consume(&mut self, count: usize) {
        if count == 0 {
            return;
        }

        let remainder_len = self.partial_remainder.len();
        if count <= remainder_len {
            // Consuming only from the remainder.
            self.partial_remainder.drain(..count);
        } else {
            // Consuming all of the remainder and some from the main list.
            let from_main_list = count - remainder_len;
            self.partial_remainder.clear();
            self.next_item_index += from_main_list;
        }
    }

    /// Looks ahead and returns the next "unbreakable" unit of content.
    /// This is typically a word (a series of non-space clusters) followed by a
    /// space, or just a single space if that's next.
    pub fn peek_next_unit(&self) -> Vec<ShapedItem> {
        let mut unit = Vec::new();
        let mut source_items = self.partial_remainder.clone();
        source_items.extend_from_slice(&self.items[self.next_item_index..]);

        if source_items.is_empty() {
            return unit;
        }

        // If the first item is a break opportunity (like a space), it's a unit on its own.
        if is_break_opportunity(&source_items[0]) {
            unit.push(source_items[0].clone());
            return unit;
        }

        // Otherwise, collect all items until the next break opportunity.
        for item in source_items {
            if is_break_opportunity(&item) {
                break;
            }
            unit.push(item.clone());
        }
        unit
    }
}

// A structured result from a hyphenation attempt.
struct HyphenationResult {
    /// The items that fit on the current line, including the new hyphen.
    line_part: Vec<ShapedItem>,
    /// The remainder of the split item to be carried over to the next line.
    remainder_part: Vec<ShapedItem>,
}

fn perform_bidi_analysis<'a, 'b: 'a>(
    styled_runs: &'a [TextRunInfo],
    full_text: &'b str,
    force_lang: Option<Language>,
) -> Result<(Vec<VisualRun<'a>>, BidiDirection), LayoutError> {
    if full_text.is_empty() {
        return Ok((Vec::new(), BidiDirection::Ltr));
    }

    let bidi_info = BidiInfo::new(full_text, None);
    let para = &bidi_info.paragraphs[0];
    let base_direction = if para.level.is_rtl() {
        BidiDirection::Rtl
    } else {
        BidiDirection::Ltr
    };

    // Create a map from each byte index to its original styled run.
    let mut byte_to_run_index: Vec<usize> = vec![0; full_text.len()];
    for (run_idx, run) in styled_runs.iter().enumerate() {
        let start = run.logical_start;
        let end = start + run.text.len();
        for i in start..end {
            byte_to_run_index[i] = run_idx;
        }
    }

    let mut final_visual_runs = Vec::new();
    let (levels, visual_run_ranges) = bidi_info.visual_runs(para, para.range.clone());

    for range in visual_run_ranges {
        let bidi_level = levels[range.start];
        let mut sub_run_start = range.start;

        // Iterate through the bytes of the visual run to detect style changes.
        for i in (range.start + 1)..range.end {
            if byte_to_run_index[i] != byte_to_run_index[sub_run_start] {
                // Style boundary found. Finalize the previous sub-run.
                let original_run_idx = byte_to_run_index[sub_run_start];
                let script = crate::text3::script::detect_script(&full_text[sub_run_start..i])
                    .unwrap_or(Script::Latin);
                final_visual_runs.push(VisualRun {
                    text_slice: &full_text[sub_run_start..i],
                    style: styled_runs[original_run_idx].style.clone(),
                    logical_start_byte: sub_run_start,
                    bidi_level: BidiLevel::new(bidi_level.number()),
                    language: force_lang.unwrap_or_else(|| {
                        crate::text3::script::script_to_language(
                            script,
                            &full_text[sub_run_start..i],
                        )
                    }),
                    script,
                });
                // Start a new sub-run.
                sub_run_start = i;
            }
        }

        // Add the last sub-run (or the only one if no style change occurred).
        let original_run_idx = byte_to_run_index[sub_run_start];
        let script = crate::text3::script::detect_script(&full_text[sub_run_start..range.end])
            .unwrap_or(Script::Latin);

        final_visual_runs.push(VisualRun {
            text_slice: &full_text[sub_run_start..range.end],
            style: styled_runs[original_run_idx].style.clone(),
            logical_start_byte: sub_run_start,
            bidi_level: BidiLevel::new(bidi_level.number()),
            script,
            language: force_lang.unwrap_or_else(|| {
                crate::text3::script::script_to_language(
                    script,
                    &full_text[sub_run_start..range.end],
                )
            }),
        });
    }

    Ok((final_visual_runs, base_direction))
}

fn get_justification_priority(class: CharacterClass) -> u8 {
    match class {
        CharacterClass::Space => 0,
        CharacterClass::Punctuation => 64,
        CharacterClass::Ideograph => 128,
        CharacterClass::Letter => 192,
        CharacterClass::Symbol => 224,
        CharacterClass::Combining => 255,
    }
}

```

## layout/src/text3/selection.rs
// Text selection logic
// 203 lines

```rust
//! Text selection helper functions
//!
//! Provides word and paragraph selection algorithms.

use azul_core::selection::{CursorAffinity, GraphemeClusterId, SelectionRange, TextCursor};

use crate::text3::cache::{PositionedItem, ShapedCluster, ShapedItem, UnifiedLayout};

/// Select the word at the given cursor position
///
/// Uses Unicode word boundaries to determine word start/end.
/// Returns a SelectionRange covering the entire word.
pub fn select_word_at_cursor(
    cursor: &TextCursor,
    layout: &UnifiedLayout,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, cluster) = find_cluster_at_cursor(cursor, layout)?;

    // Get the text from this cluster and surrounding clusters on the same line
    let line_text = extract_line_text_at_item(item_idx, layout);
    let cursor_byte_offset = cursor.cluster_id.start_byte_in_run as usize;

    // Find word boundaries
    let (word_start, word_end) = find_word_boundaries(&line_text, cursor_byte_offset);

    // Convert byte offsets to cursors
    let start_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: cursor.cluster_id.source_run,
            start_byte_in_run: word_start as u32,
        },
        affinity: CursorAffinity::Leading,
    };

    let end_cursor = TextCursor {
        cluster_id: GraphemeClusterId {
            source_run: cursor.cluster_id.source_run,
            start_byte_in_run: word_end as u32,
        },
        affinity: CursorAffinity::Trailing,
    };

    Some(SelectionRange {
        start: start_cursor,
        end: end_cursor,
    })
}

/// Select the paragraph/line at the given cursor position
///
/// Returns a SelectionRange covering the entire line from the first
/// to the last cluster on that line.
pub fn select_paragraph_at_cursor(
    cursor: &TextCursor,
    layout: &UnifiedLayout,
) -> Option<SelectionRange> {
    // Find the item containing this cursor
    let (item_idx, _) = find_cluster_at_cursor(cursor, layout)?;
    let item = &layout.items[item_idx];
    let line_index = item.line_index;

    // Find all items on this line
    let line_items: Vec<(usize, &PositionedItem)> = layout
        .items
        .iter()
        .enumerate()
        .filter(|(_, item)| item.line_index == line_index)
        .collect();

    if line_items.is_empty() {
        return None;
    }

    // Get first and last cluster on line
    let first_cluster = line_items
        .iter()
        .find_map(|(_, item)| item.item.as_cluster())?;

    let last_cluster = line_items
        .iter()
        .rev()
        .find_map(|(_, item)| item.item.as_cluster())?;

    // Create selection spanning entire line
    Some(SelectionRange {
        start: TextCursor {
            cluster_id: first_cluster.source_cluster_id,
            affinity: CursorAffinity::Leading,
        },
        end: TextCursor {
            cluster_id: last_cluster.source_cluster_id,
            affinity: CursorAffinity::Trailing,
        },
    })
}

// Helper Functions

/// Find the cluster containing the given cursor
fn find_cluster_at_cursor<'a>(
    cursor: &TextCursor,
    layout: &'a UnifiedLayout,
) -> Option<(usize, &'a ShapedCluster)> {
    layout.items.iter().enumerate().find_map(|(idx, item)| {
        if let ShapedItem::Cluster(cluster) = &item.item {
            if cluster.source_cluster_id == cursor.cluster_id {
                return Some((idx, cluster));
            }
        }
        None
    })
}

/// Extract text from all clusters on the same line as the given item
fn extract_line_text_at_item(item_idx: usize, layout: &UnifiedLayout) -> String {
    let line_index = layout.items[item_idx].line_index;

    let mut text = String::new();
    for item in &layout.items {
        if item.line_index != line_index {
            continue;
        }

        if let ShapedItem::Cluster(cluster) = &item.item {
            text.push_str(&cluster.text);
        }
    }

    text
}

/// Find word boundaries around the given byte offset
///
/// Uses a simple algorithm: word characters are alphanumeric or underscore,
/// everything else is a boundary.
fn find_word_boundaries(text: &str, cursor_offset: usize) -> (usize, usize) {
    // Clamp cursor offset to text length
    let cursor_offset = cursor_offset.min(text.len());

    // Find word start (scan backwards)
    let mut word_start = 0;
    let mut char_indices: Vec<(usize, char)> = text.char_indices().collect();

    for (i, (byte_idx, ch)) in char_indices.iter().enumerate().rev() {
        if *byte_idx >= cursor_offset {
            continue;
        }

        if !is_word_char(*ch) {
            // Found boundary, word starts after this char
            word_start = if i + 1 < char_indices.len() {
                char_indices[i + 1].0
            } else {
                text.len()
            };
            break;
        }
    }

    // Find word end (scan forwards)
    let mut word_end = text.len();
    for (byte_idx, ch) in char_indices.iter() {
        if *byte_idx <= cursor_offset {
            continue;
        }

        if !is_word_char(*ch) {
            // Found boundary, word ends before this char
            word_end = *byte_idx;
            break;
        }
    }

    // If cursor is on whitespace, select just that whitespace
    if let Some((_, ch)) = char_indices.iter().find(|(idx, _)| *idx == cursor_offset) {
        if !is_word_char(*ch) {
            // Find span of consecutive whitespace/punctuation
            let start = char_indices
                .iter()
                .rev()
                .find(|(idx, c)| *idx < cursor_offset && is_word_char(*c))
                .map(|(idx, c)| idx + c.len_utf8())
                .unwrap_or(0);

            let end = char_indices
                .iter()
                .find(|(idx, c)| *idx > cursor_offset && is_word_char(*c))
                .map(|(idx, _)| *idx)
                .unwrap_or(text.len());

            return (start, end);
        }
    }

    (word_start, word_end)
}

/// Check if a character is part of a word
#[inline]
fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

```

## layout/src/solve2.rs
// Layout solver - box model, positioning
// 0 lines

```rust
// ERROR reading layout/src/solve2.rs: [Errno 2] No such file or directory: '/Users/fschutt/Development/azul/layout/src/solve2.rs'
```

## layout/src/display_list.rs
// Display list building - borders, backgrounds, scrollbars
// 0 lines

```rust
// ERROR reading layout/src/display_list.rs: [Errno 2] No such file or directory: '/Users/fschutt/Development/azul/layout/src/display_list.rs'
```

## css/src/css_properties.rs
// CSS property definitions
// 0 lines

```rust
// ERROR reading css/src/css_properties.rs: [Errno 2] No such file or directory: '/Users/fschutt/Development/azul/css/src/css_properties.rs'
```


# TASK

Analyze the code and identify the root cause of each bug. Provide specific fixes.

Focus especially on Bug 1 (text input stopped working) since that's a regression from the current diff.

For each bug, provide:
1. Root cause analysis
2. Specific file and line numbers
3. Exact code fix (diff format preferred)

Start with Bug 1 since it's the most critical regression.

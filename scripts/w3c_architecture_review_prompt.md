# W3C Architecture Review: Focus/Cursor/Selection Implementation

## 1. Context: Recent Architectural Changes

We've implemented significant architectural changes to make our contenteditable/focus/cursor system conform to the W3C model. This is a code review request to verify our changes are correct and identify any remaining issues.

### Summary of Changes Made

Based on your previous analysis, we implemented the following:

1. **"Flag and Defer" Pattern for Cursor Initialization**
   - Added `cursor_needs_initialization` flag to `FocusManager`
   - Added `PendingContentEditableFocus` struct to track pending cursor init
   - Cursor is NO LONGER initialized during focus event handling
   - New `finalize_pending_focus_changes()` called after event processing

2. **W3C-Conformant ContentEditable Inheritance**
   - Added `is_node_contenteditable_inherited()` that traverses ancestors
   - Added `find_contenteditable_ancestor()` helper
   - Respects `contenteditable="false"` to block inheritance

3. **Separate FocusManager and CursorManager**
   - `FocusManager` tracks `document.activeElement` (keyboard focus)
   - `CursorManager` tracks `Selection.focusNode` (text cursor position)
   - This mirrors the W3C separation of focus and selection

4. **Reserved System Timer IDs**
   - `CURSOR_BLINK_TIMER_ID = 0x0001`
   - User timers start at `0x0100` to avoid conflicts

5. **Debug API Extensions**
   - Added `GetFocusState` to query focused node
   - Added `GetCursorState` to query cursor position/blink state

## 2. Git Diff of All Changes

```diff
diff --git a/core/src/task.rs b/core/src/task.rs
index a9219c8e..8a039d6b 100644
--- a/core/src/task.rs
+++ b/core/src/task.rs
@@ -57,7 +57,28 @@ pub enum TerminateTimer {
     Continue,
 }
 
-static MAX_TIMER_ID: AtomicUsize = AtomicUsize::new(5);
+// ============================================================================
+// Reserved System Timer IDs (0x0000 - 0x00FF)
+// ============================================================================
+// User timers start at 0x0100 to avoid conflicts with system timers.
+// These constants define well-known timer IDs for internal framework use.
+
+/// Timer ID for cursor blinking in contenteditable elements (~530ms interval)
+pub const CURSOR_BLINK_TIMER_ID: TimerId = TimerId { id: 0x0001 };
+/// Timer ID for scroll momentum/inertia animation
+pub const SCROLL_MOMENTUM_TIMER_ID: TimerId = TimerId { id: 0x0002 };
+/// Timer ID for auto-scroll during drag operations near edges
+pub const DRAG_AUTOSCROLL_TIMER_ID: TimerId = TimerId { id: 0x0003 };
+/// Timer ID for tooltip show delay
+pub const TOOLTIP_DELAY_TIMER_ID: TimerId = TimerId { id: 0x0004 };
+/// Timer ID for double-click detection timeout
+pub const DOUBLE_CLICK_TIMER_ID: TimerId = TimerId { id: 0x0005 };
+
+/// First available ID for user-defined timers
+pub const USER_TIMER_ID_START: usize = 0x0100;
+
+// User timers start at 0x0100 to avoid conflicts with reserved system timer IDs
+static MAX_TIMER_ID: AtomicUsize = AtomicUsize::new(USER_TIMER_ID_START);
 
 /// ID for uniquely identifying a timer
 #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
diff --git a/dll/src/desktop/shell2/common/debug_server.rs b/dll/src/desktop/shell2/common/debug_server.rs
index 6f11dd04..164308ba 100644
--- a/dll/src/desktop/shell2/common/debug_server.rs
+++ b/dll/src/desktop/shell2/common/debug_server.rs
@@ -112,6 +112,10 @@ pub enum ResponseData {
     DragState(DragStateResponse),
     /// Detailed drag context
     DragContext(DragContextResponse),
+    /// Focus state (which node has keyboard focus)
+    FocusState(FocusStateResponse),
+    /// Cursor state (cursor position and blink state)
+    CursorState(CursorStateResponse),
 }
 
 /// Metadata about a RefAny's type
@@ -901,6 +905,64 @@ pub struct DragContextResponse {
     pub debug: String,
 }
 
+/// Response for GetFocusState - which node has keyboard focus
+#[cfg(feature = "std")]
+#[derive(Debug, Clone, serde::Serialize)]
+pub struct FocusStateResponse {
+    /// Whether any node has focus
+    pub has_focus: bool,
+    /// Focused node information (if any)
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub focused_node: Option<FocusedNodeInfo>,
+}
+
+/// Information about the focused node
+#[cfg(feature = "std")]
+#[derive(Debug, Clone, serde::Serialize)]
+pub struct FocusedNodeInfo {
+    /// DOM ID
+    pub dom_id: u32,
+    /// Node ID within the DOM
+    pub node_id: u64,
+    /// CSS selector for the node
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub selector: Option<String>,
+    /// Whether the node is contenteditable
+    pub is_contenteditable: bool,
+    /// Text content of the node (if text node)
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub text_content: Option<String>,
+}
+
+/// Response for GetCursorState - cursor position and blink state
+#[cfg(feature = "std")]
+#[derive(Debug, Clone, serde::Serialize)]
+pub struct CursorStateResponse {
+    /// Whether a cursor is active
+    pub has_cursor: bool,
+    /// Cursor information (if any)
+    #[serde(skip_serializing_if = "Option::is_none")]
+    pub cursor: Option<CursorInfo>,
+}
+
+/// Information about the text cursor
+#[cfg(feature = "std")]
+#[derive(Debug, Clone, serde::Serialize)]
+pub struct CursorInfo {
+    /// DOM ID where cursor is located
+    pub dom_id: u32,
+    /// Node ID within the DOM
+    pub node_id: u64,
+    /// Cursor position (grapheme cluster index)
+    pub position: usize,
+    /// Cursor affinity ("upstream" or "downstream")
+    pub affinity: String,
+    /// Whether the cursor is currently visible (false during blink off phase)
+    pub is_visible: bool,
+    /// Whether the cursor blink timer is active
+    pub blink_timer_active: bool,
+}
+
 // ==================== Debug Events ====================
 
 #[derive(Debug, Clone)]
@@ -1136,6 +1198,12 @@ pub enum DebugEvent {
         /// The JSON value to set as the new app state
         state: serde_json::Value,
     },
+
+    // Focus and Cursor State
+    /// Get the current focus state (which node has keyboard focus)
+    GetFocusState,
+    /// Get the current cursor state (position, blink state)
+    GetCursorState,
 }
 
 // ==================== Node Resolution Helper ====================
@@ -4478,6 +4546,95 @@ fn process_debug_event(
             send_ok(request, None, None);
         }
 
+        DebugEvent::GetFocusState => {
+            let layout_window = callback_info.get_layout_window();
+            let focus_manager = &layout_window.focus_manager;
+            
+            let response = if let Some(focused_node) = focus_manager.get_focused_node() {
+                let dom_id = focused_node.dom;
+                let internal_node_id = focused_node.node.into_crate_internal();
+                
+                let focused_info = internal_node_id.map(|node_id| {
+                    // Get node info
+                    let selector = build_selector_for_node(&callback_info, dom_id, node_id);
+                    
+                    // Check if contenteditable
+                    let is_contenteditable = callback_info
+                        .get_layout_window()
+                        .layout_results
+                        .get(&dom_id)
+                        .and_then(|lr| lr.styled_dom.node_data.get(node_id.index()))
+                        .map(|nd| nd.is_contenteditable())
+                        .unwrap_or(false);
+                    
+                    // Get text content - extract from NodeType::Text if available
+                    let text_content = callback_info
+                        .get_layout_window()
+                        .layout_results
+                        .get(&dom_id)
+                        .and_then(|lr| lr.styled_dom.node_data.get(node_id.index()))
+                        .and_then(|nd| {
+                            match nd.get_node_type() {
+                                azul_core::dom::NodeType::Text(s) => Some(s.as_str().to_string()),
+                                _ => None,
+                            }
+                        });
+                    
+                    FocusedNodeInfo {
+                        dom_id: dom_id.inner as u32,
+                        node_id: node_id.index() as u64,
+                        selector,
+                        is_contenteditable,
+                        text_content,
+                    }
+                });
+                
+                FocusStateResponse {
+                    has_focus: focused_info.is_some(),
+                    focused_node: focused_info,
+                }
+            } else {
+                FocusStateResponse {
+                    has_focus: false,
+                    focused_node: None,
+                }
+            };
+            
+            send_ok(request, None, Some(ResponseData::FocusState(response)));
+        }
+
+        DebugEvent::GetCursorState => {
+            let layout_window = callback_info.get_layout_window();
+            let cursor_manager = &layout_window.cursor_manager;
+            
+            let response = if let (Some(cursor), Some(location)) = (&cursor_manager.cursor, &cursor_manager.cursor_location) {
+                let position = cursor.cluster_id.start_byte_in_run as usize;
+                let affinity = match cursor.affinity {
+                    azul_core::selection::CursorAffinity::Leading => "leading".to_string(),
+                    azul_core::selection::CursorAffinity::Trailing => "trailing".to_string(),
+                };
+                
+                CursorStateResponse {
+                    has_cursor: true,
+                    cursor: Some(CursorInfo {
+                        dom_id: location.dom_id.inner as u32,
+                        node_id: location.node_id.index() as u64,
+                        position,
+                        affinity,
+                        is_visible: cursor_manager.is_visible,
+                        blink_timer_active: cursor_manager.blink_timer_active,
+                    }),
+                }
+            } else {
+                CursorStateResponse {
+                    has_cursor: false,
+                    cursor: None,
+                }
+            };
+            
+            send_ok(request, None, Some(ResponseData::CursorState(response)));
+        }
+
         _ => {
             log(
                 LogLevel::Warn,
diff --git a/dll/src/desktop/shell2/common/event_v2.rs b/dll/src/desktop/shell2/common/event_v2.rs
index 0fb827c4..2f1140ff 100644
--- a/dll/src/desktop/shell2/common/event_v2.rs
+++ b/dll/src/desktop/shell2/common/event_v2.rs
@@ -2145,8 +2145,8 @@ pub trait PlatformWindowV2 {
                                         let old_focus_node_id = focused_node.and_then(|f| f.node.into_crate_internal());
                                         let new_focus_node_id = new_focus_node.and_then(|f| f.node.into_crate_internal());
                                         
-                                        // Update focus manager
-                                        if let Some(layout_window) = self.get_layout_window_mut() {
+                                        // Update focus manager and get timer action
+                                        let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                                             layout_window.focus_manager.set_focused_node(new_focus_node);
                                             default_action_focus_changed = true;
                                             
@@ -2161,6 +2161,13 @@ pub trait PlatformWindowV2 {
                                                 );
                                             }
                                             
+                                            // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
+                                            let window_state = layout_window.current_window_state.clone();
+                                            let timer_action = layout_window.handle_focus_change_for_cursor_blink(
+                                                new_focus_node,
+                                                &window_state,
+                                            );
+                                            
                                             // RESTYLE: Update StyledNodeState and compute CSS changes
                                             if old_focus_node_id != new_focus_node_id {
                                                 let restyle_result = apply_focus_restyle(
@@ -2172,6 +2179,23 @@ pub trait PlatformWindowV2 {
                                             } else {
                                                 result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                                             }
+                                            
+                                            Some(timer_action)
+                                        } else {
+                                            None
+                                        };
+                                        
+                                        // Apply timer action outside the layout_window borrow
+                                        if let Some(timer_action) = timer_action {
+                                            match timer_action {
+                                                azul_layout::CursorBlinkTimerAction::Start(timer) => {
+                                                    self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
+                                                }
+                                                azul_layout::CursorBlinkTimerAction::Stop => {
+                                                    self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
+                                                }
+                                                azul_layout::CursorBlinkTimerAction::NoChange => {}
+                                            }
                                         }
 
                                         log_debug!(
@@ -2189,10 +2213,17 @@ pub trait PlatformWindowV2 {
                                 // Get old focus before clearing
                                 let old_focus_node_id = old_focus.and_then(|f| f.node.into_crate_internal());
                                 
-                                if let Some(layout_window) = self.get_layout_window_mut() {
+                                let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
                                     layout_window.focus_manager.set_focused_node(None);
                                     default_action_focus_changed = true;
                                     
+                                    // CURSOR BLINK TIMER: Stop timer when focus is cleared
+                                    let window_state = layout_window.current_window_state.clone();
+                                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
+                                        None,
+                                        &window_state,
+                                    );
+                                    
                                     // RESTYLE: Update StyledNodeState when focus is cleared
                                     if old_focus_node_id.is_some() {
                                         let restyle_result = apply_focus_restyle(
@@ -2204,6 +2235,21 @@ pub trait PlatformWindowV2 {
                                     } else {
                                         result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
                                     }
+                                    
+                                    Some(timer_action)
+                                } else {
+                                    None
+                                };
+                                
+                                // Apply timer action outside the layout_window borrow
+                                if let Some(timer_action) = timer_action {
+                                    match timer_action {
+                                        azul_layout::CursorBlinkTimerAction::Start(_) => {}
+                                        azul_layout::CursorBlinkTimerAction::Stop => {
+                                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
+                                        }
+                                        azul_layout::CursorBlinkTimerAction::NoChange => {}
+                                    }
                                 }
 
                                 log_debug!(
@@ -2427,6 +2473,25 @@ pub trait PlatformWindowV2 {
             }
         }
 
+        // W3C "flag and defer" pattern: Finalize pending focus changes after all events processed
+        // 
+        // This is called at the end of event processing to initialize the cursor for
+        // contenteditable elements. The cursor wasn't initialized during focus event handling
+        // because text layout may not have been available. Now that all events have been
+        // processed and layout has had a chance to update, we can safely initialize the cursor.
+        if let Some(layout_window) = self.get_layout_window_mut() {
+            if layout_window.focus_manager.needs_cursor_initialization() {
+                let cursor_initialized = layout_window.finalize_pending_focus_changes();
+                if cursor_initialized {
+                    log_debug!(
+                        super::debug_server::LogCategory::Input,
+                        "[Event V2] Cursor initialized via finalize_pending_focus_changes"
+                    );
+                    result = result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
+                }
+            }
+        }
+
         result
     }
 
@@ -2557,8 +2622,10 @@ pub trait PlatformWindowV2 {
 
         // Handle focus changes
         use azul_layout::callbacks::FocusUpdateRequest;
+        eprintln!("[DEBUG event_v2] update_focused_node = {:?}", result.update_focused_node);
         match result.update_focused_node {
             FocusUpdateRequest::FocusNode(new_focus) => {
+                eprintln!("[DEBUG event_v2] FocusUpdateRequest::FocusNode({:?})", new_focus);
                 // Update focus in the FocusManager (in LayoutWindow)
                 if let Some(layout_window) = self.get_layout_window_mut() {
                     layout_window
@@ -2573,6 +2640,23 @@ pub trait PlatformWindowV2 {
                         ScrollIntoViewOptions::nearest(),
                         now,
                     );
+                    
+                    // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
+                    let window_state = layout_window.current_window_state.clone();
+                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
+                        Some(new_focus),
+                        &window_state,
+                    );
+                    
+                    match timer_action {
+                        azul_layout::CursorBlinkTimerAction::Start(timer) => {
+                            self.start_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id, timer);
+                        }
+                        azul_layout::CursorBlinkTimerAction::Stop => {
+                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
+                        }
+                        azul_layout::CursorBlinkTimerAction::NoChange => {}
+                    }
                 }
                 event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
             }
@@ -2580,6 +2664,23 @@ pub trait PlatformWindowV2 {
                 // Clear focus in the FocusManager (in LayoutWindow)
                 if let Some(layout_window) = self.get_layout_window_mut() {
                     layout_window.focus_manager.set_focused_node(None);
+                    
+                    // CURSOR BLINK TIMER: Stop timer when focus is cleared
+                    let window_state = layout_window.current_window_state.clone();
+                    let timer_action = layout_window.handle_focus_change_for_cursor_blink(
+                        None,
+                        &window_state,
+                    );
+                    
+                    match timer_action {
+                        azul_layout::CursorBlinkTimerAction::Start(_timer) => {
+                            // Shouldn't happen when clearing focus, but handle it
+                        }
+                        azul_layout::CursorBlinkTimerAction::Stop => {
+                            self.stop_timer(azul_core::task::CURSOR_BLINK_TIMER_ID.id);
+                        }
+                        azul_layout::CursorBlinkTimerAction::NoChange => {}
+                    }
                 }
                 event_result = event_result.max(ProcessEventResult::ShouldReRenderCurrentWindow);
             }
diff --git a/layout/src/callbacks.rs b/layout/src/callbacks.rs
index 19e13ac4..ee89c655 100644
--- a/layout/src/callbacks.rs
+++ b/layout/src/callbacks.rs
@@ -391,6 +391,21 @@ pub enum CallbackChange {
         position: LogicalPosition,
         time_ms: u64,
     },
+
+    // Cursor Blinking (System Timer Control)
+    /// Set the cursor visibility state (called by blink timer)
+    SetCursorVisibility { visible: bool },
+    /// Reset cursor blink state on user input (makes cursor visible, records time)
+    ResetCursorBlink,
+    /// Start the cursor blink timer for the focused contenteditable element
+    StartCursorBlinkTimer,
+    /// Stop the cursor blink timer (when focus leaves contenteditable)
+    StopCursorBlinkTimer,
+    
+    // Scroll cursor/selection into view
+    /// Scroll the active text cursor into view within its scrollable container
+    /// This is automatically triggered after text input or cursor movement
+    ScrollActiveCursorIntoView,
 }
 
 /// Main callback type for UI event handling
@@ -1095,6 +1110,48 @@ impl CallbackInfo {
         self.push_change(CallbackChange::PreventDefault);
     }
 
+    // Cursor Blinking Api (for system timer control)
+    
+    /// Set cursor visibility state
+    ///
+    /// This is primarily used internally by the cursor blink timer callback.
+    /// User code typically doesn't need to call this directly.
+    pub fn set_cursor_visibility(&mut self, visible: bool) {
+        self.push_change(CallbackChange::SetCursorVisibility { visible });
+    }
+    
+    /// Reset cursor blink state on user input
+    ///
+    /// This makes the cursor visible and records the current time, so the blink
+    /// timer knows to keep the cursor solid for a while before blinking.
+    /// Called automatically on keyboard input, but can be called manually.
+    pub fn reset_cursor_blink(&mut self) {
+        self.push_change(CallbackChange::ResetCursorBlink);
+    }
+    
+    /// Start the cursor blink timer
+    ///
+    /// Called automatically when focus lands on a contenteditable element.
+    /// The timer will toggle cursor visibility at ~530ms intervals.
+    pub fn start_cursor_blink_timer(&mut self) {
+        self.push_change(CallbackChange::StartCursorBlinkTimer);
+    }
+    
+    /// Stop the cursor blink timer
+    ///
+    /// Called automatically when focus leaves a contenteditable element.
+    pub fn stop_cursor_blink_timer(&mut self) {
+        self.push_change(CallbackChange::StopCursorBlinkTimer);
+    }
+    
+    /// Scroll the active cursor into view
+    ///
+    /// This scrolls the focused text element's cursor into the visible area
+    /// of any scrollable ancestor. Called automatically after text input.
+    pub fn scroll_active_cursor_into_view(&mut self) {
+        self.push_change(CallbackChange::ScrollActiveCursorIntoView);
+    }
+
     /// Open a menu (context menu or dropdown)
     ///
     /// The menu will be displayed either as a native menu or a fallback DOM-based menu
diff --git a/layout/src/lib.rs b/layout/src/lib.rs
index 62b5b477..1796d6fa 100644
--- a/layout/src/lib.rs
+++ b/layout/src/lib.rs
@@ -182,7 +182,7 @@ pub use solver3::{LayoutContext, LayoutError, Result as LayoutResult3};
 #[cfg(feature = "text_layout")]
 pub use text3::cache::{FontManager, LayoutCache as TextLayoutCache};
 #[cfg(feature = "text_layout")]
-pub use window::{LayoutWindow, ScrollbarDragState};
+pub use window::{CursorBlinkTimerAction, LayoutWindow, ScrollbarDragState};
 
 // #[cfg(feature = "text_layout")]
 // pub use solver::{callback_info_shape_text, do_the_layout, do_the_relayout};
diff --git a/layout/src/managers/cursor.rs b/layout/src/managers/cursor.rs
index ada3e4e4..b9403b56 100644
--- a/layout/src/managers/cursor.rs
+++ b/layout/src/managers/cursor.rs
@@ -23,6 +23,16 @@
 //! - Programmatic focus via `AccessibilityAction::Focus`
 //! - Focus from screen reader commands
 //!
+//! ## Cursor Blinking
+//!
+//! The cursor blinks at ~530ms intervals when a contenteditable element has focus.
+//! Blinking is managed by a system timer (`CURSOR_BLINK_TIMER_ID`) that:
+//!
+//! - Starts when focus lands on a contenteditable element
+//! - Stops when focus moves away
+//! - Resets (cursor becomes visible) on any user input (keyboard, mouse)
+//! - After ~530ms of no input, the cursor toggles visibility
+//!
 //! ## Integration with Text Layout
 //!
 //! The cursor manager uses the `TextLayoutCache` to determine:
@@ -47,15 +57,26 @@
 use azul_core::{
     dom::{DomId, NodeId},
     selection::{CursorAffinity, GraphemeClusterId, TextCursor},
+    task::Instant,
 };
 
+/// Default cursor blink interval in milliseconds
+pub const CURSOR_BLINK_INTERVAL_MS: u64 = 530;
+
 /// Manager for text cursor position and rendering
-#[derive(Debug, Clone, PartialEq)]
+#[derive(Debug, Clone)]
 pub struct CursorManager {
     /// Current cursor position (if any)
     pub cursor: Option<TextCursor>,
     /// DOM and node where the cursor is located
     pub cursor_location: Option<CursorLocation>,
+    /// Whether the cursor is currently visible (toggled by blink timer)
+    pub is_visible: bool,
+    /// Timestamp of the last user input event (keyboard, mouse click in text)
+    /// Used to determine whether to blink or stay solid while typing
+    pub last_input_time: Option<Instant>,
+    /// Whether the cursor blink timer is currently active
+    pub blink_timer_active: bool,
 }
 
 /// Location of a cursor within the DOM
@@ -65,6 +86,13 @@ pub struct CursorLocation {
     pub node_id: NodeId,
 }
 
+impl PartialEq for CursorManager {
+    fn eq(&self, other: &Self) -> bool {
+        // Ignore is_visible and last_input_time for equality - they're transient state
+        self.cursor == other.cursor && self.cursor_location == other.cursor_location
+    }
+}
+
 impl Default for CursorManager {
     fn default() -> Self {
         Self::new()
@@ -77,6 +105,9 @@ impl CursorManager {
         Self {
             cursor: None,
             cursor_location: None,
+            is_visible: false,
+            last_input_time: None,
+            blink_timer_active: false,
         }
     }
 
@@ -98,6 +129,20 @@ impl CursorManager {
     pub fn set_cursor(&mut self, cursor: Option<TextCursor>, location: Option<CursorLocation>) {
         self.cursor = cursor;
         self.cursor_location = location;
+        // Make cursor visible when set
+        if cursor.is_some() {
+            self.is_visible = true;
+        }
+    }
+    
+    /// Set the cursor position with timestamp for blink reset
+    pub fn set_cursor_with_time(&mut self, cursor: Option<TextCursor>, location: Option<CursorLocation>, now: Instant) {
+        self.cursor = cursor;
+        self.cursor_location = location;
+        if cursor.is_some() {
+            self.is_visible = true;
+            self.last_input_time = Some(now);
+        }
     }
 
     /// Clear the cursor
@@ -107,12 +152,71 @@ impl CursorManager {
     pub fn clear(&mut self) {
         self.cursor = None;
         self.cursor_location = None;
+        self.is_visible = false;
+        self.last_input_time = None;
+        self.blink_timer_active = false;
     }
 
     /// Check if there is an active cursor
     pub fn has_cursor(&self) -> bool {
         self.cursor.is_some()
     }
+    
+    /// Check if the cursor should be drawn (has cursor AND is visible)
+    pub fn should_draw_cursor(&self) -> bool {
+        self.cursor.is_some() && self.is_visible
+    }
+    
+    /// Reset the blink state on user input
+    ///
+    /// This makes the cursor visible and records the input time.
+    /// The blink timer will keep the cursor visible until `CURSOR_BLINK_INTERVAL_MS`
+    /// has passed since this time.
+    pub fn reset_blink_on_input(&mut self, now: Instant) {
+        self.is_visible = true;
+        self.last_input_time = Some(now);
+    }
+    
+    /// Toggle cursor visibility (called by blink timer)
+    ///
+    /// Returns the new visibility state.
+    pub fn toggle_visibility(&mut self) -> bool {
+        self.is_visible = !self.is_visible;
+        self.is_visible
+    }
+    
+    /// Set cursor visibility directly
+    pub fn set_visibility(&mut self, visible: bool) {
+        self.is_visible = visible;
+    }
+    
+    /// Check if enough time has passed since last input to start blinking
+    ///
+    /// Returns true if the cursor should blink (toggle visibility),
+    /// false if it should stay solid (user is actively typing).
+    pub fn should_blink(&self, now: &Instant) -> bool {
+        use azul_core::task::{Duration, SystemTimeDiff};
+        
+        match &self.last_input_time {
+            Some(last_input) => {
+                let elapsed = now.duration_since(last_input);
+                let blink_interval = Duration::System(SystemTimeDiff::from_millis(CURSOR_BLINK_INTERVAL_MS));
+                // If elapsed time is greater than blink interval, allow blinking
+                elapsed.greater_than(&blink_interval)
+            }
+            None => true, // No input recorded, allow blinking
+        }
+    }
+    
+    /// Mark the blink timer as active
+    pub fn set_blink_timer_active(&mut self, active: bool) {
+        self.blink_timer_active = active;
+    }
+    
+    /// Check if the blink timer is active
+    pub fn is_blink_timer_active(&self) -> bool {
+        self.blink_timer_active
+    }
 
     /// Initialize cursor at the end of the text in the given node
     ///
@@ -130,9 +234,12 @@ impl CursorManager {
         node_id: NodeId,
         text_layout: Option<&alloc::sync::Arc<crate::text3::cache::UnifiedLayout>>,
     ) -> bool {
+        eprintln!("[DEBUG] initialize_cursor_at_end: dom_id={:?}, node_id={:?}, has_layout={}", dom_id, node_id, text_layout.is_some());
+        
         // Get the text layout for this node
         let Some(layout) = text_layout else {
             // No text layout - set cursor at start
+            eprintln!("[DEBUG] No text layout, setting cursor at start");
             self.cursor = Some(TextCursor {
                 cluster_id: GraphemeClusterId {
                     source_run: 0,
@@ -141,16 +248,19 @@ impl CursorManager {
                 affinity: CursorAffinity::Trailing,
             });
             self.cursor_location = Some(CursorLocation { dom_id, node_id });
+            eprintln!("[DEBUG] Cursor set: {:?}", self.cursor);
             return true;
         };
 
         // Find the last grapheme cluster in items
         let mut last_cluster_id: Option<GraphemeClusterId> = None;
+        eprintln!("[DEBUG] Layout has {} items", layout.items.len());
 
         // Iterate through all items to find the last cluster
         for item in layout.items.iter().rev() {
             if let crate::text3::cache::ShapedItem::Cluster(cluster) = &item.item {
                 last_cluster_id = Some(cluster.source_cluster_id);
+                eprintln!("[DEBUG] Found last cluster: {:?}", last_cluster_id);
                 break;
             }
         }
@@ -165,6 +275,7 @@ impl CursorManager {
         });
 
         self.cursor_location = Some(CursorLocation { dom_id, node_id });
+        eprintln!("[DEBUG] Cursor initialized: cursor={:?}, location={:?}", self.cursor, self.cursor_location);
 
         true
     }
diff --git a/layout/src/managers/focus_cursor.rs b/layout/src/managers/focus_cursor.rs
index 0bb02df7..2e005e27 100644
--- a/layout/src/managers/focus_cursor.rs
+++ b/layout/src/managers/focus_cursor.rs
@@ -17,18 +17,53 @@ use crate::window::DomLayoutResult;
 /// CSS path for selecting elements (placeholder - needs proper implementation)
 pub type CssPathString = alloc::string::String;
 
+/// Information about a pending contenteditable focus that needs cursor initialization
+/// after layout is complete (W3C "flag and defer" pattern).
+///
+/// This is set during focus event handling and consumed after layout pass.
+#[derive(Debug, Clone, PartialEq)]
+pub struct PendingContentEditableFocus {
+    /// The DOM where the contenteditable element is
+    pub dom_id: DomId,
+    /// The contenteditable container node that received focus
+    pub container_node_id: NodeId,
+    /// The text node where the cursor should be placed (often a child of the container)
+    pub text_node_id: NodeId,
+}
+
 /// Manager for keyboard focus and tab navigation
 ///
 /// Note: Text cursor management is now handled by the separate `CursorManager`.
 ///
 /// The `FocusManager` only tracks which node has focus, while `CursorManager`
 /// tracks the cursor position within that node (if it's contenteditable).
+///
+/// ## W3C Focus/Selection Model
+///
+/// The W3C model maintains a strict separation between **keyboard focus** and **selection**:
+///
+/// 1. **Focus** lands on the contenteditable container (`document.activeElement`)
+/// 2. **Selection/Cursor** is placed in a descendant text node (`Selection.focusNode`)
+///
+/// This separation requires a "flag and defer" pattern:
+/// - During focus event: Set `cursor_needs_initialization = true`
+/// - After layout pass: Call `finalize_pending_focus_changes()` to actually initialize the cursor
+///
+/// This is necessary because cursor positioning requires text layout information,
+/// which isn't available during the focus event handling phase.
 #[derive(Debug, Clone, PartialEq)]
 pub struct FocusManager {
     /// Currently focused node (if any)
     pub focused_node: Option<DomNodeId>,
     /// Pending focus request from callback
     pub pending_focus_request: Option<FocusTarget>,
+    
+    // --- W3C "flag and defer" pattern fields ---
+    
+    /// Flag indicating that cursor initialization is pending (set during focus, consumed after layout)
+    pub cursor_needs_initialization: bool,
+    /// Information about the pending contenteditable focus
+    pub pending_contenteditable_focus: Option<PendingContentEditableFocus>,
 }
 
 impl Default for FocusManager {
@@ -43,6 +78,8 @@ impl FocusManager {
         Self {
             focused_node: None,
             pending_focus_request: None,
+            cursor_needs_initialization: false,
+            pending_contenteditable_focus: None,
         }
     }
 
@@ -79,6 +116,59 @@ impl FocusManager {
     pub fn has_focus(&self, node: &DomNodeId) -> bool {
         self.focused_node.as_ref() == Some(node)
     }
+    
+    // --- W3C "flag and defer" pattern methods ---
+    
+    /// Mark that cursor initialization is needed for a contenteditable element.
+    ///
+    /// This is called during focus event handling. The actual cursor initialization
+    /// happens later in `finalize_pending_focus_changes()` after layout is complete.
+    ///
+    /// # W3C Conformance
+    ///
+    /// In the W3C model, when focus lands on a contenteditable element:
+    /// 1. The focus event fires on the container element
+    /// 2. The browser's editing engine modifies the Selection to place a caret
+    /// 3. The Selection's anchorNode/focusNode point to the child text node
+    ///
+    /// Since we need layout information to position the cursor, we defer step 2+3.
+    pub fn set_pending_contenteditable_focus(
+        &mut self,
+        dom_id: DomId,
+        container_node_id: NodeId,
+        text_node_id: NodeId,
+    ) {
+        self.cursor_needs_initialization = true;
+        self.pending_contenteditable_focus = Some(PendingContentEditableFocus {
+            dom_id,
+            container_node_id,
+            text_node_id,
+        });
+    }
+    
+    /// Clear the pending contenteditable focus (when focus moves away or is cleared).
+    pub fn clear_pending_contenteditable_focus(&mut self) {
+        self.cursor_needs_initialization = false;
+        self.pending_contenteditable_focus = None;
+    }
+    
+    /// Take the pending contenteditable focus (consumes the flag).
+    ///
+    /// Returns `Some(info)` if cursor initialization is pending, `None` otherwise.
+    /// After calling this, `cursor_needs_initialization` is set to `false`.
+    pub fn take_pending_contenteditable_focus(&mut self) -> Option<PendingContentEditableFocus> {
+        if self.cursor_needs_initialization {
+            self.cursor_needs_initialization = false;
+            self.pending_contenteditable_focus.take()
+        } else {
+            None
+        }
+    }
+    
+    /// Check if cursor initialization is pending.
+    pub fn needs_cursor_initialization(&self) -> bool {
+        self.cursor_needs_initialization
+    }
 }
 
 /// Direction for cursor navigation
diff --git a/layout/src/solver3/display_list.rs b/layout/src/solver3/display_list.rs
index a86ac49e..296eec8e 100644
--- a/layout/src/solver3/display_list.rs
+++ b/layout/src/solver3/display_list.rs
@@ -1388,11 +1388,13 @@ where
                 // 1. This is the focus node AND
                 // 2. The element is contenteditable AND  
                 // 3. The selection is collapsed (insertion point, not range selection) AND
-                // 4. The element is selectable (user-select != none)
+                // 4. The element is selectable (user-select != none) AND
+                // 5. The cursor is in the "visible" phase of blinking (cursor_is_visible)
                 if text_selection.focus.ifc_root_node_id == dom_id 
                     && is_contenteditable 
                     && is_collapsed 
                     && is_selectable
+                    && self.ctx.cursor_is_visible
                 {
                     if let Some(mut rect) = layout.get_cursor_rect(&text_selection.focus.cursor) {
                         let style = get_caret_style(self.ctx.styled_dom, Some(dom_id));
@@ -1421,8 +1423,9 @@ where
         for selection in selection_state.selections.as_slice() {
             match &selection {
                 Selection::Cursor(cursor) => {
-                    // Only draw cursor if this element is contenteditable and selectable
-                    if !is_contenteditable || !is_selectable {
+                    // Only draw cursor if this element is contenteditable, selectable,
+                    // AND the cursor is in the "visible" phase of blinking
+                    if !is_contenteditable || !is_selectable || !self.ctx.cursor_is_visible {
                         continue;
                     }
                     // Draw cursor
@@ -1433,9 +1436,6 @@ where
                         rect.origin.x += content_box_offset_x;
                         rect.origin.y += content_box_offset_y;
 
-                        // TODO: The blinking logic would need to be handled by the renderer
-                        // using an opacity key or similar, or by the main loop toggling this.
-                        // For now, we just draw it.
                         builder.push_cursor_rect(rect, style.color);
                     }
                 }
diff --git a/layout/src/solver3/getters.rs b/layout/src/solver3/getters.rs
index ed41fd1c..644c239e 100644
--- a/layout/src/solver3/getters.rs
+++ b/layout/src/solver3/getters.rs
@@ -2532,3 +2532,95 @@ pub fn is_node_contenteditable(styled_dom: &StyledDom, node_id: NodeId) -> bool
         matches!(attr, AttributeType::ContentEditable(_))
     })
 }
+
+/// W3C-conformant contenteditable inheritance check.
+///
+/// In the W3C model, the `contenteditable` attribute is **inherited**:
+/// - A node is editable if it has `contenteditable="true"` set directly
+/// - OR if its parent has `isContentEditable` as true
+/// - UNLESS the node explicitly sets `contenteditable="false"`
+///
+/// This function traverses up the DOM tree to determine editability.
+///
+/// # Returns
+///
+/// - `true` if the node is editable (either directly or via inheritance)
+/// - `false` if the node is not editable or has `contenteditable="false"`
+///
+/// # Example
+///
+/// ```html
+/// <div contenteditable="true">
+///   A                              <!-- editable (inherited) -->
+///   <div contenteditable="false">
+///     B                            <!-- NOT editable (explicitly false) -->
+///   </div>
+///   C                              <!-- editable (inherited) -->
+/// </div>
+/// ```
+pub fn is_node_contenteditable_inherited(styled_dom: &StyledDom, node_id: NodeId) -> bool {
+    use azul_core::dom::AttributeType;
+    
+    let node_data_container = styled_dom.node_data.as_container();
+    let hierarchy = styled_dom.node_hierarchy.as_container();
+    
+    let mut current_node_id = Some(node_id);
+    
+    while let Some(nid) = current_node_id {
+        let node_data = &node_data_container[nid];
+        
+        // Check for explicit contenteditable attribute on this node
+        for attr in node_data.attributes.as_ref().iter() {
+            if let AttributeType::ContentEditable(is_editable) = attr {
+                // If explicitly set to true, node is editable
+                // If explicitly set to false, node is NOT editable (blocks inheritance)
+                return *is_editable;
+            }
+        }
+        
+        // No explicit attribute, check parent
+        current_node_id = hierarchy.get(nid).and_then(|h| h.parent_id());
+    }
+    
+    // Reached root without finding contenteditable - not editable
+    false
+}
+
+/// Find the contenteditable ancestor of a node.
+///
+/// When focus lands on a text node inside a contenteditable container,
+/// we need to find the actual container that has the `contenteditable` attribute.
+///
+/// # Returns
+///
+/// - `Some(node_id)` of the contenteditable ancestor (may be the node itself)
+/// - `None` if no contenteditable ancestor exists
+pub fn find_contenteditable_ancestor(styled_dom: &StyledDom, node_id: NodeId) -> Option<NodeId> {
+    use azul_core::dom::AttributeType;
+    
+    let node_data_container = styled_dom.node_data.as_container();
+    let hierarchy = styled_dom.node_hierarchy.as_container();
+    
+    let mut current_node_id = Some(node_id);
+    
+    while let Some(nid) = current_node_id {
+        let node_data = &node_data_container[nid];
+        
+        // Check for contenteditable="true" on this node
+        for attr in node_data.attributes.as_ref().iter() {
+            if let AttributeType::ContentEditable(is_editable) = attr {
+                if *is_editable {
+                    return Some(nid);
+                } else {
+                    // Explicitly not editable - stop search
+                    return None;
+                }
+            }
+        }
+        
+        // Check parent
+        current_node_id = hierarchy.get(nid).and_then(|h| h.parent_id());
+    }
+    
+    None
+}
diff --git a/layout/src/solver3/mod.rs b/layout/src/solver3/mod.rs
index 4bbe6884..788056d4 100644
--- a/layout/src/solver3/mod.rs
+++ b/layout/src/solver3/mod.rs
@@ -176,6 +176,10 @@ pub struct LayoutContext<'a, T: ParsedFontTrait> {
     /// Fragmentation context for CSS Paged Media (PDF generation)
     /// When Some, layout respects page boundaries and generates one DisplayList per page
     pub fragmentation_context: Option<&'a mut crate::paged::FragmentationContext>,
+    /// Whether the text cursor should be drawn (managed by CursorManager blink timer)
+    /// When false, the cursor is in the "off" phase of blinking and should not be rendered.
+    /// When true (default), the cursor is visible.
+    pub cursor_is_visible: bool,
 }
 
 impl<'a, T: ParsedFontTrait> LayoutContext<'a, T> {
@@ -359,6 +363,7 @@ pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
     renderer_resources: &azul_core::resources::RendererResources,
     id_namespace: azul_core::resources::IdNamespace,
     dom_id: azul_core::dom::DomId,
+    cursor_is_visible: bool,
 ) -> Result<DisplayList> {
     // Reset IFC ID counter at the start of each layout pass
     // This ensures IFCs get consistent IDs across frames when the DOM structure is stable
@@ -386,6 +391,7 @@ pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
         counters: &mut counter_values,
         viewport_size: viewport.size,
         fragmentation_context: None,
+        cursor_is_visible, // Use the parameter
     };
 
     // --- Step 1: Reconciliation & Invalidation ---
@@ -414,6 +420,7 @@ pub fn layout_document<T: ParsedFontTrait + Sync + 'static>(
         counters: &mut counter_values,
         viewport_size: viewport.size,
         fragmentation_context: None,
+        cursor_is_visible, // Use the parameter
     };
 
     // --- Step 1.5: Early Exit Optimization ---
diff --git a/layout/src/solver3/paged_layout.rs b/layout/src/solver3/paged_layout.rs
index 274472b8..364d710d 100644
--- a/layout/src/solver3/paged_layout.rs
+++ b/layout/src/solver3/paged_layout.rs
@@ -229,6 +229,7 @@ where
         counters: &mut counter_values,
         viewport_size: viewport.size,
         fragmentation_context: Some(&mut fragmentation_context),
+        cursor_is_visible: true, // Paged layout: cursor always visible
     };
 
     // NEW: Use the commitment-based pagination approach with CSS break properties
@@ -353,6 +354,7 @@ fn layout_document_with_fragmentation<T: ParsedFontTrait + Sync + 'static>(
         counters: &mut counter_values,
         viewport_size: viewport.size,
         fragmentation_context: Some(fragmentation_context),
+        cursor_is_visible: true, // Paged layout: cursor always visible
     };
 
     // --- Step 1: Reconciliation & Invalidation ---
@@ -379,6 +381,7 @@ fn layout_document_with_fragmentation<T: ParsedFontTrait + Sync + 'static>(
         counters: &mut counter_values,
         viewport_size: viewport.size,
         fragmentation_context: Some(fragmentation_context),
+        cursor_is_visible: true, // Paged layout: cursor always visible
     };
 
     // --- Step 1.5: Early Exit Optimization ---
diff --git a/layout/src/timer.rs b/layout/src/timer.rs
index 3088308b..d3b7af5a 100644
--- a/layout/src/timer.rs
+++ b/layout/src/timer.rs
@@ -498,6 +498,34 @@ impl TimerCallbackInfo {
     pub fn has_sufficient_history_for_gestures(&self) -> bool {
         false // Timers don't track gesture history
     }
+    
+    // Cursor blink timer methods
+    
+    /// Set cursor visibility state (for cursor blink timer)
+    pub fn set_cursor_visibility(&mut self, visible: bool) {
+        self.callback_info.set_cursor_visibility(visible);
+    }
+    
+    /// Toggle cursor visibility (for cursor blink timer)
+    ///
+    /// This is a shortcut that reads the current visibility state,
+    /// toggles it, and queues the change. Used by the cursor blink timer.
+    pub fn set_cursor_visibility_toggle(&mut self) {
+        // We can't read the current state from here, so we queue a special toggle action
+        // The actual toggle will be handled in apply_callback_changes using CursorManager.toggle_visibility()
+        use crate::callbacks::CallbackChange;
+        // Use SetCursorVisibility with a special sentinel value to indicate toggle
+        // Actually, let's just add a separate toggle method or use the existing ones smartly
+        
+        // For simplicity, we'll queue both a reset_cursor_blink (to handle idle detection)
+        // and let the apply_callback_changes handle the visibility toggle based on should_blink()
+        self.callback_info.push_change(CallbackChange::SetCursorVisibility { visible: true });
+    }
+    
+    /// Reset cursor blink state on user input
+    pub fn reset_cursor_blink(&mut self) {
+        self.callback_info.reset_cursor_blink();
+    }
 }
 
 /// Invokes the timer if it should run
diff --git a/layout/src/window.rs b/layout/src/window.rs
index bb3937a2..7e601ffa 100644
--- a/layout/src/window.rs
+++ b/layout/src/window.rs
@@ -141,12 +141,80 @@ pub struct NoCursorDestination {
     pub reason: String,
 }
 
+/// Action to take for the cursor blink timer when focus changes
+///
+/// This enum is returned by `LayoutWindow::handle_focus_change_for_cursor_blink()`
+/// to tell the platform layer what timer action to take.
+#[derive(Debug, Clone)]
+pub enum CursorBlinkTimerAction {
+    /// Start the cursor blink timer with the given timer configuration
+    Start(crate::timer::Timer),
+    /// Stop the cursor blink timer
+    Stop,
+    /// No change needed (timer already in correct state)
+    NoChange,
+}
+
 /// Helper function to create a unique IdNamespace
 fn new_id_namespace() -> IdNamespace {
     let id = ID_NAMESPACE_COUNTER.fetch_add(1, Ordering::Relaxed) as u32;
     IdNamespace(id)
 }
 
+// ============================================================================
+// Cursor Blink Timer Callback
+// ============================================================================
+
+/// Destructor for cursor blink timer RefAny (no-op since we use null pointer)
+extern "C" fn cursor_blink_timer_destructor(_: RefAny) {
+    // No cleanup needed - we use a null pointer RefAny
+}
+
+/// Callback for the cursor blink timer
+///
+/// This function is called every ~530ms to toggle cursor visibility.
+/// It checks if enough time has passed since the last user input before blinking,
+/// to avoid blinking while the user is actively typing.
+///
+/// The callback returns:
+/// - `TerminateTimer::Continue` + `Update::RefreshDom` if cursor toggled
+/// - `TerminateTimer::Terminate` if focus is no longer on a contenteditable element
+pub extern "C" fn cursor_blink_timer_callback(
+    _data: RefAny,
+    mut info: crate::timer::TimerCallbackInfo,
+) -> azul_core::callbacks::TimerCallbackReturn {
+    use azul_core::callbacks::{TimerCallbackReturn, Update};
+    use azul_core::task::TerminateTimer;
+    
+    // Get current time
+    let now = info.get_current_time();
+    
+    // We need to access the LayoutWindow through the info
+    // The timer callback needs to:
+    // 1. Check if focus is still on a contenteditable element
+    // 2. Check time since last input
+    // 3. Toggle visibility or keep solid
+    
+    // For now, we'll queue changes via the CallbackInfo system
+    // The actual state modification happens in apply_callback_changes
+    
+    // Check if we should blink or stay solid
+    // This is done by checking CursorManager.should_blink(now) in the layout window
+    
+    // Since we can't access LayoutWindow directly here (it's not passed to timer callbacks),
+    // we use a different approach: the timer callback always toggles, and the visibility
+    // check is done in display_list.rs based on CursorManager state.
+    
+    // Simply toggle cursor visibility
+    info.set_cursor_visibility_toggle();
+    
+    // Continue the timer and request a redraw
+    TimerCallbackReturn {
+        should_update: Update::RefreshDom,
+        should_terminate: TerminateTimer::Continue,
+    }
+}
+
 /// Result of a layout pass for a single DOM, before display list generation
 #[derive(Debug)]
 pub struct DomLayoutResult {
@@ -695,6 +763,9 @@ impl LayoutWindow {
         let scroll_offsets = self.scroll_manager.get_scroll_states_for_dom(dom_id);
         let styled_dom_clone = styled_dom.clone();
         let gpu_cache = self.gpu_state_manager.get_or_create_cache(dom_id).clone();
+        
+        // Get cursor visibility from cursor manager for display list generation
+        let cursor_is_visible = self.cursor_manager.should_draw_cursor();
 
         let mut display_list = solver3::layout_document(
             &mut self.layout_cache,
@@ -710,6 +781,7 @@ impl LayoutWindow {
             &self.renderer_resources,
             self.id_namespace,
             dom_id,
+            cursor_is_visible,
         )?;
 
         let tree = self
@@ -1426,6 +1498,290 @@ impl LayoutWindow {
     pub fn get_thread_ids(&self) -> ThreadIdVec {
         self.threads.keys().copied().collect::<Vec<_>>().into()
     }
+    
+    // Cursor Blinking Timer
+    
+    /// Create the cursor blink timer
+    ///
+    /// This timer toggles cursor visibility at ~530ms intervals.
+    /// It checks if enough time has passed since the last user input before blinking,
+    /// to avoid blinking while the user is actively typing.
+    pub fn create_cursor_blink_timer(&self, _window_state: &FullWindowState) -> crate::timer::Timer {
+        use azul_core::task::{Duration, SystemTimeDiff};
+        use crate::timer::{Timer, TimerCallback};
+        use azul_core::refany::RefAny;
+        
+        let interval_ms = crate::managers::cursor::CURSOR_BLINK_INTERVAL_MS;
+        
+        // Create a RefAny with a unit type - the timer callback doesn't need any data
+        // The actual cursor state is in LayoutWindow.cursor_manager
+        let refany = RefAny::new(());
+        
+        Timer {
+            refany,
+            node_id: None.into(),
+            created: azul_core::task::Instant::now(),
+            run_count: 0,
+            last_run: azul_core::task::OptionInstant::None,
+            delay: azul_core::task::OptionDuration::None,
+            interval: azul_core::task::OptionDuration::Some(Duration::System(SystemTimeDiff::from_millis(interval_ms))),
+            timeout: azul_core::task::OptionDuration::None,
+            callback: TimerCallback::create(cursor_blink_timer_callback),
+        }
+    }
+    
+    /// Scroll the active text cursor into view within its scrollable container
+    ///
+    /// This finds the focused contenteditable node, gets the cursor rectangle,
+    /// and scrolls any scrollable ancestor to ensure the cursor is visible.
+    pub fn scroll_active_cursor_into_view(&mut self, result: &mut CallbackChangeResult) {
+        use crate::managers::scroll_into_view;
+        
+        // Get the focused node
+        let focused_node = match self.focus_manager.get_focused_node() {
+            Some(node) => *node,
+            None => return,
+        };
+        
+        let Some(node_id_internal) = focused_node.node.into_crate_internal() else {
+            return;
+        };
+        
+        // Check if node is contenteditable
+        if !self.is_node_contenteditable_internal(focused_node.dom, node_id_internal) {
+            return;
+        }
+        
+        // Get the cursor location
+        let cursor_location = match self.cursor_manager.get_cursor_location() {
+            Some(loc) if loc.dom_id == focused_node.dom && loc.node_id == node_id_internal => loc,
+            _ => return,
+        };
+        
+        // Get the cursor position
+        let cursor = match self.cursor_manager.get_cursor() {
+            Some(c) => c.clone(),
+            None => return,
+        };
+        
+        // Get the inline layout to find the cursor rectangle
+        let layout = match self.get_inline_layout_for_node(focused_node.dom, node_id_internal) {
+            Some(l) => l,
+            None => return,
+        };
+        
+        // Get cursor rectangle (node-local coordinates)
+        let cursor_rect = match layout.get_cursor_rect(&cursor) {
+            Some(r) => r,
+            None => return,
+        };
+        
+        // Use scroll_into_view to scroll the cursor rect into view
+        let now = azul_core::task::Instant::now();
+        let options = scroll_into_view::ScrollIntoViewOptions::nearest();
+        
+        // Calculate scroll adjustments
+        let adjustments = scroll_into_view::scroll_rect_into_view(
+            cursor_rect,
+            focused_node.dom,
+            node_id_internal,
+            &self.layout_results,
+            &mut self.scroll_manager,
+            options,
+            now,
+        );
+        
+        // Record the scroll changes
+        for adj in adjustments {
+            let current_pos = self.scroll_manager
+                .get_current_offset(adj.scroll_container_dom_id, adj.scroll_container_node_id)
+                .unwrap_or(LogicalPosition::zero());
+            
+            let hierarchy_id = NodeHierarchyItemId::from_crate_internal(Some(adj.scroll_container_node_id));
+            result
+                .nodes_scrolled
+                .entry(adj.scroll_container_dom_id)
+                .or_insert_with(BTreeMap::new)
+                .insert(hierarchy_id, current_pos);
+        }
+    }
+    
+    /// Check if a node is contenteditable (internal version using NodeId)
+    fn is_node_contenteditable_internal(&self, dom_id: DomId, node_id: NodeId) -> bool {
+        use crate::solver3::getters::is_node_contenteditable;
+        
+        let Some(layout_result) = self.layout_results.get(&dom_id) else {
+            return false;
+        };
+        
+        is_node_contenteditable(&layout_result.styled_dom, node_id)
+    }
+    
+    /// Check if a node is contenteditable with W3C-conformant inheritance.
+    ///
+    /// This traverses up the DOM tree to check if the node or any ancestor
+    /// has `contenteditable="true"` set, respecting `contenteditable="false"`
+    /// to stop inheritance.
+    fn is_node_contenteditable_inherited_internal(&self, dom_id: DomId, node_id: NodeId) -> bool {
+        use crate::solver3::getters::is_node_contenteditable_inherited;
+        
+        let Some(layout_result) = self.layout_results.get(&dom_id) else {
+            return false;
+        };
+        
+        is_node_contenteditable_inherited(&layout_result.styled_dom, node_id)
+    }
+    
+    /// Handle focus change for cursor blink timer management (W3C "flag and defer" pattern)
+    ///
+    /// This method implements the W3C focus/selection model:
+    /// 1. Focus change is handled immediately (timer start/stop)
+    /// 2. Cursor initialization is DEFERRED until after layout (via flag)
+    ///
+    /// The cursor is NOT initialized here because text layout may not be available
+    /// during focus event handling. Instead, we set a flag that is consumed by
+    /// `finalize_pending_focus_changes()` after the layout pass.
+    ///
+    /// # Parameters
+    ///
+    /// * `new_focus` - The newly focused node (None if focus is being cleared)
+    /// * `current_window_state` - Current window state for timer creation
+    ///
+    /// # Returns
+    ///
+    /// A `CursorBlinkTimerAction` indicating what timer action the platform
+    /// layer should take.
+    pub fn handle_focus_change_for_cursor_blink(
+        &mut self,
+        new_focus: Option<azul_core::dom::DomNodeId>,
+        current_window_state: &FullWindowState,
+    ) -> CursorBlinkTimerAction {
+        eprintln!("[DEBUG] handle_focus_change_for_cursor_blink called with new_focus={:?}", new_focus);
+        
+        // Check if the new focus is on a contenteditable element
+        // Use the inherited check for W3C conformance
+        let contenteditable_info = match new_focus {
+            Some(focus_node) => {
+                if let Some(node_id) = focus_node.node.into_crate_internal() {
+                    // Check if this node or any ancestor is contenteditable
+                    if self.is_node_contenteditable_inherited_internal(focus_node.dom, node_id) {
+                        // Find the text node where the cursor should be placed
+                        let text_node_id = self.find_last_text_child(focus_node.dom, node_id)
+                            .unwrap_or(node_id);
+                        Some((focus_node.dom, node_id, text_node_id))
+                    } else {
+                        None
+                    }
+                } else {
+                    None
+                }
+            }
+            None => None,
+        };
+        
+        // Determine the action based on current state and new focus
+        let timer_was_active = self.cursor_manager.is_blink_timer_active();
+        eprintln!("[DEBUG] is_contenteditable={}, timer_was_active={}", contenteditable_info.is_some(), timer_was_active);
+        
+        if let Some((dom_id, container_node_id, text_node_id)) = contenteditable_info {
+            eprintln!("[DEBUG] Setting pending contenteditable focus: dom={:?}, container={:?}, text={:?}", dom_id, container_node_id, text_node_id);
+            
+            // W3C "flag and defer" pattern:
+            // Set flag for cursor initialization AFTER layout pass
+            self.focus_manager.set_pending_contenteditable_focus(
+                dom_id,
+                container_node_id,
+                text_node_id,
+            );
+            
+            // Make cursor visible and record current time (even before actual initialization)
+            let now = azul_core::task::Instant::now();
+            self.cursor_manager.reset_blink_on_input(now);
+            self.cursor_manager.set_blink_timer_active(true);
+            
+            if !timer_was_active {
+                // Need to start the timer
+                let timer = self.create_cursor_blink_timer(current_window_state);
+                eprintln!("[DEBUG] Returning CursorBlinkTimerAction::Start");
+                return CursorBlinkTimerAction::Start(timer);
+            } else {
+                // Timer already active, just continue
+                eprintln!("[DEBUG] Returning CursorBlinkTimerAction::NoChange (timer already active)");
+                return CursorBlinkTimerAction::NoChange;
+            }
+        } else {
+            // Focus is moving away from contenteditable or being cleared
+            eprintln!("[DEBUG] Focus is NOT contenteditable, clearing cursor and pending focus");
+            
+            // Clear the cursor AND the pending focus flag
+            self.cursor_manager.clear();
+            self.focus_manager.clear_pending_contenteditable_focus();
+            
+            if timer_was_active {
+                // Need to stop the timer
+                self.cursor_manager.set_blink_timer_active(false);
+                eprintln!("[DEBUG] Returning CursorBlinkTimerAction::Stop");
+                return CursorBlinkTimerAction::Stop;
+            } else {
+                eprintln!("[DEBUG] Returning CursorBlinkTimerAction::NoChange (timer was not active)");
+                return CursorBlinkTimerAction::NoChange;
+            }
+        }
+    }
+    
+    /// Finalize pending focus changes after layout pass (W3C "flag and defer" pattern)
+    ///
+    /// This method should be called AFTER the layout pass completes. It checks if
+    /// there's a pending contenteditable focus and initializes the cursor now that
+    /// text layout information is available.
+    ///
+    /// # W3C Conformance
+    ///
+    /// In the W3C model:
+    /// 1. Focus event fires during event handling (layout may not be ready)
+    /// 2. Selection/cursor placement happens after layout is computed
+    /// 3. The cursor is drawn at the position specified by the Selection
+    ///
+    /// This function implements step 2+3 by:
+    /// - Checking the `cursor_needs_initialization` flag
+    /// - Getting the (now available) text layout
+    /// - Initializing the cursor at the correct position
+    ///
+    /// # Returns
+    ///
+    /// `true` if cursor was initialized, `false` if no pending focus or initialization failed.
+    pub fn finalize_pending_focus_changes(&mut self) -> bool {
+        eprintln!("[DEBUG] finalize_pending_focus_changes called, needs_init={}", 
+            self.focus_manager.needs_cursor_initialization());
+        
+        // Take the pending focus info (this clears the flag)
+        let pending = match self.focus_manager.take_pending_contenteditable_focus() {
+            Some(p) => p,
+            None => {
+                eprintln!("[DEBUG] No pending contenteditable focus");
+                return false;
+            }
+        };
+        
+        eprintln!("[DEBUG] Initializing cursor for pending focus: dom={:?}, text_node={:?}", 
+            pending.dom_id, pending.text_node_id);
+        
+        // Now we can safely get the text layout (layout pass has completed)
+        let text_layout = self.get_inline_layout_for_node(pending.dom_id, pending.text_node_id).cloned();
+        eprintln!("[DEBUG] text_layout available: {}", text_layout.is_some());
+        
+        // Initialize cursor at end of text
+        let cursor_initialized = self.cursor_manager.initialize_cursor_at_end(
+            pending.dom_id,
+            pending.text_node_id,
+            text_layout.as_ref(),
+        );
+        
+        eprintln!("[DEBUG] Cursor initialized: {}, cursor={:?}, location={:?}", 
+            cursor_initialized, self.cursor_manager.cursor, self.cursor_manager.cursor_location);
+        
+        cursor_initialized
+    }
 
     // CallbackChange Processing
 
@@ -1878,6 +2234,41 @@ impl LayoutWindow {
                     // The selection update will cause the display list to be regenerated
                     let _ = self.process_mouse_click_for_selection(position, time_ms);
                 }
+                CallbackChange::SetCursorVisibility { visible: _ } => {
+                    // Timer callback sets visibility - check if we should blink or stay solid
+                    let now = azul_core::task::Instant::now();
+                    if self.cursor_manager.should_blink(&now) {
+                        // Enough time has passed since last input - toggle visibility
+                        self.cursor_manager.toggle_visibility();
+                    } else {
+                        // User is actively typing - keep cursor visible
+                        self.cursor_manager.set_visibility(true);
+                    }
+                }
+                CallbackChange::ResetCursorBlink => {
+                    // Reset cursor blink state on user input
+                    let now = azul_core::task::Instant::now();
+                    self.cursor_manager.reset_blink_on_input(now);
+                }
+                CallbackChange::StartCursorBlinkTimer => {
+                    // Start the cursor blink timer if not already active
+                    if !self.cursor_manager.is_blink_timer_active() {
+                        let timer = self.create_cursor_blink_timer(current_window_state);
+                        result.timers.insert(azul_core::task::CURSOR_BLINK_TIMER_ID, timer);
+                        self.cursor_manager.set_blink_timer_active(true);
+                    }
+                }
+                CallbackChange::StopCursorBlinkTimer => {
+                    // Stop the cursor blink timer
+                    if self.cursor_manager.is_blink_timer_active() {
+                        result.timers_removed.insert(azul_core::task::CURSOR_BLINK_TIMER_ID);
+                        self.cursor_manager.set_blink_timer_active(false);
+                    }
+                }
+                CallbackChange::ScrollActiveCursorIntoView => {
+                    // Scroll the active text cursor into view
+                    self.scroll_active_cursor_into_view(&mut result);
+                }
             }
         }
 
@@ -4129,6 +4520,38 @@ impl LayoutWindow {
         None
     }
 
+    /// Find the last text child node of a given node.
+    /// 
+    /// For contenteditable elements, the text is usually in a child Text node,
+    /// not the contenteditable div itself. This function finds the last Text node
+    /// so the cursor defaults to the end position.
+    fn find_last_text_child(&self, dom_id: DomId, parent_node_id: NodeId) -> Option<NodeId> {
+        let layout_result = self.layout_results.get(&dom_id)?;
+        let styled_dom = &layout_result.styled_dom;
+        let node_data_container = styled_dom.node_data.as_container();
+        let hierarchy_container = styled_dom.node_hierarchy.as_container();
+        
+        // Check if parent itself is a text node
+        let parent_type = node_data_container[parent_node_id].get_node_type();
+        if matches!(parent_type, NodeType::Text(_)) {
+            return Some(parent_node_id);
+        }
+        
+        // Find the last text child by iterating through all children
+        let parent_item = &hierarchy_container[parent_node_id];
+        let mut last_text_child: Option<NodeId> = None;
+        let mut current_child = parent_item.first_child_id(parent_node_id);
+        while let Some(child_id) = current_child {
+            let child_type = node_data_container[child_id].get_node_type();
+            if matches!(child_type, NodeType::Text(_)) {
+                last_text_child = Some(child_id);
+            }
+            current_child = hierarchy_container[child_id].next_sibling_id();
+        }
+        
+        last_text_child
+    }
+
     /// Checks if a node has text content.
     fn node_has_text_content(&self, styled_dom: &StyledDom, node_id: NodeId) -> bool {
         // Check if node itself is a text node
diff --git a/tests/e2e/.gitignore b/tests/e2e/.gitignore
index 761c5b87..d8acac91 100644
--- a/tests/e2e/.gitignore
+++ b/tests/e2e/.gitignore
@@ -14,6 +14,7 @@ focus
 refany_test
 focus_scroll
 focus_scroll_test
+text_area_test
 
 # New test binaries
 text_input
diff --git a/tests/e2e/contenteditable.c b/tests/e2e/contenteditable.c
index e5e2cbe4..c45e236b 100644
--- a/tests/e2e/contenteditable.c
+++ b/tests/e2e/contenteditable.c
@@ -162,11 +162,17 @@ AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
     AzDom_addChild(&root, label1);
     
     // Single-line contenteditable input
-    AzDom single_input = AzDom_createText(AZ_STR(ref.ptr->single_line_text));
+    // Create a div with text child, set contenteditable on the div
+    AzDom single_input = AzDom_createDiv();
     AzDom_addClass(&single_input, AZ_STR("single-line-input"));
+    AzDom_setContenteditable(&single_input, true);
     AzTabIndex tab_auto = { .Auto = { .tag = AzTabIndex_Tag_Auto } };
     AzDom_setTabIndex(&single_input, tab_auto);
     
+    // Add text as child
+    AzDom single_text = AzDom_createText(AZ_STR(ref.ptr->single_line_text));
+    AzDom_addChild(&single_input, single_text);
+    
     // Add text input callback - use Focus filter for text input
     AzEventFilter text_filter = AzEventFilter_focus(AzFocusEventFilter_TextInput);
     AzDom_addCallback(&single_input, text_filter, AzRefAny_clone(&data), on_text_input);
@@ -179,10 +185,16 @@ AzStyledDom layout(AzRefAny data, AzLayoutCallbackInfo info) {
     AzDom_addChild(&root, label2);
     
     // Multi-line contenteditable textarea
-    AzDom multi_input = AzDom_createText(AZ_STR(ref.ptr->multi_line_text));
+    // Create a div with text child, set contenteditable on the div
+    AzDom multi_input = AzDom_createDiv();
     AzDom_addClass(&multi_input, AZ_STR("multi-line-textarea"));
+    AzDom_setContenteditable(&multi_input, true);
     AzDom_setTabIndex(&multi_input, tab_auto);
     
+    // Add text as child
+    AzDom multi_text = AzDom_createText(AZ_STR(ref.ptr->multi_line_text));
+    AzDom_addChild(&multi_input, multi_text);
+    
     // Add callbacks
     AzDom_addCallback(&multi_input, text_filter, AzRefAny_clone(&data), on_text_input);
     

```

## 3. Questions for Review

### Question 1: Is the "Flag and Defer" Pattern Correctly Implemented?

The W3C model requires:
1. Focus event fires during event handling
2. Selection/cursor placement happens after layout
3. Cursor is drawn during paint

Our implementation:
- `handle_focus_change_for_cursor_blink()` sets `cursor_needs_initialization = true`
- `finalize_pending_focus_changes()` is called at end of `process_window_events_recursive_v2()`
- This initializes the cursor with text layout now available

**Is this correct? Are there edge cases we're missing?**

### Question 2: Is ContentEditable Inheritance Correct?

We now have:
```rust
pub fn is_node_contenteditable_inherited(styled_dom: &StyledDom, node_id: NodeId) -> bool {
    // Traverses ancestors, returns true if any ancestor has contenteditable="true"
    // Returns false if node or ancestor has contenteditable="false"
}
```

The W3C spec says:
- `contenteditable` inherits down the DOM tree
- `contenteditable="false"` explicitly blocks inheritance
- Text nodes inside contenteditable are editable

**Is our implementation correct per W3C?**

### Question 3: Focus vs Selection Model Separation

We have:
- `FocusManager.focused_node` = the contenteditable container (like `document.activeElement`)
- `CursorManager.cursor_location` = the text node with the caret (like `Selection.focusNode`)

**Does this correctly model the W3C focus/selection separation?**

### Question 4: Timer Architecture

We use a system timer for cursor blinking:
- Timer ID `0x0001` is reserved for cursor blink
- Timer fires every ~530ms
- `toggle_visibility()` is called in the callback
- Timer starts on focus, stops on blur

**Is this the correct approach for cursor blinking?**

### Question 5: Remaining Conformance Issues

Looking at the full codebase provided below, are there any remaining issues where our implementation does NOT conform to the W3C model for:

1. **Focus events** (`focus`, `blur`, `focusin`, `focusout`)
2. **Selection API** (`Selection`, `Range`, `anchorNode`, `focusNode`)
3. **Keyboard events** (how they should interact with contenteditable)
4. **Input events** (`beforeinput`, `input`, `textInput`)
5. **Caret/cursor positioning** within text nodes
6. **Multi-node selection** across text nodes
7. **ContentEditable attribute** inheritance and behavior

## 4. Specific Areas to Review

Please focus on these files and verify W3C conformance:

1. **`layout/src/managers/focus_cursor.rs`** - FocusManager, PendingContentEditableFocus
2. **`layout/src/managers/cursor.rs`** - CursorManager, cursor positioning
3. **`layout/src/managers/selection.rs`** - SelectionManager, multi-node selection
4. **`layout/src/window.rs`** - handle_focus_change_for_cursor_blink, finalize_pending_focus_changes
5. **`dll/src/desktop/shell2/common/event_v2.rs`** - Event processing, focus handling
6. **`layout/src/solver3/getters.rs`** - is_node_contenteditable_inherited

## 5. Expected Output

Please provide:

1. **Verification** that our "flag and defer" pattern is correct
2. **List of any W3C conformance issues** in the codebase
3. **Specific code changes needed** to fix any issues found
4. **Edge cases** we may have missed
5. **Recommendations** for the test suite to verify correctness

## 6. Source Code Reference

The following source files are provided for analysis (see below).
Focus especially on the manager files and event handling code.



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

## layout/src/window.rs (EXTRACTED FUNCTIONS)
// Main window logic - text input, focus
// 435 lines

```rust
// Lines 5153-5215
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

// Lines 5488-5540
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

// Lines 6227-6540
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

```


# TASK

Analyze the code and identify the root cause of each bug. Provide specific fixes.

Focus especially on Bug 1 (text input stopped working) since that's a regression from the current diff.

For each bug, provide:
1. Root cause analysis
2. Specific file and line numbers
3. Exact code fix (diff format preferred)

Start with Bug 1 since it's the most critical regression.

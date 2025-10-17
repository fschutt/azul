# ScrollManager Integration Example

This document shows how to integrate the new `ScrollManager` with the window event loop.

## Frame Processing Flow

```rust
// In window.rs or similar event loop

impl Window {
    pub fn process_frame(&mut self, now: Instant) {
        // 1. Begin frame - reset per-frame flags
        self.scroll_manager.begin_frame();
        
        // 2. Process any pending scroll events (from mouse wheel, etc.)
        for event in self.pending_scroll_events.drain(..) {
            self.scroll_manager.process_scroll_event(event, now.clone());
        }
        
        // 3. Update animations and check IFrame conditions
        let tick_result = self.scroll_manager.tick(now.clone());
        
        // 4. Check what happened this frame
        let frame_info = self.scroll_manager.end_frame();
        
        // 5. Decide what actions to take based on frame_info
        
        // If scroll positions changed, rebuild hit-test
        if self.scroll_manager.needs_hit_test_rebuild() {
            self.rebuild_hit_test();
        }
        
        // If we should check IFrame callbacks
        if self.scroll_manager.should_check_iframe_callbacks() {
            for (dom_id, node_id, reason) in tick_result.iframes_to_update {
                self.invoke_iframe_callback(dom_id, node_id, reason, now.clone());
            }
        }
        
        // If animations are running or scroll happened, repaint
        if tick_result.needs_repaint || frame_info.had_scroll_activity {
            self.request_repaint();
        }
    }
    
    // Mouse wheel event handler
    pub fn on_mouse_wheel(&mut self, scroll_amount: f32, now: Instant) {
        // Find which node was hit by the mouse
        let hit_node = self.hit_test(self.mouse_position);
        
        if let Some((dom_id, node_id)) = hit_node {
            // Process wheel event immediately
            self.scroll_manager.process_wheel_event(
                dom_id,
                node_id,
                scroll_amount,
                120.0, // pixels per detent
                false, // vertical scroll
                now,
            );
        }
    }
    
    // Programmatic scroll API
    pub fn scroll_to(&mut self, dom_id: DomId, node_id: NodeId, target: LogicalPosition) {
        use azul_core::task::Duration;
        use azul_layout::scroll::{EasingFunction, ScrollEvent, ScrollSource};
        
        #[cfg(feature = "std")]
        let now = Instant::System(std::time::Instant::now().into());
        #[cfg(not(feature = "std"))]
        let now = Instant::Tick(SystemTick { tick_counter: 0 });
        
        let current = self.scroll_manager
            .get_current_offset(dom_id, node_id)
            .unwrap_or_default();
        
        let delta = LogicalPosition {
            x: target.x - current.x,
            y: target.y - current.y,
        };
        
        let event = ScrollEvent {
            dom_id,
            node_id,
            delta,
            source: ScrollSource::Programmatic,
            duration: Some(Duration::System(
                std::time::Duration::from_millis(300).into()
            )),
            easing: EasingFunction::EaseOut,
        };
        
        self.scroll_manager.process_scroll_event(event, now);
    }
}
```

## Integration with IFrame Callbacks

```rust
impl Window {
    fn invoke_iframe_callback(
        &mut self,
        parent_dom_id: DomId,
        iframe_node_id: NodeId,
        reason: IFrameCallbackReason,
        now: Instant,
    ) {
        // Get current bounds and scroll info
        let bounds = self.get_node_bounds(parent_dom_id, iframe_node_id);
        let scroll_offset = self.scroll_manager
            .get_current_offset(parent_dom_id, iframe_node_id)
            .unwrap_or_default();
        
        // Create IFrameCallbackInfo with reason
        let callback_info = IFrameCallbackInfo {
            system: self.get_system_info(),
            bounds,
            scroll_offset,
            reason, // NEW: Pass the reason for re-invocation
            // ... other fields
        };
        
        // Invoke the callback
        let result = (self.iframe_callback)(callback_info);
        
        match result.dom {
            OptionStyledDom::Some(new_dom) => {
                // Update the DOM
                self.update_iframe_dom(parent_dom_id, iframe_node_id, new_dom);
                
                // Update scroll manager with new sizes
                self.scroll_manager.update_iframe_scroll_info(
                    parent_dom_id,
                    iframe_node_id,
                    result.scroll_size,
                    result.virtual_scroll_size,
                    now.clone(),
                );
                
                // Mark as invoked
                self.scroll_manager.mark_iframe_invoked(
                    parent_dom_id,
                    iframe_node_id,
                    reason,
                );
                
                // Trigger layout and repaint
                self.needs_layout = true;
            }
            OptionStyledDom::None => {
                // Callback said "no update needed"
                // Still mark as invoked to prevent repeated calls
                self.scroll_manager.mark_iframe_invoked(
                    parent_dom_id,
                    iframe_node_id,
                    reason,
                );
            }
        }
    }
}
```

## Frame Decision Logic

Based on `FrameScrollInfo`, the window can decide what to do:

```rust
let frame_info = self.scroll_manager.end_frame();

// Decision tree:
if frame_info.had_new_doms {
    // New IFrames were added - invoke them for InitialRender
    self.invoke_new_iframe_callbacks();
}

if frame_info.had_scroll_activity {
    // Scroll happened - check edge conditions
    if self.scroll_manager.should_check_iframe_callbacks() {
        // Check tick_result for iframes_to_update
    }
    
    // Rebuild hit-test with new scroll positions
    self.rebuild_hit_test();
}

if frame_info.had_programmatic_scroll {
    // Programmatic scroll - may need special handling
    // (e.g., notify external systems)
}

if self.scroll_manager.needs_repaint() {
    // Animations are running - keep rendering
    self.request_repaint();
}
```

## Per-Frame Checklist

Every frame, the window should:

1. ✅ **Begin frame** - `scroll_manager.begin_frame()`
2. ✅ **Process events** - Handle mouse wheel, keyboard, touch
3. ✅ **Tick animations** - `scroll_manager.tick(now)`
4. ✅ **Check IFrames** - Process `iframes_to_update` list
5. ✅ **End frame** - `scroll_manager.end_frame()` to get info
6. ✅ **Decide actions** - Repaint? Hit-test rebuild? Layout?

## Mouse Wheel Event Flow

```
User scrolls wheel
    ↓
WM_MOUSEWHEEL (Windows) / scrollWheel: (macOS)
    ↓
wm_mousewheel() in event.rs
    ↓
scroll_manager.process_wheel_event()
    ↓
ScrollState updated with new offset
    ↓
had_scroll_activity = true
    ↓
needs_hit_test_rebuild() = true
    ↓
Window rebuilds hit-test
    ↓
Window requests repaint
```

## Programmatic Scroll Flow

```
API call: window.scroll_to(dom, node, position)
    ↓
Create ScrollEvent with ScrollSource::Programmatic
    ↓
scroll_manager.process_scroll_event()
    ↓
Creates ScrollAnimation with easing
    ↓
had_scroll_activity = true
had_programmatic_scroll = true
    ↓
Each frame: tick() interpolates position
    ↓
needs_repaint() = true while animating
    ↓
Window keeps rendering until animation done
```

## Benefits

- **Separation of Concerns**: Scroll logic in ScrollManager, window just orchestrates
- **Predictable State**: Per-frame flags make it clear what happened
- **Easy Testing**: Can test scroll logic without window/event system
- **Flexible**: Easy to add new scroll sources or behaviors
- **IFrame Integration**: Automatic edge detection and re-invocation

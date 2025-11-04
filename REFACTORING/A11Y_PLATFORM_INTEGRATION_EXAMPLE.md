# Accessibility Platform Integration Example

This document shows **exactly** how to integrate `record_accessibility_action()` into platform event loops.

## Overview

The integration is simple:

1. **Receive action** from screen reader (via callback)
2. **Call `record_accessibility_action()`** to get synthetic EventFilters
3. **Store EventFilters** to merge with regular events
4. **Pass to event processing** which dispatches callbacks

## macOS Integration Example

### Step 1: Add Storage for Pending Actions

```rust
// In dll/src/desktop/shell2/macos/mod.rs

pub struct MacOSWindow {
    // ... existing fields ...
    
    #[cfg(feature = "accessibility")]
    /// Pending accessibility actions received from VoiceOver
    pending_a11y_actions: Vec<(azul_core::dom::DomId, azul_core::dom::NodeId, azul_core::dom::AccessibilityAction)>,
    
    #[cfg(feature = "accessibility")]
    /// Synthetic EventFilters generated from accessibility actions
    pending_synthetic_events: Vec<(azul_core::dom::DomId, azul_core::dom::NodeId, azul_core::events::EventFilter)>,
}
```

### Step 2: Setup Callback-Based Adapter

```rust
impl MacOSWindow {
    /// Initialize accessibility with callback-based action handling
    #[cfg(feature = "accessibility")]
    fn setup_accessibility(&mut self) {
        use accesskit_macos::{Adapter, ActionHandler};
        
        // Create adapter with action handler callback
        let window_id = self.window_id; // or similar unique identifier
        
        // IMPORTANT: Use weak reference to avoid retain cycle
        let action_handler = ActionHandler::new(move |node_id, action| {
            // Convert accesskit types to azul types
            let (dom_id, azul_node_id) = parse_accesskit_node_id(node_id);
            let azul_action = convert_accesskit_action(action);
            
            // Send action to main thread via channel or queue
            // Platform-specific: could use NSNotificationCenter or similar
            send_accessibility_action_to_main_thread(window_id, dom_id, azul_node_id, azul_action);
        });
        
        self.accessibility_adapter = Some(Adapter::new(
            self.window.clone(),
            action_handler,
            /* initial_tree */ TreeUpdate::default(),
        ));
    }
    
    /// Process pending accessibility actions (call from event loop)
    #[cfg(feature = "accessibility")]
    fn process_pending_a11y_actions(&mut self) {
        // Take pending actions (drains the queue)
        let actions = std::mem::take(&mut self.pending_a11y_actions);
        
        if actions.is_empty() {
            return;
        }
        
        // Process each action via record_accessibility_action
        for (dom_id, node_id, action) in actions {
            let synthetic_events = self.record_accessibility_action(dom_id, node_id, action);
            
            // Store events with their target node
            for event_filter in synthetic_events {
                self.pending_synthetic_events.push((dom_id, node_id, event_filter));
            }
        }
    }
}
```

### Step 3: Integrate Into Event Loop

```rust
// In dll/src/desktop/shell2/macos/events.rs or similar

impl MacOSWindow {
    /// Main event handler called by NSApplication
    pub fn handle_event(&mut self, event: &NSEvent) -> ProcessEventResult {
        // STEP 1: Process any pending accessibility actions FIRST
        // This happens before regular event processing
        #[cfg(feature = "accessibility")]
        self.process_pending_a11y_actions();
        
        // STEP 2: Update window state from platform event (existing code)
        match event.type_() {
            NSEventType::LeftMouseDown => {
                self.current_window_state.mouse.left_down = true;
                self.update_hit_test();
            }
            NSEventType::KeyDown => {
                let key = translate_ns_key(event);
                self.current_window_state.keyboard_state.pressed_virtual_keycodes.insert(key);
            }
            // ... other event types ...
        }
        
        // STEP 3: Process events (regular + synthetic)
        self.process_window_events_recursive_v2(0)
    }
}
```

### Step 4: Extend Event Detection to Include Synthetic Events

This is the **critical part** - merging synthetic events with regular events.

**Option A: Store in LayoutWindow (RECOMMENDED)**

```rust
// In layout/src/window.rs

pub struct LayoutWindow {
    // ... existing fields ...
    
    #[cfg(feature = "accessibility")]
    /// Synthetic EventFilters from accessibility actions
    /// These are merged with regular events during event detection
    pub pending_synthetic_events: Vec<(DomId, NodeId, EventFilter)>,
}
```

**In event_v2.rs:**

```rust
fn process_window_events_recursive_v2(&mut self, depth: usize) -> ProcessEventResult {
    // ... existing code ...
    
    // Detect all events from state comparison
    let mut events = window_state::create_events_from_states_with_gestures(
        current_state, previous_state, ...
    );
    
    // Merge synthetic accessibility events
    #[cfg(feature = "accessibility")]
    if let Some(layout_window) = self.get_layout_window_mut() {
        let synthetic_events = std::mem::take(&mut layout_window.pending_synthetic_events);
        
        for (dom_id, node_id, event_filter) in synthetic_events {
            // Add to appropriate event list based on event type
            match event_filter {
                EventFilter::Hover(hover_event) => {
                    events.hover_events.push(hover_event);
                }
                EventFilter::Window(window_event) => {
                    events.window_events.push(window_event);
                }
                EventFilter::Focus(focus_event) => {
                    events.focus_events.push(focus_event);
                }
                _ => {}
            }
        }
    }
    
    // Continue with normal dispatch...
    let dispatch_result = dispatch_events(&events, hit_test);
    // ... rest of function ...
}
```

**Option B: Pass as Parameter**

Alternatively, extend `create_events_from_states_with_gestures()` to accept synthetic events:

```rust
// In layout/src/window_state.rs

pub fn create_events_from_states_with_gestures(
    current: &FullWindowState,
    previous: &FullWindowState,
    // ... other params ...
    synthetic_events: Vec<(DomId, NodeId, EventFilter)>, // NEW
) -> Events {
    let mut events = Events::default();
    
    // Regular event detection
    // ...
    
    // Add synthetic events
    for (dom_id, node_id, event_filter) in synthetic_events {
        match event_filter {
            EventFilter::Hover(hover_event) => events.hover_events.push(hover_event),
            EventFilter::Window(window_event) => events.window_events.push(window_event),
            EventFilter::Focus(focus_event) => events.focus_events.push(focus_event),
            _ => {}
        }
    }
    
    events
}
```

Then call it with synthetic events:

```rust
let synthetic = std::mem::take(&mut self.get_layout_window_mut()?.pending_synthetic_events);

let events = window_state::create_events_from_states_with_gestures(
    current_state,
    previous_state,
    focus_manager,
    // ... other params ...
    synthetic, // Pass synthetic events
);
```

## Windows Integration Example

### UI Automation Provider

```rust
// In dll/src/desktop/shell2/windows/mod.rs

use windows::Win32::UI::Accessibility::{
    IUIAutomationElement, IUIAutomationInvokePattern,
    UIA_InvokePatternId, VARIANT,
};

pub struct Win32Window {
    // ... existing fields ...
    
    #[cfg(feature = "accessibility")]
    pending_a11y_actions: Vec<(DomId, NodeId, AccessibilityAction)>,
    
    #[cfg(feature = "accessibility")]
    uia_provider: Option<Box<dyn IUIAutomationElement>>,
}

impl Win32Window {
    #[cfg(feature = "accessibility")]
    fn setup_accessibility(&mut self) {
        // Create UI Automation provider
        let provider = UIAProvider::new(self.hwnd);
        
        // Register action handlers
        provider.on_invoke(|node_id| {
            // Convert to azul action
            let (dom_id, azul_node_id) = parse_uia_node_id(node_id);
            let action = AccessibilityAction::Default;
            
            // Queue for processing
            send_to_main_thread(dom_id, azul_node_id, action);
        });
        
        self.uia_provider = Some(Box::new(provider));
    }
    
    #[cfg(feature = "accessibility")]
    fn process_pending_a11y_actions(&mut self) {
        let actions = std::mem::take(&mut self.pending_a11y_actions);
        
        for (dom_id, node_id, action) in actions {
            let synthetic_events = self.record_accessibility_action(dom_id, node_id, action);
            // Store for merging...
        }
    }
}

// In WndProc:
extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_GETOBJECT => {
            // Return UI Automation provider
            // ...
        }
        _ => {
            // Regular event processing
            #[cfg(feature = "accessibility")]
            window.process_pending_a11y_actions();
            
            // Update window state from message
            // ...
            
            window.process_window_events_recursive_v2(0);
        }
    }
}
```

## Linux Integration Example

### AT-SPI2 DBus Interface

```rust
// In dll/src/desktop/shell2/linux/x11/mod.rs

pub struct X11Window {
    // ... existing fields ...
    
    #[cfg(feature = "accessibility")]
    pending_a11y_actions: Vec<(DomId, NodeId, AccessibilityAction)>,
    
    #[cfg(feature = "accessibility")]
    atspi_connection: Option<atspi::Connection>,
}

impl X11Window {
    #[cfg(feature = "accessibility")]
    fn setup_accessibility(&mut self) {
        use atspi::{Connection, Interface};
        
        // Connect to AT-SPI2 bus
        let connection = Connection::new().expect("Failed to connect to AT-SPI2");
        
        // Register action interface
        connection.register_interface(Interface::Action, |node_id, action_name| {
            let (dom_id, azul_node_id) = parse_atspi_node_id(node_id);
            let action = parse_atspi_action(action_name);
            
            // Queue for processing
            send_to_main_thread(dom_id, azul_node_id, action);
        });
        
        self.atspi_connection = Some(connection);
    }
    
    #[cfg(feature = "accessibility")]
    fn process_pending_a11y_actions(&mut self) {
        let actions = std::mem::take(&mut self.pending_a11y_actions);
        
        for (dom_id, node_id, action) in actions {
            let synthetic_events = self.record_accessibility_action(dom_id, node_id, action);
            // Store for merging...
        }
    }
}

// In event loop:
pub fn poll_events(&mut self) -> ProcessEventResult {
    // Process accessibility actions first
    #[cfg(feature = "accessibility")]
    self.process_pending_a11y_actions();
    
    // Process X11 events
    while let Some(event) = self.xlib.XPending(self.display) {
        // ... handle X11 event ...
    }
    
    // Process all events together
    self.process_window_events_recursive_v2(0)
}
```

## Testing Strategy

### Test with Screen Readers

**macOS (VoiceOver):**
```bash
# Enable VoiceOver
sudo defaults write com.apple.VoiceOver voiceOverOnOffKey -bool true

# Run your app
cargo run --features accessibility

# Test actions:
# - Navigate with VO+Right Arrow
# - Activate with VO+Space
# - Increment with VO+Up Arrow
# - Scroll with VO+Option+PageDown
```

**Windows (NVDA):**
```powershell
# Install NVDA
# Run your app
cargo run --features accessibility

# Test actions:
# - Navigate with Arrow keys
# - Activate with Enter/Space
# - Use application-specific commands
```

**Linux (Orca):**
```bash
# Install Orca
sudo apt install orca

# Run your app
cargo run --features accessibility

# Test actions:
# - Navigate with Arrow keys  
# - Activate with Enter/Space
# - Use Orca commands (Insert+H for help)
```

### Debug Output

Add debug logging to trace action flow:

```rust
#[cfg(feature = "accessibility")]
fn record_accessibility_action(
    &mut self,
    dom_id: DomId,
    node_id: NodeId,
    action: AccessibilityAction,
) -> Vec<EventFilter> {
    eprintln!("[A11Y] Recording action: {:?} on dom={:?} node={:?}", 
              action, dom_id, node_id);
    
    let events = self.get_layout_window_mut()
        .unwrap()
        .process_accessibility_action(dom_id, node_id, action, now);
    
    eprintln!("[A11Y] Generated {} synthetic events", events.len());
    for event in &events {
        eprintln!("  - {:?}", event);
    }
    
    events
}
```

## Common Issues

### Issue 1: Actions Not Processed

**Symptom:** Screen reader sends action, but nothing happens

**Debug:**
1. Check if `pending_a11y_actions` is populated
2. Verify `process_pending_a11y_actions()` is called
3. Check if synthetic events are generated
4. Verify events are merged into dispatch

**Solution:** Add debug logging at each step to trace action flow

### Issue 2: Callbacks Not Invoked

**Symptom:** Synthetic events generated, but callbacks don't fire

**Debug:**
1. Check if EventFilters are merged into `events` struct
2. Verify `dispatch_events()` receives merged events
3. Check if hit test is available (needed for node targeting)
4. Verify callback is registered on target node

**Solution:** Ensure EventFilters have correct event type and target node

### Issue 3: State Changes Not Visible

**Symptom:** Manager state changes, but UI doesn't update

**Debug:**
1. Check if `mark_frame_needs_regeneration()` is called
2. Verify ProcessEventResult triggers redraw
3. Check if accessibility tree is updated after state change

**Solution:** Ensure proper result propagation and tree updates

## Summary

**Key Points:**

1. **Callback-based** - No polling, immediate response
2. **Simple API** - Just like `record_input_sample()`
3. **Direct manager access** - No indirection
4. **Event merging** - Synthetic events merge with regular events
5. **Unified processing** - Same dispatch logic for all events

**Integration Steps:**

1. Add storage for pending actions
2. Setup callback-based adapter
3. Call `record_accessibility_action()` on actions
4. Store synthetic EventFilters
5. Merge into event detection
6. Process through normal dispatch

**Testing:**

1. Enable screen reader
2. Navigate and activate elements
3. Verify actions trigger callbacks
4. Check manager state changes
5. Confirm UI updates properly

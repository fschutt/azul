# W3C-Conformant Text Input Architecture Review Request

## 1. Executive Summary: Current Problems

We're implementing contenteditable text input in Azul (a Rust GUI framework). After Tab-focusing on a contenteditable `<div>`, the **Debug API reports**:

```json
{
  "has_focus": true,
  "focused_node": {
    "dom_id": 0,
    "node_id": 3,
    "is_contenteditable": true,
    "text_content": "Initial text here"
  }
}
```

But the **cursor state** returns:

```json
{
  "has_cursor": false,
  "cursor": null
}
```

**Key Problems:**
1. **Cursor not initializing** - After Tab focus lands on contenteditable, `CursorManager.cursor` stays `None`
2. **`:focus` CSS styling not rendering** - The blue border from `:focus` pseudo-class doesn't appear
3. **Timer action not being applied** - `handle_focus_change_for_cursor_blink()` returns `CursorBlinkTimerAction::Start(timer)` but timer doesn't start

## 2. Current Architecture Overview

### DOM Structure

```
div.single-line-input (contenteditable=true, tabindex=Auto)
├── Text("Initial text here")  ← Cursor should go here
```

The `contenteditable` attribute is on the **parent div**, but the cursor needs to be placed in the **child Text node**.

### Key Managers

| Manager | Purpose |
|---------|---------|
| `FocusManager` | Tracks which `DomNodeId` has keyboard focus |
| `CursorManager` | Tracks cursor position (`TextCursor`) and location (`CursorLocation`) |
| `SelectionManager` | Tracks text selections across nodes |
| `TextInputManager` | Records pending text input operations |
| `ScrollManager` | Handles scroll positions |

### Focus Change Flow (Current Implementation)

```
Tab Key Pressed
    ↓
determine_keyboard_default_action() → DefaultAction::FocusNext
    ↓
resolve_focus_target() → finds next focusable node
    ↓
focus_manager.set_focused_node(new_focus)
    ↓
handle_focus_change_for_cursor_blink(new_focus, window_state)
    ↓
├── Checks: is_node_contenteditable_internal(new_focus)
├── Calls: find_last_text_child() → gets text node
├── Calls: cursor_manager.initialize_cursor_at_end()
├── Returns: CursorBlinkTimerAction::Start(timer)
    ↓
Platform calls start_timer() ← THIS MAY NOT BE HAPPENING
```

## 3. Code Context

### handle_focus_change_for_cursor_blink (layout/src/window.rs)

```rust
pub fn handle_focus_change_for_cursor_blink(
    &mut self,
    new_focus: Option<DomNodeId>,
    current_window_state: &FullWindowState,
) -> CursorBlinkTimerAction {
    
    eprintln!("[DEBUG] handle_focus_change_for_cursor_blink called with new_focus={:?}", new_focus);
    
    // Check if the new focus is on a contenteditable element
    let is_new_focus_contenteditable = match new_focus {
        Some(focus_node) => {
            if let Some(node_id) = focus_node.node.into_crate_internal() {
                self.is_node_contenteditable_internal(focus_node.dom, node_id)
            } else {
                false
            }
        }
        None => false,
    };
    
    if is_new_focus_contenteditable {
        let focus_node = new_focus.unwrap();
        let contenteditable_node_id = focus_node.node.into_crate_internal().unwrap();
        
        // Find the last text child node of the contenteditable element
        let text_node_id = self.find_last_text_child(focus_node.dom, contenteditable_node_id)
            .unwrap_or(contenteditable_node_id);
        
        // Initialize cursor at end of text
        let text_layout = self.get_inline_layout_for_node(focus_node.dom, text_node_id).cloned();
        let cursor_initialized = self.cursor_manager.initialize_cursor_at_end(
            focus_node.dom,
            text_node_id,
            text_layout.as_ref(),
        );
        
        // Make cursor visible and record current time
        let now = azul_core::task::Instant::now();
        self.cursor_manager.reset_blink_on_input(now);
        
        if !timer_was_active {
            let timer = self.create_cursor_blink_timer(current_window_state);
            self.cursor_manager.set_blink_timer_active(true);
            return CursorBlinkTimerAction::Start(timer);
        }
    }
    // ... clear cursor for non-contenteditable
}
```

### Tab Focus Processing (event_v2.rs)

```rust
// Inside process_window_events_recursive_v2
match &default_action_result.action {
    DefaultAction::FocusNext | DefaultAction::FocusPrevious => {
        let focus_target = default_action_to_focus_target(&default_action_result.action);
        let new_focus_node = resolve_focus_target(&focus_target, layout_results, focused_node);
        
        let timer_action = if let Some(layout_window) = self.get_layout_window_mut() {
            layout_window.focus_manager.set_focused_node(new_focus_node);
            
            // CURSOR BLINK TIMER: Start/stop timer based on contenteditable focus
            let window_state = layout_window.current_window_state.clone();
            let timer_action = layout_window.handle_focus_change_for_cursor_blink(
                new_focus_node,
                &window_state,
            );
            
            // RESTYLE: Update StyledNodeState for CSS changes
            if old_focus_node_id != new_focus_node_id {
                let restyle_result = apply_focus_restyle(layout_window, old_focus_node_id, new_focus_node_id);
                result = result.max(restyle_result);
            }
            
            Some(timer_action)
        } else {
            None
        };
        
        // Apply timer action
        if let Some(timer_action) = timer_action {
            match timer_action {
                CursorBlinkTimerAction::Start(timer) => {
                    self.start_timer(CURSOR_BLINK_TIMER_ID.id, timer);
                }
                CursorBlinkTimerAction::Stop => {
                    self.stop_timer(CURSOR_BLINK_TIMER_ID.id);
                }
                CursorBlinkTimerAction::NoChange => {}
            }
        }
    }
}
```

### apply_focus_restyle (event_v2.rs)

```rust
fn apply_focus_restyle(
    layout_window: &mut LayoutWindow,
    old_focus: Option<NodeId>,
    new_focus: Option<NodeId>,
) -> ProcessEventResult {
    use azul_core::styled_dom::FocusChange;
    
    let Some((_, layout_result)) = layout_window.layout_results.iter_mut().next() else {
        return ProcessEventResult::ShouldReRenderCurrentWindow;
    };
    
    let restyle_result = layout_result.styled_dom.restyle_on_state_change(
        Some(FocusChange {
            lost_focus: old_focus,
            gained_focus: new_focus,
        }),
        None, // hover
        None, // active
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
```

## 4. Questions for W3C Conformance Review

### Question 1: Event Target vs Original Target

In the W3C DOM Events specification, there's a distinction between:
- **`event.target`**: The node that dispatched the event (may be the focused node itself)
- **`event.originalTarget`** (Firefox): The node that originally received the event before bubbling

In our implementation:
- Focus lands on the **contenteditable div** (node_id=3)
- But cursor should be placed in the **child Text node** (node_id=4)

**How should this be handled according to W3C?**
- Should focus events bubble from Text → contenteditable parent?
- Or should the focus manager track the Text node, not the contenteditable container?

### Question 2: Focus vs Selection Model

The W3C defines the Selection API:
```webidl
interface Selection {
  readonly attribute Node? anchorNode;
  readonly attribute unsigned long anchorOffset;
  readonly attribute Node? focusNode;
  readonly attribute unsigned long focusOffset;
  // ...
};
```

**Questions:**
1. Should `Selection.focusNode` point to the **Text node** with the cursor, or the **contenteditable container**?
2. Is it correct to have separate `FocusManager` (keyboard focus) and `CursorManager` (text cursor)?
3. How does the W3C model handle the case where the contenteditable contains multiple text nodes?

### Question 3: ContentEditable Attribute Inheritance

Per HTML5 spec, `contenteditable` is inherited. Our test has:

```html
<div contenteditable="true">
  Text content here
</div>
```

The Text node doesn't have `contenteditable`, but it inherits from parent.

**Questions:**
1. Should `is_node_contenteditable()` check the node itself or traverse to editable ancestor?
2. When focusing, should we focus the contenteditable ancestor or the innermost text node?

### Question 4: Cursor Initialization Timing

Current flow:
1. `set_focused_node()` is called
2. Then `handle_focus_change_for_cursor_blink()` is called
3. Inside that, `initialize_cursor_at_end()` is called

**But our debug shows `has_cursor: false`.**

Possible causes:
- `find_last_text_child()` returns `None`?
- `get_inline_layout_for_node()` returns `None`?
- `initialize_cursor_at_end()` fails silently?

**Question:** In the W3C model, when exactly should the cursor (caret) be created after focus changes?

### Question 5: Event Bubbling for Focus

The DOM Events spec says:
- `focus` and `blur` events do NOT bubble
- `focusin` and `focusout` events DO bubble

Our `FocusEventFilter` enum has:
```rust
pub enum FocusEventFilter {
    Focus,
    Blur,
    FocusIn,
    FocusOut,
    TextInput,
}
```

**Questions:**
1. Are we correctly dispatching both `Focus` (non-bubbling) and `FocusIn` (bubbling)?
2. Should the cursor be initialized during `Focus` or `FocusIn` event handling?

## 5. Test Case: contenteditable.c

```c
// Create a div with contenteditable attribute, text as child
AzDom single_input = AzDom_createDiv();
AzDom_addClass(&single_input, AZ_STR("single-line-input"));
AzDom_setContenteditable(&single_input, true);
AzTabIndex tab_auto = { .Auto = { .tag = AzTabIndex_Tag_Auto } };
AzDom_setTabIndex(&single_input, tab_auto);

// Add text as child
AzDom single_text = AzDom_createText(AZ_STR("Initial text here"));
AzDom_addChild(&single_input, single_text);
```

CSS:
```css
.single-line-input {
    font-size: 48px;
    padding: 20px;
    background-color: #2d2d2d;
    color: #ffffff;
    border: 3px solid #555555;
    min-height: 80px;
    cursor: text;
}

.single-line-input:focus {
    border-color: #0078d4;  /* Blue border when focused */
    outline: none;
}
```

## 6. Debug Output

When Tab is pressed:
```
[DEBUG] handle_focus_change_for_cursor_blink called with new_focus=Some(DomNodeId { dom: DomId { inner: 0 }, node: NodeHierarchyItemId(4) })
[DEBUG] is_new_focus_contenteditable=true, timer_was_active=false
[DEBUG] contenteditable_node_id=NodeId(3)
[DEBUG] text_node_id=NodeId(4) (was contenteditable_node_id=NodeId(3))
[DEBUG] text_layout is_some=false  ← PROBLEM: No text layout available!
[DEBUG] cursor_initialized=true
[DEBUG] Returning CursorBlinkTimerAction::Start
```

The issue: `get_inline_layout_for_node()` returns `None` because the layout hasn't been computed yet for the text node.

## 7. Specific Bug Analysis

### Bug 1: No Text Layout at Focus Time

When Tab focus happens:
1. Layout has been computed for the DOM
2. But `get_inline_layout_for_node()` returns `None`

Possible causes:
- Text layout is stored by layout node index, not DOM node ID?
- Layout results aren't accessible during event processing?
- The text node layout key is different from what we're querying?

### Bug 2: `:focus` CSS Not Rendering

The `apply_focus_restyle()` function is called, but:
- The blue border (`border-color: #0078d4`) doesn't appear
- `restyle_on_state_change()` may not be finding matching `:focus` rules

Possible causes:
- The StyledNodeState isn't being updated with focus state?
- The display list isn't being regenerated after restyle?
- `:focus` matching logic has a bug?

### Bug 3: Timer Not Starting

Even though `handle_focus_change_for_cursor_blink()` returns `Start(timer)`:
- The cursor blink timer isn't running
- Debug API shows `blink_timer_active: false`

Possible causes:
- `start_timer()` isn't being called on the platform layer?
- Timer is being immediately stopped?
- Platform timer implementation has issues?

## 8. Requested Analysis

Please provide:

1. **W3C Conformance Review**
   - How does the W3C model define contenteditable focus behavior?
   - What is the correct relationship between keyboard focus, selection, and cursor?
   - How should `originalTarget` vs `target` work for focus on contenteditable?

2. **Architecture Recommendations**
   - Should cursor initialization happen during focus or after layout?
   - What is the correct way to find the text node for cursor placement?
   - How should the system handle nested contenteditable elements?

3. **Bug Fix Recommendations**
   - Why is `get_inline_layout_for_node()` returning `None`?
   - How can we ensure the cursor is initialized even without layout?
   - What is the correct timing for `:focus` style application?

4. **Test Case Design**
   - What edge cases should E2E tests cover?
   - How to test multi-node contenteditable?
   - How to verify cursor blink timing?

## 9. Key Source Files

- `layout/src/window.rs` - LayoutWindow, handle_focus_change_for_cursor_blink, find_last_text_child
- `layout/src/managers/cursor.rs` - CursorManager, initialize_cursor_at_end
- `layout/src/managers/focus_cursor.rs` - FocusManager, resolve_focus_target
- `dll/src/desktop/shell2/common/event_v2.rs` - Tab focus processing, restyle application
- `tests/e2e/contenteditable.c` - Test case

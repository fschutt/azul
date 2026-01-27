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

## 2. Debug Output Analysis

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

## 3. Questions for W3C Conformance Review

### Question 1: Event Target vs Original Target

In the W3C DOM Events specification, there's a distinction between:
- **`event.target`**: The node that dispatched the event (may be the focused node itself)
- **`event.originalTarget`** (Firefox): The node that originally received the event before bubbling

In our implementation:
- Focus lands on the **contenteditable div** (node_id=3)
- But cursor should be placed in the **child Text node** (node_id=4)

**How should this be handled according to W3C?**

### Question 2: Focus vs Selection Model

The W3C defines the Selection API:
```webidl
interface Selection {
  readonly attribute Node? anchorNode;
  readonly attribute unsigned long anchorOffset;
  readonly attribute Node? focusNode;
  readonly attribute unsigned long focusOffset;
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

**Questions:**
1. Should `is_node_contenteditable()` check the node itself or traverse to editable ancestor?
2. When focusing, should we focus the contenteditable ancestor or the innermost text node?

### Question 4: Cursor Initialization Timing

Current flow:
1. `set_focused_node()` is called
2. Then `handle_focus_change_for_cursor_blink()` is called
3. Inside that, `initialize_cursor_at_end()` is called

**But our debug shows `has_cursor: false`.**

**Question:** In the W3C model, when exactly should the cursor (caret) be created after focus changes?

### Question 5: Event Bubbling for Focus

The DOM Events spec says:
- `focus` and `blur` events do NOT bubble
- `focusin` and `focusout` events DO bubble

**Questions:**
1. Are we correctly dispatching both `Focus` (non-bubbling) and `FocusIn` (bubbling)?
2. Should the cursor be initialized during `Focus` or `FocusIn` event handling?

## 4. Specific Bug Analysis

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

## 5. Requested Analysis

Please analyze the source code provided and give:

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

4. **Code Fix Suggestions**
   - Provide specific code changes to fix cursor initialization
   - Provide specific code changes to fix `:focus` CSS application
   - Provide specific code changes to fix timer start/stop logic

## 6. Source Code Reference

The following source files are provided for analysis (see below).
Key files to focus on:
- `layout/src/window.rs` - `handle_focus_change_for_cursor_blink()`, `find_last_text_child()`
- `layout/src/managers/cursor.rs` - `CursorManager`, `initialize_cursor_at_end()`
- `layout/src/managers/focus_cursor.rs` - `FocusManager`, `resolve_focus_target()`
- `dll/src/desktop/shell2/common/event_v2.rs` - Tab focus processing, restyle application
- `core/src/styled_dom.rs` - `restyle_on_state_change()`, `restyle_nodes_focus()`


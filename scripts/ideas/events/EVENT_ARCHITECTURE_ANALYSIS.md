# Azul Event Architecture: W3C Compliance Analysis

## Executive Summary

Azul's V2 event system uses a **state-diffing** approach: platform handlers update
`current_window_state`, then `process_window_events_recursive_v2()` compares current vs
previous state to synthesize events. This is fundamentally different from the W3C DOM Events
model (which dispatches events inline from platform handlers) but can achieve parity if
the gaps identified below are addressed.

**Overall assessment:** The scaffolding (EventPhase, propagate_event, DefaultAction, etc.)
is well-designed and W3C-aligned. However, the actual hot-path (`invoke_callbacks_v2`) does
NOT use the propagation system from `core/src/events.rs`. Instead, it manually implements
a simpler bubbling scheme. Several critical event flows are incomplete or broken.

---

## 1. Architecture Overview

### 1.1 Data Flow

```
Platform (macOS/Win/X11/Wayland)
  │
  ├──► update window_state (cursor, buttons, keys)
  ├──► record_input_sample() → GestureAndDragManager
  ├──► update_hit_test() → HoverManager.push_hit_test()
  │
  └──► process_window_events_recursive_v2(depth=0)
         │
         ├──1. determine_all_events()  → Vec<SyntheticEvent>
         ├──2. pre_callback_filter()   → split internal vs user events
         ├──3. process internal events  (text selection, shortcuts)
         ├──4. dispatch_synthetic_events() → Vec<CallbackToInvoke>
         ├──5. invoke_callbacks_v2()    → for each CallbackToInvoke
         ├──6. process_callback_result_v2() → for each result
         ├──7. post_callback_filter()   → focus, text, scroll-into-view
         ├──8. default actions          (Tab, Enter/Space, Escape)
         ├──9. focus dispatch           (FocusLost / FocusReceived)
         └─10. recurse if DOM regenerated
```

### 1.2 Two Parallel Systems

There are **two propagation systems** that are NOT connected:

| System | Location | Used? |
|--------|----------|-------|
| `propagate_event()` with Capture/Target/Bubble phases | `core/src/events.rs:762` | **NO** — never called from hot path |
| `invoke_callbacks_v2()` manual bubbling | `event_v2.rs:782` | **YES** — actual runtime path |

The `propagate_event()` system in core is complete and W3C-compliant (capture → target → bubble),
but `invoke_callbacks_v2()` completely ignores it and instead:
- Walks DOM ancestors manually for HoverEventFilter events
- Searches ALL nodes for WindowEventFilter events
- Does NOT support capture phase at all
- Does NOT use `EventPhase` to filter callbacks

### 1.3 Dual Event Filter System

Azul has an unusual dual filter system:

- **HoverEventFilter** — fires on hovered nodes (requires hit test)
- **FocusEventFilter** — fires on focused node (requires focus)
- **WindowEventFilter** — fires globally (any node can register)

This is architecturally different from W3C's `addEventListener(type, handler, {capture})`.
In W3C, ANY event type can have capture/bubble. In Azul, the filter category determines
the dispatch strategy.

---

## 2. W3C DOM Events Compliance

### 2.1 Event Phases (W3C DOM Level 2)

| Phase | W3C Spec | Azul Status |
|-------|----------|-------------|
| Capture (root → target) | Required | ✅ Active via `propagate_event()` in `dispatch_events_propagated()` |
| Target | Required | ✅ Works — deepest hit node gets callbacks |
| Bubble (target → root) | Required | ✅ Active via `propagate_event()` — Capture→Target→Bubble for HoverEventFilter |

### 2.2 Event.stopPropagation() / stopImmediatePropagation()

| Feature | W3C Spec | Azul Status |
|---------|----------|-------------|
| `stopPropagation()` | Stop after current node's handlers complete | ✅ `CallbackChange::StopPropagation` — remaining same-node handlers still fire, but propagation to other nodes stops |
| `stopImmediatePropagation()` | Stop immediately, even same-node handlers | ✅ `CallbackChange::StopImmediatePropagation` — breaks callback loop immediately. Exposed in C API as `AzCallbackInfo_stopImmediatePropagation()` |
| `preventDefault()` | Cancel default action | ✅ `CallbackChange::PreventDefault` — blocks text input, tab-focus, etc. Exposed in C API as `AzCallbackInfo_preventDefault()` |

### 2.3 Event Types — Mouse Events

| W3C Event | Azul EventType | Synthesized? | Per-Node? | Notes |
|-----------|---------------|-------------|-----------|-------|
| `mousedown` | MouseDown | ✅ | ✅ Per-node via propagation | Dispatched through `propagate_event()` for hit-tested target |
| `mouseup` | MouseUp | ✅ | ✅ Per-node via propagation | Same |
| `mousemove` | MouseOver | ✅ | ✅ Per-node via propagation | Only fires if position changed |
| `mouseenter` | MouseEnter | ✅ | ✅ Per-node | Full hover-chain diff via `get_all_hovered_nodes()` |
| `mouseleave` | MouseLeave | ✅ | ✅ Per-node | Same hover-chain diff |
| `mouseover` | MouseOver | ✅ | ✅ | HoverEventFilter::MouseOver (Azul name reuse) |
| `mouseout` | MouseOut | ✅ | ✅ | `HoverEventFilter::MouseOut` added — bubbling version of mouseleave |
| `click` | Click | ✅ | ✅ | Synthesized from mousedown+mouseup on same node |
| `dblclick` | DoubleClick | ✅ | ✅ | Detected by gesture manager |
| `contextmenu` | ContextMenu | ❌ | — | Right-click goes directly to context menu display, no separate event |

### 2.4 Event Types — Keyboard Events

| W3C Event | Azul EventType | Status | Notes |
|-----------|---------------|--------|-------|
| `keydown` | KeyDown | ✅ | Dispatched to focused node via FocusEventFilter |
| `keyup` | KeyUp | ✅ | Same |
| `keypress` | KeyPress | ❌ | Deprecated in W3C, not implemented |
| `input` | Input (TextInput) | ✅ | Generated by TextInputManager as EventProvider |
| `beforeinput` | — | ❌ | Not implemented (W3C Input Events Level 2) |
| `compositionstart` | CompositionStart | ✅ | `HoverEventFilter::CompositionStart` / `FocusEventFilter::CompositionStart` added |
| `compositionupdate` | CompositionUpdate | ✅ | `HoverEventFilter::CompositionUpdate` / `FocusEventFilter::CompositionUpdate` added |
| `compositionend` | CompositionEnd | ✅ | `HoverEventFilter::CompositionEnd` / `FocusEventFilter::CompositionEnd` added |

### 2.5 Event Types — Focus Events

| W3C Event | Azul Equivalent | Status | Notes |
|-----------|----------------|--------|-------|
| `focus` | FocusReceived | ✅ | Dispatched explicitly in focus-change block |
| `blur` | FocusLost | ✅ | Same |
| `focusin` | FocusIn | ✅ | `HoverEventFilter::FocusIn` / `FocusEventFilter::FocusIn` added — bubbles through DOM |
| `focusout` | FocusOut | ✅ | `HoverEventFilter::FocusOut` / `FocusEventFilter::FocusOut` added — bubbles through DOM |

### 2.6 Event Types — Drag and Drop (HTML5 DnD)

| W3C Event | Azul EventType | When Generated | Target | Status |
|-----------|---------------|----------------|--------|--------|
| `dragstart` | DragStart | Gesture threshold crossed | Drag source node | ✅ Works |
| `drag` | Drag | Every mousemove during drag | Drag source node | ✅ Works |
| `dragend` | DragEnd | Mouse released during drag | Drag source node | ✅ Works |
| `dragenter` | DragEnter | Hover node changed during drag | ✅ Specific `DomNodeId` under cursor | ✅ Targeted via `mouse_target` |
| `dragleave` | DragLeave | Previous hover left during drag | ✅ Previous hover node | ✅ Targeted |
| `dragover` | DragOver | Continuous hover during drag | ✅ Current hover node | ✅ Targeted |
| `drop` | Drop | Mouse released over drop target | ✅ `mouse_target` | ✅ Targeted |

### 2.7 Click Event Synthesis (W3C UIEvents)

Per W3C, a `click` event is generated when:
1. `mousedown` fires on element A
2. `mouseup` fires on element A (same element)
3. THEN `click` fires on element A

**Azul status:** ✅ Click events are synthesized when mouseup fires on the same node
as the preceding mousedown (implemented in `dispatch_events_propagated()`).
The `LeftMouseDown` HoverEventFilter matches for click events.

---

## 3. Event Dispatch Architecture Issues

### 3.1 All Events Target root_node

Every event generated by `determine_all_events()` has `target: root_node` (NodeId(0)):

```rust
events.push(SyntheticEvent::new(
    EventType::MouseDown,
    EventSource::User,
    root_node.clone(),  // ← ALWAYS root_node
    timestamp.clone(),
    EventData::None,
));
```

Events are then dispatched via `get_callback_target()` which checks: if target is root_node,
return `CallbackTarget::RootNodes`. This causes `invoke_callbacks_v2` to search the full
DOM tree for matching callbacks.

**Problem:** This conflates "window-level event" with "event that needs hit-testing to
determine target." In W3C, MouseDown dispatches to the **specific element under the cursor**,
not to the document root.

**Consequence:** The hit-test-to-callback routing happens inside `invoke_callbacks_v2`
rather than at the dispatch level. This works for simple cases but fails for:
- Per-node enter/leave (need to know which specific node to target)
- Drag enter/leave on drop targets
- Click synthesis (need to track mousedown target)

### 3.2 EventData::None for Most Events

Most synthesized events carry `EventData::None`:

```rust
EventType::MouseDown => EventData::None  // no button info!
EventType::MouseUp => EventData::None    // no button info!
EventType::MouseOver => EventData::None  // no position!
```

This means:
- `LeftMouseDown` vs `RightMouseDown` filtering in `matches_hover_filter` ALWAYS returns
  false (because `EventData::Mouse(...)` is never populated)
- Left/Right/Middle button distinction only works for the generic `MouseDown`/`MouseUp`
  which always matches

**Root cause:** `determine_all_events()` does not populate `EventData::Mouse(...)` with
button info and cursor position. The filter `LeftMouseDown` checks
`mouse_data.button == MouseButton::Left` but `event.data` is `EventData::None`.

**Impact:** A callback registered for `HoverEventFilter::LeftMouseDown` will NEVER fire
because the filter check fails. Only `HoverEventFilter::MouseDown` works.

### 3.3 Bubbling Implementation Asymmetry (RESOLVED)

> **NOTE:** This section describes the OLD `invoke_callbacks_v2` architecture that has been
> replaced by `dispatch_events_propagated()` + `propagate_event()`. The issues below are
> **no longer present** in the current code.

The OLD `invoke_callbacks_v2` implemented bubbling differently for different CallbackTargets:

| Target | Bubbling | How |
|--------|----------|-----|
| `CallbackTarget::Node(...)` | ❌ No (OLD) | Only checked the exact target node |
| `CallbackTarget::RootNodes` + HoverEventFilter | ✅ Yes | Walked from deepest hit node to root |
| `CallbackTarget::RootNodes` + other filters | ❌ No (OLD) | Searched ALL nodes, no ordering |

**Status:** ✅ RESOLVED — `dispatch_events_propagated()` now uses `propagate_event()` for all
HoverEventFilter events, which implements proper Capture→Target→Bubble through the DOM tree.
`CallbackTarget` enum was removed entirely.

---

## 4. Input Recording Architecture

### 4.1 GestureAndDragManager

The input recording pipeline is well-architected:

```
Platform Event → record_input_sample()
                     │
                     ├── start_input_session() on button down
                     ├── record_input_sample() on movement
                     └── end_current_session() on button up
                            │
                            └── detect_*() queries (immutable):
                                ├── detect_drag() — distance threshold
                                ├── detect_double_click() — timing
                                ├── detect_long_press() — duration + stillness
                                ├── detect_swipe_direction()
                                ├── detect_pinch()
                                └── detect_rotation()
```

**Strengths:**
- Clean separation: recording is mutable, detection is immutable
- Supports pen/touch with pressure and tilt
- Screen-position tracking for stable drag delta (immune to window-move feedback loop)
- Configurable thresholds via `GestureConfig`

**Issues:**

1. **Double-counting in determine_all_events:** `detect_drag()` is called during event
   synthesis, and then `DragStart` is also handled in the post-callback block. The gesture
   detection should happen ONCE, not be re-queried.

2. **Drag activation is split across two systems:**
   - `GestureAndDragManager.activate_node_drag()` — manages the drag context
   - `DragDropManager.active_drag` — legacy system, manually synced
   - Both must be kept in sync, leading to bugs when one is updated and the other isn't

3. **No touch input session management:** While the gesture manager supports
   `InputPointId::Touch(id)`, the platform handlers only call `record_input_sample` for
   mouse events. Touch events from macOS/Windows/Wayland are not wired to the gesture system.

### 4.2 HoverManager

The hover manager maintains a history of hit tests per input point:

```rust
pub struct HoverManager {
    hover_histories: HashMap<InputPointId, VecDeque<FullHitTest>>,
}
```

**Strengths:**
- History-based (can compare current vs previous hover for enter/leave detection)
- Supports multi-touch with separate histories per touch point
- `current_hover_node()` and `previous_hover_node()` return deepest hovered node

**Issues:**

1. **Only deepest node returned:** `current_hover_node()` returns only the **single deepest**
   hovered node. Per W3C, mouseenter should fire on EVERY element in the hover chain
   (all ancestors of the deepest node). A node becoming unhovered should fire mouseleave
   even if a child is still hovered.

2. **No full hover-chain diff:** To implement proper per-node enter/leave, the system needs
   to diff the complete set of hovered nodes between frames, not just the deepest node.
   The data is available (`regular_hit_test_nodes` in FullHitTest) but not used for
   per-node event generation.

3. **History size unbounded:** `push_hit_test` pushes into a VecDeque with no max size.
   On a 240Hz display with fast mouse movement, this could accumulate thousands of entries.

---

## 5. Drag and Drop Specific Issues

### 5.1 Visual Feedback Pipeline

The current approach offsets display list items before WR translation:

```
apply_drag_visual_offset() → modify DisplayList positions → build_webrender_transaction()
                                                                    → restore_drag_visual_offset()
```

**Root cause of current failure:** `node_mapping` did not contain entries for
PushStackingContext items because `set_current_node()` was not called before
`push_stacking_context()` in `generate_for_stacking_context()`. (Fixed in this session.)

**Architectural concern:** Modifying the display list is fragile:
- Must save/restore all modified items
- Text glyphs need individual position adjustment
- Child stacking contexts within the dragged node also need offsetting
- If any intermediate re-layout happens between apply and restore, offsets are lost

**Alternative approach (recommended):** Use a WebRender `SpatialId` transform on
the dragged node's stacking context. This is the GPU-native way and requires:
1. Assign a unique `SpatialId` during display list generation for draggable nodes
2. Store the mapping `NodeId → SpatialId` in layout results
3. Update the spatial transform directly via WR API (no display list modification)

### 5.2 Cursor Position in Callbacks

`getCursorPosition()` returns `(0, 0)` because `invoke_single_callback()` hardcodes
`cursor_in_viewport = OptionLogicalPosition::None`. The cursor position from
`current_window_state.mouse_state.cursor_position` is available but not passed through.
(Fixed in this session.)

### 5.3 Drop Target Detection

During drag, the node under the cursor (potential drop target) is tracked via:
- `hover_manager.current_hover_node()` — works
- `NodeDrag.current_drop_target` — set in `update_active_drag_positions()` but
  this field is `OptionDomNodeId` and does NOT get updated during drag

The hover manager's hit test IS updated every frame (platform calls `update_hit_test`),
but `NodeDrag.current_drop_target` is only set during `activate_node_drag()` and never
updated as the cursor moves over different nodes. The DragEnter/DragLeave logic in
`determine_all_events()` uses `hover_manager.current_hover_node()` which IS updated,
but the per-node dispatch is missing (events go to RootNodes).

### 5.4 Missing Drag Events on Specific Targets

The C test registers `mouseEnter`/`mouseLeave` on Zone A/B, expecting them to fire during
drag. This fails because:
1. Per-node `MouseEnter`/`MouseLeave` are never generated (Section 2.3)
2. The test should instead register for `DragEnter`/`DragLeave` BUT those events also
   don't work correctly because they're dispatched as RootNodes (Section 2.6)

---

## 6. Comprehensive Fix Plan

### Priority 1: Fix EventData population (Medium effort)

Populate `EventData::Mouse(...)` in `determine_all_events()` for mouse events:

```rust
EventType::MouseDown => EventData::Mouse(MouseEventData {
    position: current_state.mouse_state.cursor_position.get_position().unwrap_or_default(),
    button: /* determine from state diff */,
    buttons: current_button_state,
    modifiers: KeyModifiers::from_keyboard_state(&current_state.keyboard_state),
})
```

This unblocks `LeftMouseDown`, `RightMouseDown`, `MiddleMouseDown` filter matching.

### Priority 2: Per-node MouseEnter/MouseLeave (High effort)

Add to `determine_all_events()` after hover manager comparison:

```rust
// Compare FULL hover chain (all hovered nodes) between frames
let current_hovered: BTreeSet<(DomId, NodeId)> = get_all_hovered_nodes(hover_manager, current);
let previous_hovered: BTreeSet<(DomId, NodeId)> = get_all_hovered_nodes(hover_manager, previous);

// Nodes that lost hover → MouseLeave
for (dom_id, node_id) in previous_hovered.difference(&current_hovered) {
    events.push(SyntheticEvent::new(
        EventType::MouseLeave,
        EventSource::User,
        DomNodeId { dom: *dom_id, node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)) },
        timestamp.clone(),
        EventData::None,
    ));
}

// Nodes that gained hover → MouseEnter
for (dom_id, node_id) in current_hovered.difference(&previous_hovered) {
    events.push(SyntheticEvent::new(
        EventType::MouseEnter,
        EventSource::User,
        DomNodeId { dom: *dom_id, node: NodeHierarchyItemId::from_crate_internal(Some(*node_id)) },
        timestamp.clone(),
        EventData::None,
    ));
}
```

Then update `dispatch_single_event()` to use the event's `target` for per-node events
instead of always returning `CallbackTarget::RootNodes`.

### Priority 3: Target DragEnter/DragLeave at drop target node (Medium effort)

Replace `root_node` target in DragEnter/DragLeave/DragOver/Drop with the actual
hovered node:

```rust
if let Some(entered_node) = current_hover {
    events.push(SyntheticEvent::new(
        EventType::DragEnter,
        EventSource::User,
        DomNodeId { dom: dom_id, node: entered_node_hierarchy_id },
        ...
    ));
}
```

And update `get_callback_target()` to return `CallbackTarget::Node` for these events
(since target is no longer root).

### Priority 4: Click event synthesis (Medium effort)

Track the mousedown target node and generate Click when mouseup occurs on the same node:

```rust
// In the event processing, after MouseDown:
self.mousedown_target = hit_test.deepest_node();

// In the event processing, after MouseUp:
if self.mousedown_target == hit_test.deepest_node() {
    events.push(SyntheticEvent::new(EventType::Click, ...));
}
```

### Priority 5: Use propagate_event() from core (High effort, optional)

Replace the manual bubbling in `invoke_callbacks_v2()` with the existing
`propagate_event()` system from `core/src/events.rs`. This would enable:
- Capture phase support
- `stopImmediatePropagation()` working correctly
- Consistent phase-based filtering
- `EventPhase` in callbacks (useful for debugging)

This is a larger refactor since `invoke_callbacks_v2` currently does both callback
collection AND invocation in one pass, while `propagate_event` only collects.

### Priority 6: SpatialId-based drag transforms (High effort, recommended)

Replace display-list-offset approach with WebRender spatial transforms:
1. During display list generation, assign `SpatialId` to draggable nodes
2. Store `NodeId → SpatialId` mapping
3. In render loop, update spatial transform with drag delta via WR API
4. No display list modification needed, GPU-native, zero restore overhead

---

## 7. Summary Matrix

| Feature | W3C Required | Azul Status | Severity |
|---------|-------------|-------------|----------|
| Capture phase | Yes | ✅ Active via `propagate_event()` | — |
| Bubble phase | Yes | ✅ Active via `propagate_event()` | — |
| stopPropagation | Yes | ✅ Works | — |
| stopImmediatePropagation | Yes | ✅ Checked in `propagate_phase()` | — |
| preventDefault | Yes | ✅ Works | — |
| Per-node mouseenter/leave | Yes | ✅ Full hover-chain diff via `get_all_hovered_nodes()` | — |
| Left/Right/Middle button filters | Yes | ✅ `EventData::Mouse` populated with correct button | — |
| Click from mouse down+up | Yes | ✅ Synthesized when mouseup on same node as mousedown | — |
| DragEnter/Leave on target | Yes | ✅ Targets specific `DomNodeId` under cursor | — |
| Drop on target | Yes | ✅ Targets `mouse_target` | — |
| focusin/focusout (bubbling) | Yes | ✅ `HoverEventFilter::FocusIn`/`FocusOut` + `FocusEventFilter::FocusIn`/`FocusOut` | — |
| mouseover/mouseout | Yes | ✅ `HoverEventFilter::MouseOut` added, `MouseOver` existing | — |
| Cursor in callbacks | Yes | ✅ Reads from `current_window_state.mouse_state` | — |
| Display list node_mapping | N/A | ✅ `set_current_node()` before `push_stacking_context()` | — |
| Old `invoke_callbacks_v2` removed | N/A | ✅ Replaced by `dispatch_events_propagated()` | — |
| Old `dispatch_synthetic_events` removed | N/A | ✅ Dead code cleaned up | — |
| compositionstart/update/end | W3C L3 | ✅ `HoverEventFilter::CompositionStart`/`Update`/`End` + focus equivalents | — |
| stopImmediatePropagation in C API | Yes | ✅ `AzCallbackInfo_stopImmediatePropagation()` exposed | — |
| beforeinput | W3C L2 | ❌ Not implemented | Low |
| contextmenu event | Yes | ❌ Right-click → native menu, no DOM event | Low |
| Virtual keyboard API | W3C Draft | ❌ Not yet — see §8 below | Low (touch only) |

**All critical and medium items resolved.** Remaining gaps are low-priority:
- `beforeinput` (W3C Input Events Level 2) — would need to fire before text mutation
- `contextmenu` event — currently right-click goes straight to native menu
- Virtual keyboard control — W3C `VirtualKeyboard` API for touch devices (see §8)

**Completed items:**
1. ~~Display list node_mapping for PushStackingContext~~ ✅ Fixed
2. ~~Cursor position in callbacks~~ ✅ Fixed
3. ~~Per-node mouseenter/mouseleave~~ ✅ Fixed
4. ~~DragEnter/DragLeave/DragOver/Drop targeting specific nodes~~ ✅ Fixed
5. ~~EventData::Mouse population (for button-specific filters)~~ ✅ Fixed
6. ~~Use `propagate_event()` from core (W3C Capture→Target→Bubble)~~ ✅ Fixed
7. ~~Remove old `invoke_callbacks_v2` / `dispatch_synthetic_events`~~ ✅ Cleaned up
8. ~~focusin/focusout bubbling event filters~~ ✅ Added
9. ~~mouseover/mouseout event filters~~ ✅ Added  
10. ~~compositionstart/update/end IME event filters~~ ✅ Added
11. ~~stopImmediatePropagation exposed in C API~~ ✅ Added
12. ~~stopPropagation W3C-correct: same-node handlers still fire~~ ✅ Fixed

---

## 8. Virtual Keyboard API (W3C Working Draft)

The W3C [VirtualKeyboard API](https://www.w3.org/TR/virtual-keyboard/) provides
control over the on-screen keyboard on touch devices.

### 8.1 Key Concepts

| Concept | Description |
|---------|-------------|
| `navigator.virtualKeyboard.show()` | Programmatically show the software keyboard |
| `navigator.virtualKeyboard.hide()` | Programmatically hide the software keyboard |
| `overlaysContent` | When `true`, browser does NOT resize viewport for keyboard |
| `boundingRect` | Returns the keyboard's intersection with viewport as DOMRect |
| `geometrychange` event | Fired when keyboard geometry changes (show/hide/resize) |
| `virtualKeyboardPolicy` attribute | `auto` (browser handles) or `manual` (script handles) |
| `inputmode` attribute | Hints which keyboard type to show: `text`, `numeric`, `tel`, `email`, `url`, `search`, `none` |

### 8.2 Azul Mapping

For Azul, this maps to:
1. **`CallbackInfo::show_virtual_keyboard()`** — Call platform API to show soft keyboard
   - macOS: Not applicable (no touch keyboard on desktop macOS)
   - iOS: `UIResponder.becomeFirstResponder()`
   - Android/Linux touch: platform-specific input method activation
2. **`CallbackInfo::hide_virtual_keyboard()`** — Dismiss the keyboard
3. **`WindowEvent::VirtualKeyboardGeometryChanged`** — New event when keyboard bounds change
4. **`inputmode` property on DOM nodes** — Hint for keyboard type (can be a CSS property or DOM attribute)

### 8.3 Implementation Status

Currently **not implemented**. Implementation requires:
- Platform-specific keyboard show/hide calls for each target (iOS, Android, Wayland touch)
- A `VirtualKeyboardGeometryChanged` window event
- An `inputmode` property/attribute on input-like nodes
- CSS environment variables (`keyboard-inset-*`) for layout adaptation

This is a low priority since azul currently targets desktop platforms primarily.

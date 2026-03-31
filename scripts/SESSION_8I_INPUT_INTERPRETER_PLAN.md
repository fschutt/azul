# Session 8I: Configurable Input Interpreter

## Overview

Replace the hardcoded `pre_callback_filter_internal_events` → `post_callback_filter_system_changes`
chain with a configurable `extern "C" fn` on `AppConfig`.

## Current Architecture

```
Platform events → SyntheticEvent[]
    → pre_callback_filter_internal_events()   // HARDCODED: key→SelectionOp, mouse→TextSelection, etc.
    → (SystemChange[], UserEvent[])
    → dispatch user callbacks
    → post_callback_filter_system_changes()   // HARDCODED: scroll-into-view, auto-scroll timer
    → apply_system_change() for each
```

The `pre_callback_filter_internal_events` function (core/src/events.rs) contains:
- `handle_key_down`: Ctrl+C/V/A/Z/D shortcuts + VK→SelectionOp mapping
- `handle_mouse_down`: click detection, Ctrl+Click → AddCursorAtClick
- `handle_mouse_over`: drag selection
- Tab navigation (stub)

## Proposed Architecture

```rust
/// The input interpreter: maps raw events + window state to semantic actions.
///
/// Default: `default_input_interpreter` (standard desktop navigation).
/// Override on AppConfig for vim, game controls, accessibility, etc.
pub type InputInterpreterFn = extern "C" fn(
    events: &SyntheticEventVec,
    current_state: &WindowState,
    previous_state: &WindowState,
    hit_test: OptionFullHitTestRef,
    keyboard_state: &KeyboardState,
    mouse_state: &MouseState,
    focus_info: &FocusInfo,          // focused node, click count, drag start
    user_data: &mut RefAny,          // stateful: vim mode, repeat counter
) -> SystemChangeVec;

/// Post-filter: maps pre-callback system changes to post-callback actions.
pub type PostFilterFn = extern "C" fn(
    pre_changes: &SystemChangeVec,
    user_data: &mut RefAny,
) -> SystemChangeVec;
```

### AppConfig additions

```rust
pub struct AppConfig {
    // ... existing fields ...
    pub input_interpreter: InputInterpreterFn,
    pub input_interpreter_data: RefAny,          // user state (vim mode etc.)
    pub post_filter: PostFilterFn,
    pub post_filter_data: RefAny,
}
```

### FocusInfo (new struct, replaces SelectionManagerQuery + FocusManagerQuery)

```rust
#[repr(C)]
pub struct FocusInfo {
    pub focused_node: OptionDomNodeId,
    pub click_count: u8,
    pub drag_start_position: OptionLogicalPosition,
}
```

### Default implementation

`default_input_interpreter` contains the current logic from:
- `handle_key_down` (shortcuts + SelectionOp mapping)
- `handle_mouse_down` (click + Ctrl+Click)
- `handle_mouse_over` (drag selection)

`default_post_filter` contains:
- ScrollSelectionIntoView after ApplySelectionOp
- StartAutoScrollTimer after TextSelectionDrag

### Vim example

```rust
struct VimState { mode: VimMode, repeat_count: usize }

extern "C" fn vim_interpreter(
    events: &SyntheticEventVec,
    current: &WindowState, prev: &WindowState,
    hit_test: OptionFullHitTestRef,
    kb: &KeyboardState, mouse: &MouseState,
    focus: &FocusInfo, user_data: &mut RefAny,
) -> SystemChangeVec {
    let state = user_data.downcast_mut::<VimState>().unwrap();
    let mut changes = Vec::new();

    for event in events.as_slice() {
        if let EventData::Keyboard(kd) = &event.data {
            match state.mode {
                VimMode::Normal => match kd.key_code_as_vk() {
                    Some(VK::I) => { state.mode = VimMode::Insert; }
                    Some(VK::H) => { /* repeat_count × Backward Character Move */ }
                    Some(VK::W) => { /* repeat_count × Forward Word Move */ }
                    Some(VK::D) if kb.shift_down() => { /* delete to end of line */ }
                    Some(vk @ VK::Key0..=VK::Key9) => { state.repeat_count = ...; }
                    _ => {}
                },
                VimMode::Insert => {
                    // Delegate to default for normal text input
                    changes.extend(default_input_interpreter(...));
                }
            }
        }
    }
    changes.into()
}
```

## Implementation Sequence

1. Define `FocusInfo`, `InputInterpreterFn`, `PostFilterFn` in core/src/events.rs
2. Add fields to AppConfig with defaults
3. Extract current logic into `default_input_interpreter` / `default_post_filter`
4. Thread through: run.rs → process_window_events → pre_callback_filter
5. Replace direct `pre_callback_filter_internal_events` calls with `(config.input_interpreter)(...)`
6. Add to api.json + codegen
7. Also consolidate 8 CallbackChange::MoveCursor* into ApplySelectionOp via the interpreter

## Dependencies

- Needs SystemChangeVec (impl_vec! for SystemChange)
- Needs SyntheticEventVec (may already exist)
- The RefAny for user_data needs proper lifecycle management

---
slug: text-input
title: Text Input and Contenteditable
language: en
canonical_slug: text-input
audience: external
maturity: wip
guide_order: 92
topic_only: false
short_desc: Editable text — how `contenteditable`, IME, and the text3 selection model produce a working text field.
prerequisites: [events]
tracked_files:
  - core/src/hit_test.rs
  - core/src/selection.rs
  - layout/src/widgets/text_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Text Input and Contenteditable

> **WIP.** The text-input runtime is wired but several pieces are still missing: macOS lacks `NSTextInputClient` conformance (CJK / IME does not work), text from the default callback is appended at the end rather than inserted at `cursor_pos`, and `TextInputState::max_len` is not enforced. APIs may change. The shapes below are the current ones.

Two ways to make text editable in Azul:

| API | Use when |
|---|---|
| [`TextInput`](#the-textinput-widget) widget | You want a single-line input field with platform-native styling, placeholder, focus border, blinking cursor. |
| [`NodeData::set_contenteditable(true)`](#the-contenteditable-flag) | You want any DOM node to accept keyboard editing — code editor lines, multi-line text areas, rich-text spans. |

Both share the same event plumbing: a focused, contenteditable node receives `Focus(TextInput)` and `Focus(VirtualKeyDown)` events, and the framework applies the resulting edits through an incremental display-list path that does **not** re-run the layout callback.

## The `TextInput` widget

`layout/src/widgets/text_input.rs:539`. `TextInput::create()` returns a default-styled widget; `dom()` consumes it into a `Dom`.

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::TextInput;
let input = TextInput::create()
    .with_placeholder("Your name".into())
    .with_text("Felix".into())
    .dom();
```

The produced subtree is a `<div tabindex="0">` container with three children: a placeholder text node, a label text node holding the buffer, and an absolutely-positioned cursor `<div>`. The container carries the focus border, padding, and `overflow: hidden`.

### Wiring callbacks

Three optional callbacks fire in addition to the default key/text handlers:

| setter | fires on | callback signature |
|---|---|---|
| `with_on_text_input(data, cb)` | every accepted character | `extern "C" fn(RefAny, CallbackInfo, TextInputState) -> OnTextInputReturn` |
| `with_on_virtual_key_down(data, cb)` | non-text keys (arrows, backspace) | same as above |
| `with_on_focus_lost(data, cb)` | focus moved elsewhere | `extern "C" fn(RefAny, CallbackInfo, TextInputState) -> Update` |

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::{TextInput, TextInputState, OnTextInputReturn, TextInputValid};
# struct Form { name: String }
extern "C" fn on_input(
    mut data: RefAny,
    _info: CallbackInfo,
    new_state: TextInputState,
) -> OnTextInputReturn {
    if let Some(mut form) = data.downcast_mut::<Form>() {
        form.name = new_state.get_text();
    }
    OnTextInputReturn { update: Update::DoNothing, valid: TextInputValid::Yes }
}

# let form = RefAny::new(Form { name: String::new() });
let dom = azul::widgets::TextInput::create()
    .with_on_text_input(form, on_input)
    .dom();
```

`OnTextInputReturn::valid` is the gate that lets the application reject a character (e.g. "only digits"). Returning `TextInputValid::No` causes the widget to roll back the edit before mutating the DOM. `OnTextInputReturn::update` follows the usual `Update` semantics from [Events and Input](events.md).

### `TextInputState`

```rust,ignore
use azul::widgets::TextInputState;
fn fields(s: TextInputState) {
    let _: U32Vec       = s.text;          // chars as u32, FFI-friendly
    let _: OptionString = s.placeholder;
    let _: usize        = s.max_len;
    let _: usize        = s.cursor_pos;
}
```

`get_text() -> String` reconstructs a normal Rust string from the `u32` buffer. The `selection` field exists but is not yet read by the default callbacks — text input ignores selection state today.

## The `contenteditable` flag

`NodeData::set_contenteditable(true)` (or `with_contenteditable(true)` for builders) marks any node as an editable region. The flag is a packed bit on `NodeFlags`, not a string attribute, so the cost of checking it is one shift-and-mask:

```rust,no_run
# use azul::prelude::*;
let mut line = Dom::create_div();
line.set_contenteditable(true);
line.set_inline_style("tabindex: 0");
```

Once the node has focus and the `contenteditable` bit is set, every printable key press the OS produces (after shaping by the keyboard layout) is delivered as a `PendingTextEdit` and applied to the inline content of that node. No widget code is involved — the platform shell records the edit, the layout managers compute the changeset, and the renderer consumes it.

### How edits avoid a full re-layout

Text edits run through an **incremental display-list path** that bypasses the user's `layout_callback`. The motivation: if every keystroke triggered a full DOM rebuild, the user-supplied callback would return a fresh DOM with the *original* text and overwrite the edit. The path is documented in `scripts/TEXT_INPUT_ARCHITECTURE_V4.md`.

The framework distinguishes three levels of post-event work:

| level | trigger | layout_callback runs? |
|---|---|---|
| `RequestRedraw` | scroll offsets, GPU transforms | no |
| `UpdateDisplayList` | text edits, incremental relayout | no |
| `RegenerateDisplayList` | `Update::RefreshDom`, focus changes that move the DOM, hit-tester invalidation | yes |

A keystroke that does not change line dimensions takes the second path: the text cache is patched in place, the display list is regenerated from the *existing* layout tree, and a transaction is sent to WebRender. A keystroke that changes the line's measured width (e.g. inserting a wide glyph) escalates to `ShouldIncrementalRelayout`, which re-runs solver3 on the existing styled DOM but still does not call `layout_callback`.

Returning `Update::RefreshDom` from a text-input callback forces the third path. Do this only when the edit changes something the layout callback needs to see — adding a new sibling node, hiding a section, etc.

## Multi-cursor

`MultiCursorState` (`core/src/selection.rs:255`) is the Sublime-style multi-cursor type used by contenteditable nodes:

```rust,ignore
use azul_core::selection::*;
use azul_core::dom::DomNodeId;
fn make(node: DomNodeId, primary: TextCursor) {
    let mut state = MultiCursorState::new_with_cursor(primary, node, /* ce_key */ 0);
    let _id: SelectionId = state.add_cursor(primary);   // Ctrl+Click
    state.move_all_cursors(false, |c| *c);              // typing, motion, ...
    state.merge_overlapping();
}
```

Each cursor / range carries a stable `SelectionId` so external code can refer to a specific cursor across edits. The **primary** cursor — the one used for IME composition, scroll-into-view, and clipboard target — is always the last entry in `selections`. Mutations call `merge_overlapping()` to maintain the "sorted, non-overlapping" invariant.

`MultiCursorState::remap_node_ids` rewrites the stored `node_id` after a DOM rebuild reassigns `NodeId` values; if the previous node is gone the state clears itself. This is what keeps a contenteditable's cursor alive across a `RefreshDom`.

## Cursor blink and animation

`TextInputStateWrapper::cursor_animation: OptionTimerId` holds the timer that toggles cursor opacity. The widget registers this timer when the container receives focus and tears it down on focus loss. Timers survive the incremental display-list path; if focus triggers a full DOM regeneration, the timer ID is re-mapped along with the node.

## Reading edits inside a callback

`CallbackInfo::get_text_changeset()` returns `Option<&PendingTextEdit>` — the framework's record of "this is what the OS produced since the last frame". The widget consults this rather than the deprecated `keyboard_state.current_char`:

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_key(_data: RefAny, info: CallbackInfo) -> Update {
    if let Some(edit) = info.get_text_changeset() {
        let _inserted: &str = edit.inserted_text.as_str();
    }
    Update::DoNothing
}
```

`change_node_text(node_id, text)` is the corresponding write side: it replaces the text content of a node and queues an incremental display-list update. The `TextInput` widget calls it from the default text callback (`text_input.rs:1038`).

## Default actions

These keystrokes are handled by the framework after every callback returns, unless a callback called `info.prevent_default()`:

| key | effect on contenteditable |
|---|---|
| Backspace | delete grapheme before the primary cursor |
| Delete | delete grapheme after the primary cursor |
| Left/Right arrow | move cursor by one grapheme |
| Home/End | move cursor to line start/end |
| Ctrl+Home / Ctrl+End | move cursor to document start/end |
| Ctrl+A | select all (scoped to the focused contenteditable) |
| Escape | collapse multi-cursor to primary |

Suppress with `info.prevent_default()` to override; the rest of the callback chain still runs (W3C semantics — see [Events and Input](events.md#default-actions)).

## Where it goes wrong

- **macOS: typing CJK or dead keys produces nothing.** `NSTextInputClient` conformance is not declared on `GLView` / `CPUView`, so `interpretKeyEvents:` is bypassed and IME composition never starts. ASCII works because the shell extracts the character directly from the `NSEvent`. Tracked in `scripts/TEXT_INPUT_ARCHITECTURE_V4.md`.
- **Default `TextInput` callback ignores `cursor_pos`.** `text_input.rs:1029` always appends with `internal.extend(...)`. To insert at the caret, install your own `on_text_input` that splices into `TextInputState::text` at `cursor_pos`.
- **Default backspace removes the last char, not the one before `cursor_pos`.** Same root cause (`text_input.rs:1066` calls `internal.pop()`).
- **`max_len` is set but never enforced.** Add a length check in your `on_text_input` and return `TextInputValid::No` when the buffer is full.
- **First click positions the cursor at the start, not at the click.** The cursor is initialised at end-of-text in `finalize_pending_focus_changes`. The `ProcessTextSelectionClick` system change can fire before focus is granted on the first click, so the click misses. Subsequent clicks behave normally.
- **`TextInputSelection` / `TextInputSelectionRange` are dead types.** They appear in the public API but are never read; selection inside a `TextInput` widget is not wired through. Multi-node selection (drag across nodes) goes through `TextSelection` instead — see [Text Selection](text-selection.md).

## Next

- [Text Selection](text-selection.md) — anchor/focus model that ranges over arbitrary DOM, not just inside a `TextInput`.
- [Events and Input](events.md) — the underlying event plumbing.

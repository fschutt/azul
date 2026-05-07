---
slug: text-input
title: Text Input
language: en
canonical_slug: text-input
audience: external
maturity: wip
guide_order: 92
topic_only: false
short_desc: Editable text, IME, and the selection model
prerequisites: [events]
tracked_files:
  - core/src/hit_test.rs
  - core/src/selection.rs
  - layout/src/widgets/text_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
---

# Text Input
> **WIP.** The text-input runtime is wired but several pieces are still missing. macOS IME / CJK support is incomplete. APIs may change.

There are two ways to make text editable:

- The [`TextInput`](#the-textinput-widget) widget: a single-line input field with platform-native styling, placeholder, focus border, and blinking cursor.
- The [`contenteditable`](#the-contenteditable-flag) flag on any DOM node: for code-editor lines, multi-line text areas, or rich-text spans.

Both share the same event plumbing. A focused, editable node receives `Focus(TextInput)` and `Focus(VirtualKeyDown)` events. IME composition is handled by the platform shell; from your callback's point of view, you receive the produced character once composition commits.

## The TextInput widget

`TextInput::create()` returns a default-styled widget; `dom()` consumes it into a `Dom`.

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::TextInput;
let input = TextInput::create()
    .with_placeholder("Your name".into())
    .with_text("Felix".into())
    .dom();
```

The produced subtree is a focusable container with a placeholder text node, a label text node holding the buffer, and a cursor.

### Wiring callbacks

Three optional callbacks fire in addition to the default key/text handlers:

- `with_on_text_input(data, cb)`: fires for every accepted character. Signature: `extern "C" fn(RefAny, CallbackInfo, TextInputState) -> OnTextInputReturn`.
- `with_on_virtual_key_down(data, cb)`: fires for non-text keys (arrows, backspace). Same signature.
- `with_on_focus_lost(data, cb)`: fires when focus moves elsewhere. Signature: `extern "C" fn(RefAny, CallbackInfo, TextInputState) -> Update`.

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

`OnTextInputReturn::valid` is the gate that lets you reject a character (e.g. "only digits"). Returning `TextInputValid::No` rolls back the edit before mutating the DOM. `OnTextInputReturn::update` follows the usual `Update` semantics from [Events](events.md).

### TextInputState

`TextInputState` carries:

- `text: U32Vec` (characters as `u32`, FFI-friendly).
- `placeholder: OptionString`.
- `max_len: usize`.
- `cursor_pos: usize`.
- `selection: TextInputSelection`.

`TextInputState::get_text()` reconstructs a normal Rust string from the buffer.

## The contenteditable flag

`NodeData::set_contenteditable(true)` (or `Dom::with_contenteditable(true)` for builders) marks any node as an editable region:

```rust,no_run
# use azul::prelude::*;
let line = Dom::create_div()
    .with_contenteditable(true)
    .with_tab_index(TabIndex::Auto);
```

Once the node has focus and the contenteditable bit is set, every printable key press the OS produces is delivered to that node. The platform shell records the edit, the framework computes the changeset, and the renderer consumes it.

### Edits avoid a full re-layout

Text edits run through an incremental display-list path that bypasses the user's `layout_callback`. The motivation: if every keystroke triggered a full DOM rebuild, the layout callback would return a fresh DOM with the original text and overwrite the edit.

The framework distinguishes three levels of post-event work:

- Redraw only: scroll offsets, GPU transforms. Layout callback doesn't run.
- Display-list update: text edits, incremental relayout. Layout callback doesn't run.
- Full regeneration: `Update::RefreshDom`, focus changes that move the DOM. Layout callback runs.

Returning `Update::RefreshDom` from a text-input callback forces the third path. Do this only when the edit changes something the layout callback needs to see, such as adding a new sibling node or hiding a section.

## The double-update pattern

Because the layout callback is bypassed during a text edit, your application model and the live DOM can drift out of sync if you only write to one of them. The double-update pattern keeps both in sync:

1. Inside `on_text_input`, write the new text to your `RefAny` model so a future re-layout reads the right value.
2. Update the node's dataset with `Dom::with_dataset` (set during layout) and a `DatasetMergeCallback` so the in-place display list patch reflects the edit.

The widget's internal callbacks already do this for you. If you write a custom contenteditable that maintains its own buffer, mirror both.

## Reading edits inside a callback

`CallbackInfo::get_text_changeset()` returns the current `PendingTextEdit`:

```rust,no_run
# use azul::prelude::*;
extern "C" fn on_key(_data: RefAny, info: CallbackInfo) -> Update {
    if let Some(edit) = info.get_text_changeset() {
        let _inserted: &str = edit.inserted_text.as_str();
    }
    Update::DoNothing
}
```

`CallbackInfo::change_node_text(node_id, text)` is the corresponding write side. It replaces the text content of a node and queues an incremental display-list update.

## Default actions

These keystrokes are handled by the framework after every callback returns, unless a callback called `info.prevent_default()`:

- Backspace: delete the grapheme before the cursor.
- Delete: delete the grapheme after the cursor.
- Left/Right arrow: move the cursor by one grapheme.
- Home/End: move the cursor to line start/end.
- Ctrl+Home / Ctrl+End: move the cursor to document start/end.
- Ctrl+A: select all (scoped to the focused contenteditable).
- Escape: collapse selection.

Suppress with `info.prevent_default()` to override. The rest of the callback chain still runs (W3C semantics; see [Events](events.md#default-actions)).

## Where it goes wrong

- **Edit lost after `RefreshDom`.** The layout callback rebuilt the DOM with stale text. Either keep the contenteditable subtree out of the rebuild, or apply the double-update pattern so the rebuild reads from the same model the edit wrote to.
- **`max_len` is not enforced.** Add a length check in your `on_text_input` and return `TextInputValid::No` when the buffer is full.
- **First click positions the cursor at the start.** The cursor is initialised at end-of-text on focus; the first click can race with focus acquisition. Subsequent clicks behave normally.
- **`TextInputSelection` / `TextInputSelectionRange` are not yet wired through the default callbacks.** Multi-node selection across nodes goes through the cross-DOM selection model; see [Text Selection](text-selection.md).

## Coming Up Next

- [Text Selection](text-selection.md) — Selection ranges, cursors, and copy/paste
- [Scrolling](scrolling-and-drag.md) — Scroll containers, drag-and-drop, hit testing
- [Events](events.md) — Callbacks, event filters, and how state triggers relayout

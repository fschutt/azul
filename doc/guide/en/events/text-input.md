---
slug: events/text-input
title: Text Input
language: en
canonical_slug: events/text-input
audience: external
maturity: wip
guide_order: 62
topic_only: false
short_desc: Editable text, IME and composition, cursor motion, undo, validation and document sync
prerequisites: [events]
tracked_files:
  - core/src/hit_test.rs
  - core/src/selection.rs
  - layout/src/widgets/text_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T12:00:00Z
default-search-keys:
  - TextInput
  - TextInputState
  - TextInputStateWrapper
  - TextInputValid
  - TextInputSelection
  - TextInputSelectionRange
  - OnTextInputReturn
  - PendingTextEdit
  - Dom
  - CallbackInfo
  - Update
  - DatasetMergeCallback
---

# Text Input

## Introduction

*WIP.* The text-input runtime is wired but several pieces are still missing — macOS IME / CJK support is incomplete and APIs may change.

There are two ways to make text editable:

- The [`TextInput`](#the-textinput-widget) widget: a single-line input field with platform-native styling, placeholder, focus border, and blinking cursor.
- The [`contenteditable`](#the-contenteditable-flag) flag on any DOM node: for code-editor lines, multi-line text areas, or rich-text spans.

Both share the same event plumbing. A focused, editable node receives `Focus(TextInput)` and `Focus(VirtualKeyDown)` events. IME composition is handled by the platform shell; from your callback's point of view, you receive the produced character once composition commits.

## The TextInput widget

`TextInput::create()` returns a default-styled widget; `dom()` consumes it into a `Dom`.

```rust,no_run
use azul::prelude::*;

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
use azul::prelude::*;

struct Form { name: String }

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

`OnTextInputReturn::valid` is the gate that lets you reject a character (e.g. "only digits"). Returning `TextInputValid::No` rolls back the edit before mutating the DOM. `OnTextInputReturn::update` follows the usual `Update` semantics from [Events](..md).

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
use azul::prelude::*;

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
use azul::prelude::*;

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

Suppress with `info.prevent_default()` to override. The rest of the callback chain still runs (W3C semantics; see [Events](..md#default-actions)).

## IME and composition

Chinese, Japanese and Korean input - and dead keys, and emoji pickers -
go through an *input method*: the user types several keystrokes that
build up a provisional string, and only the commit produces real text.

```rust,ignore
if let Some(pre) = info.get_composition_text().into_option() {
    // provisional text: draw it underlined, do not put it in your model
}
let caret = info.get_composition_cursor();   // caret WITHIN the composition
```

Composition text is not committed text. An editor that writes it into
its buffer will duplicate every character once the commit arrives.
Draw it, and wait. `is_composing()` is the boolean form, and it is also
the guard a key handler needs: Enter during a composition selects a
candidate and must not submit the form.

On mobile, `request_soft_keyboard(true)` raises the on-screen keyboard
and `get_keyboard_inset()` reports how much of the window it now
covers - which is what you pad the layout with so the field being
edited is not behind the keyboard.

## Moving the cursor programmatically

Every motion the arrow keys perform has a method, each taking whether
to extend the selection - which is what the Shift modifier means:

```rust,ignore
info.move_cursor_left(node, /* extend_selection */ false);
info.move_cursor_to_line_end(node, true);      // shift+End
info.move_cursor_to_document_start(node, false);
info.move_cursor(dom_id, node_id, cursor);     // to an exact position
```

`delete_backward(node)` and `delete_forward(node)` are Backspace and
Delete, and `insert_text(dom_id, node_id, text)` types a string in.
All of them respect grapheme boundaries, so a family emoji deletes as
one character rather than leaving half a sequence behind.

## Dry runs: the inspect_ family

Each of those mutations has an `inspect_` twin that computes the result
and changes nothing:

```rust,ignore
let would_be = info.inspect_move_cursor_right(node);   // OptionTextCursor
let deletion = info.inspect_backspace(node);           // OptionDeleteResult
```

This is how you answer "would this do anything?" before doing it -
greying out a Delete button when the cursor is at the end, previewing
what a paste would replace with `inspect_paste_target_range()`, or
validating an edit before applying it. Nothing in an `inspect_` call
touches the document, so it is safe to call every frame.

## Undo

There are two stacks, and they are separate on purpose:

```rust,ignore
info.undo_app_state();        // your model's own undo
info.undo_structural_edit();  // the framework's text/DOM edit history
```

Text edits go on the structural stack automatically.
`commit_undo_snapshot()` closes the current undo group, which is how
you decide what one Ctrl+Z reverts: typing a word is usually one
snapshot, not eight.

`can_undo(node)` and `can_redo(node)` drive the toolbar buttons, and
`get_undo_text(node)` and `get_redo_text(node)` give the label - "Undo
typing" rather than a bare "Undo". `inspect_undo_operation()` and
`inspect_redo_operation()` return the operation itself if you want to
show more than a label.

## Validation

`get_validity_state()` reports the focused field's validity and
`get_validity_state_of(node)` any other node's - required-but-empty,
too long, failing a pattern. Reading it is how a submit button stays
disabled without your own parallel validation pass.

## Cursor blink

`start_cursor_blink_timer()` and `stop_cursor_blink_timer()` control
the caret's blink, and `reset_cursor_blink()` restarts the cycle at
"visible". Call the reset on every edit and cursor move: a caret that
keeps blinking on its own schedule while you type disappears exactly
when you are looking for it.

## Collaborative editing

A document that several people edit needs edits recorded, shipped and
acknowledged, and the framework keeps that bookkeeping:

```rust,ignore
info.record_document_edit(changeset);              // local edit -> the log
let pending = info.get_unsynced_text_edits();      // what the peer has not seen
info.mark_text_revision_synced(revision);          // peer confirmed up to here
```

`get_document_text_revision()` is the monotonic revision counter,
`get_document_edit_clone()` a snapshot of the pending changeset, and
`get_document_caret()` the caret as a document position rather than a
node-local one - which is the form that survives being sent to another
machine.

Applying a remote edit is acknowledged with
`mark_document_edit_applied(id)`, or
`mark_document_edit_applied_with_inverse(id, inverse)` when you also
supply the inverse operation, so the edit remains undoable.

Other users' cursors are painted through the selection-owner API; see
[Text Selection](text-selection.md).

## Where it goes wrong

- **Edit lost after `RefreshDom`.** The layout callback rebuilt the DOM with stale text. Either keep the contenteditable subtree out of the rebuild, or apply the double-update pattern so the rebuild reads from the same model the edit wrote to.
- **`max_len` is not enforced.** Add a length check in your `on_text_input` and return `TextInputValid::No` when the buffer is full.
- **First click positions the cursor at the start.** The cursor is initialised at end-of-text on focus; the first click can race with focus acquisition. Subsequent clicks behave normally.
- **`TextInputSelection` / `TextInputSelectionRange` are not yet wired through the default callbacks.** Multi-node selection across nodes goes through the cross-DOM selection model; see [Text Selection](text-selection.md).

## More methods

**Editing** - `insert_text`, `delete_backward`, `delete_forward`,
`create_text_input`, `get_text_changeset`, `set_text_changeset`.
`create_text_input(text)` creates an editable text surface
imperatively.

**Cursor motion** - `move_cursor`, `move_cursor_left`,
`move_cursor_right`, `move_cursor_up`, `move_cursor_down`,
`move_cursor_to_line_start`, `move_cursor_to_line_end`,
`move_cursor_to_document_start`, `move_cursor_to_document_end`.

**Dry runs** - `inspect_move_cursor_left`, `inspect_move_cursor_right`,
`inspect_move_cursor_up`, `inspect_move_cursor_down`,
`inspect_move_cursor_to_line_start`, `inspect_move_cursor_to_line_end`,
`inspect_move_cursor_to_document_start`,
`inspect_move_cursor_to_document_end`, `inspect_backspace`,
`inspect_delete`, `inspect_delete_changeset`,
`inspect_select_all_changeset`, `inspect_paste_target_range`,
`inspect_copy_changeset`, `inspect_cut_changeset`.

**Undo** - `undo_app_state`, `redo_app_state`, `undo_structural_edit`,
`redo_structural_edit`, `commit_undo_snapshot`, `can_undo`, `can_redo`,
`get_undo_text`, `get_redo_text`, `inspect_undo_operation`,
`inspect_redo_operation`.

**IME and soft keyboard** - `get_composition_text`,
`get_composition_cursor`, `is_composing`, `request_soft_keyboard`,
`get_keyboard_inset`.

**Caret** - `start_cursor_blink_timer`, `stop_cursor_blink_timer`,
`reset_cursor_blink`.

**Validation** - `get_validity_state`, `get_validity_state_of`.

**Document sync** - `record_document_edit`, `get_document_edit_clone`,
`get_unsynced_text_edits`, `get_document_text_revision`,
`mark_text_revision_synced`, `mark_document_edit_applied`,
`mark_document_edit_applied_with_inverse`, `get_document_caret`.

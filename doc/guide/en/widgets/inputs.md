---
slug: widgets/inputs
title: Input Widgets
language: en
canonical_slug: widgets/inputs
audience: external
maturity: wip
guide_order: 141
topic_only: false
short_desc: Text fields, checkboxes, radios, sliders, dropdowns
prerequisites: [widgets, text-input, text-selection]
tracked_files:
  - layout/src/widgets/text_input.rs
  - layout/src/widgets/number_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:43Z
default-search-keys:
  - TextInput
  - TextInputState
  - TextInputStateWrapper
  - TextInputValid
  - TextInputSelection
  - TextInputSelectionRange
  - OnTextInputReturn
  - NumberInput
  - NumberInputState
  - Button
  - ColorInput
  - Update
---

# Input Widgets

## Overview

*WIP.* Text input is functional with cursor, placeholder, and focus callbacks; the widget surface described here is stable.

`TextInput` and `NumberInput` are the two ways to collect typed user input. Both
render as a single-line box, both raise the same lifecycle events, and
`NumberInput` is implemented as a thin wrapper that adds numeric validation on
top of `TextInput`.

## TextInput

`TextInput` is the one-line text field. State and callbacks live together in
the widget value:

```rust,ignore
pub struct TextInput {
    pub text_input_state: TextInputStateWrapper,
    pub placeholder_style: CssPropertyWithConditionsVec,
    pub container_style:   CssPropertyWithConditionsVec,
    pub label_style:       CssPropertyWithConditionsVec,
}

pub struct TextInputState {
    pub text:        U32Vec,
    pub placeholder: OptionString,
    pub max_len:     usize,
    pub selection:   OptionTextInputSelection,
    pub cursor_pos:  usize,
}
```

The text buffer is stored as `U32Vec` (Unicode scalar values cast to `u32`) so
cursor positions are character-based and survive multi-byte UTF-8 edits without
re-indexing.

### Building one

```rust,no_run
use azul::prelude::*;

let dom = TextInput::create()
    .with_text("hello".into())
    .with_placeholder("type something...".into())
    .dom();
```

`create()` returns a `TextInput` with platform defaults. `with_text` replaces
the buffer; `set_text` is its mutable counterpart.

### Three callbacks

`TextInput` exposes three callbacks.

- `on_text_input` fires when a printable character is typed. It returns
  `OnTextInputReturn { update, valid }`.
- `on_virtual_key_down` fires when a non-printable key (arrows, Enter, Esc) is
  pressed. It returns `OnTextInputReturn { update, valid }`.
- `on_focus_lost` fires when focus moves away. It returns `Update`.

`OnTextInputReturn { valid: TextInputValid::No }` rejects the just-typed
character. The framework rolls the buffer back to the pre-edit state and
re-renders. `Yes` keeps the edit.

```rust,no_run
use azul::prelude::*;

extern "C" fn validate(
    _data: RefAny, _info: CallbackInfo, state: TextInputState,
) -> OnTextInputReturn {
    let valid = if state.get_text().chars().all(|c| c.is_alphanumeric()) {
        TextInputValid::Yes
    } else {
        TextInputValid::No
    };
    OnTextInputReturn { update: Update::DoNothing, valid }
}

let dom = TextInput::create()
    .with_on_text_input(RefAny::new(()), validate)
    .dom();
```

By default, the framework writes the new character into the buffer before
`on_focus_lost` and `on_virtual_key_down` fire. Set
`update_text_input_before_calling_focus_lost_fn = false` if your callback needs
to compare against the old buffer.

### Cursor and selection

`cursor_pos` is the character index. `selection` is `OptionTextInputSelection`,
which is either `All` or a `FromTo` range carrying `dir_from` and `dir_to`.

### Styling

The default style is a white box with a thin border that turns blue on focus.
Three slots accept CSS replacements.

- `container_style` targets the outer wrapper (border, padding, background).
- `label_style` targets the actual text node.
- `placeholder_style` targets the placeholder text shown when empty.

## NumberInput

`NumberInput` wraps `TextInput` and adds `f32` validation. Construction takes
the initial value:

```rust,no_run
use azul::prelude::*;

let dom = NumberInput::create(42.0).dom();
```

### State

```rust,ignore
pub struct NumberInputState {
    pub previous: f32,
    pub number:   f32,
    pub min:      f32,
    pub max:      f32,
}
```

`previous` is set to the prior `number` whenever a successful edit commits, so
the value-change callback can compute deltas without storing its own history.

> **Default min:** the constructor leaves `min` at `0.0`. Set it to a negative
> value if your input must accept negative numbers.

### Callbacks

`NumberInput` exposes its own value-change and focus-lost hooks, plus the
underlying `TextInput`'s text-input and virtual-key-down hooks.

- `with_on_value_change(data, fn)` fires after a typed string parses
  successfully as `f32` and lies within `[min, max]`.
- `with_on_focus_lost(data, fn)` fires when the input loses focus, regardless
  of validity.
- `with_on_text_input(data, fn)` is forwarded to the underlying `TextInput`.
- `with_on_virtual_key_down(data, fn)` is forwarded to the underlying
  `TextInput`.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_change(
    _d: RefAny, 
    _i: CallbackInfo, 
    state: NumberInputState,
) -> Update {
    let _ = state.number - state.previous;
    Update::DoNothing
}

let dom = NumberInput::create(50.0)
    .with_on_value_change(RefAny::new(()), on_change)
    .dom();
```

Invalid input is rejected by the underlying `TextInput`'s `on_text_input`
callback returning `TextInputValid::No`, so the buffer never enters an
unparseable state.

## Picking between them

- For free-form text, names, or search, use `TextInput`.
- For numbers with a known range, use `NumberInput`.
- For typed URLs, emails, or regex-validated input, use `TextInput` with a
  custom `on_text_input` returning `TextInputValid::No` for invalid characters.
- For a date or time, build one out of three `NumberInput`s; there's no
  date/time widget yet.

## Coming Up Next

- [Structural Widgets](structural.md) — Panels, splitters, tab views, list views, tree views
- [Text Input](../text-input.md) — Editable text, IME, and the selection model
- [Events](../events.md) — Callbacks, event filters, and how state triggers relayout

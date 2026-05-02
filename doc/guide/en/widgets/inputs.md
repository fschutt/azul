---
slug: widgets/inputs
title: Input Widgets
language: en
canonical_slug: widgets/inputs
audience: external
maturity: wip
guide_order: 141
topic_only: false
short_desc: Text fields, checkboxes, radios, sliders, dropdowns, and the small-aria info struct that wires accessibility.
prerequisites: [widgets, text-input, text-selection]
tracked_files:
  - layout/src/widgets/text_input.rs
  - layout/src/widgets/number_input.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:43Z
---

# Input Widgets

> **WIP** — text input is functional with cursor, placeholder, and focus
> callbacks. IME, undo/redo, and clipboard integration live in the higher-level
> text-input *manager* (`layout/src/managers/text_input.rs`); this page
> describes the widget surface that any layout sees.

`TextInput` and `NumberInput` are the two ways to collect typed user input.
Both render as a single-line box with platform-native styling, both raise
the same lifecycle events, and `NumberInput` is implemented as a thin
wrapper that adds numeric validation on top of `TextInput`.

## `TextInput`

`TextInput` (`layout/src/widgets/text_input.rs:539`) is the one-line text
field. State and callbacks live together in the widget value:

```rust,ignore
pub struct TextInput {
    pub text_input_state: TextInputStateWrapper,
    pub placeholder_style: CssPropertyWithConditionsVec,
    pub container_style:   CssPropertyWithConditionsVec,
    pub label_style:       CssPropertyWithConditionsVec,
}

pub struct TextInputState {
    pub text:        U32Vec,           // Vec<char>
    pub placeholder: OptionString,
    pub max_len:     usize,            // default 50
    pub selection:   OptionTextInputSelection,
    pub cursor_pos:  usize,
}
```

The text buffer is stored as `U32Vec` (a vector of Unicode scalar values
cast to `u32`) so cursor positions are character-based and survive
multi-byte UTF-8 edits without re-indexing.

### Building one

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::text_input::TextInput;
let dom = TextInput::create()
    .with_text("hello".into())
    .with_placeholder("type something...".into())
    .dom();
```

`create()` returns a `TextInput` with platform defaults (white background,
1 px border, system UI font). `with_text` replaces the buffer; `set_text`
is its mutable counterpart.

### Three callbacks

`TextInput` exposes three callbacks, all returning a discriminated update
type:

| Callback | Fired when | Return |
|---|---|---|
| `on_text_input` | A printable character is typed | `OnTextInputReturn { update, valid }` |
| `on_virtual_key_down` | A non-printable key (arrows, Enter, Esc) is pressed | `OnTextInputReturn { update, valid }` |
| `on_focus_lost` | Focus moves away from the input | `Update` |

`OnTextInputReturn::valid = TextInputValid::No` rejects the just-typed
character. The framework rolls the buffer back to the pre-edit state and
re-renders. `Yes` keeps the edit.

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::text_input::{TextInput, TextInputState, OnTextInputReturn, TextInputValid};
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

By default, the framework writes the new character into the buffer
*before* `on_focus_lost` and `on_virtual_key_down` fire — set
`update_text_input_before_calling_focus_lost_fn = false` if your callback
needs to compare against the old buffer.

### Cursor and selection

The cursor is a 1 px-wide animated bar managed by an internal timer
(`cursor_animation: OptionTimerId`). `cursor_pos` is the character index;
`selection` is `Option<TextInputSelection>` which is either `All` or a
`FromTo { dir_from, dir_to }` range.

For higher-level concerns — IME composition, multi-cursor editing, undo,
clipboard — see `layout/src/managers/text_input.rs`. The widget itself only
exposes the data the application needs to read or write: the current text,
where the cursor is, and what is selected.

### Styling

The default style is a white box with a 1 px gray border that turns blue
on focus. Three slots accept full CSS replacements:

| Field | Targets |
|---|---|
| `container_style` | the outer wrapper (border, padding, background) |
| `label_style` | the actual text node |
| `placeholder_style` | the gray placeholder text shown when empty |

For one-off tweaks, prefer adding inline CSS to the result of `dom()` —
the widget already attaches `__azul-native-text-input`-prefixed classes you
can target from a stylesheet.

## `NumberInput`

`NumberInput` (`layout/src/widgets/number_input.rs:59`) wraps `TextInput`
and adds `f32` validation. Construction takes the initial value:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::number_input::NumberInput;
let dom = NumberInput::create(42.0).dom();
```

Internally the widget owns both a `NumberInputStateWrapper` (the numeric
state plus its callbacks) and a `TextInput` (the rendering surface). The
wrapped text input is reachable via the same `*_style` setters, so you can
restyle it identically:

```rust,no_run
# use azul::prelude::*;
# use azul::css::dynamic_selector::CssPropertyWithConditionsVec;
# use azul::widgets::number_input::NumberInput;
# fn my_style() -> CssPropertyWithConditionsVec { panic!() }
let dom = NumberInput::create(0.0)
    .with_container_style(my_style())
    .dom();
```

### State

```rust,ignore
pub struct NumberInputState {
    pub previous: f32,           // value before the most recent change
    pub number:   f32,           // current value
    pub min:      f32,           // default: 0.0 (see note)
    pub max:      f32,           // default: f32::MAX
}
```

`previous` is set to the prior `number` whenever a successful edit
commits, so the value-change callback can compute deltas without storing
its own history.

> **Default min:** the constructor leaves `min` at `0.0`, which is
> surprising for a general-purpose number input. Set
> `state.inner.min = f32::MIN` after `create` if your input must accept
> negative numbers.

### Callbacks

`NumberInput` exposes its own value-change and focus-lost hooks, plus
the underlying `TextInput`'s text-input and virtual-key-down hooks:

| Method | Fires |
|---|---|
| `with_on_value_change(data, fn)` | After a typed string parses successfully as `f32` and lies within `[min, max]` |
| `with_on_focus_lost(data, fn)` | When the input loses focus, regardless of validity |
| `with_on_text_input(data, fn)` | Forwarded to the underlying `TextInput` |
| `with_on_virtual_key_down(data, fn)` | Forwarded to the underlying `TextInput` |

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::number_input::{NumberInput, NumberInputState};
extern "C" fn on_change(
    _d: RefAny, _i: CallbackInfo, state: NumberInputState,
) -> Update {
    // state.previous → state.number is the just-committed delta
    let _ = state.number - state.previous;
    Update::DoNothing
}

let dom = NumberInput::create(50.0)
    .with_on_value_change(RefAny::new(()), on_change)
    .dom();
```

Invalid input — non-numeric characters, values outside `[min, max]` — is
rejected by the underlying `TextInput`'s `on_text_input` callback returning
`TextInputValid::No`, so the buffer never enters an unparseable state.

## Picking between them

| Need | Use |
|---|---|
| free-form text, names, search | `TextInput` |
| numbers with a known range | `NumberInput` |
| typed URLs / emails / regex-validated input | `TextInput` + custom `on_text_input` returning `TextInputValid::No` for invalid characters |
| a date or time | neither (no widget yet — build one out of three `NumberInput`s) |

For multi-line text editing the widget is not yet implemented (the module
declaration in `layout/src/widgets/mod.rs:156` is commented out). Wrap
your own `Dom::create_text` in a focusable `<div contenteditable>` if you
need a textarea today.

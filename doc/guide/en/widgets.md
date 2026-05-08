---
slug: widgets
title: Built-in Widgets
language: en
canonical_slug: widgets
audience: external
maturity: wip
guide_order: 140
topic_only: false
short_desc: Built-in widgets and how to write your own
prerequisites: [styling, events, text-input]
tracked_files:
  - layout/src/widgets/button.rs
  - layout/src/widgets/check_box.rs
  - layout/src/widgets/color_input.rs
  - layout/src/widgets/drop_down.rs
  - layout/src/widgets/file_input.rs
  - layout/src/widgets/frame.rs
  - layout/src/widgets/label.rs
  - layout/src/widgets/mod.rs
  - layout/src/widgets/progressbar.rs
  - layout/src/widgets/titlebar.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:43Z
---

# Built-in Widgets

> **WIP.** Most leaf widgets (`Button`, `CheckBox`, `ColorInput`, `DropDown`,
> `FileInput`, `ProgressBar`, `Frame`, `Titlebar`) are usable today. Container
> widgets with richer state (see [Structural Widgets](widgets/structural.md))
> have known gaps that are called out per widget.

A widget is a value with a `dom()` method that returns a `Dom` subtree. Widgets
carry per-platform default styles, store callbacks in their own state, and
convert into a `Dom` exactly once. Because the output is a plain `Dom`, you can
mix widgets and hand-rolled markup freely.

```rust,no_run
use azul::prelude::*;

let dom = Dom::create_div()
    .with_child(Button::create("OK".into()).dom());
```

This page covers the leaf widgets. For text and numeric input, and the larger
composite widgets, see:

- [Input Widgets](widgets/inputs.md): `TextInput`, `NumberInput`.
- [Structural Widgets](widgets/structural.md): `TabHeader`, `TreeView`,
  `ListView`, `Ribbon`.

## Common shape

Every widget follows the same builder pattern.

- Construct with `Widget::create(...)` (or `new`).
- Set callbacks with `with_on_*(data, fn)` or `set_on_*(...)`.
- Override styles with `with_*_style(css)` or `set_*_style(...)`.
- Convert to a DOM with `widget.dom()`.

Callbacks are `extern "C" fn(RefAny, CallbackInfo, ...) -> Update`. The `RefAny`
argument is the value you passed to `with_on_*` and is downcast back to your
application state inside the callback.

## Button

`Button` renders as a button node with a label child. `ButtonType` selects one
of eight variants for color and contrast.

```rust,ignore
pub enum ButtonType {
    Default, Primary, Secondary, Success,
    Danger,  Warning, Info,      Link,
}
```

```rust,no_run
use azul::prelude::*;

extern "C" fn on_click(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

let dom = Button::create("Save".into())
    .with_button_type(ButtonType::Primary)
    .with_on_click(RefAny::new(()), on_click)
    .dom();
```

The button is focusable. Set `Button.image` to put an icon next to the label.

## CheckBox

`CheckBox` is a small box that toggles its filled state on click. The toggle
callback receives the post-click `CheckBoxState`.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_toggle(_d: RefAny, _i: CallbackInfo, _s: CheckBoxState) -> Update {
    Update::RefreshDom
}

let dom = CheckBox::create(true)
    .with_on_toggle(RefAny::new(()), on_toggle)
    .dom();
```

## ColorInput

`ColorInput` is a colored swatch that fires a callback when clicked. Azul does
not ship a built-in color-picker dialog. Open one yourself in the callback (see
[File Dialogs](file-dialogs.md) for `ColorPickerDialog`).

```rust,no_run
use azul::prelude::*;

extern "C" fn on_change(_d: RefAny, _i: CallbackInfo, _s: ColorInputState) -> Update {
    Update::DoNothing
}

let dom = ColorInput::create(ColorU { r: 255, g: 128, b: 0, a: 255 })
    .with_on_value_change(RefAny::new(()), on_change)
    .dom();
```

## FileInput

`FileInput` renders as a button labelled either the placeholder text or the
basename of the currently selected path. Like `ColorInput`, it does not show a
native file dialog itself. Your callback owns that.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_path(_d: RefAny, _i: CallbackInfo, _s: FileInputState) -> Update {
    Update::RefreshDom
}

let dom = FileInput::create(None.into())
    .with_default_text("Open project...".into())
    .with_on_path_change(RefAny::new(()), on_path)
    .dom();
```

## DropDown

`DropDown` shows the currently selected choice plus a dropdown indicator.
Receiving focus opens a native menu populated from `choices`. Selecting an item
invokes the callback with the chosen index.

```rust,no_run
use azul::prelude::*;

extern "C" fn on_pick(_d: RefAny, _i: CallbackInfo, _idx: usize) -> Update {
    Update::RefreshDom
}

let choices = StringVec::from_vec(vec!["Red".into(), "Green".into(), "Blue".into()]);
let dom = DropDown::create(choices)
    .with_on_choice_change(RefAny::new(()), on_pick)
    .dom();
```

The trigger is focusable. Tabbing to it and pressing Space opens the menu just
like a click.

## ProgressBar

`ProgressBar` shows a horizontal bar filled to a percentage. The completion is
clamped to `[0, 100]`. Supply your own `bar_background` and
`container_background` for custom palettes.

```rust,no_run
use azul::prelude::*;

let dom = ProgressBar::create(42.5).dom();
```

`set_height` overrides the bar height. The rendered bar scales with its
container.

## Frame

`Frame` is a titled border container, analogous to an HTML fieldset or a Win32
group box: a thin border with the title text inset into the top edge.

```rust,no_run
use azul::prelude::*;

let dom = Frame::create(
    "Network".into(),
    Dom::create_div(),
).dom();
```

## Titlebar

`Titlebar` renders a custom window chrome bar. Use `Titlebar::dom` for a
title-only bar (the OS still draws the close, minimize, and maximize buttons).
Use `Titlebar::domWithButtons(...)` for a windowless mode where the app draws
its own controls.

Pair the buttoned form with `WindowDecorations::None` (see
[Windowing](windowing.md)) to draw the entire title bar yourself.

## Styling

Every widget exposes its inline default styles as a public field you can replace
before calling `dom()`. For one-off styling, prefer adding CSS to the produced
`Dom` via `Dom::with_css_property` or `Dom::with_css`.

```rust,no_run
use azul::prelude::*;

let dom = Button::create("Save".into()).dom();
```

## Coming Up Next

- [Input Widgets](widgets/inputs.md) — Text fields, checkboxes, radios, sliders, dropdowns
- [Structural Widgets](widgets/structural.md) — Panels, splitters, tab views, list views, tree views
- [Accessibility](accessibility.md) — Screen reader integration and ARIA roles

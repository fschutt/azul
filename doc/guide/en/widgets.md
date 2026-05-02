---
slug: widgets
title: Built-in Widgets
language: en
canonical_slug: widgets
audience: external
maturity: wip
guide_order: 140
topic_only: false
short_desc: Built-in widgets — buttons, labels, lists, dialogs — and the conventions for writing your own.
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

> **WIP** — most leaf widgets (Button, Label, CheckBox, ColorInput, DropDown,
> FileInput, ProgressBar, Frame, Titlebar) are usable today. Container widgets
> with richer state — see [Structural Widgets](widgets/structural.md) — have
> known gaps that are called out per widget.

A widget in azul is a value type with a `dom()` method that returns a
`Dom` subtree. Widgets carry per-platform default styles, store callbacks
in their own state, and convert into DOM exactly once. Because the output
is a plain `Dom`, you mix widgets and hand-rolled markup freely:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::{button::Button, label::Label};
let dom = Dom::create_div()
    .with_child(Label::create("Click below".into()).dom())
    .with_child(Button::create("OK".into()).dom());
```

This page covers the leaf widgets. For text editing, numeric input, and
the larger composite widgets, see:

- [Input Widgets](widgets/inputs.md) — `TextInput`, `NumberInput`
- [Structural Widgets](widgets/structural.md) — `Tabs`, `ListView`,
  `TreeView`, `Ribbon`, `NodeGraph`

All widgets live in `layout::widgets`. The shared callback macro
`impl_widget_callback!` (`layout/src/widgets/mod.rs:8`) generates the
boilerplate (`Wrapper { refany, callback }`, `OptionWrapper`, `From`
impls) used throughout.

## Common shape

Every widget follows the same builder pattern:

| Step | Method |
|---|---|
| Construct | `Widget::create(...)` (or `new`) |
| Set callbacks | `with_on_click(data, fn)` / `set_on_click(...)` |
| Override styles | `with_*_style(css)` / `set_*_style(...)` |
| Convert to DOM | `widget.dom()` |

Callbacks are `extern "C" fn(RefAny, CallbackInfo, ...) -> Update`. The
`RefAny` argument is the value you passed to `with_on_*` and is downcast
back to your application state inside the callback.

## Button

`Button` (`layout/src/widgets/button.rs:74`) renders as a `<button>` node
with a label child. `ButtonType` selects one of eight Bootstrap-derived
variants for color and contrast:

```rust,ignore
pub enum ButtonType {
    Default, Primary, Secondary, Success,
    Danger,  Warning, Info,      Link,
}
```

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::button::{Button, ButtonType};
extern "C" fn on_click(_data: RefAny, _info: CallbackInfo) -> Update {
    Update::DoNothing
}

let dom = Button::create("Save".into())
    .with_button_type(ButtonType::Primary)
    .with_on_click(RefAny::new(()), on_click)
    .dom();
```

The button registers `HoverEventFilter::MouseUp` for clicks and is
focusable (`TabIndex::Auto`). Pass an `ImageRef` via `set_image` to put
an icon next to the label. Each variant attaches the corresponding
`__azul-btn-*` class (e.g. `__azul-btn-primary`) so a stylesheet can
override the platform default.

## Label

`Label` (`layout/src/widgets/label.rs:18`) is centered text with
platform-appropriate defaults — 13 px on Windows/Linux, 12 px on macOS,
with `system:ui` as the font family. Use it for headings, status lines,
and any place you want OS-native typography without writing CSS.

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::label::Label;
let dom = Label::create("Ready.".into()).dom();
```

The output node has the `__azul-native-label` class — restyle it with
project-wide CSS if you need a different font.

## CheckBox

`CheckBox` (`layout/src/widgets/check_box.rs:43`) is a 14 × 14 box that
toggles its filled state on `MouseUp`. The toggle callback receives the
post-click `CheckBoxState`:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::check_box::{CheckBox, CheckBoxState};
extern "C" fn on_toggle(_d: RefAny, _i: CallbackInfo, _s: CheckBoxState) -> Update {
    Update::RefreshDom
}

let dom = CheckBox::create(true)
    .with_on_toggle(RefAny::new(()), on_toggle)
    .dom();
```

The widget toggles its visual state by animating the inner box's opacity
from 0 to 100, so the DOM does not need to be rebuilt on every click.

## ColorInput

`ColorInput` (`layout/src/widgets/color_input.rs:24`) is a coloured swatch
that fires a callback when clicked. Azul does **not** ship a built-in
color-picker dialog — open one yourself in the callback:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::color_input::{ColorInput, ColorInputState};
extern "C" fn on_change(_d: RefAny, _i: CallbackInfo, _s: ColorInputState) -> Update {
    // open your own picker; update state when the user confirms
    Update::DoNothing
}

let dom = ColorInput::create(ColorU { r: 255, g: 128, b: 0, a: 255 })
    .with_on_value_change(RefAny::new(()), on_change)
    .dom();
```

## FileInput

`FileInput` (`layout/src/widgets/file_input.rs:28`) renders as a button
labelled either the placeholder text (default `"Select File..."`) or the
basename of the currently selected path. Like `ColorInput`, it does not
show a native file dialog itself — your callback owns that:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::file_input::{FileInput, FileInputState};
extern "C" fn on_path(_d: RefAny, _i: CallbackInfo, _s: FileInputState) -> Update {
    Update::RefreshDom
}

let dom = FileInput::create(None.into())
    .with_default_text("Open project...".into())
    .with_on_path_change(RefAny::new(()), on_path)
    .dom();
```

The widget always returns at least `Update::RefreshDom` so the displayed
filename refreshes after the callback completes.

## DropDown

`DropDown` (`layout/src/widgets/drop_down.rs`) shows the currently
selected choice plus a `▾` icon. Receiving focus opens a native menu
populated from `choices`; selecting an item invokes the callback with the
chosen index:

```rust,no_run
# use azul::prelude::*;
# use azul::vec::StringVec;
# use azul::widgets::drop_down::DropDown;
extern "C" fn on_pick(_d: RefAny, _i: CallbackInfo, _idx: usize) -> Update {
    Update::RefreshDom
}

let choices = StringVec::from_vec(vec!["Red".into(), "Green".into(), "Blue".into()]);
let dom = DropDown::new(choices)
    .with_on_choice_change(RefAny::new(()), on_pick)
    .dom();
```

Because the popup is a real OS menu (via `azul_core::menu`), it stays open
across window resizes and respects the platform's menu styling. The
trigger node is focusable; tabbing to it and pressing Space opens the
menu just like a click.

## ProgressBar

`ProgressBar` (`layout/src/widgets/progressbar.rs:174`) shows a 15 px-tall
horizontal bar filled to a percentage. The completion is clamped to
`[0, 100]`. Supply your own `bar_background` and `container_background` for
custom palettes:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::progressbar::ProgressBar;
let dom = ProgressBar::create(42.5).dom();
```

`set_height` overrides the bar height; the rendered bar is two flex
children sized via percentage widths so it scales with its container.

## Frame

`Frame` (`layout/src/widgets/frame.rs:217`) is a titled border container,
analogous to HTML `<fieldset>` or a Win32 group box: a 1 px border with the
title text inset into the top edge.

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::frame::Frame;
let dom = Frame::create(
    "Network".into(),
    Dom::create_div().with_inline_css("padding: 8px;"),
).dom();
```

## Titlebar

`Titlebar` (`layout/src/widgets/titlebar.rs:104`) renders a custom window
chrome bar with two modes:

| Mode | Method | When to use |
|---|---|---|
| Title-only | `Titlebar::dom()` | OS still draws the close/minimize/maximize buttons |
| Full CSD | `Titlebar::dom_with_buttons(...)` | windowless mode where the app must draw its own controls |

Default sizes match each platform: 28 px on macOS, 32 px on Windows, 30 px
on Linux. `Titlebar::from_system_style` and `from_system_style_csd`
populate the metrics from the runtime `SystemStyle` so the bar matches
discovered system fonts and accent colours.

Pair the CSD form with `WindowDecorations::None` and
`has_decorations: true` (see [Windowing](windowing.md)) to draw the
entire title bar yourself.

## What about styling?

Every widget exposes its inline default styles as
`CssPropertyWithConditionsVec` fields you can replace before calling
`dom()`. For one-off styling, prefer adding inline CSS to the produced
`Dom` instead:

```rust,no_run
# use azul::prelude::*;
# use azul::widgets::button::Button;
let dom = Button::create("Save".into())
    .dom()
    .with_inline_css("min-width: 120px; margin: 8px;");
```

This composes with the widget's own classes (`__azul-native-button`,
`__azul-btn-default`, etc.) — your stylesheet can target either to roll
project-wide themes.

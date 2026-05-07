---
slug: windowing
title: Windows, Menus, Decorations
language: en
canonical_slug: windowing
audience: external
maturity: mature
guide_order: 130
topic_only: false
short_desc: Windows, menus, decorations, and per-window state
prerequisites: [hello-world, events]
tracked_files:
  - core/src/window.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T05:54:43Z
---

# Windows, Menus, Decorations

A window is created by passing a `WindowCreateOptions` to `App::run` (the first window) or to `CallbackInfo::create_window` (subsequent windows). The framework manages a window's lifecycle, decorations, menus, and per-window state; your code describes what should happen via the layout callback and event handlers.

```rust,no_run
use azul::prelude::*;

struct App;

extern "C" fn layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

fn main() {
    let app = App::create(RefAny::new(App), AppConfig::create());
    app.run(WindowCreateOptions::create(layout));
}
```

## Configuring the first window

`WindowCreateOptions::create(layout_callback)` returns a struct with sensible defaults: a 640×480 window, light theme, smooth scrolling, native menus on Windows/macOS. Tweak via the public fields:

```rust,no_run
use azul::prelude::*;

extern "C" fn layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

let mut win = WindowCreateOptions::create(layout);
win.window_state.title = "My App".into();
win.window_state.size.dimensions = LogicalSize::new(1024.0, 768.0);
win.window_state.flags.is_resizable = true;
win.size_to_content = false;
```

`WindowCreateOptions` public fields:

- `window_state: FullWindowState` — initial state (size, title, flags, ...).
- `create_callback: OptionCallback` — optional fn called once after window opens.
- `renderer: OptionRendererOptions` — VSync, sRGB, hardware accel.
- `theme: OptionWindowTheme` — light/dark override.
- `size_to_content: bool` — resize to fit first layout (default false).
- `hot_reload: bool` — CSS hot-reload on file change.

`FullWindowState` carries title, size, flags, position, theme, and the layout callback. The framework keeps it in sync with the OS as the user resizes and moves the window.

## Window flags

`WindowFlags` groups boolean window properties. The defaults match a normal application window:

- `frame: WindowFrame` — `Normal`, `Minimized`, `Maximized`, or `Fullscreen`.
- `decorations: WindowDecorations` — controls the title bar (see below).
- `background_material: WindowBackgroundMaterial` — compositor blur/transparency.
- `window_type: WindowType` — `Normal`, `Menu`, `Tooltip`, or `Dialog`.
- `is_visible`, `is_resizable`, `has_focus` — standard toggles.
- `is_always_on_top`, `smooth_scroll_enabled`, `autotab_enabled`.
- `use_native_menus`, `use_native_context_menus` — default to true on Windows/macOS, false on Linux.
- `is_top_level`, `prevent_system_sleep`.
- `fullscreen_mode: FullScreenMode` — animation for fullscreen transitions.
- `has_decorations` — opt-in for client-side decorations.
- `close_requested` — set by the OS, cleared by your callback to veto a close.

To toggle a flag at runtime, modify the window state from inside a callback:

```rust,no_run
use azul::prelude::*;

extern "C" fn toggle_fullscreen(_: RefAny, mut info: CallbackInfo) -> Update {
    let mut state = info.get_current_window_state().clone();
    state.flags.frame = match state.flags.frame {
        WindowFrame::Fullscreen => WindowFrame::Normal,
        _ => WindowFrame::Fullscreen,
    };
    info.modify_window_state(state);
    Update::DoNothing
}
```

## Decorations and CSD

`WindowDecorations` controls the title bar:

- `Normal` — full native title bar with controls.
- `NoTitle` — native frame, but no title text. App must draw its own.
- `NoTitleAutoInject` — same as `NoTitle`, but the framework prepends a styled title bar to your DOM.
- `NoControls` — title bar present, controls (min/max/close) hidden.
- `None` — borderless. Combine with `has_decorations: true` for full client-side decorations.

Client-side decorations are mandatory on Wayland (no native protocol), opt-in on Windows/macOS/X11. With `decorations: WindowDecorations::None` and `has_decorations: true`, the framework lets you draw the title bar yourself; mark a node with the class `__azul-native-titlebar` to make dragging it move the window.

`WindowDecorations::NoTitleAutoInject` is the easy path: the framework injects a default title bar that respects the system style and handles drag automatically.

`WindowFrame` controls the frame state. Setting `frame` and calling `modify_window_state` performs the corresponding OS-level transition. On macOS, the speed of the fullscreen transition is controlled by `WindowFlags.fullscreen_mode` (`SlowFullScreen` plays the system animation; `FastFullScreen` is instant).

## Background materials

`WindowBackgroundMaterial` selects the platform compositor's blur/transparency effect:

- `Opaque`. Default on both macOS and Windows.
- `Transparent`. Translucent with no blur on both platforms.
- `Sidebar`. Sidebar material on macOS, light acrylic on Windows.
- `Menu`. Menu material on macOS, acrylic on Windows.
- `HUD`. HUD material on macOS, dark acrylic on Windows.
- `Titlebar`. Titlebar material on macOS, mica on Windows.
- `MicaAlt`. Equivalent to `Titlebar` on macOS, mica alt on Windows.

X11 and Wayland ignore this field. To use a transparent material, also set the layout body's CSS `background-color` to a translucent value.

## Multiple windows

From inside any callback, push a new `WindowCreateOptions`:

```rust,no_run
use azul::prelude::*;

extern "C" fn child_layout(_: RefAny, _: LayoutCallbackInfo) -> Dom {
    Dom::create_body()
}

extern "C" fn open_settings(mut data: RefAny, mut info: CallbackInfo) -> Update {
    let mut win = WindowCreateOptions::create(child_layout);
    win.window_state.title = "Settings".into();
    win.window_state.size.dimensions = LogicalSize::new(400.0, 300.0);
    info.create_window(win);
    Update::DoNothing
}
```

The new window opens after the callback returns. The two windows share the same `RefAny` if you pass it to both layout callbacks, or each window can carry its own data.

The application stays alive as long as at least one window is open; closing the last window ends the event loop and `App::run` returns.

To close the current window from a callback:

```rust,ignore
info.close_window();
```

To intercept the close button, set `FullWindowState.close_callback` on the state. Returning `Update::DoNothing` and clearing `flags.close_requested` keeps the window open:

```rust,no_run
use azul::prelude::*;

extern "C" fn on_close(_: RefAny, mut info: CallbackInfo) -> Update {
    let mut state = info.get_current_window_state().clone();
    state.flags.close_requested = false;       // veto the close
    info.modify_window_state(state);
    Update::DoNothing
}
```

## Menus

`Menu` is a tree of `MenuItem`s, used for both menu bars and context menus:

```rust,no_run
use azul::prelude::*;

struct State;

extern "C" fn on_open(_: RefAny, _: CallbackInfo) -> Update {
    Update::DoNothing
}

extern "C" fn on_quit(_: RefAny, _: CallbackInfo) -> Update {
    Update::DoNothing
}

let data: RefAny = RefAny::new(State);

let mut open_item = StringMenuItem::create("Open…".into());
open_item.callback = Some(CoreMenuCallback { callback: on_open.into(), refany: data.clone() }).into();

let mut quit_item = StringMenuItem::create("Quit".into());
quit_item.callback = Some(CoreMenuCallback { callback: on_quit.into(), refany: data }).into();

let file_menu = Menu::create(vec![
    MenuItem::String(open_item),
    MenuItem::Separator,
    MenuItem::String(quit_item),
].into());
```

`MenuItem` has three variants: `String(StringMenuItem)`, `Separator`, and `BreakLine` (for horizontal layouts).

`StringMenuItem` is built via `StringMenuItem::create(label)`, then populated through public fields:

- `accelerator: OptionVirtualKeyCodeCombo`
- `callback: OptionCoreMenuCallback`
- `children: MenuItemVec`
- `icon: OptionMenuItemIcon`
- `menu_item_state: MenuItemState` — `Normal`, `Disabled`, or `Greyed`.

```rust,ignore
let mut item = StringMenuItem::create("Bold".into());
item.accelerator = Some(VirtualKeyCodeCombo {
    keys: vec![VirtualKeyCode::LControl, VirtualKeyCode::B].into()
}).into();
item.menu_item_state = MenuItemState::Greyed;
item.icon = Some(MenuItemIcon::Checkbox(true)).into();
```

### Menu bars

Attach a `Menu` to the body of your DOM:

```rust,no_run
use azul::prelude::*;

fn build_menu() -> Menu {
    Menu::create(Vec::<MenuItem>::new().into())
}

let mut body = Dom::create_body().with_menu_bar(build_menu());
```

The framework uses native menus where supported (Windows HMENU, macOS NSMenu) when `WindowFlags.use_native_menus` is true; otherwise it renders a borderless azul popup window.

### Context menus

Attach a context menu to any node:

```rust,no_run
use azul::prelude::*;

fn build_menu() -> Menu {
    Menu::create(Vec::<MenuItem>::new().into())
}

let mut node = Dom::create_div().with_context_menu(build_menu());
```

The default trigger is right-click; change `Menu.context_mouse_btn` (`ContextMenuMouseButton::Right | Left | Middle`) for other triggers.

To open a menu programmatically (e.g. from a hamburger button click):

```rust,no_run
use azul::prelude::*;

fn build_menu() -> Menu {
    Menu::create(Vec::<MenuItem>::new().into())
}

extern "C" fn on_click(_: RefAny, mut info: CallbackInfo) -> Update {
    info.open_menu(build_menu());                                 // at cursor
    info.open_menu_at(build_menu(), LogicalPosition { x: 100.0, y: 50.0 });
    Update::DoNothing
}
```

`MenuPopupPosition` controls placement relative to the cursor or the clicked element (`BottomLeftOfCursor`, `TopOfHitRect`, `AutoCursor`, …). Set via `Menu::with_popup_position(...)` or `Menu::set_popup_position(...)`.

## Window types

`WindowType` tells the OS what kind of window this is, which affects taskbar/dock visibility, focus behaviour, and whether the window auto-closes on focus loss:

- `Normal` — standard application window.
- `Menu` — always-on-top, frameless, auto-closes on focus loss.
- `Tooltip` — always-on-top, no input.
- `Dialog` — blocks parent window (modal).

Set `window_state.flags.window_type` before passing to `create_window`.

## Tooltips

`CallbackInfo` exposes:

```rust,ignore
info.show_tooltip("Save the file".into());                            // at cursor
info.show_tooltip_at("...".into(), LogicalPosition { x: 100., y: 0. });
info.hide_tooltip();
```

Hover-triggered tooltips are automatic for any node with `aria-label`, `alt`, or `title` attributes. The framework starts a timer and shows the tooltip when it fires.

## Window state in callbacks

Read from `CallbackInfo`:

```rust,ignore
info.get_current_window_state()    // &FullWindowState
info.get_current_window_flags()    // WindowFlags
info.get_current_keyboard_state()  // KeyboardState
info.get_current_mouse_state()     // MouseState
```

Write via `info.modify_window_state(new_state)`. The framework diffs against the previous state and applies changes (moving the window, resizing, toggling flags) after the callback returns.

## Monitors

`Monitor` describes a connected display. Window-DPI changes (`WindowEventFilter::DpiChanged`) and monitor changes (`WindowEventFilter::MonitorChanged`) fire when the user drags a window to another display.

Monitor-connected and monitor-disconnected events are application-scoped (`ApplicationEventFilter::MonitorConnected` / `MonitorDisconnected`).

## Window icons and taskbar

`WindowIcon` and `TaskBarIcon` are RGBA byte buffers. Set them on the platform-specific options:

```rust,ignore
win.window_state.platform_specific_options.windows_options.window_icon =
    Some(WindowIcon::Small(SmallWindowIconBytes { ... })).into();
```

On X11, set `LinuxWindowOptions.window_icon`. macOS uses the icon from the `.app` bundle (no per-window override).

## Platform-specific options

`PlatformSpecificOptions` groups the four platforms:

- `windows_options: WindowsWindowOptions` — parent HWND, redirection bitmap, taskbar icon.
- `linux_options: LinuxWindowOptions` — X11/Wayland app ID, WM_CLASS, theme.
- `mac_options: MacWindowOptions` — reserved.
- `wasm_options: WasmWindowOptions` — reserved.

Linux options carry X11-specific hints (`x11_window_types`, `x11_wm_classes`, `x11_resize_increments`) and Wayland-specific identifiers (`wayland_app_id`, `wayland_theme`). Most apps don't need these — the defaults work.

## Common errors

- **Window opens off-screen** — `window_state.position` was set to a `WindowPosition::Initialized(...)` outside any monitor's bounds. Set to `Uninitialized` to let the OS place it.
- **Title bar disappears** — `decorations: WindowDecorations::None` without `has_decorations: true` produces a borderless window with no way to drag. Either set `has_decorations: true` (and draw your own title bar) or use `Normal` / `NoTitleAutoInject`.
- **Native menus on Linux** — `use_native_menus` defaults to false on Linux because Linux has no universal menu protocol. The framework renders a borderless azul popup instead.
- **Close button does nothing** — you registered a close callback that returned `Update::DoNothing` but didn't clear `flags.close_requested`. Clear it explicitly to veto the close.

## Coming Up Next

- [Events](events.md) — Callbacks, event filters, and how state triggers relayout
- [File Dialogs](file-dialogs.md) — Native open/save dialogs and folder pickers
- [Accessibility](accessibility.md) — Screen reader integration and ARIA roles
- [Built-in Widgets](widgets.md) — Built-in widgets and how to write your own

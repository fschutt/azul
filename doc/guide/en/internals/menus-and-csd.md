---
slug: menus-and-csd
title: Window Menus
language: en
canonical_slug: menus-and-csd
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Menus and client-side decorations across platforms
prerequisites: [code-organization, system-style]
tracked_files:
  - core/src/menu.rs
  - dll/src/desktop/csd.rs
  - dll/src/desktop/menu.rs
  - dll/src/desktop/menu_renderer.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
---

# Menus and Client-Side Decorations

> **WIP.** Submenu lifecycle tracking and image-icon rendering are unfinished
> ([`menu_renderer.rs:170`](../../../../dll/src/desktop/menu_renderer.rs),
> [`menu_renderer.rs:402`](../../../../dll/src/desktop/menu_renderer.rs)).
> The unified menu pipeline is the source of truth; the per-platform native
> menu modules (`shell2/windows/menu.rs`, `shell2/macos/menu.rs`,
> `shell2/linux/gnome_menu/`) wrap the same `Menu` data and exist in parallel.

Menus are Azul windows. CSD (client-side decorations) is an Azul DOM the shell
splices above the user's content. Both pipelines share `Menu` from
[`core/src/menu.rs`](../../../../core/src/menu.rs) and a stylesheet generator
on [`SystemStyle`](../../../../css/src/system.rs).

## The shared Menu model

[`core/src/menu.rs:40`](../../../../core/src/menu.rs) defines the cross-platform
data model. `#[repr(C)]` so the same bytes pass through the FFI boundary.

```rust,ignore
#[derive(Debug, Default, Clone, PartialEq, PartialOrd, Hash, Eq, Ord)]
#[repr(C)]
pub struct Menu {
    pub items: MenuItemVec,
    pub position: MenuPopupPosition,
    pub context_mouse_btn: ContextMenuMouseButton,
}

#[repr(C, u8)]
pub enum MenuItem {
    String(StringMenuItem),
    Separator,
    BreakLine,
}
```

`MenuPopupPosition` ([`core/src/menu.rs:91`](../../../../core/src/menu.rs)) is
ten variants — `AutoCursor`, `AutoHitRect`, four explicit cursor anchors, and
four explicit hit-rect anchors. Only `AutoCursor` and `AutoHitRect` flip on
overflow; the explicit variants clamp.

## Why callbacks are usize

`StringMenuItem.callback` is `OptionCoreMenuCallback`, where
[`CoreMenuCallback`](../../../../core/src/menu.rs) holds a `CoreCallback`
storing the function pointer as a `usize`:

```rust,ignore
#[repr(C)]
pub struct CoreMenuCallback {
    pub refany: RefAny,
    pub callback: CoreCallback, // usize-encoded fn pointer
}
```rust

`azul-core` cannot reference `azul-layout`'s `CallbackInfo` struct without
creating a dependency cycle, so the function pointer ships as an opaque
integer. `azul-layout` decodes it with
`azul_layout::callbacks::Callback::from_core(...)` (see
[`menu_renderer.rs:67`](../../../../dll/src/desktop/menu_renderer.rs)). The
memory layouts of `CoreCallback` and `Callback` are guaranteed identical;
[`layout/src/callbacks.rs`](../../../../layout/src/callbacks.rs) carries the
size/align asserts.

## Menus are windows

[`dll/src/desktop/menu.rs:381`](../../../../dll/src/desktop/menu.rs) is the
single entry point for showing any menu — context menu, dropdown,
submenu, CSD menu-bar dropdown:

```rust,ignore
pub fn show_menu(
    menu: Menu,
    system_style: Arc<SystemStyle>,
    parent_window_position: LogicalPosition,
    trigger_rect: Option<LogicalRect>,
    cursor_position: Option<LogicalPosition>,
    parent_menu_id: Option<u64>,
) -> WindowCreateOptions
```

It fills a `FullWindowState` with `WindowType::Menu`, `is_always_on_top = true`,
`is_resizable = false`, `decorations = WindowDecorations::None`,
`size_to_content = true`, and a layout callback (`menu_layout_callback`,
[`menu.rs:354`](../../../../dll/src/desktop/menu.rs)) that downcasts the
attached `MenuWindowData` and renders. The caller is expected to feed the
returned `WindowCreateOptions` to `info.create_window(...)` from inside an
event callback. This is identical on X11, Wayland, Windows, and macOS — no
platform branching in the call site.

`MenuWindowData` ([`menu.rs:29`](../../../../dll/src/desktop/menu.rs)) carries
everything the layout callback needs for that menu and any submenu it spawns:

```rust,ignore
pub struct MenuWindowData {
    pub menu: Menu,
    pub system_style: Arc<SystemStyle>,
    pub parent_window_position: LogicalPosition,
    pub trigger_rect: Option<LogicalRect>,
    pub cursor_position: Option<LogicalPosition>,
    pub parent_menu_id: Option<u64>,
    pub menu_window_id: Option<u64>,
    pub child_menu_ids: Arc<std::sync::Mutex<Vec<u64>>>,
}
```

`child_menu_ids` is intended for cascade close (closing a parent should close
all spawned submenu windows). It is allocated but never populated — the TODO at
[`menu_renderer.rs:170`](../../../../dll/src/desktop/menu_renderer.rs) is the
hole.

## Position math

[`calculate_menu_position`](../../../../dll/src/desktop/menu.rs) at
[`menu.rs:53`](../../../../dll/src/desktop/menu.rs) is the single positioning
function. It picks a reference point (cursor, then trigger-rect midpoint, then
parent window origin), looks up the display via
`get_display_at_point`/`get_primary_display`
([`dll/src/desktop/display.rs`](../../../../dll/src/desktop/display.rs)), and
then dispatches by `MenuPopupPosition`:

- **`AutoCursor`.** Tries the right-bottom of the cursor, flips horizontally or vertically on overflow, then clamps.
- **`AutoHitRect`.** Places at the right-bottom of the trigger, flips on overflow, then clamps.
- **`BottomRightOfCursor`, `BottomLeftOfCursor`, `TopRightOfCursor`, `TopLeftOfCursor`.** No flip. Clamp only.
- **`BottomOfHitRect`, `TopOfHitRect`.** Anchored vertically to the trigger rect, then clamped.
- **`RightOfHitRect`, `LeftOfHitRect`.** Submenu placement. Tries the named side, falls back to the opposite side on overflow. The top edge aligns with the trigger rect.

`clamp_to_work_area` ([`menu.rs:338`](../../../../dll/src/desktop/menu.rs)) is
the last step in every branch and forces `pos + menu_size` to stay inside
`display.work_area`. The work area is the display rect minus the OS taskbar /
panel — [`display.rs`](../../../../dll/src/desktop/display.rs) is responsible
for setting it correctly per platform.

## Rendering

[`menu_renderer.rs:187`](../../../../dll/src/desktop/menu_renderer.rs)
generates a `Dom` from a `Menu`:

```rust,ignore
pub fn create_menu_dom_with_css(
    menu: &Menu,
    system_style: &SystemStyle,
    menu_window_data: RefAny,
) -> Dom
```

Per-item HTML structure ([`menu_renderer.rs:264`](../../../../dll/src/desktop/menu_renderer.rs)):

```text
<div class="menu-item [menu-item-disabled|menu-item-greyed|menu-item-has-submenu]" id="menu-item-{idx}">
  <div class="menu-item-icon">[checkbox or image]</div>
  <div class="menu-item-label">Label Text</div>
  <div class="menu-item-shortcut">Ctrl+C</div>
  <div class="menu-item-arrow">▶</div>  <!-- only if has children -->
</div>
```rust

Two callbacks are wired per item, only when `menu_item_state` is `Normal`:

- `HoverEventFilter::MouseDown` →
  [`menu_item_click_callback`](../../../../dll/src/desktop/menu_renderer.rs)
  ([`menu_renderer.rs:52`](../../../../dll/src/desktop/menu_renderer.rs)).
  Decodes the stored `CoreCallback`, invokes the user's handler, then sets
  `state.flags.close_requested = true`.
- `HoverEventFilter::MouseOver` →
  [`submenu_hover_callback`](../../../../dll/src/desktop/menu_renderer.rs)
  ([`menu_renderer.rs:99`](../../../../dll/src/desktop/menu_renderer.rs)),
  attached only when `children` is non-empty. Builds a `Menu` with
  `MenuPopupPosition::RightOfHitRect`, calls `show_menu(...)`, hands the
  result to `info.create_window(...)`. The new window's ID is dropped — see
  the WIP banner.

`MenuItemIcon::Image(_)` is currently rendered as an empty `<div>`
([`menu_renderer.rs:401`](../../../../dll/src/desktop/menu_renderer.rs)).
Image rendering inside the menu DOM is not yet wired up.

## The menu stylesheet

[`SystemStyleMenuExt::create_menu_stylesheet`](../../../../dll/src/desktop/menu_renderer.rs)
at [`menu_renderer.rs:495`](../../../../dll/src/desktop/menu_renderer.rs)
synthesises the CSS from `SystemStyle` colours, fonts, and `corner_radius`:

- **`.menu-container`.** Background, border, `corner_radius`, `box-shadow`, and `min-width: 160px`.
- **`.menu-item`.** Flex row with padding, `cursor: pointer`, and `user-select: none`.
- **`.menu-item:hover`.** Uses `colors.selection_background` and `colors.selection_text`.
- **`.menu-item-disabled`, `.menu-item-greyed`.** Uses `colors.disabled_text`, `cursor: default`, and no hover.
- **`.menu-item-icon`.** 20x20 box with right margin.
- **`.menu-item-checkbox-checked`.** Bold checkmark glyph.
- **`.menu-item-label`.** `flex-grow: 1` and `white-space: nowrap`.
- **`.menu-item-shortcut`.** Right-aligned and dimmed via `opacity: 0.6`.
- **`.menu-item-arrow`.** Dimmed, used for the submenu indicator arrow.
- **`.menu-separator`.** 1 px line with padding.

The function builds a `String` via `format!`, parses it with
`new_from_str` ([`css/src/parser2.rs`](../../../../css/src/parser2.rs)), and
returns the first parsed `Stylesheet`. Parser warnings are routed through
`log_debug!(LogCategory::General, ...)` rather than surfaced. Padding is a
hard-coded `8.0` ([`menu_renderer.rs:553`](../../../../dll/src/desktop/menu_renderer.rs))
even though `corner_radius` is read from `metrics`.

The stylesheet uses `box-shadow`, `cursor`, `user-select`, `white-space`, and
`opacity`. Whether any of these are honoured by the layout/render path depends
on the parser's property whitelist; properties that are not understood are
silently dropped.

## Native menu bars (per-platform)

`show_menu` is the popup path. Application menu bars are still platform-native:

- **Win32.** Uses `CreateMenu` and `AppendMenuW`. Per-item `WM_COMMAND` IDs map to `CoreMenuCallback` via `BTreeMap<u16, CoreMenuCallback>`.
  - [`shell2/windows/menu.rs`](../../../../dll/src/desktop/shell2/windows/menu.rs)
- **macOS.** Uses `NSMenu` and `NSMenuItem` via `objc2`. A click invokes `AzulMenuTarget::menuItemAction:`, which pushes a tag to a global `Mutex<Vec<isize>>` drained by the event loop.
  - [`shell2/macos/menu.rs`](../../../../dll/src/desktop/shell2/macos/menu.rs)
- **GNOME.** Uses DBus `org.gtk.Menus` and `org.gtk.Actions`, exposed at a sanitised app object path. `dlopen`s `libdbus-1` to avoid a hard link dep.
  - [`shell2/linux/gnome_menu/`](../../../../dll/src/desktop/shell2/linux/gnome_menu/manager.rs)
- **X11 / Wayland popup-menus.** Defines a parallel `MenuLayoutData` plus a `menu_layout_callback` that mirrors [`menu.rs:354`](../../../../dll/src/desktop/menu.rs). `create_menu_window_options` and `create_menu_popup_options` exist but have no callers. `show_menu` is the live path.
  - [`shell2/linux/x11/menu.rs`](../../../../dll/src/desktop/shell2/linux/x11/menu.rs)
  - [`shell2/linux/wayland/menu.rs`](../../../../dll/src/desktop/shell2/linux/wayland/menu.rs)

The X11 / Wayland duplicates are dead-on-arrival and slated for removal. New
backends should call `show_menu`.

## CSD: when does it run

[`csd.rs:162`](../../../../dll/src/desktop/csd.rs) defines the gate:

```rust,ignore
#[inline]
pub fn should_inject_csd(has_decorations: bool, decorations: WindowDecorations) -> bool {
    has_decorations && decorations == WindowDecorations::None
}
```

If `has_decorations == false` the user wants a fully borderless window — no
titlebar at all. If `decorations` is anything other than `None` (`Normal`,
`NoTitle`, `NoControls`), the OS draws the titlebar — Azul stays out. Only the
`(true, None)` combination triggers DOM injection.

## CSD: what gets injected

[`wrap_user_dom_with_decorations`](../../../../dll/src/desktop/csd.rs) at
[`csd.rs:172`](../../../../dll/src/desktop/csd.rs) is the single splice point:

```rust,ignore
pub fn wrap_user_dom_with_decorations(
    user_dom: StyledDom,
    window_title: &str,
    should_inject_titlebar: bool,
    system_style: &SystemStyle,
) -> StyledDom
```

It looks at the user DOM's root `NodeData` for an attached `Menu`
(`get_menu_bar()`), then optionally appends:

1. **Titlebar** ([`csd.rs:94`](../../../../dll/src/desktop/csd.rs)) — built by
   [`Titlebar::from_system_style_csd`](../../../../layout/src/widgets/titlebar.rs)
   in `azul-layout`. `dom_with_buttons` returns a `Dom` with the close /
   minimise / maximise buttons; `SystemStyle::create_csd_stylesheet` styles it.
2. **Menu bar** ([`csd.rs:114`](../../../../dll/src/desktop/csd.rs)) —
   horizontal flex container of `<div class="csd-menubar-item">`s, one per
   top-level `MenuItem::String`. A `MouseDown` callback on each item calls
   `show_menu(...)` with the original menu's `children`.
3. **User content**.

The container is a `Dom::create_html()` (not a body) so the titlebar and user
content do not double-nest under `<body>`.

## CSD: the csd-* stylesheet

[`SystemStyle::create_csd_stylesheet`](../../../../css/src/system.rs) at
[`css/src/system.rs:1512`](../../../../css/src/system.rs) emits these classes:

- **`.csd-titlebar`.** 32 px high with `cursor: grab` and `user-select: none`.
- **`.csd-title`.** Text-overflow ellipsis, centred. Left-aligned on Linux.
- **`.csd-buttons`.** Flex row with a 4 px gap.
- **`.csd-button`.** 32x24, transparent, hover-tinted.
- **`.csd-button:hover`.** Tint depends on `Theme::Light` vs `Theme::Dark`.
- **`.csd-close:hover`.** Red, `rgb(232, 17, 35)` on every platform.
- **Platform overrides.** On macOS the traffic-light buttons are 12x12 and positioned absolutely at `left: 8px`. On Linux the title is left-aligned.

The macOS path positions `.csd-buttons` at `left: 8px` and overrides
`.csd-close`, `.csd-minimize`, `.csd-maximize` with their canonical
red / yellow / green circles. The Linux path only re-aligns the title; the
button group still uses the standard layout.

## Menu-bar dropdowns

[`csd_menubar_item_callback`](../../../../dll/src/desktop/csd.rs) at
[`csd.rs:34`](../../../../dll/src/desktop/csd.rs) is the bridge between CSD and
the unified menu pipeline. The callback's `RefAny` is a `Menu`. On click:

1. Read `system_style` from `CallbackInfo`.
2. Read the parent window's position from `WindowPosition::Initialized`.
3. Read the trigger rect from `info.get_hit_node_rect()`.
4. Build `WindowCreateOptions` via `crate::desktop::menu::show_menu(...)`.
5. Hand them to `info.create_window(...)`.

This is the same path a context menu takes — there is no separate "menubar
popup" implementation.

## Hashing for diff

[`Menu::get_hash`](../../../../core/src/menu.rs) at
[`core/src/menu.rs:77`](../../../../core/src/menu.rs) returns a 64-bit hash via
the standard library's `DefaultHasher`. It's used by
`shell2/windows/menu.rs::WindowsMenuBar.hash` to decide whether the native
`HMENU` needs to be rebuilt when a `Menu` is re-attached to a window. The
unified popup pipeline rebuilds the DOM every layout pass, so it does not use
the hash — but the type is `#[repr(C)]` and `Hash`, so any backend can.

## Coming Up Next

- [Shell2 Common Layer](shell2-common.md) — Shared shell infrastructure across platforms
- [Shell2 — Windows](shell2-windows.md) — Windows shell - Win32 messages, DirectComposition, IME
- [Shell2 — macOS](shell2-macos.md) — macOS shell - Cocoa, AppKit, IME, a11y

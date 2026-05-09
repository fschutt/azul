---
slug: windowing/linux-x11
title: Windowing — Linux X11
language: en
canonical_slug: windowing/linux-x11
audience: contributor
maturity: wip
guide_order: null
topic_only: false
short_desc: Linux X11 shell - Xlib, GLX, XInput2
prerequisites: [windowing/common]
tracked_files:
  - dll/src/desktop/shell2/linux/mod.rs
  - dll/src/desktop/shell2/linux/registry.rs
  - dll/src/desktop/shell2/linux/resources.rs
  - dll/src/desktop/shell2/linux/system_style.rs
  - dll/src/desktop/shell2/linux/timer.rs
  - dll/src/desktop/shell2/linux/common/gl.rs
  - dll/src/desktop/shell2/linux/common/mod.rs
  - dll/src/desktop/shell2/linux/x11/accessibility.rs
  - dll/src/desktop/shell2/linux/x11/clipboard.rs
  - dll/src/desktop/shell2/linux/x11/defines.rs
  - dll/src/desktop/shell2/linux/x11/dlopen.rs
  - dll/src/desktop/shell2/linux/x11/events.rs
  - dll/src/desktop/shell2/linux/x11/gl.rs
  - dll/src/desktop/shell2/linux/x11/menu.rs
  - dll/src/desktop/shell2/linux/x11/mod.rs
  - dll/src/desktop/shell2/linux/x11/tooltip.rs
last_generated_rev: 7ecd570e4c0c3584e5107e770058c16cb59fa6e7
generated_at: 2026-05-02T00:00:00Z
default-search-keys:
  - WindowCreateOptions
  - FullWindowState
  - WindowFlags
  - LayoutCallback
  - HoverEventFilter
---

# Windowing — Linux X11

## Overview

*WIP — XRandR DPI handling and the override-redirect tooltip path are still iterating. Most code paths are stable.* The X11 backend is `LinuxWindow::X11(X11Window)` selected by `LinuxWindow::select_backend`. Selection priority:

1. `AZ_BACKEND=x11` / `wayland` — explicit override.
2. `WAYLAND_DISPLAY` set → Wayland.
3. `DISPLAY` set → X11.
4. Otherwise `WindowError::NoBackendAvailable`.

`X11Window` holds an `Rc<Xlib>`, `Rc<Egl>`, `Rc<Xkb>`, optional `Rc<Xrender>` (for ARGB visuals), optional `Rc<Gtk3Im>` (for IME fallback when XIM is not available), the display + window IDs, the WM_DELETE_WINDOW atom, the IME manager, the render mode (`Gpu(GlContext, GlFunctions)` or `Cpu(Option<GC>)`), the tooltip window, the GNOME menu manager, the D-Bus connection (for screensaver inhibit), the embedded `event::CommonWindowState`, and the linux-specific `timer_fds` (`BTreeMap<TimerId, fd>`).

## Loading Xlib at runtime

`X11Window::new_with_resources` opens the Xlib + EGL + XKB shared libraries via `Library::load`, which wraps `libc::dlopen` with `RTLD_LAZY`. Each `Xlib`, `Egl`, `Xkb`, `Xrender`, `Gtk3Im` struct contains hundreds of function pointers populated by the `load_symbol!` macro.

`load_first_available::<Library>(&["libX11.so.6", "libX11.so"])` from the common dlopen module is used here for each library. The Xrender and GTK IM libs are optional — `new_with_resources` falls back gracefully when they fail to load (no ARGB transparency, no IME).

The custom error handler `x11_error_handler` is installed via `XSetErrorHandler` so non-fatal X errors get logged instead of terminating the process.

## Window creation: ARGB visual probe

`try_create_argb_window` attempts a 32-bit `TrueColor` visual with an alpha channel for true window-background transparency. The flow:

1. `XMatchVisualInfo(display, screen, 32, TrueColor, &out)` — probe for a 32-bit visual.
2. `XRenderFindVisualFormat(display, visual)` — confirm the visual has a non-zero alpha mask.
3. `XCreateColormap(display, root, visual, AllocNone)` — needed for non-default visuals.
4. `XCreateWindow` with that visual + colormap, depth 32.

The colormap is freed in `X11Window::close`. If any step fails the path falls back to the default visual and `has_argb_visual = false` is recorded for the renderer.

## Render modes

```rust,ignore
enum RenderMode {
    Gpu(gl::GlContext, GlFunctions),
    Cpu(Option<GC>),                  // X11 GC for XPutImage in CPU path
}
```

`gl::GlContext` wraps EGL: `eglGetDisplay` → `eglInitialize` → `eglBindAPI(EGL_OPENGL_API)` → `eglChooseConfig` with 8/8/8/8 RGBA → `eglCreateContext` → `eglCreateWindowSurface`. EGL is preferred over GLX because it works identically on Wayland; the function-pointer loader is the same.

CPU mode gets selected when:

- `AZ_BACKEND=cpu` / `AZUL_RENDERER=software`.
- GPU init fails (any step in `GlContext::new` returns an error).
- The GPU is blacklisted (see [Common](common.md)).

In CPU mode rendering goes through `cpurender` → `AzulPixmap` → `XPutImage` over the GC. Caches (`glyph_cache`, `retained_pixmap`, `previous_display_list`, `bgra_buffer`) are kept on the window so each frame reuses the previous frame's allocations.

## Event loop

`X11Window::poll_event` is the per-iteration body called from the multi-window loop. Sequence:

1. `check_timers_and_threads()` — drain any timerfds that have fired.
2. `XPending(display)` loop — drain the queue:
   - `XFilterEvent` first (consumed by IME → `continue`).
   - `XNextEvent` → dispatch by `event.type_`.

Per-event-type handlers:

- **`Expose`.** Calls `render_and_present()` directly. Per-rect expose is used when damage rects are available.
- **`FocusIn` / `FocusOut`.** Sets `window_focused` and `dynamic_selector_context.window_focused`, then calls `sync_ime_position_to_os()`.
- **`ConfigureNotify`.** Resize calls `regenerate_layout`. Position-only changes trigger a DPI re-check via `display::get_display_at_point`.
- **`ClientMessage`.** Compares against `wm_delete_window_atom` for window close.
- **`ButtonPress` / `Release`.** Routes to `handle_mouse_button`.
- **`MotionNotify`.** Routes to `handle_mouse_move`, which updates the hit test and fires `process_window_events`.
- **`KeyPress` / `Release`.** Routes to `handle_keyboard` for XKB translation and IME.
- **`EnterNotify` / `LeaveNotify`.** Routes to `handle_mouse_crossing`.
- **Dynamic XRandR event base.** Refreshes the monitor cache via `crate::desktop::display::get_monitors`.

If an event yields `ProcessEventResult != DoNothing`, `request_redraw` is called.

The X11 `run()` wraps `poll_event` in a multi-window loop that also drains `pending_window_creates` for popup menus and dialogs. Idle is `wait_for_x11_connection_activity` which uses `XConnectionNumber` + `select(2)` with a 16 ms timeout so timers can still fire even if no native events arrive.

`request_redraw` sends per-rect `Expose` events when `gpu_damage_rects` is populated; otherwise sends a single full-window expose. `XSendEvent` + `XFlush` to wake the loop.

## XRandR breakpoint detection

`try_subscribe_xrandr` loads `libXrandr.so.2`, calls `XRRQueryExtension` to find the event base, and `XRRSelectInput(root, RR_SCREEN_CHANGE_NOTIFY_MASK)` to subscribe. The Xrandr library is intentionally `mem::forget`-leaked — the function pointers must outlive the window.

The X11 backend also tracks responsive breakpoints. On every `ConfigureNotify` resize, `dynamic_selector_context` is updated and compared with `viewport_breakpoint_changed` against the constant list:

```rust,ignore
const CSS_BREAKPOINTS: &[f32] = &[320.0, 480.0, 640.0, 768.0, 1024.0,
                                   1280.0, 1440.0, 1920.0];
```

Crossing one logs the breakpoint change for the layout subsystem; the relayout itself fires unconditionally on size change.

## IME via XIM (with GTK fallback)

`ImeManager` opens an XIM via `XOpenIM`, then creates an XIC with `XIMPreeditNothing | XIMStatusNothing`. `XSetLocaleModifiers("")` is called first — XIM only works when the locale is initialised.

`filter_event` (called from `poll_event` before dispatch) calls `XFilterEvent`. When XIM consumes the event, the loop `continue`s without further handling. Otherwise `XLookupString` produces UTF-8 that flows through the standard `process_text_input` path.

If `XOpenIM` returns null (no IM server running), the manager falls back to GTK 3's `GtkIMContext` loaded via `Gtk3Im::new`. Both code paths feed the same `process_text_input(text)` on `LayoutWindow`.

## Tooltips

`TooltipWindow` is a transient override-redirect window:

```rust,ignore
let attrs = XSetWindowAttributes { override_redirect: True, ... };
XCreateWindow(display, root, 0, 0, w, h, 0, depth, InputOutput,
              visual, CWOverrideRedirect | CWBackPixel, &mut attrs);
```

It exists for the lifetime of the parent window and is shown/hidden via `XMapWindow` / `XUnmapWindow`. Text is drawn with `XDrawString` on a fixed-width font (`-misc-fixed-medium-r-normal--13-...`). The tooltip moves with `XMoveWindow` on cursor motion.

This is the legacy path used until per-DOM tooltip rendering is fully wired through `LayoutWindow.tooltip_manager`. The constants (`TOOLTIP_BG_COLOR`, `TOOLTIP_CHAR_WIDTH_PX`, etc.) hard-code an old style; styled tooltips will route through the standard render pipeline like context menus do.

## Native menus

X11 has two menu paths:

1. **GNOME global menu** via DBus `org.gtk.Menus` / `org.gtk.Actions` — see [Linux DBus](linux-dbus.md). Activated via `gnome_menu::should_use_gnome_menus()` which checks `XDG_CURRENT_DESKTOP`.
2. **Popup menu window** — `create_menu_window_options` returns a `WindowCreateOptions` with `WindowDecorations::None`, `is_always_on_top`, and a `LayoutCallback` that renders the menu through the standard `menu_renderer::create_menu_dom_with_css` path. The popup is then created by the main event loop via `pending_window_creates`.

## Clipboard

The X11 clipboard module wraps the `x11-clipboard` crate. On every event-loop iteration `sync_clipboard` is called — it writes any pending content from `ClipboardManager` to *both* the `CLIPBOARD` and `PRIMARY` selections (Ctrl+C/V and middle-click respectively). Reads use a 3 second timeout per selection so worst-case wait is 6 s when both selections are stale.

## D-Bus integration

The X11 window holds an optional `*mut DBusConnection` used for two things:

- **Screen-saver inhibit** — `org.freedesktop.ScreenSaver.Inhibit` is called when the user enables the `WindowFlags::keep_screen_awake` flag. The cookie is stored in `screensaver_inhibit_cookie` so the pair `Inhibit` / `UnInhibit` can be matched.
- **GNOME menu protocol** — the same connection is used by `gnome_menu::GnomeMenuManager` to register `org.gtk.Menus` / `org.gtk.Actions` object paths.

The libdbus-1 SO is loaded via `dbus::DBusLib::new` which uses the same `Library` / `load_first_available` machinery as Xlib.

## Timers (timerfd)

`linux::timer::start_timerfd` wraps `timerfd_create(CLOCK_MONOTONIC, TFD_NONBLOCK | TFD_CLOEXEC)` and `timerfd_settime`. Each Azul timer registered on the window allocates one timerfd; when the fd becomes readable the timer has fired. `check_timers_and_threads` in `poll_event` calls `read(fd, &mut buf, 8)` to consume and dispatch.

Timer fds are mixed into the `select(2)` set in `wait_for_x11_connection_activity` so the idle wait wakes both for X events and timer fires.

## Multi-window support

The global registry lives in `linux/registry.rs` (thread-local `HashMap<X11WindowId, *mut X11Window>`). `register_x11_window` inserts; `get_x11_window` looks up; `get_all_x11_window_ids` gives the loop iteration order. Pointers are leaked via `Box::into_raw` in the run loop; `unregister_x11_window` returns the pointer for `Box::from_raw` cleanup when the window is closed.

The same registry is used by both X11 and Wayland — Wayland uses `wl_display as u64` as the registry key (see [Linux Wayland](linux-wayland.md)).

## Accessibility

The X11 accessibility module uses `accesskit_unix::Adapter` (AT-SPI 2 over DBus) when the `a11y` cargo feature is on. `Adapter::new` is wrapped in `catch_unwind` because DBus connection failures can panic from inside `accesskit`. Action requests are queued in `pending_actions: Arc<Mutex<Vec<ActionRequest>>>` and drained from the event loop.

## Defines and visual-info layout

The X11 defines module is the literal Rust translation of the Xlib C headers: `Display`, `Window`, `Atom`, `XEvent` union, `XErrorEvent`, `XSetWindowAttributes`, `XVisualInfo`, `XClientMessageEvent`, event-type constants (`Expose = 12`, `KeyPress = 2`, etc.), and event-mask flags. The file is mechanical — fields and constants must match the C ABI exactly. When adding event-mask bits, cross-check against `/usr/include/X11/X.h`.

## Linux-specific helpers

- **`linux/system_style.rs`.** Reads GTK/Adwaita system colors via gsettings and populates `azul_css::system::SystemStyle`.
- **`linux/resources.rs`.** Holds `AppResources`, the shared font cache, app data, system style, and icon provider Arc-shared between all windows on Linux.
- **`linux/common/gl.rs`.** Holds `GlFunctions::initialize`, which fills `GenericGlContext` via `eglGetProcAddress` with a `dlsym` fallback over libGL.so.1.
- **`linux/timer.rs`.** timerfd helpers reused by both X11 and Wayland.
- **`linux/registry.rs`.** Thread-local window registry shared by both Linux backends.

## Known issues / TODOs

- `check_gpu_blacklist` is defined but never called from the X11 GPU init path. When wired, an llvmpipe / NVIDIA-with-no-GLSL / old-Intel detection should fall through to `RenderMode::Cpu`.
- Tooltip rendering is still bitmap text via `XDrawString`; styled tooltips will rebuild on top of the same `pending_window_creates` queue used by menu popups.
- The X11 popup menu path does not yet support nested submenus from `Menu::items[i].submenu` — each level needs its own popup window parented to the previous.

## Coming Up Next

- [Linux Wayland](linux-wayland.md) — Linux Wayland shell - wl_surface, xdg-shell, libinput
- [Linux DBus](linux-dbus.md) — Linux DBus integration for a11y, dialogs, notifications
- [Common](common.md) — Shared shell infrastructure across platforms
- [Windowing Overview](../windowing.md) — Per-window aggregate, headless variant, and the platform shell layer
